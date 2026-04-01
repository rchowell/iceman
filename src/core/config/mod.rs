pub mod iceman;
pub mod pyiceberg;

use std::collections::HashMap;

/// Shared catalog reference that both config formats produce.
/// Maps directly to iceberg-rust's `CatalogBuilder::load(name, props)`.
#[derive(Debug, Clone)]
pub struct CatalogRef {
    pub name: String,
    pub catalog_type: String,
    pub props: HashMap<String, String>,
}
