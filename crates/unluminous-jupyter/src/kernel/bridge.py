# Unluminous kernel bridge.
#
# Starts one Jupyter kernel through jupyter_client and relays between it and the editor.
# Commands arrive on standard input as JSON, one object per line. Events leave on standard
# output as JSON, one object per line. Standard error is free text for diagnostics.
# The editor embeds this file in its binary and runs it with the machine's own Python.
import sys, os

# The notebook's folder is the working directory, and a file there called json.py or queue.py would
# replace the library of that name. Take the working directory off the module search path first.
sys.path[:] = [p for p in sys.path if p not in ("", ".", os.getcwd())]
import json, threading, time, ast, traceback, queue

# Keep the protocol on a private copy of standard output, and point the real one at the null
# device, so nothing the kernel or a library prints can corrupt the stream.
_proto = os.fdopen(os.dup(1), "w", encoding="utf-8", newline="\n")
_null = open(os.devnull, "w")
os.dup2(_null.fileno(), 1)
sys.stdout = _null

_lock = threading.Lock()


def emit(**event):
    """Write one event as a line of JSON."""
    line = json.dumps(event, ensure_ascii=False, default=str)
    with _lock:
        try:
            _proto.write(line + "\n")
            _proto.flush()
        except Exception:
            pass


try:
    import jupyter_client  # noqa: F401
    from jupyter_client.manager import KernelManager
    from jupyter_client.kernelspec import KernelSpecManager
except Exception as problem:
    emit(event="failed", message="jupyter_client could not be imported: %s" % problem, missing="jupyter_client")
    os._exit(1)

HELPER = r'''
def f(ns):
    import json, types
    skip = {"In", "Out", "exit", "quit", "get_ipython", "open"}
    rows = []
    for name, value in list(ns.items()):
        if name.startswith("_") or name in skip or isinstance(value, types.ModuleType):
            continue
        row = {"name": name, "type": type(value).__name__, "value": "", "shape": None, "size": None}
        try:
            text = repr(value)
        except BaseException:
            text = "<repr failed>"
        row["value"] = text if len(text) <= 200 else text[:200] + "..."
        try:
            row["shape"] = str(tuple(value.shape))
        except BaseException:
            pass
        try:
            row["size"] = int(len(value))
        except BaseException:
            pass
        rows.append(row)
    return json.dumps(rows)
'''
VARIABLES_EXPRESSION = "(lambda g: (exec(%r, g), g['f'](get_ipython().user_ns))[1])({})" % HELPER

km = None
kc = None
pending = {}          # jupyter msg_id -> (our id, kind)
pending_lock = threading.Lock()
expected_end = False  # true while a restart or shutdown is requested, so the watchdog stays quiet
stopping = threading.Event()
waiting_ready = threading.Event()
died_sent = False


def kernel_pid():
    """The operating system process id of the kernel, when the provisioner knows it."""
    try:
        return km.provisioner.process.pid
    except Exception:
        try:
            return km.provisioner.pid
        except Exception:
            return None


def remember(msg_id, request, kind):
    """Record which of our requests a jupyter message id belongs to."""
    with pending_lock:
        pending[msg_id] = (request, kind)


def lookup(msg):
    """The (our id, kind) for the request a message answers, or (None, None)."""
    parent = (msg.get("parent_header") or {}).get("msg_id")
    with pending_lock:
        return pending.get(parent, (None, None))


def relay_iopub(msg):
    """Turn one iopub message into an event."""
    kind = msg["header"]["msg_type"]
    c = msg["content"]
    request, origin = lookup(msg)
    if kind == "status":
        if origin in (None, "execute"):
            emit(event="status", state=c.get("execution_state"), request=request)
    elif kind == "execute_input":
        emit(event="execute_input", request=request, execution_count=c.get("execution_count"))
    elif kind == "stream":
        emit(event="stream", request=request, name=c.get("name"), text=c.get("text"))
    elif kind in ("display_data", "update_display_data"):
        display_id = (c.get("transient") or {}).get("display_id")
        emit(event=kind, request=request, data=c.get("data", {}), metadata=c.get("metadata", {}), display_id=display_id)
    elif kind == "execute_result":
        emit(event=kind, request=request, execution_count=c.get("execution_count"), data=c.get("data", {}), metadata=c.get("metadata", {}))
    elif kind == "error":
        emit(event=kind, request=request, ename=c.get("ename"), evalue=c.get("evalue"), traceback=c.get("traceback", []))
    elif kind == "clear_output":
        emit(event=kind, request=request, wait=bool(c.get("wait")))


