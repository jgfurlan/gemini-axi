use clap::{CommandFactory, Parser, error::ErrorKind};
use gemini_axi::{
    GeminiDriver,
    cli::{Cli, Command, DisplayArgs},
    error::{AxiError, Result},
    inspect, integration, output,
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
use tracing_subscriber::EnvFilter;

fn main() {
    let argv: Vec<_> = std::env::args().collect();
    if argv.len() == 2 && matches!(argv[1].as_str(), "-v" | "-V" | "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            if e.kind() == ErrorKind::DisplayVersion {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            let help = e.kind() == ErrorKind::DisplayHelp;
            let mut command = Cli::command();
            command.build();
            if let Some(sub) = argv
                .iter()
                .find(|s| matches!(s.as_str(), "chat" | "research" | "gem"))
                && let Some(found) = command.find_subcommand_mut(sub)
            {
                command = found.clone();
            }
            let mut data = help_output(command);
            if !help {
                let message = e
                    .to_string()
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("Invalid usage")
                    .trim_start_matches("error: ")
                    .to_string();
                data.as_object_mut()
                    .unwrap()
                    .insert("error".into(), json!({"code":"usage", "message":message}));
            }
            output::print(data);
            std::process::exit(if help { 0 } else { 2 });
        }
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(if cli.verbose {
            "gemini_axi=debug"
        } else {
            "gemini_axi=warn"
        }))
        .with_writer(std::io::stderr)
        .init();
    let endpoint = cli.cdp.clone();
    let prompt_hint = match &cli.command {
        Some(Command::Gem(_)) => "gemini-axi gem --id <id> \"<prompt>\"",
        Some(Command::Research(_)) => "gemini-axi research \"<topic>\" -o <report.md>",
        _ => "gemini-axi chat \"<prompt>\"",
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            output::print(
                json!({"error":{"code":"runtime_unavailable","message":"Cannot initialize execution runtime"},"help":["gemini-axi"]}),
            );
            std::process::exit(1);
        }
    };
    if let Err(err) = runtime.block_on(run(cli)) {
        let (code, message, help, exit) = match &err {
            AxiError::Configuration(s) => ("configuration", s.clone(), "gemini-axi --setup-hooks <claude|codex|opencode>", 1),
            AxiError::NotSignedIn => ("not_signed_in", "Sign into Gemini in agents Chrome before continuing".into(), "gemini-axi", 1),
            AxiError::Usage(s) => ("usage", s.clone(), "gemini-axi --help", 2),
            AxiError::EmptyPrompt => ("usage", "A nonempty prompt or prompt file is required".into(), prompt_hint, 2),
            AxiError::FileRead(path, _) => ("file_read", format!("Cannot read {}", path.display()), "gemini-axi chat --prompt-file <readable-file>", 1),
            AxiError::FileWrite(path, _) => ("file_write", format!("Cannot write {}", path.display()), "gemini-axi chat \"<prompt>\" -o <writable-file>", 1),
            AxiError::ConnectionRefused(_) | AxiError::Http(_) | AxiError::WebSocket(_) => ("browser_unavailable", "Cannot reach agents Chrome; check its remote debugging endpoint and access permission".into(), "gemini-axi", 1),
            AxiError::Timeout(_, _) | AxiError::GenerationDidNotStart(_) => ("timeout", "Gemini did not complete in time; check the conversation in Chrome before retrying".into(), "gemini-axi", 1),
            AxiError::ResearchPlanNotCompleted => ("research_incomplete", "Research produced a plan without a completed report".into(), "gemini-axi research --conversation <url> --full", 1),
            _ => ("operation_failed", "Gemini operation could not complete; check login and app readiness".into(), "gemini-axi", 1),
        };
        tracing::debug!(error = %err, "Operation failed");
        let help = help.replacen("gemini-axi", &command_prefix(&endpoint), 1);
        output::print(json!({"error":{"code":code,"message":message},"help":[help]}));
        std::process::exit(exit);
    }
}

