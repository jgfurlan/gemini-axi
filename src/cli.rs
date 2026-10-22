use clap::{Args, Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use crate::error::{AxiError, Result};

#[derive(Parser, Debug)]
#[command(
    name = "gemini-axi",
    about = "Zero-token CDP bridge for the Gemini Web App",
    version,
    help_template = "\
{before-help}{name} {version}
{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}"
)]
pub struct Cli {
    /// Chrome DevTools Protocol endpoint URL
    #[arg(
        long,
        env = "GEMINI_AXI_CDP",
        default_value = "http://127.0.0.1:9222",
        global = true,
        help = "CDP endpoint (e.g. http://127.0.0.1:9222 or ws://...)"
    )]
    pub cdp: String,

    /// Verbose logging to stderr
    #[arg(short, long, global = true, help = "Log debug info to stderr")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Send a chat prompt to Gemini (with optional model selection)
    #[command(about = "Send a chat prompt to Gemini")]
    Chat(ChatArgs),

    /// Run Deep Research, auto-approve research plan, and save dossier to disk
    #[command(about = "Run Deep Research and save dossier to disk")]
    Research(ResearchArgs),

    /// Interact with a custom Gemini Gem
    #[command(about = "Interact with a custom Gem")]
    Gem(GemArgs),
}

#[derive(Args, Debug)]
pub struct ChatArgs {
    /// Prompt text to send
    #[arg(value_name = "PROMPT", help = "Prompt text string")]
    pub prompt: Option<String>,

    /// Read prompt text from file
    #[arg(
        short = 'p',
        long = "prompt-file",
        value_name = "FILE",
        help = "File containing the prompt"
    )]
    pub prompt_file: Option<PathBuf>,

    /// Select Gemini model (e.g. argon, pro, flash)
    #[arg(
        short = 'm',
        long = "model",
        value_name = "MODEL",
        help = "Model identifier (e.g. argon, pro, flash)"
    )]
    pub model: Option<String>,

    /// Write response Markdown to file instead of stdout
    #[arg(
        short = 'o',
        long = "output",
        value_name = "FILE",
        help = "Save response Markdown to file"
    )]
    pub output: Option<PathBuf>,

    /// Timeout in seconds
    #[arg(
        long = "timeout",
        default_value = "120",
        value_name = "SECS",
        help = "Operation timeout in seconds"
    )]
    pub timeout: u64,
}

impl ChatArgs {
    pub fn resolve_prompt(&self) -> Result<String> {
        if let Some(ref p) = self.prompt {
            return Ok(p.clone());
        }
        if let Some(ref path) = self.prompt_file {
            return fs::read_to_string(path).map_err(|e| AxiError::FileRead(path.clone(), e));
        }
        Err(AxiError::EmptyPrompt)
    }
}

#[derive(Args, Debug)]
pub struct ResearchArgs {
    /// Research prompt text string
    #[arg(value_name = "PROMPT", help = "Research prompt string")]
    pub prompt: Option<String>,

    /// Read research prompt from file
    #[arg(
        short = 'p',
        long = "prompt-file",
        value_name = "FILE",
        help = "File containing research prompt"
    )]
    pub prompt_file: Option<PathBuf>,

    /// Destination file for the complete research dossier
    #[arg(
        short = 'o',
        long = "output",
        required = true,
        value_name = "FILE",
        help = "Destination file for Markdown dossier"
    )]
    pub output: PathBuf,

    /// Timeout in seconds (Deep Research may take several minutes)
    #[arg(
        long = "timeout",
        default_value = "600",
        value_name = "SECS",
        help = "Operation timeout in seconds"
    )]
    pub timeout: u64,
}

impl ResearchArgs {
    pub fn resolve_prompt(&self) -> Result<String> {
        if let Some(ref p) = self.prompt {
            return Ok(p.clone());
        }
        if let Some(ref path) = self.prompt_file {
            return fs::read_to_string(path).map_err(|e| AxiError::FileRead(path.clone(), e));
        }
        Err(AxiError::EmptyPrompt)
    }
}

#[derive(Args, Debug)]
pub struct GemArgs {
    /// Custom Gem ID or slug
    #[arg(
        long = "id",
        required = true,
        value_name = "GEM_ID",
        help = "Gem identifier or slug"
    )]
    pub id: String,

    /// Input file to send as prompt
    #[arg(
        short = 'i',
        long = "input",
        value_name = "FILE",
        help = "Input file to send to Gem"
    )]
    pub input: Option<PathBuf>,

    /// Direct prompt string
    #[arg(value_name = "PROMPT", help = "Direct prompt string")]
    pub prompt: Option<String>,

    /// Save response Markdown to file instead of stdout
    #[arg(
        short = 'o',
        long = "output",
        value_name = "FILE",
        help = "Save response Markdown to file"
    )]
    pub output: Option<PathBuf>,

    /// Timeout in seconds
    #[arg(
        long = "timeout",
        default_value = "180",
        value_name = "SECS",
        help = "Operation timeout in seconds"
    )]
    pub timeout: u64,
}

impl GemArgs {
    pub fn resolve_prompt(&self) -> Result<String> {
        if let Some(ref input_path) = self.input {
            return fs::read_to_string(input_path)
                .map_err(|e| AxiError::FileRead(input_path.clone(), e));
        }
        if let Some(ref p) = self.prompt {
            return Ok(p.clone());
        }
        Err(AxiError::EmptyPrompt)
    }
}
