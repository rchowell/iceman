use anyhow::Result;
use iceberg::table::Table;
use serde_json::{json, Value};

use super::resolve_snapshot;

fn manifest_to_json(mf: &iceberg::spec::ManifestFile, reference_snapshot_id: Option<i64>) -> Value {
    let mut row = json!({
        "content": mf.content as i32,
        "path": mf.manifest_path,
        "length": mf.manifest_length,
        "partition_spec_id": mf.partition_spec_id,
        "added_snapshot_id": mf.added_snapshot_id,
        "added_data_files_count": mf.added_files_count,
        "existing_data_files_count": mf.existing_files_count,
        "deleted_data_files_count": mf.deleted_files_count,
        "added_rows_count": mf.added_rows_count,
        "existing_rows_count": mf.existing_rows_count,
        "deleted_rows_count": mf.deleted_rows_count,
    });
    if let Some(ref_id) = reference_snapshot_id {
        row["reference_snapshot_id"] = json!(ref_id);
    }
    row
}

pub async fn manifests(table: &Table, snapshot_id: Option<i64>) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let snapshot = resolve_snapshot(metadata, snapshot_id)?;
    let manifest_list = snapshot
        .load_manifest_list(table.file_io(), metadata)
        .await?;
    Ok(manifest_list
        .entries()
        .iter()
        .map(|mf| manifest_to_json(mf, None))
        .collect())
}

pub async fn all_manifests(table: &Table) -> Result<Vec<Value>> {
    let metadata = table.metadata();
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for snapshot in metadata.snapshots() {
        let manifest_list = snapshot
            .load_manifest_list(table.file_io(), metadata)
            .await?;
        for mf in manifest_list.entries() {
            if seen.insert(mf.manifest_path.clone()) {
                rows.push(manifest_to_json(mf, Some(snapshot.snapshot_id())));
            }
        }
    }
    Ok(rows)
}
