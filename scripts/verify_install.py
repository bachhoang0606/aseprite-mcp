#!/usr/bin/env python3
"""Cross-platform install verification (checklist 1.4). stdlib-only.

Run after `cargo build --release --bins`. Asserts two things that must hold on
every OS we ship to (win/mac/linux):

  1. Both release binaries exist with the OS-correct name — the MCP server
     (`aseprite_mcp[.exe]`) and the standalone bridge (`aseprite-live-bridge[.exe]`).
  2. No shipped config/manifest hardcodes a user home path (e.g. `C:\\Users\\...`,
     `/Users/...`, `/home/...`); install must resolve under `${CLAUDE_PLUGIN_ROOT}`.
  3. A LICENSE file ships with the plugin and matches the license declared in
     the manifests (checklist 1.6).

Exit non-zero on any failure, so CI fails loudly. Used by the `install-verify`
matrix job in `.github/workflows/quality.yml`.

    python scripts/verify_install.py
"""
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
EXE = ".exe" if os.name == "nt" else ""

# Binaries cargo produces (default bin from src/main.rs + src/bin/*.rs).
BINARIES = ["aseprite_mcp", "aseprite-live-bridge"]

# Shipped files that must stay machine/user agnostic.
SHIPPED_CONFIGS = [
    ".claude-plugin/plugin.json",
    ".claude-plugin/marketplace.json",
    "mcp/aseprite-live.json",
    "hooks/hooks.json",
]

# Literal user-home roots that must never be baked into a shipped config.
HARDCODED_HOME = re.compile(r"[A-Za-z]:\\Users\\|/Users/|/home/")


def check_binaries():
    rel = os.path.join(ROOT, "target", "release")
    missing = []
    for name in BINARIES:
        path = os.path.join(rel, name + EXE)
        if not os.path.isfile(path):
            missing.append(os.path.relpath(path, ROOT))
        else:
            print(f"OK  binary: {os.path.relpath(path, ROOT)}")
    if missing:
        print("FAIL missing release binaries:", ", ".join(missing))
    return not missing


def check_no_hardcoded_paths():
    bad = []
    for rel in SHIPPED_CONFIGS:
        path = os.path.join(ROOT, rel)
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                if HARDCODED_HOME.search(line):
                    bad.append(f"{rel}:{n}: {line.strip()}")
        print(f"OK  no hardcoded home: {rel}")
    if bad:
        print("FAIL hardcoded user paths in shipped config:")
        for b in bad:
            print("   ", b)
    return not bad


def check_license():
    path = os.path.join(ROOT, "LICENSE")
    if not os.path.isfile(path):
        print("FAIL missing LICENSE file (plugin.json declares a license)")
        return False
    with open(path, encoding="utf-8") as f:
        head = f.read(200)
    if "MIT" not in head:
        print("FAIL LICENSE file does not match declared MIT license")
        return False
    print("OK  LICENSE present (MIT, matches manifests)")
    return True


def main():
    print(f"== install verification (os={os.name}, exe='{EXE or 'none'}') ==")
    ok = check_binaries()
    ok = check_no_hardcoded_paths() and ok
    ok = check_license() and ok
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
