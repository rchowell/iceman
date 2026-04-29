pub mod iceman;

use std::collections::HashMap;

/// Basic catalog configuration matching.
#[derive(Debug, Clone)]
pub struct CatalogConfig {
    /// The catalog implementation to use e.g. "rest" or "".
    pub kind: String,
    /// The catalog name
    pub name: String,
    /// Arbitrary configuration properties
    pub props: HashMap<String, String>,
}

impl CatalogConfig {
    /// Applies CLI flag overrides (--uri, --warehouse, --credential) onto the resolved props.
    pub fn apply_overrides(
        &mut self,
        uri: Option<&str>,
        warehouse: Option<&str>,
        credential: Option<&str>,
        ugi: Option<&str>,
    ) {
        if let Some(v) = uri {
            self.props.insert("uri".to_string(), v.to_string());
        }
        if let Some(v) = warehouse {
            self.props.insert("warehouse".to_string(), v.to_string());
        }
        if let Some(v) = credential {
            self.props.insert("credential".to_string(), v.to_string());
        }
        if let Some(v) = ugi {
            self.props.insert("ugi".to_string(), v.to_string());
        }
    }
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
        cfg.apply_overrides(
            Some("http://example.com"),
            Some("wh"),
            Some("cred"),
            Some("user"),
        );
        assert_eq!(cfg.props.get("uri").unwrap(), "http://example.com");
        assert_eq!(cfg.props.get("warehouse").unwrap(), "wh");
        assert_eq!(cfg.props.get("credential").unwrap(), "cred");
        assert_eq!(cfg.props.get("ugi").unwrap(), "user");
    }

    #[test]
    fn test_apply_overrides_none() {
        let mut cfg = empty_config();
        cfg.props.insert("uri".to_string(), "original".to_string());
        cfg.apply_overrides(None, None, None, None);
        assert_eq!(cfg.props.get("uri").unwrap(), "original");
    }

    #[test]
    fn test_apply_overrides_partial() {
        let mut cfg = empty_config();
        cfg.props
            .insert("uri".to_string(), "http://original.com".to_string());
        cfg.apply_overrides(None, Some("new_wh"), None, None);
        assert_eq!(cfg.props.get("uri").unwrap(), "http://original.com");
        assert_eq!(cfg.props.get("warehouse").unwrap(), "new_wh");
    }

    #[test]
    fn test_apply_overrides_replaces_existing() {
        let mut cfg = empty_config();
        cfg.props
            .insert("uri".to_string(), "http://old.com".to_string());
        cfg.apply_overrides(Some("http://new.com"), None, None, None);
        assert_eq!(cfg.props.get("uri").unwrap(), "http://new.com");
    }
}
