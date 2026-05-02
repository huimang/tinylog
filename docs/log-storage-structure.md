# Tinylog Log Storage Structure and Trunk Workflow

> Status: **implemented prototype**
>
> This document describes the current trunk-based `.tog` prototype used by the Java writer and Rust viewer.

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
- **Write path**: write raw lines into `log-buffer-{trunkIndex}.tmp`, compress the whole trunk when the buffer reaches the configured threshold, then append the compressed trunk to the main `.tog` file

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

Or expanded:

```text
[version:3][compressionAlgorithm:2][trunkSizeKb:2][baseTimestampUtcMillis:8][totalLogLineCount:8][trunkCount:3]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
...
```

## Write Workflow

The write workflow uses a temporary raw buffer file for each trunk.

### Buffer File Naming

```text
log-buffer-{trunkIndex}.tmp
```

Examples:

```text
log-buffer-0.tmp
log-buffer-1.tmp
log-buffer-2.tmp
```

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
+-------------------------+
| Open log-buffer-0.tmp   |
+-------------------------+
            |
            v
+----------------------------------------------+
| Append raw line                              |
| offsetMillis + level + contentLength + content |
+----------------------------------------------+
            |
            v
+-------------------------------+
| buffer size >= trunkSize ?    |
+-------------------------------+
      | yes                          | no
      v                              |
+-------------------------+          |
| Read raw buffer bytes   |          |
+-------------------------+          |
            |                       |
            v                       |
+----------------------------------+ |
| Compress whole trunk with gzip   | |
+----------------------------------+ |
            |                       |
            v                       |
+------------------------------------------------------+
| Append trunkLogLineCount + compressedContentLength + payload |
+------------------------------------------------------+
            |
            v
+-----------------------------------+
| Update totalLogLineCount / trunkCount |
+-----------------------------------+
            |
            v
+-------------------------------+
| Delete current buffer file    |
+-------------------------------+
            |
            v
+--------------------------------------+
| Open next log-buffer-{trunkIndex}.tmp|
+--------------------------------------+
            |
            +---------------------------> back to append raw line
```

### Write Steps

1. Create the main `.tog` file and initialize the header
2. Create `log-buffer-0.tmp`
3. For each incoming log line:
   1. parse the plaintext timestamp
   2. compute `offsetMillis = lineTimestampUtcMillis - baseTimestampUtcMillis`
   3. append `[offsetMillis:4][level:1][contentLength:4][content]` to the current buffer file
4. When the buffer file reaches the configured `trunkSizeKb` threshold:
   1. read the entire raw buffer
   2. compress the whole buffer using the selected header algorithm
   3. append one trunk to the main `.tog`
   4. update `totalLogLineCount`
   5. update `trunkCount`
   6. remove the old buffer file
   7. start the next buffer file
5. When writing ends, flush the final non-empty buffer as the last trunk

## Read and Browse Workflow

The reader/viewer works against the header plus the appended trunk sequence.

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
2. Java writer/reader and the Rust converter/viewer tools must switch together
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
6. Buffer files use `log-buffer-{trunkIndex}.tmp`
