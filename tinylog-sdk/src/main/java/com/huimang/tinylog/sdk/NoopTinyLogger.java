package com.huimang.tinylog.sdk;

import java.util.Objects;

/**
 * Provides a safe default logger that accepts calls but emits nothing.
 */
public final class NoopTinyLogger implements TinyLogger {
    private final String name;

    /**
     * Creates a no-op logger for the provided logical name.
     */
    public NoopTinyLogger(String name) {
        this.name = Objects.requireNonNull(name, "name");
    }

    @Override
    /**
     * Returns the logical logger name.
     */
    public String getName() {
        return name;
    }

    @Override
    /**
     * Ignores trace events in the scaffold implementation.
     */
    public void trace(String message) {
    }

    @Override
    /**
     * Ignores debug events in the scaffold implementation.
     */
    public void debug(String message) {
    }

    @Override
    /**
     * Ignores info events in the scaffold implementation.
     */
    public void info(String message) {
    }

    @Override
    /**
     * Ignores warning events in the scaffold implementation.
     */
    public void warn(String message) {
    }

    @Override
    /**
     * Ignores error events in the scaffold implementation.
     */
    public void error(String message) {
    }

    @Override
    /**
     * Ignores error events with failure context in the scaffold implementation.
     */
    public void error(String message, Throwable throwable) {
    }
}
