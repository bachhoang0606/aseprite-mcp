#!/usr/bin/env python3
"""Asset search — find CC0 assets & Lospec palettes, capture provenance. Stdlib-only.

The 2D analog of blender-mcp's PolyHaven/Sketchfab flow (research §F, roadmap #12): search
curated free/CC0 sources, preview a candidate, then fetch it into the project with attribution
captured at fetch time. Two sources:
  - Lospec  — palettes (turns knowledge/palettes from a handful into thousands).
  - Hugging Face `nyuuzyou/OpenGameArt-CC0` — 15.7k genuinely-CC0 art assets, queried via the
    datasets-server REST API (JSON, so no Parquet reader / no new dependency).

Lean-deps / Windows-SAC: stdlib only (urllib + json), NO requests/duckdb/pyarrow, no Rust HTTP
crate. Network egress is OPT-IN (`--online` or ASEPRITE_MCP_ALLOW_NET=1) with a no-API default:
offline `search` falls back to the bundled catalog (knowledge/asset-catalog.json) so it stays
useful — and CI-testable — with zero network.

Provenance: every `fetch` appends a MANIFEST.json record + a CREDITS.txt line (the ULPC pattern)
with a per-source-accurate license — CC0-1.0 for the HF dataset; for palettes a colour list is
not a copyrightable work, so we record courtesy attribution, NOT a CC0 claim.

Usage:
    python tools/asset_search.py search [query] [--source lospec|hf|all] [--tag T] [--limit N] [--online]
    python tools/asset_search.py fetch  <id> [--source ...] [--out DIR] [--online]
    python tools/asset_search.py --selftest
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
DEFAULT_CATALOG = os.path.join(ROOT, "knowledge", "asset-catalog.json")
DEFAULT_OUT = os.path.join("assets", "imported")

ALLOW_NET_ENV = "ASEPRITE_MCP_ALLOW_NET"
HF_DATASET = "nyuuzyou/OpenGameArt-CC0"
HF_SEARCH = "https://datasets-server.huggingface.co/search"
LOSPEC_PALETTE = "https://lospec.com/palette-list/{slug}.json"
LOSPEC_PAGE = "https://lospec.com/palette-list/{slug}"

USER_AGENT = "aseprite-mcp-asset-search/1.0 (+stdlib)"
HTTP_TIMEOUT = 20
MAX_JSON_BYTES = 8 * 1024 * 1024       # 8 MB — datasets-server pages / a palette JSON are tiny
MAX_DOWNLOAD_BYTES = 32 * 1024 * 1024  # 32 MB — a single asset image ceiling
ALLOWED_SCHEMES = ("http", "https")
PALETTE_LICENSE = "color list — attribution courtesy (see source)"
IMAGE_EXTS = (".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp")


# ---- network gate (mirrors the ADR-0003 ASEPRITE_MCP_ALLOW_LUA pattern) ----------------------
def is_truthy(v):
    return str(v).strip().lower() in ("1", "true", "yes", "on")


def network_allowed(online_flag):
    """Egress only when `--online` is passed or ASEPRITE_MCP_ALLOW_NET is truthy. Off by default."""
    return bool(online_flag) or is_truthy(os.environ.get(ALLOW_NET_ENV, ""))


def gate_error(action):
    return (
        f"{action} needs network, which is off by default. Re-run with --online or set "
        f"{ALLOW_NET_ENV}=1. Offline, `search` still works from the bundled catalog "
        f"(knowledge/asset-catalog.json)."
    )


# ---- small pure helpers ----------------------------------------------------------------------
def _first_str(row, keys):
    for k in keys:
        v = row.get(k)
        if isinstance(v, str) and v.strip():
            return v.strip()
    return None


def _as_url(v):
    """Coerce a field to an http(s) URL string, or None. Handles datasets-server image cells
    (a dict with `src`) and bare strings."""
    if isinstance(v, dict):
        v = v.get("src") or v.get("url")
    if isinstance(v, str) and urllib.parse.urlparse(v).scheme in ALLOWED_SCHEMES:
        return v
    return None


def _first_url(row, keys):
    for k in keys:
        u = _as_url(row.get(k))
        if u:
            return u
    return None


def _as_tags(v):
    if isinstance(v, list):
        return [str(t) for t in v if str(t).strip()]
    if isinstance(v, str) and v.strip():
        return [t.strip() for t in v.replace(";", ",").split(",") if t.strip()]
    return []


def _safe_name(s):
    """A filesystem-safe stem (no traversal): keep [A-Za-z0-9._-], collapse the rest to '-'."""
    out = "".join(c if (c.isalnum() or c in "._-") else "-" for c in str(s)).strip("-._")
    return out or "asset"


def _ext_from_url(url):
    path = urllib.parse.urlparse(url).path
    _, ext = os.path.splitext(path)
    ext = ext.lower()
    return ext if ext in IMAGE_EXTS else ".png"


def _norm_hex(c):
    return "#" + str(c).lstrip("#").upper()


# ---- source parsers (pure → unit-tested without network) -------------------------------------
def parse_lospec_palette(data, slug):
    """Lospec palette JSON `{name, author, colors:[hex…]}` → normalized candidate."""
    colors = [_norm_hex(c) for c in data.get("colors", []) if str(c).strip()]
    return {
        "source": "lospec", "kind": "palette", "id": slug,
        "name": data.get("name") or slug,
        "author": data.get("author") or "unknown",
        "license": PALETTE_LICENSE,
        "source_url": LOSPEC_PAGE.format(slug=slug),
        "preview_url": None,
        "download_url": LOSPEC_PALETTE.format(slug=slug),
        "colors": colors,
        "tags": [],
    }


def parse_hf_rows(data, limit=None):
    """HF datasets-server `/search` JSON → normalized asset candidates. Defensive: pulls a name
    and URLs from whatever recognizable fields a row has; skips rows it can't make sense of."""
    out = []
    for item in data.get("rows", []):
        row = item.get("row") if isinstance(item, dict) else None
        if not isinstance(row, dict):
            continue
        name = _first_str(row, ["title", "name", "filename", "file_name"])
        if not name and row.get("id") is not None:
            name = str(row.get("id"))
        if not name:
            continue
        cand = {
            "source": "hf", "kind": "asset",
            "id": str(row.get("id") if row.get("id") is not None else name),
            "name": name,
            "author": _first_str(row, ["author", "submitter", "uploader", "creator"])
            or "OpenGameArt contributor",
            # CC0 by dataset construction; honour an explicit per-row license if present.
            "license": _first_str(row, ["license"]) or "CC0-1.0",
            "source_url": _first_url(row, ["page_url", "source_url", "url"])
            or "https://huggingface.co/datasets/" + HF_DATASET,
            "preview_url": _first_url(row, ["preview_url", "preview", "thumbnail", "thumbnail_url", "image_url", "image"]),
            "download_url": _first_url(row, ["download_url", "download", "file_url", "content_url", "url", "image"]),
            "colors": None,
            "tags": _as_tags(row.get("tags")),
        }
        out.append(cand)
        if limit and len(out) >= limit:
            break
    return out


