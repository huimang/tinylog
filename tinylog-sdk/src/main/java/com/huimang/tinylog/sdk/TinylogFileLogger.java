package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.model.LogLevel;
import java.util.Objects;

/**
 * Persists SDK logging calls into one TinyLog file through the shared file logger factory.
 */
public final class TinylogFileLogger implements Logger {
    private final String name;
    private final TinylogFileLoggerFactory factory;

    /**
     * Creates one file-backed logger for the provided logical name.
     */
    TinylogFileLogger(String name, TinylogFileLoggerFactory factory) {
        this.name = Objects.requireNonNull(name, "name");
        this.factory = Objects.requireNonNull(factory, "factory");
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
