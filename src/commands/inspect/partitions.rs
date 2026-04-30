use std::collections::HashMap;

use anyhow::Result;
use iceberg::spec::DataContentType;
use iceberg::table::Table;
use serde::Serialize;

use crate::render::{Cell, Tabular};

use super::{load_alive_entries, partition_strings, resolve_snapshot};

#[derive(Debug, Default, Serialize)]
pub struct PartitionRow {
    pub partition: String,
    pub spec_id: i32,
    pub record_count: u64,
    pub file_count: u32,
    pub total_data_file_size_in_bytes: u64,
    pub position_delete_record_count: u64,
    pub position_delete_file_count: u32,
    pub equality_delete_record_count: u64,
    pub equality_delete_file_count: u32,
}

impl Tabular for PartitionRow {
    fn headers(verbose: bool) -> &'static [&'static str] {
        if verbose {
            &[
                "partition",
                "spec_id",
                "record_count",
                "file_count",
                "total_data_file_size_in_bytes",
                "position_delete_record_count",
                "position_delete_file_count",
                "equality_delete_record_count",
                "equality_delete_file_count",
            ]
        } else {
            &[
                "partition",
                "file_count",
                "record_count",
                "total_data_file_size_in_bytes",
            ]
        }
    }

    fn row(&self, verbose: bool) -> Vec<Cell> {
        if verbose {
            vec![
                Cell::Str(self.partition.clone()),
                Cell::Int(i64::from(self.spec_id)),
                Cell::UInt(self.record_count),
                Cell::UInt(u64::from(self.file_count)),
                Cell::UInt(self.total_data_file_size_in_bytes),
                Cell::UInt(self.position_delete_record_count),
                Cell::UInt(u64::from(self.position_delete_file_count)),
                Cell::UInt(self.equality_delete_record_count),
                Cell::UInt(u64::from(self.equality_delete_file_count)),
            ]
        } else {
            vec![
                Cell::Str(self.partition.clone()),
                Cell::UInt(u64::from(self.file_count)),
                Cell::UInt(self.record_count),
                Cell::UInt(self.total_data_file_size_in_bytes),
            ]
        }
    }
}

pub async fn extract(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<PartitionRow>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let alive = load_alive_entries(table, metadata, snapshot).await?;
    let spec_id = metadata.default_partition_spec().spec_id();

    let mut stats: HashMap<String, PartitionRow> = HashMap::new();
    for entry in &alive {
        let key = partition_strings(entry.data_file().partition()).join(",");
        let stat = stats.entry(key.clone()).or_insert_with(|| PartitionRow {
            partition: key,
            spec_id,
            ..Default::default()
        });
        let df = entry.data_file();
        match df.content_type() {
            DataContentType::Data => {
                stat.record_count += df.record_count();
                stat.file_count += 1;
                stat.total_data_file_size_in_bytes += df.file_size_in_bytes();
            }
            DataContentType::PositionDeletes => {
                stat.position_delete_record_count += df.record_count();
                stat.position_delete_file_count += 1;
            }
            DataContentType::EqualityDeletes => {
                stat.equality_delete_record_count += df.record_count();
                stat.equality_delete_file_count += 1;
            }
        }
    }

    let mut rows: Vec<PartitionRow> = stats.into_values().collect();
    rows.sort_by(|a, b| a.partition.cmp(&b.partition));
    Ok(rows)
}
