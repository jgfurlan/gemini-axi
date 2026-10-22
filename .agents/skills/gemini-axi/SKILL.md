---
name: gemini-axi
description: Use Gemini Web App chat, Deep Research reports, or custom Gems through the agents Chrome session.
---

Chat, run Deep Research, and use Gems through the Gemini Web App.

Build from the gemini-axi checkout with Cargo; replace <gemini-axi-repo> with its absolute path.

- `cargo run --manifest-path <gemini-axi-repo>/Cargo.toml -- chat "<prompt>"`
- `cargo run --manifest-path <gemini-axi-repo>/Cargo.toml -- research "<topic>" -o <report.md>`
- `cargo run --manifest-path <gemini-axi-repo>/Cargo.toml -- gem`

Stdout is TOON; diagnostics go to stderr. Responses preview 1000 Unicode characters; use --full for complete stdout or -o <file> for complete Markdown. Use --fields to select response fields. Default browser endpoint: http://127.0.0.1:9333; override with --cdp <url>. Run without arguments for live login and models. Never repeat a prompt just to retrieve a longer response: save it with -o on the initial call.
