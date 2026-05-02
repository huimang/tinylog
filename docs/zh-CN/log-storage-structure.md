# Tinylog 日志存储结构与 Trunk 流程设计

> 状态：**已实现的原型**
>
> 本文档描述当前已经落地的 trunk-based `.tog` 原型，以及 Rust converter 和 Rust viewer 的实际行为。

## 设计目标

这一版设计不再采用**按行压缩**，而是改为**按 trunk 压缩**。

目标如下：

1. 保持写入流程简单、顺序追加友好
2. 让同一个 trunk 内的重复文本可以被整体压缩复用
3. 保持浏览和局部解压的可控性
4. 让 Java 写入端和 Rust 查看端都能基于同一份明确的磁盘结构实现

## 设计概要

- **默认压缩算法**：`gzip`
- **存储单元**：`trunk`
- **默认 trunk 大小**：`512 KB`
- **基准时间**：文件 header 中保存一个全局 UTC 基准时间戳
- **写入路径**：Rust converter 会直接从源日志构建 trunk 字节区间；大文件下由多个 worker 并行压缩连续 trunk 批次，最后由 master 顺序合并到最终 `.tog` 文件

## 文件 Header 结构

主 `.tog` 文件以一个固定长度 header 开头，采用 **big-endian** 存储。

| 字段 | 长度 | 默认值 | 含义 |
| --- | ---: | --- | --- |
| `versionMajor` | 1 字节 |  | Maven 版本号第一段 |
| `versionMinor` | 1 字节 |  | Maven 版本号第二段 |
| `versionPatch` | 1 字节 |  | Maven 版本号第三段 |
| `compressionAlgorithm` | 2 字节 | 1 | 文件级压缩算法 ID |
| `trunkSizeKb` | 2 字节 | 512 | trunk 大小，单位 KB |
| `baseTimestampUtcMillis` | 8 字节 |  | 文件级 UTC 基准时间戳（毫秒） |
| `totalLogLineCount` | 8 字节 | 0 | 已持久化日志总行数 |
| `trunkCount` | 3 字节 | 0 | 已写入主文件的 trunk 总数 |
### Header 布局

```text
[version:3]
[compressionAlgorithm:2]
[trunkSizeKb:2]
[baseTimestampUtcMillis:8]
[totalLogLineCount:8]
[trunkCount:3]
```

### Header 说明

1. `version` 来自 Maven 版本号，例如：
   - `0.1.0-SNAPSHOT` -> `[0, 1, 0]`
   - `-SNAPSHOT` 这类后缀不进入 header
2. `trunkSizeKb` 以无符号 16 位整数存储，单位是 KB
3. 设计上允许的 trunk 上限是 **64 MB**，实现时需要做范围校验
4. `baseTimestampUtcMillis` 是全文件唯一的时间基准，所有日志行的 `offsetMillis` 都相对它计算
5. 每次 trunk 成功落盘后，都要回写更新 `totalLogLineCount` 和 `trunkCount`

## 压缩算法 ID

现有算法 ID 空间继续保留，但默认算法改回 `gzip`。

| ID | 算法 | 默认 |
| --- | --- | --- |
| `0` | none | 否 |
| `1` | gzip | **是** |
| `2` | zip | 否 |
| `3` | deflate | 否 |
| `4` | bzip2 | 否 |
| `5` | xz | 否 |
| `6` | zstd | 否 |
| `7` | snappy | 否 |

## Trunk 结构

每个完成的 trunk 追加到主 `.tog` 文件时，结构如下：

| 字段 | 长度 | 含义 |
| --- | ---: | --- |
| `trunkLogLineCount` | 2 字节 | 当前 trunk 内的日志行数 |
| `compressedContentLength` | 4 字节 | 压缩后 trunk 内容长度 |
| `compressedContent` | N 字节 | 整个 trunk 的压缩内容 |
### Trunk 布局

```text
[trunkLogLineCount:2]
[compressedContentLength:4]
[compressedContent:N]
```

## Trunk 内部原始日志行结构

一个 trunk 在压缩前，内部按顺序保存多条原始日志行：

| 字段 | 长度 | 含义 |
| --- | ---: | --- |
| `offsetMillis` | 4 字节 | 相对 `baseTimestampUtcMillis` 的毫秒偏移 |
| `level` | 1 字节 | 持久化日志级别标识 |
| `contentLength` | 4 字节 | 当前日志内容字节长度 |
| `content` | N 字节 | UTF-8 日志内容，不做行级压缩 |

### 原始日志行布局

```text
[offsetMillis:4][level:1][contentLength:4][content:N]
[offsetMillis:4][level:1][contentLength:4][content:N]
[offsetMillis:4][level:1][contentLength:4][content:N]
...
```

### 原始日志行说明

1. `content` 只存时间戳后面的日志内容
2. 时间戳恢复公式固定为：

   ```text
   actualTimestampUtcMillis = baseTimestampUtcMillis + offsetMillis
   ```

3. `offsetMillis` 使用 4 字节存储，因此单个文件可表达大约 `2^32` 毫秒的相对时间跨度
4. `contentLength` 使用 4 字节，是因为你已经明确要求每行使用显式长度，而不是换行符分隔

## 完整文件结构

```text
[header]
[trunk-0]
[trunk-1]
[trunk-2]
...
```

展开后可表示为：

