package com.huimang.tinylog.sdk;

/**
 * Defines the business-facing logging contract used by Java applications.
 */
public interface Logger {
    /**
     * Returns the logical logger name.
     */
    String getName();

    /**
     * Records a trace-level business event.
     */
    void trace(String message);

    /**
     * Records a debug-level business event.
     */
    void debug(String message);

    /**
     * Records an info-level business event.
     */
    void info(String message);

    /**
     * Records a warning-level business event.
     */
    void warn(String message);

    /**
     * Records an error-level business event.
     */
    void error(String message);

    /**
     * Records an error-level event together with failure context.
     */
    void error(String message, Throwable throwable);
}
