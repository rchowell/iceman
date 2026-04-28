use std::collections::HashMap;

use anyhow::Result;
use futures::future::try_join_all;
use iceberg::spec::{DataContentType, ManifestEntryRef, SnapshotRef, TableMetadata};
use iceberg::table::Table;
use serde_json::{json, Value};

use super::resolve_snapshot;

struct PartitionStats {
    spec_id: i32,
    record_count: u64,
    file_count: u32,
    total_data_file_size_in_bytes: u64,
    position_delete_record_count: u64,
    position_delete_file_count: u32,
    equality_delete_record_count: u64,
    equality_delete_file_count: u32,
}

impl PartitionStats {
    fn new(spec_id: i32) -> Self {
        Self {
            spec_id,
            record_count: 0,
            file_count: 0,
            total_data_file_size_in_bytes: 0,
            position_delete_record_count: 0,
            position_delete_file_count: 0,
            equality_delete_record_count: 0,
            equality_delete_file_count: 0,
        }
    }
}

fn partition_key(entry: &ManifestEntryRef) -> String {
    let df = entry.data_file();
    let parts: Vec<String> = df
        .partition()
        .fields()
        .iter()
        .map(|f| match f {
            Some(lit) => format!("{lit:?}"),
            None => "null".to_string(),
        })
        .collect();
    parts.join(",")
}

async fn load_alive_entries(
    table: &Table,
    metadata: &TableMetadata,
    snapshot: &SnapshotRef,
) -> Result<Vec<ManifestEntryRef>> {
    let manifest_list = snapshot
        .load_manifest_list(table.file_io(), metadata)
        .await?;
    let futs: Vec<_> = manifest_list
        .entries()
        .iter()
        .map(|mf| mf.load_manifest(table.file_io()))
        .collect();
    let manifests = try_join_all(futs).await?;
    Ok(manifests
        .into_iter()
        .flat_map(|m| {
            let (entries, _metadata) = m.into_parts();
            entries
        })
        .filter(|e| e.is_alive())
        .collect())
}

pub async fn partitions(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;

    let spec = metadata.default_partition_spec();
    let spec_id = spec.spec_id();

    let mut stats_map: HashMap<String, PartitionStats> = HashMap::new();

    for entry in &alive {
        let key = partition_key(entry);
        let stats = stats_map.entry(key).or_insert_with(|| PartitionStats::new(spec_id));
        let df = entry.data_file();
        match df.content_type() {
            DataContentType::Data => {
                stats.record_count += df.record_count();
                stats.file_count += 1;
                stats.total_data_file_size_in_bytes += df.file_size_in_bytes();
            }
            DataContentType::PositionDeletes => {
                stats.position_delete_record_count += df.record_count();
                stats.position_delete_file_count += 1;
            }
            DataContentType::EqualityDeletes => {
                stats.equality_delete_record_count += df.record_count();
                stats.equality_delete_file_count += 1;
            }
        }
    }

    let mut rows: Vec<Value> = stats_map
        .into_iter()
        .map(|(key, stats)| {
            json!({
                "partition": key,
                "spec_id": stats.spec_id,
                "record_count": stats.record_count,
                "file_count": stats.file_count,
                "total_data_file_size_in_bytes": stats.total_data_file_size_in_bytes,
                "position_delete_record_count": stats.position_delete_record_count,
                "position_delete_file_count": stats.position_delete_file_count,
                "equality_delete_record_count": stats.equality_delete_record_count,
                "equality_delete_file_count": stats.equality_delete_file_count,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a["partition"]
            .as_str()
            .cmp(&b["partition"].as_str())
    });
    Ok(rows)
}