fn help_output(mut command: clap::Command) -> Value {
    let name = command.get_name().to_string();
    let usage = command
        .render_usage()
        .to_string()
        .trim_start_matches("Usage: ")
        .to_string();
    let usage = if usage.starts_with("gemini-axi") {
        usage
    } else {
        format!("gemini-axi {usage}")
    };
    let mut flags = Vec::new();
    let mut arguments = Vec::new();
    for arg in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
        let mut names = Vec::new();
        if let Some(short) = arg.get_short() {
            names.push(format!("-{short}"));
        }
        if let Some(long) = arg.get_long() {
            names.push(format!("--{long}"));
        }
        let value = arg
            .get_value_names()
            .and_then(|v| v.first())
            .map(ToString::to_string)
            .unwrap_or_else(|| arg.get_id().to_string());
        let mut text = if names.is_empty() {
            value.clone()
        } else {
            names.join("/")
        };
        if !arg.is_positional() && arg.get_action().takes_values() {
            text.push_str(&format!(" <{value}>"));
        }
        if let Some(help) = arg.get_help() {
            text.push_str(&format!(": {help}"));
        }
        let defaults = arg
            .get_default_values()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(",");
        if !defaults.is_empty() {
            text.push_str(&format!(" [default: {defaults}]"));
        }
        if arg.is_positional() {
            arguments.push(text);
        } else {
            flags.push(text);
        }
    }
    let examples = command
        .get_after_help()
        .map(ToString::to_string)
        .unwrap_or_default()
        .lines()
        .skip(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut data = json!({"usage":usage,"description":command.get_about().map(ToString::to_string),"flags":flags,"examples":examples});
    if let Some(requirement) = match name.as_str() {
        "chat" => Some("PROMPT or --prompt-file is required"),
        "research" => Some("PROMPT, --prompt-file, or --conversation is required"),
        "gem" => Some("Omit --id to list; --id requires PROMPT or --input"),
        _ => None,
    } {
        data["required"] = json!(requirement);
    }
    if !arguments.is_empty() {
        data["arguments"] = json!(arguments);
    }
    if name == "gemini-axi" {
        data["commands"] = json!(["chat", "research", "gem"]);
    }
    data
}

