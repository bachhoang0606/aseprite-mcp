#!/usr/bin/env python3
"""pixel-doctor - diagnose the aseprite-mcp live-infra failure modes. stdlib-only.

The deterministic, file/config side of the `/pixel-doctor` skill (the skill orchestrates the
LIVE probe via `live_preflight`, which only the running server can answer). Checks the recurring
pain (verified against src/live.rs + src/aseprite.rs + mcp/aseprite-live.json):

  1. ~/.claude.json `aseprite-live` server: does `command` exist? is the sibling
     `aseprite-live-bridge[.exe]` BESIDE it (the spawn_bridge invariant)? does it point at the
     WRONG repo (underscore `aseprite_mcp` vs hyphen `aseprite-mcp`)? is there a shadowing
     second `aseprite` stdio server?
  2. Aseprite executable resolvable for the OFFLINE tools - mirrors src/aseprite.rs
     `locate_executable` order (ASEPRITE_PATH if on disk -> OS install dirs -> `where/which aseprite`).
  3. Stale registered binary - registered exe OLDER than this repo's built target/release exe
     (serverVersion is hardwired 0.1.0, so mtime is the only reliable discriminator).
  4. Windows Smart App Control state - Enforce blocks freshly-built exes (OS error 4551 / EUNKNOWN).
  5. Bridge ports 9876/9877 listening (best-effort socket probe).

Pure diagnosis logic is unit-tested (tests/test_pixel_doctor.py); the env probes are best-effort.

    python scripts/pixel_doctor.py [--json] [--claude-json PATH] [--preflight PREFLIGHT.json]
    python scripts/pixel_doctor.py --selftest

Exit non-zero if any check is FAIL (WARN/INFO do not fail the run).
"""
import argparse
import json
import os
import socket
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
EXE = ".exe" if os.name == "nt" else ""
SERVER_BIN = "aseprite_mcp" + EXE
BRIDGE_BIN = "aseprite-live-bridge" + EXE
LIVE_PORT = 9876
CONTROL_PORT = 9877

# OK = healthy, WARN = suspicious (won't fail), FAIL = broken, INFO = context.
OK, WARN, FAIL, INFO = "OK", "WARN", "FAIL", "INFO"


def finding(check, status, detail, fix=""):
    return {"check": check, "status": status, "detail": detail, "fix": fix}


