use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
    process::Command,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};
use uuid::Uuid;

use crate::{
    broker::{socket_path, BUNDLE_IDENTIFIER},
    protocol::{
        encode_frame, AnswerPayload, AnswerValue, AskPayload, ClientMessage, RequestStatus,
        ServerMessage, StoredRequest, MAX_ASK_PAYLOAD_BYTES, MAX_FRAME_BYTES, PROTOCOL_VERSION,
    },
};

const HOST_READY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Parser)]
#[command(name = "auq", version, about = "Ask a user through AUQ Wizard")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Submit an AUQ JSON payload from stdin and wait for its answer.
    Ask {
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Wait for an existing request.
    Wait { request_id: String },
    /// Show an existing request's status.
    Status { request_id: String },
    /// Cancel a pending request.
    Cancel { request_id: String },
    /// Enable new AUQ GUI requests.
    Enable,
    /// Disable new AUQ GUI requests and use native agent interaction.
    Disable,
    /// Show whether new AUQ GUI requests are enabled.
    Enabled,
    #[command(hide = true)]
    ClaudeHook,
    #[command(hide = true)]
    CodexHook { event: CodexHookEvent },
}

#[derive(Clone, Copy, ValueEnum)]
enum CodexHookEvent {
    PreToolUse,
    PermissionRequest,
}

#[derive(Debug)]
struct TerminalResult {
    request_id: String,
    status: RequestStatus,
    result: Option<AnswerPayload>,
}

pub fn is_cli_invocation() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let executable_name = args
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if executable_name == "auq" {
        return true;
    }
    matches!(
        args.get(1).map(String::as_str),
        Some("ask")
            | Some("wait")
            | Some("status")
            | Some("cancel")
            | Some("enable")
            | Some("disable")
            | Some("enabled")
            | Some("claude-hook")
            | Some("codex-hook")
            | Some("--help")
            | Some("-h")
            | Some("--version")
            | Some("-V")
    )
}

pub fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("auq: failed to initialize runtime: {error}");
            return 1;
        }
    };
    match runtime.block_on(run_async()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("auq: {error:#}");
            1
        }
    }
}

