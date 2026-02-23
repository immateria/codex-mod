# Network Mediation (Managed Proxy) UX + Behavior Notes

This doc captures how network mediation currently behaves in `code-rs`, and the
user-facing UX contract that keeps "net off vs net on" behavior understandable.

It is intentionally practical: the goal is to prevent "I toggled net on/off and
nothing changed" confusion, and to ensure approval prompts are consistent with
the actual policy semantics.

## What "Network Mediation" Means

Network mediation is a **managed, local proxy** started by core (outside the
sandbox). When enabled, sandboxed tools are configured (and on macOS, enforced)
to send outbound network traffic through that proxy. The proxy applies:

- allow/deny domain rules
- local/private network protections (defense-in-depth)
- an interactive "allow this time / allow for session" approval flow for
  allowlist misses (when enabled)

Key point for users:

- **Mediation OFF**: if a tool has network access at all, it is **direct** and
  **these lists do not apply**.
- **Mediation ON**: sandboxed tool traffic is **forced through the proxy**
  (where supported) and **these lists/prompting rules apply**.

Important note (managed mode):

- The managed proxy binds **ephemeral loopback ports** at runtime. The
  persisted `[network].proxy_url/admin_url/socks_url` values are effectively
  placeholders today (they matter only if we add an explicit "standalone proxy"
  mode later).

## User-Facing Mental Model (OFF vs ON)

### OFF (direct / unmanaged)

- Network behavior is determined only by **sandbox policy**.
- The `[network]` allow/deny lists are effectively **inactive**.
- No network approval prompts should appear (because the proxy is not in the
  path).
- Some tools may still respect pre-existing `HTTP_PROXY`/`HTTPS_PROXY` env vars;
  that is still "unmanaged" (not the managed proxy).

### ON (managed proxy / mediated)

- For supported executions, outbound network is mediated by the proxy.
- The proxy applies allow/deny rules and may prompt the user for allowlist
  misses.
- Denylist blocks and local/private blocks do **not** prompt; they hard-block.
- Enforcement differs by sandbox/platform:
  - macOS seatbelt: can fail-closed and restrict outbound access to the proxy
    endpoints.
  - Linux seccomp sandbox: when `network_access=false`, the sandbox blocks TCP
    connect/bind entirely, so proxy mediation cannot work; when
    `network_access=true`, mediation is best-effort (env proxies can be ignored).
  - no sandbox / `danger-full-access`: best-effort (env proxies can be ignored).
- Gotcha: `NO_PROXY` is set to include loopback + private ranges. On platforms
  without strict sandbox enforcement, some clients may bypass the managed proxy
  for those targets (so allow/deny rules will not apply to that traffic).

## Policy Semantics (What Prompts vs What Hard-Blocks)

Network requests are evaluated in this order (current implementation):

1. **Denied domains** (`[network].denied_domains`): always blocks, no prompt.
2. **Local/private protection** (`[network].allow_local_binding=false`):
   blocks loopback/private and DNS-rebind-to-private targets unless explicitly
   allowlisted. No prompt.
3. **Allowlist enforcement** (`[network].allowed_domains`):
   - if allowlist is empty, or host is not allowlisted -> "not allowed"
   - "not allowed" can invoke the **interactive approval flow** (when the
     managed proxy is running with a decider attached).

Implications for UX copy:

- Prompts are **only** for "not allowlisted" cases.
- If the user hits a denylist/local/mode block, the fix is "edit settings",
  not "approve".
- Prompts are only enabled when the managed proxy is started with a decider
  attached (currently: sandbox modes `read-only` / `workspace-write`).

## Mode: `full` vs `limited`

`mode` affects what the proxy will allow even if a host is allowlisted:

- `full`
  - all HTTP methods allowed
  - HTTPS `CONNECT` is tunneled (no MITM) and therefore works
