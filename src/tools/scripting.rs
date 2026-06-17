use rmcp::schemars;
use serde::Deserialize;

use crate::server::AsepriteServer;

// ============================================================================
// Security gate (ADR-0003, checklist 10.1)
// ============================================================================

/// Env flag that must be explicitly enabled to allow arbitrary Lua / CLI
/// execution. `run_lua_script` and `execute_cli` run an unrestricted Aseprite
/// `--batch --script` process, which is effectively code execution on the host,
/// so they are **disabled by default** and only run when an operator opts in.
const ALLOW_LUA_ENV: &str = "ASEPRITE_MCP_ALLOW_LUA";

fn is_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether arbitrary Lua/CLI execution is opt-in-enabled for this process.
pub fn lua_execution_allowed() -> bool {
    std::env::var(ALLOW_LUA_ENV)
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

/// Actionable error returned when the gate is closed.
fn lua_disabled_error(tool: &str) -> String {
    format!(
        "{tool} is disabled by default: it runs arbitrary Aseprite Lua/CLI, which is \
         effectively code execution on this machine (see \
         docs/adr/0003-run-lua-script-security.md and SECURITY.md). To enable, set \
         {ALLOW_LUA_ENV}=1 in the MCP server environment. Never enable it to \
         auto-run unreviewed scripts."
    )
}

// ============================================================================
// Parameter Structs
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunLuaScriptParams {
    /// Lua source to run in Aseprite's scripting environment.
    pub script: String,
    /// Optional sprite file to open before the script runs.
    pub file_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCliParams {
    /// Raw Aseprite CLI arguments (batch mode is always prepended).
    /// Example: ["sprite.ase", "--save-as", "out.png"].
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChangeColorModeParams {
    /// Path to the sprite file to convert (saved in place).
    pub file_path: String,
    /// Target colour mode: "rgb", "grayscale", or "indexed".
    pub mode: String,
}

// ============================================================================
// Tool Implementations
// ============================================================================

pub async fn run_lua_script(server: &AsepriteServer, p: RunLuaScriptParams) -> Result<String, String> {
    if !lua_execution_allowed() {
        return Err(lua_disabled_error("run_lua_script"));
    }
    match p.file_path.as_deref() {
        Some(path) => server.execute_script_on_file(path, &p.script).await,
        None => server.execute_script(&p.script).await,
    }
}

pub async fn execute_cli(server: &AsepriteServer, p: ExecuteCliParams) -> Result<String, String> {
    if !lua_execution_allowed() {
        return Err(lua_disabled_error("execute_cli"));
    }
    match server.run_cli(&p.args).await {
        Ok(out) if out.success => Ok(out.result_text()),
        Ok(out) => Err(out.result_text()),
        Err(e) => Err(format!("CLI execution failed: {e}")),
    }
}

/// Change a sprite file's colour mode and save it in place. Unlike `run_lua_script`,
/// this runs a FIXED, safe script (no caller-supplied code), so it is not gated.
pub async fn change_color_mode(
    server: &AsepriteServer,
    p: ChangeColorModeParams,
) -> Result<String, String> {
    let format = match p.mode.trim().to_ascii_lowercase().as_str() {
        "rgb" => "rgb",
        "grayscale" | "greyscale" | "gray" | "grey" => "grayscale",
        "indexed" => "indexed",
        other => {
            return Err(format!(
                "unknown colour mode {other:?} (use 'rgb', 'grayscale', or 'indexed')"
            ));
        }
    };
    let lua = format!(
        "local s = app.sprite\n\
         if not s then print('ERROR: no sprite opened') return end\n\
         app.command.ChangeColorMode{{ ui=false, format='{format}' }}\n\
         s:saveAs(s.filename)\n\
         print('OK: colour mode -> {format}')\n"
    );
    server.execute_script_on_file(&p.file_path, &lua).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_values_open_the_gate() {
        for v in ["1", "true", "TRUE", "Yes", " on ", "On"] {
            assert!(is_truthy(v), "{v:?} should be truthy");
        }
    }

    #[test]
    fn falsy_or_unset_keeps_gate_closed() {
        for v in ["0", "false", "no", "off", "", "maybe", "2"] {
            assert!(!is_truthy(v), "{v:?} should be falsy");
        }
    }

    #[test]
    fn disabled_error_is_actionable() {
        let e = lua_disabled_error("run_lua_script");
        // Names the tool, the env opt-in, and points at the security docs.
        assert!(e.contains("run_lua_script"));
        assert!(e.contains(ALLOW_LUA_ENV));
        assert!(e.contains("SECURITY.md") || e.contains("0003"));
    }
}