async fn run_async() -> Result<i32> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(0);
    };
    match command {
        Commands::Ask { request_id } => {
            ensure_auq_enabled()?;
            let payload: AskPayload = read_stdin_json()?;
            payload.validate()?;
            let request_id = request_id.unwrap_or_else(|| Uuid::now_v7().to_string());
            let terminal = submit_and_wait(request_id, payload, true).await?;
            print_terminal_result(&terminal);
            Ok(exit_code(terminal.status))
        }
        Commands::Wait { request_id } => {
            let terminal = wait_for_existing(request_id, true).await?;
            print_terminal_result(&terminal);
            Ok(exit_code(terminal.status))
        }
        Commands::Status { request_id } => {
            let request = one_shot(ClientMessage::Status {
                version: PROTOCOL_VERSION,
                request_id,
            })
            .await?;
            match request {
                ServerMessage::Status {
                    request: Some(request),
                    ..
                } => print_status(&request),
                ServerMessage::Status { request: None, .. } => bail!("request was not found"),
                ServerMessage::Error { message, .. } => bail!(message),
                _ => bail!("unexpected broker response"),
            }
            Ok(0)
        }
        Commands::Cancel { request_id } => {
            let response = one_shot(ClientMessage::Cancel {
                version: PROTOCOL_VERSION,
                request_id,
            })
            .await?;
            match response {
                ServerMessage::Result {
                    request_id,
                    status,
                    result,
                    ..
                } => {
                    print_terminal_result(&TerminalResult {
                        request_id,
                        status,
                        result,
                    });
                    Ok(exit_code(status))
                }
                ServerMessage::Error { message, .. } => bail!(message),
                _ => bail!("unexpected broker response"),
            }
        }
        Commands::Enable => {
            crate::preferences::set_enabled(true)?;
            println!("AUQ GUI routing is enabled.");
            Ok(0)
        }
        Commands::Disable => {
            crate::preferences::set_enabled(false)?;
            println!("AUQ GUI routing is disabled. Use native agent interaction.");
            Ok(0)
        }
        Commands::Enabled => {
            println!(
                "{}",
                if crate::preferences::is_enabled()? {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            Ok(0)
        }
        Commands::ClaudeHook => run_claude_hook().await,
        Commands::CodexHook { event } => run_codex_hook(event),
    }
}

async fn submit_and_wait(
    request_id: String,
    payload: AskPayload,
    announce: bool,
) -> Result<TerminalResult> {
    wait_loop(
        request_id.clone(),
        ClientMessage::Ask {
            version: PROTOCOL_VERSION,
            request_id,
            payload,
        },
        announce,
    )
    .await
}

async fn wait_for_existing(request_id: String, announce: bool) -> Result<TerminalResult> {
    wait_loop(
        request_id.clone(),
        ClientMessage::Wait {
            version: PROTOCOL_VERSION,
            request_id,
        },
        announce,
    )
    .await
}

async fn wait_loop(
    request_id: String,
    mut first_message: ClientMessage,
    announce: bool,
) -> Result<TerminalResult> {
    let mut announced = false;
    let mut reconnects = 0_u8;
    loop {
        let stream = connect_or_launch().await?;
        let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
        framed.send(encode_frame(&first_message)?).await?;

        while let Some(frame) = framed.next().await {
            let message: ServerMessage = serde_json::from_str(&frame?)?;
            match message {
                ServerMessage::Ack { .. } => {
                    if announce && !announced {
                        eprintln!("AUQ request: {request_id}");
                        announced = true;
                    }
                }
                ServerMessage::Result {
                    request_id,
                    status,
                    result,
                    ..
                } => {
                    return Ok(TerminalResult {
                        request_id,
                        status,
                        result,
                    });
                }
                ServerMessage::HostShutdown { request_id, .. } => {
                    bail!(
                        "AUQ Wizard quit while request {request_id} is pending. Resume with `auq wait {request_id}`."
                    );
                }
                ServerMessage::Error { message, .. } => bail!(message),
                ServerMessage::Status { .. } => bail!("unexpected status response"),
            }
        }

        reconnects += 1;
        if reconnects > 3 {
            bail!("lost connection to AUQ Wizard after three reconnect attempts");
        }
        first_message = ClientMessage::Wait {
            version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
        };
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn one_shot(message: ClientMessage) -> Result<ServerMessage> {
    let stream = connect_or_launch().await?;
    let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
    framed.send(encode_frame(&message)?).await?;
    let response = framed
        .next()
        .await
        .context("AUQ Wizard closed the connection")??;
    serde_json::from_str(&response).context("invalid broker response")
}

async fn connect_or_launch() -> Result<UnixStream> {
    if let Ok(stream) = UnixStream::connect(socket_path()).await {
        return Ok(stream);
    }
    launch_host()?;
    let started = tokio::time::Instant::now();
    while started.elapsed() < HOST_READY_TIMEOUT {
        match UnixStream::connect(socket_path()).await {
            Ok(stream) => return Ok(stream),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    bail!("AUQ Wizard did not become ready within 15 seconds")
}

fn launch_host() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-gj", "-b", BUNDLE_IDENTIFIER])
            .status()
            .context("failed to invoke macOS open")?;
        if !status.success() {
            bail!("macOS could not launch AUQ Wizard");
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        bail!("cold start is currently implemented only on macOS")
    }
}

fn read_stdin_json<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut input = String::new();
    io::stdin()
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_to_string(&mut input)?;
    if input.len() > MAX_FRAME_BYTES {
        bail!("input exceeds 3 MB");
    }
    serde_json::from_str(&input).context("stdin is not valid AUQ JSON")
}

async fn run_claude_hook() -> Result<i32> {
    if !crate::preferences::is_enabled()? {
        return Ok(0);
    }
    let hook_input: Value = read_stdin_json()?;
    let original_input = hook_input
        .get("tool_input")
        .cloned()
        .context("Claude hook input is missing tool_input")?;
    let payload: AskPayload = serde_json::from_value(original_input.clone())?;
    payload.validate()?;
    let question_texts = payload
        .questions
        .iter()
        .map(|question| question.question.clone())
        .collect::<Vec<_>>();
    let request_id = Uuid::now_v7().to_string();
    let terminal = submit_and_wait(request_id, payload, false).await?;

    match terminal.status {
        RequestStatus::Answered => {
            let mut updated = original_input;
            let updated_object = updated
                .as_object_mut()
                .context("Claude tool_input must be an object")?;
            let result = terminal.result.context("answered AUQ result is missing")?;
            updated_object.insert(
                "answers".into(),
                serde_json::to_value(claude_answers(result, &question_texts)?)?,
            );
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "updatedInput": updated
                    }
                }))?
            );
        }
        RequestStatus::Canceled => {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": "The user canceled AUQ."
                    }
                }))?
            );
        }
        RequestStatus::Pending => bail!("broker returned a pending terminal result"),
    }
    Ok(0)
}

fn claude_answers(
    result: AnswerPayload,
    question_texts: &[String],
) -> Result<BTreeMap<String, String>> {
    if let Some(response) = result.response {
        return Ok(question_texts
            .iter()
            .map(|question| (question.clone(), response.clone()))
            .collect());
    }
    let answers = result.answers.context("AUQ result is missing answers")?;
    Ok(answers
        .into_iter()
        .map(|(question, answer)| {
            let answer = match answer {
                AnswerValue::Single(value) => value,
                AnswerValue::Multiple(values) => values.join(", "),
            };
            (question, answer)
        })
        .collect())
}

