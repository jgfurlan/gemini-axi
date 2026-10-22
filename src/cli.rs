use clap::{Args, Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use crate::error::{AxiError, Result};

#[derive(Parser, Debug)]
#[command(
    name = "gemini-axi",
    about = crate::integration::DESCRIPTION,
    version,
    after_help = "Examples:\n  gemini-axi\n  gemini-axi chat \"<prompt>\"\n  gemini-axi research \"<topic>\" -o report.md",
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
        default_value = "http://127.0.0.1:9333",
        global = true,
        help = "CDP endpoint (defaults to http://127.0.0.1:9333)"
    )]
    pub cdp: String,

    /// Verbose logging to stderr
    #[arg(long, global = true, help = "Log debug info to stderr")]
    pub verbose: bool,

    /// Install or repair project-scoped session integration (explicit opt-in)
    #[arg(long, value_parser = ["claude", "codex", "opencode"])]
    pub setup_hooks: Option<String>,

    /// Generate the installable skill from CLI guidance
    #[arg(long, value_name = "FILE", conflicts_with_all = ["setup_hooks", "check_skill"])]
    pub write_skill: Option<PathBuf>,

    /// Fail if the committed skill differs from CLI guidance
    #[arg(long, value_name = "FILE", conflicts_with = "setup_hooks")]
    pub check_skill: Option<PathBuf>,

    #[arg(long, hide = true, conflicts_with_all = ["setup_hooks", "write_skill", "check_skill"])]
    pub session_end: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Returns true if CDP was explicitly provided via --cdp or GEMINI_AXI_CDP.
    pub fn is_explicit_cdp(&self) -> bool {
        std::env::var("GEMINI_AXI_CDP").is_ok()
            || std::env::args().any(|a| a == "--cdp" || a.starts_with("--cdp="))
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Send a chat prompt to Gemini (with optional model selection)
    #[command(
        about = "Send a chat prompt to Gemini",
        after_help = "Examples:\n  gemini-axi chat \"<prompt>\"\n  gemini-axi chat --prompt-file request.md -o response.md --full"
    )]
    Chat(ChatArgs),

    /// Run Deep Research, auto-approve research plan, and save dossier to disk
    #[command(
        about = "Run Deep Research and save dossier to disk",
        after_help = "Examples:\n  gemini-axi research \"<topic>\" -o report.md\n  gemini-axi research --conversation <url> --full"
    )]
    Research(ResearchArgs),

    /// Interact with a custom Gemini Gem
    #[command(
        about = "List Gems or interact with a custom Gem",
        after_help = "Examples:\n  gemini-axi gem\n  gemini-axi gem --id <id> \"<prompt>\""
    )]
    Gem(GemArgs),
}

#[derive(Args, Debug)]
pub struct ChatArgs {
    #[command(flatten)]
    pub display: DisplayArgs,
    /// Prompt text to send
    #[arg(
        value_name = "PROMPT",
        conflicts_with = "prompt_file",
        help = "Prompt text string"
    )]
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

    /// Save complete response Markdown and include a preview on stdout
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
    #[command(flatten)]
    pub display: DisplayArgs,
    /// Research prompt text string
    #[arg(
        value_name = "PROMPT",
        conflicts_with = "prompt_file",
        help = "Research prompt string"
    )]
    pub prompt: Option<String>,

    /// Read research prompt from file
    #[arg(
        short = 'p',
        long = "prompt-file",
        value_name = "FILE",
        help = "File containing research prompt"
    )]
    pub prompt_file: Option<PathBuf>,

    /// Existing Gemini conversation URL to resume (e.g. https://gemini.google.com/app/...)
    #[arg(
        long = "conversation",
        value_name = "URL",
        help = "Existing Gemini conversation URL to resume"
    )]
    pub conversation: Option<String>,

    /// Destination file for the complete research dossier
    #[arg(
        short = 'o',
        long = "output",
        value_name = "FILE",
        help = "Destination file for Markdown dossier"
    )]
    pub output: Option<PathBuf>,

    /// Timeout in seconds (Deep Research may take 5-20 minutes, default: 1800s / 30m)
    #[arg(
        long = "timeout",
        default_value = "1800",
        value_name = "SECS",
        help = "Operation timeout in seconds (default: 1800)"
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
        if self.conversation.is_some() {
            return Ok(String::new());
        }
        Err(AxiError::EmptyPrompt)
    }
}

#[derive(Args, Debug)]
pub struct GemArgs {
    #[command(flatten)]
    pub display: DisplayArgs,
    /// Custom Gem ID or slug
    #[arg(long = "id", value_name = "GEM_ID", help = "Gem identifier or slug")]
    pub id: Option<String>,

    /// Input file to send as prompt
    #[arg(
        short = 'i',
        long = "input",
        value_name = "FILE",
        help = "Input file to send to Gem"
    )]
    pub input: Option<PathBuf>,

    /// Direct prompt string
    #[arg(
        value_name = "PROMPT",
        conflicts_with = "input",
        help = "Direct prompt string"
    )]
    pub prompt: Option<String>,

    /// Save complete response Markdown and include a preview on stdout
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

#[derive(Args, Debug, Default)]
pub struct DisplayArgs {
    /// Include complete response (default: first 1000 Unicode characters)
    #[arg(long)]
    pub full: bool,
    /// Fields: response,chars,model,conversation,output,truncated; Gem lists: id,name,url
    #[arg(long, value_delimiter = ',')]
    pub fields: Vec<String>,
}
