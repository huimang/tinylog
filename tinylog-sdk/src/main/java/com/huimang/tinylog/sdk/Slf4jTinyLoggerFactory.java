package com.huimang.tinylog.sdk;

import java.util.Objects;
import org.slf4j.ILoggerFactory;
import org.slf4j.LoggerFactory;

/**
 * Resolves TinyLog loggers from the SLF4J 2.x logging ecosystem.
 */
public final class Slf4jTinyLoggerFactory implements TinyLoggerFactory {
    private final ILoggerFactory delegate;

    /**
     * Creates a factory backed by the active global SLF4J provider.
     */
    public Slf4jTinyLoggerFactory() {
        this(LoggerFactory.getILoggerFactory());
    }

    /**
     * Creates a factory backed by an explicit SLF4J logger factory.
     */
    public Slf4jTinyLoggerFactory(ILoggerFactory delegate) {
        this.delegate = Objects.requireNonNull(delegate, "delegate");
    }

    /**
     * Returns the wrapped SLF4J factory for integration-oriented scenarios.
     */
    public ILoggerFactory getDelegate() {
        return delegate;
    }

    @Override
    public TinyLogger getLogger(String name) {
        return new Slf4jTinyLogger(delegate.getLogger(name));
    }
}
