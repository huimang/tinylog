package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.io.CompressionAlgorithm;
import com.huimang.tinylog.core.model.LogLevel;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import org.yaml.snakeyaml.Yaml;

/**
 * Loads TinyLog runtime configuration from YAML or properties resources.
 */
public final class TinylogConfigurationLoader {
    private static final List<String> DEFAULT_RESOURCE_NAMES = Collections.unmodifiableList(
            Arrays.asList("tinylog.yml", "tinylog.yaml", "tinylog.properties"));
    private static final String DEFAULT_PATTERN = "[%logger] [%context] %message";

    /**
     * Loads the first matching default configuration resource from the context class loader.
     */
    public TinylogConfiguration loadDefault() throws IOException {
        ClassLoader classLoader = Thread.currentThread().getContextClassLoader();
        if (classLoader == null) {
            classLoader = TinylogConfigurationLoader.class.getClassLoader();
        }
        for (String resourceName : DEFAULT_RESOURCE_NAMES) {
            InputStream input = classLoader.getResourceAsStream(resourceName);
            if (input == null) {
                continue;
            }
            try {
                return resourceName.endsWith(".properties")
                        ? parseProperties(input)
                        : parseYaml(input);
            } finally {
                input.close();
            }
        }
        throw new IOException("unable to find tinylog.yml, tinylog.yaml, or tinylog.properties on the classpath");
    }

    /**
     * Loads one explicit configuration file.
     */
    public TinylogConfiguration load(Path path) throws IOException {
        String fileName = path.getFileName().toString().toLowerCase(Locale.ROOT);
        InputStream input = Files.newInputStream(path);
        try {
            return fileName.endsWith(".properties") ? parseProperties(input) : parseYaml(input);
        } finally {
            input.close();
        }
    }

    private TinylogConfiguration parseYaml(InputStream input) {
        Object raw = new Yaml().load(input);
        if (!(raw instanceof Map)) {
            throw new IllegalArgumentException("tinylog yaml root must be a mapping");
        }
        Map<?, ?> root = (Map<?, ?>) raw;
        Object tinylogNode = root.containsKey("tinylog") ? root.get("tinylog") : root;
        if (!(tinylogNode instanceof Map)) {
            throw new IllegalArgumentException("tinylog yaml must contain a tinylog mapping");
        }
        return buildConfiguration(asStringKeyMap((Map<?, ?>) tinylogNode));
    }

    private TinylogConfiguration parseProperties(InputStream input) throws IOException {
        Properties properties = new Properties();
        properties.load(input);
        Map<String, Object> tinylog = new LinkedHashMap<String, Object>();
        Map<String, Object> root = new LinkedHashMap<String, Object>();
        Map<String, Object> appenders = new LinkedHashMap<String, Object>();
        Map<String, Object> masking = new LinkedHashMap<String, Object>();
        tinylog.put("root", root);
        tinylog.put("appenders", appenders);
        tinylog.put("masking", masking);

        for (String key : properties.stringPropertyNames()) {
            String value = properties.getProperty(key);
            if (!key.startsWith("tinylog.")) {
                continue;
            }
            String suffix = key.substring("tinylog.".length());
            if (suffix.startsWith("root.")) {
                applyProperty(root, suffix.substring("root.".length()), value);
            } else if (suffix.startsWith("appender.")) {
                String appenderSuffix = suffix.substring("appender.".length());
                int separator = appenderSuffix.indexOf('.');
                if (separator < 0) {
                    throw new IllegalArgumentException("invalid appender property: " + key);
                }
                String appenderName = appenderSuffix.substring(0, separator);
                Map<String, Object> appender = ensureNestedMap(appenders, appenderName);
                applyProperty(appender, appenderSuffix.substring(separator + 1), value);
            } else if (suffix.startsWith("masking.")) {
                applyProperty(masking, suffix.substring("masking.".length()), value);
            }
        }

        return buildConfiguration(tinylog);
    }

