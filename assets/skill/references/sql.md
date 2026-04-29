# SQL over iceman metadata views

`iceman inspect <table> -q "<sql>"` materializes Iceberg metadata into DuckDB views
and runs your SQL against them. This file documents the views, key column codes,
and a handful of canned queries.

## How materialization works

For each view name found *literally* in your SQL, iceman extracts the matching
metadata, writes NDJSON to a temp dir, and `CREATE TABLE`s it via
`read_json_auto`. Implications:

- Reference views by exact name. Avoid only-aliasing: `WITH s AS (SELECT * FROM
  snapshots)` is fine because `snapshots` still appears in the text.
- Snapshot-scoped views (`manifests`, `entries`, `files`, etc.) are skipped if the
  table has no current snapshot - your query will see them as missing tables.
- Each query starts from a fresh in-memory DuckDB; no cross-query state.

## Views

Snake_case names; the typed mode equivalents in `commands.md` use kebab-case.

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
variants may return multiple rows per file (one per snapshot the file was valid in).

## Status codes (entries / all_entries)

| Value | Meaning                                   |
|-------|-------------------------------------------|
| 0     | Existing (unchanged in this snapshot)     |
| 1     | Added                                     |
| 2     | Deleted                                   |

Common pattern: `WHERE status = 1` for "added in this snapshot".

## Content type codes (manifests / all_manifests)

| Value | Meaning             |
|-------|---------------------|
| 0     | Tracks data files   |
| 1     | Tracks delete files |

## Canned queries

### Latest 5 snapshots

```sql
SELECT snapshot_id, parent_snapshot_id, committed_at, summary
FROM snapshots
ORDER BY committed_at DESC
LIMIT 5;
```

### Storage growth per snapshot

```sql
SELECT snapshot_id,
       count(*)                     AS files_added,
       sum(file_size_in_bytes)      AS bytes_added
FROM all_entries
WHERE status = 1
GROUP BY snapshot_id
ORDER BY snapshot_id;
```

### Manifest bloat across history

```sql
SELECT reference_snapshot_id,
       count(*)     AS n_manifests,
       sum(length)  AS manifest_bytes
FROM all_manifests
GROUP BY reference_snapshot_id
ORDER BY manifest_bytes DESC;
```

### Active branches and tags

```sql
SELECT name, type, snapshot_id, max_reference_age_ms
FROM refs
ORDER BY type, name;
```

### Partition-level record counts

```sql
SELECT partition, record_count, file_count, total_size
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
SELECT count(*)                 AS n_files,
       avg(file_size_in_bytes)  AS avg_bytes,
       sum(file_size_in_bytes)  AS total_bytes
FROM data_files;
```

## Tips

- DuckDB infers schema from NDJSON. If a column is `null` in every row, downstream
  type-coercion can fail; cast explicitly (e.g. `CAST(parent_snapshot_id AS BIGINT)`).
- `summary` and `partition` columns are nested JSON / structs. Use DuckDB's
  `summary->>'operation'` or `partition.field_name` to drill in.
- Use `--output json` when piping to `jq` or another parser. The default text mode
  is for humans.
- Add `LIMIT` defensively for `all_entries` and `all_data_files` on large tables.
