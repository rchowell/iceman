use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use iceberg::spec::{DataContentType, Datum, ManifestEntryRef};
use iceberg::table::Table;
use serde::Serialize;

use crate::render::{Cell, Tabular};

use super::entries::partition_cell;
use super::{load_alive_entries, partition_strings, resolve_snapshot};

#[derive(Debug, Serialize)]
pub struct FileRow {
    pub content: i32,
    pub file_path: String,
    pub file_format: String,
    pub partition: Option<Vec<String>>,
    pub record_count: u64,
    pub file_size_in_bytes: u64,
    pub column_sizes: Option<BTreeMap<String, u64>>,
    pub value_counts: Option<BTreeMap<String, u64>>,
    pub null_value_counts: Option<BTreeMap<String, u64>>,
    pub nan_value_counts: Option<BTreeMap<String, u64>>,
    pub lower_bounds: Option<BTreeMap<String, String>>,
    pub upper_bounds: Option<BTreeMap<String, String>>,
    pub sort_order_id: Option<i32>,
}

impl Tabular for FileRow {
    fn headers() -> &'static [&'static str] {
        &[
            "content",
            "file_path",
            "file_format",
            "partition",
            "record_count",
            "file_size_in_bytes",
            "column_sizes",
            "value_counts",
            "null_value_counts",
            "nan_value_counts",
            "lower_bounds",
            "upper_bounds",
            "sort_order_id",
        ]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::Int(i64::from(self.content)),
            Cell::Str(self.file_path.clone()),
            Cell::Str(self.file_format.clone()),
            partition_cell(self.partition.as_ref()),
            Cell::UInt(self.record_count),
            Cell::UInt(self.file_size_in_bytes),
            json_cell(self.column_sizes.as_ref()),
            json_cell(self.value_counts.as_ref()),
            json_cell(self.null_value_counts.as_ref()),
            json_cell(self.nan_value_counts.as_ref()),
            json_cell(self.lower_bounds.as_ref()),
            json_cell(self.upper_bounds.as_ref()),
            self.sort_order_id
                .map_or(Cell::Null, |n| Cell::Int(i64::from(n))),
        ]
    }
}

fn json_cell<V: Serialize>(m: Option<&BTreeMap<String, V>>) -> Cell {
    match m {
        None => Cell::Null,
        Some(map) => Cell::Str(serde_json::to_string(map).unwrap_or_default()),
    }
}

fn u64_map(m: &HashMap<i32, u64>) -> Option<BTreeMap<String, u64>> {
    if m.is_empty() {
        None
    } else {
        Some(m.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }
}

fn datum_map(m: &HashMap<i32, Datum>) -> Option<BTreeMap<String, String>> {
    if m.is_empty() {
        None
    } else {
        Some(m.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }
}

fn from_entry(entry: &ManifestEntryRef) -> FileRow {
    let df = entry.data_file();
    let partition_v = partition_strings(df.partition());
    FileRow {
        content: df.content_type() as i32,
        file_path: df.file_path().to_string(),
        file_format: format!("{:?}", df.file_format()),
        partition: if partition_v.is_empty() { None } else { Some(partition_v) },
        record_count: df.record_count(),
        file_size_in_bytes: df.file_size_in_bytes(),
        column_sizes: u64_map(df.column_sizes()),
        value_counts: u64_map(df.value_counts()),
        null_value_counts: u64_map(df.null_value_counts()),
        nan_value_counts: u64_map(df.nan_value_counts()),
        lower_bounds: datum_map(df.lower_bounds()),
        upper_bounds: datum_map(df.upper_bounds()),
        sort_order_id: df.sort_order_id(),
    }
}

fn is_data(e: &ManifestEntryRef) -> bool {
    e.data_file().content_type() == DataContentType::Data
}

fn is_delete(e: &ManifestEntryRef) -> bool {
    e.data_file().content_type() != DataContentType::Data
}

pub async fn alive(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<FileRow>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    Ok(alive.iter().map(from_entry).collect())
}

async fn alive_filtered(
    table: &Table,
    snapshot_id: Option<i64>,
    pred: fn(&ManifestEntryRef) -> bool,
) -> Result<Vec<FileRow>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    Ok(alive.iter().filter(|e| pred(e)).map(from_entry).collect())
}

async fn all_filtered(
    table: &Table,
    pred: fn(&ManifestEntryRef) -> bool,
) -> Result<Vec<FileRow>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    for snapshot in metadata.snapshots() {
        let alive = load_alive_entries(table, metadata, snapshot).await?;
        rows.extend(alive.iter().filter(|e| pred(e)).map(from_entry));
    }
    Ok(rows)
}

pub async fn data(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<FileRow>> {
    alive_filtered(table, snapshot_id, is_data).await
}

pub async fn delete(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<FileRow>> {
    alive_filtered(table, snapshot_id, is_delete).await
}

pub async fn all_data(table: &Table) -> Result<Vec<FileRow>> {
    all_filtered(table, is_data).await
}

pub async fn all_delete(table: &Table) -> Result<Vec<FileRow>> {
    all_filtered(table, is_delete).await
}
