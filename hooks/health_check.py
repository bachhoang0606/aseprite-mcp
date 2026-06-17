#!/usr/bin/env python3
"""SessionStart hook: report aseprite-live bridge health as session context.

Stdlib-only. Probes the plugin port (9876) and the standalone bridge control
port (9877) so the session starts knowing whether live drawing is reachable —
surfacing churn/port problems early (checklist 7.4). Never blocks; always exits 0.
"""
import json
import os
import socket


def port_open(port: int) -> bool:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(0.3)
    try:
        sock.connect(("127.0.0.1", port))
        return True
    except OSError:
        return False
    finally:
        sock.close()


def main() -> None:
    plugin_port = int(os.environ.get("ASEPRITE_MCP_LIVE_PORT", "9876"))
    control_port = int(
        os.environ.get("ASEPRITE_MCP_LIVE_CONTROL_PORT", str(plugin_port + 1))
    )
    plugin = port_open(plugin_port)
    control = port_open(control_port)

    if control:
        msg = (
            f"aseprite-live: standalone bridge control port {control_port} is OPEN "
            f"(bridge running); plugin port {plugin_port} "
            f"{'OPEN' if plugin else 'CLOSED'}. Call live_preflight to confirm the "
            f"Aseprite plugin is actually connected before any live drawing."
        )
    elif plugin:
        msg = (
            f"aseprite-live: plugin port {plugin_port} is open but control port "
            f"{control_port} is closed — likely an old in-process server or a stale "
            f"bridge. Restart the MCP client so the standalone aseprite-live-bridge "
            f"spawns. Always live_preflight before drawing; never batch-fallback."
        )
    else:
        msg = (
            f"aseprite-live: bridge not detected (ports {plugin_port}/{control_port} "
            f"closed). It auto-spawns when the MCP server starts — ensure the "
            f"aseprite-live-bridge binary sits next to the MCP binary, open Aseprite "
            f"with the plugin enabled, then call live_preflight."
        )

    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": msg,
                }
            }
        )
    )


if __name__ == "__main__":
    main()
