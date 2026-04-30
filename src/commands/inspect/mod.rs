pub mod entries;
pub mod files;
pub mod history;
pub mod manifests;
pub mod metadata_log;
pub mod partitions;
pub mod refs;
pub mod snapshots;

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use futures::future::try_join_all;
use iceberg::spec::{ManifestEntryRef, SnapshotRef, TableMetadata};
use iceberg::table::Table;
use serde::Serialize;

use crate::cli::{MetadataTable, OutputFormat};
use crate::render::{RenderOpts, Tabular, render_rows};

pub async fn run(
    table: &Table,
    inspect_type: Option<&MetadataTable>,
    query: Option<&str>,
    snapshot_id: Option<i64>,
    limit: Option<usize>,
    fmt: OutputFormat,
    opts: RenderOpts,
) -> Result<()> {
    if let Some(sql) = query {
        let rows = run_query(table, sql).await?;
        render_query_result(&rows, limit, fmt, opts)?;
        return Ok(());
    }

    let Some(t) = inspect_type else {
        anyhow::bail!("either a metadata table positional argument or --query/-q is required");
    };

    let metadata = table.metadata();
    match t {
        MetadataTable::Snapshots => render_typed(snapshots::extract(metadata), limit, fmt, opts),
        MetadataTable::History => render_typed(history::extract(metadata), limit, fmt, opts),
        MetadataTable::MetadataLog => {
            render_typed(metadata_log::extract(metadata), limit, fmt, opts)
        }
        MetadataTable::Refs => render_typed(refs::extract(metadata), limit, fmt, opts),
        MetadataTable::Manifests => render_typed(
            manifests::current(table, snapshot_id).await?,
            limit,
            fmt,
            opts,
        ),
        MetadataTable::AllManifests => render_typed(manifests::all(table).await?, limit, fmt, opts),
        MetadataTable::Entries => render_typed(
            entries::current(table, snapshot_id).await?,
            limit,
            fmt,
            opts,
        ),
        MetadataTable::AllEntries => render_typed(entries::all(table).await?, limit, fmt, opts),
        MetadataTable::Files => {
            render_typed(files::alive(table, snapshot_id).await?, limit, fmt, opts)
        }
        MetadataTable::DataFiles => {
            render_typed(files::data(table, snapshot_id).await?, limit, fmt, opts)
        }
        MetadataTable::DeleteFiles => {
            render_typed(files::delete(table, snapshot_id).await?, limit, fmt, opts)
        }
        MetadataTable::AllDataFiles => {
            render_typed(files::all_data(table).await?, limit, fmt, opts)
        }
        MetadataTable::AllDeleteFiles => {
            render_typed(files::all_delete(table).await?, limit, fmt, opts)
        }
        MetadataTable::Partitions => render_typed(
            partitions::extract(table, snapshot_id).await?,
            limit,
            fmt,
            opts,
        ),
    }
}

fn render_typed<T: Serialize + Tabular>(
    rows: Vec<T>,
    limit: Option<usize>,
    fmt: OutputFormat,
    opts: RenderOpts,
) -> Result<()> {
    let rows = match limit {
        Some(n) => rows.into_iter().take(n).collect(),
        None => rows,
    };
    render_rows(&rows, fmt, opts)
}

pub fn resolve_snapshot(
    metadata: &TableMetadata,
    snapshot_id: Option<i64>,
) -> Result<&SnapshotRef> {
    match snapshot_id {
        Some(id) => metadata
            .snapshot_by_id(id)
            .ok_or_else(|| anyhow::anyhow!("snapshot {id} not found")),
        None => metadata
            .current_snapshot()
            .ok_or_else(|| anyhow::anyhow!("table has no current snapshot")),
    }
}

pub(super) fn partition_strings(s: &iceberg::spec::Struct) -> Vec<String> {
    s.fields()
        .iter()
        .map(|f| match f {
            Some(lit) => format!("{lit:?}"),
            None => "null".to_string(),
        })
        .collect()
}

pub(super) async fn load_entries_for_snapshot(
    table: &Table,
    metadata: &TableMetadata,
    snapshot: &SnapshotRef,
) -> Result<Vec<ManifestEntryRef>> {
    let manifest_list = snapshot
        .load_manifest_list(table.file_io(), metadata)
        .await?;
    let futs = manifest_list
        .entries()
        .iter()
        .map(|mf| mf.load_manifest(table.file_io()));
    let manifests = try_join_all(futs).await?;
    Ok(manifests
        .into_iter()
        .flat_map(|m| m.into_parts().0)
        .collect())
}

pub(super) async fn load_alive_entries(
    table: &Table,
    metadata: &TableMetadata,
    snapshot: &SnapshotRef,
) -> Result<Vec<ManifestEntryRef>> {
    let mut entries = load_entries_for_snapshot(table, metadata, snapshot).await?;
    entries.retain(|e| e.is_alive());
    Ok(entries)
}

