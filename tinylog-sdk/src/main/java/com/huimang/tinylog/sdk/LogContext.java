package com.huimang.tinylog.sdk;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Stores per-thread business variables that can be rendered by the TinyLog pattern formatter.
 */
public final class LogContext {
    private static final ThreadLocal<Map<String, String>> VARIABLES =
            new ThreadLocal<Map<String, String>>() {
                @Override
                protected Map<String, String> initialValue() {
                    return new LinkedHashMap<String, String>();
                }
            };

    private LogContext() {
    }

    /**
     * Associates one variable with the current thread.
     */
    public static void put(String key, String value) {
        if (key == null) {
            throw new IllegalArgumentException("key must not be null");
        }
        if (value == null) {
            VARIABLES.get().remove(key);
            return;
        }
        VARIABLES.get().put(key, value);
    }

    /**
     * Removes one variable from the current thread.
     */
    public static void remove(String key) {
        if (key == null) {
            return;
        }
        VARIABLES.get().remove(key);
    }

    /**
     * Clears all variables from the current thread.
     */
    public static void clear() {
        VARIABLES.get().clear();
    }

    /**
     * Returns an immutable snapshot of the current thread variables.
     */
    static Map<String, String> snapshot() {
        return Collections.unmodifiableMap(new LinkedHashMap<String, String>(VARIABLES.get()));
    }
}
