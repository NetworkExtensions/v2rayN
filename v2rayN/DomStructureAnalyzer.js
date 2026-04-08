/**
 * DomStructureAnalyzer.js
 * Extracted from xiaohongshu.com/explore
 * Contains DOM structure analysis and monitoring utilities
 */

// ============================================================================
// Resource Loading Error Handler with Retry Logic
// ============================================================================
const ResourceErrorHandler = {
    FORMULA_ASSETS_LOAD_ERROR: "FORMULA_ASSETS_LOAD_ERROR",

    // Utility to get stored error data from localStorage
    getStoredErrors() {
        const stored = localStorage.getItem(this.FORMULA_ASSETS_LOAD_ERROR);
        return stored ? JSON.parse(stored) : [];
    },

    // Save error data to localStorage or send to tracker
    saveError(errorData) {
        try {
            const tracker = this.getTracker();
            if (tracker) {
                tracker(errorData);
            } else {
                const errors = this.getStoredErrors();
                if (errors.length >= 1000) return;
                errors.push(errorData);
                localStorage.setItem(this.FORMULA_ASSETS_LOAD_ERROR, JSON.stringify(errors));
            }
        } catch (e) {
            console.error({ error: e, errorData });
        }
    },

    // Get the tracker function from window.eaglet or window.insight
    getTracker() {
        if (typeof window === "undefined") return null;
        const push = window.eaglet?.push || window.insight?.push;
        if (push) {
            return (data) => push(this.enrichData(data), "ApmXrayTracker");
        }
        return null;
    },

    // Enrich tracking data with metadata
    enrichData(data) {
        const artifactVersion = "4.0.16";
        const packageName = "xhs-pc-web";
        const packageVersion = "6.5.0";

        return Object.assign({}, data, {
            context_artifactName: "formula",
            context_artifactVersion: artifactVersion || "unknown",
            measurement_data: Object.assign({}, data.measurement_data, {
                packageName: packageName || "unknown",
                packageVersion: packageVersion || "unknown"
            })
        });
    },

    // Build retry URL by switching from fe-static to cdn
    buildRetryUrl(originalUrl) {
        const staticDomain = "//fe-static.xhscdn.com";
        if (originalUrl && originalUrl.includes(staticDomain)) {
            return originalUrl.replace(staticDomain, "//cdn.xiaohongshu.com") +
                "?business=fe&scene=feplatform";
        }
        return null;
    },

    // Retry loading a failed resource
    retryResource(url, type, measurementData) {
        const newUrl = this.buildRetryUrl(url);
        if (!newUrl) {
            this.saveError({
                measurement_name: "reload_resource_error",
                measurement_data: Object.assign({}, measurementData, {
                    retryErrorType: "newUrlError",
                    timestamp: String(Date.now())
                })
            });
            return;
        }

        let element;
        if (type === "js") {
            element = document.createElement("script");
            element.src = newUrl;
        } else if (type === "css") {
            element = document.createElement("link");
            element.rel = "stylesheet";
            element.href = newUrl;
        }

        if (element) {
            element.dataset.formulaAssetRetry = "1";
            document.head.appendChild(element);

            element.addEventListener("load", () => {
                this.saveError({
                    measurement_name: "reload_resource_duration",
                    measurement_data: Object.assign({}, measurementData, {
                        duration: Date.now() - new Date(Number(measurementData.timestamp)).getTime(),
                        retryResourceUrl: element.src || element.href
                    })
                });
            });

            element.addEventListener("error", () => {
                this.saveError({
                    measurement_name: "reload_resource_error",
                    measurement_data: {
                        timestamp: String(Date.now()),
                        retryErrorType: "retryOnloadError",
                        retryResourceUrl: element.src || element.href
                    }
                });
            });
        }
    },

    // Initialize error event listener
    init() {
        const resourceList = [
            "resource/js/bundler-runtime.37b4d1be.js",
            "resource/js/vendor-dynamic.218e75f8.js",
            "resource/css/vendor-dynamic.caf3c090.css",
            "resource/js/library-polyfill.29a884fe.js",
            "resource/js/library-axios.4d38c57d.js",
            "resource/js/library-vue.e91ead26.js",
            "resource/js/library-lodash.936df096.js",
            "resource/js/vendor.5bab5213.js",
            "resource/js/index.7bc6aee1.js",
            "resource/css/index.0e131d41.css"
        ];

        window.addEventListener("error", (event) => {
            const target = event.target;
            if (!target) return;

            const url = target.href || target.src;
            if (!url) return;

            const isFormulaCdnRetry = target.dataset?.formulaCdnRetry;
            const isInResourceList = resourceList.some(r => url.includes(r));

            if (!isFormulaCdnRetry && !isInResourceList) return;

            const isAlreadyRetried = target.dataset?.formulaAssetRetry;
            const resourceType = target.tagName === "LINK" ? "css" : "js";

            const errorData = {
                measurement_name: "biz_load_error_count",
                measurement_data: {
                    path: window.location.href,
                    resourceType: resourceType,
                    resourceUrl: url || "-",
                    timestamp: String(Date.now())
                }
            };

            if (!isAlreadyRetried) {
                this.saveError(errorData);
                this.retryResource(url, resourceType, errorData.measurement_data);
            }
        }, true);

        // Process stored errors on page load
        window.addEventListener("load", () => {
            try {
                const tracker = this.getTracker();
                if (!tracker) return;

                const storedErrors = this.getStoredErrors();
                if (storedErrors.length > 0) {
                    for (const error of storedErrors) {
                        tracker(error);
                    }
                }
                localStorage.removeItem(this.FORMULA_ASSETS_LOAD_ERROR);
            } catch (e) {
                console.error(e);
            }
        });
    }
};

