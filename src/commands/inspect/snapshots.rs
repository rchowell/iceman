use std::collections::HashMap;

use iceberg::spec::TableMetadata;
use serde::Serialize;

use crate::render::{Cell, Tabular};

#[derive(Debug, Serialize)]
pub struct SnapshotRow {
    pub snapshot_id: i64,
    pub parent_id: Option<i64>,
    pub timestamp_ms: i64,
    pub operation: String,
    pub manifest_list: String,
    pub summary: HashMap<String, String>,
}

impl Tabular for SnapshotRow {
    fn headers() -> &'static [&'static str] {
        &["snapshot_id", "parent_id", "timestamp_ms", "operation", "manifest_list", "summary"]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::Int(self.snapshot_id),
            self.parent_id.map_or(Cell::Null, Cell::Int),
            Cell::Int(self.timestamp_ms),
            Cell::Str(self.operation.clone()),
            Cell::Str(self.manifest_list.clone()),
            Cell::Str(serde_json::to_string(&self.summary).unwrap_or_default()),
        ]
    }
}

pub fn extract(metadata: &TableMetadata) -> Vec<SnapshotRow> {
    metadata
        .snapshots()
        .map(|s| SnapshotRow {
            snapshot_id: s.snapshot_id(),
            parent_id: s.parent_snapshot_id(),
            timestamp_ms: s.timestamp_ms(),
            operation: s.summary().operation.as_str().to_string(),
            manifest_list: s.manifest_list().to_string(),
            summary: s.summary().additional_properties.clone(),
        })
        .collect()
}
