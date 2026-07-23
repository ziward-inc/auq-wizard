use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;

const CLAUDE_SKILL: &str = include_str!("../resources/integrations/claude/auq/SKILL.md");
const CODEX_SKILL: &str = include_str!("../resources/integrations/codex/auq/SKILL.md");
const CODEX_OPENAI_YAML: &str =
    include_str!("../resources/integrations/codex/auq/agents/openai.yaml");

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    pub cli: bool,
    pub claude: bool,
    pub codex: bool,
    pub autostart: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub auq_enabled: bool,
    pub cli: bool,
    pub claude_skill: bool,
    pub claude_hook: bool,
    pub codex_skill: bool,
    pub codex_hooks: bool,
    pub autostart: bool,
    pub path_ready: bool,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub fn get_integration_status(app: AppHandle) -> Result<IntegrationStatus, String> {
    integration_status(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn install_integrations(
    app: AppHandle,
    options: InstallOptions,
) -> Result<IntegrationStatus, String> {
    install_integrations_inner(&app, &options).map_err(|error| error.to_string())
}

fn install_integrations_inner(
    app: &AppHandle,
    options: &InstallOptions,
) -> Result<IntegrationStatus> {
    let home = dirs::home_dir().context("could not locate the home directory")?;
    let cli_path = home.join(".local/bin/auq");

    if options.cli {
        install_cli_symlink(&cli_path)?;
    }
    if options.claude {
        install_managed_file(&home.join(".claude/skills/auq/SKILL.md"), CLAUDE_SKILL)?;
        merge_claude_settings(&home.join(".claude/settings.json"), &cli_path)?;
    }
    if options.codex {
        let skill_root = home.join(".agents/skills/auq");
        install_managed_file(&skill_root.join("SKILL.md"), CODEX_SKILL)?;
        install_managed_file(&skill_root.join("agents/openai.yaml"), CODEX_OPENAI_YAML)?;
        merge_codex_hooks(&home.join(".codex/hooks.json"), &cli_path)?;
    }
    if options.autostart {
        app.autolaunch().enable()?;
    }

    let store = app.store("settings.json")?;
    store.set("onboardingComplete", json!(true));
    store.save()?;
    integration_status(app)
}

fn integration_status(app: &AppHandle) -> Result<IntegrationStatus> {
    let home = dirs::home_dir().context("could not locate the home directory")?;
    let cli_path = home.join(".local/bin/auq");
    let claude_settings = read_json_or_default(&home.join(".claude/settings.json"))?;
    let codex_hooks = read_json_or_default(&home.join(".codex/hooks.json"))?;
    let path_ready = std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|path| path == home.join(".local/bin"))
    });
    let mut warnings = Vec::new();
    if !path_ready {
        warnings.push("~/.local/bin is not currently present in PATH.".into());
    }
    if contains_hook_command(&codex_hooks, "codex-hook") {
        warnings.push("Review and trust the AUQ hooks with /hooks in Codex.".into());
    }

    Ok(IntegrationStatus {
        auq_enabled: crate::preferences::is_enabled()?,
        cli: cli_path.exists(),
        claude_skill: home.join(".claude/skills/auq/SKILL.md").exists(),
        claude_hook: contains_hook_command(&claude_settings, "claude-hook"),
        codex_skill: home.join(".agents/skills/auq/SKILL.md").exists(),
        codex_hooks: contains_hook_event_command(&codex_hooks, "PreToolUse", "codex-hook")
            && contains_hook_event_command(&codex_hooks, "PermissionRequest", "codex-hook"),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        path_ready,
        warnings,
    })
}

fn install_cli_symlink(destination: &Path) -> Result<()> {
    let target = std::env::current_exe().context("could not locate AUQ Wizard executable")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.symlink_metadata().is_ok() {
        if fs::read_link(destination).ok().as_deref() == Some(target.as_path()) {
            return Ok(());
        }
        bail!(
            "{} already exists and was not replaced; remove it manually if it belongs to an older AUQ installation",
            destination.display()
        );
    }
    symlink(&target, destination).with_context(|| {
        format!(
            "failed to link {} to {}",
            destination.display(),
            target.display()
        )
    })
}