// ============================================================================
// First Meaningful Paint (FMP) DOM Observer
// ============================================================================
const FMPObserver = {
    namespace: "__FST__",

    init() {
        const win = window;
        const doc = document;

        try {
            const data = win[this.namespace] = win[this.namespace] || {
                runned: false,
                observer: null,
                mutaRecords: [],
                imgObserver: null,
                imgRecords: [],
                run: this.run.bind(this)
            };

            if (!data.runned) {
                data.run(data);
            }
        } catch (e) {
            console.error("FMP Observer init error:", e);
        }
    },

    run(state) {
        if (state.runned) return;

        const excludedTags = ["HTML", "HEAD", "META", "LINK", "SCRIPT", "STYLE", "NOSCRIPT"];

        if (!window.MutationObserver || !window.performance || !window.performance.now) {
            return;
        }

        state.runned = true;

        // MutationObserver to track DOM changes
        state.observer = new MutationObserver((mutations) => {
            try {
                state.mutaRecords.push({
                    mutations: mutations,
                    startTime: window.performance.now()
                });

                // Process mutations that added visible elements
                mutations
                    .filter(mutation => {
                        const targetName = (mutation.target.nodeName || "").toUpperCase();
                        return mutation.type === "childList" &&
                               targetName &&
                               excludedTags.indexOf(targetName) === -1 &&
                               mutation.addedNodes &&
                               mutation.addedNodes.length;
                    })
                    .forEach(mutation => {
                        Array.from(mutation.addedNodes)
                            .filter(node => {
                                const nodeName = (node.nodeName || "").toUpperCase();
                                return node.nodeType === 1 && // Element node
                                       nodeName === "IMG" &&
                                       node.isConnected &&
                                       !node.closest("[fmp-ignore]") &&
                                       !node.hasAttribute("fmp-ignore");
                            })
                            .forEach(img => {
                                img.addEventListener("load", () => {
                                    try {
                                        const loadTime = window.performance.now();
                                        const src = img.getAttribute("src") || "";

                                        requestAnimationFrame(function checkFrame(now) {
                                            try {
                                                if (img && img.naturalWidth && img.naturalHeight) {
                                                    state.imgRecords.push({
                                                        name: src.split(":")[1] || src,
                                                        responseEnd: now,
                                                        loadTime: loadTime,
                                                        startTime: 0,
                                                        duration: 0,
                                                        type: "loaded"
                                                    });
                                                } else {
                                                    requestAnimationFrame(checkFrame);
                                                }
                                            } catch (e) {}
                                        });
                                    } catch (e) {}
                                });
                            });
                    });
            } catch (e) {}
        });

        state.observer.observe(document, {
            childList: true,
            subtree: true
        });

        // PerformanceObserver to track resource loading
        if (window.PerformanceObserver) {
            state.imgObserver = new PerformanceObserver((list) => {
                try {
                    list.getEntries()
                        .filter(entry =>
                            entry.initiatorType === "img" ||
                            entry.initiatorType === "css" ||
                            entry.initiatorType === "link"
                        )
                        .forEach(entry => {
                            state.imgRecords.push({
                                name: entry.name.split(":")[1] || entry.name,
                                responseEnd: entry.responseEnd,
                                startTime: entry.startTime,
                                duration: entry.duration,
                                type: "entry"
                            });
                        });
                } catch (e) {}
            });

            state.imgObserver.observe({ entryTypes: ["resource"] });
        }
    }
};

