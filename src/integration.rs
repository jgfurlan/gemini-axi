//! Explicit, project-scoped integrations. Normal commands never install hooks.
use crate::error::{AxiError, Result};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

pub const DESCRIPTION: &str = "Chat, run Deep Research, and use Gems through the Gemini Web App";
pub const GUIDANCE: &[&str] = &[
    "gemini-axi chat \"<prompt>\"",
    "gemini-axi research \"<topic>\" -o <report.md>",
    "gemini-axi gem",
];
pub fn skill() -> String {
    let commands = GUIDANCE
        .iter()
        .map(|s| {
            format!(
                "- `cargo run --manifest-path <gemini-axi-repo>/Cargo.toml -- {}`",
                s.strip_prefix("gemini-axi ").unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\nname: gemini-axi\ndescription: Use Gemini Web App chat, Deep Research reports, or custom Gems through the agents Chrome session.\n---\n\n{DESCRIPTION}.\n\nBuild from the gemini-axi checkout with Cargo; replace <gemini-axi-repo> with its absolute path.\n\n{commands}\n\nStdout is TOON; diagnostics go to stderr. Responses preview 1000 Unicode characters; use --full for complete stdout or -o <file> for complete Markdown. Use --fields to select response fields. Default browser endpoint: http://127.0.0.1:9333; override with --cdp <url>. Run without arguments for live login and models. Never repeat a prompt just to retrieve a longer response: save it with -o on the initial call.\n"
    )
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn ensure_project_path(path: &Path) -> Result<()> {
    let root = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|e| AxiError::Configuration(e.to_string()))?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut existing = absolute.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| AxiError::Configuration("Invalid integration path".into()))?;
    }
    let real = fs::canonicalize(existing).map_err(|e| AxiError::Configuration(e.to_string()))?;
    if !real.starts_with(root) {
        return Err(AxiError::Configuration(format!(
            "Project integration path {} points outside this project",
            path.display()
        )));
    }
    Ok(())
}
fn write_changed(path: &Path, text: &str) -> Result<bool> {
    ensure_project_path(path)?;
    if fs::read_to_string(path).ok().as_deref() == Some(text) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AxiError::FileWrite(path.into(), e))?;
    }
    fs::write(path, text).map_err(|e| AxiError::FileWrite(path.into(), e))?;
    Ok(true)
}
fn read_optional(path: &Path) -> Result<String> {
    ensure_project_path(path)?;
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(AxiError::FileRead(path.into(), e)),
    }
}
pub fn portable_binary() -> Result<String> {
    let exe = std::env::current_exe().map_err(|e| AxiError::Other(e.to_string()))?;
    let canonical = fs::canonicalize(&exe).unwrap_or(exe.clone());
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("gemini-axi");
            if candidate.is_file() {
                if fs::canonicalize(candidate).ok().as_ref() == Some(&canonical) {
                    return Ok("gemini-axi".into());
                }
                break;
            }
        }
    }
    Ok(exe.to_string_lossy().into())
}
pub fn setup(app: &str, endpoint: &str) -> Result<bool> {
    let root = std::env::current_dir().map_err(|e| AxiError::Other(e.to_string()))?;
    let prefix = format!(
        "cd {} && {} --cdp {}",
        quote(&root.to_string_lossy()),
        quote(&portable_binary()?),
        quote(endpoint)
    );
    if app == "opencode" {
        let source =
            include_str!("opencode.js").replace("__COMMAND__", &serde_json::to_string(&prefix)?);
        let path = root.join(".opencode/plugins/gemini-axi.js");
        let old = read_optional(&path)?;
        if !old.is_empty() && !old.starts_with("// gemini-axi-managed:") {
            return Err(AxiError::Configuration(
                "Existing .opencode/plugins/gemini-axi.js is unmanaged; move it before setup"
                    .into(),
            ));
        }
        return write_changed(&path, &source);
    }
    let dir = if app == "claude" { ".claude" } else { ".codex" };
    let path = root.join(dir).join(if app == "claude" {
        "settings.json"
    } else {
        "hooks.json"
    });
    let content = read_optional(&path)?;
    let mut config: Value = if content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&content)?
    };
    let hooks = config
        .as_object_mut()
        .ok_or_else(|| AxiError::Configuration("hook configuration must be an object".into()))?
        .entry("hooks")
        .or_insert(json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| AxiError::Configuration("hooks must be an object".into()))?;
    for (event, suffix) in [("SessionStart", ""), ("SessionEnd", " --session-end")] {
        let groups = hooks
            .entry(event)
            .or_insert(json!([]))
            .as_array_mut()
            .ok_or_else(|| AxiError::Configuration("hook event must be an array".into()))?;
        let command = format!("{prefix}{suffix} # gemini-axi-managed");
        let replacement = json!({"type":"command", "command":command, "timeout":20});
        let mut found = false;
        for group in groups.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                for hook in list {
                    if hook["command"]
                        .as_str()
                        .is_some_and(|s| s.ends_with("# gemini-axi-managed"))
                    {
                        *hook = replacement.clone();
                        found = true;
                    }
                }
            }
        }
        if !found {
            groups.push(json!({"matcher":"", "hooks":[replacement]}));
        }
    }
    let codex_config = if app == "codex" {
        let path = root.join(".codex/config.toml");
        let text = read_optional(&path)?;
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| AxiError::Configuration("invalid project .codex/config.toml".into()))?;
        if doc.get("features").is_some_and(|v| !v.is_table_like()) {
            return Err(AxiError::Configuration(
                "Project .codex/config.toml features must be a table".into(),
            ));
        }
        doc["features"]["hooks"] = toml_edit::value(true);
        Some((path, doc.to_string()))
    } else {
        None
    };
    let mut changed = write_changed(&path, &(serde_json::to_string_pretty(&config)? + "\n"))?;
    if let Some((path, text)) = codex_config {
        changed |= write_changed(&path, &text)?;
    }
    Ok(changed)
}
pub fn capture() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .take(1024 * 1024)
        .read_to_string(&mut input)
        .map_err(|e| AxiError::Other(e.to_string()))?;
    let data: Value = serde_json::from_str(&input).unwrap_or(json!({}));
    let files = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let record = json!({"session_id":data["session_id"], "transcript_path":data["transcript_path"], "files_touched":files});
    write_changed(
        Path::new(".gemini-axi/last-session.json"),
        &(serde_json::to_string(&record)? + "\n"),
    )?;
    Ok(())
}