def relay_variables(request, c):
    """Turn the reply to the variables expression into an event."""
    try:
        result = (c.get("user_expressions") or {}).get("vars") or {}
        if result.get("status") != "ok":
            raise ValueError(result.get("evalue") or c.get("evalue") or "the kernel does not support variables")
        text = result["data"]["text/plain"]
        rows = json.loads(ast.literal_eval(text))
        emit(event="variables", request=request, rows=rows)
    except Exception as problem:
        emit(event="error", request=request, ename="VariablesUnavailable", evalue=str(problem), traceback=[])


def relay_shell(msg):
    """Turn one shell reply into an event."""
    kind = msg["header"]["msg_type"]
    c = msg["content"]
    request, origin = lookup(msg)
    if kind == "execute_reply":
        if origin == "variables":
            relay_variables(request, c)
        else:
            emit(event=kind, request=request, status=c.get("status"), execution_count=c.get("execution_count"))
    elif kind == "complete_reply":
        emit(event=kind, request=request, matches=c.get("matches", []), cursor_start=c.get("cursor_start", 0), cursor_end=c.get("cursor_end", 0), metadata=c.get("metadata", {}))
    elif kind == "inspect_reply":
        emit(event=kind, request=request, found=bool(c.get("found")), data=c.get("data", {}))
    elif kind == "is_complete_reply":
        emit(event=kind, request=request, status=c.get("status"), indent=c.get("indent", ""))
    elif kind == "kernel_info_reply":
        if origin == "restart" and waiting_ready.is_set():
            waiting_ready.clear()
            emit(event="restarted", info=c, pid=kernel_pid())


def relay_stdin(msg):
    """Turn an input request from the kernel into an event."""
    if msg["header"]["msg_type"] == "input_request":
        request, _ = lookup(msg)
        c = msg["content"]
        emit(event="input_request", request=request, prompt=c.get("prompt", ""), password=bool(c.get("password")))


def reader(getter, relay):
    """Read one channel until the bridge ends, relaying each message."""
    while not stopping.is_set():
        try:
            msg = getter(timeout=0.3)
        except queue.Empty:
            continue
        except Exception:
            if stopping.is_set():
                return
            time.sleep(0.2)
            continue
        try:
            relay(msg)
        except Exception:
            sys.stderr.write(traceback.format_exc())


def watchdog():
    """Report once when the kernel process dies without being asked to."""
    global died_sent
    while not stopping.is_set():
        time.sleep(1)
        try:
            alive = km.is_alive()
        except Exception:
            alive = True
        if not alive and not expected_end and not died_sent:
            died_sent = True
            emit(event="died", reason="the kernel process ended")


def fetch_info():
    """Ask the kernel for its kernel_info and return the reply content."""
    msg_id = kc.kernel_info()
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            reply = kc.get_shell_msg(timeout=1)
        except queue.Empty:
            continue
        if reply["parent_header"].get("msg_id") == msg_id:
            return reply["content"]
    return {}


def note(text):
    """Write a progress line to standard error, which the editor keeps for diagnostics."""
    print("bridge: " + text, file=sys.stderr, flush=True)


def start(cmd):
    """Start the kernel and report the result."""
    global km, kc
    name = cmd.get("kernel")
    if name in (None, "python3", "python"):
        import importlib.util
        if importlib.util.find_spec("ipykernel") is None:
            emit(event="failed", message="ipykernel is not installed for this Python", missing="ipykernel")
            os._exit(1)
    try:
        note("starting the kernel process")
        km = KernelManager(kernel_name=name) if name else KernelManager()
        km.start_kernel(cwd=cmd.get("cwd") or None, stdout=_null, stderr=sys.stderr)
        note("kernel process started, connecting")
        kc = km.client()
        kc.start_channels()
        note("waiting for the kernel to answer")
        kc.wait_for_ready(timeout=60)
        info = fetch_info()
    except BaseException as problem:
        emit(event="failed", message="%s: %s" % (type(problem).__name__, problem), missing=None)
        try:
            km.shutdown_kernel(now=True)
        except Exception:
            pass
        os._exit(1)
    emit(event="started", info=info, pid=kernel_pid())
    for getter, relay in ((kc.get_iopub_msg, relay_iopub), (kc.get_shell_msg, relay_shell), (kc.get_stdin_msg, relay_stdin)):
        threading.Thread(target=reader, args=(getter, relay), daemon=True).start()
    threading.Thread(target=watchdog, daemon=True).start()


