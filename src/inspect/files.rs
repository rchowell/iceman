use std::collections::HashMap;

use anyhow::Result;
use futures::future::try_join_all;
use iceberg::spec::{DataContentType, ManifestEntryRef, SnapshotRef, TableMetadata};
use iceberg::table::Table;
use serde_json::{json, Value};

use super::resolve_snapshot;

fn file_to_json(entry: &ManifestEntryRef) -> Value {
    let df = entry.data_file();
    json!({
        "content": df.content_type() as i32,
        "file_path": df.file_path(),
        "file_format": format!("{:?}", df.file_format()),
        "partition": format_partition(df.partition()),
        "record_count": df.record_count(),
        "file_size_in_bytes": df.file_size_in_bytes(),
        "column_sizes": format_i32_u64_map(df.column_sizes()),
        "value_counts": format_i32_u64_map(df.value_counts()),
        "null_value_counts": format_i32_u64_map(df.null_value_counts()),
        "nan_value_counts": format_i32_u64_map(df.nan_value_counts()),
        "lower_bounds": format_datum_map(df.lower_bounds()),
        "upper_bounds": format_datum_map(df.upper_bounds()),
        "sort_order_id": df.sort_order_id(),
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

fn format_i32_u64_map(m: &HashMap<i32, u64>) -> Value {
    if m.is_empty() {
        Value::Null
    } else {
        let obj: serde_json::Map<String, Value> = m
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect();
        Value::Object(obj)
    }
}

fn format_datum_map(m: &HashMap<i32, iceberg::spec::Datum>) -> Value {
    if m.is_empty() {
        Value::Null
    } else {
        let obj: serde_json::Map<String, Value> = m
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v.to_string())))
            .collect();
        Value::Object(obj)
    }
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

pub async fn files(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    Ok(alive.iter().map(|e| file_to_json(e)).collect())
}

pub async fn data_files(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    Ok(alive
        .iter()
        .filter(|e| e.data_file().content_type() == DataContentType::Data)
        .map(|e| file_to_json(e))
        .collect())
}

pub async fn delete_files(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    Ok(alive
        .iter()
        .filter(|e| e.data_file().content_type() != DataContentType::Data)
        .map(|e| file_to_json(e))
        .collect())
}

pub async fn all_data_files(table: &Table) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    for snapshot in metadata.snapshots() {
        let alive = load_alive_entries(table, metadata, snapshot).await?;
        rows.extend(
            alive
                .iter()
                .filter(|e| e.data_file().content_type() == DataContentType::Data)
                .map(|e| file_to_json(e)),
        );
    }
    Ok(rows)
}

pub async fn all_delete_files(table: &Table) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    for snapshot in metadata.snapshots() {
        let alive = load_alive_entries(table, metadata, snapshot).await?;
        rows.extend(
            alive
                .iter()
                .filter(|e| e.data_file().content_type() != DataContentType::Data)
                .map(|e| file_to_json(e)),
        );
    }
    Ok(rows)
}
