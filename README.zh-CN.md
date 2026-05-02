# TinyLog

<p align="center">
  <img src="assets/tinylog-logo.svg" alt="TinyLog logo" width="320" />
</p>

<p align="center">
  <img src="assets/tinylog-demo.gif" alt="TinyLog terminal demo" width="900" />
</p>

[English README](README.md)

`TinyLog` 是一个面向**高密度日志存储**和**低内存日志访问**的项目原型。
它主要解决传统明文日志的两个问题：存储空间过大，以及大文件遍历时内存开销过高。

## 项目目标

传统日志通常以明文方式存储，这会带来两个系统性问题：

1. **存储冗余高**：明文日志存在大量重复内容，文件会迅速膨胀
2. **读取放大明显**：日志文件一旦变大，扫描、浏览、过滤、定位都会消耗大量内存

当前项目围绕两个产品面展开：

1. **Java SDK**：用于业务项目集成，提供类似 `slf4j` 角色的业务日志接口
2. **Rust Tools**：用于把明文日志转换成 `.tog` 专有日志文件，并以类似 `vim` 的方式打开、浏览、搜索和定位

## 模块划分

| 模块 | 职责 |
| --- | --- |
| `tinylog-core` | 核心日志领域模型、编解码抽象、读写接口 |
| `tinylog-sdk` | 面向业务的 Java 日志 API、工厂、SLF4J 2.0.17 桥接 |
| `tinylog-rust-common` | Rust 侧共享的 TinyLog 格式与 protobuf 契约支持 |
| `tinylog-converter` | 用于把明文日志转换成 `.tog` 的 Rust CLI |
| `tinylog-viewer` | 用于交互式浏览 TinyLog 文件的 Rust CLI |

## 协作准则

仓库协作规则、工程约束和提交规范统一放在 [`AGENTS.md`](AGENTS.md)。

## 当前技术方向

- **Java 命名空间**：`com.huimang.tinylog`
- **Java 构建**：`tinylog-core` 和 `tinylog-sdk` 的 Maven 多模块工程
- **Java SDK 兼容性**：`slf4j-api:2.0.17`，并已验证 `slf4j-simple:2.0.17`
- **Rust Workspace**：拆分为 `tinylog-rust-common`、`tinylog-converter` 和 `tinylog-viewer`
- **共享契约**：protobuf 定义位于 `tinylog-core/src/main/proto/tinylog/prototype.proto`
- **Java protobuf 生成说明**：`docs/zh-CN/protobuf-java-generation.md`
- **存储结构说明（英文）**：`docs/log-storage-structure.md`
- **存储结构说明（中文）**：`docs/zh-CN/log-storage-structure.md`

## 当前原型文件格式

当前原型使用 **trunk-based** 的 `.tog` 二进制布局，通过整块压缩、轻量索引和低内存窗口读取来支持交互式浏览。

完整的存储结构、字段定义和设计说明请查看：

- 英文：[`docs/log-storage-structure.md`](docs/log-storage-structure.md)
- 中文：[`docs/zh-CN/log-storage-structure.md`](docs/zh-CN/log-storage-structure.md)

## 手动测试命令

当前原型接受以下格式的明文日志：

```text
<yyyy-MM-dd HH:mm:ss,SSS> [LEVEL] <message>
```

转换器会把这个时间文本按 **UTC 日历值** 解释，把第一个 `[LEVEL]` 标记提取到独立的 1 字节级别字段中，并把这段级别文本从最终存储的消息内容里移除；viewer 再用持久化的级别和 UTC 偏移时间恢复展示内容。对于 **100 MiB** 以内的输入，转换器默认走串行模式，避免调度开销；超过这个阈值后，转换阶段会先由 master 扫描并规划 trunk 边界，再把连续 trunk 批次交给并行 worker 压缩，最后按顺序合并成最终 `.tog` 文件，并输出每个 worker 的处理进度。

### 1. 创建示例 `normal.log`

```bash
cat > normal.log <<'EOF'
2026-05-01 22:01:00,253 [INFO] service started
2026-05-01 22:01:00,278 [WARN] user signed in
2026-05-01 22:01:00,353 [ERROR] order created
EOF
```

### 2. 转换 `normal.log` 为 `normal.tog`

```bash
cargo run --quiet --manifest-path tinylog-converter/Cargo.toml -- normal.log normal.tog
```

辅助脚本：

```bash
scripts/tinylog-convert.sh normal.log
```

期望输出：

```text
using serial conversion mode for inputs up to 100.00 MiB
progress: 0/120 (0.00%)
progress: 120/120 (100.00%)
converted normal.log to normal.tog using gzip
source size: 120 (120 B)
output size: 111 (111 B)
compression ratio: 92.50%
elapsed: 4ms
```

对于超过 `100 MiB` 的大文件，转换器会直接按字节范围开始索引，并按 **已完成 trunk / 分配 trunk** 的比例输出一行实时刷新的 worker 进度，例如：

```text
building trunk index and preparing worker assignments for huge.log
indexing: 0/10737418317 (0.00%)
indexing: 10737418317/10737418317 (100.00%)
compressing 157 trunks with 16 workers
writing: 1: 0% 2: 0% 3: 0% 4: 0%
writing: 1: 10% 2: 20% 3: 24% 4: 10%
```

### 3. 用 Rust Viewer 打开 `normal.tog`

```bash
cargo run --quiet --manifest-path tinylog-viewer/Cargo.toml -- normal.tog
```

辅助脚本：

```bash
scripts/tinylog-view.sh normal.tog
scripts/tinylog-open.sh normal.log
```

按键说明：

```text
j / DownArrow   move down
k / UpArrow     move up
Enter           move down by 1/4 page
d / PageDown    page down
u / PageUp      page up
g               jump to top
G               jump to bottom
:N              跳到第 N 行
/keyword        按 trunk 搜索关键字，并跳到最近的结果
:debug          按级别过滤（也支持 :trace/:info/:warn/:error）
:help           打开帮助弹窗
n               跳到下一个已缓存结果 / 继续按 trunk 搜索
p               跳到上一个已缓存结果 / 继续按 trunk 搜索
Esc             清除过滤/搜索，或关闭帮助弹窗
q               quit
```

期望界面：

```text
TinyLog viewer | file=normal.tog | records=3 | line=1 | j/k move  enter +1/4  d/u page  g/G ends  q quit

1 ▪2026-05-01 22:01:00,253 [INFO] service started
2  2026-05-01 22:01:00,278 [WARN] user signed in
3  2026-05-01 22:01:00,353 [ERROR] order created
```

viewer 会保持运行，像一个轻量级的 vim 风格浏览器。展示区分为左右两栏：左边是蓝色的逻辑日志行号，右边是内容区，行号右侧偏右的位置会预留一个淡橙色的矩形标记来表示当前焦点行，而且只会显示在该日志的第一个物理展示行上。单条日志可能因为宽度限制或自身换行而展示成多行，但仍只对应一个逻辑序号。当前焦点行会先在可视区域内自由移动，只有继续向上或向下移动会把它推出屏幕边界时，展示区域才会开始滚动。

### 4. 只运行 converter 自动化测试

```bash
cargo test -p tinylog-converter convert_plaintext_log_writes_parseable_tog
```

## 下一阶段方向

1. 完善 TinyLog 文件头、trunk 布局和索引结构
2. 优化流式写入、读取和压缩链路
3. 在抽象日志 API 后面补上默认 Java SDK 实现
4. 继续增强 Rust viewer 的分页、搜索和跳转能力

## License

本项目使用 [MIT License](LICENSE)。