- `limited`
  - only `GET/HEAD/OPTIONS` allowed for HTTP requests
  - HTTPS `CONNECT` is blocked (so HTTPS effectively fails)
  - SOCKS is also blocked in limited mode

UX requirement: the settings UI must explicitly say "`limited` blocks HTTPS" or
users will interpret it as a "safer but still works" option.

## Coverage (What Is Actually Mediated Today)

Mediated today (best-effort / enforced depends on sandbox):

- shell tool execution (`exec`) (macOS seatbelt enforces; other platforms are
  best-effort)
- streamable shell tool (`exec_command` / `write_stdin`) uses the managed proxy
  env overrides, and on macOS uses seatbelt for sandboxed PTY children (fail-closed
  when managed network is enforced)
- `web_fetch` uses the managed proxy for:
  - `reqwest` fetch path
  - browser fetch path via `--proxy-server` + `Proxy-Authorization` (attempt id)

Not fully mediated today:

- the interactive "browser" tool (not all browser paths are guaranteed to use
  the managed proxy)
- linux sandboxed execution with `network_access=false` cannot reach the proxy
  (seccomp blocks TCP connect/bind)
- non-sandboxed executions (`danger-full-access`) cannot be strictly enforced
  (env vars can be ignored by child processes)

UX requirement: show a short "coverage" hint so users know what to expect.

## Making Mediation "Actually Enforced" (At Least On macOS)

Today, only macOS seatbelt provides a straightforward path to "fail closed"
enforcement that blocks direct egress and allows only proxy egress. To get
network mediation to behave like a real policy boundary (not just proxy env
vars), the remaining work is:

### macOS: Ensure Every Network-Capable Execution Is Seatbelt-Scoped

- For any tool that can initiate outbound network, ensure the actual process
  doing the network I/O runs under seatbelt with `enforce_managed_network=true`,
  and that proxy endpoints are present in the child env (`HTTP_PROXY`/etc).
- `exec` already does this when it runs via the seatbelt sandbox path.
- `exec_command` (PTY sessions) uses seatbelt on macOS when sandboxed (i.e. not
  `danger-full-access` and not `requires_escalated_permissions()`), by spawning the
  PTY child under `/usr/bin/sandbox-exec` with the same dynamic network policy.
  - Limitation: if the user requests escalated permissions, we intentionally
    fall back to best-effort (proxy env only) rather than silently "pretending"
    enforcement exists.
- The interactive `browser` tool should launch Chrome with proxy flags and (if
  we want enforcement) be executed under a sandboxed path that prevents direct
  egress (so `NO_PROXY` cannot bypass for non-loopback targets).

### Windows: Can We Make It Work?

Best-effort mediation (what we can do without admin/driver work):

- Start the managed proxy and inject `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` and
  `NO_PROXY` into child processes (`exec`, `exec_command`), plus route internal
  HTTP clients (like `web_fetch`) through the proxy.
- This works for many CLI tools, but it is not a security boundary because
  processes can ignore proxy env vars.

Enforced mediation (hard):

- Windows does not have a direct equivalent of seatbelt/landlock for "allow
  only loopback proxy ports" without using heavier mechanisms (AppContainer,
  Windows Firewall/WFP filters, or a dedicated sandbox helper).
- If we want true enforcement on Windows, we likely need a platform-specific
  sandbox runner that can block direct outbound traffic while allowing only the
  managed proxy endpoints. That is a substantial project and may require admin
  privileges depending on the approach (firewall/WFP).

## TUI UX Contract

### Settings → Network: Make OFF vs ON Obvious

The Network settings view shows a compact header block that always includes:

- current status summary (one line)
  - `OFF: direct (sandbox policy decides)`
  - `ON: mediated (managed proxy)`
- one-line prompt rule summary
  - `Prompts only for allowlist misses. Deny/local/mode blocks require edits.`
- one-line coverage summary
  - `Mediates: exec, exec_command, web_fetch (browser partial)`

