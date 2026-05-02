# tinylog

[中文版说明](README.zh-CN.md)

`tinylog` is a project scaffold for **high-density log storage** and **low-memory log access**.
It targets the two main pain points of traditional plaintext logging: excessive storage cost and expensive traversal of very large files.

## Vision

Traditional logs are usually stored as plaintext. That creates two systemic issues:

1. **Storage overhead**: plaintext logs contain high redundancy and grow quickly over time.
2. **Read amplification**: once files become large, scanning, browsing, filtering, and locating entries can consume too much memory.

The project is initialized around two product surfaces:

1. **Java SDK** for application integration, with business-facing logging APIs similar in role to `slf4j`.
2. **Rust viewer** for opening and navigating proprietary tinylog files with a `vim`-like workflow for browsing, searching, and positioning.

## Modules

| Module | Responsibility |
| --- | --- |
| `tinylog-core` | Core log domain model, codec abstractions, and reader/writer contracts |
| `tinylog-sdk` | Business-facing Java logging API, logger factories, and SLF4J 2.0.17 bridge support |
| `tinylog-viewer` | Rust CLI scaffold for browsing proprietary tinylog files |

## Engineering Guidelines

### 1. Interface design is business-first

Public interfaces should be named and shaped around **business capabilities**, not around storage engines, codecs, buffers, or transport details.

- Prefer terms such as `log`, `browse`, `search`, `jump`, `record`, and `query`
- Avoid leaking implementation detail into business-facing APIs
- Keep interfaces abstract enough to support multiple backends and file formats

### 2. Modules must be independent and self-contained

Each module should have a clear boundary, a coherent responsibility, and minimal cross-module assumptions.

- `tinylog-core` defines the shared contracts
- `tinylog-sdk` focuses on application integration
- `tinylog-viewer` evolves independently as a dedicated client
- Cross-module dependencies should remain explicit and minimal

### 3. Code should be commented by default

Code should include comments or doc comments unless the intent is completely obvious.

- Explain business meaning and boundary decisions
- Keep comments concise and durable
- Prefer API-level comments for public types and methods

### 4. Documentation is language-separated

- The root README should stay English-only
- Chinese project-facing documentation should live in standalone Chinese files
- API names and code symbols should remain stable and language-neutral

### 5. Commit metadata conventions

- **Author** should be the repository owner
- **Committer** can be a dedicated AI identity configured locally
- **Commit messages** must describe the change itself and **must not mention any AI, model, tool, or agent identity**
- Every relatively complete, stable feature should be committed immediately
- If several commits were created while iterating on the same feature or fix, they should be squashed into one clean feature-level commit before continuing

Example commit message style:

```text
viewer: initialize rust cli scaffold
core: add log query abstraction
sdk: introduce business-facing logger factory
```

Recommended workflow:

1. Finish one coherent feature end-to-end
2. Confirm it is in a stable state
3. Create exactly one commit for that feature
4. If multiple intermediate commits exist, reset back to the feature start and recombine them into one clean commit

## Current Technical Direction

- **Java namespace**: `com.huimang.tinylog`
- **Java build**: Maven multi-module project for `tinylog-core` and `tinylog-sdk`
- **Java SDK compatibility**: `slf4j-api:2.0.17` with verified `slf4j-simple:2.0.17` integration
- **Rust viewer**: standalone Cargo project under `tinylog-viewer`
- **Storage redesign draft (EN)**: `docs/log-storage-structure.md`
- **Storage redesign draft (ZH-CN)**: `docs/zh-CN/log-storage-structure.md`

## Prototype File Format

The current prototype uses a **trunk-based** binary layout in **big-endian** order.

1. **Version**: 3 bytes, sourced from the Maven version tuple
2. **Compression algorithm**: 2 bytes
3. **Trunk size**: 2 bytes, stored in KB
4. **Base timestamp**: 8 bytes, UTC milliseconds
5. **Total log line count**: 8 bytes
6. **Trunk count**: 3 bytes
7. **File extension**: `.tog`
8. Repeated for each completed trunk:
   - **Trunk log line count**: 2 bytes
   - **Compressed trunk length**: 4 bytes
   - **Compressed trunk bytes**: the full raw trunk payload after whole-trunk compression

In other words:

```text
[version:3][compression:2][trunkSizeKb:2][baseTimestampUtcMillis:8][totalLogLineCount:8][trunkCount:3]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
...
```

Current prototype notes:

- The Java writer buffers raw lines into `log-buffer-{trunkIndex}.tmp` files
- Once the buffer reaches the configured trunk size, the whole trunk is compressed and appended to the main `.tog`
- Each raw line inside a decompressed trunk uses `[offsetMillis:4][contentLength:4][content:N]`
- The Rust viewer reads the same binary format directly and only decompresses the trunk(s) needed for the current visible window
- The complete storage design is documented in `docs/log-storage-structure.md` and `docs/zh-CN/log-storage-structure.md`

Compression algorithm IDs:

| ID | Algorithm |
| --- | --- |
| `0` | none |
| `1` | gzip |
| `2` | zip |
| `3` | deflate |
| `4` | bzip2 |
| `5` | xz |
| `6` | zstd |
| `7` | snappy |

## Manual Prototype Testing

The current prototype accepts plaintext log lines in this format:

```text
<yyyy-MM-dd HH:mm:ss,SSS> <message>
```

The converter interprets that timestamp text as a **UTC calendar value**, and the viewer renders the reconstructed timestamp in UTC as well. That keeps the displayed text stable because every raw line only stores its millisecond offset from the file-level UTC base timestamp.

### 1. Create a sample `normal.log`

```bash
cat > normal.log <<'EOF'
2026-05-01 22:01:00,253 service started
2026-05-01 22:01:00,278 user signed in
2026-05-01 22:01:00,353 order created
EOF
```

### 2. Convert `normal.log` to `normal.tog`

```bash
mvn -q -pl tinylog-core -am package
java -jar tinylog-core/target/tinylog-core-0.1.0-SNAPSHOT-all.jar normal.log normal.tog
```

Expected output:

```text
converted normal.log to normal.tog using gzip
```

### 3. Read `normal.tog` with the Rust viewer

```bash
cargo run --quiet --manifest-path tinylog-viewer/Cargo.toml -- normal.tog
```

Key bindings:

```text
j / DownArrow   move down
k / UpArrow     move up
Enter           move down by 1/4 page
d / PageDown    page down
u / PageUp      page up
g               jump to top
G               jump to bottom
q               quit
```

Expected screen content:

```text
tinylog viewer | file=normal.tog | records=3 | line=1 | j/k move  enter +1/4  d/u page  g/G ends  q quit
     1> 2026-05-01 22:01:00,253 service started
     2  2026-05-01 22:01:00,278 user signed in
     3  2026-05-01 22:01:00,353 order created
```

The viewer stays open like a lightweight vim-style browser. The display area is rendered as two independent panes: a blue left logical line-number pane and a right content pane, with a pale-orange one-character marker slot beside the line numbers for the focused row. One logical log line can span multiple rendered rows because of width limits or embedded newlines, but it still keeps a single sequence number in the left pane. The focused line moves freely inside the viewport and the screen scrolls only when another move would push that focused row past the top or bottom edge.

### 4. Re-run the automated converter test only

```bash
mvn -q -pl tinylog-core -Dtest=PlainTextLogToTinylogConverterTest test
```

## Near-Term Roadmap

1. Define the tinylog file header, block layout, and index structure
2. Implement streaming writer/reader paths and compression codecs
3. Add a default Java SDK implementation behind the abstract logging API
4. Add paging, search, and jump workflows to the Rust viewer
