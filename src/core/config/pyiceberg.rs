use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

use super::CatalogConfig;

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
    pub fn resolve_catalog(&self, name: Option<&str>) -> Result<CatalogConfig, ConfigError> {
        let catalog_name = name
            .or(self.default_catalog.as_deref())
            .unwrap_or("default");

        // Case-insensitive catalog lookup (matches PyIceberg behavior)
        let catalog_name_lower = catalog_name.to_lowercase();
        let entry = self
            .catalog
            .iter()
            .find(|(k, _)| k.to_lowercase() == catalog_name_lower)
            .map(|(_, v)| v);

        let mut props = HashMap::new();

        if let Some(entry) = entry {
            // Flatten nested YAML properties into dot-separated keys
            for (k, v) in &entry.properties {
                flatten_yaml_value(&k.to_lowercase(), v, &mut props);
            }

            if let Some(ref uri) = entry.uri {
                props.insert("uri".to_string(), uri.clone());
            }
            if let Some(ref warehouse) = entry.warehouse {
                props.insert("warehouse".to_string(), warehouse.clone());
            }

            Ok(CatalogConfig {
                name: catalog_name.to_string(),
                kind: entry.catalog_type.clone().unwrap_or_default().to_lowercase(),
                props,
            })
        } else {
            // Allow catalog-less operation: CLI overrides / env vars may supply enough
            Ok(CatalogConfig {
                name: catalog_name.to_string(),
                kind: String::new(),
                props,
            })
        }
    }
}

/// Recursively flatten a YAML value into dot-separated keys.
///
/// Nested mappings like `s3: { endpoint: "http://..." }` become
/// `s3.endpoint = "http://..."`, matching PyIceberg's behavior.
fn flatten_yaml_value(prefix: &str, value: &serde_yaml::Value, out: &mut HashMap<String, String>) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (k, v) in map {
                let key_str = yaml_scalar_to_string(k).to_lowercase();
                let full_key = if prefix.is_empty() {
                    key_str
                } else {
                    format!("{prefix}.{key_str}")
                };
                flatten_yaml_value(&full_key, v, out);
            }
        }
        other => {
            out.insert(prefix.to_string(), yaml_scalar_to_string(other));
        }
    }
}