Keep it short: the view must work in small terminals.

### Network Approvals: Dedicated Prompt (Temporary Only)

Network approvals render as a **network-specific modal** (not the generic exec
approval). Options are intentionally temporary:

- Allow once
- Allow for session
- Deny network for this run
- Deny and open Settings → Network

Current deny semantics (core behavior today):

- A single deny marks the **entire current attempt** as denied (no more prompts;
  all subsequent network requests in that attempt hard-deny). The UI must label
  this accurately (e.g. "Deny network for this run") so users don't interpret
  it as "deny only this host".

Implementation sketch (exact wiring):

- `code-rs/tui/src/chatwidget/history_pipeline/runtime_flow/approvals.rs`
  - if `ExecApprovalRequestEvent.network_approval_context.is_some()`, push
    `ApprovalRequest::Network { ... }` instead of `ApprovalRequest::Exec`.
- `code-rs/tui/src/user_approval_widget.rs`
  - add `ApprovalRequest::Network { id, host, protocol, command, reason }`
  - build `SelectOption`s without any execpolicy/persist actions
  - keep decisions mapped to `ReviewDecision::{Approved, ApprovedForSession, Denied, Abort}`
- `code-rs/tui/src/chatwidget/settings_routing.rs`
  - add a helper callable from the approval widget to open the Network section
    (`ensure_settings_overlay_section(SettingsSection::Network)`), used only for
    the optional "open settings" action.

### 3) Make The "net ..." Status Segment Clickable

Users should be able to discover and reach Network settings from where the
state is visible.

- Add a new clickable action (e.g. `ClickableAction::ShowNetworkSettings`).
- Wire it through:
  - `code-rs/tui/src/chatwidget/shared_defs.rs` (enum)
  - `code-rs/tui/src/chatwidget/input_pipeline.rs` (handle_click)
  - header/statusline region builders:
    - If the top header template adds a `{network}` placeholder in the future,
      make it clickable there too.
    - For the bottom statusline, make the `StatusLineItem::NetworkMediation`
      segment clickable and open Settings → Network.

Hover should cover the entire segment, like model/reasoning currently does.

### 4) Error Messaging: Make Hard Blocks Actionable

Hard-blocked requests (denylist/local/mode) frequently surface as:

- proxy JSON `{"status":"blocked","reason":...}`
- or a plain text message (e.g. "method not allowed in limited mode")

Wherever we surface these errors to the user (history cells, approval feedback,
or tool output), include a short actionable hint:

- `Edit Settings → Network to allow this host, or switch mode to full.`

This is optional but high leverage; it reduces repeated confusion during early
adoption.

## Acceptance Criteria (Manual)

1. Toggle mediation OFF:
   - statusline shows `net off`
   - Settings → Network header states rules are inactive
2. Toggle mediation ON:
   - statusline shows `net full` / `net limited`
   - Settings → Network header states proxy mediation is active
3. Trigger a network allowlist miss:
   - approval modal shows `host` + `protocol`
   - options are only allow once / allow for session / deny (no "Always allow")
   - deny text makes it clear it denies network for the remainder of the run
4. Trigger a denylist block:
   - no approval prompt
   - error points user to Settings → Network
5. Clicking `net ...` opens Settings → Network.

## References (Current Code)

- Core approval context: `code-rs/protocol/src/approvals.rs`
- Core network approval flow: `code-rs/core/src/network_approval.rs`
- Proxy policy order: `code-rs/network-proxy/src/runtime.rs`
- Proxy mode semantics: `code-rs/network-proxy/src/config.rs`
- TUI approval handler: `code-rs/tui/src/chatwidget/history_pipeline/runtime_flow/approvals.rs`
- TUI approval modal options: `code-rs/tui/src/user_approval_widget.rs`
- Statusline `net` rendering: `code-rs/tui/src/chatwidget/status_line_flow.rs`
