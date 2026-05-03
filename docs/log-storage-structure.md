# Tinylog Log Storage Structure and Trunk Workflow

> Status: **implemented prototype**
>
> This document describes the current trunk-based `.tog` prototype used by the Java writer, Rust converter, and Rust viewer.

## Purpose

This redesign replaces **per-line compression** with **trunk-based compression**.

Goals:

1. Keep write-time behavior simple and append-friendly
2. Reuse repeated text across many lines inside the same compressed unit
3. Preserve predictable browsing and partial decoding behavior
4. Make the on-disk structure explicit enough for both Java writing and Rust viewing

## Design Summary

- **Default compression algorithm**: `gzip`
- **Storage unit**: `trunk`
- **Default trunk size**: `512 KB`
- **Base timestamp**: one file-level UTC timestamp in the header
- **Write path**: the Java writer appends into an in-memory trunk buffer and mirrors the not-yet-merged raw records into a `.buffer` sidecar file, while the Rust converter builds trunk byte ranges directly from the source log, lets workers compress contiguous trunk batches in parallel for large inputs, and merges the worker outputs into the final `.tog` file

## File Header

The main `.tog` file begins with a fixed-size header in **big-endian** order.

| Field | Size | Default Value | Meaning |
| --- | ---: | --- | --- |
| `versionMajor` | 1 byte |  | First numeric segment of the Maven version |
| `versionMinor` | 1 byte |  | Second numeric segment of the Maven version |
| `versionPatch` | 1 byte |  | Third numeric segment of the Maven version |
| `compressionAlgorithm` | 2 bytes | 1 | Header-level compression algorithm ID |
| `trunkSizeKb` | 2 bytes | 512 | Trunk size in KB |
| `baseTimestampUtcMillis` | 8 bytes |  | File-level base timestamp in UTC milliseconds |
| `totalLogLineCount` | 8 bytes | 0 | Total number of log lines already persisted into trunks |
| `trunkCount` | 3 bytes | 0 | Total number of completed trunks already appended |
### Header Layout

```text
[version:3]
[compressionAlgorithm:2]
[trunkSizeKb:2]
[baseTimestampUtcMillis:8]
[totalLogLineCount:8]
[trunkCount:3]
```

### Header Notes

1. `version` comes from the Maven project version, for example:
   - `0.1.0-SNAPSHOT` -> `[0, 1, 0]`
   - qualifiers such as `-SNAPSHOT` are ignored
2. `trunkSizeKb` is stored as an unsigned 16-bit value in KB
3. The intended upper bound is **64 MB**, and the field should be validated accordingly during implementation
4. `baseTimestampUtcMillis` is the single reference point for all line-level `offsetMillis` values
5. `totalLogLineCount` and `trunkCount` are updated after each successful trunk flush

## Compression Algorithm IDs

The existing algorithm ID space stays available, but the default changes back to `gzip`.

| ID | Algorithm | Default |
| --- | --- | --- |
| `0` | none | no |
| `1` | gzip | **yes** |
| `2` | zip | no |
| `3` | deflate | no |
| `4` | bzip2 | no |
| `5` | xz | no |
| `6` | zstd | no |
| `7` | snappy | no |

## Trunk Format

Each completed trunk is appended to the main `.tog` file using this structure:

| Field | Size | Meaning |
| --- | ---: | --- |
| `trunkLogLineCount` | 2 bytes | Number of lines in this trunk |
| `compressedContentLength` | 4 bytes | Number of bytes in the compressed trunk payload |
| `compressedContent` | N bytes | Compressed raw trunk payload |
### Trunk Layout

```text
[trunkLogLineCount:2]
[compressedContentLength:4]
[compressedContent:N]
```

## Raw Line Format Inside a Trunk

Before compression, a trunk contains raw log lines in sequence:

| Field | Size | Meaning |
| --- | ---: | --- |
| `offsetMillis` | 4 bytes | Millisecond offset from `baseTimestampUtcMillis` |
| `level` | 1 byte | Persisted log level identifier |
| `contentLength` | 4 bytes | Length of the log content in bytes |
| `content` | N bytes | UTF-8 log content, not compressed at line level |

### Raw Line Layout

```text
[offsetMillis:4][level:1][contentLength:4][content:N]
[offsetMillis:4][level:1][contentLength:4][content:N]
[offsetMillis:4][level:1][contentLength:4][content:N]
...
```

### Raw Line Notes

1. `content` stores the log text after the leading timestamp has been removed
2. Timestamp reconstruction is always:

   ```text
   actualTimestampUtcMillis = baseTimestampUtcMillis + offsetMillis
   ```

3. `offsetMillis` is stored in 4 bytes, so one file can represent about `2^32` milliseconds from the base timestamp
4. `contentLength` is 4 bytes because the user explicitly chose explicit per-line length over newline termination

## Full File Structure

```text
[header]
[trunk-0]
[trunk-1]
[trunk-2]
...
```

When the Java writer has raw records that have not been merged into a completed trunk yet, it also keeps a sidecar buffer file next to the main file:

```text
app.tog
app.tog.buffer
```

The sidecar begins with the same `baseTimestampUtcMillis` and then stores raw lines in the same `[offsetMillis][level][contentLength][content]` layout used inside a trunk.

Or expanded:

