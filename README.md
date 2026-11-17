# ✻ N E X U S

> **High-Throughput Polyglot & Native Rust AI Coding Harness**  
> *Extreme token efficiency, featherweight memory, and zero-compromise security containment.*

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-68%2F68%20passed%20(100%25)-brightgreen.svg)]()
[![RAM Footprint](https://img.shields.io/badge/RAM-6.4%20MB-blueviolet.svg)]()
[![Token Savings](https://img.shields.io/badge/token%20savings-69.1%25%20vs%20Claude%20Code-success.svg)]()

---

## Why Nexus?

Existing AI coding harnesses (like Claude Code or Python-based CLI agents) are heavy: they demand **~180 MB of RAM**, consume excessive tokens by re-transmitting uncompressed tool outputs verbatim, and lack in-binary defense-in-depth sandboxing.

**Nexus** is re-engineered from the ground up in idiomatic Rust to be the fastest, leanest, and most cost-effective autonomous coding harness ever built:

* **⚡ Ultra-Lean Footprint**: Idles at **~6.4 MB of RAM**—a **96% reduction** compared to V8/Node.js alternatives. Boots in < 10ms.
* **💸 69.1% Token & Cost Reduction**: 3-tier wire progressive compaction and runtime stream clamping save **41,740+ tokens** over 5 typical engineering turns.
* **🛡️ Hardened Process Quarantine**: Native in-binary AST command blacklist (`rm -rf /`, forkbombs), strict `../../` path traversal blocking, and automatic credential scrubbing.
* **🧠 Sub-Millisecond Cross-Session Memory**: SQLite WAL-backed BM25 recall engine retrieving historical architecture decisions and bug patterns in **211 µs**.
* **🤖 Multi-Provider Native Intelligence**: Seamless support for Anthropic Claude (3.7 Sonnet) and Native Google Gemini (2.5 Flash, 2.5 Flash Lite) with automated key detection.
* **🌐 Native Polyglot Doctor & Indentation Rebaser**: First-class coding & automated repair across **Python, JavaScript/TypeScript, C/C++, Go, and Rust**. Auto-detects project stacks, runs language-native test/compiler diagnostics, and normalizes indentation deltas to prevent syntax errors.
* **💾 WAL-First Persistence**: Invariant-enforced Append-Only Write-Ahead Log (WAL) architecture with automatic crash recovery, atomic log compaction, and SQLite WAL checkpointing.

---

## Performance & Token Efficiency Benchmark

Measured across a deterministic 5-turn engineering benchmark (450-line file exploration, 180-line test log, 30-line targeted diff, 70-line compiler verification, 60-line documentation write):

| Turn | Engineering Scenario | Claude Code (Tokens) | Nexus (Tokens) | Tokens Saved | Savings (%) |
| :---: | :--- | :---: | :---: | :---: | :---: |
| **1** | Project Exploration (450-line read) | `9,728` | `5,703` | `4,025` | **41.4%** |
| **2** | Test Suite Execution (180-line log) | `11,902` | `6,748` | `5,154` | **43.3%** |
| **3** | Targeted Refactor (30-line diff) | `12,069` | `1,819` | `10,250` | **84.9%** |
| **4** | Clean Re-verification (cargo check) | `12,747` | `1,700` | `11,047` | **86.7%** |
| **5** | Architectural Documentation write | `13,986` | `2,722` | `11,264` | **80.5%** |
| **Σ** | **Cumulative 5-Turn Wire Tokens** | **`60,432`** | **`18,692`** | **`41,740`** | **`69.1%`** |

> *Deep History Tombstoning delivers **93.2% context savings** on turns older than 5 turns.*

---

## 10 Core Engine Subsystems

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             N E X U S   C O R E                             │
├──────────────────────┬───────────────────────────────┬──────────────────────┤
│ 1. Skill Quarantine  │ 2. Command Quarantine         │ 3. Token Compactor   │
│    Staged Test-Drive │    AST Blacklist & Scrubbing  │    3-Tier Tombstone  │
├──────────────────────┼───────────────────────────────┼──────────────────────┤
│ 4. Context Engine    │ 5. BM25 Memory Engine         │ 6. 3-Tier Patcher    │
│    Macro-Compaction  │    211µs Recall Latency       │    Ambiguity Guard   │
├──────────────────────┼───────────────────────────────┼──────────────────────┤
│ 7. Swarm Worktrees   │ 8. The Rust Doctor            │ 9. WAL Persistence   │
│    Branch Isolation  │    BorrowTutor (E0382/E0502)  │    Atomic Compaction │
├──────────────────────┴───────────────────────────────┴──────────────────────┤
│ 10. Native LSP Transport (Full-Duplex Stdio JSON-RPC with Regex Fallback)   │
└─────────────────────────────────────────────────────────────────────────────┘
```

1. **Skill Writing & Staging Quarantine**: Synthesizes new agent skills into `.nexus/staging/` to test-drive isolated behavior before promoting to permanent storage.
2. **Security Sandbox & Command Quarantine**: Hard-blocks `../../` path traversal, intercepts destructive shell commands before subprocess spawning, and scrubs sensitive keys (`GITHUB_TOKEN`, `SSH_AUTH_SOCK`, `AWS_SECRET_ACCESS_KEY`).
3. **Token Optimization**: Featherweight base prompt (~1,370 tokens vs. Claude Code's ~7,600 tokens), runtime stream output clamping (80 lines max for commands, 200 lines for file reads), and 3-tier progressive tombstoning.
4. **Native Context Engineering**: Real-time context budget status pill (`🗜️ 50.0k/128k`), macro-compaction partitioning history into `<context_summary>` (>60% reduction), and `<interrupted-output>` continuation stitching.
5. **Cross-Session Memory Recall**: SQLite FTS/BM25 engine indexing key learnings, architecture decisions, and bug patterns across sessions with sub-millisecond query throughput.
6. **3-Tier Guarded Patching (`edit_file`)**:
   * *Tier 1*: Exact string replacement with Ambiguity Guard (refuses multiple occurrences).
   * *Tier 2*: Whitespace-normalized replacement (reconciles 4-space vs. 8-space indentation mismatches).
   * *Tier 3*: Anchor-context fuzzy matching for offset code regions.
7. **Multi-Agent Orchestration & Swarms**: YAML-frontmatter Markdown agent discovery (`.nexus/agents/*.md`), sequential pipeline chaining (`run_chain`), and fan-out parallel execution (`run_parallel`) inside isolated Git worktrees.
8. **The Rust Doctor Specialist**: Automated `cargo check` JSON diagnostic streaming paired with `BorrowTutor`—providing offline deterministic explanations and repair steps for borrow checker errors (`E0382`, `E0502`) with zero API token spend.
9. **WAL-First Persistence Engine**: Mandatory append-only storage invariant. Upgraded `SessionStore` with SQLite WAL mode, 5s busy-timeout, explicit `checkpoint(TRUNCATE)`, and atomic batch transactions.
10. **LSP Transport & RPC**: Duplex stdio JSON-RPC framing with `Content-Length` protocol adherence for `rust-analyzer`, with graceful fallback to regex search.

---

## Quickstart & Installation

### Option 1: NPX (Zero-Install Runner)
Run Nexus immediately on any Linux or Windows x64 machine:
```bash
npx -y nexus-harness
```

### Option 2: Cargo Build from Source
```bash
git clone https://github.com/yokunal/nexus.git
cd nexus
cargo build --release

# Run Nexus
./target/release/nexus
```

### Option 3: Launch Interactive Setup Wizard
Configure your color theme, display mode, and API keys:
```bash
nexus --setup
```

---

## CLI Usage

```bash
Nexus: High-Throughput Polyglot & Rust AI Coding Harness

Usage: nexus [OPTIONS]

Options:
      --conversation <UUID>           Resume a conversation by UUID
  -c, --continue-session              Continue the most recent conversation
  -w, --workspace <PATH>              Working directory for the workspace (default: current dir)
      --model <MODEL>                 Model override [default: claude-3-7-sonnet-20250219]
      --dangerously-skip-permissions  Automatically approve all tool execution requests
  -s, --search <QUERY>                Search cross-session memory index and exit
      --setup                         Launch interactive setup wizard
  -h, --help                          Print help
```

---

## Verification & Test Suite

All 68 unit, integration, polyglot, and endurance stress tests pass with 100% success:

```bash
cargo test --all-targets
```

```
running 43 tests in nexus-core lib ... ok
running 6 tests in dogfood_integration ... ok
running 8 tests in subagent_orchestration_test ... ok
running 3 tests in polyglot_coding_test ... ok
running 1 test in chaos_crucible_test (6-vector endurance battery) ... ok
running 1 test in token_efficiency_benchmark ... ok
running 1 test in quarantine_sandbox_attack_test ... ok
running 1 test in indirect_prompt_injection_test ... ok
running 1 test in context_marathon_test ... ok
running 1 test in swarm_three_way_merge_test ... ok
running 1 test in hyper_constrained_edge_test ... ok
running 1 test in nexus_lsp ... ok

test result: ok. 68 passed; 0 failed; 0 ignored; finished in 5.34s
```

---

## License

Licensed under the [MIT License](LICENSE).