# ---- pure diagnosis core (unit-tested) -------------------------------------------------------
def find_mcp_servers(cfg):
    """All `mcpServers` maps anywhere in a ~/.claude.json (top-level + per-project). Returns a
    list of (where, name->entry dict)."""
    out = []

    def walk(node, where):
        if isinstance(node, dict):
            servers = node.get("mcpServers")
            if isinstance(servers, dict):
                out.append((where, servers))
            for k, v in node.items():
                if k != "mcpServers":
                    walk(v, f"{where}.{k}" if where else str(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, f"{where}[{i}]")

    walk(cfg, "")
    return out


def find_live_server(cfg):
    """The `aseprite-live` server entry (first found), plus whether a shadowing plain `aseprite`
    stdio server also exists (README warns it can shadow the live one)."""
    live, shadow = None, False
    for _where, servers in find_mcp_servers(cfg):
        if "aseprite-live" in servers and live is None:
            live = servers["aseprite-live"]
        if "aseprite" in servers:
            shadow = True
    return live, shadow


def diagnose_command(command, repo_root, isfile):
    """Static checks on the configured `aseprite-live` server command. `isfile` is injected so
    the core is testable without a real filesystem. Returns a list of findings."""
    out = []
    if not command:
        out.append(finding("claude.json/command", FAIL,
                           "no `command` on the aseprite-live server",
                           "add the server (or run scripts/install-plugin) and repoint it"))
        return out
    if not isfile(command):
        out.append(finding("claude.json/exe", FAIL,
                           f"configured server exe does not exist: {command}",
                           "rebuild (`cargo build --release --bins`) and repoint ~/.claude.json"))
        return out
    out.append(finding("claude.json/exe", OK, f"server exe exists: {command}"))

    # The spawn_bridge invariant: the sibling bridge must be in the SAME dir as the server exe.
    # Do NOT assume `target/release` - a custom build dir (e.g. target/rmcp1) is fine if co-located.
    bridge = os.path.join(os.path.dirname(command), BRIDGE_BIN)
    if isfile(bridge):
        out.append(finding("sibling-bridge", OK, f"bridge beside server: {bridge}"))
    else:
        out.append(finding("sibling-bridge", FAIL,
                           f"no {BRIDGE_BIN} beside the server exe ({bridge}) - spawn_bridge bails, "
                           f"so bridgeLinked stays false and the plugin never links",
                           f"copy a SAC-approved {BRIDGE_BIN} into {os.path.dirname(command)}"))

    # Wrong-repo guard: an `aseprite_mcp` (underscore) path while editing the hyphen `aseprite-mcp`.
    norm = command.replace("\\", "/").lower()
    root_norm = repo_root.replace("\\", "/").lower()
    if "/aseprite_mcp/" in norm and "/aseprite-mcp" in root_norm:
        out.append(finding("wrong-repo", WARN,
                           "server command points into the underscore `aseprite_mcp` repo while you "
                           "are working in the hyphen `aseprite-mcp` checkout - stale/missing features",
                           f"repoint ~/.claude.json command at {os.path.join(repo_root, 'target', 'release', SERVER_BIN)}"))
    return out


def locate_aseprite(env, candidates, isfile, which):
    """Replicate src/aseprite.rs::locate_executable resolution for the OFFLINE tools:
    ASEPRITE_PATH (only if it exists on disk) -> OS install candidates -> `aseprite` on PATH.
    Returns (path, how) or (None, None)."""
    raw = env.get("ASEPRITE_PATH")
    if raw and isfile(raw):
        return raw, "ASEPRITE_PATH"
    for c in candidates:
        if isfile(c):
            return c, "install-dir"
    found = which("aseprite")
    if found:
        return found, "PATH"
    return None, None


def stale_check(registered_exe, built_exe, isfile, getmtime):
    """Flag when the registered/running exe is OLDER than this repo's freshly-built exe (you
    rebuilt but reconnect still runs the old file). Pure (mtime fns injected)."""
    if not (isfile(registered_exe) and isfile(built_exe)):
        return None
    try:
        if os.path.abspath(registered_exe) == os.path.abspath(built_exe):
            return finding("stale-binary", OK, "registered exe IS this repo's built exe")
        if getmtime(registered_exe) < getmtime(built_exe):
            return finding("stale-binary", WARN,
                          "registered exe is OLDER than this repo's target/release build - a /mcp "
                          "reconnect would still run the stale binary",
                          "repoint ~/.claude.json at the fresh exe (or rebuild where it points), "
                          "kill the orphan server, then /mcp reconnect")
        return finding("stale-binary", INFO, "registered exe differs from this repo's build but is not older")
    except OSError:
        return None


def classify_preflight(d):
    """Turn a `live_preflight` / `live_status` JSON dict into a diagnosis. The heart of the skill's
    decision tree: bridgeLinked is the discriminator between the bridge layer and the plugin layer."""
    if d.get("ready") or d.get("connected"):
        return OK, "connected", "Live session connected - proceed with live_* tools."
    if not d.get("bridgeLinked", False):
        return FAIL, "bridge-down", (
            "bridgeLinked=false -> the server can't reach its bridge. Causes: the sibling "
            "aseprite-live-bridge is missing beside the server exe; a SAC-blocked server/bridge "
            "(OS 4551 / EUNKNOWN on a fresh build); or an orphan server holding the port. "
            "Run pixel_doctor static checks, fix the sibling/SAC/orphan, then /mcp reconnect."
        )
    if d.get("lastHello") in (None, "", {}):
        return FAIL, "plugin-not-connected", (
            "bridgeLinked=true but lastHello is null -> the bridge is up, but no Aseprite plugin "
            "connected. Launch Aseprite, enable the aseprite-mcp-plugin extension, focus the window "
            "once (its reconnect timer ticks on focus), then re-run live_preflight."
        )
    return WARN, "transient", "bridge up and a hello seen but not ready - re-run live_preflight."


# ---- best-effort environment probes (not in the pure core) -----------------------------------
def default_claude_json():
    return os.path.join(os.path.expanduser("~"), ".claude.json")


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def aseprite_candidates():
    if os.name == "nt":
        return [
            r"C:\Program Files\Aseprite\Aseprite.exe",
            r"C:\Program Files (x86)\Steam\steamapps\common\Aseprite\Aseprite.exe",
            r"C:\Program Files\Steam\steamapps\common\Aseprite\Aseprite.exe",
        ]
    if sys.platform == "darwin":
        return ["/Applications/Aseprite.app/Contents/MacOS/aseprite"]
    home = os.path.expanduser("~")
    return [os.path.join(home, ".steam/debian-installation/steamapps/common/Aseprite/aseprite")]


def sac_state():
    """Windows Smart App Control state, or None off-Windows / on error. Enforce(1) blocks
    freshly-built exes. Read-only registry query via the stdlib `winreg`."""
    if os.name != "nt":
        return None
    try:
        import winreg
        key = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                             r"SYSTEM\CurrentControlSet\Control\CI\Policy")
        val, _ = winreg.QueryValueEx(key, "VerifiedAndReputablePolicyState")
        return {0: "off", 1: "enforce", 2: "evaluation"}.get(val, f"unknown({val})")
    except OSError:
        return "unknown"


