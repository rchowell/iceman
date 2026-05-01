use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};
use aws_config::BehaviorVersion;
use aws_credential_types::provider::ProvideCredentials;
use iceberg::{Catalog, CatalogBuilder};
use iceberg_catalog_glue::GlueCatalogBuilder;
use iceberg_catalog_hms::HmsCatalogBuilder;
use iceberg_catalog_rest::RestCatalogBuilder;
use iceberg_catalog_s3tables::S3TablesCatalogBuilder;
use iceberg_catalog_sql::SqlCatalogBuilder;
use iceberg_storage_opendal::OpenDalStorageFactory;
use iceberg_storage_opendal::{AwsCredential, AwsCredentialLoad, CustomAwsCredentialLoader};
use serde::{Deserialize, Serialize};

use crate::cli::IcemanCli;
use crate::error::ConfigError;

#[derive(Debug, Clone)]
pub struct CatalogConfig {
    pub kind: String,
    pub name: String,
    pub props: HashMap<String, String>,
}

impl CatalogConfig {
    pub fn apply_overrides(
        &mut self,
        uri: Option<&str>,
        warehouse: Option<&str>,
        credential: Option<&str>,
    ) {
        for (key, value) in [
            ("uri", uri),
            ("warehouse", warehouse),
            ("credential", credential),
        ] {
            if let Some(v) = value {
                self.props.insert(key.to_string(), v.to_string());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IcemanConfig {
    #[serde(default, rename = "default-catalog")]
    pub default_catalog: Option<String>,

    #[serde(default)]
    pub catalog: HashMap<String, IcemanCatalogConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcemanCatalogConfig {
    #[serde(default, rename = "type")]
    pub catalog_type: Option<String>,

    #[serde(default)]
    pub uri: Option<String>,

    #[serde(default)]
    pub warehouse: Option<String>,

    #[serde(flatten)]
    pub properties: HashMap<String, String>,
}

impl IcemanConfig {
    pub fn resolve_catalog(&self, name: Option<&str>) -> Result<CatalogConfig, ConfigError> {
        let catalog_name = name
            .or(self.default_catalog.as_deref())
            .unwrap_or("default");

        let entry = self
            .catalog
            .get(catalog_name)
            .ok_or_else(|| ConfigError::CatalogNotFound {
                name: catalog_name.to_string(),
            })?;

        let mut props = entry.properties.clone();
        if let Some(ref uri) = entry.uri {
            props.insert("uri".to_string(), uri.clone());
        }
        if let Some(ref warehouse) = entry.warehouse {
            props.insert("warehouse".to_string(), warehouse.clone());
        }

        Ok(CatalogConfig {
            name: catalog_name.to_string(),
            kind: entry.catalog_type.clone().unwrap_or_default(),
            props,
        })
    }
}

pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let home = dirs::home_dir().ok_or(ConfigError::NoConfigDir)?;
    Ok(home.join(".config").join("iceman").join("config.toml"))
}

pub fn load_config(path: Option<&Path>) -> Result<IcemanConfig, ConfigError> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    if !config_path.exists() {
        return Ok(IcemanConfig::default());
    }

    let contents = std::fs::read_to_string(&config_path).map_err(|e| ConfigError::Read {
        path: config_path.clone(),
        source: e,
    })?;

    toml::from_str(&contents).map_err(|e| ConfigError::TomlParse {
        path: config_path,
        source: e,
    })
}

pub async fn resolve_catalog(cli: &IcemanCli) -> Result<Arc<dyn Catalog>> {
    let cfg = load_config(cli.config.as_deref())?;
    let mut catalog_ref = cfg.resolve_catalog(cli.catalog.as_deref())?;
    catalog_ref.apply_overrides(
        cli.uri.as_deref(),
        cli.warehouse.as_deref(),
        cli.credential.as_deref(),
    );
    load_catalog(&catalog_ref).await
}

pub async fn load_catalog(catalog: &CatalogConfig) -> Result<Arc<dyn Catalog>> {
    let name = catalog.name.clone();
    let props = catalog.props.clone();

    let kind: &str = if catalog.kind.is_empty() {
        infer_catalog_type(&props)?
    } else {
        &catalog.kind
    };

    match kind {
        "rest" => load_rest(name, props).await,
        "s3tables" => load_s3tables(name, props).await,
        "glue" => load_glue(name, props).await,
        "hive" => load_hms(name, props).await,
        "sql" => load_sql(name, props).await,
        other => bail!(
            "unsupported catalog type: '{other}' (supported: rest, s3tables, glue, hive, sql)"
        ),
    }
}

pub fn infer_catalog_type<S: std::hash::BuildHasher>(
    props: &HashMap<String, String, S>,
) -> Result<&'static str> {
    if let Some(uri) = props.get("uri") {
        if uri.starts_with("http") {
            return Ok("rest");
        }
        if uri.starts_with("thrift") {
            return Ok("hive");
        }
        if uri.starts_with("sqlite") || uri.starts_with("postgresql") {
            return Ok("sql");
        }
        bail!("could not infer catalog type from URI: {uri}");
    }
    bail!(
        "no catalog type specified and no URI to infer from; \
         set 'type' in your catalog configuration or provide --uri"
    );
}

async fn load_rest(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    Ok(Arc::new(
        RestCatalogBuilder::default().load(name, props).await?,
    ))
}

async fn load_s3tables(
    name: String,
    mut props: HashMap<String, String>,
) -> Result<Arc<dyn Catalog>> {
    if let Some(wh) = props.remove("warehouse") {
        props.entry("table_bucket_arn".to_string()).or_insert(wh);
    }

    let sdk_config = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let cred_provider = sdk_config
        .credentials_provider()
        .expect("no AWS credential provider found")
        .clone();
    let loader = CustomAwsCredentialLoader::new(Arc::new(SdkCredentialBridge(cred_provider)));
    let factory = Arc::new(OpenDalStorageFactory::S3 {
        customized_credential_load: Some(loader),
    });

    Ok(Arc::new(
        S3TablesCatalogBuilder::default()
            .with_storage_factory(factory)
            .load(name, props)
            .await?,
    ))
}

struct SdkCredentialBridge(aws_credential_types::provider::SharedCredentialsProvider);

#[async_trait::async_trait]
impl AwsCredentialLoad for SdkCredentialBridge {
    async fn load_credential(
        &self,
        _client: reqwest::Client,
    ) -> anyhow::Result<Option<AwsCredential>> {
        let creds = self.0.provide_credentials().await?;
        Ok(Some(AwsCredential {
            access_key_id: creds.access_key_id().to_string(),
            secret_access_key: creds.secret_access_key().to_string(),
            session_token: creds.session_token().map(ToString::to_string),
            expires_in: creds.expiry().map(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                chrono::DateTime::from_timestamp(
                    duration.as_secs().cast_signed(),
                    duration.subsec_nanos(),
                )
                .unwrap_or_default()
            }),
        }))
    }
}

async fn load_glue(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    Ok(Arc::new(
        GlueCatalogBuilder::default().load(name, props).await?,
    ))
}

async fn load_hms(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    Ok(Arc::new(
        HmsCatalogBuilder::default().load(name, props).await?,
    ))
}

async fn load_sql(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    Ok(Arc::new(
        SqlCatalogBuilder::default()
            .with_storage_factory(Arc::new(iceberg::io::LocalFsStorageFactory))
            .load(name, props)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> CatalogConfig {
        CatalogConfig {
            kind: String::new(),
            name: "test".to_string(),
            props: HashMap::new(),
        }
    }

    #[test]
    fn test_apply_overrides_all() {
        let mut cfg = empty_config();
        cfg.apply_overrides(Some("http://example.com"), Some("wh"), Some("cred"));
        assert_eq!(cfg.props["uri"], "http://example.com");
        assert_eq!(cfg.props["warehouse"], "wh");
        assert_eq!(cfg.props["credential"], "cred");
    }

    #[test]
    fn test_apply_overrides_none() {
        let mut cfg = empty_config();
        cfg.props.insert("uri".to_string(), "original".to_string());
        cfg.apply_overrides(None, None, None);
        assert_eq!(cfg.props["uri"], "original");
    }

    #[test]
    fn test_apply_overrides_partial() {
        let mut cfg = empty_config();
        cfg.props
            .insert("uri".to_string(), "http://original.com".to_string());
        cfg.apply_overrides(None, Some("new_wh"), None);
        assert_eq!(cfg.props["uri"], "http://original.com");
        assert_eq!(cfg.props["warehouse"], "new_wh");
    }

    #[test]
    fn test_infer_rest_from_http() {
        let mut props = HashMap::new();
        props.insert("uri".to_string(), "http://localhost:8181".to_string());
        assert_eq!(infer_catalog_type(&props).unwrap(), "rest");
    }

    #[test]
    fn test_infer_rest_from_https() {
        let mut props = HashMap::new();
        props.insert("uri".to_string(), "https://catalog.example.com".to_string());
        assert_eq!(infer_catalog_type(&props).unwrap(), "rest");
    }

    #[test]
    fn test_infer_hive_from_thrift() {
        let mut props = HashMap::new();
        props.insert("uri".to_string(), "thrift://localhost:9083".to_string());
        assert_eq!(infer_catalog_type(&props).unwrap(), "hive");
    }

    #[test]
    fn test_infer_sql_from_sqlite() {
        let mut props = HashMap::new();
        props.insert("uri".to_string(), "sqlite:///tmp/test.db".to_string());
        assert_eq!(infer_catalog_type(&props).unwrap(), "sql");
    }

    #[test]
    fn test_infer_sql_from_postgresql() {
        let mut props = HashMap::new();
        props.insert(
            "uri".to_string(),
            "postgresql://localhost/iceberg".to_string(),
        );
        assert_eq!(infer_catalog_type(&props).unwrap(), "sql");
    }

    #[test]
    fn test_infer_fails_unknown_scheme() {
        let mut props = HashMap::new();
        props.insert("uri".to_string(), "ftp://localhost:21".to_string());
        let err = infer_catalog_type(&props).unwrap_err();
        assert!(err.to_string().contains("could not infer catalog type"));
    }

    #[test]
    fn test_infer_fails_no_uri() {
        let props = HashMap::new();
        let err = infer_catalog_type(&props).unwrap_err();
        assert!(err.to_string().contains("no catalog type specified"));
    }
}
