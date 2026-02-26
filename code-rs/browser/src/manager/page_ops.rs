use crate::Result;
use crate::page::Page;
use tracing::warn;

use super::BrowserManager;

const CONSOLE_CAPTURE_INSTALL_JS: &str = r#"(function(){
            try {
                if (!window.__code_console_logs) {
                    window.__code_console_logs = [];
                    const push = (level, message) => {
                        try {
                            window.__code_console_logs.push({ timestamp: new Date().toISOString(), level, message });
                            if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
                        } catch (_) {}
                    };

                    ['log','warn','error','info','debug'].forEach(function(method) {
                        try {
                            const orig = console[method];
                            console[method] = function() {
                                try {
                                    var args = Array.prototype.slice.call(arguments);
                                    var msg = args.map(function(a) {
                                        try {
                                            if (a && typeof a === 'object') return JSON.stringify(a);
                                            return String(a);
                                        } catch (_) { return String(a); }
                                    }).join(' ');
                                    push(method, msg);
                                } catch(_) {}
                                if (orig) return orig.apply(console, arguments);
                            };
                        } catch(_) {}
                    });

                    window.addEventListener('error', function(e) {
                        try {
                            var msg = e && e.message ? e.message : 'Script error';
                            var stack = e && e.error && e.error.stack ? ('\\n' + e.error.stack) : '';
                            push('exception', msg + stack);
                        } catch(_) {}
                    });
                    window.addEventListener('unhandledrejection', function(e) {
                        try {
                            var reason = e && e.reason;
                            if (reason && typeof reason === 'object') { try { reason = JSON.stringify(reason); } catch(_) {} }
                            push('unhandledrejection', String(reason));
                        } catch(_) {}
                    });
                }
                return true;
            } catch (_) { return false; }
        })()"#;

pub(super) async fn install_console_capture(page: &Page, context: &str) {
    if let Err(e) = page.inject_js(CONSOLE_CAPTURE_INSTALL_JS).await {
        warn!("Failed to install console capture {context}: {e}");
    }
}

impl BrowserManager {
    /// Execute JavaScript code with enhanced return value handling.
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        let page = self.get_or_create_page().await?;
        page.execute_javascript(code).await
    }

    /// Capture console logs from the browser, including errors and unhandled rejections.
    pub async fn get_console_logs(&self, lines: Option<usize>) -> Result<serde_json::Value> {
        let page = self.get_or_create_page().await?;

        // 1) Prefer CDP-captured buffer (event-based). If we have entries, return them.
        let cdp_logs = page.get_console_logs_tail(lines).await;
        if cdp_logs.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            return Ok(cdp_logs);
        }

        // 2) Fallback to JS-installed hook (ensures capture on pages where events are unavailable).
        let requested = lines.unwrap_or(0);
        let script = format!(
            r#"(function() {{
                try {{
                    if (!window.__code_console_logs) {{
                        window.__code_console_logs = [];
                        const push = (level, message) => {{
                            try {{
                                window.__code_console_logs.push({{ timestamp: new Date().toISOString(), level, message }});
                                if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
                            }} catch (_) {{}}
                        }};

                        ['log','warn','error','info','debug'].forEach(function(method) {{
                            try {{
                                const orig = console[method];
                                console[method] = function() {{
                                    try {{
                                        var args = Array.prototype.slice.call(arguments);
                                        var msg = args.map(function(a) {{
                                            try {{ if (a && typeof a === 'object') return JSON.stringify(a); return String(a); }}
                                            catch (_) {{ return String(a); }}
                                        }}).join(' ');
                                        push(method, msg);
                                    }} catch(_) {{}}
                                    if (orig) return orig.apply(console, arguments);
                                }};
                            }} catch(_) {{}}
                        }});

                        window.addEventListener('error', function(e) {{
                            try {{
                                var msg = e && e.message ? e.message : 'Script error';
                                var stack = e && e.error && e.error.stack ? ('\n' + e.error.stack) : '';
                                push('exception', msg + stack);
                            }} catch(_) {{}}
                        }});
                        window.addEventListener('unhandledrejection', function(e) {{
                            try {{
                                var reason = e && e.reason;
                                if (reason && typeof reason === 'object') {{ try {{ reason = JSON.stringify(reason); }} catch(_) {{}} }}
                                push('unhandledrejection', String(reason));
                            }} catch(_) {{}}
                        }});
                    }}

                    var logs = window.__code_console_logs || [];
                    var n = {requested};
                    return (n && n > 0) ? logs.slice(-n) : logs;
                }} catch (err) {{
                    return [{{ timestamp: new Date().toISOString(), level: 'error', message: 'capture failed: ' + (err && err.message ? err.message : String(err)) }}];
                }}
            }})()"#
        );

        page.inject_js(&script).await
    }
}
