use anyhow::Result;
use futures::future::try_join_all;
use iceberg::spec::{ManifestEntryRef, SnapshotRef, TableMetadata};
use iceberg::table::Table;
use serde_json::{json, Value};

use super::resolve_snapshot;

fn entry_to_json(entry: &ManifestEntryRef) -> Value {
    let df = entry.data_file();
    json!({
        "status": entry.status() as i32,
        "snapshot_id": entry.snapshot_id(),
        "sequence_number": entry.sequence_number(),
        "file_sequence_number": entry.sequence_number(),
        "content": df.content_type() as i32,
        "file_path": df.file_path(),
        "file_format": format!("{:?}", df.file_format()),
        "record_count": df.record_count(),
        "file_size_in_bytes": df.file_size_in_bytes(),
        "partition": format_partition(df.partition()),
    })
}

fn format_partition(s: &iceberg::spec::Struct) -> Value {
    let fields: Vec<String> = s
        .fields()
        .iter()
        .map(|f| match f {
            Some(lit) => format!("{lit:?}"),
            None => "null".to_string(),
        })
        .collect();
    if fields.is_empty() {
        Value::Null
    } else {
        json!(fields)
    }
}

async fn load_entries_for_snapshot(
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
        .collect())
}

pub async fn entries(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let all = load_entries_for_snapshot(table, metadata, snapshot).await?;
    Ok(all.iter().map(|e| entry_to_json(e)).collect())
}

pub async fn all_entries(table: &Table) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    for snapshot in metadata.snapshots() {
        let ents = load_entries_for_snapshot(table, metadata, snapshot).await?;
        rows.extend(ents.iter().map(|e| entry_to_json(e)));
    }
    Ok(rows)
}
