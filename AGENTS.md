# TinyLog Agent Guidelines

This document defines the collaboration rules for people and automation working in this repository.

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
- `tinylog-rust-common` keeps shared Rust format and contract code isolated
- `tinylog-converter` owns plaintext-to-TinyLog conversion
- `tinylog-viewer` owns interactive browsing
- Cross-module dependencies should remain explicit and minimal

### 3. Code should be commented by default

Code should include comments or doc comments unless the intent is completely obvious.

- Explain business meaning and boundary decisions
- Keep comments concise and durable
- Prefer API-level comments for public types and methods

### 4. Documentation is language-separated

- The root README should stay English-first
- Chinese project-facing documentation should live in standalone Chinese files
- API names and code symbols should remain stable and language-neutral

## Commit Conventions

### 1. Commit metadata

- **Author** should be the actual human contributor who owns the change
- **Committer** should be set by the AI according to its own origin and identity source
- When the AI has a known upstream identity, it should use the matching committer name and email for that source
- For GitHub Copilot-based work, use `Copilot Committer <copilot-committer@github.com>`
- **Commit messages** must describe the change itself and **must not mention any AI, model, tool, or agent identity**

### 2. Commit granularity

- Every relatively complete, stable feature should be committed immediately
- If several commits were created while iterating on the same feature or fix, they should be squashed into one clean feature-level commit before continuing
- One commit should cover one coherent feature boundary; do not mix unrelated features, refactors, examples, and tooling changes into the same commit unless they are inseparable parts of the same behavior change

Example commit message style:

```text
viewer: initialize rust cli scaffold
core: add log query abstraction
sdk: introduce business-facing logger factory
```

### 3. Recommended workflow

1. Finish one coherent feature end-to-end
2. Confirm it is in a stable state
3. Create exactly one commit for that feature
4. If multiple intermediate commits exist, recombine them into one clean commit before continuing
