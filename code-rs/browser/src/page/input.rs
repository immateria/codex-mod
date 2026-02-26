use super::Page;
use super::VIRTUAL_CURSOR_JS;

use crate::BrowserError;
use crate::Result;
use chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventParams;
use chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventType;
use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventParams;
use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventType;
use chromiumoxide::cdp::browser_protocol::input::MouseButton;
use std::time::Duration;
use tracing::debug;
use tracing::warn;

impl Page {
    /// (NEW) Injects a virtual cursor element into the page at the current coordinates.
    pub async fn inject_virtual_cursor(&self) -> Result<()> {
        let cursor = self.cursor_state.lock().await.clone();
        let cursor_x = cursor.x;
        let cursor_y = cursor.y;

        // First try the externalized installer for easier iteration.
        // The JS must define `window.__vcInstall(x,y)` and create window.__vc with __version=11.
        let external = format!(
            "{VIRTUAL_CURSOR_JS}\n;(()=>{{ try {{ return (window.__vcInstall ? window.__vcInstall : function(x,y){{}})({cursor_x:.0},{cursor_y:.0}); }} catch (e) {{ return String(e && e.message || e); }} }})()"
        );
        if let Ok(res) = self.cdp_page.evaluate(external).await {
            let status = res
                .value()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if status == "ok" {
                return Ok(());
            } else {
                warn!("Virtual cursor injection reported: {}", status);
                return Err(BrowserError::CdpError(format!(
                    "Virtual cursor injection failed: {status}"
                )));
            }
        }
        warn!("Virtual cursor injection failed: no response");
        Err(BrowserError::CdpError("Virtual cursor injection failed: no response".into()))
    }

    /// Ensures an editable element is focused before typing without stealing focus.
    /// Rules:
    /// - If the deeply focused element (piercing shadow DOM and same-origin iframes) is editable, do nothing.
    /// - Otherwise, try to focus the editable element directly under the virtual cursor location.
    /// - Never fall back to any other candidate (prevents unexpected focus steals).
    async fn ensure_editable_focused(&self) -> Result<bool> {
        let cursor = self.cursor_state.lock().await.clone();
        let cursor_x = cursor.x;
        let cursor_y = cursor.y;

        let script = format!(
            r#"
            (function(cursorX, cursorY) {{
                const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
                const isEditable = (el) => !!el && (
                    (el.tagName === 'INPUT' && isEditableInputType(el.type)) ||
                    el.tagName === 'TEXTAREA' ||
                    el.isContentEditable === true
                );

                const deepActiveElement = () => {{
                    try {{
                        let ae = document.activeElement;
                        // Pierce shadow roots
                        while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {{
                            ae = ae.shadowRoot.activeElement;
                        }}
                        // Pierce same-origin iframes
                        while (ae && ae.tagName === 'IFRAME') {{
                            try {{
                                const doc = ae.contentWindow && ae.contentWindow.document;
                                if (!doc) break;
                                let inner = doc.activeElement;
                                if (!inner) break;
                                while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {{
                                    inner = inner.shadowRoot.activeElement;
                                }}
                                ae = inner;
                            }} catch (_) {{ break; }}
                        }}
                        return ae || null;
                    }} catch (_) {{ return null; }}
                }};

                const deepElementFromPoint = (x, y) => {{
                    // Walk composed tree using elementsFromPoint, then descend into open shadow roots and same-origin iframes
                    const walk = (root, gx, gy) => {{
                        let list = [];
                        try {{
                            list = (root.elementsFromPoint ? root.elementsFromPoint(gx, gy) : [root.elementFromPoint(gx, gy)].filter(Boolean)) || [];
                        }} catch (_) {{ list = []; }}
                        for (const el of list) {{
                            // Descend into shadow root if present
                            if (el && el.shadowRoot) {{
                                const deep = walk(el.shadowRoot, gx, gy);
                                if (deep) return deep;
                            }}
                            // Descend into same-origin iframe
                            if (el && el.tagName === 'IFRAME') {{
                                try {{
                                    const rect = el.getBoundingClientRect();
                                    const lx = gx - rect.left; // local X inside iframe viewport
                                    const ly = gy - rect.top;  // local Y inside iframe viewport
                                    const doc = el.contentWindow && el.contentWindow.document;
                                    if (doc) {{
                                        const deep = walk(doc, lx, ly);
                                        if (deep) return deep;
                                    }}
                                }} catch(_) {{ /* cross-origin: skip */ }}
                            }}
                            if (el) return el;
                        }}
                        return null;
                    }};
                    return walk(document, x, y);
                }};

                // 1) If something is already focused and is editable (deeply), keep it.
                const current = deepActiveElement();
                if (isEditable(current)) return true;

                // 2) Otherwise, only try to focus the editable element under the cursor.
                if (Number.isFinite(cursorX) && Number.isFinite(cursorY)) {{
                    let el = deepElementFromPoint(cursorX, cursorY);
                    // climb up to an editable ancestor if needed within same composed tree
                    const canFocus = (n) => n && typeof n.focus === 'function';
                    let walker = el;
                    while (walker && !isEditable(walker)) {{
                        walker = walker.parentElement || (walker.getRootNode && (walker.getRootNode().host || null)) || null;
                    }}
                    if (isEditable(walker) && canFocus(walker)) {{
                        walker.focus();
                        const after = deepActiveElement();
                        return after === walker;
                    }}
                }}
                return false; // Do not steal focus by picking arbitrary candidates.
            }})({cursor_x}, {cursor_y})
            "#
        );

        let result = self.cdp_page.evaluate(script).await?;
        let focused = result.value().and_then(serde_json::Value::as_bool).unwrap_or(false);
        Ok(focused)
    }

