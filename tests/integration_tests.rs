use clap::Parser;
use std::fs;
use std::time::Duration;
use tokio::process::Command;

mod common;
use common::mock_cdp::{MockCdpConfig, MockCdpServer};
use gemini_axi::GeminiDriver;
use gemini_axi::cli::{Cli, Command as CliCommand};
use gemini_axi::error::AxiError;
use gemini_axi::gemini::selectors::*;

#[tokio::test]
async fn test_cdp_discovery_and_connect() {
    let server = MockCdpServer::start(MockCdpConfig::default()).await;

    let target_ws = gemini_axi::cdp::discover_target(&server.base_url, None)
        .await
        .expect("target discovery should succeed");
    assert!(target_ws.starts_with("ws://127.0.0.1:"));

    let client = gemini_axi::CdpClient::connect(&target_ws)
        .await
        .expect("WebSocket connection should succeed");

    client
        .enable_standard_domains()
        .await
        .expect("standard domains should enable");

    let location = client
        .evaluate_string("window.location.href")
        .await
        .expect("evaluation should succeed")
        .expect("string result expected");

    assert_eq!(location, "https://gemini.google.com/app");
}

#[tokio::test]
async fn test_gemini_driver_chat_flow() {
    let fixture_md = "# Architecture\n\n- Invariant-driven\n- Deterministic";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: fixture_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(
        &server.base_url,
        Some(BASE_APP_URL),
        Duration::from_secs(5),
    )
    .await
    .expect("driver should connect and initialize");

    let response = driver
        .run_chat("Explain invariants", Some("argon"), Duration::from_secs(5))
        .await
        .expect("chat should complete");

    assert_eq!(response, fixture_md);
}

#[tokio::test]
async fn test_gemini_driver_research_flow() {
    let dossier_md = "# Deep Research Dossier: Cryptographic Proofs\n\nComparative analysis of SNARKs vs STARKs.";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: dossier_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(
        &server.base_url,
        Some(BASE_APP_URL),
        Duration::from_secs(5),
    )
    .await
    .expect("driver should connect and initialize");

    let dossier = driver
        .run_research("Research ZK proofs", Duration::from_secs(10))
        .await
        .expect("research should complete");

    assert_eq!(dossier, dossier_md);
}

#[tokio::test]
async fn test_gemini_driver_gem_flow() {
    let gem_md = "# Hypertrophy Analysis\n\nOptimal training volume confirmed.";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: gem_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(
        &server.base_url,
        Some(BASE_APP_URL),
        Duration::from_secs(5),
    )
    .await
    .expect("driver should connect and initialize");

    let response = driver
        .run_gem(
            "hypertrophy-physiologist",
            "athlete-data",
            Duration::from_secs(5),
        )
        .await
        .expect("gem interaction should complete");

    assert_eq!(response, gem_md);
}

