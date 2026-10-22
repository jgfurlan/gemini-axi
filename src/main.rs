use clap::Parser;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

use gemini_axi::GeminiDriver;
use gemini_axi::cli::{ChatArgs, Cli, Command, GemArgs, ResearchArgs};
use gemini_axi::error::{AxiError, Result};
use gemini_axi::gemini::selectors::BASE_APP_URL;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Direct all internal logging strictly to stderr to keep stdout 100% clean for agents
    let filter = if cli.verbose {
        EnvFilter::new("gemini_axi=debug")
    } else {
        EnvFilter::new("gemini_axi=warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    if let Err(err) = run(cli).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    gemini_axi::cdp::ensure_cdp_reachable(&cli.cdp, cli.is_explicit_cdp()).await?;

    match cli.command {
        Command::Chat(args) => handle_chat(&cli.cdp, args).await,
        Command::Research(args) => handle_research(&cli.cdp, args).await,
        Command::Gem(args) => handle_gem(&cli.cdp, args).await,
    }
}

async fn handle_chat(cdp: &str, args: ChatArgs) -> Result<()> {
    let prompt = args.resolve_prompt()?;
    let timeout = Duration::from_secs(args.timeout);

    let driver = GeminiDriver::connect_and_prepare(cdp, None, Duration::from_secs(30)).await?;

    let res = driver
        .run_chat(&prompt, args.model.as_deref(), timeout)
        .await;

    let conv_url = driver
        .client()
        .evaluate_string("window.location.href")
        .await
        .unwrap_or(None)
        .unwrap_or_default();
    if conv_url.contains("gemini.google.com/app/") {
        eprintln!("info: conversation created: {conv_url}");
    }

    let _ = driver.close().await;
    let response = res?;

    if let Some(ref out_path) = args.output {
        write_output_file(out_path, &response)?;
        println!("{}", out_path.display());
    } else {
        print!("{response}");
        if !response.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

async fn handle_research(cdp: &str, args: ResearchArgs) -> Result<()> {
    let prompt = args.resolve_prompt()?;
    let timeout = Duration::from_secs(args.timeout);
    let preferred_url = args.conversation.as_deref();

    let driver =
        GeminiDriver::connect_and_prepare(cdp, preferred_url, Duration::from_secs(30)).await?;

    let res = driver.run_research(&prompt, timeout).await;

    let conv_url = driver
        .client()
        .evaluate_string("window.location.href")
        .await
        .unwrap_or(None)
        .unwrap_or_default();
    if conv_url.contains("gemini.google.com/app/") {
        eprintln!("info: conversation created: {conv_url}");
    }

    let _ = driver.close().await;
    let dossier = res?;

    write_output_file(&args.output, &dossier)?;
    println!("{}", args.output.display());

    Ok(())
}

async fn handle_gem(cdp: &str, args: GemArgs) -> Result<()> {
    let prompt = args.resolve_prompt()?;
    let timeout = Duration::from_secs(args.timeout);

    let driver =
        GeminiDriver::connect_and_prepare(cdp, Some(BASE_APP_URL), Duration::from_secs(30)).await?;

    let res = driver.run_gem(&args.id, &prompt, timeout).await;

    let conv_url = driver
        .client()
        .evaluate_string("window.location.href")
        .await
        .unwrap_or(None)
        .unwrap_or_default();
    if conv_url.contains("gemini.google.com/app/") {
        eprintln!("info: conversation created: {conv_url}");
    }

    let _ = driver.close().await;
    let response = res?;

    if let Some(ref out_path) = args.output {
        write_output_file(out_path, &response)?;
        println!("{}", out_path.display());
    } else {
        print!("{response}");
        if !response.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn write_output_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| AxiError::FileWrite(path.to_path_buf(), e))?;
    }
    fs::write(path, content).map_err(|e| AxiError::FileWrite(path.to_path_buf(), e))?;
    Ok(())
}
