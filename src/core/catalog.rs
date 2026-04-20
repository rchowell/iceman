use std::collections::HashMap;
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
use iceberg_storage_opendal::{AwsCredential, AwsCredentialLoad, CustomAwsCredentialLoader};
use iceberg_storage_opendal::OpenDalStorageFactory;

use super::config::CatalogConfig;

/// Loads a catalog implementation from a [CatalogConfig].
///
/// When `kind` is empty, attempts to infer the catalog type from the URI
/// (matching PyIceberg's `infer_catalog_type` behavior).
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

/// Infer catalog type from the URI property, matching PyIceberg behavior.
pub fn infer_catalog_type(props: &HashMap<String, String>) -> Result<&'static str> {
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

// ── catalog loaders ─────────────────────────────────────────────────

async fn load_rest(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    let catalog = RestCatalogBuilder::default().load(name, props).await?;
    Ok(Arc::new(catalog))
}

async fn load_s3tables(
    name: String,
    mut props: HashMap<String, String>,
) -> Result<Arc<dyn Catalog>> {
    if let Some(wh) = props.remove("warehouse") {
        props.entry("table_bucket_arn".to_string()).or_insert(wh);
    }

    // Bridge AWS SDK credentials (which support SSO, IMDS, etc.) into
    // opendal's credential loader so that reading table metadata from S3
    // uses the same credential chain as the S3Tables API calls.
    let sdk_config = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let cred_provider = sdk_config
        .credentials_provider()
        .expect("no AWS credential provider found")
        .clone();
    let loader = CustomAwsCredentialLoader::new(Arc::new(SdkCredentialBridge(cred_provider)));
    let factory = Arc::new(OpenDalStorageFactory::S3 {
        configured_scheme: "s3".to_string(),
        customized_credential_load: Some(loader),
    });

    let catalog = S3TablesCatalogBuilder::default()
        .with_storage_factory(factory)
        .load(name, props)
        .await?;
    Ok(Arc::new(catalog))
}

/// Bridges AWS SDK credential provider into reqsign's `AwsCredentialLoad`
/// so opendal can use SSO / IMDS / profile credentials.
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
            session_token: creds.session_token().map(|s| s.to_string()),
            expires_in: creds.expiry().map(|t| {
                let duration = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                chrono::DateTime::from_timestamp(
                    duration.as_secs() as i64,
                    duration.subsec_nanos(),
                )
                .unwrap_or_default()
            }),
        }))
    }
}

async fn load_glue(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    let catalog = GlueCatalogBuilder::default().load(name, props).await?;
    Ok(Arc::new(catalog))
}

async fn load_hms(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    let catalog = HmsCatalogBuilder::default().load(name, props).await?;
    Ok(Arc::new(catalog))
}

async fn load_sql(name: String, props: HashMap<String, String>) -> Result<Arc<dyn Catalog>> {
    let catalog = SqlCatalogBuilder::default()
        .with_storage_factory(Arc::new(iceberg::io::LocalFsStorageFactory))
        .load(name, props)
        .await?;
    Ok(Arc::new(catalog))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            err.to_string().contains("could not infer catalog type"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_infer_fails_no_uri() {
        let props = HashMap::new();
        let err = infer_catalog_type(&props).unwrap_err();
        assert!(
            err.to_string().contains("no catalog type specified"),
            "unexpected error: {err}"
        );
    }
}
