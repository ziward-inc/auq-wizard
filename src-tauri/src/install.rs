use std::{
    env, fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const CLAUDE_SKILL: &str = include_str!("../resources/integrations/claude/auq/SKILL.md");
const CODEX_SKILL: &str = include_str!("../resources/integrations/codex/auq/SKILL.md");
const CODEX_OPENAI_YAML: &str =
    include_str!("../resources/integrations/codex/auq/agents/openai.yaml");
const CODEX_RPC_TIMEOUT: Duration = Duration::from_secs(5);
const CODEX_RPC_REQUEST_ID: u64 = 2;

const EXPECTED_CODEX_HOOKS: [(&str, &str, &str); 2] = [
    ("preToolUse", "PreToolUse", "pre-tool-use"),
    (
        "permissionRequest",
        "PermissionRequest",
        "permission-request",
    ),
];

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    pub cli: bool,
    pub claude: bool,
    pub codex: bool,
    pub autostart: bool,
    #[serde(default)]
    pub replace_cli: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub auq_enabled: bool,
    pub cli: bool,
    pub cli_conflict: bool,
    pub claude_skill: bool,
    pub claude_hook: bool,
    pub codex_skill: bool,
    pub codex_hooks: bool,
    pub codex_hook_trust: CodexHookTrust,
    pub codex_hook_reviews: Vec<CodexHookReview>,
    pub autostart: bool,
    pub path_ready: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexHookTrust {
    #[default]
    Unavailable,
    NotInstalled,
    Trusted,
    Untrusted,
    Modified,
    Disabled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHookReview {
    pub event_name: String,
    pub command: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredCodexHook {
    key: String,
    event_name: String,
    handler_type: String,
    matcher: String,
    command: String,
    source_path: PathBuf,
    source: String,
    enabled: bool,
    is_managed: bool,
    current_hash: String,
    trust_status: String,
}

#[derive(Debug, Deserialize)]
struct HooksListResponse {
    data: Vec<HooksListEntry>,
}

#[derive(Debug, Deserialize)]
struct HooksListEntry {
    cwd: PathBuf,
    hooks: Vec<DiscoveredCodexHook>,
    #[serde(default)]
    errors: Vec<HookLoadError>,
}

#[derive(Debug, Deserialize)]
struct HookLoadError {
    message: String,
}

#[derive(Debug)]
struct CodexHookSnapshot {
    trust: CodexHookTrust,
    hooks: Vec<DiscoveredCodexHook>,
}

#[tauri::command]
pub async fn get_integration_status(app: AppHandle) -> Result<IntegrationStatus, String> {
    integration_status(&app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn install_integrations(
    app: AppHandle,
    options: InstallOptions,
) -> Result<IntegrationStatus, String> {
    install_integrations_inner(&app, &options)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn trust_codex_hooks(app: AppHandle) -> Result<IntegrationStatus, String> {
    trust_codex_hooks_inner(&app)
        .await
        .map_err(|error| error.to_string())
}

async fn install_integrations_inner(
    app: &AppHandle,
    options: &InstallOptions,
) -> Result<IntegrationStatus> {
    let home = dirs::home_dir().context("could not locate the home directory")?;
    let cli_path = home.join(".local/bin/auq");

    if options.cli {
        install_cli_symlink(&cli_path, options.replace_cli)?;
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
    integration_status(app).await
}

async fn integration_status(app: &AppHandle) -> Result<IntegrationStatus> {
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
    let cli_target = std::env::current_exe().context("could not locate AUQ Wizard executable")?;
    let cli = fs::read_link(&cli_path).ok().as_deref() == Some(cli_target.as_path());
    let cli_conflict = cli_path.symlink_metadata().is_ok() && !cli;
    let codex_hooks_installed =
        contains_hook_event_command(&codex_hooks, "PreToolUse", "codex-hook")
            && contains_hook_event_command(&codex_hooks, "PermissionRequest", "codex-hook");
    let codex_snapshot = if codex_hooks_installed {
        match query_auq_codex_hooks(&home, &cli_path).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log::warn!("could not inspect Codex hook trust: {error:#}");
                CodexHookSnapshot {
                    trust: CodexHookTrust::Unavailable,
                    hooks: Vec::new(),
                }
            }
        }
    } else {
        CodexHookSnapshot {
            trust: CodexHookTrust::NotInstalled,
            hooks: Vec::new(),
        }
    };
    let codex_hook_reviews = codex_snapshot
        .hooks
        .iter()
        .map(|hook| CodexHookReview {
            event_name: display_codex_event_name(&hook.event_name).to_string(),
            command: hook.command.clone(),
        })
        .collect();

    Ok(IntegrationStatus {
        auq_enabled: crate::preferences::is_enabled()?,
        cli,
        cli_conflict,
        claude_skill: home.join(".claude/skills/auq/SKILL.md").exists(),
        claude_hook: contains_hook_command(&claude_settings, "claude-hook"),
        codex_skill: home.join(".agents/skills/auq/SKILL.md").exists(),
        codex_hooks: codex_hooks_installed,
        codex_hook_trust: codex_snapshot.trust,
        codex_hook_reviews,
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        path_ready,
        warnings,
    })
}

async fn trust_codex_hooks_inner(app: &AppHandle) -> Result<IntegrationStatus> {
    let home = dirs::home_dir().context("could not locate the home directory")?;
    let cli_path = home.join(".local/bin/auq");
    let snapshot = query_auq_codex_hooks(&home, &cli_path).await?;
    match snapshot.trust {
        CodexHookTrust::Trusted => return integration_status(app).await,
        CodexHookTrust::Untrusted | CodexHookTrust::Modified => {}
        CodexHookTrust::Disabled => bail!("AUQ hooks are disabled in Codex"),
        CodexHookTrust::NotInstalled => bail!("AUQ hooks are not installed in Codex"),
        CodexHookTrust::Unavailable => bail!("Codex hook trust is unavailable"),
    }

    let state = snapshot
        .hooks
        .into_iter()
        .map(|hook| {
            (
                hook.key,
                json!({
                    "trusted_hash": hook.current_hash,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    codex_app_server_request(
        &home,
        "config/batchWrite",
        json!({
            "edits": [{
                "keyPath": "hooks.state",
                "value": Value::Object(state),
                "mergeStrategy": "upsert"
            }],
            "filePath": null,
            "expectedVersion": null,
            "reloadUserConfig": true
        }),
    )
    .await?;

    let status = integration_status(app).await?;
    ensure!(
        status.codex_hook_trust == CodexHookTrust::Trusted,
        "Codex did not report the AUQ hooks as trusted"
    );
    Ok(status)
}

async fn query_auq_codex_hooks(home: &Path, cli_path: &Path) -> Result<CodexHookSnapshot> {
    let result = codex_app_server_request(
        home,
        "hooks/list",
        json!({
            "cwds": [home],
        }),
    )
    .await?;
    let response: HooksListResponse =
        serde_json::from_value(result).context("Codex hooks/list returned an invalid response")?;
    let entry = response
        .data
        .into_iter()
        .find(|entry| entry.cwd == home)
        .context("Codex hooks/list did not return the requested directory")?;
    ensure!(
        entry.errors.is_empty(),
        "Codex hooks/list failed: {}",
        entry
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    );

    auq_codex_hook_snapshot(entry, home, cli_path)
}

fn auq_codex_hook_snapshot(
    entry: HooksListEntry,
    home: &Path,
    cli_path: &Path,
) -> Result<CodexHookSnapshot> {
    let source_path = home.join(".codex/hooks.json");
    let mut hooks = Vec::with_capacity(EXPECTED_CODEX_HOOKS.len());
    for (event_name, _, argument) in EXPECTED_CODEX_HOOKS {
        let expected_command = codex_hook_command(cli_path, argument);
        let matches = entry
            .hooks
            .iter()
            .filter(|hook| {
                hook.event_name == event_name
                    && hook.handler_type == "command"
                    && hook.matcher == "^Bash$"
                    && hook.command == expected_command
                    && hook.source_path == source_path
                    && hook.source == "user"
                    && !hook.is_managed
                    && hook.current_hash.starts_with("sha256:")
            })
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "Codex did not report exactly one installed AUQ {event_name} hook"
        );
        hooks.push(matches.into_iter().next().expect("one hook was matched"));
    }

    let trust = if hooks.iter().any(|hook| !hook.enabled) {
        CodexHookTrust::Disabled
    } else if hooks.iter().any(|hook| hook.trust_status == "modified") {
        CodexHookTrust::Modified
    } else if hooks.iter().any(|hook| hook.trust_status == "untrusted") {
        CodexHookTrust::Untrusted
    } else if hooks.iter().all(|hook| hook.trust_status == "trusted") {
        CodexHookTrust::Trusted
    } else {
        CodexHookTrust::Unavailable
    };

    Ok(CodexHookSnapshot { trust, hooks })
}

fn codex_hook_command(cli_path: &Path, argument: &str) -> String {
    format!(
        "{} codex-hook {argument}",
        shell_quote(&cli_path.to_string_lossy())
    )
}

fn display_codex_event_name(event_name: &str) -> &str {
    EXPECTED_CODEX_HOOKS
        .iter()
        .find_map(|(wire_name, display_name, _)| {
            (*wire_name == event_name).then_some(*display_name)
        })
        .unwrap_or(event_name)
}

async fn codex_app_server_request(cwd: &Path, method: &str, params: Value) -> Result<Value> {
    let executable = find_codex_executable(cwd)?;
    let mut child = Command::new(&executable)
        .arg("app-server")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to start {}", executable.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .context("Codex app-server stdin was unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("Codex app-server stdout was unavailable")?;
    let messages = [
        json!({
            "method": "initialize",
            "id": 1,
            "params": {
                "clientInfo": {
                    "name": "auq-wizard",
                    "title": "AUQ Wizard",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true
                }
            }
        }),
        json!({
            "method": "initialized",
            "params": {}
        }),
        json!({
            "method": method,
            "id": CODEX_RPC_REQUEST_ID,
            "params": params
        }),
    ];
    for message in messages {
        stdin
            .write_all(&serde_json::to_vec(&message)?)
            .await
            .context("failed to write to Codex app-server")?;
        stdin
            .write_all(b"\n")
            .await
            .context("failed to write to Codex app-server")?;
    }
    stdin
        .flush()
        .await
        .context("failed to flush Codex app-server request")?;

    let mut lines = BufReader::new(stdout).lines();
    let response = tokio::time::timeout(CODEX_RPC_TIMEOUT, async {
        loop {
            let line = lines
                .next_line()
                .await
                .context("failed to read from Codex app-server")?
                .context("Codex app-server exited before responding")?;
            let message: Value =
                serde_json::from_str(&line).context("Codex app-server returned invalid JSON")?;
            if message.get("id").and_then(Value::as_u64) != Some(CODEX_RPC_REQUEST_ID) {
                continue;
            }
            if let Some(error) = message.get("error") {
                bail!("Codex app-server request failed: {error}");
            }
            return message
                .get("result")
                .cloned()
                .context("Codex app-server response did not include a result");
        }
    })
    .await;

    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    match response {
        Ok(result) => result,
        Err(_) => bail!("Codex app-server request timed out"),
    }
}

fn find_codex_executable(home: &Path) -> Result<PathBuf> {
    let path_candidates = env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths)
                .map(|path| path.join("codex"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let known_candidates = [
        home.join(".local/bin/codex"),
        home.join(".npm-global/bin/codex"),
        home.join(".bun/bin/codex"),
        home.join(".volta/bin/codex"),
        home.join("Library/pnpm/codex"),
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/usr/bin/codex"),
    ];
    path_candidates
        .into_iter()
        .chain(known_candidates)
        .find(|path| path.is_file())
        .map(|path| native_codex_executable(&path).unwrap_or(path))
        .context("Codex CLI was not found")
}

fn native_codex_executable(executable: &Path) -> Option<PathBuf> {
    let (platform_package, target_triple) = match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => ("@openai/codex-darwin-arm64", "aarch64-apple-darwin"),
        ("macos", "x86_64") => ("@openai/codex-darwin-x64", "x86_64-apple-darwin"),
        _ => return None,
    };
    let wrapper = fs::canonicalize(executable).ok()?;
    if wrapper.file_name()? != "codex.js" || wrapper.parent()?.file_name()? != "bin" {
        return None;
    }
    let package_root = wrapper.parent()?.parent()?;
    [
        package_root
            .join("node_modules")
            .join(platform_package)
            .join("vendor")
            .join(target_triple)
            .join("bin/codex"),
        package_root
            .join("vendor")
            .join(target_triple)
            .join("bin/codex"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn install_cli_symlink(destination: &Path, replace_existing: bool) -> Result<()> {
    let target = std::env::current_exe().context("could not locate AUQ Wizard executable")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.symlink_metadata().is_ok() {
        if fs::read_link(destination).ok().as_deref() == Some(target.as_path()) {
            return Ok(());
        }
        if !replace_existing {
            bail!(
                "{} already exists; confirm replacement to continue",
                destination.display()
            );
        }
        let backup = destination.with_extension(format!(
            "backup.{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S%3f")
        ));
        fs::rename(destination, &backup).with_context(|| {
            format!(
                "failed to back up {} to {}",
                destination.display(),
                backup.display()
            )
        })?;
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
    let command = codex_hook_command(cli_path, argument);
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
    fn cli_install_requires_confirmation_before_replacing() {
        let root = std::env::temp_dir().join(format!("auq-install-test-{}", uuid::Uuid::now_v7()));
        let destination = root.join("auq");
        fs::create_dir_all(&root).unwrap();
        fs::write(&destination, b"old auq").unwrap();

        let error = install_cli_symlink(&destination, false).unwrap_err();
        assert!(error.to_string().contains("confirm replacement"));
        assert_eq!(fs::read(&destination).unwrap(), b"old auq");

        install_cli_symlink(&destination, true).unwrap();
        assert_eq!(
            fs::read_link(&destination).unwrap(),
            std::env::current_exe().unwrap()
        );
        let backup = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("auq.backup.")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"old auq");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/A B/it's"), "'/A B/it'\\''s'");
    }

    #[test]
    fn native_codex_executable_resolves_the_binary_bundled_with_the_npm_wrapper() {
        let (platform_package, target_triple) = match (env::consts::OS, env::consts::ARCH) {
            ("macos", "aarch64") => ("@openai/codex-darwin-arm64", "aarch64-apple-darwin"),
            ("macos", "x86_64") => ("@openai/codex-darwin-x64", "x86_64-apple-darwin"),
            _ => return,
        };
        let root = std::env::temp_dir().join(format!("auq-codex-test-{}", uuid::Uuid::now_v7()));
        let package_root = root.join("node_modules/@openai/codex");
        let wrapper = package_root.join("bin/codex.js");
        let native = package_root
            .join("node_modules")
            .join(platform_package)
            .join("vendor")
            .join(target_triple)
            .join("bin/codex");
        fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
        fs::create_dir_all(native.parent().unwrap()).unwrap();
        fs::write(&wrapper, b"#!/usr/bin/env node").unwrap();
        fs::write(&native, b"codex").unwrap();

        assert_eq!(
            native_codex_executable(&wrapper),
            Some(fs::canonicalize(native).unwrap())
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_detection_walks_nested_values() {
        let value = json!({"hooks": {"PreToolUse": [{"command": "auq claude-hook"}]}});
        assert!(contains_hook_command(&value, "claude-hook"));
    }

    #[test]
    fn codex_hook_snapshot_accepts_only_the_exact_auq_hooks() {
        let home = PathBuf::from("/Users/test");
        let cli_path = home.join(".local/bin/auq");
        let entry = HooksListEntry {
            cwd: home.clone(),
            hooks: vec![
                discovered_codex_hook(
                    &home,
                    &cli_path,
                    "preToolUse",
                    "pre-tool-use",
                    "trusted",
                    true,
                ),
                discovered_codex_hook(
                    &home,
                    &cli_path,
                    "permissionRequest",
                    "permission-request",
                    "trusted",
                    true,
                ),
                DiscoveredCodexHook {
                    key: "lookalike".into(),
                    event_name: "preToolUse".into(),
                    handler_type: "command".into(),
                    matcher: "^Bash$".into(),
                    command: "other codex-hook pre-tool-use".into(),
                    source_path: home.join(".codex/hooks.json"),
                    source: "user".into(),
                    enabled: true,
                    is_managed: false,
                    current_hash: "sha256:lookalike".into(),
                    trust_status: "untrusted".into(),
                },
            ],
            errors: Vec::new(),
        };

        let snapshot = auq_codex_hook_snapshot(entry, &home, &cli_path).unwrap();
        assert_eq!(snapshot.trust, CodexHookTrust::Trusted);
        assert_eq!(snapshot.hooks.len(), 2);
        assert!(snapshot
            .hooks
            .iter()
            .all(|hook| hook.command.starts_with("'/Users/test/.local/bin/auq'")));
    }

    #[test]
    fn codex_hook_snapshot_reports_changed_and_disabled_hooks() {
        let home = PathBuf::from("/Users/test");
        let cli_path = home.join(".local/bin/auq");
        let snapshot = auq_codex_hook_snapshot(
            HooksListEntry {
                cwd: home.clone(),
                hooks: vec![
                    discovered_codex_hook(
                        &home,
                        &cli_path,
                        "preToolUse",
                        "pre-tool-use",
                        "modified",
                        true,
                    ),
                    discovered_codex_hook(
                        &home,
                        &cli_path,
                        "permissionRequest",
                        "permission-request",
                        "trusted",
                        true,
                    ),
                ],
                errors: Vec::new(),
            },
            &home,
            &cli_path,
        )
        .unwrap();
        assert_eq!(snapshot.trust, CodexHookTrust::Modified);

        let disabled = auq_codex_hook_snapshot(
            HooksListEntry {
                cwd: home.clone(),
                hooks: vec![
                    discovered_codex_hook(
                        &home,
                        &cli_path,
                        "preToolUse",
                        "pre-tool-use",
                        "trusted",
                        false,
                    ),
                    discovered_codex_hook(
                        &home,
                        &cli_path,
                        "permissionRequest",
                        "permission-request",
                        "trusted",
                        true,
                    ),
                ],
                errors: Vec::new(),
            },
            &home,
            &cli_path,
        )
        .unwrap();
        assert_eq!(disabled.trust, CodexHookTrust::Disabled);
    }

    fn discovered_codex_hook(
        home: &Path,
        cli_path: &Path,
        event_name: &str,
        argument: &str,
        trust_status: &str,
        enabled: bool,
    ) -> DiscoveredCodexHook {
        DiscoveredCodexHook {
            key: format!(
                "{}:{event_name}:0:0",
                home.join(".codex/hooks.json").display()
            ),
            event_name: event_name.into(),
            handler_type: "command".into(),
            matcher: "^Bash$".into(),
            command: codex_hook_command(cli_path, argument),
            source_path: home.join(".codex/hooks.json"),
            source: "user".into(),
            enabled,
            is_managed: false,
            current_hash: format!("sha256:{argument}"),
            trust_status: trust_status.into(),
        }
    }
}
