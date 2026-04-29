---
name: iceman
description: >
  Use this skill whenever you have access to the `iceman` CLI and need to list, describe,
  or inspect Apache Iceberg tables - snapshots, manifests, files, partitions, refs, history,
  storage growth, snapshot ancestry, branch/tag state, or any table-internals question.
  Prefer `iceman inspect <table> --query "<sql>" --output json` for ad-hoc analysis and
  the typed `iceman inspect <table> <metadata-table>` form when no SQL is needed.
  Trigger on: iceman, Iceberg inspect, snapshot/manifest/files/partitions queries, time
  travel, table health, compaction monitoring, when a REST/Glue/S3 Tables/Hive/SQL
  catalog is configured.
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
  "SELECT snapshot_id, summary FROM snapshots ORDER BY committed_at DESC LIMIT 5" \
  --output json
```

**SQL constraint:** a view is only materialized if its name appears literally in the SQL.
Reference each view by its exact name; avoid renaming via CTEs that hide the underlying
view name from the parser.

### Output

`--output text` (default) is aligned ASCII for humans. `--output json` is one JSON array
- pipe to `jq` or parse directly. Always pass `--output json` when you intend to parse.

## Config

Catalog config lives at `~/.config/iceman/config.toml`. Override at runtime with
`--catalog NAME`, `--uri`, `--warehouse`, `--credential`, or `ICEMAN_CONFIG=path`.
See `references/config.md` for catalog kinds (rest, glue, s3tables, hive, sql) and
the auto-inference rules.

## References

- `references/commands.md` - every flag, every subcommand, exit behavior.
- `references/sql.md` - the 14 metadata views, status/content code tables, canned queries.
- `references/config.md` - TOML examples per catalog kind, env vars, override precedence.