#[tokio::test]
async fn test_connection_refused_error() {
    // Unused port on localhost
    let bad_url = "http://127.0.0.1:49151";
    let res = gemini_axi::cdp::discover_target(bad_url, None).await;

    assert!(res.is_err());
    let err = res.unwrap_err();
    match err {
        AxiError::ConnectionRefused(url) => {
            assert!(url.contains("49151"));
        }
        _ => panic!("Expected ConnectionRefused error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_cdp_injected_error() {
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: "error".into(),
        stream_poll_count: 0,
        inject_error: true,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let client = gemini_axi::CdpClient::connect(&server.ws_url)
        .await
        .expect("connect succeeds");

    let res = client.evaluate("2 + 2").await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    match err {
        AxiError::CdpProtocol { code, message } => {
            assert_eq!(code, -32000);
            assert!(message.contains("synthetic evaluation failure"));
        }
        _ => panic!("Expected CdpProtocol error, got: {err:?}"),
    }
}

#[test]
fn test_html_fixtures_match_selectors() {
    let chat_html = fs::read_to_string("tests/fixtures/gemini_chat.html")
        .expect("chat fixture must be readable");
    let research_html = fs::read_to_string("tests/fixtures/gemini_research.html")
        .expect("research fixture must be readable");
    let gem_html =
        fs::read_to_string("tests/fixtures/gemini_gem.html").expect("gem fixture must be readable");

    // Chat fixture checks
    assert!(chat_html.contains("rich-textarea"));
    assert!(chat_html.contains("ql-editor"));
    assert!(chat_html.contains("send-button"));
    assert!(chat_html.contains("model-picker-button"));
    assert!(chat_html.contains("model-response-text"));

    // Research fixture checks
    assert!(research_html.contains("research-plan"));
    assert!(
        research_html.contains("start-research-button") || research_html.contains("Start research")
    );
    assert!(research_html.contains("deep-research-toggle"));

    // Gem fixture checks
    assert!(gem_html.contains("gem-title"));
    assert!(gem_html.contains("model-response-text"));
}

#[test]
fn test_cli_parsing_and_defaults() {
    // 1. Chat command
    let cli = Cli::parse_from(["gemini-axi", "chat", "Draft invariant spec", "-m", "argon"]);
    assert_eq!(cli.cdp, "http://127.0.0.1:9333");
    match cli.command {
        Some(CliCommand::Chat(args)) => {
            assert_eq!(args.prompt, Some("Draft invariant spec".into()));
            assert_eq!(args.model, Some("argon".into()));
            assert_eq!(args.timeout, 120);
            assert_eq!(args.resolve_prompt().unwrap(), "Draft invariant spec");
        }
        _ => panic!("Expected Chat command"),
    }

    // 2. Research command
    let cli = Cli::parse_from([
        "gemini-axi",
        "--cdp",
        "http://127.0.0.1:9333",
        "research",
        "Investigate Paxos",
        "-o",
        "out/report.md",
    ]);
    assert_eq!(cli.cdp, "http://127.0.0.1:9333");
    match cli.command {
        Some(CliCommand::Research(args)) => {
            assert_eq!(args.prompt, Some("Investigate Paxos".into()));
            assert_eq!(
                args.output.as_ref().unwrap().to_str().unwrap(),
                "out/report.md"
            );
            assert_eq!(args.timeout, 1800);
        }
        _ => panic!("Expected Research command"),
    }

    // 3. Gem command
    let cli = Cli::parse_from([
        "gemini-axi",
        "gem",
        "--id",
        "physiologist",
        "analyze heart rate",
    ]);
    match cli.command {
        Some(CliCommand::Gem(args)) => {
            assert_eq!(args.id.as_deref(), Some("physiologist"));
            assert_eq!(args.prompt, Some("analyze heart rate".into()));
            assert_eq!(args.timeout, 180);
        }
        _ => panic!("Expected Gem command"),
    }
}

#[tokio::test]
async fn test_cli_binary_chat_execution() {
    let fixture_md = "# Architecture Review\n\nArgon response completed.";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: fixture_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let output = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .arg("--cdp")
        .arg(&server.base_url)
        .arg("chat")
        .arg("Hello Argon")
        .arg("-m")
        .arg("argon")
        .output()
        .await
        .expect("binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("response: \"# Architecture Review\\n\\nArgon response completed.\""));
}

#[tokio::test]
async fn test_cli_binary_research_file_output() {
    let dossier_md = "# Full Dossier\n\nComprehensive findings.";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: dossier_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
        plan_only: false,
    })
    .await;

    let temp_dir = tempfile::tempdir().unwrap();
    let prompt_file = temp_dir.path().join("request.md");
    let output_file = temp_dir.path().join("dossier.md");

    fs::write(&prompt_file, "Deep research on Rust async runtimes").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .arg("--cdp")
        .arg(&server.base_url)
        .arg("research")
        .arg("--prompt-file")
        .arg(&prompt_file)
        .arg("--output")
        .arg(&output_file)
        .output()
        .await
        .expect("binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Stdout includes the saved path and structured dossier preview.
    assert!(stdout.contains(output_file.to_str().unwrap()));
    assert!(stdout.contains("Full Dossier"));

    // File on disk must contain the dossier markdown
    let saved_content = fs::read_to_string(&output_file).unwrap();
    assert_eq!(saved_content, dossier_md);
}

#[tokio::test]
async fn test_cli_binary_failure_output_cleanliness() {
    // Port where nothing is listening
    let bad_cdp = "http://127.0.0.1:49991";

    let output = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .arg("--cdp")
        .arg(bad_cdp)
        .arg("chat")
        .arg("Test failure")
        .output()
        .await
        .expect("binary should execute");

    assert!(!output.status.success());
    // Errors use the same structured stdout channel as successful data.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("browser_unavailable"));

    // No raw dependency diagnostics are exposed at the default log level.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty());
}

