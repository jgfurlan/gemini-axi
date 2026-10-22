# gemini-axi

> High-performance, headless, zero-token Agent eXecutable Interface (AXI) in Rust for the Gemini Web App.

`gemini-axi` bridges autonomous agent fleets (Firstmate, Claude Code, Codex, AGY) and the **Gemini Web App** ecosystem (Gemini Advanced, Argon, Deep Research, Gems, and Canvas) with **near-zero token consumption** for the calling LLM.

---

## ⚡ The Architectural Vision: "Argon for Specs, Flash for Code"

Running multi-step research and architecture design directly inside frontier coding agents burns millions of expensive tokens and exhausts strict provider quotas in minutes.

`gemini-axi` enables a radically cost-efficient and mathematically sound division of labor:

```
┌─────────────────────────────────────────────────────────────────┐
│                     1. ARCHITECTURE & SPEC                      │
│   Gemini Web App (Argon / Deep Research / Custom Gems)          │
│   • Multi-turn web research, clinical papers, deep reasoning    │
│   • Produces dense, unambiguous, deterministic specs & LaTeX   │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                     gemini-axi  │ (Zero-token CLI pipe)
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                    2. SURGICAL IMPLEMENTATION                   │
│   Workhorse Coding Agents (Gemini 3.8 Flash / Compiler TDD)     │
│   • Implements specs with compiler-checked rigor                │
│   • Near-infinite runway, zero quota stress on frontier models   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🚀 Key Features

* **Zero-Token Agent Overhead:** The calling agent invokes a single UNIX command (`gemini-axi research ...`).
  All browser navigation, WebSocket communication, plan waiting, and Markdown parsing is handled in native compiled Rust.
  The LLM receives only the final file path or response text (< 80 tokens total).
* **Deep Research Automation:** Triggers Gemini Deep Research runs, auto-approves research plans, monitors multi-step synthesis, and saves complete dossiers directly to disk.
* **Direct Argon Chat & Custom Gems:** Converse with Gemini 2.0 / Argon models and custom domain Gems headlessly without API rate limits.
* **Direct CDP WebSocket Bridge:** Communicates directly with Chrome's remote debugging port (`ws://127.0.0.1:9222`) over Tokio-Tungstenite with zero Electron or Node baggage.

---

## 🛠️ CLI Interface

```bash
# Run Deep Research and stream clean Markdown directly to disk
gemini-axi research --prompt-file spec-request.md --output data/reports/dossier.md

# One-shot architecture chat with Argon (prints response Markdown to stdout)
gemini-axi chat "Draft an invariant-driven architecture for a distributed ledger" -m argon

# One-shot chat saving Markdown directly to file (prints only the file path to stdout)
gemini-axi chat "Summarize Paxos consensus invariants" -m argon -o spec.md

# Query a custom domain Gem using athlete input data
gemini-axi gem --id "hypertrophy-physiologist" --input athlete-data.json
```

### Global Options

* `--cdp <URL>`: Chrome DevTools Protocol endpoint URL (default: `http://127.0.0.1:9222`, env: `GEMINI_AXI_CDP`).
* `-v, --verbose`: Enables debug logging on `stderr` (keeps `stdout` clean for agent consumption).
* `-h, --help`: Prints short, focused command help.

### Subcommands

* `chat [PROMPT]`:
  * `-p, --prompt-file <FILE>`: Read prompt from file instead of inline argument.
  * `-m, --model <MODEL>`: Select model via web interface (e.g. `argon`, `pro`, `flash`).
  * `-o, --output <FILE>`: Write response Markdown to file instead of `stdout`.
  * `--timeout <SECS>`: Response timeout in seconds (default: `120`).
* `research [PROMPT]`:
  * `-p, --prompt-file <FILE>`: Read research prompt from file.
  * `-o, --output <FILE>`: Destination file for synthesized dossier Markdown (required).
  * `--timeout <SECS>`: Total timeout for research and plan execution (default: `600`).
* `gem [PROMPT]`:
  * `--id <GEM_ID>`: Custom Gem identifier or slug (required).
  * `-i, --input <FILE>`: Input file containing context or data for the Gem.
  * `-o, --output <FILE>`: Write response Markdown to file instead of `stdout`.
  * `--timeout <SECS>`: Response timeout in seconds (default: `180`).

---

## 🌐 Chrome Setup with Remote Debugging

Recent versions of Google Chrome ignore `--remote-debugging-port` when launched against the default user profile while another Chrome instance is running.
To enable headless automation with your authenticated Google account, launch Chrome with a dedicated `--user-data-dir`.

### 1. Launch Chrome with Debugging Port and Dedicated Profile

On macOS:
```bash
/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
  --remote-debugging-port=9222 \
  --user-data-dir="$HOME/.gemini-axi/chrome-profile"
```

On Linux:
```bash
google-chrome \
  --remote-debugging-port=9222 \
  --user-data-dir="$HOME/.gemini-axi/chrome-profile"
```

On Windows:
```cmd
chrome.exe --remote-debugging-port=9222 --user-data-dir="%LOCALAPPDATA%\gemini-axi\chrome-profile"
```

### 2. Sign In Once

Open `https://gemini.google.com` in the dedicated Chrome window and sign in to your Google account.
Your session cookies and credentials remain persisted inside `--user-data-dir`.
All subsequent `gemini-axi` commands connect to that running Chrome instance without prompting for authentication.

---

## 📦 Architecture and CDP Client Rationale

`gemini-axi` uses `tokio-tungstenite` rather than high-level browser harnesses like `chromiumoxide`.

* **Zero Process Management Overhead:** High-level browser frameworks bundle Chrome management, process launching, and complex state trees designed for automated testing.
* **Direct Protocol Control:** `gemini-axi` attaches to an existing authenticated user session over Chrome's raw JSON-RPC WebSocket interface (`Runtime.evaluate`, `Page.navigate`, `Input.dispatchKeyEvent`).
* **Sub-Millisecond Startup:** Startup time is under 3ms with a memory footprint under 15MB.
* **Deterministic Verification:** Raw CDP messaging allows local mock WebSocket servers to fully test chat, research, model switching, and error handling without external dependencies.

---

## License

MIT © [Joao Gabriel Furlan](https://github.com/jgfurlan)
