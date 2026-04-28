# Iceberg Metadata Query Patterns

All queries use `catalog.db.table.<metadata_table>` notation. Substitute your actual
catalog/database/table names. Engine-specific syntax notes are called out inline.

## Table of Contents
1. [Snapshot & History](#snapshot--history)
2. [Storage Monitoring](#storage-monitoring)
3. [File Lifecycle & Audit](#file-lifecycle--audit)
4. [Manifest Analysis](#manifest-analysis)
5. [Partition Inspection](#partition-inspection)
6. [Branch & Tag Operations](#branch--tag-operations)
7. [Cross-Table Joins](#cross-table-joins)
8. [Delete File Monitoring](#delete-file-monitoring)

---

## Snapshot & History

### Full snapshot history with operation type
```sql
SELECT
    h.made_current_at,
    h.snapshot_id,
    h.parent_id,
    h.is_current_ancestor,
    s.operation,
    s.summary['added-records']    AS added_records,
    s.summary['deleted-records']  AS deleted_records,
    s.summary['total-records']    AS total_records
FROM catalog.db.table.history h
JOIN catalog.db.table.snapshots s USING (snapshot_id)
ORDER BY h.made_current_at;
```

### Detect rollbacks (parent_id reused)
```sql
SELECT parent_id, COUNT(*) AS child_count
FROM catalog.db.table.history
WHERE parent_id IS NOT NULL
GROUP BY parent_id
HAVING COUNT(*) > 1;
```

### All snapshots for a specific timestamp range
```sql
SELECT snapshot_id, committed_at, operation
FROM catalog.db.table.snapshots
WHERE committed_at BETWEEN TIMESTAMP '2024-01-01' AND TIMESTAMP '2024-02-01'
ORDER BY committed_at;
```

### Snapshot that was current at a point in time (time travel target)
```sql
SELECT snapshot_id, committed_at
FROM catalog.db.table.snapshots
WHERE committed_at <= TIMESTAMP '2024-06-15 12:00:00'
ORDER BY committed_at DESC
LIMIT 1;
```

---

## Storage Monitoring

### Storage added per snapshot (growth curve)
```sql
SELECT
    snapshot_id,
    SUM(data_file.file_size_in_bytes) AS bytes_added,
    COUNT(*)                          AS files_added,
    SUM(data_file.record_count)       AS records_added
FROM catalog.db.table.all_entries
WHERE status = 1  -- added
GROUP BY snapshot_id
ORDER BY snapshot_id;
```

### Total active storage (current snapshot)
```sql
SELECT
    COUNT(*)                    AS file_count,
    SUM(file_size_in_bytes)     AS total_bytes,
    SUM(record_count)           AS total_records
FROM catalog.db.table.files
WHERE content = 0;  -- data files only
```

### Storage by file format
```sql
SELECT
    file_format,
    COUNT(*)                AS file_count,
    SUM(file_size_in_bytes) AS total_bytes
FROM catalog.db.table.files
GROUP BY file_format;
```

### Average file size (identify small file problem)
```sql
SELECT
    COUNT(*)                                              AS file_count,
    SUM(file_size_in_bytes)                               AS total_bytes,
    AVG(file_size_in_bytes)                               AS avg_file_size,
    MIN(file_size_in_bytes)                               AS min_file_size,
    MAX(file_size_in_bytes)                               AS max_file_size,
    SUM(CASE WHEN file_size_in_bytes < 134217728         -- < 128 MB
             THEN 1 ELSE 0 END)                           AS small_file_count
FROM catalog.db.table.files
WHERE content = 0;
```

---

## File Lifecycle & Audit

### All operations on a specific file (full lifecycle)
```sql
SELECT
    snapshot_id,
    sequence_number,
    status,  -- 0=existing, 1=added, 2=deleted
    data_file.file_path,
    data_file.record_count,
    data_file.file_size_in_bytes
FROM catalog.db.table.all_entries
WHERE data_file.file_path = 's3://your-bucket/path/to/file.parquet'
ORDER BY sequence_number;
```

### Files added in a specific snapshot
```sql
SELECT
    data_file.file_path,
    data_file.record_count,
    data_file.file_size_in_bytes,
    data_file.partition
FROM catalog.db.table.entries
WHERE snapshot_id = 1059035530770364194
  AND status = 1;
```

### Files added in a snapshot with full stats (join with files)
```sql
SELECT f.*, e.snapshot_id
FROM catalog.db.table.entries e
JOIN catalog.db.table.files f ON e.data_file.file_path = f.file_path
WHERE e.status = 1
  AND e.snapshot_id = 1059035530770364194;
```

### Orphaned files candidates (deleted but still tracked)
```sql
-- Files marked deleted across all snapshots that are no longer in current files
SELECT DISTINCT data_file.file_path
FROM catalog.db.table.all_entries
WHERE status = 2
  AND data_file.file_path NOT IN (
    SELECT file_path FROM catalog.db.table.files
  );
```

---

## Manifest Analysis

### Manifest size and file counts per snapshot
```sql
SELECT
    reference_snapshot_id,
    COUNT(*)                                                         AS manifest_count,
    SUM(length)                                                      AS total_manifest_bytes,
    SUM(added_data_files_count + existing_data_files_count)          AS total_data_files,
    SUM(added_delete_files_count + existing_delete_files_count)      AS total_delete_files
FROM catalog.db.table.all_manifests
GROUP BY reference_snapshot_id
ORDER BY reference_snapshot_id;
```

### Total size of valid manifests (deduped by path)
```sql
SELECT SUM(length) AS total_manifest_bytes
FROM (
    SELECT DISTINCT path, length
    FROM catalog.db.table.all_manifests
);
```

### Manifests for a specific snapshot
```sql
SELECT path, length, added_data_files_count, existing_data_files_count, deleted_data_files_count
FROM catalog.db.table.all_manifests
WHERE reference_snapshot_id = 6272782676904868561;
```

### Manifest count growth (detect manifest bloat)
```sql
SELECT
    reference_snapshot_id,
    COUNT(*) AS manifest_count
FROM catalog.db.table.all_manifests
GROUP BY reference_snapshot_id
ORDER BY reference_snapshot_id;
```

---

## Partition Inspection

### Current partition stats
```sql
SELECT
    partition,
    record_count,
    file_count,
    total_data_file_size_in_bytes,
    position_delete_record_count,
    equality_delete_record_count,
    last_updated_at
FROM catalog.db.table.partitions
ORDER BY record_count DESC;
```

### Partitions with significant delete accumulation (compaction candidates)
```sql
SELECT
    partition,
    record_count,
    position_delete_record_count,
    equality_delete_record_count,
    ROUND(100.0 * (position_delete_record_count + equality_delete_record_count)
          / NULLIF(record_count, 0), 2) AS delete_pct
FROM catalog.db.table.partitions
WHERE (position_delete_record_count + equality_delete_record_count) > 0
ORDER BY delete_pct DESC;
```

### Partition evolution across snapshots
```sql
SELECT
    e.snapshot_id,
    f.partition,
    COUNT(*) AS files_added,
    SUM(f.file_size_in_bytes) AS bytes_added
FROM catalog.db.table.entries e
JOIN catalog.db.table.files f ON e.data_file.file_path = f.file_path
WHERE e.status = 1
GROUP BY e.snapshot_id, f.partition
ORDER BY e.snapshot_id;
```

---

## Branch & Tag Operations

### List all refs with their snapshot timestamps
```sql
SELECT
    r.name,
    r.type,
    r.snapshot_id,
    s.committed_at,
    r.min_snapshots_to_keep,
    r.max_snapshot_age_in_ms
FROM catalog.db.table.refs r
JOIN catalog.db.table.snapshots s USING (snapshot_id)
ORDER BY r.type, r.name;
```

### Refs at risk of snapshot expiry (both retention constraints set)
```sql
SELECT name, type, snapshot_id, min_snapshots_to_keep, max_snapshot_age_in_ms
FROM catalog.db.table.refs
WHERE min_snapshots_to_keep IS NOT NULL
  AND max_snapshot_age_in_ms IS NOT NULL;
```

### Storage size per branch (current snapshot of each branch)
```sql
SELECT
    r.name     AS branch_name,
    e.snapshot_id,
    COUNT(*)   AS file_count,
    SUM(f.file_size_in_bytes) AS total_bytes
FROM catalog.db.table.refs r
JOIN catalog.db.table.all_entries e ON r.snapshot_id = e.snapshot_id
JOIN catalog.db.table.files f ON e.data_file.file_path = f.file_path
WHERE r.type = 'BRANCH'
GROUP BY r.name, e.snapshot_id
ORDER BY r.name;
```

### Files in a specific branch
```sql
SELECT r.name AS branch_name, f.*
FROM catalog.db.table.refs r
JOIN catalog.db.table.all_entries e ON r.snapshot_id = e.snapshot_id
JOIN catalog.db.table.files f ON e.data_file.file_path = f.file_path
WHERE r.type = 'BRANCH'
  AND r.name = 'your-branch-name';
```

### Files unique to branch1 (not in branch2) — diff branches
```sql
SELECT 'branch1' AS branch, f.*
FROM catalog.db.table.refs r1
JOIN catalog.db.table.all_entries e1 ON r1.snapshot_id = e1.snapshot_id
JOIN catalog.db.table.files f ON e1.data_file.file_path = f.file_path
WHERE r1.type = 'BRANCH' AND r1.name = 'branch1'
  AND f.file_path NOT IN (
    SELECT f2.file_path
    FROM catalog.db.table.refs r2
    JOIN catalog.db.table.all_entries e2 ON r2.snapshot_id = e2.snapshot_id
    JOIN catalog.db.table.files f2 ON e2.data_file.file_path = f2.file_path
    WHERE r2.type = 'BRANCH' AND r2.name = 'branch2'
  )
UNION ALL
SELECT 'branch2' AS branch, f.*
FROM catalog.db.table.refs r1
JOIN catalog.db.table.all_entries e1 ON r1.snapshot_id = e1.snapshot_id
JOIN catalog.db.table.files f ON e1.data_file.file_path = f.file_path
WHERE r1.type = 'BRANCH' AND r1.name = 'branch2'
  AND f.file_path NOT IN (
    SELECT f2.file_path
    FROM catalog.db.table.refs r2
    JOIN catalog.db.table.all_entries e2 ON r2.snapshot_id = e2.snapshot_id
    JOIN catalog.db.table.files f2 ON e2.data_file.file_path = f2.file_path
    WHERE r2.type = 'BRANCH' AND r2.name = 'branch1'
  );
```

---

## Cross-Table Joins

### Full file lifecycle with manifest context
```sql
SELECT
    e.snapshot_id,
    e.sequence_number,
    e.status,
    m.added_snapshot_id,
    m.added_data_files_count,
    m.deleted_data_files_count
FROM catalog.db.table.all_entries e
JOIN catalog.db.table.all_manifests m ON e.snapshot_id = m.added_snapshot_id
WHERE e.data_file.file_path = 's3://your-bucket/path/to/file.parquet'
ORDER BY e.sequence_number;
```

### Snapshot summary enriched with file stats
```sql
SELECT
    s.snapshot_id,
    s.committed_at,
    s.operation,
    s.summary['added-records']  AS added_records,
    s.summary['total-records']  AS total_records,
    COUNT(e.data_file.file_path) FILTER (WHERE e.status = 1) AS files_added,
    COUNT(e.data_file.file_path) FILTER (WHERE e.status = 2) AS files_deleted
FROM catalog.db.table.snapshots s
LEFT JOIN catalog.db.table.all_entries e USING (snapshot_id)
GROUP BY s.snapshot_id, s.committed_at, s.operation, s.summary
ORDER BY s.committed_at;
```

---

## Delete File Monitoring

### Current delete file accumulation
```sql
SELECT
    content,  -- 1=position deletes, 2=equality deletes
    COUNT(*)                    AS delete_file_count,
    SUM(file_size_in_bytes)     AS total_bytes,
    SUM(record_count)           AS total_delete_records
FROM catalog.db.table.files
WHERE content IN (1, 2)
GROUP BY content;
```

### Delete files per partition (compaction prioritization)
```sql
SELECT
    partition,
    content,
    COUNT(*)            AS delete_file_count,
    SUM(record_count)   AS delete_record_count
FROM catalog.db.table.files
WHERE content IN (1, 2)
GROUP BY partition, content
ORDER BY delete_record_count DESC;
```