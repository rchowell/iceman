use iceberg::spec::TableMetadata;
use serde_json::{json, Value};

pub fn snapshots(metadata: &TableMetadata) -> Vec<Value> {
    metadata
        .snapshots()
        .map(|s| {
            json!({
                "snapshot_id": s.snapshot_id(),
                "parent_id": s.parent_snapshot_id(),
                "timestamp_ms": s.timestamp_ms(),
                "operation": s.summary().operation.as_str(),
                "manifest_list": s.manifest_list(),
                "summary": &s.summary().additional_properties,
            })
        })
        .collect()
}

pub fn history(metadata: &TableMetadata) -> Vec<Value> {
    let current_id = metadata.current_snapshot_id();
    let mut ancestor_ids = std::collections::HashSet::new();
    if let Some(mut id) = current_id {
        loop {
            ancestor_ids.insert(id);
            match metadata.snapshot_by_id(id).and_then(|s| s.parent_snapshot_id()) {
                Some(pid) => id = pid,
                None => break,
            }
        }
    }

    metadata
        .history()
        .iter()
        .map(|log| {
            json!({
                "made_current_at": log.timestamp_ms,
                "snapshot_id": log.snapshot_id,
                "parent_id": metadata
                    .snapshot_by_id(log.snapshot_id)
                    .and_then(|s| s.parent_snapshot_id()),
                "is_current_ancestor": ancestor_ids.contains(&log.snapshot_id),
            })
        })
        .collect()
}

pub fn metadata_log(metadata: &TableMetadata) -> Vec<Value> {
    metadata
        .metadata_log()
        .iter()
        .map(|log| {
            json!({
                "timestamp": log.timestamp_ms,
                "file": log.metadata_file,
            })
        })
        .collect()
}
