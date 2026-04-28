pub mod entries;
pub mod files;
pub mod manifests;
pub mod partitions;
pub mod refs;
pub mod snapshots;

use anyhow::Result;
use iceberg::table::Table;
use iceberg::spec::TableMetadata;

use crate::cli::InspectTable;

pub async fn run(
    table: &Table,
    inspect_type: &InspectTable,
    snapshot_id: Option<i64>,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let rows = match inspect_type {
        InspectTable::Snapshots => snapshots::snapshots(table.metadata()),
        InspectTable::History => snapshots::history(table.metadata()),
        InspectTable::MetadataLog => snapshots::metadata_log(table.metadata()),
        InspectTable::Refs => refs::refs(table.metadata()),
        InspectTable::Manifests => manifests::manifests(table, snapshot_id).await?,
        InspectTable::AllManifests => manifests::all_manifests(table).await?,
        InspectTable::Entries => entries::entries(table, snapshot_id).await?,
        InspectTable::AllEntries => entries::all_entries(table).await?,
        InspectTable::Files => files::files(table, snapshot_id).await?,
        InspectTable::DataFiles => files::data_files(table, snapshot_id).await?,
        InspectTable::DeleteFiles => files::delete_files(table, snapshot_id).await?,
        InspectTable::AllDataFiles => files::all_data_files(table).await?,
        InspectTable::AllDeleteFiles => files::all_delete_files(table).await?,
        InspectTable::Partitions => partitions::partitions(table, snapshot_id).await?,
    };

    let rows = if let Some(limit) = limit {
        rows.into_iter().take(limit).collect()
    } else {
        rows
    };

    if json {
        println!("{}", serde_json::to_string(&rows)?);
    } else {
        print_table(&rows);
    }
    Ok(())
}

fn print_table(rows: &[serde_json::Value]) {
    if rows.is_empty() {
        return;
    }
    let obj = match &rows[0] {
        serde_json::Value::Object(m) => m,
        _ => {
            for row in rows {
                println!("{row}");
            }
            return;
        }
    };
    let keys: Vec<&String> = obj.keys().collect();

    let mut widths: Vec<usize> = keys.iter().map(|k| k.len()).collect();
    for row in rows {
        if let serde_json::Value::Object(m) = row {
            for (i, key) in keys.iter().enumerate() {
                let val = format_cell(m.get(*key));
                widths[i] = widths[i].max(val.len().min(60));
            }
        }
    }

    // header
    let header: Vec<String> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| format!("{:<width$}", k, width = widths[i]))
        .collect();
    println!("{}", header.join("  "));
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("{}", sep.join("  "));

    // rows
    for row in rows {
        if let serde_json::Value::Object(m) = row {
            let cells: Vec<String> = keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let val = format_cell(m.get(*key));
                    if val.len() > 60 {
                        format!("{:<width$}", format!("{}...", &val[..57]), width = widths[i])
                    } else {
                        format!("{:<width$}", val, width = widths[i])
                    }
                })
                .collect();
            println!("{}", cells.join("  "));
        }
    }
}

fn format_cell(val: Option<&serde_json::Value>) -> String {
    match val {
        None => String::new(),
        Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(v) => v.to_string(),
    }
}

pub fn resolve_snapshot(
    metadata: &TableMetadata,
    snapshot_id: Option<i64>,
) -> Result<&iceberg::spec::SnapshotRef> {
    match snapshot_id {
        Some(id) => metadata
            .snapshot_by_id(id)
            .ok_or_else(|| anyhow::anyhow!("snapshot {id} not found")),
        None => metadata
            .current_snapshot()
            .ok_or_else(|| anyhow::anyhow!("table has no current snapshot")),
    }
}
