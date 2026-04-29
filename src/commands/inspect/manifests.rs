use std::collections::HashSet;

use anyhow::Result;
use iceberg::spec::ManifestFile;
use iceberg::table::Table;
use serde::Serialize;

use crate::render::{Cell, Tabular};

use super::resolve_snapshot;

#[derive(Debug, Serialize)]
pub struct ManifestRow {
    pub content: i32,
    pub path: String,
    pub length: i64,
    pub partition_spec_id: i32,
    pub added_snapshot_id: i64,
    pub added_data_files_count: Option<u32>,
    pub existing_data_files_count: Option<u32>,
    pub deleted_data_files_count: Option<u32>,
    pub added_rows_count: Option<u64>,
    pub existing_rows_count: Option<u64>,
    pub deleted_rows_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_snapshot_id: Option<i64>,
}

impl Tabular for ManifestRow {
    fn headers() -> &'static [&'static str] {
        &[
            "content",
            "path",
            "length",
            "partition_spec_id",
            "added_snapshot_id",
            "added_data_files_count",
            "existing_data_files_count",
            "deleted_data_files_count",
            "added_rows_count",
            "existing_rows_count",
            "deleted_rows_count",
            "reference_snapshot_id",
        ]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::Int(i64::from(self.content)),
            Cell::Str(self.path.clone()),
            Cell::Int(self.length),
            Cell::Int(i64::from(self.partition_spec_id)),
            Cell::Int(self.added_snapshot_id),
            opt_u32(self.added_data_files_count),
            opt_u32(self.existing_data_files_count),
            opt_u32(self.deleted_data_files_count),
            self.added_rows_count.map_or(Cell::Null, Cell::UInt),
            self.existing_rows_count.map_or(Cell::Null, Cell::UInt),
            self.deleted_rows_count.map_or(Cell::Null, Cell::UInt),
            self.reference_snapshot_id.map_or(Cell::Null, Cell::Int),
        ]
    }
}

fn opt_u32(v: Option<u32>) -> Cell {
    v.map_or(Cell::Null, |n| Cell::UInt(u64::from(n)))
}

fn from_manifest(mf: &ManifestFile, reference_snapshot_id: Option<i64>) -> ManifestRow {
    ManifestRow {
        content: mf.content as i32,
        path: mf.manifest_path.clone(),
        length: mf.manifest_length,
        partition_spec_id: mf.partition_spec_id,
        added_snapshot_id: mf.added_snapshot_id,
        added_data_files_count: mf.added_files_count,
        existing_data_files_count: mf.existing_files_count,
        deleted_data_files_count: mf.deleted_files_count,
        added_rows_count: mf.added_rows_count,
        existing_rows_count: mf.existing_rows_count,
        deleted_rows_count: mf.deleted_rows_count,
        reference_snapshot_id,
    }
}

pub async fn current(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<ManifestRow>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let manifest_list = snapshot
        .load_manifest_list(table.file_io(), metadata)
        .await?;
    Ok(manifest_list
        .entries()
        .iter()
        .map(|mf| from_manifest(mf, None))
        .collect())
}

pub async fn all(table: &Table) -> Result<Vec<ManifestRow>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for snapshot in metadata.snapshots() {
        let manifest_list = snapshot
            .load_manifest_list(table.file_io(), metadata)
            .await?;
        for mf in manifest_list.entries() {
            if seen.insert(mf.manifest_path.clone()) {
                rows.push(from_manifest(mf, Some(snapshot.snapshot_id())));
            }
        }
    }
    Ok(rows)
}
