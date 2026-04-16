"""
Python kernel for the REPL tool.
Communicates over JSON lines on stdin/stdout.

Protocol (host → kernel):
  {"type": "exec", "id": "<id>", "code": "<code>"}
  {"type": "run_tool_result", "id": "<call_id>", ...}
  {"type": "emit_image_result", "id": "<id>", ...}

Protocol (kernel → host):
  {"type": "exec_result", "id": "<id>", "ok": true/false, "output": "...", "error": null/"..."}
  {"type": "run_tool", "id": "<call_id>", "exec_id": "...", "tool": "...", "arguments": "..."}
  {"type": "emit_image", "id": "<id>", "exec_id": "...", "data": "...", "media_type": "..."}
"""

import sys
import io
import os
import json
import traceback
import base64
import threading
import uuid


# ── Globals ──────────────────────────────────────────────────────────────

# Persistent namespace across exec cells, like a Jupyter kernel.
_namespace = {"__name__": "__repl__", "__builtins__": __builtins__}

# The ID of the currently-executing cell.
_active_exec_id = None

# Pending tool-call futures: {call_id: threading.Event + result dict}
_pending_tools = {}
_pending_tools_lock = threading.Lock()

# Pending emit_image futures
_pending_images = {}
_pending_images_lock = threading.Lock()

# Monotonic tool-call sequence counter.
_tool_seq = 0
_tool_seq_lock = threading.Lock()


# ── I/O helpers ──────────────────────────────────────────────────────────

def _send(message):
    """Write a JSON line to stdout (the host channel)."""
    line = json.dumps(message, ensure_ascii=False, default=str)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()


def _log(msg):
    """Write a diagnostic line to stderr."""
    sys.stderr.write(f"[kernel_python] {msg}\n")
    sys.stderr.flush()


# ── Codex API exposed to user code ───────────────────────────────────────

class _CodexAPI:
    """Namespace injected as `codex` into the REPL environment."""

    @staticmethod
    def run_tool(tool_name, arguments=None):
        """Call a host tool synchronously and return the result dict."""
        global _tool_seq
        if _active_exec_id is None:
            raise RuntimeError("codex.run_tool() can only be called during exec")

        with _tool_seq_lock:
            _tool_seq += 1
            seq = _tool_seq
        call_id = f"{_active_exec_id}-tool-{seq}"

        event = threading.Event()
        result_box = [None]

        with _pending_tools_lock:
            _pending_tools[call_id] = (event, result_box)

        args_str = json.dumps(arguments) if arguments is not None else "{}"
        _send({
            "type": "run_tool",
            "id": call_id,
            "exec_id": _active_exec_id,
            "tool": tool_name,
            "arguments": args_str,
        })

        # Block until the host responds (the stdin reader thread delivers it).
        event.wait()
        return result_box[0]

    @staticmethod
    def emit_image(data_or_path, media_type=None):
        """Send an image to the host. Accepts bytes, base64 str, or a file path."""
        if _active_exec_id is None:
            raise RuntimeError("codex.emit_image() can only be called during exec")

        if isinstance(data_or_path, (str, os.PathLike)) and os.path.isfile(str(data_or_path)):
            path = str(data_or_path)
            with open(path, "rb") as f:
                raw = f.read()
            b64 = base64.b64encode(raw).decode("ascii")
            if media_type is None:
                ext = os.path.splitext(path)[1].lower()
                media_type = {
                    ".png": "image/png",
                    ".jpg": "image/jpeg",
                    ".jpeg": "image/jpeg",
                    ".gif": "image/gif",
                    ".webp": "image/webp",
                    ".svg": "image/svg+xml",
                }.get(ext, "image/png")
        elif isinstance(data_or_path, bytes):
            b64 = base64.b64encode(data_or_path).decode("ascii")
            if media_type is None:
                media_type = "image/png"
        elif isinstance(data_or_path, str):
            b64 = data_or_path
            if media_type is None:
                media_type = "image/png"
        else:
            raise TypeError(f"emit_image: expected bytes, str, or path, got {type(data_or_path).__name__}")

        img_id = str(uuid.uuid4())
        event = threading.Event()
        result_box = [None]

        with _pending_images_lock:
            _pending_images[img_id] = (event, result_box)

        _send({
            "type": "emit_image",
            "id": img_id,
            "exec_id": _active_exec_id,
            "data": b64,
            "media_type": media_type,
        })

        event.wait()
        return result_box[0]


