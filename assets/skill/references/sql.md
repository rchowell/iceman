# SQL over iceman metadata views

`iceman inspect <table> -q "<sql>"` materializes Iceberg metadata into DuckDB
views and runs your SQL against them. This file documents the views, exact
column names, key codes, and ready-to-run queries.

## How materialization works

For each view name found *literally* in your SQL, iceman extracts the matching
metadata, writes NDJSON to a temp dir, and `CREATE TABLE`s it via
`read_json_auto`. Implications:

- Reference views by exact name. Avoid only-aliasing: `WITH s AS (SELECT *
  FROM snapshots)` is fine because `snapshots` still appears in the text.
- Snapshot-scoped views (`manifests`, `entries`, `files`, etc.) are skipped if
  the table has no current snapshot - your query will see them as missing.
- Each query starts from a fresh in-memory DuckDB; no cross-query state.

## Views

Snake_case SQL names. The typed-mode equivalents in `commands.md` use kebab-case.

| View                   | Scope                              | Use                                   |
|------------------------|------------------------------------|---------------------------------------|
| `snapshots`            | all snapshots                      | snapshot inspection, time travel      |
| `history`              | current ref's lineage              | ancestry, rollback detection          |
| `metadata_log_entries` | all metadata files                 | metadata file evolution               |
| `refs`                 | current                            | branches and tags                     |
| `manifests`            | current snapshot                   | current manifest state                |
| `all_manifests`        | all snapshots                      | manifest churn / cross-snapshot       |
| `entries`              | current snapshot                   | per-file ops in current snapshot      |
| `all_entries`          | all snapshots                      | full audit trail                      |
| `files`                | current snapshot, live             | data + delete files combined          |
| `data_files`           | current snapshot, live             | data files only                       |
| `delete_files`         | current snapshot, live             | delete files only                     |
| `all_data_files`       | all snapshots                      | cross-snapshot data file tracking     |
| `all_delete_files`     | all snapshots                      | cross-snapshot delete file tracking   |
| `partitions`           | current snapshot                   | per-partition record/file counts      |

Tables without the `all_` prefix reflect only the **current snapshot**. `all_*`
variants may return multiple rows per file (one per snapshot the file was
valid in).

## Schema cheatsheet

These are the **exact** columns iceman emits. Generic Iceberg-spec docs mention
fields iceman does not surface; trust this table for what you can SELECT.

### `snapshots`

`snapshot_id` (BIGINT), `parent_id` (BIGINT, nullable), `timestamp_ms`
(BIGINT), `operation` (VARCHAR), `manifest_list` (VARCHAR), `summary` (STRUCT
of string→string).

Note: time is `timestamp_ms` (epoch ms), not `committed_at`. Convert with
`to_timestamp(timestamp_ms / 1000)`.

### `history`

`made_current_at` (BIGINT, epoch ms), `snapshot_id`, `parent_id`,
`is_current_ancestor`.

### `metadata_log_entries`

`timestamp` (BIGINT, epoch ms), `file` (VARCHAR).

### `refs`

`name`, `type` (`"branch"` or `"tag"`), `snapshot_id`,
`max_reference_age_in_ms`, `min_snapshots_to_keep`, `max_snapshot_age_in_ms`.

### `manifests` / `all_manifests`

`content` (INT, 0=data, 1=delete), `path`, `length`, `partition_spec_id`,
`added_snapshot_id`, `added_data_files_count`, `existing_data_files_count`,
`deleted_data_files_count`, `added_rows_count`, `existing_rows_count`,
`deleted_rows_count`. `all_manifests` adds `reference_snapshot_id`.

### `entries` / `all_entries`

iceman flattens the spec's `data_file` struct to top level, so columns are:

`status` (INT), `snapshot_id`, `sequence_number`, `file_sequence_number`,
`content`, `file_path`, `file_format`, `record_count`, `file_size_in_bytes`,
`partition` (VARCHAR[] or `null`).

You will **not** find `data_file.file_path` etc. — there is no nested struct.

### `files` / `data_files` / `delete_files` / `all_data_files` / `all_delete_files`

`content`, `file_path`, `file_format`, `partition` (VARCHAR[]),
`record_count`, `file_size_in_bytes`, `column_sizes`, `value_counts`,
`null_value_counts`, `nan_value_counts`, `lower_bounds`, `upper_bounds`,
`sort_order_id`.