# ---- offline catalog -------------------------------------------------------------------------
def load_catalog(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def _load_local_colors(path):
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        colors = data.get("colors")
        return [_norm_hex(c) for c in colors] if isinstance(colors, list) else None
    except (OSError, ValueError):
        return None


def _catalog_palette_candidate(entry, root):
    cand = {
        "source": entry.get("source", "lospec"), "kind": "palette", "id": entry["id"],
        "name": entry.get("name", entry["id"]), "author": entry.get("author", "unknown"),
        "license": entry.get("license", PALETTE_LICENSE),
        "source_url": entry.get("source_url", ""),
        "preview_url": None, "download_url": entry.get("download_url"),
        "colors": None, "tags": entry.get("tags", []),
        "needs_online": bool(entry.get("fetch_online")),
    }
    lp = entry.get("local_path")
    if lp:
        cand["colors"] = _load_local_colors(os.path.join(root, lp))
        cand["needs_online"] = cand["colors"] is None
    return cand


def _catalog_asset_candidate(entry):
    return {
        "source": entry.get("source", "hf"), "kind": entry.get("kind", "asset"), "id": entry["id"],
        "name": entry.get("name", entry["id"]), "author": entry.get("author", "unknown"),
        "license": entry.get("license", "CC0-1.0"),
        "source_url": entry.get("source_url", ""),
        "preview_url": None, "download_url": None,
        "colors": None, "tags": entry.get("tags", []),
        "needs_online": bool(entry.get("fetch_online")),
        "hf_dataset": entry.get("hf_dataset"),
    }


def search_catalog(catalog, root, query="", source="all", tag=None, limit=None):
    """Offline search over the bundled catalog. Stable, sorted output."""
    cands = [_catalog_palette_candidate(e, root) for e in catalog.get("palettes", [])]
    cands += [_catalog_asset_candidate(e) for e in catalog.get("assets", [])]
    q = (query or "").lower().strip()
    tagq = tag.lower() if tag else None

    def keep(c):
        if source not in ("all", None) and c["source"] != source:
            return False
        tags = [t.lower() for t in c.get("tags", [])]
        if tagq and tagq not in tags:
            return False
        if q:
            hay = " ".join([c["id"], c["name"], c["author"], " ".join(c.get("tags", []))]).lower()
            if q not in hay:
                return False
        return True

    cands = [c for c in cands if keep(c)]
    cands.sort(key=lambda c: (c["source"], c["id"]))
    return cands[:limit] if limit else cands


# ---- HTTP (guarded; only reached when the gate is open) --------------------------------------
def _require_http_url(url):
    if urllib.parse.urlparse(url).scheme not in ALLOWED_SCHEMES:
        raise ValueError(f"refusing non-http(s) URL: {url!r}")
    return url


def _http_get_json(url, timeout=HTTP_TIMEOUT):
    _require_http_url(url)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 (scheme guarded above)
        raw = resp.read(MAX_JSON_BYTES + 1)
    if len(raw) > MAX_JSON_BYTES:
        raise ValueError(f"response exceeds {MAX_JSON_BYTES} bytes")
    return json.loads(raw.decode("utf-8"))


def _http_download(url, dest, max_bytes=MAX_DOWNLOAD_BYTES, timeout=HTTP_TIMEOUT):
    _require_http_url(url)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 (scheme guarded above)
        data = resp.read(max_bytes + 1)
    if len(data) > max_bytes:
        raise ValueError(f"download exceeds {max_bytes} bytes — refusing")
    with open(dest, "wb") as f:
        f.write(data)
    return len(data)


def fetch_lospec_palette(slug, timeout=HTTP_TIMEOUT):
    data = _http_get_json(LOSPEC_PALETTE.format(slug=urllib.parse.quote(slug)), timeout=timeout)
    return parse_lospec_palette(data, slug)


def search_hf(query, limit=20, dataset=HF_DATASET, config="default", split="train", timeout=HTTP_TIMEOUT):
    params = urllib.parse.urlencode({
        "dataset": dataset, "config": config, "split": split,
        "query": query or "", "offset": 0, "length": max(1, min(int(limit), 100)),
    })
    return parse_hf_rows(_http_get_json(f"{HF_SEARCH}?{params}", timeout=timeout), limit)


# ---- provenance (MANIFEST.json + CREDITS.txt) ------------------------------------------------
def manifest_record(cand, filename, fetched_offline):
    rec = {
        "kind": cand["kind"], "source": cand["source"], "id": cand["id"],
        "name": cand["name"], "author": cand["author"], "license": cand["license"],
        "source_url": cand["source_url"], "file": filename,
        "fetched_offline": bool(fetched_offline),
    }
    if cand["kind"] == "palette" and cand.get("colors"):
        rec["colors"] = cand["colors"]
    return rec


def credits_line(rec):
    return (
        f'"{rec["name"]}" by {rec["author"]} — {rec["license"]} — '
        f'{rec["source_url"]} (file: {rec["file"]})'
    )


def append_manifest(out_dir, rec):
    path = os.path.join(out_dir, "MANIFEST.json")
    items = []
    if os.path.exists(path):
        try:
            with open(path, encoding="utf-8") as f:
                items = json.load(f)
            if not isinstance(items, list):
                items = []
        except ValueError:
            items = []
    items.append(rec)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(items, f, indent=2)
    return path


def append_credits(out_dir, rec):
    path = os.path.join(out_dir, "CREDITS.txt")
    new = not os.path.exists(path)
    with open(path, "a", encoding="utf-8") as f:
        if new:
            f.write("# Asset credits — provenance captured at fetch time (SPEC-011).\n")
        f.write(credits_line(rec) + "\n")
    return path


# ---- fetch -----------------------------------------------------------------------------------
def resolve_candidate(catalog, root, asset_id, source, online):
    for c in search_catalog(catalog, root, source=source or "all"):
        if c["id"] == asset_id:
            return c
    if online and source in (None, "all", "lospec"):
        try:
            return fetch_lospec_palette(asset_id)
        except (urllib.error.URLError, ValueError, OSError):
            pass
    return None


def fetch_candidate(cand, out_dir, online):
    os.makedirs(out_dir, exist_ok=True)
    if cand["kind"] == "palette":
        colors = cand.get("colors")
        fetched_offline = True
        if not colors:
            if not online:
                raise PermissionError(gate_error(f"fetching palette '{cand['id']}'"))
            resolved = fetch_lospec_palette(cand["id"])
            colors = resolved["colors"]
            cand = {**cand, "colors": colors, "name": resolved["name"], "author": resolved["author"]}
            fetched_offline = False
        if not colors:
            raise ValueError(f"palette '{cand['id']}' resolved to no colours")
        filename = _safe_name(cand["id"]) + ".json"
        with open(os.path.join(out_dir, filename), "w", encoding="utf-8") as f:
            json.dump({"name": cand["name"], "source": cand["source_url"], "colors": colors}, f, indent=2)
        rec = manifest_record(cand, filename, fetched_offline)
    else:  # asset
        if not online:
            raise PermissionError(gate_error(f"downloading asset '{cand['id']}'"))
        url = cand.get("download_url") or cand.get("preview_url")
        if not url:
            raise ValueError(
                f"asset '{cand['id']}' has no download URL — run `search --source hf --online` to "
                f"get individual asset URLs (the catalog entry is the dataset pointer)"
            )
        filename = _safe_name(cand["id"]) + _ext_from_url(url)
        _http_download(url, os.path.join(out_dir, filename))
        rec = manifest_record(cand, filename, fetched_offline=False)
    append_manifest(out_dir, rec)
    append_credits(out_dir, rec)
    return rec


# ---- CLI -------------------------------------------------------------------------------------
def cmd_search(args):
    catalog = load_catalog(args.catalog)
    results = search_catalog(catalog, ROOT, args.query, args.source, args.tag, args.limit)
    if network_allowed(args.online) and args.source in ("hf", "all"):
        try:
            results = search_hf(args.query, args.limit or 20) + results
        except (urllib.error.URLError, ValueError, OSError) as e:
            print(f"# note: HF live search failed ({e}); showing catalog only", file=sys.stderr)
    elif args.source in ("hf", "all") and not network_allowed(args.online):
        print(f"# note: offline — HF asset search needs --online; showing catalog. "
              f"({gate_error('HF search')})", file=sys.stderr)
    print(json.dumps(results, indent=2))
    return 0


def candidate_from_url(url, name=None, author=None, license_=None, source="hf", asset_id=None):
    """Ad-hoc asset candidate for a direct download URL (e.g. a `download_url` from an HF
    `search --online` result) so it can be fetched WITH provenance."""
    stem = asset_id or os.path.basename(urllib.parse.urlparse(url).path) or "asset"
    return {
        "source": source if source in ("hf", "lospec", "local") else "hf",
        "kind": "asset", "id": stem, "name": name or stem,
        "author": author or "unknown",
        "license": license_ or "unknown — verify at source_url before use",
        "source_url": url, "preview_url": None, "download_url": url, "colors": None, "tags": [],
    }


def cmd_fetch(args):
    online = network_allowed(args.online)
    # Direct-URL fetch: completes the HF flow (search --online → a download_url → fetch --url).
    if args.url:
        if not online:
            print(gate_error(f"downloading {args.url}"), file=sys.stderr)
            return 3
        try:
            _require_http_url(args.url)
        except ValueError as e:
            print(str(e), file=sys.stderr)
            return 2
        src = args.source if args.source != "all" else "hf"
        cand = candidate_from_url(args.url, args.name, args.author, args.license, src, args.id)
        rec = fetch_candidate(cand, args.out, online=True)
        print(json.dumps(rec, indent=2))
        return 0

    if not args.id:
        print("fetch needs an <id> (palette slug / catalog id) or --url <download-url>", file=sys.stderr)
        return 2
    catalog = load_catalog(args.catalog)
    cand = resolve_candidate(catalog, ROOT, args.id, args.source, online)
    if not cand:
        print(f"no candidate '{args.id}' (try `search`{'' if online else '; many need --online'})",
              file=sys.stderr)
        return 2
    try:
        rec = fetch_candidate(cand, args.out, online)
    except PermissionError as e:
        print(str(e), file=sys.stderr)
        return 3
    print(json.dumps(rec, indent=2))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="Search & fetch CC0 assets / Lospec palettes (stdlib, gated).")
    ap.add_argument("--selftest", action="store_true", help="run offline self-checks and exit")
    sub = ap.add_subparsers(dest="command")

    s = sub.add_parser("search", help="search palettes/assets (offline catalog unless --online)")
    s.add_argument("query", nargs="?", default="", help="free-text query")
    s.add_argument("--source", choices=["lospec", "hf", "all"], default="all")
    s.add_argument("--tag", default=None)
    s.add_argument("--limit", type=int, default=None)
    s.add_argument("--online", action="store_true")
    s.add_argument("--catalog", default=DEFAULT_CATALOG)

    f = sub.add_parser("fetch", help="fetch one candidate into --out with MANIFEST/CREDITS")
    f.add_argument("id", nargs="?", help="palette slug / catalog id (omit when using --url)")
    f.add_argument("--url", default=None, help="direct download URL (an asset's download_url from search)")
    f.add_argument("--name", default=None, help="display name for a --url fetch")
    f.add_argument("--author", default=None, help="author for a --url fetch (provenance)")
    f.add_argument("--license", default=None, help="license for a --url fetch (provenance; verify it)")
    f.add_argument("--source", choices=["lospec", "hf", "all"], default="all")
    f.add_argument("--out", default=DEFAULT_OUT)
    f.add_argument("--online", action="store_true")
    f.add_argument("--catalog", default=DEFAULT_CATALOG)

    args = ap.parse_args(argv)
    if args.selftest:
        return _selftest()
    if args.command == "search":
        return cmd_search(args)
    if args.command == "fetch":
        return cmd_fetch(args)
    ap.print_help()
    return 1