def restart(cmd):
    """Restart the kernel and report when it answers kernel_info."""
    global expected_end, died_sent
    expected_end = True
    try:
        km.restart_kernel(now=False)
    except Exception as problem:
        expected_end = False
        emit(event="failed", message="restart failed: %s" % problem, missing=None)
        return
    died_sent = False
    waiting_ready.set()
    deadline = time.time() + 60
    while waiting_ready.is_set() and time.time() < deadline:
        remember(kc.kernel_info(), cmd.get("id"), "restart")
        for _ in range(20):
            if not waiting_ready.is_set():
                break
            time.sleep(0.1)
    if waiting_ready.is_set():
        waiting_ready.clear()
        emit(event="died", reason="the kernel did not answer after a restart")
    expected_end = False


def shutdown(cmd):
    """Stop the kernel, report it, and end the bridge."""
    global expected_end
    expected_end = True
    stopping.set()
    try:
        kc.stop_channels()
    except Exception:
        pass
    try:
        km.shutdown_kernel(now=False)
    except Exception:
        try:
            km.shutdown_kernel(now=True)
        except Exception:
            pass
    emit(event="stopped", request=cmd.get("id"))
    os._exit(0)


def handle(cmd):
    """Run one command from the editor."""
    name = cmd.get("cmd")
    rid = cmd.get("id")
    if name == "start":
        start(cmd)
    elif name == "kernelspecs":
        specs = KernelSpecManager().get_all_specs()
        rows = [{"name": n, "display_name": s["spec"].get("display_name", n), "language": s["spec"].get("language", "")} for n, s in specs.items()]
        emit(event="kernelspecs", request=rid, specs=rows)
    elif kc is None:
        emit(event="error", request=rid, ename="NoKernel", evalue="the kernel has not started", traceback=[])
    elif name == "execute":
        remember(kc.execute(cmd.get("code", ""), silent=bool(cmd.get("silent")), store_history=bool(cmd.get("store_history", True)), allow_stdin=bool(cmd.get("allow_stdin", True)), user_expressions=cmd.get("user_expressions") or None, stop_on_error=True), rid, "execute")
    elif name == "complete":
        remember(kc.complete(cmd.get("code", ""), cmd.get("cursor", 0)), rid, "complete")
    elif name == "inspect":
        remember(kc.inspect(cmd.get("code", ""), cmd.get("cursor", 0), cmd.get("detail", 0)), rid, "inspect")
    elif name == "is_complete":
        remember(kc.is_complete(cmd.get("code", "")), rid, "is_complete")
    elif name == "input_reply":
        kc.input(cmd.get("value", ""))
    elif name == "interrupt":
        km.interrupt_kernel()
    elif name == "restart":
        restart(cmd)
    elif name == "shutdown":
        shutdown(cmd)
    elif name == "variables":
        remember(kc.execute("", silent=True, store_history=False, allow_stdin=False, user_expressions={"vars": VARIABLES_EXPRESSION}), rid, "variables")


def main():
    """Serve commands until told to stop or until the editor closes our standard input."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        cmd = None
        try:
            cmd = json.loads(line)
            handle(cmd)
        except BaseException as problem:
            sys.stderr.write(traceback.format_exc())
            rid = cmd.get("id") if isinstance(cmd, dict) else None
            emit(event="error", request=rid, ename=type(problem).__name__, evalue=str(problem), traceback=[])
    # The editor has gone. Do not leave a kernel behind.
    if kc is not None:
        shutdown({})
    os._exit(0)


main()
