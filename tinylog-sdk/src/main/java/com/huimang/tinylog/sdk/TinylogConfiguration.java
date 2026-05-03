package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.model.LogLevel;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

/**
 * Holds the parsed Java logging configuration used by the TinyLog SDK runtime.
 */
public final class TinylogConfiguration {
    private final LogLevel rootLevel;
    private final List<String> rootAppenders;
    private final Map<String, TinylogAppenderConfiguration> appenders;
    private final TinylogMaskingConfiguration masking;

    /**
     * Creates one immutable TinyLog runtime configuration.
     */
    public TinylogConfiguration(LogLevel rootLevel,
            List<String> rootAppenders,
            Map<String, TinylogAppenderConfiguration> appenders,
            TinylogMaskingConfiguration masking) {
        this.rootLevel = Objects.requireNonNull(rootLevel, "rootLevel");
        this.rootAppenders = Collections.unmodifiableList(new ArrayList<String>(rootAppenders));
        this.appenders = Collections.unmodifiableMap(
                new LinkedHashMap<String, TinylogAppenderConfiguration>(appenders));
        this.masking = Objects.requireNonNull(masking, "masking");
    }

    /**
     * Returns the minimum enabled severity for the logger runtime.
     */
    public LogLevel getRootLevel() {
        return rootLevel;
    }

    /**
     * Returns the appender names attached to the root logger.
     */
    public List<String> getRootAppenders() {
        return rootAppenders;
    }

    /**
     * Returns the named appender definitions.
     */
    public Map<String, TinylogAppenderConfiguration> getAppenders() {
        return appenders;
    }

    /**
     * Returns the configured masking behavior.
     */
    public TinylogMaskingConfiguration getMasking() {
        return masking;
    }

    /**
     * Returns the names of all configured appenders.
     */
    Set<String> appenderNames() {
        return appenders.keySet();
    }
}
