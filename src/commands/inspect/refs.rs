use iceberg::spec::{SnapshotRetention, TableMetadata};
use serde::Serialize;

use crate::render::{Cell, Tabular};

#[derive(Debug, Serialize)]
pub struct RefRow {
    pub name: String,
    #[serde(rename = "type")]
    pub ref_type: &'static str,
    pub snapshot_id: i64,
    pub max_reference_age_in_ms: Option<i64>,
    pub min_snapshots_to_keep: Option<i32>,
    pub max_snapshot_age_in_ms: Option<i64>,
}

impl Tabular for RefRow {
    fn headers() -> &'static [&'static str] {
        &[
            "name",
            "type",
            "snapshot_id",
            "max_reference_age_in_ms",
            "min_snapshots_to_keep",
            "max_snapshot_age_in_ms",
        ]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::Str(self.name.clone()),
            Cell::Str(self.ref_type.to_string()),
            Cell::Int(self.snapshot_id),
            self.max_reference_age_in_ms.map_or(Cell::Null, Cell::Int),
            self.min_snapshots_to_keep
                .map_or(Cell::Null, |n| Cell::Int(i64::from(n))),
            self.max_snapshot_age_in_ms.map_or(Cell::Null, Cell::Int),
        ]
    }
}

pub fn extract(metadata: &TableMetadata) -> Vec<RefRow> {
    metadata
        .refs()
        .iter()
        .map(|(name, snap_ref)| {
            let (ref_type, min_snapshots, max_snapshot_age, max_ref_age) =
                match &snap_ref.retention {
                    SnapshotRetention::Branch {
                        min_snapshots_to_keep,
                        max_snapshot_age_ms,
                        max_ref_age_ms,
                    } => (
                        "branch",
                        *min_snapshots_to_keep,
                        *max_snapshot_age_ms,
                        *max_ref_age_ms,
                    ),
                    SnapshotRetention::Tag { max_ref_age_ms } => {
                        ("tag", None, None, *max_ref_age_ms)
                    }
                };
            RefRow {
                name: name.clone(),
                ref_type,
                snapshot_id: snap_ref.snapshot_id,
                max_reference_age_in_ms: max_ref_age,
                min_snapshots_to_keep: min_snapshots,
                max_snapshot_age_in_ms: max_snapshot_age,
            }
        })
        .collect()
}
