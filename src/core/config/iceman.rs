use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

use super::CatalogRef;

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
    pub fn resolve_catalog(&self, name: Option<&str>) -> Result<CatalogRef, ConfigError> {
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

        Ok(CatalogRef {
            name: catalog_name.to_string(),
            catalog_type: entry.catalog_type.clone().unwrap_or_default(),
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

    let config: IcemanConfig =
        toml::from_str(&contents).map_err(|e| ConfigError::TomlParse {
            path: config_path,
            source: e,
        })?;

    Ok(config)
}

fn ensure_config_dir(path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

pub fn init_default_config() -> Result<PathBuf, ConfigError> {
    let path = default_config_path()?;
    if !path.exists() {
        ensure_config_dir(&path)?;
        let default_content = r#"# Iceman configuration
# See: https://github.com/rch/iceman

# default-catalog = "my_catalog"

# [catalog.my_catalog]
# type = "rest"
# uri = "http://localhost:8181"
# warehouse = "my_warehouse"
# s3.endpoint = "http://localhost:9000"
"#;
        std::fs::write(&path, default_content).map_err(|e| ConfigError::Write {
            path: path.clone(),
            source: e,
        })?;
    }
    Ok(path)
}
