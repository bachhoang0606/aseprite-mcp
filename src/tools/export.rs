use rmcp::schemars;
use serde::Deserialize;

use crate::server::AsepriteServer;

// ============================================================================
// Parameter Structs
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportSpriteParams {
    /// Source sprite file to read.
    pub file_path: String,
    /// Destination path; the extension selects the format (png / gif / jpg / …).
    pub output_path: String,
    /// Integer upscale factor (2 = double size).
    pub scale: Option<u32>,
    /// Export only this layer (default: every visible layer).
    pub layer: Option<String>,
    /// Export only this animation tag's frames (default: all frames).
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportSpritesheetParams {
    /// Source sprite file to read.
    pub file_path: String,
    /// Destination image for the packed sheet (e.g. "sheet.png").
    pub output_image: String,
    /// Optional sidecar JSON describing the frames (e.g. "sheet.json").
    pub output_data: Option<String>,
    /// Packing layout: "horizontal" | "vertical" | "rows" | "columns" | "packed".
    pub sheet_type: Option<String>,
    /// Column count for the "rows" layout.
    pub columns: Option<u32>,
    /// Trim transparent margins from each frame.
    pub trim: Option<bool>,
    /// Emit animation tags as `meta.frameTags` in the JSON (default: true).
    pub list_tags: Option<bool>,
    /// Emit layer info as `meta.layers` in the JSON (default: false).
    pub list_layers: Option<bool>,
    /// Emit slice info as `meta.slices` in the JSON (default: false).
    pub list_slices: Option<bool>,
}

// ============================================================================
// Tool Implementations
// ============================================================================

/// Build the Aseprite CLI args for a single-sprite export: optional scale / layer /
/// tag filters, then `--save-as <out>`.
fn export_sprite_cli_args(p: &ExportSpriteParams, resolved_output: &str) -> Vec<String> {
    let mut args = vec![p.file_path.clone()];
    if let Some(scale) = p.scale {
        args.extend(["--scale".to_string(), scale.to_string()]);
    }
    if let Some(layer) = &p.layer {
        args.extend(["--layer".to_string(), layer.clone()]);
    }
    if let Some(tag) = &p.tag {
        args.extend(["--tag".to_string(), tag.clone()]);
    }
    args.extend(["--save-as".to_string(), resolved_output.to_string()]);
    args
}

pub async fn export_sprite(
    server: &AsepriteServer,
    p: ExportSpriteParams,
) -> Result<String, String> {
    let resolved_output = server.resolve_output_path(&p.output_path);
    let args = export_sprite_cli_args(&p, &resolved_output);
    match server.run_cli(&args).await {
        Ok(out) if out.success => Ok(format!("Exported {} -> {}", p.file_path, resolved_output)),
        Ok(out) => Err(out.result_text()),
        Err(e) => Err(format!("Export failed: {e}")),
    }
}

/// Builds the Aseprite CLI argument list for a spritesheet export.
///
/// When a JSON data file is requested, tag metadata (`meta.frameTags`) is
/// included by default so engines can key animations by tag without a separate
/// tag map (gap surfaced by the Tier-B 5.4 eval — see evals/RESULTS.md).
fn spritesheet_cli_args(
    p: &ExportSpritesheetParams,
    resolved_image: &str,
    resolved_data: Option<&str>,
) -> Vec<String> {
    let mut args = vec![p.file_path.clone(), "--sheet".to_string(), resolved_image.to_string()];

    if let Some(data_path) = resolved_data {
        args.push("--data".to_string());
        args.push(data_path.to_string());
        if p.list_tags.unwrap_or(true) {
            args.push("--list-tags".to_string());
        }
        if p.list_layers.unwrap_or(false) {
            args.push("--list-layers".to_string());
        }
        if p.list_slices.unwrap_or(false) {
            args.push("--list-slices".to_string());
        }
    }
    if let Some(ref sheet_type) = p.sheet_type {
        args.push("--sheet-type".to_string());
        args.push(sheet_type.clone());
    }
    if let Some(columns) = p.columns {
        args.push("--sheet-columns".to_string());
        args.push(columns.to_string());
    }
    if p.trim.unwrap_or(false) {
        args.push("--trim".to_string());
    }
    args
}

pub async fn export_spritesheet(
    server: &AsepriteServer,
    p: ExportSpritesheetParams,
) -> Result<String, String> {
    let resolved_image = server.resolve_output_path(&p.output_image);
    let resolved_data = p.output_data.as_ref().map(|d| server.resolve_output_path(d));
    let args = spritesheet_cli_args(&p, &resolved_image, resolved_data.as_deref());

    match server.run_cli(&args).await {
        Ok(output) => {
            if output.success {
                Ok(format!(
                    "Spritesheet exported: {}{}",
                    resolved_image,
                    resolved_data
                        .map(|d| format!(", data: {}", d))
                        .unwrap_or_default()
                ))
            } else {
                Err(output.result_text())
            }
        }
        Err(e) => Err(format!("Export failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(output_data: Option<&str>) -> ExportSpritesheetParams {
        ExportSpritesheetParams {
            file_path: "in.aseprite".into(),
            output_image: "sheet.png".into(),
            output_data: output_data.map(String::from),
            sheet_type: None,
            columns: None,
            trim: None,
            list_tags: None,
            list_layers: None,
            list_slices: None,
        }
    }

    #[test]
    fn data_export_includes_frame_tags_by_default() {
        let p = params(Some("sheet.json"));
        let args = spritesheet_cli_args(&p, "sheet.png", Some("sheet.json"));
        assert!(
            args.contains(&"--list-tags".to_string()),
            "JSON data must carry meta.frameTags by default: {args:?}"
        );
        assert!(!args.contains(&"--list-layers".to_string()));
        assert!(!args.contains(&"--list-slices".to_string()));
    }

    #[test]
    fn metadata_flags_are_opt_in_and_opt_out() {
        let mut p = params(Some("sheet.json"));
        p.list_tags = Some(false);
        p.list_layers = Some(true);
        p.list_slices = Some(true);
        let args = spritesheet_cli_args(&p, "sheet.png", Some("sheet.json"));
        assert!(!args.contains(&"--list-tags".to_string()));
        assert!(args.contains(&"--list-layers".to_string()));
        assert!(args.contains(&"--list-slices".to_string()));
    }

    #[test]
    fn no_data_file_means_no_metadata_flags() {
        let p = params(None);
        let args = spritesheet_cli_args(&p, "sheet.png", None);
        assert!(!args.iter().any(|a| a.starts_with("--list-") || a == "--data"));
    }
}