#[tokio::test]
async fn test_mock_cdp_rejects_stale_response_and_returns_fresh() {
    let stale_text = "# Stale Response\n\nSou um modelo de linguagem.";
    let fresh_text = "OK 42";

    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: fresh_text.into(),
        stream_poll_count: 2,
        inject_error: false,
        stale_response: Some(stale_text.into()),
        plan_only: false,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(&server.base_url, None, Duration::from_secs(5))
        .await
        .unwrap();

    let result = driver
        .run_chat(
            "teste gemini-axi: responda apenas com a palavra OK e a soma de 17 + 25",
            None,
            Duration::from_secs(10),
        )
        .await
        .unwrap();

    assert_eq!(result, fresh_text);
    assert!(!result.contains("Sou um modelo"));
}

#[tokio::test]
async fn test_mock_cdp_fails_when_generation_does_not_start() {
    let stale_text = "Sou um modelo de linguagem.";

    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: "never reached".into(),
        stream_poll_count: 0, // generation never starts
        inject_error: false,
        stale_response: Some(stale_text.into()),
        plan_only: false,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(&server.base_url, None, Duration::from_secs(5))
        .await
        .unwrap();

    let err = driver
        .run_chat(
            "teste gemini-axi: falha proposital",
            None,
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();

    match err {
        gemini_axi::error::AxiError::GenerationDidNotStart(_) => {}
        other => panic!("Expected GenerationDidNotStart error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_mock_cdp_fails_when_research_only_produces_plan() {
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: "Aqui está meu plano para abordar esse tema. Iniciar pesquisa".into(),
        stream_poll_count: 0,
        inject_error: false,
        stale_response: None,
        plan_only: true,
    })
    .await;

    let driver = GeminiDriver::connect_and_prepare(&server.base_url, None, Duration::from_secs(5))
        .await
        .unwrap();

    let err = driver
        .run_research(
            "teste gemini-axi: breve resumo historico sobre a invencao do transistor",
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();

    match err {
        gemini_axi::error::AxiError::ResearchPlanNotCompleted => {}
        other => panic!("Expected ResearchPlanNotCompleted error, got: {other:?}"),
    }
}

async fn invoke(server: &MockCdpServer, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .args(["--cdp", &server.base_url])
        .args(args)
        .output()
        .await
        .unwrap()
}

#[tokio::test]
async fn dashboard_and_empty_gems_are_definitive_and_do_not_submit() {
    let server = MockCdpServer::start(MockCdpConfig::default()).await;
    let home = invoke(&server, &[]).await;
    assert!(home.status.success());
    let text = String::from_utf8(home.stdout).unwrap();
    assert!(text.contains("browser: reachable"));
    assert!(text.contains("login: signed_in"));
    assert!(text.contains("models[3]: Fast,Thinking,Pro"));
    assert!(text.contains("bin:"));
    let gems = invoke(&server, &["gem"]).await;
    assert!(gems.status.success());
    let text = String::from_utf8(gems.stdout).unwrap();
    assert!(text.contains("count: 0"));
    assert!(text.contains("gems: []"));
    assert!(text.contains("No Gems found in this Gemini account"));
    let calls = server.calls.lock().unwrap();
    assert!(!calls.iter().any(|c| c == "Input.insertText"));
    assert_eq!(
        calls.iter().filter(|c| *c == "Target.closeTarget").count(),
        2
    );
}

#[tokio::test]
async fn unavailable_and_signed_out_dashboards() {
    let offline = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .args(["--cdp", "http://127.0.0.1:49991"])
        .output()
        .await
        .unwrap();
    assert!(offline.status.success());
    assert!(String::from_utf8_lossy(&offline.stdout).contains("browser: unreachable"));
    let server = MockCdpServer::start_with_state(
        MockCdpConfig::default(),
        serde_json::json!({"login":"signed_out","model":null,"models":[],"gems":[]}),
    )
    .await;
    let home = invoke(&server, &[]).await;
    assert!(String::from_utf8_lossy(&home.stdout).contains("signed_out"));
    let list = invoke(&server, &["gem"]).await;
    assert!(!list.status.success());
    assert!(!String::from_utf8_lossy(&list.stdout).contains("No Gems found"));
}

#[tokio::test]
async fn gem_list_projects_fields_and_carries_endpoint() {
    let server = MockCdpServer::start_with_state(MockCdpConfig::default(), serde_json::json!({"login":"signed_in","model":null,"models":[],"gems":[{"id":"a", "name":"A, Gem", "url":"https://gemini.google.com/gem/a"}]})).await;
    let list = invoke(&server, &["gem", "--fields", "id,url"]).await;
    assert!(list.status.success());
    let text = String::from_utf8(list.stdout).unwrap();
    assert!(text.contains("gems[1]{id,url}:"));
    assert!(!text.contains("A, Gem"));
    assert!(text.contains(&server.base_url));
}

#[tokio::test]
async fn long_content_is_previewed_and_file_is_complete_for_all_commands() {
    let response = format!("# Report\n{}", "ç🦀,".repeat(600));
    for command in ["chat", "research", "gem"] {
        let server = MockCdpServer::start(MockCdpConfig {
            response_markdown: response.clone(),
            ..Default::default()
        })
        .await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("full.md");
        let mut args = vec![command, "<prompt>", "-o", path.to_str().unwrap()];
        if command == "gem" {
            args.extend(["--id", "custom"]);
        }
        let result = invoke(&server, &args).await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains("truncated: true"));
        assert!(text.contains(&format!("chars: {}", response.chars().count())));
        assert!(text.contains("--full") || text.contains("cat --"));
        assert!(text.len() < response.len() + 1000);
        assert_eq!(fs::read_to_string(path).unwrap(), response);
    }
}

#[tokio::test]
async fn full_and_projection_work_for_chat_and_gem() {
    for command in ["chat", "gem", "research"] {
        let response = format!("{} END", "ç".repeat(1200));
        let server = MockCdpServer::start(MockCdpConfig {
            response_markdown: response.clone(),
            ..Default::default()
        })
        .await;
        let mut args = vec![command, "<prompt>", "--full", "--fields", "response,chars"];
        if command == "gem" {
            args.extend(["--id", "custom"]);
        }
        let result = invoke(&server, &args).await;
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains(&response));
        assert!(!text.contains("model:"));
        assert!(!text.contains("use --full"));
    }
}

