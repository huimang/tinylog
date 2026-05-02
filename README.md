# TinyLog

<p align="center">
  <img src="assets/tinylog-logo.svg" alt="TinyLog logo" width="320" />
</p>

[Chinese README](README.zh-CN.md)

`TinyLog` is a project scaffold for **high-density log storage** and **low-memory log access**.
It targets the two main pain points of traditional plaintext logging: excessive storage cost and expensive traversal of very large files.

## Vision

Traditional logs are usually stored as plaintext. That creates two systemic issues:

1. **Storage overhead**: plaintext logs contain high redundancy and grow quickly over time.
2. **Read amplification**: once files become large, scanning, browsing, filtering, and locating entries can consume too much memory.

The project is initialized around two product surfaces:

1. **Java SDK** for application integration, with business-facing logging APIs similar in role to `slf4j`.
2. **Rust viewer** for converting plaintext logs into proprietary TinyLog files, then opening and navigating them with a `vim`-like workflow for browsing, searching, and positioning.

## Modules

| Module | Responsibility |
| --- | --- |
| `tinylog-core` | Core log domain model, codec abstractions, and reader/writer contracts |
| `tinylog-sdk` | Business-facing Java logging API, logger factories, and SLF4J 2.0.17 bridge support |
| `tinylog-viewer` | Rust CLI scaffold for converting plaintext logs to `.tog` and browsing proprietary TinyLog files |

## Collaboration Guidelines

Repository collaboration rules, engineering conventions, and commit conventions live in [`AGENTS.md`](AGENTS.md).

## Current Technical Direction

- **Java namespace**: `com.huimang.tinylog`
- **Java build**: Maven multi-module project for `tinylog-core` and `tinylog-sdk`
- **Java SDK compatibility**: `slf4j-api:2.0.17` with verified `slf4j-simple:2.0.17` integration
- **Rust viewer**: standalone Cargo project under `tinylog-viewer`, responsible for both `.tog` conversion and interactive browsing
- **Storage redesign draft (EN)**: `docs/log-storage-structure.md`
- **Storage redesign draft (ZH-CN)**: `docs/zh-CN/log-storage-structure.md`

## Prototype File Format

The current prototype uses a **trunk-based** `.tog` binary layout with whole-trunk compression, lightweight indexing, and low-memory windowed reads for interactive browsing.

The full storage structure, field definitions, and design rationale live in:

- English: [`docs/log-storage-structure.md`](docs/log-storage-structure.md)
- Chinese: [`docs/zh-CN/log-storage-structure.md`](docs/zh-CN/log-storage-structure.md)

## Manual Prototype Testing

The current prototype accepts plaintext log lines in this format:

```text
<yyyy-MM-dd HH:mm:ss,SSS> [LEVEL] <message>
```

The converter interprets that timestamp text as a **UTC calendar value**, extracts the first `[LEVEL]` token into a dedicated one-byte field, removes that token from the stored message body, and the viewer reconstructs the line using the persisted level plus the UTC timestamp offset.

### 1. Create a sample `normal.log`

```bash
cat > normal.log <<'EOF'
2026-05-01 22:01:00,253 [INFO] service started
2026-05-01 22:01:00,278 [WARN] user signed in
2026-05-01 22:01:00,353 [ERROR] order created
EOF
```

### 2. Convert `normal.log` to `normal.tog`

```bash
cargo run --quiet --manifest-path tinylog-viewer/Cargo.toml -- convert normal.log normal.tog
```

Helper script:

```bash
scripts/tinylog-convert.sh normal.log
```

Expected output:

```text
counting total lines in normal.log
progress: 0/3 (0.00%)
progress: 3/3 (100.00%)
converted normal.log to normal.tog using gzip
source size: 120 (120 B)
output size: 111 (111 B)
compression ratio: 92.50%
elapsed: 4ms
```

### 3. Read `normal.tog` with the Rust viewer

```bash
cargo run --quiet --manifest-path tinylog-viewer/Cargo.toml -- normal.tog
```

Helper scripts:

```bash
scripts/tinylog-view.sh normal.tog
scripts/tinylog-open.sh normal.log
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
:N              jump to line N
/keyword        search keyword by trunk and jump to the nearest result
n               move to next cached search result
p               move to previous cached search result
Esc             clear active search and remove highlight
q               quit
```

Expected screen content:

```text
TinyLog viewer | file=normal.tog | records=3 | line=1 | j/k move  enter +1/4  d/u page  g/G ends  q quit
     1 ▪2026-05-01 22:01:00,253 [INFO] service started
     2  2026-05-01 22:01:00,278 [WARN] user signed in
     3  2026-05-01 22:01:00,353 [ERROR] order created
```

The viewer stays open like a lightweight vim-style browser. The display area is rendered as two independent panes: a blue left logical line-number pane and a right content pane, with a pale-orange rectangular marker offset slightly to the right of the line numbers for the focused row. The marker is shown only on the first physical row of the focused logical log entry. One logical log line can span multiple rendered rows because of width limits or embedded newlines, but it still keeps a single sequence number in the left pane. The focused line moves freely inside the viewport and the screen scrolls only when another move would push that focused row past the top or bottom edge.

### 4. Re-run the automated converter test only

```bash
cd tinylog-viewer && cargo test converter::tests::convert_plaintext_log_writes_parseable_tog
```

## Near-Term Roadmap

1. Define the TinyLog file header, block layout, and index structure
2. Implement streaming writer/reader paths and compression codecs
3. Add a default Java SDK implementation behind the abstract logging API
4. Add paging, search, and jump workflows to the Rust viewer

## License

This project is licensed under the [MIT License](LICENSE).
