package com.huimang.tinylog.core.model;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/**
 * Represents one logical log event in the TinyLog domain model.
 */
public final class LogRecord {
    private final long timestampMillis;
    private final LogLevel level;
    private final String source;
    private final String context;
    private final String message;
    private final Map<String, String> attributes;

    /**
     * Creates an immutable log record that can be stored, streamed, or queried.
     */
    public LogRecord(long timestampMillis,
            LogLevel level,
            String source,
            String context,
            String message,
            Map<String, String> attributes) {
        this.timestampMillis = timestampMillis;
        this.level = Objects.requireNonNull(level, "level");
        this.source = Objects.requireNonNull(source, "source");
        this.context = Objects.requireNonNull(context, "context");
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
     * Returns the business source that produced the event.
     */
    public String getSource() {
        return source;
    }

    /**
     * Returns the business context associated with the event.
     */
    public String getContext() {
        return context;
    }

    /**
     * Returns the legacy logger-style source name.
     *
     * @deprecated Prefer {@link #getSource()} so the core model stays language-neutral.
     */
    @Deprecated
    public String getLoggerName() {
        return getSource();
    }

    /**
     * Returns the legacy thread-style context name.
     *
     * @deprecated Prefer {@link #getContext()} so the core model stays language-neutral.
     */
    @Deprecated
    public String getThreadName() {
        return getContext();
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
