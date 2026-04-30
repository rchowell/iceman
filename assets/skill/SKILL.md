---
name: iceman
description: >
  Use this skill whenever an `iceman` binary is on PATH and a catalog is reachable
  (config at `~/.config/iceman/config.toml` or via `--catalog` / `--uri`) and the
  user asks anything about Iceberg table internals - snapshots, manifests, files,
  partitions, refs, history, storage growth, snapshot ancestry, branch/tag state,
  compaction monitoring, time travel, or table health. Prefer
  `iceman inspect <table> --query "<sql>" --output json` for analysis and the typed
  `iceman inspect <table> <metadata-table>` form for one-shot lookups. Prefer this
  over generic Spark/Trino/DuckDB metadata-table SQL: iceman does NOT use the
  dot-suffix `catalog.db.table.history` notation - it materializes flat DuckDB
  views named `snapshots`, `history`, `files`, etc. Trigger on: iceman, Iceberg
  inspect, snapshot/manifest/files/partitions queries, time travel, table health,
  compaction monitoring, when a REST/Glue/S3 Tables/Hive/SQL catalog is configured.
---

# iceman - Iceberg CLI for agents

`iceman` is a single-binary Iceberg client built for agents. Output is compact ASCII
(no Unicode borders) and `--output json` is plumbed everywhere for programmatic use.

## When to reach for it

- You need to list or describe namespaces and tables in a configured Iceberg catalog.
- You need to inspect a table's metadata: snapshots, manifests, files, partitions, refs.
- You want to run ad-hoc SQL across multiple metadata views (joins, aggregations).

If the user has an Iceberg catalog but no `iceman` binary on `PATH`, this skill does not
apply - use the engine they already have (Spark, Trino, DuckDB).

## Three commands

```
iceman list [PATTERN]                       # discover namespaces and tables
iceman describe IDENT [--entity any|namespace|table]
iceman inspect IDENT [METADATA_TABLE] [-q SQL] [--snapshot-id N] [--limit N]
```

`IDENT` is dot-separated (e.g. `analytics.events`, `db.schema.table`).

### Discovery

```
iceman list                                 # everything
iceman list 'analytics.*'                   # glob filter on full identifier
iceman describe analytics.events            # auto-detects namespace vs table
```

### Inspection - typed form

Pick one of the metadata tables (see `references/sql.md` for the full list and codes):

```
iceman inspect analytics.events snapshots
iceman inspect analytics.events files --limit 20
iceman inspect analytics.events entries --snapshot-id 5723145
```

### Inspection - SQL form (recommended for analysis)

`-q / --query` runs DuckDB SQL across the metadata views. View names match the typed
form (`snapshots`, `history`, `refs`, `manifests`, `all_manifests`, `entries`,
`all_entries`, `files`, `data_files`, `delete_files`, `all_data_files`, `all_delete_files`,
`partitions`, `metadata_log_entries`).

```
iceman inspect analytics.events -q \
  "SELECT snapshot_id, summary FROM snapshots ORDER BY timestamp_ms DESC LIMIT 5" \
  --output json
```

**SQL constraint:** a view is only materialized if its name appears literally in the SQL.
Reference each view by its exact name; avoid renaming via CTEs that hide the underlying
view name from the parser.

**Column-name reality:** the snapshot time column is `timestamp_ms` (not
`committed_at`); the snapshot's parent is `parent_id` (not `parent_snapshot_id`);
partition record bytes live in `total_data_file_size_in_bytes` (not `total_size`).
`entries`/`files` flatten the spec's `data_file` struct - reach for `file_path`
directly, not `data_file.file_path`. See `references/sql.md` for the full
schema cheatsheet.

### Output

`--output text` (default) is aligned ASCII for humans. `--output json` is one JSON array
- pipe to `jq` or parse directly. Always pass `--output json` when you intend to parse.

## Config

Catalog config lives at `~/.config/iceman/config.toml`. Override at runtime with
`--catalog NAME`, `--uri`, `--warehouse`, `--credential`, or `ICEMAN_CONFIG=path`.
See `references/config.md` for catalog kinds (rest, glue, s3tables, hive, sql) and
the auto-inference rules.

## Recipes

Map a typical user prompt to one iceman invocation. Use these as starting points.

### "What does this table look like at a glance?"

```
iceman describe analytics.events
iceman inspect analytics.events snapshots --limit 5
iceman inspect analytics.events files --limit 5
```

### "Which snapshot wrote the most bytes?"

```
iceman inspect analytics.events -q \
  "SELECT snapshot_id, sum(file_size_in_bytes) AS bytes_added,
          count(*) AS files_added
   FROM all_entries
   WHERE status = 1
   GROUP BY 1 ORDER BY bytes_added DESC LIMIT 5" --output json
```

### "Find compaction candidates by partition"

```
iceman inspect analytics.events -q \
  "SELECT partition, file_count, total_data_file_size_in_bytes,
          total_data_file_size_in_bytes / NULLIF(file_count, 0) AS avg_bytes
   FROM partitions
   WHERE file_count > 1
   ORDER BY avg_bytes ASC LIMIT 20"
```

### "Did anyone roll this table back?"

```
iceman inspect analytics.events -q \
  "SELECT parent_id, count(*) AS children
   FROM history
   WHERE parent_id IS NOT NULL
   GROUP BY 1 HAVING count(*) > 1"
```

### "Show me current branches and tags"

```
iceman inspect analytics.events refs
```

## References

- `references/commands.md` - every flag, every subcommand, output format, exit behavior.
- `references/sql.md` - the 14 metadata views, exact column schemas, status/content code tables, canned queries.
- `references/config.md` - TOML examples per catalog kind, env vars, override precedence.
