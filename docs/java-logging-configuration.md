# TinyLog Java Logging Configuration

TinyLog provides a Java-side configuration module in `tinylog-sdk` for projects that want a business-focused logging facade with configurable console and file outputs.

## Configuration files

The loader searches the classpath in this order:

1. `tinylog.yml`
2. `tinylog.yaml`
3. `tinylog.properties`

Use `LoggerFactory.loadDefault()` to load the first matching file from `resources/`.

## Supported features

- Console and file appenders
- Root-level filtering and appender-level filtering
- Size-based file rotation with archive retention
- Optional file splitting by log level
- Environment placeholders
- Thread-local business variables through `LogContext`
- Message masking for password, mobile number, and email
- Variable masking for `password`, `mobile`, `email`, and `partial`

Timestamp and log level are always rendered by the runtime and are **not configurable**.

## YAML format

```yaml
tinylog:
  root:
    level: trace
    appenders:
      - console
      - infoFile
  appenders:
    console:
      type: console
      target: SYSTEM_OUT
      level: trace
      pattern: "[%logger] requestId=%var{requestId:-missing} user=%env{USER:-unknown} %message"
    infoFile:
      type: file
      level: info
      fileName: logs/app.tog
      filePattern: logs/archive/app-%i.tog
      format: tog
      compression: gzip
      trunkSizeKb: 512
      pattern: "[%logger] requestId=%var{requestId:-missing} %message"
      splitByLevel: false
      policies:
        size:
          size: 10MB
      strategy:
        max: 5
  masking:
    contentRules:
      - password
      - mobile
      - email
    variableRules:
      requestId: partial
      userId: partial
```

## Properties format

```properties
tinylog.root.level=trace
tinylog.root.appenders=console,infoFile

tinylog.appender.console.type=console
tinylog.appender.console.target=SYSTEM_OUT
tinylog.appender.console.level=trace
tinylog.appender.console.pattern=[%logger] requestId=%var{requestId:-missing} user=%env{USER:-unknown} %message

tinylog.appender.infoFile.type=file
tinylog.appender.infoFile.level=info
tinylog.appender.infoFile.fileName=logs/app.tog
tinylog.appender.infoFile.filePattern=logs/archive/app-%i.tog
tinylog.appender.infoFile.format=tog
tinylog.appender.infoFile.compression=gzip
tinylog.appender.infoFile.trunkSizeKb=512
tinylog.appender.infoFile.pattern=[%logger] requestId=%var{requestId:-missing} %message
tinylog.appender.infoFile.policies.size.size=10MB
tinylog.appender.infoFile.strategy.max=5

tinylog.masking.contentRules=password,mobile,email
tinylog.masking.variableRules.requestId=partial
tinylog.masking.variableRules.userId=partial
```

## Pattern placeholders

The formatter keeps timestamp and level fixed, then renders the configured pattern.

| Placeholder | Meaning |
| --- | --- |
| `%message` / `%msg` | Final message content |
| `%logger` | Logical logger name |
| `%context` / `%thread` | Current thread name |
| `%env{KEY}` | Environment variable |
| `%env{KEY:-default}` | Environment variable with fallback |
| `%var{KEY}` | Value from `LogContext` |
| `%var{KEY:-default}` | Context value with fallback |

## Level filtering

- `tinylog.root.level` blocks events before they reach any appender.
- `tinylog.appender.<name>.level` adds an appender-specific threshold.

For example, `root.level=trace` with `infoFile.level=info` means:

- console can print all levels
- the file appender only stores `INFO`, `WARN`, and `ERROR`

## File rotation

File appenders currently support **size-based rotation**:

- `policies.size.size`: threshold such as `256KB`, `10MB`, or `1GB`
- `strategy.max`: number of retained archives

If `splitByLevel=true`, TinyLog writes level-specific files such as `app-info.log` and `app-error.log`. If `fileName` or `filePattern` contains `%level`, the placeholder is replaced directly.

## File formats

File appenders support two formats:

- `format: text` for line-oriented text files
- `format: tog` for TinyLog trunk-based binary files

For `format: tog`, the appender stores TinyLog records directly and still applies:

- root/appender level filtering
- pattern-based body rendering
- content and variable masking
- size-based rotation
- a `.buffer` sidecar that keeps not-yet-merged raw records recoverable until they are flushed into the main `.tog`

Optional `.tog` settings:

- `compression`: defaults to `gzip`
- `trunkSizeKb`: defaults to `512`

## Masking

### Content rules

- `password`
- `mobile`
- `email`

Content rules are applied to the rendered message body.

### Variable rules

Variable rules are keyed by variable name and support:

- `password`
- `mobile`
- `email`
- `partial`

## Example usage

```java
try (LoggerFactory factory = LoggerFactory.loadDefault()) {
    Logger logger = factory.getLogger("checkout");
    LogContext.put("requestId", "REQ-20260503-0001");
    LogContext.put("userId", "user-95270086");
    logger.info("checkout completed for user email=tinylog@example.com");
    LogContext.clear();
}
```

See `tinylog-example/src/main/resources/tinylog.yml` for a working YAML configuration that enables:

- console output for all levels
- `.tog` file output for `INFO` and above
