use super::Page;

use crate::BrowserError;
use crate::Result;
use tracing::debug;

impl Page {
    pub async fn inject_js(&self, script: &str) -> Result<serde_json::Value> {
        let result = self.cdp_page.evaluate(script).await?;
        Ok(result.value().cloned().unwrap_or(serde_json::Value::Null))
    }

    pub async fn close(&self) -> Result<()> {
        // Note: chromiumoxide's close() takes ownership, so we can't call it on Arc<Page>
        // The page will be closed when the Arc is dropped
        Ok(())
    }

    /// Return a snapshot (tail) of the CDP-captured console buffer.
    pub async fn get_console_logs_tail(&self, lines: Option<usize>) -> serde_json::Value {
        let buf = self.console_logs.lock().await;
        if buf.is_empty() {
            return serde_json::Value::Array(vec![]);
        }
        let n = lines.unwrap_or(0);
        let slice: Vec<serde_json::Value> = if n > 0 && n < buf.len() {
            buf[buf.len() - n..].to_vec()
        } else {
            buf.clone()
        };
        serde_json::Value::Array(slice)
    }

    /// Execute JavaScript code with enhanced return value handling
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        debug!(
            "Executing JavaScript: {}...",
            &code.chars().take(100).collect::<String>()
        );

        // Create the user code with sourceURL for better debugging
        let user_code_with_source = format!("{code}\n//# sourceURL=browser_js_user_code.js");
        let user_code_json = serde_json::to_string(&user_code_with_source)
            .map_err(|err| BrowserError::CdpError(format!("Failed to serialize user code: {err}")))?;

        // Use the improved JavaScript harness
        let wrapped = format!(
            r#"(async () => {{
  const __meta = {{ startTs: Date.now(), urlBefore: location.href }};
  const __logs = [];
  const __errs = [];

  const __orig = {{
    log: console.log, warn: console.warn, error: console.error, debug: console.debug
  }};

  function __normalize(v, d = 0) {{
    const MAX_DEPTH = 3, MAX_STR = 4000;
    if (d > MAX_DEPTH) return {{ __type: 'truncated' }};
    if (v === undefined) return {{ __type: 'undefined' }};
    if (v === null || typeof v === 'number' || typeof v === 'boolean') return v;
    if (typeof v === 'string') return v.length > MAX_STR ? v.slice(0, MAX_STR) + '…' : v;
    if (typeof v === 'bigint') return {{ __type: 'bigint', value: v.toString() + 'n' }};
    if (typeof v === 'symbol') return {{ __type: 'symbol', value: String(v) }};
    if (typeof v === 'function') return {{ __type: 'function', name: v.name || '' }};

    if (typeof Element !== 'undefined' && v instanceof Element) {{
      return {{
        __type: 'element',
        tag: v.tagName, id: v.id || null, class: v.className || null,
        text: (v.textContent || '').trim().slice(0, 200)
      }};
    }}
    try {{ return JSON.parse(JSON.stringify(v)); }} catch {{}}

    if (Array.isArray(v)) return v.slice(0, 50).map(x => __normalize(x, d + 1));

    const out = Object.create(null);
    let n = 0;
    for (const k in v) {{
      if (!Object.prototype.hasOwnProperty.call(v, k)) continue;
      out[k] = __normalize(v[k], d + 1);
      if (++n >= 50) {{ out.__truncated = true; break; }}
    }}
    return out;
  }}

  const __push = (level, args) => {{
    __logs.push({{ level, args: args.map(a => __normalize(a)) }});
  }};
  console.log  = (...a) => {{ __push('log', a);  __orig.log(...a);  }};
  console.warn = (...a) => {{ __push('warn', a); __orig.warn(...a); }};
  console.error= (...a) => {{ __push('error',a); __orig.error(...a); }};
  console.debug= (...a) => {{ __push('debug',a); __orig.debug(...a); }};

  window.addEventListener('error', e => {{
    try {{ __errs.push(String(e.error || e.message || e)); }} catch {{ __errs.push('window.error'); }}
  }});
  window.addEventListener('unhandledrejection', e => {{
    try {{ __errs.push('unhandledrejection: ' + String(e.reason)); }} catch {{ __errs.push('unhandledrejection'); }}
  }});

  try {{
    const AsyncFunction = Object.getPrototypeOf(async function(){{}}).constructor;
    const __userCode = {user_code_json};
    const evaluator = new AsyncFunction('__code', '"use strict"; return eval(__code);');
    const raw = await evaluator(__userCode);
    const value = (raw === undefined ? null : __normalize(raw));

    return {{
      success: true,
      value,
      logs: __logs,
      errors: __errs,
      meta: {{
        urlBefore: __meta.urlBefore,
        urlAfter: location.href,
        durationMs: Date.now() - __meta.startTs
      }}
    }};
  }} catch (err) {{
    return {{
      success: false,
      value: null,
      error: String(err),
      stack: (err && err.stack) ? String(err.stack) : '',
      logs: __logs,
      errors: __errs
    }};
  }} finally {{
    console.log = __orig.log;
    console.warn = __orig.warn;
    console.error = __orig.error;
    console.debug = __orig.debug;
  }}
}})()"#
        );

        tracing::debug!("Executing JavaScript code: {}", code);
        tracing::debug!("Wrapped code: {}", wrapped);

        // Execute the wrapped code - chromiumoxide's evaluate method handles async functions
        let result = self.cdp_page.evaluate(wrapped).await?;
        let result_value = result.value().cloned().unwrap_or(serde_json::Value::Null);

        tracing::debug!("JavaScript execution result: {}", result_value);

        // Give a very brief moment for potential navigation or DOM updates triggered
        // by the script. Keep this low to avoid inflating tool latency.
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { 120 } else { 40 };
        tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;

        Ok(result_value)
    }

}