async fn run_query(table: &Table, sql: &str) -> Result<Vec<serde_json::Value>> {
    let sql = sql.trim_end_matches(';').trim();
    let metadata = table.metadata();
    let dir = tempfile::tempdir()?;
    let sql_lower = sql.to_lowercase();
    let has_snapshot = metadata.current_snapshot().is_some();

    let mut create_stmts = Vec::new();
    for &kind in MetadataTable::ALL {
        if !sql_lower.contains(kind.sql_name()) {
            continue;
        }
        if kind.requires_snapshot() && !has_snapshot {
            continue;
        }
        if let Some(stmt) = extract_and_write(kind, table, &dir).await? {
            create_stmts.push(stmt);
        }
    }

    let result_path = dir.path().join("_result.json");
    let mut script = String::new();
    for stmt in &create_stmts {
        script.push_str(stmt);
        script.push_str(";\n");
    }
    script.push_str(&format!(
        "COPY ({sql}) TO '{}' (FORMAT JSON, ARRAY TRUE);\n",
        result_path.display()
    ));

    run_duckdb(&script)?;

    let result_json = std::fs::read_to_string(&result_path)
        .context("duckdb produced no result file; check the SQL")?;
    Ok(serde_json::from_str(&result_json)?)
}

fn run_duckdb(script: &str) -> Result<()> {
    let mut child = Command::new("duckdb")
        .arg("-bail")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "the 'duckdb' CLI is required for -q/--query but was not found on PATH. \
                     Install it: https://duckdb.org/docs/installation/ \
                     (e.g. 'brew install duckdb' on macOS)."
                )
            } else {
                anyhow::Error::new(e).context("failed to spawn duckdb")
            }
        })?;

    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin.write_all(script.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("duckdb failed: {}", stderr.trim());
    }
    Ok(())
}

async fn extract_and_write(
    kind: MetadataTable,
    table: &Table,
    dir: &tempfile::TempDir,
) -> Result<Option<String>> {
    let metadata = table.metadata();
    let name = kind.sql_name();
    let stmt = match kind {
        MetadataTable::Snapshots => write_table(dir, name, &snapshots::extract(metadata))?,
        MetadataTable::History => write_table(dir, name, &history::extract(metadata))?,
        MetadataTable::MetadataLog => write_table(dir, name, &metadata_log::extract(metadata))?,
        MetadataTable::Refs => write_table(dir, name, &refs::extract(metadata))?,
        MetadataTable::Manifests => {
            let rows = manifests::current(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::AllManifests => {
            let rows = manifests::all(table).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::Entries => {
            let rows = entries::current(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::AllEntries => {
            let rows = entries::all(table).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::Files => {
            let rows = files::alive(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::DataFiles => {
            let rows = files::data(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::DeleteFiles => {
            let rows = files::delete(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::AllDataFiles => {
            let rows = files::all_data(table).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::AllDeleteFiles => {
            let rows = files::all_delete(table).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
        MetadataTable::Partitions => {
            let rows = partitions::extract(table, None).await.unwrap_or_default();
            write_table(dir, name, &rows)?
        }
    };
    Ok(stmt)
}

fn write_table<T: Serialize>(
    dir: &tempfile::TempDir,
    name: &str,
    rows: &[T],
) -> Result<Option<String>> {
    if rows.is_empty() {
        return Ok(None);
    }
    let ndjson: String = rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let path = dir.path().join(format!("{name}.ndjson"));
    std::fs::write(&path, ndjson)?;
    Ok(Some(format!(
        "CREATE TABLE {name} AS SELECT * FROM read_json_auto('{}')",
        path.display()
    )))
}

fn render_query_result(
    rows: &[serde_json::Value],
    limit: Option<usize>,
    fmt: OutputFormat,
    opts: RenderOpts,
) -> Result<()> {
    let rows: Vec<&serde_json::Value> = match limit {
        Some(n) => rows.iter().take(n).collect(),
        None => rows.iter().collect(),
    };

    match fmt {
        OutputFormat::Json => crate::render::render_jsonl_values(&rows),
        OutputFormat::Text => print_query_table(&rows, opts),
    }
}

fn print_query_table(rows: &[&serde_json::Value], opts: RenderOpts) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let serde_json::Value::Object(first) = rows[0] else {
        for row in rows {
            println!("{row}");
        }
        return Ok(());
    };
    let keys: Vec<&String> = first.keys().collect();

    let displayed: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            keys.iter()
                .map(|k| {
                    if let serde_json::Value::Object(m) = row {
                        format_value(m.get(*k))
                    } else {
                        String::new()
                    }
                })
                .collect()
        })
        .collect();

    let numeric: Vec<bool> = (0..keys.len())
        .map(|c| {
            rows.iter().all(|row| {
                matches!(
                    row.as_object().and_then(|m| m.get(keys[c])),
                    Some(serde_json::Value::Number(_) | serde_json::Value::Null) | None
                )
            })
        })
        .collect();

    let header_strs: Vec<&str> = keys.iter().map(|k| k.as_str()).collect();
    crate::render::render_string_table(&header_strs, &displayed, &numeric, opts)
}

fn format_value(v: Option<&serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}
