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
        CliCommand::Chat(args) => {
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
        CliCommand::Research(args) => {
            assert_eq!(args.prompt, Some("Investigate Paxos".into()));
            assert_eq!(args.output.to_str().unwrap(), "out/report.md");
            assert_eq!(args.timeout, 600);
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
        CliCommand::Gem(args) => {
            assert_eq!(args.id, "physiologist");
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
    assert_eq!(stdout.trim(), fixture_md);
}

#[tokio::test]
async fn test_cli_binary_research_file_output() {
    let dossier_md = "# Full Dossier\n\nComprehensive findings.";
    let server = MockCdpServer::start(MockCdpConfig {
        response_markdown: dossier_md.into(),
        stream_poll_count: 1,
        inject_error: false,
        stale_response: None,
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
    // Stdout should ONLY be the output file path!
    assert_eq!(stdout.trim(), output_file.to_str().unwrap());

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
    // stdout MUST be empty on failure
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().is_empty());

    // stderr must contain clean, single line error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("error: connection refused connecting to CDP at http://127.0.0.1:49991")
    );
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