def port_listening(port, host="127.0.0.1", timeout=0.3):
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except OSError:
        return False


# ---- report assembly -------------------------------------------------------------------------
def run_checks(claude_json_path, preflight=None):
    findings = []
    cfg = read_json(claude_json_path)
    if cfg is None:
        findings.append(finding("claude.json", WARN,
                               f"could not read {claude_json_path} (so config checks are skipped)",
                               "pass --claude-json PATH if it lives elsewhere"))
        live, shadow, command = None, False, None
    else:
        live, shadow = find_live_server(cfg)
        if live is None:
            findings.append(finding("claude.json/server", FAIL,
                                   "no `aseprite-live` MCP server in ~/.claude.json",
                                   "install the plugin / add the server, pointing at target/release/" + SERVER_BIN))
            command = None
        else:
            command = live.get("command")
            findings += diagnose_command(command, ROOT, os.path.isfile)
        if shadow:
            findings.append(finding("shadow-server", WARN,
                                   "a second stdio MCP server named `aseprite` also exists - it can "
                                   "shadow the live one (README)",
                                   "ensure the client uses `aseprite-live`, not `aseprite`"))

    # Stale binary (only when we have a registered command).
    built = os.path.join(ROOT, "target", "release", SERVER_BIN)
    if command:
        s = stale_check(command, built, os.path.isfile, os.path.getmtime)
        if s:
            findings.append(s)

    # Offline Aseprite resolution.
    path, how = locate_aseprite(os.environ, aseprite_candidates(), os.path.isfile,
                               lambda n: __import__("shutil").which(n))
    if path:
        findings.append(finding("aseprite-exe (offline tools)", OK, f"resolved via {how}: {path}"))
    else:
        findings.append(finding("aseprite-exe (offline tools)", WARN,
                               "Aseprite executable not found - the OFFLINE export/CLI tools will "
                               "error 'Aseprite executable not found' (the LIVE path does not need it)",
                               "set ASEPRITE_PATH to Aseprite's full path (only needed for offline tools)"))

    # SAC (Windows).
    sac = sac_state()
    if sac == "enforce":
        findings.append(finding("smart-app-control", WARN,
                               "SAC is ENFORCE - a freshly-built exe is blocked (OS 4551 / EUNKNOWN on "
                               "/mcp reconnect) until approved",
                               "after a rebuild, launch the exe once interactively (or restore an approved "
                               "copy) before reconnecting; for `cargo test`, just re-run it once"))
    elif sac:
        findings.append(finding("smart-app-control", INFO, f"SAC state: {sac}"))

    # Live ports (best-effort).
    ctrl, plug = port_listening(CONTROL_PORT), port_listening(LIVE_PORT)
    findings.append(finding("ports", OK if ctrl else INFO,
                           f"control {CONTROL_PORT}: {'listening' if ctrl else 'not listening'}; "
                           f"plugin {LIVE_PORT}: {'listening' if plug else 'not listening'}",
                           "" if ctrl else "control port down -> bridge not up (see sibling-bridge / SAC / orphan)"))

    # Preflight JSON, if the caller piped one in.
    if preflight is not None:
        status, mode, msg = classify_preflight(preflight)
        findings.append(finding("live-preflight", status, f"{mode}: {msg}"))
    else:
        findings.append(finding("live-preflight", INFO,
                               "run `live_preflight` (MCP) and pass it via --preflight to classify the live side"))
    return findings