    // Move the mouse by relative offset from current position
    pub async fn move_mouse_relative(&self, dx: f64, dy: f64) -> Result<(f64, f64)> {
        // Get current position
        let cursor = self.cursor_state.lock().await;
        let current_x = cursor.x;
        let current_y = cursor.y;
        drop(cursor);

        // Calculate new position
        let new_x = current_x + dx;
        let new_y = current_y + dy;

        debug!(
            "Moving mouse relatively by ({}, {}) from ({}, {}) to ({}, {})",
            dx, dy, current_x, current_y, new_x, new_y
        );

        // Use absolute move with the calculated position
        self.move_mouse(new_x, new_y).await?;
        Ok((new_x, new_y))
    }

    // (NEW) Move the mouse to the specified coordinates
    pub async fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        debug!("Moving mouse to ({}, {})", x, y);

        // Clamp and floor coordinates
        let move_x = x.floor().max(0.0);
        let move_y = y.floor().max(0.0);

        let mut cursor = self.cursor_state.lock().await;

        // If target is effectively the same as current, avoid dispatching/animating
        if (cursor.x - move_x).abs() < 0.5 && (cursor.y - move_y).abs() < 0.5 {
            drop(cursor);
            // Ensure cursor is present/updated even if no move
            let _ = self.ensure_virtual_cursor().await;
            return Ok(());
        }

        // Dispatch the mouse move event, including the current button state
        let move_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(move_x)
            .y(move_y)
            .button(cursor.button.clone()) // Pass the button state
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(move_params).await?;

        // Update cursor position
        cursor.x = move_x;
        cursor.y = move_y;
        drop(cursor); // Release lock before JavaScript evaluation

        // First check if cursor exists, if not inject it
        let check_script = "typeof window.__vc !== 'undefined'";
        let cursor_exists = self
            .cdp_page
            .evaluate(check_script)
            .await
            .ok()
            .and_then(|result| result.value().and_then(serde_json::Value::as_bool))
            .unwrap_or(false);

        if !cursor_exists {
            debug!("Virtual cursor not found, injecting it now");
            if let Err(e) = self.inject_virtual_cursor().await {
                warn!("Failed to inject virtual cursor: {}", e);
            }
        }

        // For internal browser, snap instantly without animation. For external, animate and respect duration.
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let dur_ms = if is_external {
            self
                .cdp_page
                .evaluate(format!(
                    "(function(x,y){{ try {{ if(window.__vc && window.__vc.update) return window.__vc.update(x,y)|0; }} catch(_e){{}} return 0; }})({move_x:.0},{move_y:.0})"
                ))
                .await
                .ok()
                .and_then(|r| r.value().and_then(serde_json::Value::as_u64))
                .unwrap_or(0)
        } else {
            // Internal browser: snap immediately and report zero duration
            let _ = self
                .cdp_page
                .evaluate(format!(
                    "(function(x,y){{ try {{ if(window.__vc && window.__vc.snapTo) {{ window.__vc.snapTo(x,y); return 0; }} }} catch(_e){{}} return 0; }})({move_x:.0},{move_y:.0})"
                ))
                .await;
            0
        };

