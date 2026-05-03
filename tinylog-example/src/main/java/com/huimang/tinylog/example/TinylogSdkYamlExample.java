package com.huimang.tinylog.example;

import com.huimang.tinylog.sdk.Logger;
import com.huimang.tinylog.sdk.LoggerFactory;
import com.huimang.tinylog.sdk.LogContext;

/**
 * Demonstrates the YAML-backed TinyLog Java configuration module.
 */
public final class TinylogSdkYamlExample {
    private TinylogSdkYamlExample() {
    }

    /**
     * Writes one short mixed-level session to both console and file outputs.
     */
    public static void main(String[] args) throws Exception {
        try (LoggerFactory factory = LoggerFactory.loadDefault()) {
            Logger logger = factory.getLogger(TinylogSdkYamlExample.class);
            LogContext.put("requestId", "REQ-20260503-0001");
            LogContext.put("userId", "user-95270086");
            logger.trace("trace payload password=super-secret");
            logger.debug("debug payload phone=13812345678");
            logger.info("checkout completed for user email=tinylog@example.com");
            logger.warn("inventory is low for sku=SKU-001");
            logger.error("payment failed for order=ORD-0007", new IllegalStateException("upstream timeout"));
            LogContext.clear();
        }
    }
}
