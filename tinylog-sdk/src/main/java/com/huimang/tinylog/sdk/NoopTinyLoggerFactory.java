package com.huimang.tinylog.sdk;

/**
 * Creates no-op loggers as a placeholder until a real backend is wired in.
 */
public final class NoopTinyLoggerFactory implements TinyLoggerFactory {
    @Override
    /**
     * Returns a logger that preserves the requested business name.
     */
    public Logger getLogger(String name) {
        return new NoopTinyLogger(name);
    }
}
