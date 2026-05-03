package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.io.PrototypeLogFileWriter;
import com.huimang.tinylog.core.model.LogRecord;
import com.huimang.tinylog.core.model.LogLevel;
import java.io.Closeable;
import java.io.IOException;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Date;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.TimeZone;
import java.util.concurrent.ConcurrentHashMap;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Creates TinyLog SDK loggers backed by the project's TinyLog YAML or properties configuration.
 */
public final class LoggerFactory implements TinyLoggerFactory, Closeable {
    private final TinylogConfiguration configuration;
    private final TinylogMasker masker;
    private final List<TinylogAppenderRuntime> appenders;
    private final Map<String, Logger> loggers = new ConcurrentHashMap<String, Logger>();
    private volatile boolean closed;

    /**
     * Creates a factory from one already parsed TinyLog configuration.
     */
    public LoggerFactory(TinylogConfiguration configuration) throws IOException {
        this.configuration = Objects.requireNonNull(configuration, "configuration");
        this.masker = new TinylogMasker(configuration.getMasking());
        this.appenders = buildAppenders(configuration);
    }

    /**
     * Loads the default configuration resource from the application classpath.
     */
    public static LoggerFactory loadDefault() throws IOException {
        return new LoggerFactory(new TinylogConfigurationLoader().loadDefault());
    }

    /**
     * Loads one explicit configuration file.
     */
    public static LoggerFactory load(Path path) throws IOException {
        return new LoggerFactory(new TinylogConfigurationLoader().load(path));
    }

    @Override
    public Logger getLogger(String name) {
        ensureOpen();
        Objects.requireNonNull(name, "name");
        Logger existing = loggers.get(name);
        if (existing != null) {
            return existing;
        }
        Logger created = new ConfiguredLogger(name, this);
        Logger raced = loggers.putIfAbsent(name, created);
        return raced == null ? created : raced;
    }

    @Override
    public void close() throws IOException {
        closed = true;
        IOException firstFailure = null;
        for (TinylogAppenderRuntime appender : appenders) {
            try {
                appender.close();
            } catch (IOException exception) {
                if (firstFailure == null) {
                    firstFailure = exception;
                }
            }
        }
        if (firstFailure != null) {
            throw firstFailure;
        }
    }

    void append(String loggerName, LogLevel level, String message, Throwable throwable) {
        ensureOpen();
        if (!isEnabled(configuration.getRootLevel(), level)) {
            return;
        }
        TinylogLogEvent event = new TinylogLogEvent(
                System.currentTimeMillis(),
                level,
                loggerName,
                Thread.currentThread().getName(),
                formatMessage(message, throwable),
                LogContext.snapshot());
        for (TinylogAppenderRuntime appender : appenders) {
            if (!isEnabled(appender.level(), level)) {
                continue;
            }
            try {
                appender.append(event, masker);
            } catch (IOException exception) {
                throw new IllegalStateException("failed to write configured tinylog event", exception);
            }
        }
    }

    private List<TinylogAppenderRuntime> buildAppenders(TinylogConfiguration configuration) throws IOException {
        List<TinylogAppenderRuntime> runtimes = new ArrayList<TinylogAppenderRuntime>();
        for (String appenderName : configuration.getRootAppenders()) {
            TinylogAppenderConfiguration appender = configuration.getAppenders().get(appenderName);
            if (appender.getType() == TinylogAppenderConfiguration.Type.CONSOLE) {
                runtimes.add(new ConsoleAppenderRuntime(appender));
            } else {
                runtimes.add(new FileAppenderRuntime(appender));
            }
        }
        return Collections.unmodifiableList(runtimes);
    }

    private boolean isEnabled(LogLevel threshold, LogLevel actual) {
        return actual.ordinal() >= threshold.ordinal();
    }

    private String formatMessage(String message, Throwable throwable) {
        Objects.requireNonNull(message, "message");
        if (throwable == null) {
            return message;
        }
        StringWriter buffer = new StringWriter();
        PrintWriter writer = new PrintWriter(buffer);
        writer.print(message);
        writer.println();
        throwable.printStackTrace(writer);
        writer.flush();
        return trimTrailingLineBreaks(buffer.toString());
    }

