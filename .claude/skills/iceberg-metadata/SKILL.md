---
name: iceberg-metadata
description: >
  Reference and query guidance for Apache Iceberg metadata tables. Use this skill whenever
  the user asks about Iceberg table internals, metadata inspection, snapshot management,
  manifest files, data file tracking, time travel, branch/tag management, table health,
  storage monitoring, or debugging Iceberg table state. Trigger on keywords like: Iceberg
  metadata, snapshot, manifest, iceberg history, all_data_files, all_manifests, iceberg refs,
  iceberg entries, iceberg files, iceberg partitions, table branching, Iceberg time travel,
  compaction monitoring, or any question about querying Iceberg table internals.
---

# Apache Iceberg Metadata Tables

Iceberg exposes table internals as queryable **metadata tables**, addressed by appending a
suffix to the table name:

```sql
-- Generic pattern (engine-agnostic)
SELECT * FROM catalog.db.table.<metadata_table>;

-- Spark SQL example
SELECT * FROM my_catalog.db.table.history;

-- Trino/Athena/DuckDB follow the same dot-notation convention
```

## Metadata Table Inventory

| Table | Scope | Primary Use |
|---|---|---|
| `history` | current lineage | Snapshot ancestry, rollback detection |
| `metadata_log_entries` | all metadata files | Metadata file evolution |
| `snapshots` | all snapshots | Snapshot inspection, time travel |
| `entries` | all file ops across all snapshots | Full audit trail |
| `files` | current snapshot | Active data & delete file stats |
| `manifests` | current snapshot | Current manifest state |
| `partitions` | current snapshot | Partition-level stats |
| `all_data_files` | all snapshots | Cross-snapshot data file tracking |
| `all_delete_files` | all snapshots | Cross-snapshot delete file tracking |
| `all_entries` | all snapshots | All file ops (data + deletes) |
| `all_manifests` | all snapshots | Cross-snapshot manifest tracking |
| `refs` | current | Branches and tags |

**Scoping note:** Tables without the `all_` prefix reflect the **current snapshot** only.
`all_*` variants may return multiple rows per file (one per snapshot where the file was valid).

## Quick Reference by Task

**Diagnose snapshot history / rollbacks**
→ Query `history` — look for rows where `parent_id` appears more than once (rollback indicator).

**Inspect a specific snapshot's files**
→ Join `entries` (filter `snapshot_id` + `status=1`) with `files` on `file_path`.

**Monitor storage growth over time**
→ `all_entries` grouped by `snapshot_id`, sum `data_file.file_size_in_bytes` where `status=1`.

**Check manifest bloat**
→ `all_manifests` grouped by `reference_snapshot_id`, sum `length`.

**List active branches/tags**
→ `refs` — filter `type = 'BRANCH'` or `type = 'TAG'`.

**Find files in a branch not in main**
→ Anti-join pattern across two `refs`+`entries`+`files` subqueries (see `query-patterns.md`).

**See partition-level record counts**
→ `partitions` — includes `record_count` and `file_count` per partition value.

## Status Codes (entries / all_entries)

| Value | Meaning |
|---|---|
| 0 | Existing (unchanged in this snapshot) |
| 1 | Added |
| 2 | Deleted |

## Content Type Codes (manifests / all_manifests)

| Value | Meaning |
|---|---|
| 0 | Tracks data files |
| 1 | Tracks delete files |

## Reference Files

- **`references/schemas.md`** — Full field-by-field schema for every metadata table.
  Load when you need precise column names, types, or need to build a complex query.

- **`references/query-patterns.md`** — Curated SQL patterns for common operational tasks
  (storage monitoring, branch diffing, lifecycle tracking, partition evolution, etc.).
  Load when the user needs a ready-to-use or adaptable query.

## Engine Compatibility Notes

- All metadata tables use the same **dot-notation naming** across Spark, Trino, Dremio,
  Flink, and most Iceberg-compatible engines.
- **Flink** supports `history`, `snapshots`, `files`, and `manifests`; `all_*` tables
  may not be available depending on version.
- **DuckDB** supports metadata tables via the `iceberg_scan` + metadata extension.
- Column names are spec-defined and stable across engines; behavior of `null` fields
  (e.g., `contains_nan` in V1 tables) may vary.
- `partition_summaries` lower/upper bounds are stored as **binary-encoded** type-specific
  values in the raw Avro; query engines decode them for you.
