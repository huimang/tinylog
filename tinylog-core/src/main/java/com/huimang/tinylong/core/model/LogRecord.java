package com.huimang.tinylong.core.model;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/**
 * Represents one logical log event in the tinylog domain model.
 */
public final class LogRecord {
    private final long timestampMillis;
    private final LogLevel level;
    private final String loggerName;
    private final String threadName;
    private final String message;
    private final Map<String, String> attributes;

    /**
     * Creates an immutable log record that can be stored, streamed, or queried.
     */
    public LogRecord(long timestampMillis,
            LogLevel level,
            String loggerName,
            String threadName,
            String message,
            Map<String, String> attributes) {
        this.timestampMillis = timestampMillis;
        this.level = Objects.requireNonNull(level, "level");
        this.loggerName = Objects.requireNonNull(loggerName, "loggerName");
        this.threadName = Objects.requireNonNull(threadName, "threadName");
        this.message = Objects.requireNonNull(message, "message");
        this.attributes = Collections.unmodifiableMap(new LinkedHashMap<String, String>(
                attributes == null ? Collections.<String, String>emptyMap() : attributes));
    }

    /**
     * Returns the event timestamp in milliseconds.
     */
    public long getTimestampMillis() {
        return timestampMillis;
    }

    /**
     * Returns the business severity of the event.
     */
    public LogLevel getLevel() {
        return level;
    }

    /**
     * Returns the logical logger name used by the caller.
     */
    public String getLoggerName() {
        return loggerName;
    }

    /**
     * Returns the thread that produced the event.
     */
    public String getThreadName() {
        return threadName;
    }

    /**
     * Returns the main business message of the event.
     */
    public String getMessage() {
        return message;
    }

    /**
     * Returns immutable event attributes for filtering or display.
     */
    public Map<String, String> getAttributes() {
        return attributes;
    }
}
