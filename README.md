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

1. **Start timestamp**: 8 bytes, milliseconds since epoch
2. **Record count**: 8 bytes
3. Repeated for each record:
   - **Millisecond offset** from the start timestamp: 4 bytes
   - **Content length**: 3 bytes
   - **Content bytes**: UTF-8 payload

In other words:

```text
[startTimestamp:8][recordCount:8]
[offset:4][contentLength:3][content:N]
[offset:4][contentLength:3][content:N]
...
```

Current prototype notes:

- The Java prototype writer stores the rendered log **message** as the payload
- The Java prototype reader rebuilds `LogRecord` instances using the decoded message
- The Rust viewer reads the same binary format directly and prints timestamps plus content
- This prototype focuses on validating the binary framing first, before adding richer structured payloads and indexes

## Near-Term Roadmap / 下一阶段建议

1. Define the tinylog file header, block layout, and index structure
2. Implement streaming writer/reader paths and compression codecs
3. Add a default Java SDK implementation behind the abstract logging API
4. Add paging, search, and jump workflows to the Rust viewer