    private TinylogConfiguration buildConfiguration(Map<String, Object> tinylog) {
        Map<String, Object> root = nestedMap(tinylog, "root");
        Map<String, Object> appendersNode = nestedMap(tinylog, "appenders");
        Map<String, Object> maskingNode = nestedMap(tinylog, "masking");

        Map<String, TinylogAppenderConfiguration> appenders =
                new LinkedHashMap<String, TinylogAppenderConfiguration>();
        for (Map.Entry<String, Object> entry : appendersNode.entrySet()) {
            if (!(entry.getValue() instanceof Map)) {
                continue;
            }
            appenders.put(entry.getKey(), buildAppender(entry.getKey(), asStringKeyMap((Map<?, ?>) entry.getValue())));
        }
        if (appenders.isEmpty()) {
            throw new IllegalArgumentException("at least one tinylog appender must be configured");
        }

        LogLevel rootLevel = parseLevel(stringValue(root.get("level"), "INFO"));
        List<String> rootAppenders = stringList(root.get("appenders"));
        if (rootAppenders.isEmpty()) {
            rootAppenders = new ArrayList<String>(appenders.keySet());
        }
        for (String appenderName : rootAppenders) {
            if (!appenders.containsKey(appenderName)) {
                throw new IllegalArgumentException("undefined root appender: " + appenderName);
            }
        }
        return new TinylogConfiguration(rootLevel, rootAppenders, appenders, buildMasking(maskingNode));
    }

    private TinylogAppenderConfiguration buildAppender(String name, Map<String, Object> node) {
        TinylogAppenderConfiguration.Type type = parseAppenderType(stringValue(node.get("type"), null));
        LogLevel level = parseLevel(stringValue(node.get("level"), "TRACE"));
        String pattern = stringValue(node.get("pattern"), DEFAULT_PATTERN);
        if (type == TinylogAppenderConfiguration.Type.CONSOLE) {
            return new TinylogAppenderConfiguration(
                    name,
                    type,
                    level,
                    pattern,
                    parseConsoleTarget(stringValue(node.get("target"), "SYSTEM_OUT")),
                    null,
                    null,
                    null,
                    null,
                    0,
                    false,
                    0L,
                    0);
        }
        String fileName = stringValue(node.get("fileName"), null);
        if (isBlank(fileName)) {
            throw new IllegalArgumentException("file appender " + name + " requires fileName");
        }
        Map<String, Object> policies = nestedMap(node, "policies");
        Map<String, Object> sizePolicy = nestedMap(policies, "size");
        Map<String, Object> strategy = nestedMap(node, "strategy");
        return new TinylogAppenderConfiguration(
                name,
                type,
                level,
                pattern,
                null,
                fileName,
                stringValue(node.get("filePattern"), null),
                parseFileFormat(stringValue(node.get("format"), "text")),
                parseCompressionAlgorithm(stringValue(node.get("compression"), "gzip")),
                intValue(node.get("trunkSizeKb"), 512),
                booleanValue(node.get("splitByLevel"), false),
                parseSizeBytes(stringValue(sizePolicy.get("size"), null)),
                intValue(strategy.get("max"), 5));
    }

    private TinylogMaskingConfiguration buildMasking(Map<String, Object> maskingNode) {
        Set<String> contentRules = new LinkedHashSet<String>();
        for (String rule : stringList(maskingNode.get("contentRules"))) {
            contentRules.add(rule.toLowerCase(Locale.ROOT));
        }
        Map<String, String> variableRules = new LinkedHashMap<String, String>();
        Map<String, Object> rawVariableRules = nestedMap(maskingNode, "variableRules");
        for (Map.Entry<String, Object> entry : rawVariableRules.entrySet()) {
            variableRules.put(entry.getKey(), stringValue(entry.getValue(), "").toLowerCase(Locale.ROOT));
        }
        return new TinylogMaskingConfiguration(contentRules, variableRules);
    }

    private void applyProperty(Map<String, Object> root, String suffix, String value) {
        String[] parts = suffix.split("\\.");
        Map<String, Object> current = root;
        for (int index = 0; index < parts.length - 1; index++) {
            current = ensureNestedMap(current, parts[index]);
        }
        current.put(parts[parts.length - 1], normalizePropertyValue(value));
    }

