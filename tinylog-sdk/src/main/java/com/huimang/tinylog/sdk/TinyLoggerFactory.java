package com.huimang.tinylog.sdk;

/**
 * Resolves business-facing loggers without exposing backend details.
 */
public interface TinyLoggerFactory {
    /**
     * Returns a logger for the provided logical name.
     */
    TinyLogger getLogger(String name);

    /**
     * Returns a logger derived from a Java type name.
     */
    default TinyLogger getLogger(Class<?> type) {
        return getLogger(type.getName());
    }
}
