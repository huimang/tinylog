package com.huimang.tinylog.sdk;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;

/**
 * Holds masking rules for message bodies and business variables.
 */
public final class TinylogMaskingConfiguration {
    private final Set<String> contentRules;
    private final Map<String, String> variableRules;

    /**
     * Creates one immutable masking definition.
     */
    public TinylogMaskingConfiguration(Set<String> contentRules, Map<String, String> variableRules) {
        this.contentRules = Collections.unmodifiableSet(new LinkedHashSet<String>(contentRules));
        this.variableRules = Collections.unmodifiableMap(new LinkedHashMap<String, String>(variableRules));
    }

    /**
     * Returns the enabled content masking rules.
     */
    public Set<String> getContentRules() {
        return contentRules;
    }

    /**
     * Returns variable-specific masking rules keyed by variable name.
     */
    public Map<String, String> getVariableRules() {
        return variableRules;
    }
}
