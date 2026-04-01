use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

use super::CatalogRef;

const PYICEBERG_YAML: &str = ".pyiceberg.yaml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PyIcebergConfig {
    #[serde(default, rename = "default-catalog", skip_serializing_if = "Option::is_none")]
    pub default_catalog: Option<String>,

    #[serde(default)]
    pub catalog: HashMap<String, PyIcebergCatalogConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PyIcebergCatalogConfig {
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub catalog_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warehouse: Option<String>,

    #[serde(flatten)]
    pub properties: HashMap<String, serde_yaml::Value>,
}

impl PyIcebergConfig {
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

        let mut props: HashMap<String, String> = entry
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), yaml_value_to_string(v)))
            .collect();

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

fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other).unwrap_or_default(),
    }
}

/// Search for .pyiceberg.yaml in pyiceberg's standard locations.
/// Order: $PYICEBERG_HOME/, ~/, ./ — first found wins.
pub fn find_config_file() -> Option<PathBuf> {
    let candidates = [
        std::env::var("PYICEBERG_HOME").ok().map(PathBuf::from),
        dirs::home_dir(),
        std::env::current_dir().ok(),
    ];

    candidates
        .into_iter()
        .flatten()
        .map(|dir| dir.join(PYICEBERG_YAML))
        .find(|p| p.exists())
}

pub fn load_config(path: Option<&Path>) -> Result<PyIcebergConfig, ConfigError> {
    let config_path = match path {
        Some(p) => {
            if !p.exists() {
                return Err(ConfigError::NotFound {
                    path: p.to_path_buf(),
                });
            }
            p.to_path_buf()
        }
        None => match find_config_file() {
            Some(p) => p,
            None => return Ok(PyIcebergConfig::default()),
        },
    };

    let contents = std::fs::read_to_string(&config_path).map_err(|e| ConfigError::Read {
        path: config_path.clone(),
        source: e,
    })?;

    let mut config: PyIcebergConfig =
        serde_yaml::from_str(&contents).map_err(|e| ConfigError::YamlParse {
            path: config_path,
            source: e,
        })?;

    // Overlay PYICEBERG_ environment variables
    let env_config = parse_pyiceberg_env_vars();
    merge_config(&mut config, env_config);

    Ok(config)
}

pub fn init_default_config() -> Result<PathBuf, ConfigError> {
    let home = dirs::home_dir().ok_or(ConfigError::NoConfigDir)?;
    let path = home.join(PYICEBERG_YAML);
    if !path.exists() {
        let default_content = r#"# PyIceberg configuration
# See: https://py.iceberg.apache.org/configuration/

# default-catalog: my_catalog

# catalog:
#   my_catalog:
#     type: rest
#     uri: http://localhost:8181
#     warehouse: my_warehouse
#     s3.endpoint: http://localhost:9000
"#;
        std::fs::write(&path, default_content).map_err(|e| ConfigError::Write {
            path: path.clone(),
            source: e,
        })?;
    }
    Ok(path)
}

// -- env var parsing --

/// Parse PYICEBERG_ environment variables into a PyIcebergConfig.
///
/// Env var mapping: strip `PYICEBERG_` prefix, split on `__` (maxsplit=2),
/// then in each segment replace `__` with `.` and `_` with `-`, lowercase.
fn parse_pyiceberg_env_vars() -> PyIcebergConfig {
    let mut config = PyIcebergConfig::default();

    for (key, value) in std::env::vars() {
        let Some(remainder) = key.strip_prefix("PYICEBERG_") else {
            continue;
        };

        let segments = split_max(remainder, "__", 2);
        let transformed: Vec<String> = segments
            .iter()
            .map(|s| s.replace("__", ".").replace('_', "-").to_lowercase())
            .collect();

        match transformed.as_slice() {
            [single] => {
                if single == "default-catalog" {
                    config.default_catalog = Some(value);
                }
            }
            [prefix, catalog_name, property_key] if prefix == "catalog" => {
                let entry = config
                    .catalog
                    .entry(catalog_name.clone())
                    .or_default();

                match property_key.as_str() {
                    "type" => entry.catalog_type = Some(value),
                    "uri" => entry.uri = Some(value),
                    "warehouse" => entry.warehouse = Some(value),
                    _ => {
                        entry
                            .properties
                            .insert(property_key.clone(), serde_yaml::Value::String(value));
                    }
                }
            }
            _ => {} // ignore malformed
        }
    }

    config
}

/// Split `s` on `delim` at most `max_splits` times.
fn split_max<'a>(s: &'a str, delim: &str, max_splits: usize) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut remaining = s;
    let mut splits = 0;

    while splits < max_splits {
        if let Some(pos) = remaining.find(delim) {
            parts.push(&remaining[..pos]);
            remaining = &remaining[pos + delim.len()..];
            splits += 1;
        } else {
            break;
        }
    }
    parts.push(remaining);
    parts
}

/// Merge overlay config into base. Overlay values win when present.
fn merge_config(base: &mut PyIcebergConfig, overlay: PyIcebergConfig) {
    if overlay.default_catalog.is_some() {
        base.default_catalog = overlay.default_catalog;
    }

    for (name, overlay_entry) in overlay.catalog {
        let base_entry = base.catalog.entry(name).or_default();

        if overlay_entry.catalog_type.is_some() {
            base_entry.catalog_type = overlay_entry.catalog_type;
        }
        if overlay_entry.uri.is_some() {
            base_entry.uri = overlay_entry.uri;
        }
        if overlay_entry.warehouse.is_some() {
            base_entry.warehouse = overlay_entry.warehouse;
        }
        for (k, v) in overlay_entry.properties {
            base_entry.properties.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_max() {
        assert_eq!(split_max("A__B__C__D", "__", 2), vec!["A", "B", "C__D"]);
        assert_eq!(split_max("A__B", "__", 2), vec!["A", "B"]);
        assert_eq!(split_max("ABCD", "__", 2), vec!["ABCD"]);
        assert_eq!(split_max("A__B__C", "__", 1), vec!["A", "B__C"]);
    }

    #[test]
    fn test_env_var_transform() {
        // Simulates PYICEBERG_CATALOG__DEFAULT__S3__ACCESS_KEY_ID
        let segments = split_max("CATALOG__DEFAULT__S3__ACCESS_KEY_ID", "__", 2);
        let transformed: Vec<String> = segments
            .iter()
            .map(|s| s.replace("__", ".").replace('_', "-").to_lowercase())
            .collect();
        assert_eq!(transformed, vec!["catalog", "default", "s3.access-key-id"]);
    }

    #[test]
    fn test_env_var_default_catalog() {
        let segments = split_max("DEFAULT_CATALOG", "__", 2);
        let transformed: Vec<String> = segments
            .iter()
            .map(|s| s.replace("__", ".").replace('_', "-").to_lowercase())
            .collect();
        assert_eq!(transformed, vec!["default-catalog"]);
    }
}