```text
[version:3][compressionAlgorithm:2][trunkSizeKb:2][baseTimestampUtcMillis:8][totalLogLineCount:8][trunkCount:3]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
[trunkLogLineCount:2][compressedContentLength:4][compressedContent:N]
...
```

## 写入流程

当前写入流程由 Rust converter 实现。小文件保持串行转换，大文件切换到 master/worker 并行转换。

### 写入流程图

```text
+-------------------------+
| 创建主 .tog 文件        |
+-------------------------+
            |
            v
+-------------------------+
| 写入固定 header         |
+-------------------------+
            |
            v
+----------------------------------------------+
| 对于 > 100 MiB 的输入，先按 trunkSize 跳转， |
| 再对齐到下一个记录起点                       |
+----------------------------------------------+
            |
            v
+----------------------------------------------+
| 将连续 trunk 区间分配给多个 worker           |
+----------------------------------------------+
            |
            v
+----------------------------------------------+
| worker 读取规划好的 trunk 字节区间           |
| 解析记录和 multiline continuation            |
| 生成 offsetMillis / level / contentLength    |
| 并压缩整个 trunk                             |
+----------------------------------------------+
            |
            v
+---------------------------------------------------+
| worker 把 trunkLogLineCount、压缩内容以及元信息回传 |
+---------------------------------------------------+
            |
            v
+----------------------------------+
| master 顺序合并结果并回写 header |
+----------------------------------+
```

### 写入步骤

1. 创建主 `.tog` 文件并初始化 header
2. 对于不超过 `100 MiB` 的输入，直接在单进程串行解析并刷出 trunk
3. 对于更大的输入：
   1. 按配置的 trunk 字节大小向前跳转
   2. 把每个边界对齐到下一个记录起点（`\n` 后接时间戳形态前缀）
   3. 用这些字节区间作为 trunk 规划结果
   4. 把连续 trunk 区间分配给 worker
4. 每个 worker 读取自己负责的源字节区间；遇到带时间戳的行就开始新记录，后续非时间戳行则并入上一条记录，作为 multiline continuation
5. worker 生成 `[offsetMillis:4][level:1][contentLength:4][content]`，压缩整个 trunk，并把记录数与时间戳元信息回传给 master
6. master 按顺序合并 worker 输出，并最终写回 `totalLogLineCount` 和 `trunkCount`

## 读取与浏览流程

读取端和 viewer 都以 header + trunk 序列的方式工作。

### 读取流程图

```text
+-------------------+
| 打开 .tog 文件    |
+-------------------+
          |
          v
+-------------------+
| 读取 header       |
+-------------------+
          |
          v
+-------------------------+
| 确定目标 trunk 范围     |
+-------------------------+
          |
          v
+-------------------------+
| 读取 trunk 元数据       |
+-------------------------+
          |
          v
+----------------------------+
| 读取压缩后的 trunk 内容    |
+----------------------------+
          |
          v
+----------------------------+
| 只解压需要的 trunk         |
+----------------------------+
          |
          v
+----------------------------+
| 解析 trunk 内部原始日志行  |
+----------------------------+
          |
          v
+------------------------------------------------------+
| 根据 baseTimestampUtcMillis + offset 重建时间戳       |
+------------------------------------------------------+
          |
          v
+----------------------------+
| 过滤或渲染目标日志行       |
+----------------------------+
```

### 读取步骤

1. 读取固定 header
2. 打开文件时先快速扫描全部 trunk 的位置和行数，并缓存在内存中
3. 基于内存索引定位当前可见范围或查询范围
4. 只读取目标 trunk 的压缩内容
5. 只解压当前窗口、当前搜索步骤或当前过滤步骤所需的 trunk
6. 解析 trunk 内部的原始日志行
7. 基于文件级 UTC 基准时间恢复实际时间
8. 返回需要展示或查询的日志记录

## Viewer 侧预期

Rust viewer 仍然保持轻量级 vim 风格浏览：

- 保留交互式导航方式
- 不解压无关 trunk
- 只解压当前可视窗口所需的 trunk 或 trunk 子集
- 搜索和级别过滤采用按需继续的 trunk 扫描方式，而不是启动时一次性解压整文件

也就是说，这次设计把解压粒度从**按行**改成了**按 trunk**。

## 兼容性说明

这一版设计与当前 prototype 格式**不兼容**。

影响如下：

1. 旧版 `.tog` 文件需要重新转换
2. 任何 writer/reader 实现都必须一起切换；当前已经落地的是 Rust converter + Rust viewer 这一套流程
3. 测试需要覆盖：
   - 版本号写入与解析
   - trunk 刷盘逻辑
   - header 计数回写
   - 最后一个未满 trunk 的刷盘
   - viewer 只解压目标 trunk 的行为

## 当前契约

当前原型契约如下：

1. Header 顺序固定为：`version -> compression -> trunkSizeKb -> baseTimestampUtcMillis -> totalLogLineCount -> trunkCount`
2. 所有 trunk 共用一个文件级 UTC 基准时间戳
3. trunk 内每行格式固定为：`[offsetMillis:4][level:1][contentLength:4][content]`
4. trunk 格式固定为：`[trunkLogLineCount:2][compressedContentLength:4][compressedContent]`
5. 默认压缩算法为 `gzip`
6. 大文件索引按字节范围进行、边界对齐到记录起点，记录数统计放到 worker 压缩阶段完成