    private Map<String, Object> ensureNestedMap(Map<String, Object> root, String key) {
        Object existing = root.get(key);
        if (existing instanceof Map) {
            return castMap(existing);
        }
        Map<String, Object> created = new LinkedHashMap<String, Object>();
        root.put(key, created);
        return created;
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> castMap(Object value) {
        return (Map<String, Object>) value;
    }

    private Map<String, Object> nestedMap(Map<String, Object> root, String key) {
        Object value = root.get(key);
        if (value instanceof Map) {
            return asStringKeyMap((Map<?, ?>) value);
        }
        return new LinkedHashMap<String, Object>();
    }

    private Map<String, Object> asStringKeyMap(Map<?, ?> source) {
        Map<String, Object> converted = new LinkedHashMap<String, Object>();
        for (Map.Entry<?, ?> entry : source.entrySet()) {
            converted.put(String.valueOf(entry.getKey()), entry.getValue());
        }
        return converted;
    }

    private Object normalizePropertyValue(String value) {
        if (value == null) {
            return null;
        }
        String trimmed = value.trim();
        if (trimmed.indexOf(',') >= 0) {
            List<String> items = new ArrayList<String>();
            for (String item : trimmed.split(",")) {
                if (!isBlank(item)) {
                    items.add(item.trim());
                }
            }
            return items;
        }
        return trimmed;
    }

    private TinylogAppenderConfiguration.Type parseAppenderType(String value) {
        if (isBlank(value)) {
            throw new IllegalArgumentException("tinylog appender type must be provided");
        }
        String normalized = value.trim().toLowerCase(Locale.ROOT);
        if ("console".equals(normalized)) {
            return TinylogAppenderConfiguration.Type.CONSOLE;
        }
        if ("file".equals(normalized)) {
            return TinylogAppenderConfiguration.Type.FILE;
        }
        throw new IllegalArgumentException("unsupported tinylog appender type: " + value);
    }

    private TinylogAppenderConfiguration.ConsoleTarget parseConsoleTarget(String value) {
        if (isBlank(value)) {
            return TinylogAppenderConfiguration.ConsoleTarget.SYSTEM_OUT;
        }
        String normalized = value.trim().toUpperCase(Locale.ROOT);
        if ("SYSTEM_ERR".equals(normalized)) {
            return TinylogAppenderConfiguration.ConsoleTarget.SYSTEM_ERR;
        }
        return TinylogAppenderConfiguration.ConsoleTarget.SYSTEM_OUT;
    }

    private TinylogAppenderConfiguration.FileFormat parseFileFormat(String value) {
        if (isBlank(value)) {
            return TinylogAppenderConfiguration.FileFormat.TEXT;
        }
        String normalized = value.trim().toUpperCase(Locale.ROOT);
        return TinylogAppenderConfiguration.FileFormat.valueOf(normalized);
    }

    private CompressionAlgorithm parseCompressionAlgorithm(String value) {
        if (isBlank(value)) {
            return CompressionAlgorithm.GZIP;
        }
        return CompressionAlgorithm.valueOf(value.trim().toUpperCase(Locale.ROOT));
    }

    private LogLevel parseLevel(String value) {
        if (isBlank(value)) {
            return LogLevel.INFO;
        }
        return LogLevel.valueOf(value.trim().toUpperCase(Locale.ROOT));
    }

    private List<String> stringList(Object value) {
        if (value == null) {
            return new ArrayList<String>();
        }
        if (value instanceof List) {
            List<?> raw = (List<?>) value;
            List<String> result = new ArrayList<String>(raw.size());
            for (Object item : raw) {
                if (item != null && !isBlank(String.valueOf(item))) {
                    result.add(String.valueOf(item).trim());
                }
            }
            return result;
        }
        String stringValue = String.valueOf(value);
        if (isBlank(stringValue)) {
            return new ArrayList<String>();
        }
        List<String> result = new ArrayList<String>();
        for (String item : stringValue.split(",")) {
            if (!isBlank(item)) {
                result.add(item.trim());
            }
        }
        return result;
    }

    private String stringValue(Object value, String defaultValue) {
        if (value == null) {
            return defaultValue;
        }
        String converted = String.valueOf(value);
        return isBlank(converted) ? defaultValue : converted.trim();
    }

    private boolean booleanValue(Object value, boolean defaultValue) {
        if (value == null) {
            return defaultValue;
        }
        return Boolean.parseBoolean(String.valueOf(value));
    }

    private int intValue(Object value, int defaultValue) {
        if (value == null || isBlank(String.valueOf(value))) {
            return defaultValue;
        }
        return Integer.parseInt(String.valueOf(value).trim());
    }

    private long parseSizeBytes(String value) {
        if (isBlank(value)) {
            return 0L;
        }
        String normalized = value.trim().toUpperCase(Locale.ROOT);
        long multiplier = 1L;
        if (normalized.endsWith("KB")) {
            multiplier = 1024L;
            normalized = normalized.substring(0, normalized.length() - 2);
        } else if (normalized.endsWith("MB")) {
            multiplier = 1024L * 1024L;
            normalized = normalized.substring(0, normalized.length() - 2);
        } else if (normalized.endsWith("GB")) {
            multiplier = 1024L * 1024L * 1024L;
            normalized = normalized.substring(0, normalized.length() - 2);
        } else if (normalized.endsWith("B")) {
            normalized = normalized.substring(0, normalized.length() - 1);
        }
        return Long.parseLong(normalized.trim()) * multiplier;
    }

    private boolean isBlank(String value) {
        return value == null || value.trim().isEmpty();
    }
}
