package com.huimang.tinylong.sdk;

/**
 * Creates no-op loggers as a placeholder until a real backend is wired in.
 */
public final class NoopTinyLoggerFactory implements TinyLoggerFactory {
    @Override
    /**
     * Returns a logger that preserves the requested business name.
     */
    public TinyLogger getLogger(String name) {
        return new NoopTinyLogger(name);
    }
}
