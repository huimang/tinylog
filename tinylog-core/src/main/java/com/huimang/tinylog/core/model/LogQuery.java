package com.huimang.tinylog.core.model;

/**
 * Describes a business-oriented filter for browsing or searching log data.
 */
public final class LogQuery {
    private final Long startTimestampMillis;
    private final Long endTimestampMillis;
    private final LogLevel minimumLevel;
    private final String keyword;

    private LogQuery(Builder builder) {
        this.startTimestampMillis = builder.startTimestampMillis;
        this.endTimestampMillis = builder.endTimestampMillis;
        this.minimumLevel = builder.minimumLevel;
        this.keyword = builder.keyword;
    }

    /**
     * Returns the inclusive start of the time range, if present.
     */
    public Long getStartTimestampMillis() {
        return startTimestampMillis;
    }

    /**
     * Returns the inclusive end of the time range, if present.
     */
    public Long getEndTimestampMillis() {
        return endTimestampMillis;
    }

    /**
     * Returns the minimum accepted severity, if present.
     */
    public LogLevel getMinimumLevel() {
        return minimumLevel;
    }

    /**
     * Returns the keyword used for message matching, if present.
     */
    public String getKeyword() {
        return keyword;
    }

    /**
     * Starts a fluent query builder.
     */
    public static Builder builder() {
        return new Builder();
    }

    /**
     * Builds a query without exposing storage-specific details to callers.
     */
    public static final class Builder {
        private Long startTimestampMillis;
        private Long endTimestampMillis;
        private LogLevel minimumLevel;
        private String keyword;

        private Builder() {
        }

        /**
         * Sets the inclusive start timestamp.
         */
        public Builder startTimestampMillis(Long value) {
            this.startTimestampMillis = value;
            return this;
        }

        /**
         * Sets the inclusive end timestamp.
         */
        public Builder endTimestampMillis(Long value) {
            this.endTimestampMillis = value;
            return this;
        }

        /**
         * Sets the minimum accepted log level.
         */
        public Builder minimumLevel(LogLevel value) {
            this.minimumLevel = value;
            return this;
        }

        /**
         * Sets a keyword for message matching.
         */
        public Builder keyword(String value) {
            this.keyword = value;
            return this;
        }

        /**
         * Creates the immutable query instance.
         */
        public LogQuery build() {
            return new LogQuery(this);
        }
    }
}
