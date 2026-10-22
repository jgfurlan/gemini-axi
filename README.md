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
* **Direct CDP WebSocket Bridge:** Communicates directly with Chrome's remote debugging port (`http://127.0.0.1:9333`) over Tokio-Tungstenite with zero Electron or Node baggage.

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

* `--cdp <URL>`: Chrome DevTools Protocol endpoint URL (default: `http://127.0.0.1:9333`, env: `GEMINI_AXI_CDP`).
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
  * `--conversation <URL>`: Resume or attach to an existing Gemini conversation URL.
  * `--timeout <SECS>`: Total timeout for research and plan execution (default: `1800` / 30 min).
* `gem [PROMPT]`:
  * `--id <GEM_ID>`: Custom Gem identifier or slug (required).
  * `-i, --input <FILE>`: Input file containing context or data for the Gem.
  * `-o, --output <FILE>`: Write response Markdown to file instead of `stdout`.
  * `--timeout <SECS>`: Response timeout in seconds (default: `180`).

---

## 🌐 Connecting to Chrome: Dedicated Agents Chrome

`gemini-axi` connects by default to a dedicated Chrome instance running on port `9333` with profile `~/.agents-chrome`.

### Why a Dedicated Chrome Instance?

1. **No Intrusive Permission Popups:** Chrome's default daily profile requires user authorization on every connection via an OS-level *"Allow remote debugging"* prompt. Dedicated debugging port `9333` with `--user-data-dir` uses standard DevTools protocol without continuous confirmation dialogs.
2. **No Background Throttling:** Normal browsing aggressively throttles background and occluded tabs (pausing `requestAnimationFrame` and clamping timers to 1Hz), which stalls streaming AI responses. Starting with backgrounding disabled ensures reliable, unthrottled execution.
3. **Total Isolation:** Keeps personal tabs, browsing history, and cookies completely separate from automated agents.

### Step-by-Step Setup

1. **One-Time Startup & Login:**
   Launch Chrome with the dedicated agents profile:

   **macOS:**
   ```bash
   open -na "Google Chrome" --args \
     --user-data-dir="$HOME/.agents-chrome" \
     --remote-debugging-port=9333 \
     --no-first-run \
     --no-default-browser-check \
     --disable-renderer-backgrounding \
     --disable-backgrounding-occluded-windows \
     --disable-background-timer-throttling
   ```

   **Linux:**
   ```bash
   google-chrome \
     --user-data-dir="$HOME/.agents-chrome" \
     --remote-debugging-port=9333 \
     --no-first-run \
     --no-default-browser-check \
     --disable-renderer-backgrounding \
     --disable-backgrounding-occluded-windows \
     --disable-background-timer-throttling &
   ```

2. **Log into Google Gemini:**
   In that newly opened Chrome window, navigate to `https://gemini.google.com` and sign in once. Your session and cookies remain saved in `~/.agents-chrome`.

3. **Run `gemini-axi`:**
   Simply run commands without extra flags:
   ```bash
   gemini-axi chat "Hello Gemini"
   ```

   * **Auto-Launch:** If Chrome is not running when `gemini-axi` executes with default options, `gemini-axi` automatically spawns the agents Chrome instance with the appropriate flags and waits for port 9333 to respond.
   * **Custom Endpoint:** If using an existing CDP endpoint or remote runner, specify `--cdp`:
     ```bash
     gemini-axi --cdp http://127.0.0.1:9222 chat "Hello Gemini"
     ```
     When `--cdp` is explicitly supplied, `gemini-axi` connects directly without attempting to launch any browser.

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