# ---- offline self-test -----------------------------------------------------------------------
def _selftest():
    import tempfile

    # Lospec parse: hex normalized with '#', name/author carried.
    lp = parse_lospec_palette({"name": "Demo", "author": "Artist", "colors": ["ffffff", "#000000"]}, "demo")
    assert lp["colors"] == ["#FFFFFF", "#000000"] and lp["author"] == "Artist", lp
    assert lp["source_url"].endswith("/palette-list/demo"), lp

    # HF parse: good row kept + normalized; junk row skipped; CC0 default.
    hf = parse_hf_rows({"rows": [
        {"row": {"id": 7, "title": "Tree", "image": {"src": "https://x/t.png"}, "tags": ["nature", "tree"]}},
        {"row": {"nothing": True}},
    ]})
    assert len(hf) == 1 and hf[0]["name"] == "Tree" and hf[0]["license"] == "CC0-1.0", hf
    assert hf[0]["download_url"] == "https://x/t.png" and hf[0]["tags"] == ["nature", "tree"], hf

    # Gate: off by default, on with flag.
    assert network_allowed(False) is False and network_allowed(True) is True
    assert is_truthy("on") and not is_truthy("0")

    # URL guard rejects non-http(s).
    try:
        _require_http_url("file:///etc/passwd")
        raise AssertionError("expected URL guard to reject file://")
    except ValueError:
        pass

    # Offline catalog search: local palette has colours; lospec pointer needs online; hf filter.
    catalog = load_catalog(DEFAULT_CATALOG)
    allc = search_catalog(catalog, ROOT)
    pico = [c for c in allc if c["id"] == "pico-8"]
    assert pico and pico[0]["colors"], "pico-8 should resolve colours from its local file"
    endesga = [c for c in allc if c["id"] == "endesga-32"]
    assert endesga and endesga[0]["colors"] is None and endesga[0]["needs_online"], "lospec pointer needs --online"
    assert all(c["source"] == "hf" for c in search_catalog(catalog, ROOT, source="hf")), "source filter"
    assert search_catalog(catalog, ROOT, query="pico"), "query filter finds pico-8"

    # Fetch a LOCAL palette offline → file + manifest + credits; license is courtesy, not CC0.
    with tempfile.TemporaryDirectory() as d:
        rec = fetch_candidate(pico[0], d, online=False)
        assert rec["fetched_offline"] and "CC0" not in rec["license"], rec
        assert os.path.exists(os.path.join(d, "pico-8.json"))
        with open(os.path.join(d, "MANIFEST.json"), encoding="utf-8") as fh:
            man = json.load(fh)
        assert len(man) == 1 and man[0]["id"] == "pico-8", man
        with open(os.path.join(d, "CREDITS.txt"), encoding="utf-8") as fh:
            creds = fh.read()
        assert "PICO-8" in creds and "lospec.com" in creds, creds
        # An online-only asset refuses to fetch offline, loudly.
        ds = [c for c in allc if c["id"] == "opengameart-cc0"][0]
        try:
            fetch_candidate(ds, d, online=False)
            raise AssertionError("expected gate to block offline asset fetch")
        except PermissionError as e:
            assert ALLOW_NET_ENV in str(e), e

    print(json.dumps({"selftest": "ok"}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
