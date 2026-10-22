# gemini-axi

Rust AXI for Gemini Web App chat, Deep Research and custom Gems through agents Chrome (CDP port **9333**). It uses the signed-in browser account, rather than a Gemini API key. Gemini account access and usage limits still apply.

```sh
cargo build --release
./target/release/gemini-axi                 # live Chrome/login/models dashboard
./target/release/gemini-axi chat "<prompt>"
./target/release/gemini-axi research "<topic>" -o report.md
./target/release/gemini-axi gem             # available Gems; explicit zero when empty
./target/release/gemini-axi gem --id <id> "<prompt>"
```

Use `gemini-axi` below when installed on PATH, or substitute `./target/release/gemini-axi`. Next-step hints omit the default endpoint and retain a custom endpoint when needed. Global `--cdp <url>` / `GEMINI_AXI_CDP` override the browser endpoint; `--verbose` logs diagnostics to stderr. `-v`, `-V` and `--version` print only the version, without connecting to Chrome.

## Output contract

Stdout contains TOON for data, errors, help and next steps. Internal browser and response data stay typed; serialization happens at the output boundary. Progress and diagnostics go to stderr. The encoder supports the CLI's object, scalar, scalar-array and uniform flat-table schemas; keys and string values follow TOON §7 canonical quoting: simple keys and unambiguous strings stay bare; delimiters, type-like strings and control characters are quoted and escaped to preserve Markdown safely.

For example:

```toon
result:
  chars: 7
  conversation: "https://gemini.google.com/app/<id>"
  model: Argon
  response: OK AXI.
  truncated: false
help[1]: gemini-axi gem
```

Responses preview the first **1000 Unicode characters** by default and include the total `chars` count and `truncated` status. `--full` prints the complete response. `-o <file>` always saves complete Markdown, while stdout still includes a preview and metadata. Choose `--full` or `-o` on the initial request: repeating a chat sends another prompt. A truncated response suggests reading the saved file when available.

`--fields response,chars,model,conversation,output,truncated` selects response fields for all three commands. Omit fields you do not need; `output` exists only when a file was saved. `gem` lists only `id,name` by default, with `--fields id,name,url` available. Lists include the total count. Model metadata is observed from the current picker, not inferred from a requested model name; it is `null` when unavailable. The dashboard reads visible model choices, falling back to the current model if choices cannot be read.

## Commands

Each command accepts `--help` for its own flags, defaults and examples. Flags and positional arguments are strictly validated before browser access. Commands never prompt interactively.

| Command | Input | Optional flags | Default timeout |
| --- | --- | --- | --- |
| `chat` | `"<prompt>"` or `-p/--prompt-file <file>` | `-m/--model <name>`, `-o/--output <file>`, `--full`, `--fields <fields>`, `--timeout <seconds>` | 120 seconds |
| `research` | `"<topic>"`, `-p/--prompt-file <file>`, or `--conversation <url>` | `-o/--output <file>`, `--full`, `--fields <fields>`, `--timeout <seconds>` | 1800 seconds |
| `gem` | No arguments to list; `--id <id>` plus `"<prompt>"` or `-i/--input <file>` to send | `-o/--output <file>`, `--full`, `--fields <fields>`, `--timeout <seconds>` | 180 seconds |

Research approves the generated plan and waits for the **completed report**, rather than returning the plan. `--conversation https://gemini.google.com/app/<id>` resumes existing research; a completed report can be read again without submitting another prompt. New chat and Gem requests use fresh tabs. Owned tabs close after completion; existing research tabs and unrelated tabs remain open. Chrome stays running.

The no-argument dashboard observes a fresh Gemini tab without sending a prompt or launching Chrome. It reports `browser`, `login` and `models`; browser unavailability is a successful dashboard result. Sending commands retain automatic launch of the dedicated agents Chrome profile on the default endpoint. An explicit endpoint disables launch. Login may be `unknown` when the app cannot be observed reliably; sign-in and Chrome access permission must be handled in the browser.

Errors have a stable `error.code`, an actionable message and help on stdout. Exit **0** means success (including empty lists and already configured integrations), **1** means an operation, browser or file error, and **2** means invalid usage. No dependency stack traces appear on stdout.

## Ambient context and skill

Recommended: explicitly opt into a **project-scoped session integration**. Nothing is installed by ordinary commands, and setup never writes user-wide configuration:

```sh
gemini-axi --setup-hooks claude
gemini-axi --setup-hooks codex
gemini-axi --setup-hooks opencode
```

Select the app you use. The formats follow [Codex hooks](https://learn.chatgpt.com/docs/hooks) and [OpenCode plugin lifecycle](https://opencode.ai/v2/docs/build/plugins/). Setup installs or repairs native `SessionStart` / `SessionEnd` hooks in `.claude/settings.json` or `.codex/hooks.json`; Codex also gets `[features].hooks = true` in project `.codex/config.toml`. OpenCode receives `.opencode/plugins/gemini-axi.js`, with ambient system context for OpenCode 1 and 2, capture on the OpenCode 1 session-deletion event, and a final session snapshot on OpenCode 2 plugin unload. Repeating setup preserves unrelated hooks and makes no filesystem changes when already current. Hook commands carry the chosen CDP endpoint and project directory, use a PATH binary only when it resolves to this executable, and repair an absolute fallback when setup runs after relocation. The chosen executable must remain available after setup.

At session end, capture stores the session ID, transcript **path** (when supplied by the harness), and tracked files changed relative to HEAD in `.gemini-axi/last-session.json`. It does not copy transcripts or claim a precise per-session file history. Future dashboards show only the last session ID and changed-file count. This context and capture remain scoped to the project where setup ran.

Alternatively, install the on-demand skill (use either integration path or both):

```sh
npx -y skills add jgfurlan/gemini-axi --skill gemini-axi
```

The committed `.agents/skills/gemini-axi/SKILL.md` comes from the same description and command guidance as the home view, without live state. Its examples work from a source checkout without a global install. Regenerate and verify it with:

```sh
cargo run -- --write-skill .agents/skills/gemini-axi/SKILL.md
cargo run -- --check-skill .agents/skills/gemini-axi/SKILL.md
```

The test suite checks the generated skill for drift on every test run, including CI. Commit hook configuration only if appropriate for your project; `.gemini-axi/` session records are local runtime state.

## Validation

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --check-skill .agents/skills/gemini-axi/SKILL.md
```

Mock CDP tests cover chat, research, Gem responses and lists, Unicode truncation, full output files, field selection, empty accounts, structured errors, preflight validation, help, live/offline dashboards and idempotent session setup. They send no real Gemini prompts.

MIT © [Joao Gabriel Furlan](https://github.com/jgfurlan)
