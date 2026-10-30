//! The desktop app's config.json, which the CLI shares: the journal folder
//! and the capture sources live there, per device, outside the journal.

use anyhow::{Context, Result};
use dayfile_core::CaptureConfig;
use serde_json::Value;
use std::path::PathBuf;

/// `$DAYFILE_CONFIG`, else the app's config dir. Reads fall back to the id
/// the app had before the rename (dev.journal.desktop).
fn paths() -> Vec<PathBuf> {
    if let Some(p) = std::env::var_os("DAYFILE_CONFIG") {
        return vec![PathBuf::from(p)];
    }
    let Some(dir) = dirs::config_dir() else { return Vec::new() };
    ["dev.dayfile.app", "dev.journal.desktop"].iter().map(|id| dir.join(id).join("config.json")).collect()
}

pub fn load() -> Value {
    paths()
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

pub fn capture() -> CaptureConfig {
    load().get("capture").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

/// Write `capture` back, keeping every other key (lock, reminder, root).
pub fn save_capture(cfg: &CaptureConfig) -> Result<PathBuf> {
    let path = paths().into_iter().next().context("no config directory on this system")?;
    let mut v = load();
    v.as_object_mut().context("config.json is not an object")?.insert("capture".into(), serde_json::to_value(cfg)?);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&v)?)?;
    Ok(path)
}

pub fn root() -> Option<PathBuf> {
    let root = PathBuf::from(load().get("root")?.as_str()?);
    root.join("entries").is_dir().then_some(root)
}