```text
[version:3][compressionAlgorithm:2][trunkSizeKb:2][baseTimestampUtcMillis:8][totalLogLineCount:8][trunkCount:3]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
...
```

## Write Workflow

The current write workflow is implemented by the Rust converter. Small inputs stay serial, while larger inputs switch to a master/worker conversion pipeline.

### Write Flow Diagram

```text
+-------------------------+
| Create main .tog file   |
+-------------------------+
            |
            v
+-------------------------+
| Write fixed header      |
+-------------------------+
            |
            v
+----------------------------------------------+
| For inputs > 100 MiB, build trunk boundaries |
| by jumping near trunkSize and aligning to    |
| the next record-start marker                 |
+----------------------------------------------+
            |
            v
+----------------------------------------------+
| Assign contiguous trunk ranges to workers    |
+----------------------------------------------+
            |
            v
+----------------------------------------------+
| Worker reads one planned trunk range         |
| parses records / multiline continuations     |
| writes offsetMillis + level + contentLength  |
| and compresses the whole trunk               |
+----------------------------------------------+
            |
            v
+------------------------------------------------------+
| Worker emits trunkLogLineCount + compressed payload  |
| plus trunk metadata back to the master               |
+------------------------------------------------------+
            |
            v
+-----------------------------------+
| Master merges worker outputs      |
| and finalizes header counters     |
+-----------------------------------+
```

### Write Steps

1. Create the main `.tog` file and initialize the header
2. For the Java writer, create or reset the `.buffer` sidecar file for not-yet-merged raw records
3. For inputs up to `100 MiB`, parse records serially and flush trunks directly from the converter process
4. For larger inputs:
   1. jump forward by the configured trunk size in bytes
   2. align each boundary to the next record-start marker (`newline + timestamp-like prefix`)
   3. use the resulting byte ranges as planned trunks
   4. assign consecutive trunk ranges to workers
5. Each worker reads its planned source bytes, parses the first timestamped line as a new record, and appends any following non-timestamp lines to that record as multiline continuation content
6. Each worker encodes `[offsetMillis:4][level:1][contentLength:4][content]`, compresses the whole trunk, and reports record counts plus timestamp metadata back to the master
7. The Java writer clears the `.buffer` sidecar whenever the in-memory trunk is successfully merged into the main `.tog` file, including normal close and log rotation
8. The master merges worker outputs in order and writes the final `totalLogLineCount` and `trunkCount` values into the header

## Read and Browse Workflow

The reader/viewer works against the header plus the appended trunk sequence. When a Java `.buffer` sidecar is present and still contains raw records, the reader appends those buffered records after the persisted trunks so graceful-close and crash-recovery reads stay consistent.

### Read Flow Diagram

```text
+-------------------+
| Open .tog file    |
+-------------------+
          |
          v
+-------------------+
| Read header       |
+-------------------+
          |
          v
+-------------------------+
| Find target trunk range |
+-------------------------+
          |
          v
+-------------------------+
| Read trunk metadata     |
+-------------------------+
          |
          v
+----------------------------+
| Read compressed payload    |
+----------------------------+
          |
          v
+----------------------------+
| Decompress needed trunk    |
+----------------------------+
          |
          v
+----------------------------+
| Parse raw lines in trunk   |
+----------------------------+
          |
          v
+------------------------------------------------------+
| Rebuild timestamp = baseTimestampUtcMillis + offset  |
+------------------------------------------------------+
          |
          v
+----------------------------+
| Filter or render lines     |
+----------------------------+
```

### Read Steps

1. Read the fixed header
2. Scan all trunk offsets plus line counts once and cache that index in memory at open time
3. Use the cached index to resolve the visible range or requested scan range
4. Read only the target trunk payloads
5. Decompress only the trunks needed for the current window, search step, or filter step
6. Parse raw lines inside those trunks
7. Reconstruct each timestamp from the file-level base timestamp
8. Return only the requested records to the caller or viewer

## Viewer Expectations

The Rust viewer should continue to behave like a lightweight vim-style browser:

- keep the interactive navigation model
- avoid decoding unrelated trunks
- decode only the trunk or trunk subset needed for the current visible window
- for search and level filtering, continue scanning trunks on demand instead of decoding the whole file up front

This means the redesign changes the **decompression granularity** from **line-level** to **trunk-level**.

## Compatibility Notes

This redesign is **not backward compatible** with the current prototype layout.

Implications:

1. Existing `.tog` files produced by the old format will need reconversion
2. Any writer/reader implementation must switch together; the currently implemented write paths are the Java writer and the Rust converter, and the current browser is the Rust viewer
3. Tests must be updated to cover:
   - version byte parsing
   - trunk flushing
   - header counter updates
   - final partial trunk flushing
   - viewer-side trunk-only decoding

## Current Contract

The current prototype contract is:

1. Header order: `version -> compression -> trunkSizeKb -> baseTimestampUtcMillis -> totalLogLineCount -> trunkCount`
2. One file-level UTC base timestamp is shared by all trunks
3. Each line inside a trunk is `[offsetMillis:4][level:1][contentLength:4][content]`
4. Each trunk is `[trunkLogLineCount:2][compressedContentLength:4][compressedContent]`
5. Default compression is `gzip`
6. Large-input indexing is byte-based, aligns boundaries to record starts, and leaves record counting to workers during compression