#[tokio::test]
async fn invalid_input_fails_before_browser_calls_and_help_is_successful() {
    let server = MockCdpServer::start(MockCdpConfig::default()).await;
    for args in [
        vec!["chat"],
        vec!["chat", " "],
        vec!["research"],
        vec!["gem", "--id", "x"],
        vec!["chat", "x", "--unknown"],
        vec!["chat", "x", "--fields", "typo"],
        vec!["chat", "x", "--timeout", "0"],
        vec!["research", "--conversation", "https://example.com"],
        vec!["gem", "prompt"],
        vec!["--setup-hooks", "claude", "chat", "x"],
    ] {
        let result = invoke(&server, &args).await;
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&result.stdout).contains("usage"));
        assert!(result.stderr.is_empty());
    }
    for command in ["chat", "research", "gem"] {
        let result = invoke(&server, &[command, "--help"]).await;
        assert!(result.status.success());
        let text = String::from_utf8_lossy(&result.stdout);
        assert!(text.contains("--full"));
        assert!(text.contains("examples[2]"));
        assert!(text.contains("--cdp"));
        assert!(!text.contains("gemini-axi gemini-axi"));
        assert!(text.contains("required:"));
    }
    assert!(server.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn protocol_errors_are_translated_and_file_errors_are_structured() {
    let server = MockCdpServer::start(MockCdpConfig {
        inject_error: true,
        ..Default::default()
    })
    .await;
    let result = invoke(&server, &["chat", "x"]).await;
    assert_eq!(result.status.code(), Some(1));
    let text = String::from_utf8_lossy(&result.stdout);
    assert!(text.contains("operation_failed"));
    assert!(!text.contains("synthetic"));
    let result = invoke(
        &server,
        &["chat", "--prompt-file", "/no/such/gemini-axi-file"],
    )
    .await;
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stdout).contains("file_read"));
}

