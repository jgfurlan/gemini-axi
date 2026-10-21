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

* **Zero-Token Agent Overhead:** The calling agent invokes a single UNIX command (`gemini-axi research "..."`). 100% of browser navigation, WebSocket communication, waiting, and Markdown parsing is handled in native compiled Rust. The LLM only receives the final path or output string (< 80 tokens total).
* **Deep Research Automation:** Triggers Gemini Deep Research runs, auto-approves research plans, monitors multi-step synthesis, and saves complete multi-page dossiers directly to disk.
* **Direct Argon Chat & Custom Gems:** Converse with Gemini 2.0 / Argon models and custom domain Gems headlessly without API rate limits or feature gating.
* **Direct CDP WebSocket Bridge:** Talks directly to Chrome's remote debugging port (`ws://127.0.0.1:9222`) over Tokio-Tungstenite with zero electron/node baggage.

---

## 🛠️ CLI Interface (Planned)

```bash
# Run Deep Research and stream clean Markdown directly to disk
gemini-axi research --prompt-file spec-request.md --output data/reports/dossier.md

# One-shot architecture chat with Argon
gemini-axi chat "Draft an invariant-driven architecture for a distributed ledger" -m argon

# Query a custom domain Gem
gemini-axi gem --id "hypertrophy-physiologist" --input athlete-data.json
```

---

## 📦 Architecture

* **Language:** Rust (Tokio, Tokio-Tungstenite, Clap)
* **Browser Driver:** Headless Chrome DevTools Protocol (CDP) WebSocket
* **Memory footprint:** < 15MB RAM
* **Startup time:** < 3ms

---

## License

MIT © [Joao Gabriel Furlan](https://github.com/jgfurlan)