fn install_managed_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let hash_path = managed_hash_path(path)?;
    if path.exists() {
        let current = fs::read(path)?;
        if current == content.as_bytes() {
            return Ok(());
        }
        let current_hash = hash_bytes(&current);
        let managed_hash = fs::read_to_string(&hash_path).unwrap_or_default();
        if managed_hash.trim() != current_hash {
            bail!(
                "{} has user changes and was not overwritten",
                path.display()
            );
        }
        let backup = path.with_extension(format!(
            "backup.{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));
        fs::copy(path, backup)?;
    }
    atomic_write(path, content.as_bytes())?;
    atomic_write(&hash_path, hash_bytes(content.as_bytes()).as_bytes())?;
    Ok(())
}

fn merge_claude_settings(path: &Path, cli_path: &Path) -> Result<()> {
    let mut root = read_json_or_default(path)?;
    let object = root
        .as_object_mut()
        .context("Claude settings must be a JSON object")?;
    let hooks = object.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .context("Claude hooks must be an object")?;
    let groups = hooks
        .entry("PreToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("Claude PreToolUse hooks must be an array")?;
    if !groups
        .iter()
        .any(|group| contains_text(group, "claude-hook"))
    {
        groups.push(json!({
            "matcher": "AskUserQuestion",
            "hooks": [{
                "type": "command",
                "command": cli_path.to_string_lossy(),
                "args": ["claude-hook"],
                "timeout": 86400,
                "statusMessage": "Waiting for an answer in AUQ Wizard"
            }]
        }));
    }

    let permissions = object.entry("permissions").or_insert_with(|| json!({}));
    let permissions = permissions
        .as_object_mut()
        .context("Claude permissions must be an object")?;
    let allow = permissions
        .entry("allow")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("Claude permission allow rules must be an array")?;
    let fallback_rule = Value::String("Bash(auq ask *)".into());
    if !allow.contains(&fallback_rule) {
        allow.push(fallback_rule);
    }
    write_json_with_backup(path, &root)
}

fn merge_codex_hooks(path: &Path, cli_path: &Path) -> Result<()> {
    let mut root = read_json_or_default(path)?;
    let object = root
        .as_object_mut()
        .context("Codex hooks must be a JSON object")?;
    object.entry("description").or_insert_with(|| {
        Value::String("User lifecycle hooks managed alongside AUQ Wizard.".into())
    });
    let hooks = object.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .context("Codex hooks must be an object")?;
    add_codex_hook_group(hooks, "PreToolUse", "pre-tool-use", cli_path)?;
    add_codex_hook_group(hooks, "PermissionRequest", "permission-request", cli_path)?;
    write_json_with_backup(path, &root)
}

fn add_codex_hook_group(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    argument: &str,
    cli_path: &Path,
) -> Result<()> {
    let groups = hooks
        .entry(event)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .with_context(|| format!("Codex {event} hooks must be an array"))?;
    if groups.iter().any(|group| contains_text(group, argument)) {
        return Ok(());
    }
    let command = format!(
        "{} codex-hook {argument}",
        shell_quote(&cli_path.to_string_lossy())
    );
    groups.push(json!({
        "matcher": "^Bash$",
        "hooks": [{
            "type": "command",
            "command": command,
            "timeout": 30,
            "statusMessage": "Validating AUQ command"
        }]
    }));
    Ok(())
}

fn read_json_or_default(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("{} contains invalid JSON", path.display()))
}

fn write_json_with_backup(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(value)?;
    if path.exists() && fs::read(path)? == encoded {
        return Ok(());
    }
    if path.exists() {
        let backup = path.with_extension(format!(
            "json.backup.{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));
        fs::copy(path, backup)?;
    }
    atomic_write(path, &encoded)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("managed file has no parent")?;
    fs::create_dir_all(parent)?;
    let file_name = path.file_name().context("managed file has no name")?;
    let temporary = parent.join(format!(
        ".{}.auq-tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn managed_hash_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().context("managed file has no parent")?;
    let file_name = path.file_name().context("managed file has no name")?;
    Ok(parent.join(format!(".{}.auq-hash", file_name.to_string_lossy())))
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn contains_hook_command(value: &Value, needle: &str) -> bool {
    contains_hook_event_command(value, "PreToolUse", needle)
        || contains_hook_event_command(value, "PermissionRequest", needle)
}

fn contains_hook_event_command(value: &Value, event: &str, needle: &str) -> bool {
    value
        .pointer(&format!("/hooks/{event}"))
        .is_some_and(|hooks| contains_text(hooks, needle))
}

fn contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(value) => value.contains(needle),
        Value::Array(values) => values.iter().any(|value| contains_text(value, needle)),
        Value::Object(values) => values.values().any(|value| contains_text(value, needle)),
        _ => false,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/A B/it's"), "'/A B/it'\\''s'");
    }

    #[test]
    fn hook_detection_walks_nested_values() {
        let value = json!({"hooks": {"PreToolUse": [{"command": "auq claude-hook"}]}});
        assert!(contains_hook_command(&value, "claude-hook"));
    }
}