#[tokio::test]
async fn hooks_are_project_scoped_idempotent_and_preserve_unrelated_entries() {
    for app in ["claude", "codex", "opencode"] {
        let temp = tempfile::tempdir().unwrap();
        if app != "opencode" {
            let dir = temp.path().join(format!(".{app}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(if app == "claude" {"settings.json"} else {"hooks.json"}), r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo unrelated"}]}]}}"#).unwrap();
        }
        let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
            .current_dir(temp.path())
            .args(["--setup-hooks", app])
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let path = temp.path().join(match app {
            "claude" => ".claude/settings.json",
            "codex" => ".codex/hooks.json",
            _ => ".opencode/plugins/gemini-axi.js",
        });
        let before = fs::read_to_string(&path).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
            .current_dir(temp.path())
            .args(["--setup-hooks", app])
            .output()
            .await
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("already_configured"));
        assert_eq!(before, fs::read_to_string(path).unwrap());
        if app != "opencode" {
            assert!(before.contains("echo unrelated"));
            assert!(before.contains("SessionEnd"));
        }
        if app == "codex" {
            assert!(
                fs::read_to_string(temp.path().join(".codex/config.toml"))
                    .unwrap()
                    .contains("hooks = true")
            );
        }
    }
}

#[test]
fn committed_skill_matches_home_guidance() {
    assert_eq!(
        fs::read_to_string(".agents/skills/gemini-axi/SKILL.md").unwrap(),
        gemini_axi::integration::skill()
    );
}

#[tokio::test]
async fn versions_are_bare_and_missing_files_do_not_connect() {
    let server = MockCdpServer::start(MockCdpConfig::default()).await;
    for flag in ["-v", "-V", "--version"] {
        let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
            .arg(flag)
            .output()
            .await
            .unwrap();
        assert!(result.status.success());
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            env!("CARGO_PKG_VERSION")
        );
        assert!(result.stderr.is_empty());
    }
    let result = invoke(&server, &["gem", "--id", "x", "--input", "/no/such/file"]).await;
    assert_eq!(result.status.code(), Some(1));
    assert!(server.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn hook_repairs_paths_and_rejects_invalid_config_without_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join(".codex");
    fs::create_dir_all(&dir).unwrap();
    let hooks = dir.join("hooks.json");
    let config = dir.join("config.toml");
    fs::write(&hooks, r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"/old/gemini-axi # gemini-axi-managed"}]}]}}"#).unwrap();
    fs::write(
        &config,
        "model = 'example'\n[features]\nhooks = false\nother = true\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .current_dir(temp.path())
        .args(["--setup-hooks", "codex"])
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    assert!(
        !fs::read_to_string(&hooks)
            .unwrap()
            .contains("/old/gemini-axi")
    );
    let text = fs::read_to_string(&config).unwrap();
    assert!(text.contains("hooks = true"));
    assert!(text.contains("other = true"));
    assert!(text.contains("model = 'example'"));
    fs::write(&config, "features = false\n").unwrap();
    let previous = fs::read_to_string(&hooks).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .current_dir(temp.path())
        .args(["--setup-hooks", "codex"])
        .output()
        .await
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stdout).contains("configuration"));
    assert_eq!(fs::read_to_string(hooks).unwrap(), previous);
}

#[cfg(unix)]
#[tokio::test]
async fn setup_refuses_symlinks_outside_project() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".claude")).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .current_dir(project.path())
        .args(["--setup-hooks", "claude"])
        .output()
        .await
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stdout).contains("outside this project"));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn session_end_captures_metadata_and_next_dashboard_uses_compact_summary() {
    use tokio::io::AsyncWriteExt;
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .current_dir(temp.path())
        .arg("--session-end")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"session_id":"s1","transcript_path":"/tmp/transcript.jsonl","secret":"never saved"}"#).await.unwrap();
    let result = child.wait_with_output().await.unwrap();
    assert!(result.status.success());
    let record = fs::read_to_string(temp.path().join(".gemini-axi/last-session.json")).unwrap();
    assert!(record.contains("s1"));
    assert!(record.contains("/tmp/transcript.jsonl"));
    assert!(!record.contains("never saved"));
    let result = Command::new(env!("CARGO_BIN_EXE_gemini-axi"))
        .current_dir(temp.path())
        .args(["--cdp", "http://127.0.0.1:49991"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&result.stdout);
    assert!(text.contains("last_session"));
    assert!(text.contains("s1"));
    assert!(!text.contains("/tmp/transcript.jsonl"));
}
