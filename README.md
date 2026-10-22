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

## 🌐 Connecting to Chrome

`gemini-axi` can connect to your everyday Chrome instance (already logged in with your Google account) **without needing a dedicated profile or restarting your browser**.

### Method 1: Everyday Chrome via `chrome://inspect` (Recommended)

1. Open your standard Google Chrome browser where you are already signed in to Gemini.
2. In the omnibox, navigate to:
   ```text
   chrome://inspect/#remote-debugging
   ```
3. Check the box **"Allow remote debugging for this browser instance"** (or in Portuguese: "Permitir a depuração remota para esta instância do navegador").
4. Chrome will create a `DevToolsActivePort` file inside your user-data directory and start listening on loopback (default port `9222`).
5. (macOS only): On the first connection, Chrome presents an approval dialog asking *"Allow remote debugging?"*. Click **"Allow"** (or run `browser-harness mac-approve`).
6. Run any `gemini-axi` command without arguments:
   ```bash
   gemini-axi chat "teste gemini-axi: responda 'calibracao ok'"
   ```
   `gemini-axi` automatically scans standard profile directories (`Library/Application Support/Google/Chrome`, `~/.config/google-chrome`, `%LOCALAPPDATA%/Google/Chrome/User Data`), reads `DevToolsActivePort`, attaches to your active Gemini session via the browser WebSocket (`Target.attachToTarget`), and drives the conversation headlessly.

---

### Method 2: Dedicated Profile or Custom Endpoint (`--cdp`)

If you prefer an isolated automation profile or run a headless Chrome on a remote machine:

1. Launch Chrome with `--remote-debugging-port` and a dedicated `--user-data-dir`:

   **macOS:**
   ```bash
   /Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
     --remote-debugging-port=9222 \
     --user-data-dir="$HOME/.gemini-axi/chrome-profile"
   ```

   **Linux:**
   ```bash
   google-chrome \
     --remote-debugging-port=9222 \
     --user-data-dir="$HOME/.gemini-axi/chrome-profile"
   ```

   **Windows:**
   ```cmd
   chrome.exe --remote-debugging-port=9222 --user-data-dir="%LOCALAPPDATA%\gemini-axi\chrome-profile"
   ```

2. Sign in once at `https://gemini.google.com`. Credentials remain saved in the directory.
3. Pass `--cdp` (or set `GEMINI_AXI_CDP`):
   ```bash
   gemini-axi --cdp http://127.0.0.1:9222 chat "Hello Gemini"
   ```

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