fn run_codex_hook(event: CodexHookEvent) -> Result<i32> {
    let hook_input: Value = read_stdin_json()?;
    let Some(command) = hook_input
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
    else {
        return Ok(0);
    };
    if parse_canonical_ask(command).is_err() {
        return Ok(0);
    }

    if !crate::preferences::is_enabled()? {
        if matches!(event, CodexHookEvent::PreToolUse) {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": "AUQ GUI routing is disabled. Use native user interaction instead."
                    }
                }))?
            );
        }
        return Ok(0);
    }

    let output = match event {
        CodexHookEvent::PreToolUse => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": { "command": command }
            }
        }),
        CodexHookEvent::PermissionRequest => json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow" }
            }
        }),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(0)
}

fn ensure_auq_enabled() -> Result<()> {
    if !crate::preferences::is_enabled()? {
        bail!(
            "AUQ GUI routing is disabled. Ask through the agent's native interaction instead, or run `auq enable`."
        );
    }
    Ok(())
}

fn parse_canonical_ask(command: &str) -> Result<AskPayload> {
    let lines: Vec<&str> = command.lines().collect();
    if lines.len() < 3 || lines[0] != "auq ask <<'AUQ_JSON'" || lines.last() != Some(&"AUQ_JSON") {
        bail!("not a canonical AUQ command");
    }
    let json = lines[1..lines.len() - 1].join("\n");
    if json.len() > MAX_ASK_PAYLOAD_BYTES {
        bail!("AUQ payload exceeds 1 MB");
    }
    let payload: AskPayload = serde_json::from_str(&json)?;
    payload.validate()?;
    Ok(payload)
}

fn exit_code(status: RequestStatus) -> i32 {
    match status {
        RequestStatus::Answered => 0,
        RequestStatus::Canceled => 2,
        RequestStatus::Pending => 1,
    }
}

fn print_status(request: &StoredRequest) {
    println!("# AUQ Status");
    println!();
    println!("- Request ID: `{}`", request.request_id);
    println!("- Status: `{}`", request.status.as_str());
    println!("- Queue sequence: `{}`", request.sequence);
}

fn print_terminal_result(terminal: &TerminalResult) {
    println!("# AUQ Result");
    println!();
    println!("- Request ID: `{}`", terminal.request_id);
    println!("- Status: `{}`", terminal.status.as_str());
    if let Some(result) = &terminal.result {
        if let Some(response) = &result.response {
            println!();
            println!("## Response");
            println!();
            println!("{response}");
        }
        if let Some(answers) = &result.answers {
            println!();
            println!("## Answers");
            for (question, answer) in answers {
                println!();
                println!("### {question}");
                println!();
                match answer {
                    AnswerValue::Single(value) => println!("{value}"),
                    AnswerValue::Multiple(values) => {
                        for value in values {
                            println!("- {value}");
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical(body: &str) -> String {
        format!("auq ask <<'AUQ_JSON'\n{body}\nAUQ_JSON")
    }

    #[test]
    fn accepts_only_the_canonical_heredoc() {
        let body = r#"{"questions":[{"question":"Choose?","header":"Choice","options":[{"label":"A","description":"First"},{"label":"B","description":"Second"}],"multiSelect":false}]}"#;
        parse_canonical_ask(&canonical(body)).unwrap();
        assert!(parse_canonical_ask(&format!("{} && whoami", canonical(body))).is_err());
        assert!(parse_canonical_ask(&format!("rtk {}", canonical(body))).is_err());
        assert!(parse_canonical_ask(&canonical("$(whoami)")).is_err());
    }

    #[test]
    fn markdown_result_keeps_answer_types() {
        let result = AnswerPayload {
            answers: Some(BTreeMap::from([
                ("One?".into(), AnswerValue::Single("A".into())),
                (
                    "Many?".into(),
                    AnswerValue::Multiple(vec!["A".into(), "B".into()]),
                ),
            ])),
            response: None,
        };
        assert_eq!(result.answers.unwrap().len(), 2);
    }

    #[test]
    fn claude_answers_join_multi_select_values() {
        let result = AnswerPayload {
            answers: Some(BTreeMap::from([
                ("One?".into(), AnswerValue::Single("A".into())),
                (
                    "Many?".into(),
                    AnswerValue::Multiple(vec!["A".into(), "B".into()]),
                ),
            ])),
            response: None,
        };

        assert_eq!(
            claude_answers(result, &[]).unwrap(),
            BTreeMap::from([("Many?".into(), "A, B".into()), ("One?".into(), "A".into()),])
        );
    }

    #[test]
    fn claude_answers_apply_a_free_response_to_each_question() {
        let result = AnswerPayload {
            answers: None,
            response: Some("Use the existing defaults.".into()),
        };

        assert_eq!(
            claude_answers(result, &["One?".into(), "Two?".into()]).unwrap(),
            BTreeMap::from([
                ("One?".into(), "Use the existing defaults.".into()),
                ("Two?".into(), "Use the existing defaults.".into()),
            ])
        );
    }
}
