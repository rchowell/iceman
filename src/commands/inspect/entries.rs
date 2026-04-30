use anyhow::Result;
use iceberg::spec::ManifestEntryRef;
use iceberg::table::Table;
use serde::Serialize;

use crate::render::{Cell, Tabular};

use super::{load_entries_for_snapshot, partition_strings, resolve_snapshot};

#[derive(Debug, Serialize)]
pub struct EntryRow {
    pub status: i32,
    pub snapshot_id: Option<i64>,
    pub sequence_number: Option<i64>,
    pub file_sequence_number: Option<i64>,
    pub content: i32,
    pub file_path: String,
    pub file_format: String,
    pub record_count: u64,
    pub file_size_in_bytes: u64,
    pub partition: Option<Vec<String>>,
}

impl Tabular for EntryRow {
    fn headers(verbose: bool) -> &'static [&'static str] {
        if verbose {
            &[
                "status",
                "snapshot_id",
                "sequence_number",
                "file_sequence_number",
                "content",
                "file_path",
                "file_format",
                "record_count",
                "file_size_in_bytes",
                "partition",
            ]
        } else {
            &[
                "status",
                "snapshot_id",
                "content",
                "file_path",
                "record_count",
                "file_size_in_bytes",
            ]
        }
    }

    fn row(&self, verbose: bool) -> Vec<Cell> {
        if verbose {
            vec![
                Cell::Int(i64::from(self.status)),
                self.snapshot_id.map_or(Cell::Null, Cell::Int),
                self.sequence_number.map_or(Cell::Null, Cell::Int),
                self.file_sequence_number.map_or(Cell::Null, Cell::Int),
                Cell::Int(i64::from(self.content)),
                Cell::Str(self.file_path.clone()),
                Cell::Str(self.file_format.clone()),
                Cell::UInt(self.record_count),
                Cell::UInt(self.file_size_in_bytes),
                partition_cell(self.partition.as_ref()),
            ]
        } else {
            vec![
                Cell::Int(i64::from(self.status)),
                self.snapshot_id.map_or(Cell::Null, Cell::Int),
                Cell::Int(i64::from(self.content)),
                Cell::Str(self.file_path.clone()),
                Cell::UInt(self.record_count),
                Cell::UInt(self.file_size_in_bytes),
            ]
        }
    }
}

pub(super) fn from_entry(entry: &ManifestEntryRef) -> EntryRow {
    let df = entry.data_file();
    EntryRow {
        status: entry.status() as i32,
        snapshot_id: entry.snapshot_id(),
        sequence_number: entry.sequence_number(),
        file_sequence_number: entry.sequence_number(),
        content: df.content_type() as i32,
        file_path: df.file_path().to_string(),
        file_format: format!("{:?}", df.file_format()),
        record_count: df.record_count(),
        file_size_in_bytes: df.file_size_in_bytes(),
        partition: partition_field(df.partition()),
    }
}

fn partition_field(s: &iceberg::spec::Struct) -> Option<Vec<String>> {
    let v = partition_strings(s);
    if v.is_empty() { None } else { Some(v) }
}

pub(super) fn partition_cell(p: Option<&Vec<String>>) -> Cell {
    match p {
        None => Cell::Null,
        Some(v) => Cell::Str(serde_json::to_string(v).unwrap_or_default()),
    }
}

pub async fn current(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<EntryRow>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let all = load_entries_for_snapshot(table, metadata, snapshot).await?;
    Ok(all.iter().map(from_entry).collect())
}

pub async fn all(table: &Table) -> Result<Vec<EntryRow>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    for snapshot in metadata.snapshots() {
        let ents = load_entries_for_snapshot(table, metadata, snapshot).await?;
        rows.extend(ents.iter().map(from_entry));
    }
    Ok(rows)
}