    private String trimTrailingLineBreaks(String value) {
        int end = value.length();
        while (end > 0) {
            char current = value.charAt(end - 1);
            if (current != '\n' && current != '\r') {
                break;
            }
            end--;
        }
        return value.substring(0, end);
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("configured tinylog logger factory is already closed");
        }
    }

    private interface TinylogAppenderRuntime extends Closeable {
        LogLevel level();

        void append(TinylogLogEvent event, TinylogMasker masker) throws IOException;
    }

    private static final class ConfiguredLogger implements Logger {
        private final String name;
        private final LoggerFactory factory;

        private ConfiguredLogger(String name, LoggerFactory factory) {
            this.name = name;
            this.factory = factory;
        }

        @Override
        public String getName() {
            return name;
        }

        @Override
        public void trace(String message) {
            factory.append(name, LogLevel.TRACE, message, null);
        }

        @Override
        public void debug(String message) {
            factory.append(name, LogLevel.DEBUG, message, null);
        }

        @Override
        public void info(String message) {
            factory.append(name, LogLevel.INFO, message, null);
        }

        @Override
        public void warn(String message) {
            factory.append(name, LogLevel.WARN, message, null);
        }

        @Override
        public void error(String message) {
            factory.append(name, LogLevel.ERROR, message, null);
        }

        @Override
        public void error(String message, Throwable throwable) {
            factory.append(name, LogLevel.ERROR, message, throwable);
        }
    }

    private static final class ConsoleAppenderRuntime implements TinylogAppenderRuntime {
        private final LogLevel level;
        private final TinylogPatternFormatter formatter;
        private final java.io.PrintStream stream;

        private ConsoleAppenderRuntime(TinylogAppenderConfiguration configuration) {
            this.level = configuration.getLevel();
            this.formatter = new TinylogPatternFormatter(configuration.getPattern());
            this.stream = configuration.getConsoleTarget() == TinylogAppenderConfiguration.ConsoleTarget.SYSTEM_ERR
                    ? System.err
                    : System.out;
        }

        @Override
        public LogLevel level() {
            return level;
        }

        @Override
        public synchronized void append(TinylogLogEvent event, TinylogMasker masker) {
            stream.println(formatter.formatLine(event, masker));
        }

        @Override
        public void close() {
        }
    }

    private static final class FileAppenderRuntime implements TinylogAppenderRuntime {
        private final TinylogAppenderConfiguration configuration;
        private final TinylogPatternFormatter formatter;
        private final Map<String, TextFileHandle> textHandles = new LinkedHashMap<String, TextFileHandle>();
        private final Map<String, TogFileHandle> togHandles = new LinkedHashMap<String, TogFileHandle>();

        private FileAppenderRuntime(TinylogAppenderConfiguration configuration) throws IOException {
            this.configuration = configuration;
            this.formatter = new TinylogPatternFormatter(configuration.getPattern());
            ensureParentDirectory(resolveActiveFilePath(LogLevel.INFO));
        }

        @Override
        public LogLevel level() {
            return configuration.getLevel();
        }

        @Override
        public synchronized void append(TinylogLogEvent event, TinylogMasker masker) throws IOException {
            if (configuration.getFileFormat() == TinylogAppenderConfiguration.FileFormat.TOG) {
                appendTog(event, masker);
                return;
            }
            appendText(event, masker);
        }

        private void appendText(TinylogLogEvent event, TinylogMasker masker) throws IOException {
            Path target = resolveActiveFilePath(event.level());
            String key = configuration.isSplitByLevel()
                    ? event.level().name().toLowerCase(Locale.ROOT)
                    : "default";
            TextFileHandle handle = textHandles.get(key);
            if (handle == null || !handle.path.equals(target)) {
                closeTextHandle(handle);
                handle = openTextHandle(target);
                textHandles.put(key, handle);
            }
            byte[] bytes = (formatter.formatLine(event, masker) + System.lineSeparator())
                    .getBytes(StandardCharsets.UTF_8);
            handle = rotateTextIfNeeded(handle, event.level(), bytes.length);
            handle.writer.write(new String(bytes, StandardCharsets.UTF_8));
            handle.writer.flush();
        }

        private void appendTog(TinylogLogEvent event, TinylogMasker masker) throws IOException {
            Path target = resolveActiveFilePath(event.level());
            String key = configuration.isSplitByLevel()
                    ? event.level().name().toLowerCase(Locale.ROOT)
                    : "default";
            TogFileHandle handle = togHandles.get(key);
            if (handle == null || !handle.path.equals(target)) {
                closeTogHandle(handle);
                handle = openTogHandle(target);
                togHandles.put(key, handle);
            }
            handle.writer.append(new LogRecord(
                    event.timestampMillis(),
                    event.level(),
                    event.loggerName(),
                    event.threadName(),
                    formatter.formatBody(event, masker),
                    null));
            handle.writer.flush();
            rotateTogIfNeeded(handle, event.level(), key);
        }

        @Override
        public synchronized void close() throws IOException {
            IOException firstFailure = null;
            for (TextFileHandle handle : textHandles.values()) {
                try {
                    closeTextHandle(handle);
                } catch (IOException exception) {
                    if (firstFailure == null) {
                        firstFailure = exception;
                    }
                }
            }
            for (TogFileHandle handle : togHandles.values()) {
                try {
                    closeTogHandle(handle);
                } catch (IOException exception) {
                    if (firstFailure == null) {
                        firstFailure = exception;
                    }
                }
            }
            textHandles.clear();
            togHandles.clear();
            if (firstFailure != null) {
                throw firstFailure;
            }
        }

        private TextFileHandle rotateTextIfNeeded(TextFileHandle handle, LogLevel level, int nextMessageBytes)
                throws IOException {
            if (configuration.getMaxFileSizeBytes() <= 0L) {
                return handle;
            }
            long currentSize = Files.exists(handle.path) ? Files.size(handle.path) : 0L;
            if (currentSize + nextMessageBytes <= configuration.getMaxFileSizeBytes()) {
                return handle;
            }
            closeHandle(handle);
            rotateArchives(level);
            TextFileHandle reopened = openTextHandle(handle.path);
            String key = configuration.isSplitByLevel()
                    ? level.name().toLowerCase(Locale.ROOT)
                    : "default";
            textHandles.put(key, reopened);
            return reopened;
        }

        private void rotateTogIfNeeded(TogFileHandle handle, LogLevel level, String key) throws IOException {
            if (configuration.getMaxFileSizeBytes() <= 0L || !Files.exists(handle.path)) {
                return;
            }
            if (Files.size(handle.path) <= configuration.getMaxFileSizeBytes()) {
                return;
            }
            closeTogHandle(handle);
            rotateArchives(level);
            togHandles.put(key, openTogHandle(handle.path));
        }

        private void rotateArchives(LogLevel level) throws IOException {
            Path active = resolveActiveFilePath(level);
            ensureParentDirectory(active);
            int maxArchives = Math.max(0, configuration.getMaxArchivedFiles());
            if (maxArchives == 0) {
                Files.deleteIfExists(active);
                return;
            }
            for (int index = maxArchives; index >= 1; index--) {
                Path target = resolveArchiveFilePath(level, index);
                if (index == maxArchives) {
                    Files.deleteIfExists(target);
                }
                Path source = index == 1 ? active : resolveArchiveFilePath(level, index - 1);
                if (Files.exists(source)) {
                    ensureParentDirectory(target);
                    Files.move(source, target);
                }
            }
        }

        private Path resolveActiveFilePath(LogLevel level) {
            return Paths.get(applyLevel(configuration.getFileName(), level));
        }

        private Path resolveArchiveFilePath(LogLevel level, int index) {
            String pattern = configuration.getFilePattern();
            if (isBlank(pattern)) {
                pattern = configuration.getFileName() + ".%i";
            }
            return Paths.get(applyIndex(applyLevel(pattern, level), index));
        }

        private String applyLevel(String value, LogLevel level) {
            if (value == null) {
                return null;
            }
            if (configuration.isSplitByLevel()) {
                String normalizedLevel = level.name().toLowerCase(Locale.ROOT);
                if (value.contains("%level")) {
                    return value.replace("%level", normalizedLevel);
                }
                int extensionStart = value.lastIndexOf('.');
                if (extensionStart > value.lastIndexOf('/')) {
                    return value.substring(0, extensionStart) + "-" + normalizedLevel + value.substring(extensionStart);
                }
                return value + "-" + normalizedLevel;
            }
            return value.replace("%level", level.name().toLowerCase(Locale.ROOT));
        }

        private String applyIndex(String value, int index) {
            return value.contains("%i") ? value.replace("%i", String.valueOf(index)) : value + "." + index;
        }

        private TextFileHandle openTextHandle(Path path) throws IOException {
            ensureParentDirectory(path);
            return new TextFileHandle(path, Files.newBufferedWriter(
                    path,
                    StandardCharsets.UTF_8,
                    StandardOpenOption.CREATE,
                    StandardOpenOption.APPEND));
        }

        private TogFileHandle openTogHandle(Path path) throws IOException {
            ensureParentDirectory(path);
            return new TogFileHandle(path, new PrototypeLogFileWriter(
                    path,
                    configuration.getCompressionAlgorithm(),
                    configuration.getTrunkSizeKb()));
        }

        private void ensureParentDirectory(Path path) throws IOException {
            Path parent = path.getParent();
            if (parent != null) {
                Files.createDirectories(parent);
            }
        }

        private void closeHandle(TextFileHandle handle) throws IOException {
            closeTextHandle(handle);
        }

        private void closeTextHandle(TextFileHandle handle) throws IOException {
            if (handle == null) {
                return;
            }
            handle.writer.close();
        }

        private void closeTogHandle(TogFileHandle handle) throws IOException {
            if (handle == null) {
                return;
            }
            handle.writer.close();
        }

        private boolean isBlank(String value) {
            return value == null || value.trim().isEmpty();
        }

        private static final class TextFileHandle {
            private final Path path;
            private final java.io.Writer writer;

            private TextFileHandle(Path path, java.io.Writer writer) {
                this.path = path;
                this.writer = writer;
            }
        }

        private static final class TogFileHandle {
            private final Path path;
            private final PrototypeLogFileWriter writer;

            private TogFileHandle(Path path, PrototypeLogFileWriter writer) {
                this.path = path;
                this.writer = writer;
            }
        }
    }

    private static final class TinylogLogEvent {
        private final long timestampMillis;
        private final LogLevel level;
        private final String loggerName;
        private final String threadName;
        private final String message;
        private final Map<String, String> variables;

        private TinylogLogEvent(long timestampMillis,
                LogLevel level,
                String loggerName,
                String threadName,
                String message,
                Map<String, String> variables) {
            this.timestampMillis = timestampMillis;
            this.level = level;
            this.loggerName = loggerName;
            this.threadName = threadName;
            this.message = message;
            this.variables = variables;
        }

        private long timestampMillis() {
            return timestampMillis;
        }

        private LogLevel level() {
            return level;
        }

        private String loggerName() {
            return loggerName;
        }

        private String threadName() {
            return threadName;
        }

        private String message() {
            return message;
        }

        private Map<String, String> variables() {
            return variables;
        }
    }

    private static final class TinylogPatternFormatter {
        private static final Pattern TOKEN = Pattern.compile("%(message|msg|logger|context|thread|env\\{[^}]+\\}|var\\{[^}]+\\})");
        private static final ThreadLocal<SimpleDateFormat> FORMATTER = new ThreadLocal<SimpleDateFormat>() {
            @Override
            protected SimpleDateFormat initialValue() {
                SimpleDateFormat formatter = new SimpleDateFormat("yyyy-MM-dd HH:mm:ss,SSS");
                formatter.setTimeZone(TimeZone.getDefault());
                return formatter;
            }
        };

        private final String pattern;

        private TinylogPatternFormatter(String pattern) {
            this.pattern = pattern;
        }

        private String formatLine(TinylogLogEvent event, TinylogMasker masker) {
            return FORMATTER.get().format(new Date(event.timestampMillis()))
                    + " ["
                    + event.level().name()
                    + "] "
                    + formatBody(event, masker);
        }

        private String formatBody(TinylogLogEvent event, TinylogMasker masker) {
            Matcher matcher = TOKEN.matcher(pattern);
            StringBuffer rendered = new StringBuffer();
            while (matcher.find()) {
                matcher.appendReplacement(rendered, Matcher.quoteReplacement(resolveToken(matcher.group(1), event, masker)));
            }
            matcher.appendTail(rendered);
            return masker.maskContent(rendered.toString());
        }

        private String resolveToken(String token, TinylogLogEvent event, TinylogMasker masker) {
            if ("message".equals(token) || "msg".equals(token)) {
                return masker.maskContent(event.message());
            }
            if ("logger".equals(token)) {
                return event.loggerName();
            }
            if ("context".equals(token) || "thread".equals(token)) {
                return event.threadName();
            }
            if (token.startsWith("env{")) {
                Placeholder placeholder = Placeholder.parse(token.substring(4, token.length() - 1));
                String value = System.getenv(placeholder.key);
                return masker.maskVariable(placeholder.key, value == null ? placeholder.defaultValue : value);
            }
            if (token.startsWith("var{")) {
                Placeholder placeholder = Placeholder.parse(token.substring(4, token.length() - 1));
                String value = event.variables().get(placeholder.key);
                return masker.maskVariable(placeholder.key, value == null ? placeholder.defaultValue : value);
            }
            return "";
        }
    }

    private static final class Placeholder {
        private final String key;
        private final String defaultValue;

        private Placeholder(String key, String defaultValue) {
            this.key = key;
            this.defaultValue = defaultValue == null ? "" : defaultValue;
        }

        private static Placeholder parse(String raw) {
            int separator = raw.indexOf(":-");
            if (separator < 0) {
                return new Placeholder(raw, "");
            }
            return new Placeholder(raw.substring(0, separator), raw.substring(separator + 2));
        }
    }

    private static final class TinylogMasker {
        private static final Pattern PASSWORD = Pattern.compile("(?i)(password\\s*[=:]\\s*)([^\\s,;]+)");
        private static final Pattern MOBILE = Pattern.compile("(?<!\\d)(1[3-9]\\d{9})(?!\\d)");
        private static final Pattern EMAIL = Pattern.compile("([A-Za-z0-9._%+-]{1,64})@([A-Za-z0-9.-]+\\.[A-Za-z]{2,})");
        private final TinylogMaskingConfiguration configuration;

        private TinylogMasker(TinylogMaskingConfiguration configuration) {
            this.configuration = configuration;
        }

        private String maskContent(String raw) {
            String value = raw == null ? "" : raw;
            for (String rule : configuration.getContentRules()) {
                if ("password".equals(rule)) {
                    value = replaceGroup(PASSWORD, value, 2, "******");
                } else if ("mobile".equals(rule)) {
                    value = replaceMatch(MOBILE, value, true);
                } else if ("email".equals(rule)) {
                    value = replaceMatch(EMAIL, value, false);
                }
            }
            return value;
        }

        private String maskVariable(String name, String raw) {
            String value = raw == null ? "" : raw;
            String rule = configuration.getVariableRules().get(name);
            if (rule == null || value.isEmpty()) {
                return value;
            }
            if ("password".equals(rule)) {
                return "******";
            }
            if ("mobile".equals(rule)) {
                return maskMobile(value);
            }
            if ("email".equals(rule)) {
                int at = value.indexOf('@');
                if (at <= 1) {
                    return "***";
                }
                return value.substring(0, 1) + "***" + value.substring(at);
            }
            if ("partial".equals(rule)) {
                if (value.length() <= 4) {
                    return "****";
                }
                return value.substring(0, 2) + "****" + value.substring(value.length() - 2);
            }
            return value;
        }

        private String replaceGroup(Pattern pattern, String value, int groupIndex, String replacement) {
            Matcher matcher = pattern.matcher(value);
            StringBuffer buffer = new StringBuffer();
            while (matcher.find()) {
                StringBuilder rebuilt = new StringBuilder();
                for (int index = 1; index <= matcher.groupCount(); index++) {
                    rebuilt.append(index == groupIndex ? replacement : matcher.group(index));
                }
                matcher.appendReplacement(buffer, Matcher.quoteReplacement(rebuilt.toString()));
            }
            matcher.appendTail(buffer);
            return buffer.toString();
        }

        private String replaceMatch(Pattern pattern, String value, boolean mobile) {
            Matcher matcher = pattern.matcher(value);
            StringBuffer buffer = new StringBuffer();
            while (matcher.find()) {
                String replacement = mobile
                        ? maskMobile(matcher.group())
                        : maskEmail(matcher.group(1), matcher.group(2));
                matcher.appendReplacement(buffer, Matcher.quoteReplacement(replacement));
            }
            matcher.appendTail(buffer);
            return buffer.toString();
        }

        private String maskMobile(String mobile) {
            if (mobile.length() < 7) {
                return "******";
            }
            return mobile.substring(0, 3) + "****" + mobile.substring(mobile.length() - 4);
        }

        private String maskEmail(String local, String domain) {
            if (local.length() <= 1) {
                return "***@" + domain;
            }
            return local.substring(0, 1) + "***@" + domain;
        }
    }
}
