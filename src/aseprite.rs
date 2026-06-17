//! Aseprite CLI runner.
//!
//! Locates the Aseprite executable once and runs Lua scripts or raw CLI commands in
//! `--batch` mode. This powers the *offline* escape-hatch tools (`run_lua_script`,
//! `execute_cli`, `export_*`, `change_color_mode`). The live drawing path does NOT
//! use this — it talks to the in-editor plugin over the WebSocket bridge.

use anyhow::{Context, Result, anyhow};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Hard ceiling on a single Aseprite invocation; a hung or over-complex batch run is
/// killed rather than blocking the server indefinitely.
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// Environment override for the Aseprite executable location.
const EXE_ENV: &str = "ASEPRITE_PATH";

/// Captured result of one Aseprite batch invocation.
#[derive(Debug)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl ScriptOutput {
    /// Collapse the capture into a single human/agent-facing line: the trimmed
    /// stdout on success, else an `Error: …` from whichever stream carried a message.
    pub fn result_text(&self) -> String {
        if self.success {
            let out = self.stdout.trim();
            if out.is_empty() {
                "Operation completed successfully.".to_string()
            } else {
                out.to_string()
            }
        } else {
            let detail = [self.stderr.trim(), self.stdout.trim()]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or("unknown error");
            format!("Error: {detail}")
        }
    }
}

/// Runs Aseprite in `--batch` mode. The executable is resolved once at construction.
#[derive(Debug)]
pub struct AsepriteRunner {
    exe: PathBuf,
    scratch: PathBuf,
}

impl AsepriteRunner {
    /// Resolve the Aseprite executable and prepare a scratch dir for temp scripts.
    pub fn new() -> Result<Self> {
        let exe = locate_executable()?;
        let scratch = std::env::temp_dir().join("aseprite_mcp");
        std::fs::create_dir_all(&scratch)
            .context("creating the Aseprite MCP scratch directory")?;
        info!("Aseprite executable: {}", exe.display());
        Ok(Self { exe, scratch })
    }

    /// Run a Lua script with no sprite pre-loaded.
    pub async fn run_script(&self, lua: &str) -> Result<ScriptOutput> {
        let script = self.stage_script(lua).await?;
        let out = self
            .invoke(vec!["--script".into(), script.clone().into_os_string()])
            .await;
        self.discard(&script).await;
        out
    }

    /// Run a Lua script with `file_path` opened first (it becomes `app.sprite`).
    pub async fn run_script_on_file(&self, file_path: &str, lua: &str) -> Result<ScriptOutput> {
        let script = self.stage_script(lua).await?;
        let out = self
            .invoke(vec![
                file_path.into(),
                "--script".into(),
                script.clone().into_os_string(),
            ])
            .await;
        self.discard(&script).await;
        out
    }

    /// Run Aseprite with raw CLI arguments (after the implicit `--batch`).
    pub async fn run_cli(&self, args: &[String]) -> Result<ScriptOutput> {
        self.invoke(args.iter().map(OsString::from).collect()).await
    }

    /// Write `lua` to a uniquely-named scratch file and return its path.
    async fn stage_script(&self, lua: &str) -> Result<PathBuf> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "script_{}_{}.lua",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let path = self.scratch.join(name);
        tokio::fs::write(&path, lua)
            .await
            .context("writing a temporary Lua script")?;
        Ok(path)
    }

    /// Best-effort removal of a staged scratch script.
    async fn discard(&self, path: &PathBuf) {
        if let Err(e) = tokio::fs::remove_file(path).await {
            warn!("could not remove temp script {}: {e}", path.display());
        }
    }

    /// Spawn `aseprite --batch <extra…>`, enforce the timeout, capture both streams.
    async fn invoke(&self, extra: Vec<OsString>) -> Result<ScriptOutput> {
        debug!("aseprite --batch {extra:?}");
        let mut child = Command::new(&self.exe)
            .arg("--batch")
            .args(&extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning the Aseprite process")?;

        // Detach the pipes before awaiting so the child can still be killed on timeout.
        let mut out_pipe = child.stdout.take();
        let mut err_pipe = child.stderr.take();

        let status = match tokio::time::timeout(RUN_TIMEOUT, child.wait()).await {
            Ok(res) => res.context("waiting on the Aseprite process")?,
            Err(_) => {
                warn!("Aseprite exceeded {}s — killing it", RUN_TIMEOUT.as_secs());
                child.kill().await.ok();
                return Err(anyhow!(
                    "Aseprite timed out after {}s (the operation is too complex or the process is stuck)",
                    RUN_TIMEOUT.as_secs()
                ));
            }
        };

        Ok(ScriptOutput {
            stdout: drain(&mut out_pipe).await,
            stderr: drain(&mut err_pipe).await,
            success: status.success(),
        })
    }
}

/// Read a captured pipe to a lossy-UTF-8 string (empty when the pipe is absent).
async fn drain<R: AsyncReadExt + Unpin>(pipe: &mut Option<R>) -> String {
    let Some(r) = pipe.as_mut() else {
        return String::new();
    };
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).await.ok();
    String::from_utf8_lossy(&buf).into_owned()
}

/// Resolve the Aseprite executable: the `ASEPRITE_PATH` override first, then the
/// well-known install locations and `PATH` for the host OS.
fn locate_executable() -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(EXE_ENV) {
        let p = PathBuf::from(&raw);
        if p.exists() {
            return Ok(p);
        }
        debug!("{EXE_ENV}={raw} is not on disk; falling back to a search");
    }

    for candidate in os_candidates() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    if let Some(p) = which_on_path() {
        return Ok(p);
    }

    Err(anyhow!(
        "Aseprite executable not found. Set {EXE_ENV} to its full path."
    ))
}

/// Well-known per-OS install locations (factual paths, checked in order).
#[cfg(target_os = "windows")]
fn os_candidates() -> Vec<PathBuf> {
    [
        r"C:\Program Files\Aseprite\Aseprite.exe",
        r"C:\Program Files (x86)\Steam\steamapps\common\Aseprite\Aseprite.exe",
        r"C:\Program Files\Steam\steamapps\common\Aseprite\Aseprite.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "macos")]
fn os_candidates() -> Vec<PathBuf> {
    vec![PathBuf::from(
        "/Applications/Aseprite.app/Contents/MacOS/aseprite",
    )]
}

#[cfg(target_os = "linux")]
fn os_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        v.push(PathBuf::from(format!(
            "{home}/.steam/debian-installation/steamapps/common/Aseprite/aseprite"
        )));
    }
    v
}

/// Ask the OS resolver (`where` on Windows, `which` elsewhere) for `aseprite` on PATH.
fn which_on_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let finder = "where";
    #[cfg(not(target_os = "windows"))]
    let finder = "which";

    let output = std::process::Command::new(finder)
        .arg("aseprite")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let first = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    let p = PathBuf::from(first);
    p.exists().then_some(p)
}
