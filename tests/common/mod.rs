//! Shared test helpers.
//!
//! Provides a SQLite-backed Iceberg catalog for integration tests that don't
//! need a remote service.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::Catalog;
use iceberg::io::LocalFsStorageFactory;
use iceberg_catalog_sql::{SQL_CATALOG_PROP_BIND_STYLE, SqlBindStyle, SqlCatalogBuilder};
use iceberg::CatalogBuilder;
use sqlx::migrate::MigrateDatabase;
use tempfile::TempDir;

/// A self-contained SQLite catalog with temporary directories that are cleaned
/// up on drop.
pub struct TestCatalog {
    #[allow(dead_code)]
    pub catalog: Arc<dyn Catalog>,
    pub warehouse_dir: TempDir,
    _db_dir: TempDir,
}

impl TestCatalog {
    /// Create a fresh SQLite-backed catalog with empty state.
    pub async fn new() -> Self {
        let warehouse_dir = TempDir::new().expect("create warehouse tempdir");
        let db_dir = TempDir::new().expect("create db tempdir");
        let db_path = db_dir.path().join("catalog.db");
        let db_uri = format!("sqlite:{}", db_path.to_str().unwrap());

        sqlx::Sqlite::create_database(&db_uri)
            .await
            .expect("create sqlite database");

        let catalog = SqlCatalogBuilder::default()
            .with_storage_factory(Arc::new(LocalFsStorageFactory))
            .load(
                "test",
                HashMap::from([
                    ("uri".to_string(), db_uri),
                    (
                        "warehouse".to_string(),
                        warehouse_dir.path().to_str().unwrap().to_string(),
                    ),
                    (
                        SQL_CATALOG_PROP_BIND_STYLE.to_string(),
                        SqlBindStyle::QMark.to_string(),
                    ),
                ]),
            )
            .await
            .expect("create sql catalog");

        Self {
            catalog: Arc::new(catalog),
            warehouse_dir,
            _db_dir: db_dir,
        }
    }

    /// Write a `.pyiceberg.yaml` pointing at this catalog and return its path.
    pub fn write_pyiceberg_config(&self, dir: &TempDir) -> std::path::PathBuf {
        let db_dir = &self._db_dir;
        let db_uri = format!(
            "sqlite:{}",
            db_dir.path().join("catalog.db").to_str().unwrap()
        );
        let warehouse = self.warehouse_dir.path().to_str().unwrap();
        let config = format!(
            "default-catalog: default\n\
             \n\
             catalog:\n\
             \x20 default:\n\
             \x20   type: sql\n\
             \x20   uri: {db_uri}\n\
             \x20   warehouse: {warehouse}\n\
             \x20   sql_bind_style: QMark\n"
        );
        let path = dir.path().join(".pyiceberg.yaml");
        std::fs::write(&path, config).expect("write test config");
        path
    }
}
