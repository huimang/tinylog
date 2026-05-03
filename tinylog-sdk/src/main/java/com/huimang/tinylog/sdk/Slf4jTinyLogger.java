package com.huimang.tinylog.sdk;

import java.util.Objects;

/**
 * Adapts one SLF4J logger to the TinyLog business-facing logger contract.
 */
public final class Slf4jTinyLogger implements com.huimang.tinylog.sdk.Logger {
    private final org.slf4j.Logger delegate;

    /**
     * Creates a TinyLog logger backed by one SLF4J logger instance.
     */
    public Slf4jTinyLogger(org.slf4j.Logger delegate) {
        this.delegate = Objects.requireNonNull(delegate, "delegate");
    }

    /**
     * Returns the wrapped SLF4J logger for integration-oriented scenarios.
     */
    public org.slf4j.Logger getDelegate() {
        return delegate;
    }

    @Override
    public String getName() {
        return delegate.getName();
    }

    @Override
    public void trace(String message) {
        delegate.trace(message);
    }

    @Override
    public void debug(String message) {
        delegate.debug(message);
    }

    @Override
    public void info(String message) {
        delegate.info(message);
    }

    @Override
    public void warn(String message) {
        delegate.warn(message);
    }

    @Override
    public void error(String message) {
        delegate.error(message);
    }

    @Override
    public void error(String message, Throwable throwable) {
        delegate.error(message, throwable);
    }
}