def print_report(findings, as_json):
    if as_json:
        print(json.dumps(findings, indent=2))
    else:
        print("== pixel-doctor ==")
        for f in findings:
            line = f"[{f['status']:4}] {f['check']}: {f['detail']}"
            print(line)
            if f["fix"] and f["status"] in (WARN, FAIL):
                print(f"        fix: {f['fix']}")
        worst = FAIL if any(f["status"] == FAIL for f in findings) else (
            WARN if any(f["status"] == WARN for f in findings) else OK)
        print(f"-- verdict: {worst} --")
    return 1 if any(f["status"] == FAIL for f in findings) else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="Diagnose aseprite-mcp live-infra problems (stdlib).")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--json", action="store_true", help="machine-readable findings")
    ap.add_argument("--claude-json", default=None, help="path to ~/.claude.json (default: home)")
    ap.add_argument("--preflight", default=None, help="a live_preflight JSON file to classify")
    args = ap.parse_args(argv)
    if args.selftest:
        return _selftest()
    preflight = read_json(args.preflight) if args.preflight else None
    findings = run_checks(args.claude_json or default_claude_json(), preflight)
    return print_report(findings, args.json)


# ---- offline self-test -----------------------------------------------------------------------
def _selftest():
    # find_live_server: nested mcpServers + shadow detection.
    cfg = {"mcpServers": {"other": {}},
           "projects": {"/p": {"mcpServers": {"aseprite-live": {"command": "X"}, "aseprite": {}}}}}
    live, shadow = find_live_server(cfg)
    assert live == {"command": "X"} and shadow is True, (live, shadow)

    # Build paths with os.path.join so separators match diagnose_command on every OS.
    hyphen_root = os.path.join("work", "aseprite-mcp")
    base = os.path.join("R", "target", "release")
    server, bridge = os.path.join(base, SERVER_BIN), os.path.join(base, BRIDGE_BIN)

    # diagnose_command: healthy (exe + sibling bridge).
    ok = diagnose_command(server, hyphen_root, lambda p, fs={server, bridge}: p in fs)
    assert any(f["check"] == "sibling-bridge" and f["status"] == OK for f in ok), ok
    # missing exe -> FAIL.
    bad = diagnose_command(os.path.join("nope", SERVER_BIN), hyphen_root, lambda p: False)
    assert bad[0]["status"] == FAIL, bad
    # exe present but no sibling bridge -> FAIL.
    lone = os.path.join("x", SERVER_BIN)
    nb = diagnose_command(lone, hyphen_root, lambda p, only={lone}: p in only)
    assert any(f["check"] == "sibling-bridge" and f["status"] == FAIL for f in nb), nb
    # wrong-repo (underscore path while editing the hyphen checkout) -> WARN.
    ubase = os.path.join("u", "aseprite_mcp", "target", "release")
    userver, ubridge = os.path.join(ubase, SERVER_BIN), os.path.join(ubase, BRIDGE_BIN)
    wr = diagnose_command(userver, hyphen_root, lambda p, fs={userver, ubridge}: p in fs)
    assert any(f["check"] == "wrong-repo" and f["status"] == WARN for f in wr), wr

    # locate_aseprite: ASEPRITE_PATH wins only if on disk; else candidates; else PATH; else None.
    assert locate_aseprite({"ASEPRITE_PATH": "/a"}, [], lambda p: p == "/a", lambda n: None) == ("/a", "ASEPRITE_PATH")
    assert locate_aseprite({"ASEPRITE_PATH": "/gone"}, ["/c"], lambda p: p == "/c", lambda n: None) == ("/c", "install-dir")
    assert locate_aseprite({}, [], lambda p: False, lambda n: "/onpath") == ("/onpath", "PATH")
    assert locate_aseprite({}, [], lambda p: False, lambda n: None) == (None, None)

    # stale_check: older registered -> WARN; same path -> OK.
    mt = {"/old": 1.0, "/new": 2.0}
    st = stale_check("/old", "/new", lambda p: True, lambda p: mt[p])
    assert st["status"] == WARN, st
    same = stale_check("/new", "/new", lambda p: True, lambda p: mt[p])
    assert same["status"] == OK, same

    # classify_preflight: the bridgeLinked decision tree.
    assert classify_preflight({"ready": True})[1] == "connected"
    assert classify_preflight({"ready": False, "bridgeLinked": False})[1] == "bridge-down"
    assert classify_preflight({"ready": False, "bridgeLinked": True, "lastHello": None})[1] == "plugin-not-connected"

    print(json.dumps({"selftest": "ok"}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