fn validate_prompt(prompt: &str) -> Result<()> {
    if prompt.trim().is_empty() {
        return Err(AxiError::EmptyPrompt);
    }
    Ok(())
}
fn validate_fields(display: &DisplayArgs, list: bool) -> Result<()> {
    let allowed: &[&str] = if list {
        &["id", "name", "url"]
    } else {
        &[
            "response",
            "chars",
            "model",
            "conversation",
            "output",
            "truncated",
        ]
    };
    for field in &display.fields {
        if !allowed.contains(&field.as_str()) {
            return Err(AxiError::Usage(format!(
                "Unknown field {field}; valid fields: {}",
                allowed.join(",")
            )));
        }
    }
    Ok(())
}
fn project(value: &mut Value, display: &DisplayArgs) {
    if !display.fields.is_empty() {
        value
            .as_object_mut()
            .unwrap()
            .retain(|k, _| display.fields.contains(k));
    }
}
async fn run(cli: Cli) -> Result<()> {
    if cli.command.is_some()
        && (cli.setup_hooks.is_some()
            || cli.write_skill.is_some()
            || cli.check_skill.is_some()
            || cli.session_end)
    {
        return Err(AxiError::Usage(
            "Integration flags cannot be combined with a subcommand".into(),
        ));
    }
    if let Some(path) = &cli.write_skill {
        write_output_file(path, &integration::skill())?;
        output::print(
            json!({"skill": path, "state":"generated", "help":["gemini-axi --setup-hooks <claude|codex|opencode>"]}),
        );
        return Ok(());
    }
    if let Some(path) = &cli.check_skill {
        let content = fs::read_to_string(path).map_err(|e| AxiError::FileRead(path.clone(), e))?;
        if content != integration::skill() {
            return Err(AxiError::Usage(
                "Skill is stale; regenerate with gemini-axi --write-skill <file>".into(),
            ));
        }
        output::print(json!({"skill":path,"state":"current"}));
        return Ok(());
    }
    if cli.session_end {
        return integration::capture();
    }
    if let Some(app) = &cli.setup_hooks {
        let changed = integration::setup(app, &cli.cdp)?;
        output::print(
            json!({"integration":app,"state":if changed {"installed_or_repaired"} else {"already_configured"},"scope":"project", "help":["gemini-axi"]}),
        );
        return Ok(());
    }
    let Some(command) = cli.command.as_ref() else {
        return home(&cli.cdp).await;
    };
    let (prompt, display, model, path, timeout, preferred, gem) = match command {
        Command::Chat(args) => {
            let prompt = args.resolve_prompt()?;
            validate_prompt(&prompt)?;
            validate_fields(&args.display, false)?;
            (
                prompt,
                &args.display,
                args.model.as_deref(),
                args.output.as_deref(),
                args.timeout,
                None,
                None,
            )
        }
        Command::Research(args) => {
            let prompt = args.resolve_prompt()?;
            if args.conversation.is_none() {
                validate_prompt(&prompt)?;
            }
            if let Some(url) = &args.conversation {
                let parsed = url::Url::parse(url).map_err(|_| {
                    AxiError::Usage("--conversation requires a Gemini conversation URL".into())
                })?;
                if parsed.scheme() != "https"
                    || parsed.host_str() != Some("gemini.google.com")
                    || !parsed.path().starts_with("/app/")
                {
                    return Err(AxiError::Usage(
                        "--conversation requires https://gemini.google.com/app/<id>".into(),
                    ));
                }
            }
            validate_fields(&args.display, false)?;
            (
                prompt,
                &args.display,
                None,
                args.output.as_deref(),
                args.timeout,
                args.conversation.as_deref(),
                None,
            )
        }
        Command::Gem(args) => {
            if args.id.is_none() {
                if args.prompt.is_some()
                    || args.input.is_some()
                    || args.output.is_some()
                    || args.display.full
                {
                    return Err(AxiError::Usage(
                        "--id is required to send a Gem prompt".into(),
                    ));
                }
                validate_fields(&args.display, true)?;
                let state = inspect::inspect(&cli.cdp, true).await?;
                if state.login != "signed_in" {
                    return Err(AxiError::NotSignedIn);
                }
                let rows: Vec<_> = state
                    .gems
                    .iter()
                    .map(|g| {
                        let mut row = json!({"id":g.id,"name":g.name,"url":g.url});
                        if args.display.fields.is_empty() {
                            row.as_object_mut().unwrap().remove("url");
                        } else {
                            project(&mut row, &args.display);
                        }
                        row
                    })
                    .collect();
                output::print(
                    json!({"count":rows.len(), "gems": rows, "state":if rows.is_empty() {"No Gems found in this Gemini account"} else {"Gems available"}, "help":[format!("{} gem --id <id> \"<prompt>\"", command_prefix(&cli.cdp))]}),
                );
                return Ok(());
            }
            let id = args.id.as_deref().unwrap();
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(AxiError::Usage(
                    "Gem ID must contain only letters, digits, - or _".into(),
                ));
            }
            let prompt = args.resolve_prompt()?;
            validate_prompt(&prompt)?;
            validate_fields(&args.display, false)?;
            (
                prompt,
                &args.display,
                None,
                args.output.as_deref(),
                args.timeout,
                None,
                Some(id),
            )
        }
    };
    if timeout == 0 {
        return Err(AxiError::Usage(
            "--timeout must be greater than zero".into(),
        ));
    }
    if !cli.cdp.starts_with("ws://") && !cli.cdp.starts_with("wss://") {
        gemini_axi::cdp::ensure_cdp_reachable(&cli.cdp, cli.is_explicit_cdp()).await?;
    }
    let driver =
        GeminiDriver::connect_and_prepare(&cli.cdp, preferred, Duration::from_secs(30)).await?;
    eprintln!("progress: waiting for Gemini response");
    let response = match command {
        Command::Chat(_) => {
            driver
                .run_chat(&prompt, model, Duration::from_secs(timeout))
                .await
        }
        Command::Research(_) => {
            driver
                .run_research(&prompt, Duration::from_secs(timeout))
                .await
        }
        Command::Gem(_) => {
            driver
                .run_gem(gem.unwrap(), &prompt, Duration::from_secs(timeout))
                .await
        }
    };
    let conversation = driver
        .client()
        .evaluate_string("window.location.href")
        .await
        .ok()
        .flatten();
    let observed = inspect::state(driver.client()).await.ok();
    let _ = driver.close().await;
    let response = response?;
    if let Some(path) = path {
        write_output_file(path, &response)?;
    }
    let chars = response.chars().count();
    let truncated = !display.full && chars > 1000;
    let preview = if truncated {
        response.chars().take(1000).collect::<String>()
    } else {
        response
    };
    let mut data = json!({"response":preview,"chars":chars,"truncated":truncated,"model":observed.and_then(|s| s.model),"conversation":conversation});
    if let Some(path) = path {
        data["output"] = json!(path);
    }
    project(&mut data, display);
    let mut help = Vec::new();
    // Never suggest repeating a paid mutation merely to display more text.
    if truncated {
        help.push(if let Some(path) = path {
            format!("cat -- {}", shell_quote(&path.to_string_lossy()))
        } else {
            "gemini-axi chat --help (use --full or -o <file> on the next request)".into()
        });
    }
    help.push(format!("{} gem", command_prefix(&cli.cdp)));
    output::print(json!({"result":data,"help":help}));
    Ok(())
}
fn command_prefix(endpoint: &str) -> String {
    if endpoint.trim_end_matches('/') == "http://127.0.0.1:9333" {
        "gemini-axi".into()
    } else {
        format!("gemini-axi --cdp {}", shell_quote(endpoint))
    }
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
async fn home(endpoint: &str) -> Result<()> {
    let exe = std::env::current_exe()
        .map_err(|e| AxiError::Other(e.to_string()))?
        .to_string_lossy()
        .into_owned();
    let bin = if let Ok(home) = std::env::var("HOME") {
        exe.strip_prefix(&(home + "/"))
            .map(|s| format!("~/{s}"))
            .unwrap_or(exe)
    } else {
        exe
    };
    let mut data = json!({"bin":bin,"description":integration::DESCRIPTION});
    match inspect::inspect(endpoint, false).await {
        Ok(state) => {
            data["browser"] = json!("reachable");
            data["login"] = json!(state.login);
            data["models"] = json!(state.models);
        }
        Err(err) => {
            data["browser"] = json!(if matches!(
                err,
                AxiError::ConnectionRefused(_) | AxiError::Http(_) | AxiError::WebSocket(_)
            ) {
                "unreachable"
            } else {
                "reachable"
            });
            data["login"] = json!("unknown");
            data["models"] = json!([]);
        }
    }
    if let Ok(text) = fs::read_to_string(".gemini-axi/last-session.json")
        && let Ok(last) = serde_json::from_str::<Value>(&text)
    {
        data["last_session"] = json!({"session_id":last["session_id"],"files_touched":last["files_touched"].as_array().map(Vec::len).unwrap_or(0)});
    }
    data["help"] = json!(
        integration::GUIDANCE
            .iter()
            .map(|s| s.replacen("gemini-axi", &command_prefix(endpoint), 1))
            .collect::<Vec<_>>()
    );
    output::print(data);
    Ok(())
}
fn write_output_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| AxiError::FileWrite(path.into(), e))?;
    }
    fs::write(path, content).map_err(|e| AxiError::FileWrite(path.into(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_omit_only_the_default_endpoint() {
        assert_eq!(command_prefix("http://127.0.0.1:9333"), "gemini-axi");
        assert_eq!(command_prefix("http://127.0.0.1:9333/"), "gemini-axi");
        assert_eq!(
            command_prefix("http://127.0.0.1:9444"),
            "gemini-axi --cdp 'http://127.0.0.1:9444'"
        );
        assert_eq!(
            command_prefix("https://example.com/a'b"),
            "gemini-axi --cdp 'https://example.com/a'\\''b'"
        );
    }
}
