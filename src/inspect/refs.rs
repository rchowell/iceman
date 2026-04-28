use iceberg::spec::{SnapshotRetention, TableMetadata};
use serde_json::{json, Value};

pub fn refs(metadata: &TableMetadata) -> Vec<Value> {
    metadata
        .refs()
        .iter()
        .map(|(name, snap_ref)| {
            let (ref_type, min_snapshots, max_snapshot_age, max_ref_age) = match &snap_ref.retention
            {
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
            json!({
                "name": name,
                "type": ref_type,
                "snapshot_id": snap_ref.snapshot_id,
                "max_reference_age_in_ms": max_ref_age,
                "min_snapshots_to_keep": min_snapshots,
                "max_snapshot_age_in_ms": max_snapshot_age,
            })
        })
        .collect()
}
