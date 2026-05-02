# Protobuf 到 Java 源码生成说明

## 为什么要记录

当前共享原型契约位于：

`tinylog-core/src/main/proto/tinylog/prototype.proto`

这次从 `.proto` 生成 Java 源码的过程比预期更曲折，因此把最终做法和踩坑点记录下来，避免后面重复排查。

## 最终做法

TinyLog 仍然以 `.proto` 作为共享契约的唯一来源，但 **Java 生成结果直接提交到仓库**：

- schema：`tinylog-core/src/main/proto/tinylog/prototype.proto`
- Java 生成源码：`tinylog-core/src/main/java/com/huimang/tinylog/proto/`

这样做的原因是：普通 Maven 构建不需要在每台机器上都重新解决 `protoc`、插件下载和环境差异问题。

## 这次遇到的问题

### 1. Maven 构建时动态生成不够稳定

如果把 protobuf Java 生成绑定到 Maven 生命周期，会带来这些问题：

- 插件和工具下载慢，而且行为不稳定
- 某些环境会长时间停住，拿不到明确错误
- 正常构建会依赖外部工具是否可用，而不是只依赖仓库里已有源码

因此目前仓库采用“**保留 proto + 提交生成后的 Java 源码**”的方式。

### 2. `grpc_tools.protoc` 不能直接完成 Java 生成

直接执行：

```bash
python3 -m grpc_tools.protoc --java_out=...
```

并不能稳定完成 Java 生成，因为对应的 Java codegen plugin 不在这个路径里。

### 3. `protoc-jar` 还需要显式传入标准 protobuf include

后面改用 `protoc-jar` 可以生成 Java 源码，但当 schema 使用这些 wrapper types 时：

- `google.protobuf.UInt64Value`
- `google.protobuf.Int32Value`
- `google.protobuf.StringValue`

还必须显式提供标准 protobuf include 路径，否则会报 `wrappers.proto` 找不到。

## 当前 schema 规则

对于需要保留“字段是否出现”语义的标量查询字段，优先使用 wrapper types，而不是依赖更脆弱的 proto3 optional 组合。

```proto
import "google/protobuf/wrappers.proto";

message PrototypeLogQuery {
  google.protobuf.UInt64Value start_timestamp_millis = 1;
  google.protobuf.UInt64Value end_timestamp_millis = 2;
  google.protobuf.Int32Value minimum_level = 3;
  google.protobuf.StringValue keyword = 4;
}
```

这样 Java 和 Rust 两边都更容易对齐，同时还能保留 presence 语义。

## 后续重新生成方式

如果 `prototype.proto` 发生变化，使用 `protoc-jar` 重新生成 Java 源码：

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

## 维护约束

共享 protobuf 契约发生变化时，按下面顺序处理：

1. 更新 `prototype.proto`
2. 重新生成 Java protobuf 源码
3. 保证 Rust 侧 `tinylog-rust-common/build.rs` 的 `prost` 生成仍然正常
4. 重新执行 Rust 和 Maven 测试
