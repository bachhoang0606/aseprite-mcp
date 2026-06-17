# Release process

Checklist 1.3 / 1.7. The heavy lifting is automated; a release is a tag.

## Automated (CI)

1. Merge to `main` with the **Quality gates** workflow green (Rust 3-OS,
   install-verify 3-OS incl. LICENSE check, Python gates, packaging manifests,
   `cargo audit`).
2. Tag and push:

   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```

3. The **Release** workflow (`.github/workflows/release.yml`) builds
   `aseprite_mcp` + `aseprite-live-bridge` in release mode on
   ubuntu/windows/macos, packages each with the Aseprite Lua extension +
   LICENSE/README/SECURITY, and publishes a GitHub Release with the three
   archives (`aseprite-mcp-{linux-x86_64,windows-x86_64,macos-arm64}.tar.gz`)
   attached and generated notes.
   - Dry-run without a tag: `gh workflow run release.yml --ref <branch>`
     (builds + uploads artifacts, skips the publish job).

## Manual (before tagging)

1. Update `CHANGELOG.md`: move `Unreleased` into a `## vX.Y.Z` section.
2. Bump `version` in `Cargo.toml` and `.claude-plugin/plugin.json` (keep equal).
3. Run the live smoke test with Aseprite open (`scripts/smoke/live-smoke.ps1`,
   or `live_preflight` → draw → export by hand); batch gates alone don't prove
   the live path.
4. Re-score `docs/aidd/COMPLETENESS_CHECKLIST.md` and commit the delta.

## After the release

- Verify the three archives install: unpack, run `scripts/install-plugin.ps1`
  / `install-plugin.sh`, `live_preflight` → ready.
- Announce/update marketplace metadata if the plugin is published.
