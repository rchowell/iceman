use iceberg::spec::TableMetadata;
use serde::Serialize;

use crate::render::{Cell, Tabular};

#[derive(Debug, Serialize)]
pub struct MetadataLogRow {
    pub timestamp: i64,
    pub file: String,
}

impl Tabular for MetadataLogRow {
    fn headers() -> &'static [&'static str] {
        &["timestamp", "file"]
    }

    fn row(&self) -> Vec<Cell> {
        vec![Cell::Int(self.timestamp), Cell::Str(self.file.clone())]
    }
}

pub fn extract(metadata: &TableMetadata) -> Vec<MetadataLogRow> {
    metadata
        .metadata_log()
        .iter()
        .map(|log| MetadataLogRow {
            timestamp: log.timestamp_ms,
            file: log.metadata_file.clone(),
        })
        .collect()
}