fn yaml_scalar_to_string(v: &serde_yaml::Value) -> String {
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

    // ── flatten nested YAML ─────────────────────────────────────────

    #[test]
    fn test_flatten_yaml_nested_mapping() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            r#"
            endpoint: http://localhost:9000
            access-key-id: mykey
            "#,
        )
        .unwrap();

        let mut out = HashMap::new();
        flatten_yaml_value("s3", &yaml, &mut out);
        assert_eq!(out.get("s3.endpoint").unwrap(), "http://localhost:9000");
        assert_eq!(out.get("s3.access-key-id").unwrap(), "mykey");
    }

    #[test]
    fn test_flatten_yaml_deeply_nested() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            r#"
            a:
              b:
                c: deep
            "#,
        )
        .unwrap();

        let mut out = HashMap::new();
        flatten_yaml_value("", &yaml, &mut out);
        assert_eq!(out.get("a.b.c").unwrap(), "deep");
    }

    #[test]
    fn test_flatten_yaml_scalar_unchanged() {
        let yaml = serde_yaml::Value::String("hello".into());
        let mut out = HashMap::new();
        flatten_yaml_value("key", &yaml, &mut out);
        assert_eq!(out.get("key").unwrap(), "hello");
    }

    #[test]
    fn test_flatten_yaml_lowercases_keys() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            r#"
            Endpoint: http://localhost:9000
            "#,
        )
        .unwrap();

        let mut out = HashMap::new();
        flatten_yaml_value("S3", &yaml, &mut out);
        // prefix is passed already-lowercased by resolve_catalog,
        // but nested keys should also be lowercased
        assert_eq!(out.get("S3.endpoint").unwrap(), "http://localhost:9000");
    }

    // ── resolve_catalog ─────────────────────────────────────────────

    fn make_config(yaml: &str) -> PyIcebergConfig {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_resolve_catalog_basic() {
        let cfg = make_config(
            r#"
            catalog:
              my_cat:
                type: rest
                uri: http://localhost:8181
                warehouse: wh1
            "#,
        );
        let cat = cfg.resolve_catalog(Some("my_cat")).unwrap();
        assert_eq!(cat.kind, "rest");
        assert_eq!(cat.props.get("uri").unwrap(), "http://localhost:8181");
        assert_eq!(cat.props.get("warehouse").unwrap(), "wh1");
    }

    #[test]
    fn test_resolve_catalog_case_insensitive() {
        let cfg = make_config(
            r#"
            catalog:
              My_Catalog:
                type: rest
                uri: http://localhost:8181
            "#,
        );
        // Lookup with different casing should still find it
        let cat = cfg.resolve_catalog(Some("my_catalog")).unwrap();
        assert_eq!(cat.kind, "rest");

        let cat = cfg.resolve_catalog(Some("MY_CATALOG")).unwrap();
        assert_eq!(cat.kind, "rest");
    }

    #[test]
    fn test_resolve_catalog_missing_returns_empty() {
        let cfg = make_config(
            r#"
            catalog:
              existing:
                type: rest
            "#,
        );
        // Missing catalog should return empty config, not error
        let cat = cfg.resolve_catalog(Some("nonexistent")).unwrap();
        assert_eq!(cat.kind, "");
        assert!(cat.props.is_empty());
        assert_eq!(cat.name, "nonexistent");
    }

    #[test]
    fn test_resolve_catalog_no_config_at_all() {
        let cfg = PyIcebergConfig::default();
        let cat = cfg.resolve_catalog(None).unwrap();
        assert_eq!(cat.name, "default");
        assert_eq!(cat.kind, "");
        assert!(cat.props.is_empty());
    }

    #[test]
    fn test_resolve_catalog_flattens_nested_properties() {
        let cfg = make_config(
            r#"
            catalog:
              my_cat:
                type: rest
                uri: http://localhost:8181
                s3:
                  endpoint: http://localhost:9000
                  access-key-id: mykey
            "#,
        );
        let cat = cfg.resolve_catalog(Some("my_cat")).unwrap();
        assert_eq!(
            cat.props.get("s3.endpoint").unwrap(),
            "http://localhost:9000"
        );
        assert_eq!(cat.props.get("s3.access-key-id").unwrap(), "mykey");
    }

    #[test]
    fn test_resolve_catalog_flat_dotted_keys_preserved() {
        let cfg = make_config(
            r#"
            catalog:
              my_cat:
                type: rest
                uri: http://localhost:8181
                s3.endpoint: http://localhost:9000
            "#,
        );
        let cat = cfg.resolve_catalog(Some("my_cat")).unwrap();
        assert_eq!(
            cat.props.get("s3.endpoint").unwrap(),
            "http://localhost:9000"
        );
    }

    #[test]
    fn test_resolve_catalog_type_lowercased() {
        let cfg = make_config(
            r#"
            catalog:
              my_cat:
                type: REST
                uri: http://localhost:8181
            "#,
        );
        let cat = cfg.resolve_catalog(Some("my_cat")).unwrap();
        assert_eq!(cat.kind, "rest");
    }

    #[test]
    fn test_resolve_catalog_default_fallback_chain() {
        // With default-catalog set
        let cfg = make_config(
            r#"
            default-catalog: secondary
            catalog:
              secondary:
                type: rest
                uri: http://secondary:8181
              default:
                type: rest
                uri: http://default:8181
            "#,
        );
        let cat = cfg.resolve_catalog(None).unwrap();
        assert_eq!(cat.props.get("uri").unwrap(), "http://secondary:8181");

        // Without default-catalog, falls back to "default"
        let cfg = make_config(
            r#"
            catalog:
              default:
                type: rest
                uri: http://default:8181
            "#,
        );
        let cat = cfg.resolve_catalog(None).unwrap();
        assert_eq!(cat.props.get("uri").unwrap(), "http://default:8181");
    }
}