# Inject into the namespace.
_namespace["codex"] = _CodexAPI


# ── Exec handler ─────────────────────────────────────────────────────────

def _handle_exec(exec_id, code):
    """Execute a code cell in the persistent namespace."""
    global _active_exec_id
    _active_exec_id = exec_id

    capture = io.StringIO()
    old_stdout = sys.stdout
    old_stderr = sys.stderr

    # Redirect stdout/stderr so print() output is captured.
    sys.stdout = capture
    sys.stderr = capture

    try:
        # Try to compile as an expression first (so we can show its repr).
        # If it fails, compile as exec (statements).
        try:
            compiled = compile(code, "<repl>", "eval")
            is_expr = True
        except SyntaxError:
            compiled = compile(code, "<repl>", "exec")
            is_expr = False

        if is_expr:
            result = eval(compiled, _namespace)
            if result is not None:
                _namespace["_"] = result
                print(repr(result))
        else:
            exec(compiled, _namespace)

        output = capture.getvalue()
        sys.stdout = old_stdout
        sys.stderr = old_stderr

        _send({
            "type": "exec_result",
            "id": exec_id,
            "ok": True,
            "output": output,
            "error": None,
        })

    except Exception:
        output = capture.getvalue()
        sys.stdout = old_stdout
        sys.stderr = old_stderr
        error_msg = traceback.format_exc()

        _send({
            "type": "exec_result",
            "id": exec_id,
            "ok": False,
            "output": output,
            "error": error_msg,
        })

    finally:
        sys.stdout = old_stdout
        sys.stderr = old_stderr
        _active_exec_id = None

        # Settle any still-pending tool calls from this exec.
        with _pending_tools_lock:
            stale = [k for k in _pending_tools if k.startswith(f"{exec_id}-tool-")]
            for k in stale:
                ev, box = _pending_tools.pop(k)
                box[0] = {"ok": False, "error": "cell terminated before tool call completed"}
                ev.set()

        with _pending_images_lock:
            stale = list(_pending_images.keys())
            for k in stale:
                ev, box = _pending_images.pop(k)
                box[0] = {"ok": False, "error": "cell terminated before emit_image completed"}
                ev.set()


# ── Host message dispatch ────────────────────────────────────────────────

def _handle_tool_result(message):
    call_id = message.get("id")
    if not call_id:
        return
    with _pending_tools_lock:
        entry = _pending_tools.pop(call_id, None)
    if entry:
        event, result_box = entry
        result_box[0] = message
        event.set()
    else:
        _log(f"unexpected run_tool_result for unknown id: {call_id}")


def _handle_emit_image_result(message):
    img_id = message.get("id")
    if not img_id:
        return
    with _pending_images_lock:
        entry = _pending_images.pop(img_id, None)
    if entry:
        event, result_box = entry
        result_box[0] = message
        event.set()
    else:
        _log(f"unexpected emit_image_result for unknown id: {img_id}")


def _dispatch(message):
    msg_type = message.get("type")
    if msg_type == "exec":
        _handle_exec(message["id"], message["code"])
    elif msg_type == "run_tool_result":
        _handle_tool_result(message)
    elif msg_type == "emit_image_result":
        _handle_emit_image_result(message)
    else:
        _log(f"ignoring unknown message type: {msg_type}")


# ── Main loop ────────────────────────────────────────────────────────────

def main():
    """Read JSON lines from stdin and dispatch."""
    # Use unbuffered binary stdin to avoid readline issues.
    stdin = sys.stdin

    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            _log("ignoring non-JSON line from host")
            continue

        # Tool results and image results must be delivered to the blocked
        # exec thread immediately — dispatch them on a background thread.
        msg_type = message.get("type")
        if msg_type in ("run_tool_result", "emit_image_result"):
            threading.Thread(target=_dispatch, args=(message,), daemon=True).start()
        else:
            _dispatch(message)


if __name__ == "__main__":
    main()
