# Protobuf Java Generation Notes

## Why this document exists

The shared prototype contract now lives in:

`tinylog-core/src/main/proto/tinylog/prototype.proto`

The Java generation path was more fragile than expected, so this document records the decisions and pitfalls to avoid rediscovering them later.

## Final approach

TinyLog keeps the `.proto` file as the source of truth, but the generated Java protobuf sources are committed into the repository:

- schema: `tinylog-core/src/main/proto/tinylog/prototype.proto`
- generated Java: `tinylog-core/src/main/java/com/huimang/tinylog/proto/`

This avoids making normal Maven builds depend on downloading or locating `protoc` correctly on every machine.

## What went wrong

### 1. Maven-time code generation was unreliable

Using a Maven plugin to generate protobuf Java sources during the build added avoidable complexity:

- plugin and tool download behavior was slow and inconsistent
- local environments could stall before producing actionable output
- the build became sensitive to external tool availability instead of just compiling checked-in code

Because of that, the repository now prefers committed generated Java sources over mandatory build-time generation.

### 2. `grpc_tools.protoc` was not enough for Java

Using:

```bash
python3 -m grpc_tools.protoc --java_out=...
```

did not work because the Java codegen plugin was not available in that path.

### 3. `protoc-jar` needed explicit standard protobuf includes

`protoc-jar` was the workable path for Java generation, but once the schema started using wrapper types like:

- `google.protobuf.UInt64Value`
- `google.protobuf.Int32Value`
- `google.protobuf.StringValue`

the command also needed the standard protobuf include path. Without that, `wrappers.proto` could not be found.

## Current schema rule

For optional scalar query fields, use protobuf wrapper types instead of relying on a more fragile proto3 optional setup:

```proto
import "google/protobuf/wrappers.proto";

message PrototypeLogQuery {
  google.protobuf.UInt64Value start_timestamp_millis = 1;
  google.protobuf.UInt64Value end_timestamp_millis = 2;
  google.protobuf.Int32Value minimum_level = 3;
  google.protobuf.StringValue keyword = 4;
}
```

This keeps Java and Rust behavior aligned while preserving field presence.

## Regeneration workflow

If `prototype.proto` changes, regenerate Java sources with `protoc-jar` and the protobuf include path:

```bash
cd /path/to/tinylog

PROTO_INCLUDE=$(python3 - <<'PY'
import grpc_tools, os
print(os.path.join(os.path.dirname(grpc_tools.__file__), '_proto'))
PY
)

java -jar /path/to/protoc-jar-3.11.4.jar \
  --java_out=tinylog-core/src/main/java \
  -I tinylog-core/src/main/proto \
  -I "$PROTO_INCLUDE" \
  tinylog-core/src/main/proto/tinylog/prototype.proto
```

## Maintenance rule

When the shared protobuf contract changes:

1. Update `prototype.proto`
2. Regenerate the Java protobuf sources
3. Keep Rust `prost` generation working through `tinylog-rust-common/build.rs`
4. Re-run both Rust and Maven tests
