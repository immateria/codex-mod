(function(){
  // 1) Tab blocking: override window.open + intercept target="_blank"
  try {
    const originalOpen = window.open;
    const openProxy = new Proxy(originalOpen, {
      apply(_t, _this, args) {
        const url = args[0];
        if (url) location.href = url;
        return null;
      }
    });
    Object.defineProperty(window, 'open', { value: openProxy, writable: false, configurable: false });

    const urlFrom = n => n?.href ?? n?.getAttribute?.('href') ?? n?.getAttribute?.('post-outbound-link') ?? n?.dataset?.url ?? n?.dataset?.href ?? null;
    const intercept = e => {
      const path = e.composedPath?.() ?? [];
      for (const n of path) {
        if (!n?.getAttribute) continue;
        if (n.getAttribute('target') === '_blank') {
          const url = urlFrom(n);
          if (url) { e.preventDefault(); e.stopImmediatePropagation(); location.href = url; }
          return;
        }
      }
    };
    ['pointerdown','click','auxclick'].forEach(ev => document.addEventListener(ev, intercept, { capture: true }));
    document.addEventListener('keydown', e => {
      if ((e.key === 'Enter' || e.key === ' ') && document.activeElement?.getAttribute?.('target') === '_blank') {
        e.preventDefault(); const url = urlFrom(document.activeElement); if (url) location.href = url;
      }
    }, { capture: true });
    document.addEventListener('submit', e => {
      if (e.target?.target === '_blank') { e.preventDefault(); e.target.target = '_self'; e.target.submit(); }
    }, { capture: true });
    try {
      const observeTarget = document.documentElement || document;
      if (observeTarget) {
        new MutationObserver(muts => muts.forEach(m => m.addedNodes.forEach(n => n && n.shadowRoot && ['pointerdown','click','auxclick'].forEach(ev => n.shadowRoot.addEventListener(ev, intercept, { capture: true })) ))).observe(observeTarget, { subtree: true, childList: true });
      }
    } catch (e) { console.warn('Tab block MO failed', e); }
  } catch (e) { console.warn('Tab blocking failed', e); }

  // 2) SPA history hooks
  try {
    const dispatch = () => {
      try {
        const ev = new Event('codex:locationchange');
        window.dispatchEvent(ev);
        window.__code_last_url = location.href;
      } catch {}
    };
    const push = history.pushState.bind(history);
    const repl = history.replaceState.bind(history);
    history.pushState = function(...a){ const r = push(...a); dispatch(); return r; };
    history.replaceState = function(...a){ const r = repl(...a); dispatch(); return r; };
    window.addEventListener('popstate', dispatch, { passive: true });
    dispatch();
  } catch (e) { console.warn('SPA hook failed', e); }

  // 3) Console capture: install once and persist for the lifetime of the document
  try {
    if (!window.__code_console_logs) {
      window.__code_console_logs = [];
      const push = (level, message) => {
        try {
          window.__code_console_logs.push({ timestamp: new Date().toISOString(), level, message });
          if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
        } catch (_) {}
      };

      // Override console methods once
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

      // Capture uncaught errors
      window.addEventListener('error', function(e) {
        try {
          var msg = e && e.message ? e.message : 'Script error';
          var stack = e && e.error && e.error.stack ? ('\n' + e.error.stack) : '';
          push('exception', msg + stack);
        } catch(_) {}
      });
      // Capture unhandled promise rejections
      window.addEventListener('unhandledrejection', function(e) {
        try {
          var reason = e && e.reason;
          if (reason && typeof reason === 'object') {
            try { reason = JSON.stringify(reason); } catch(_) {}
          }
          push('unhandledrejection', String(reason));
        } catch(_) {}
      });
    }
  } catch (e) { /* swallow */ }

  // 4) Stealth: reduce headless/automation signals for basic anti-bot checks
  try {
    // webdriver: undefined
    try { Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => undefined }); } catch(_) {}

    // languages
    try {
      const langs = ['en-US','en'];
      Object.defineProperty(Navigator.prototype, 'languages', { get: () => langs.slice() });
      Object.defineProperty(Navigator.prototype, 'language', { get: () => 'en-US' });
    } catch(_) {}

    // plugins & mimeTypes
    try {
      const fakePlugin = { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format' };
      const arrLike = (len) => ({ length: len, item(i){ return this[i]; } });
      const plugins = arrLike(1); plugins[0] = fakePlugin;
      const mimes = arrLike(2); mimes[0] = { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format' }; mimes[1] = { type: 'application/x-google-chrome-pdf', suffixes: 'pdf', description: 'Portable Document Format' };
      Object.defineProperty(Navigator.prototype, 'plugins', { get: () => plugins });
      Object.defineProperty(Navigator.prototype, 'mimeTypes', { get: () => mimes });
    } catch(_) {}

    // hardwareConcurrency & deviceMemory
    try { Object.defineProperty(Navigator.prototype, 'hardwareConcurrency', { get: () => 8 }); } catch(_) {}
    try { Object.defineProperty(Navigator.prototype, 'deviceMemory', { get: () => 8 }); } catch(_) {}

    // permissions.query
    try {
      const orig = navigator.permissions && navigator.permissions.query ? navigator.permissions.query.bind(navigator.permissions) : null;
      if (orig) {
        navigator.permissions.query = function(p){
          if (p && p.name === 'notifications') { return Promise.resolve({ state: 'granted' }); }
          return orig(p);
        }
      }
    } catch(_) {}

    // WebGL vendor/renderer
    try {
      const spoof = (proto) => {
        const orig = proto.getParameter;
        Object.defineProperty(proto, 'getParameter', { value: function(p){
          const UNMASKED_VENDOR_WEBGL = 0x9245; // WEBGL_debug_renderer_info
          const UNMASKED_RENDERER_WEBGL = 0x9246;
          if (p === UNMASKED_VENDOR_WEBGL) return 'Apple Inc.';
          if (p === UNMASKED_RENDERER_WEBGL) return 'Apple M2';
          return orig.apply(this, arguments);
        }});
      };
      if (window.WebGLRenderingContext) spoof(WebGLRenderingContext.prototype);
      if (window.WebGL2RenderingContext) spoof(WebGL2RenderingContext.prototype);
    } catch(_) {}

    // userAgentData (hints)
    try {
      if (!('userAgentData' in navigator)) {
        Object.defineProperty(Navigator.prototype, 'userAgentData', { get: () => ({
          brands: [ { brand: 'Chromium', version: '128' }, { brand: 'Google Chrome', version: '128' } ],
          mobile: false,
          platform: navigator.platform || 'macOS'
        })});
      }
    } catch(_) {}
  } catch(_) { /* ignore */ }

  // Note: the virtual cursor itself is installed on demand via runtime tooling.
})();

