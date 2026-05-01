# Iceberg Metadata Tables

Iceberg exposes table internals as queryable metadata tables. Address them by
appending the table name as a suffix to the table identifier, e.g.
catalog.db.table.snapshots. With iceman, use: iceman inspect db.table snapshots.

## Metadata Tables

| Table                | Scope              | Usage                                 |
|----------------------|--------------------|---------------------------------------|
| manifests            | current snapshot   | Current manifest state                |
| files                | current snapshot   | Active data & delete file stats       |
| partitions           | current snapshot   | Partition-level stats                 |
| refs                 | current            | Branches and tags                     |
| history              | all lineage        | Snapshot ancestry, rollback detection |
| snapshots            | all snapshots      | Snapshot inspection, time travel      |
| all_data_files       | all snapshots      | Cross-snapshot data file tracking     |
| all_delete_files     | all snapshots      | Cross-snapshot delete file tracking   |
| all_entries          | all snapshots      | All file ops (data + deletes)         |
| all_manifests        | all snapshots      | Cross-snapshot manifest tracking      |
| entries              | all snapshots      | Full audit trail (all file ops)       |
| metadata_log_entries | all metadata files | Metadata file evolution               |

Tables without the all_ prefix reflect the current snapshot only. all_*
variants may return multiple rows per file (one per snapshot the file was
valid in).

## Quick Reference by Task

Diagnose snapshot history / rollbacks
  history -- rows where parent_id appears more than once indicate a rollback.

Inspect a specific snapshot's files
  Join entries (filter snapshot_id + status=1) with files on file_path.

Monitor storage growth over time
  all_entries grouped by snapshot_id, sum data_file.file_size_in_bytes where status=1.

Check manifest bloat
  all_manifests grouped by reference_snapshot_id, sum length.

List active branches/tags
  refs -- filter type = 'BRANCH' or type = 'TAG'.

See partition-level record counts
  partitions -- includes record_count and file_count per partition value.

## Status Codes

Used by entries / all_entries.

| Value | Meaning                               |
|-------|---------------------------------------|
| 0     | Existing (unchanged in this snapshot) |
| 1     | Added                                 |
| 2     | Deleted                               |

## Content Type Codes

Used by manifests / all_manifests.

| Value | Meaning             |
|-------|---------------------|
| 0     | Tracks data files   |
| 1     | Tracks delete files |
