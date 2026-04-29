use std::collections::HashSet;

use iceberg::spec::TableMetadata;
use serde::Serialize;

use crate::render::{Cell, Tabular};

#[derive(Debug, Serialize)]
pub struct HistoryRow {
    pub made_current_at: i64,
    pub snapshot_id: i64,
    pub parent_id: Option<i64>,
    pub is_current_ancestor: bool,
}

impl Tabular for HistoryRow {
    fn headers() -> &'static [&'static str] {
        &["made_current_at", "snapshot_id", "parent_id", "is_current_ancestor"]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::Int(self.made_current_at),
            Cell::Int(self.snapshot_id),
            self.parent_id.map_or(Cell::Null, Cell::Int),
            Cell::Bool(self.is_current_ancestor),
        ]
    }
}

pub fn extract(metadata: &TableMetadata) -> Vec<HistoryRow> {
    let mut ancestors = HashSet::new();
    if let Some(mut id) = metadata.current_snapshot_id() {
        loop {
            ancestors.insert(id);
            match metadata
                .snapshot_by_id(id)
                .and_then(|s| s.parent_snapshot_id())
            {
                Some(pid) => id = pid,
                None => break,
            }
        }
    }

    metadata
        .history()
        .iter()
        .map(|log| HistoryRow {
            made_current_at: log.timestamp_ms,
            snapshot_id: log.snapshot_id,
            parent_id: metadata
                .snapshot_by_id(log.snapshot_id)
                .and_then(|s| s.parent_snapshot_id()),
            is_current_ancestor: ancestors.contains(&log.snapshot_id),
        })
        .collect()
}
