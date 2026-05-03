package com.huimang.tinylog.sdk;

/**
 * Resolves business-facing loggers without exposing backend details.
 */
public interface TinyLoggerFactory {
    /**
     * Returns a logger for the provided logical name.
     */
    Logger getLogger(String name);

    /**
     * Returns a logger derived from a Java type name.
     */
    default Logger getLogger(Class<?> type) {
        return getLogger(type.getName());
    }
}
