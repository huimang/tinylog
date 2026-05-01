# tinylog

`tinylog` is a project scaffold for **high-density log storage** and **low-memory log access**.
It targets the two main pain points of traditional plaintext logging: excessive storage cost and expensive traversal of very large files.

> 中文支持：仓库文档以英文为主，允许补充中文说明；关键约定会尽量保持中英文都容易理解。

## Vision / 项目目标

Traditional logs are usually stored as plaintext. That creates two systemic issues:

1. **Storage overhead**: plaintext logs contain high redundancy and grow quickly over time.
2. **Read amplification**: once files become large, scanning, browsing, filtering, and locating entries can consume too much memory.

The project is initialized around two product surfaces:

1. **Java SDK** for application integration, with business-facing logging APIs similar in role to `slf4j`.
2. **Rust viewer** for opening and navigating proprietary tinylog files with a `vim`-like workflow for browsing, searching, and positioning.

## Modules / 模块划分

| Module | Responsibility |
| --- | --- |
| `tinylog-core` | Core log domain model, codec abstractions, and reader/writer contracts |
| `tinylog-sdk` | Business-facing Java logging API, logger factories, and SLF4J 2.0.17 bridge support |
| `tinylog-viewer` | Rust CLI scaffold for browsing proprietary tinylog files |

## Engineering Guidelines / 工程准则

### 1. Interface design is business-first / 接口设计强调业务语义

Public interfaces should be named and shaped around **business capabilities**, not around storage engines, codecs, buffers, or transport details.

- Prefer terms such as `log`, `browse`, `search`, `jump`, `record`, and `query`
- Avoid leaking implementation detail into business-facing APIs
- Keep interfaces abstract enough to support multiple backends and file formats

### 2. Modules must be independent and self-contained / 模块必须独立自洽

Each module should have a clear boundary, a coherent responsibility, and minimal cross-module assumptions.

- `tinylog-core` defines the shared contracts
- `tinylog-sdk` focuses on application integration
- `tinylog-viewer` evolves independently as a dedicated client
- Cross-module dependencies should remain explicit and minimal

### 3. Code should be commented by default / 原则上代码需要提供注释

Code should include comments or doc comments unless the intent is completely obvious.

- Explain business meaning and boundary decisions
- Keep comments concise and durable
- Prefer API-level comments for public types and methods

### 4. Documentation is English-first, with Chinese support / 文档以英文为主，支持中文

- New project-facing documentation should be primarily written in English
- Chinese can be added for clarification where it improves collaboration
- API names and code symbols should remain stable and language-neutral

### 5. Commit metadata conventions / 提交规范

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

## Current Technical Direction / 当前技术方向

- **Java namespace**: `com.huimang.tinylog`
- **Java build**: Maven multi-module project for `tinylog-core` and `tinylog-sdk`
- **Java SDK compatibility**: `slf4j-api:2.0.17` with verified `slf4j-simple:2.0.17` integration
- **Rust viewer**: standalone Cargo project under `tinylog-viewer`

## Prototype File Format / 当前原型格式

The current prototype uses a compact binary layout in **big-endian** order:

1. **Compression algorithm**: 2 bytes
2. **Start timestamp**: 8 bytes, milliseconds since epoch
3. **Record count**: 8 bytes
4. **File extension**: `.tog`
5. Repeated for each record:
   - **Millisecond offset** from the start timestamp: 4 bytes
   - **Compressed content length**: 3 bytes
   - **Compressed content bytes**: UTF-8 message payload after line-body compression

In other words:

```text
[compression:2][startTimestamp:8][recordCount:8]
[offset:4][compressedLength:3][compressedContent:N]
[offset:4][compressedLength:3][compressedContent:N]
...
```

Current prototype notes:

- The Java prototype writer stores the rendered log **message** as the payload and compresses it per line
- The Java prototype reader rebuilds `LogRecord` instances using the decoded message
- The Java prototype converter can transform `normal.log` plaintext input into a `.tog` file
- The Rust viewer reads the same binary format directly, resolves the header algorithm, and decompresses only the visible records
- This prototype focuses on validating the binary framing first, before adding richer structured payloads and indexes

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

## Manual Prototype Testing / 手动测试命令

The current prototype accepts plaintext log lines in this format:

```text
<yyyy-MM-dd HH:mm:ss,SSS> <message>
```

The converter interprets that local timestamp using the **system default time zone** of the machine running the conversion.

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
mvn -q -pl tinylog-core -am compile
java -cp tinylog-core/target/classes com.huimang.tinylog.core.io.PlainTextLogToTinylogCli normal.log normal.tog
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
d / PageDown    page down
u / PageUp      page up
g               jump to top
G               jump to bottom
q               quit
```

Expected screen content:

```text
tinylog viewer | file=normal.tog | records=3 | line=1 | j/k move  d/u page  g/G ends  q quit

2026-05-01 22:01:00,253 service started
2026-05-01 22:01:00,278 user signed in
2026-05-01 22:01:00,353 order created
```

The viewer stays open like a lightweight vim-style browser. It only reads and decodes the currently visible page of records. Records outside the visible window are left untouched until they are needed.

### 4. Re-run the automated converter test only

```bash
mvn -q -pl tinylog-core -Dtest=PlainTextLogToTinylogConverterTest test
```

## Near-Term Roadmap / 下一阶段建议

1. Define the tinylog file header, block layout, and index structure
2. Implement streaming writer/reader paths and compression codecs
3. Add a default Java SDK implementation behind the abstract logging API
4. Add paging, search, and jump workflows to the Rust viewer
