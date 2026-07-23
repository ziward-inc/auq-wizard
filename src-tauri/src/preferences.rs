use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::broker::{AppState, BUNDLE_IDENTIFIER};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    auq_enabled: Option<bool>,
}

pub fn is_enabled() -> Result<bool> {
    read_enabled(&preferences_path()?)
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    write_enabled(&preferences_path()?, enabled)
}

#[tauri::command]
pub fn get_auq_enabled() -> Result<bool, String> {
    is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_auq_enabled(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    set_enabled(enabled).map_err(|error| error.to_string())?;
    state.broker.changed(&app);
    Ok(enabled)
}

fn preferences_path() -> Result<PathBuf> {
    Ok(dirs::data_dir()
        .context("could not locate the application data directory")?
        .join(BUNDLE_IDENTIFIER)
        .join("preferences.json"))
}

fn read_enabled(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let preferences: Preferences = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} contains invalid JSON", path.display()))?;
    Ok(preferences.auq_enabled.unwrap_or(true))
}

fn write_enabled(path: &Path, enabled: bool) -> Result<()> {
    let parent = path.parent().context("preferences path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".preferences.auq-tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(&Preferences {
        auq_enabled: Some(enabled),
    })?;
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_enabled_and_persists_changes() {
        let directory = std::env::temp_dir().join(format!("auq-prefs-{}", uuid::Uuid::now_v7()));
        let path = directory.join("preferences.json");

        assert!(read_enabled(&path).unwrap());
        write_enabled(&path, false).unwrap();
        assert!(!read_enabled(&path).unwrap());
        write_enabled(&path, true).unwrap();
        assert!(read_enabled(&path).unwrap());

        fs::remove_dir_all(directory).unwrap();
    }
}