`column_sizes` and the count maps are MAP<column_id_string, BIGINT>.
`lower_bounds` / `upper_bounds` are MAP<column_id_string, VARCHAR> — iceman
pre-decodes Iceberg's binary bounds to strings, so cast before typed
comparisons (e.g. `CAST(lower_bounds['3'] AS BIGINT)`).

### `partitions`

`partition` (VARCHAR — JSON-encoded partition tuple, **not a struct**),
`spec_id`, `record_count`, `file_count`, `total_data_file_size_in_bytes`,
`position_delete_record_count`, `position_delete_file_count`,
`equality_delete_record_count`, `equality_delete_file_count`.

## Status codes (entries / all_entries)

| Value | Meaning                                   |
|-------|-------------------------------------------|
| 0     | Existing (unchanged in this snapshot)     |
| 1     | Added                                     |
| 2     | Deleted                                   |

Common pattern: `WHERE status = 1` for "added in this snapshot".

## Content type codes

`manifests` / `all_manifests`: 0 = tracks data files, 1 = tracks delete files.

`entries` / `files` and friends: 0 = data, 1 = position delete, 2 = equality
delete.

## Canned queries

### Latest 5 snapshots

```sql
SELECT snapshot_id,
       parent_id,
       to_timestamp(timestamp_ms / 1000) AS committed_at,
       operation,
       summary
FROM snapshots
ORDER BY timestamp_ms DESC
LIMIT 5;
```

### Storage growth per snapshot

```sql
SELECT snapshot_id,
       count(*)                AS files_added,
       sum(file_size_in_bytes) AS bytes_added
FROM all_entries
WHERE status = 1
GROUP BY snapshot_id
ORDER BY snapshot_id;
```

### Manifest bloat across history

```sql
SELECT reference_snapshot_id,
       count(*)    AS n_manifests,
       sum(length) AS manifest_bytes
FROM all_manifests
GROUP BY reference_snapshot_id
ORDER BY manifest_bytes DESC;
```

### Active branches and tags

```sql
SELECT name, type, snapshot_id,
       max_reference_age_in_ms, min_snapshots_to_keep, max_snapshot_age_in_ms
FROM refs
ORDER BY type, name;
```

### Partition-level record counts

```sql
SELECT partition,
       record_count,
       file_count,
       total_data_file_size_in_bytes
FROM partitions
ORDER BY record_count DESC
LIMIT 20;
```

### Find rollbacks (parent appears more than once)

```sql
SELECT parent_id, count(*) AS children
FROM history
WHERE parent_id IS NOT NULL
GROUP BY parent_id
HAVING count(*) > 1;
```

### Average data file size in current snapshot

```sql
SELECT count(*)                AS n_files,
       avg(file_size_in_bytes) AS avg_bytes,
       sum(file_size_in_bytes) AS total_bytes
FROM data_files;
```

### Snapshot summary, drilled in

`summary` is a STRUCT after `read_json_auto` infers stable keys. Use
struct/map indexing — `summary['operation']` works in both cases. Avoid
PostgreSQL `->>` syntax; iceman emits a struct, not a JSON string.

```sql
SELECT snapshot_id,
       summary['operation']      AS op,
       summary['added-records']  AS added,
       summary['total-records']  AS total
FROM snapshots
ORDER BY timestamp_ms DESC
LIMIT 10;
```

### Compaction candidates by partition

```sql
SELECT partition,
       file_count,
       total_data_file_size_in_bytes,
       total_data_file_size_in_bytes / NULLIF(file_count, 0) AS avg_file_bytes,
       position_delete_record_count + equality_delete_record_count
         AS delete_records
FROM partitions
WHERE file_count > 1
ORDER BY avg_file_bytes ASC
LIMIT 20;
```

## Tips

- DuckDB infers schema from NDJSON. If a column is `null` in every row,
  downstream type-coercion can fail; cast explicitly (e.g.
  `CAST(parent_id AS BIGINT)`).
- `summary` is a STRUCT (not a JSON string). Use `summary['operation']`, not
  `summary->>'operation'`.
- `partition` is a VARCHAR array on file/entry views and a JSON-encoded
  VARCHAR on `partitions`. There is no nested struct — drill in with
  `partition[1]` (array, 1-indexed) or `json_extract(partition, '$[0]')`
  on `partitions`.
- `lower_bounds` / `upper_bounds` are already string-decoded; cast for typed
  comparisons.
- Use `--output json` when piping to `jq`. Default text mode is for humans.
- Add `LIMIT` defensively for `all_entries` and `all_data_files` on large
  tables.
