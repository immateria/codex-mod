use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub fn get_browser_tools_schema() -> Vec<BrowserToolSchema> {
    vec![
        BrowserToolSchema {
            name: "browser.goto".to_owned(),
            description: "Navigate to a URL and wait for the page to load".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to"
                    },
                    "wait": {
                        "oneOf": [
                            {
                                "type": "string",
                                "enum": ["domcontentloaded", "networkidle", "networkidle0", "networkidle2", "load"],
                                "description": "Wait for a specific event"
                            },
                            {
                                "type": "object",
                                "properties": {
                                    "delay_ms": {
                                        "type": "number",
                                        "description": "Wait for a specific delay in milliseconds"
                                    }
                                },
                                "required": ["delay_ms"]
                            }
                        ],
                        "description": "Wait strategy for page load"
                    }
                },
                "required": ["url"]
            }),
        },
        BrowserToolSchema {
            name: "browser.screenshot".to_owned(),
            description: "Take a screenshot of the current page".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["viewport", "full_page"],
                        "description": "Screenshot mode (default: viewport)"
                    },
                    "segments_max": {
                        "type": "number",
                        "description": "Maximum number of segments for full_page mode (default: 8)"
                    },
                    "region": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number" },
                            "y": { "type": "number" },
                            "width": { "type": "number" },
                            "height": { "type": "number" }
                        },
                        "required": ["x", "y", "width", "height"],
                        "description": "Optional region to capture"
                    },
                    "inject_js": {
                        "type": "string",
                        "description": "JavaScript to inject before screenshot"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["png", "webp"],
                        "description": "Image format (default: png)"
                    }
                }
            }),
        },
        BrowserToolSchema {
            name: "browser.setViewport".to_owned(),
            description: "Set the browser viewport size".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "width": {
                        "type": "number",
                        "description": "Viewport width in pixels"
                    },
                    "height": {
                        "type": "number",
                        "description": "Viewport height in pixels"
                    },
                    "device_scale_factor": {
                        "type": "number",
                        "description": "Device scale factor (default: 1.0)"
                    },
                    "mobile": {
                        "type": "boolean",
                        "description": "Enable mobile mode (default: false)"
                    }
                },
                "required": ["width", "height"]
            }),
        },
        BrowserToolSchema {
            name: "browser.close".to_owned(),
            description: "Close the page or browser".to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "what": {
                        "type": "string",
                        "enum": ["page", "browser"],
                        "description": "What to close (default: page)"
                    }
                }
            }),
        },
    ]
}