// ============================================================================
// CSS-in-JS Style Injection Utilities
// ============================================================================
const StyleInjector = {
    // Injects CSS string into a <style> tag in the document head
    injectCSS(cssString, id = null) {
        const styleEl = document.createElement("style");
        styleEl.type = "text/css";
        if (id) styleEl.id = id;
        styleEl.textContent = cssString;
        document.head.appendChild(styleEl);
        return styleEl;
    },

    // CSS custom properties (variables) for theming
    themeVariables: {
        // Background colors
        "--bg": "rgba(255, 255, 255, 1)",
        "--fill1": "rgba(48, 48, 52, 0.05)",
        "--fill2": "rgba(48, 48, 52, 0.1)",
        "--fill3": "rgba(48, 48, 52, 0.2)",
        "--fill4": "rgba(48, 48, 52, 0.5)",
        "--fill5": "rgba(48, 48, 52, 0.99)",

        // Text colors
        "--title": "rgba(0, 0, 0, 0.8)",
        "--paragraph": "rgba(0, 0, 0, 0.62)",
        "--description": "rgba(0, 0, 0, 0.45)",
        "--always-white": "rgba(255, 255, 255, 1)",

        // Primary/Accent
        "--primary": "rgba(255, 36, 66, 1)",
        "--color-red": "#ff2e4d",
        "--color-white": "#fff",

        // Elevation
        "--elevation-high-background": "rgba(255, 255, 255, 1)",
        "--elevation-high-shadow": "0 8px 24px rgba(0, 0, 0, 0.12)",
        "--elevation-low-shadow": "0 4px 16px rgba(0, 0, 0, 0.08)",

        // Mask/Backdrop
        "--mask-backdrop": "rgba(0, 0, 0, 0.2)",
        "--mask-paper": "rgba(255, 255, 255, 1)"
    }
};

// ============================================================================
// Vue Component Scoped CSS Utilities
// ============================================================================
const VueStyleUtils = {
    // Generate attribute selector for scoped styles (simulating Vue's scoped CSS)
    scopedSelector(attr, element) {
        return `${element}[${attr}]`;
    },

    // Animation keyframes extracted from page
    keyframes: {
        // Fade animation
        fadeIn: `
            @keyframes fadeIn {
                0% { opacity: 0; }
                100% { opacity: 1; }
            }
        `,

        // Staged fade animation
        fadeInStaged: `
            @keyframes fadeInStaged {
                0% { opacity: 0.2; }
                20% { opacity: 0.2; }
                100% { opacity: 1; }
            }
        `,

        // Slide animations
        slideUp: `
            @keyframes slideUp {
                from { transform: translateY(100%); opacity: 0; }
                to { transform: translateY(0); opacity: 1; }
            }
        `,

        slideRight: `
            @keyframes slideRight {
                from { transform: translateX(100%); }
                to { transform: translateX(0); }
            }
        `,

        // Spinner rotation
        spinnerRotate: `
            @keyframes spinnerRotate {
                from { transform: rotate(0deg); }
                to { transform: rotate(360deg); }
            }
        `,

        // Skeleton pulse
        skeletonPulse: `
            @keyframes skeletonPulse {
                0%, 100% { opacity: 0.1; }
                50% { opacity: 0.4; }
            }
        `,

        // Wave animations for avatar
        waveInSide: `
            @keyframes waveInSide {
                0% { transform: scale(1); }
                50% { transform: scale(1.1); }
                100% { transform: scale(1); }
            }
        `,

        waveOutSide: `
            @keyframes waveOutSide {
                0% { transform: scale(1.1); opacity: 0; }
                50% { transform: scale(1.1); opacity: 0; }
                51% { transform: scale(1.1); opacity: 1; }
                75% { border-width: 1px; opacity: 1; }
                100% { transform: scale(1.2); border-width: 0; opacity: 0; }
            }
        `,

        // Search lights effect
        searchLights: `
            @keyframes searchLights {
                0% { left: -20px; top: 0; }
                100% { left: 100%; top: 0; }
            }
        `
    }
};