        // Only wait when connected to an external browser and there is a non-zero duration
        if is_external && dur_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(dur_ms)).await;
        }

        Ok(())
    }

    /// (UPDATED) Click at the specified coordinates with visual animation
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        debug!("Clicking at ({}, {})", x, y);

        // Use move_mouse to handle movement, clamping, and state update
        self.move_mouse(x, y).await?;

        // Get the final coordinates after potential clamping
        let cursor = self.cursor_state.lock().await;
        let click_x = cursor.x;
        let click_y = cursor.y;
        drop(cursor); // Release lock before async calls

        // Trigger click pulse animation via virtual cursor API and wait briefly
        let click_ms_val = self
            .cdp_page
            .evaluate(
                "(function(){ if(window.__vc && window.__vc.clickPulse){ return window.__vc.clickPulse(); } return 0; })()",
            )
            .await
            .ok()
            .and_then(|r| r.value().and_then(serde_json::Value::as_u64))
            .unwrap_or(0);

        // Mouse down
        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Add a small delay between press and release
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;

        // Mouse up
        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Wait briefly so the page processes the click; avoid long animation waits
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { click_ms_val.min(240) } else { 40 };
        if settle_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;
        }

        Ok(())
    }

    /// Perform mouse down at the current position
    pub async fn mouse_down_at_current(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        let x = cursor.x;
        let y = cursor.y;
        let is_down = cursor.is_mouse_down;
        drop(cursor);

        if is_down {
            debug!("Mouse is already down at ({}, {})", x, y);
            return Ok((x, y));
        }

        debug!("Mouse down at current position ({}, {})", x, y);

        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Update mouse state (track button for drag moves)
        let mut cursor = self.cursor_state.lock().await;
        cursor.is_mouse_down = true;
        cursor.button = MouseButton::Left;
        drop(cursor);

        Ok((x, y))
    }

    /// Perform mouse up at the current position
    pub async fn mouse_up_at_current(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        let x = cursor.x;
        let y = cursor.y;
        let is_down = cursor.is_mouse_down;
        drop(cursor);

        if !is_down {
            debug!("Mouse is already up at ({}, {})", x, y);
            return Ok((x, y));
        }

        debug!("Mouse up at current position ({}, {})", x, y);

        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Update mouse state
        let mut cursor = self.cursor_state.lock().await;
        cursor.is_mouse_down = false;
        cursor.button = MouseButton::None;
        drop(cursor);

        Ok((x, y))
    }

    /// Click at the current mouse position without moving the cursor
    pub async fn click_at_current(&self) -> Result<(f64, f64)> {
        // Get the current cursor position and check if mouse is down
        let cursor = self.cursor_state.lock().await;
        let click_x = cursor.x;
        let click_y = cursor.y;
        let was_down = cursor.is_mouse_down;
        debug!(
            "Clicking at current position ({}, {}), mouse was_down: {}",
            click_x, click_y, was_down
        );
        drop(cursor); // Release lock before async calls

        // If mouse is already down, release it first
        if was_down {
            debug!("Mouse was down, releasing first before click");
            self.mouse_up_at_current().await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        }

        // Trigger click animation through virtual cursor API
        let click_ms_val = self
            .cdp_page
            .evaluate(
                "(function(){ if(window.__vc && window.__vc.clickPulse){ return window.__vc.clickPulse(); } return 0; })()",
            )
            .await
            .ok()
            .and_then(|r| r.value().and_then(serde_json::Value::as_u64))
            .unwrap_or(0);

        // Mouse down
        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Add a small delay between press and release
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;

        // Mouse up
        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Wait briefly so the page processes the click; avoid long animation waits
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { click_ms_val.min(240) } else { 40 };
        if settle_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;
        }

        Ok((click_x, click_y))
    }

    /// Type text into the currently focused element with optimized typing strategies
    pub async fn type_text(&self, text: &str) -> Result<()> {
        // Replace em dashes with regular dashes
        let processed_text = text.replace('—', " - ");
        debug!("Typing text: {}", processed_text);

        // Ensure an editable element is focused first. If we cannot ensure focus
        // on an editable field, bail to avoid sending keystrokes to the wrong place.
        let ensured = self.ensure_editable_focused().await?;
        if !ensured {
            debug!("No editable focus ensured; skipping typing to avoid stealing focus");
            return Ok(());
        }

        // Install a temporary focus guard that keeps focus anchored on the
        // currently focused editable unless the user intentionally sends Tab/Enter.
        let _ = self.execute_javascript(
            r#"(() => {
                try {
                  const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
                  const isEditable = (el) => !!el && (
                    (el.tagName === 'INPUT' && isEditableInputType(el.type)) ||
                    el.tagName === 'TEXTAREA' ||
                    el.isContentEditable === true
                  );
                  const deepActiveElement = (rootDoc) => {
                    let ae = (rootDoc || document).activeElement;
                    // Shadow roots
                    while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {
                      ae = ae.shadowRoot.activeElement;
                    }
                    // Same-origin iframes
                    while (ae && ae.tagName === 'IFRAME') {
                      try {
                        const doc = ae.contentWindow && ae.contentWindow.document;
                        if (!doc) break;
                        let inner = doc.activeElement;
                        if (!inner) break;
                        while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {
                          inner = inner.shadowRoot.activeElement;
                        }
                        ae = inner;
                      } catch (_) { break; }
                    }
                    return ae || null;
                  };

                  const w = window;
                  if (!w.__codeFG) {
                    w.__codeFG = {
                      active: false,
                      lastKey: null,
                      anchor: null,
                      onKeyDown: null,
                      onFocusIn: null,
                      onBlur: null,
                      install() {
                        const anchor = deepActiveElement();
                        if (!isEditable(anchor)) return false;
                        this.anchor = anchor;
                        this.active = true;
                        this.lastKey = null;
                        this.onKeyDown = (e) => { this.lastKey = e && e.key; };
                        this.onFocusIn = (e) => {
                          if (!this.active) return;
                          const a = this.anchor;
                          const curr = deepActiveElement();
                          if (!a || a === curr) return;
                          // Allow intentional navigations
                          if (this.lastKey === 'Tab' || this.lastKey === 'Enter') return;
                          // If anchor was detached or hidden, stop guarding
                          try {
                            const cs = a.ownerDocument && a.ownerDocument.defaultView && a.ownerDocument.defaultView.getComputedStyle(a);
                            const hidden = !a.isConnected || (cs && (cs.display === 'none' || cs.visibility === 'hidden'));
                            if (hidden) { this.active = false; return; }
                          } catch(_){}
                          // Restore focus asynchronously to override app-level auto-tabbing
                          setTimeout(() => { try { a.focus && a.focus(); } catch(_){} }, 0);
                        };
                        this.onBlur = () => { /* ignore */ };
                        document.addEventListener('keydown', this.onKeyDown, true);
                        document.addEventListener('focusin', this.onFocusIn, true);
                        document.addEventListener('blur', this.onBlur, true);
                        return true;
                      },
                      uninstall() {
                        try {
                          document.removeEventListener('keydown', this.onKeyDown, true);
                          document.removeEventListener('focusin', this.onFocusIn, true);
                          document.removeEventListener('blur', this.onBlur, true);
                        } catch(_){}
                        this.active = false;
                        this.anchor = null;
                        this.lastKey = null;
                        return true;
                      }
                    };
                  }
                  return window.__codeFG.install();
                } catch(_) { return false; }
            })()"#
        ).await;

        let text_len = processed_text.len();

        if text_len >= 100 {
            // Large text: paste-style insertion with no per-char delay
            // Try to insert at caret for input/textarea and contenteditable; fall back to raw key events without delay.
            let js = format!(
                r#"(() => {{
  try {{
    const T = {text_json};
    const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
    const isEditable = (el) => !!el && ((el.tagName === 'INPUT' && isEditableInputType(el.type)) || el.tagName === 'TEXTAREA' || el.isContentEditable === true);
    const deepActiveElement = (rootDoc) => {{
      let ae = (rootDoc || document).activeElement;
      while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {{ ae = ae.shadowRoot.activeElement; }}
      while (ae && ae.tagName === 'IFRAME') {{
        try {{
          const doc = ae.contentWindow && ae.contentWindow.document; if (!doc) break;
          let inner = doc.activeElement; if (!inner) break;
          while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {{ inner = inner.shadowRoot.activeElement; }}
          ae = inner;
        }} catch (_) {{ break; }}
      }}
      return ae || null;
    }};
    const ae = deepActiveElement();
    if (!isEditable(ae)) return {{ success: false, reason: 'no-editable' }};

    if (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA') {{
      const start = ae.selectionStart|0, end = ae.selectionEnd|0;
      const val = String(ae.value || '');
      const before = val.slice(0, start), after = val.slice(end);
      ae.value = before + T + after;
      const pos = (before + T).length;
      ae.selectionStart = ae.selectionEnd = pos;
      try {{ ae.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: T }})); }} catch (_e) {{}}
      return {{ success: true, inserted: T.length, caret: pos }};
    }} else if (ae.isContentEditable === true) {{
      try {{ if (document.execCommand) {{ document.execCommand('insertText', false, T); return {{ success: true, inserted: T.length }}; }} }} catch (_e) {{}}
      try {{
        const sel = window.getSelection();
        if (sel && sel.rangeCount) {{
          const r = sel.getRangeAt(0);
          r.deleteContents();
          r.insertNode(document.createTextNode(T));
          r.collapse(false);
          return {{ success: true, inserted: T.length }};
        }}
      }} catch (_e) {{}}
      return {{ success: false, reason: 'contenteditable-failed' }};
    }}
    return {{ success: false, reason: 'unsupported' }};
  }} catch (e) {{ return {{ success: false, error: String(e) }}; }}
}})()"#,
                text_json = serde_json::to_string(&processed_text).unwrap_or_else(|_| "".to_string())
            );

            let _ = self.execute_javascript(&js).await;
        } else {
            // Short/medium text: per-character with reduced delay 30–60ms
            for ch in processed_text.chars() {
                if ch == '\n' {
                    self.press_key("Enter").await?;
                } else if ch == '\t' {
                    self.press_key("Tab").await?;
                } else {
                    let params = DispatchKeyEventParams::builder()
                        .r#type(DispatchKeyEventType::Char)
                        .text(ch.to_string())
                        .build()
                        .map_err(BrowserError::CdpError)?;
                    self.cdp_page.execute(params).await?;
                }

                // Reduced natural typing delay
                let delay = 30 + (rand::random::<u64>() % 31); // 30–60ms
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }

        // Remove the focus guard shortly after typing to cover post-typing side effects
        let _ = self.execute_javascript(
            r#"(() => { try { if (window.__codeFG && window.__codeFG.uninstall) { setTimeout(() => { try { window.__codeFG.uninstall(); } catch(_){} }, 500); return true; } return false; } catch(_) { return false; } })()"#
        ).await;

        Ok(())
    }

    /// Press a key (e.g., "Enter", "Tab", "Escape", "ArrowDown")
    pub async fn press_key(&self, key: &str) -> Result<()> {
        debug!("Pressing key: {}", key);

        // Map key names to their proper codes and virtual key codes
        let (code, text, windows_virtual_key_code, native_virtual_key_code) = match key {
            "Enter" => ("Enter", Some("\r"), Some(13), Some(13)),
            "Tab" => ("Tab", Some("\t"), Some(9), Some(9)),
            "Escape" => ("Escape", None, Some(27), Some(27)),
            "Backspace" => ("Backspace", None, Some(8), Some(8)),
            "Delete" => ("Delete", None, Some(46), Some(46)),
            "ArrowUp" => ("ArrowUp", None, Some(38), Some(38)),
            "ArrowDown" => ("ArrowDown", None, Some(40), Some(40)),
            "ArrowLeft" => ("ArrowLeft", None, Some(37), Some(37)),
            "ArrowRight" => ("ArrowRight", None, Some(39), Some(39)),
            "Home" => ("Home", None, Some(36), Some(36)),
            "End" => ("End", None, Some(35), Some(35)),
            "PageUp" => ("PageUp", None, Some(33), Some(33)),
            "PageDown" => ("PageDown", None, Some(34), Some(34)),
            "Space" => ("Space", Some(" "), Some(32), Some(32)),
            _ => (key, None, None, None), // Default fallback
        };

        // Key down
        let mut down_builder = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(key.to_string())
            .code(code.to_string());

        if let Some(vk) = windows_virtual_key_code {
            down_builder = down_builder.windows_virtual_key_code(vk);
        }
        if let Some(nvk) = native_virtual_key_code {
            down_builder = down_builder.native_virtual_key_code(nvk);
        }

        let down_params = down_builder.build().map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Send char event for keys that produce text
        if let Some(text_str) = text {
            let char_params = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::Char)
                .key(key.to_string())
                .code(code.to_string())
                .text(text_str.to_string())
                .build()
                .map_err(BrowserError::CdpError)?;
            self.cdp_page.execute(char_params).await?;
        }

        // Key up
        let mut up_builder = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key.to_string())
            .code(code.to_string());

        if let Some(vk) = windows_virtual_key_code {
            up_builder = up_builder.windows_virtual_key_code(vk);
        }
        if let Some(nvk) = native_virtual_key_code {
            up_builder = up_builder.native_virtual_key_code(nvk);
        }

        let up_params = up_builder.build().map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        Ok(())
    }

    /// Get the current cursor position
    pub async fn get_cursor_position(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        Ok((cursor.x, cursor.y))
    }
}
