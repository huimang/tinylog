# tinylog

[English README](README.md)

`tinylog` 是一个面向**高密度日志存储**和**低内存日志访问**的项目原型。
它主要解决传统明文日志的两个问题：存储空间过大，以及大文件遍历时内存开销过高。

## 项目目标

传统日志通常以明文方式存储，这会带来两个系统性问题：

1. **存储冗余高**：明文日志存在大量重复内容，文件会迅速膨胀
2. **读取放大明显**：日志文件一旦变大，扫描、浏览、过滤、定位都会消耗大量内存

当前项目围绕两个产品面展开：

1. **Java SDK**：用于业务项目集成，提供类似 `slf4j` 角色的业务日志接口
2. **Rust Viewer**：用于打开和浏览 `.tog` 专有日志文件，采用类似 `vim` 的浏览方式

## 模块划分

| 模块 | 职责 |
| --- | --- |
| `tinylog-core` | 核心日志领域模型、编解码抽象、读写接口 |
| `tinylog-sdk` | 面向业务的 Java 日志 API、工厂、SLF4J 2.0.17 桥接 |
| `tinylog-viewer` | 用于浏览专有日志文件的 Rust 客户端 |

## 工程准则

### 1. 接口设计强调业务语义

公共接口应该围绕**业务能力**命名，而不是围绕存储引擎、编解码器、缓冲区或传输细节命名。

- 优先使用 `log`、`browse`、`search`、`jump`、`record`、`query` 这类业务词汇
- 避免把实现细节暴露到业务接口上
- 保持抽象性，允许未来支持多种后端和多种文件格式

### 2. 模块必须独立自洽

每个模块都要有清晰边界、完整职责，并尽量减少跨模块假设。

- `tinylog-core` 定义共享契约
- `tinylog-sdk` 负责应用接入
- `tinylog-viewer` 作为独立客户端单独演进
- 跨模块依赖要显式且最小化

### 3. 代码默认提供注释

除非意图非常明显，否则代码应包含注释或文档注释。

- 解释业务含义和边界决策
- 注释保持简洁、耐久
- 公共类型和方法优先使用 API 级注释

### 4. 文档语言策略

- 根 README 英文优先
- 中文内容放到独立中文文档中
- API 名称和代码符号保持稳定、语言中立

### 5. 提交规范

- **Author** 使用仓库拥有者身份
- **Committer** 可以使用本地配置的自动化身份
- **Commit message** 只描述改动本身，不能出现 AI、模型、工具等字样
- 每个相对完整、稳定的功能应及时提交
- 如果同一功能修了多次提交，应在继续前整理成一个干净的功能级提交

提交信息示例：

```text
viewer: initialize rust cli scaffold
core: add log query abstraction
sdk: introduce business-facing logger factory
```

建议工作流：

1. 完成一个完整功能
2. 确认进入稳定状态
3. 为该功能创建一个提交
4. 如果有多次中间提交，先整理成一个干净提交再继续

## 当前技术方向

- **Java 命名空间**：`com.huimang.tinylog`
- **Java 构建**：`tinylog-core` 和 `tinylog-sdk` 的 Maven 多模块工程
- **Java SDK 兼容性**：`slf4j-api:2.0.17`，并已验证 `slf4j-simple:2.0.17`
- **Rust Viewer**：位于 `tinylog-viewer` 的独立 Cargo 工程
- **存储结构设计稿（英文）**：`docs/log-storage-structure.md`
- **存储结构设计稿（中文）**：`docs/zh-CN/log-storage-structure.md`

## 当前原型文件格式

当前原型采用 **trunk-based** 的二进制布局，字节序为 **big-endian**。

1. **版本号**：3 字节，来自 Maven 版本号
2. **压缩算法**：2 字节
3. **Trunk 大小**：2 字节，单位 KB
4. **基准时间戳**：8 字节，UTC 毫秒时间戳
5. **日志总行数**：8 字节
6. **Trunk 数量**：3 字节
7. **文件扩展名**：`.tog`
8. 对每个已完成 trunk 重复：
   - **Trunk 日志行数**：2 字节
   - **Trunk 压缩后长度**：4 字节
   - **Trunk 压缩内容**：整个原始 trunk 的压缩结果

即：

```text
[version:3][compression:2][trunkSizeKb:2][baseTimestampUtcMillis:8][totalLogLineCount:8][trunkCount:3]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
...
```

当前原型说明：

- Java 写入端先把原始日志行写入 `log-buffer-{trunkIndex}.tmp`
- 缓冲达到 trunk 阈值后，会压缩整个 trunk 并追加到主 `.tog`
- 解压后的 trunk 内，每条原始日志行为 `[offsetMillis:4][contentLength:4][content:N]`
- Rust viewer 只会解压当前可视窗口需要的 trunk
- 完整设计见 `docs/log-storage-structure.md` 和 `docs/zh-CN/log-storage-structure.md`

压缩算法 ID：

| ID | 算法 |
| --- | --- |
| `0` | none |
| `1` | gzip |
| `2` | zip |
| `3` | deflate |
| `4` | bzip2 |
| `5` | xz |
| `6` | zstd |
| `7` | snappy |

## 手动测试命令

当前原型接受以下格式的明文日志：

```text
<yyyy-MM-dd HH:mm:ss,SSS> <message>
```

转换器会把这个时间文本按 **UTC 日历值** 解释，viewer 也会按 UTC 方式还原显示，因此每条日志只需保存相对于文件级 UTC 基准时间的偏移量。

### 1. 创建示例 `normal.log`

```bash
cat > normal.log <<'EOF'
2026-05-01 22:01:00,253 service started
2026-05-01 22:01:00,278 user signed in
2026-05-01 22:01:00,353 order created
EOF
```

### 2. 转换 `normal.log` 为 `normal.tog`

```bash
mvn -q -pl tinylog-core -am package
java -jar tinylog-core/target/tinylog-core-0.1.0-SNAPSHOT-all.jar normal.log normal.tog
```

期望输出：

```text
converted normal.log to normal.tog using gzip
```

### 3. 用 Rust Viewer 打开 `normal.tog`

```bash
cargo run --quiet --manifest-path tinylog-viewer/Cargo.toml -- normal.tog
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
q               quit
```

期望界面：

```text
tinylog viewer | file=normal.tog | records=3 | line=1 | j/k move  enter +1/4  d/u page  g/G ends  q quit

1▏2026-05-01 22:01:00,253 service started
2  2026-05-01 22:01:00,278 user signed in
3  2026-05-01 22:01:00,353 order created
```

viewer 会保持运行，像一个轻量级的 vim 风格浏览器。展示区分为左右两栏：左边是蓝色的逻辑日志行号，右边是内容区，行号右侧偏右的位置会预留一个淡橙色的矩形标记来表示当前焦点行，而且只会显示在该日志的第一个物理展示行上。单条日志可能因为宽度限制或自身换行而展示成多行，但仍只对应一个逻辑序号。当前焦点行会先在可视区域内自由移动，只有继续向上或向下移动会把它推出屏幕边界时，展示区域才会开始滚动。

### 4. 只运行 converter 自动化测试

```bash
mvn -q -pl tinylog-core -Dtest=PlainTextLogToTinylogConverterTest test
```

## 下一阶段方向

1. 完善 tinylog 文件头、trunk 布局和索引结构
2. 优化流式写入、读取和压缩链路
3. 在抽象日志 API 后面补上默认 Java SDK 实现
4. 继续增强 Rust viewer 的分页、搜索和跳转能力