// ============================================================================
// DOM Element Selectors (Extracted from page structure)
// ============================================================================
const DOMSelectors = {
    // Main app container
    app: "#app",

    // Component selectors with data-v-* attributes (Vue scoped styles)
    components: {
        cookiePreferences: "[data-v-dd88d2fb]",
        cookieBanner: "[data-v-04ef0ea4]",
        redsIcon: "[data-v-55b36ac6]",
        redsMask: "[data-v-54eb1bb4]",
        redsModal: "[data-v-f9867710]",
        redsDialogPaper: "[data-v-fa27eb18]",
        redsText: "[data-v-a847e398]",
        redsDialogButton: "[data-v-93fd2b7e]",
        redsDialogButtonGroup: "[data-v-f6941e10]",
        redsAvatar: ".reds-avatar",
        redsButtonNew: ".reds-button-new",
        redsToast: "[data-v-286985d4]",
        redsSticky: "[data-v-73916935]",
        redsUploader: ".reds-uploader",
        redsTabPaneList: ".reds-tab-pane-list",
        divider: "[data-v-39ecb380]",
        badge: "[data-v-0755b6ef]",
        dropdown: "[data-v-87beb1be]",
        messageContainer: "[data-v-2a210922]",
        dotToggle: "[data-v-147b1aa0]",
        skeleton: "[data-v-2001e7e1]",
        errorPage: "[data-v-6df8bfcd]",
        noteCard: "[data-v-56cede88]",
        backIconTip: "[data-v-d33abd98]",
        closeBox: "[data-v-6c30aded]",
        courseVideo: "[data-v-07a2fed7]",
        accessModal: "[data-v-5f656e39]",
        aiMessage: "[data-v-4e333c32]",
        userMessage: "[data-v-2c437084]",
        aiChatRenderer: "[data-v-3f9fd7fc]",
        aiImageContainer: "[data-v-4a69e01e]"
    },

    // CSS class patterns
    patterns: {
        redsComponent: /^reds-/,
        dataAttribute: /^data-v-/,
        cookieBanner: /^cookie-/,
        skeleton: /^skeleton-/
    }
};

// ============================================================================
// Component Structure Analyzer
// ============================================================================
const ComponentAnalyzer = {
    // Analyze a DOM element and return its component hierarchy
    analyzeElement(element) {
        const info = {
            tagName: element.tagName,
            classes: Array.from(element.classList),
            dataAttributes: {},
            hasVueScope: false,
            vueScopeId: null,
            children: []
        };

        // Extract data-v-* attributes (Vue scoped style identifiers)
        for (const attr of element.attributes) {
            if (attr.name.startsWith("data-v-")) {
                info.dataAttributes[attr.name] = attr.value;
                info.hasVueScope = true;
                info.vueScopeId = attr.name;
            }
        }

        return info;
    },

    // Recursively analyze component tree
    analyzeTree(root, maxDepth = 5, currentDepth = 0) {
        if (currentDepth >= maxDepth) return null;

        const info = this.analyzeElement(root);

        for (const child of root.children) {
            const childInfo = this.analyzeTree(child, maxDepth, currentDepth + 1);
            if (childInfo) {
                info.children.push(childInfo);
            }
        }

        return info;
    },

    // Find all elements matching a Vue scope ID
    findByScopeId(scopeId) {
        return document.querySelectorAll(`[${scopeId}]`);
    },

    // Get computed styles for an element
    getComputedStyleInfo(element) {
        const computed = window.getComputedStyle(element);
        return {
            display: computed.display,
            position: computed.position,
            zIndex: computed.zIndex,
            visibility: computed.visibility,
            opacity: computed.opacity
        };
    }
};

// ============================================================================
// Export for use in different environments
// ============================================================================
if (typeof module !== "undefined" && module.exports) {
    module.exports = {
        ResourceErrorHandler,
        FMPObserver,
        StyleInjector,
        VueStyleUtils,
        DOMSelectors,
        ComponentAnalyzer
    };
} else if (typeof window !== "undefined") {
    window.DomStructureAnalyzer = {
        ResourceErrorHandler,
        FMPObserver,
        StyleInjector,
        VueStyleUtils,
        DOMSelectors,
        ComponentAnalyzer
    };

    // Auto-initialize FMP observer and resource error handler
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", () => {
            FMPObserver.init();
            ResourceErrorHandler.init();
        });
    } else {
        FMPObserver.init();
        ResourceErrorHandler.init();
    }
}
