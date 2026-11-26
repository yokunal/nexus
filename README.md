# Nexus 2.0: Native Autonomous AI Coding Cockpit

> **Version:** 2.0.0-PROD  
> **Repository:** https://github.com/yokunal/nexus  
> **Docs:** https://github.com/yokunal/nexus-docs  
> **Status:** Production-Ready / Enterprise Grade  

---

## About

**Nexus** is a fast, lightweight (~6.1 MB release binary), autonomous AI coding cockpit built in native Rust (Edition 2021). Built to rival and extend terminal coding assistants like Claude Code, Nexus combines deterministic compiler/LSP diagnostics, crash-safe append-only Write-Ahead Logging (WAL), zero-lock **Neural-FS** session memory with 7-character Git-style hashes (`#4419ae4`), parallel multi-agent swarms, and a modern **Tri-Fusion TUI**.

### Key Highlights
- **Tri-Fusion Terminal UI:** ASCII onboarding banner, animated `⟡ Nexus is thinking...` spinner, styled `<thinking>` reasoning blocks, silent background bash execution (collapsed one-line status cards), and instant **`Ctrl+O` / `/expand`** output toggling.
- **Neural-FS Zero-Lock Session Memory:** Append-only `.jsonl` transcripts and `manifest.tsv` index with deterministic 7-char Git-style hashes—resume any session instantly with `nexus -r <hash>` or `nexus -c`.
- **Autonomous Plan-Act-Verify (PAV) Engine:** Configurable step ceilings (`--max-steps`, default `50`), unattended `-a / --autonomous` mode, automatic in-flight context compaction, and stuck-loop circuit breakers.
- **Safety & Durability:** Workspace path sandboxing, destructive-command quarantine, CRC32-verified circular WAL persistence, and automatic per-turn Git snapshots.
- **Verified Quality:** Backed by the Nexus 2.0 Crucible Test Battery (75/75 test cases across 5 vectors in `crates/nexus-v2`, plus `nexus-core` unit and integration suites).

---

## Overview

Welcome to the definitive technical reference for **Nexus**, an elite autonomous AI coding harness engineered in native Rust. Designed from first principles to exceed the capabilities of standard AI assistants like Claude Code, Nexus provides deep multi-agent coordination, deterministic AST verification, append-only write-ahead logging (WAL), and zero-overhead memory retrieval.

This document serves as the comprehensive guide covering architecture, CLI usage, agent swarms, tool dispatch, persistence guarantees, configuration, and security models.

---

## Table of Contents

0. [About](#about)
1. [Introduction & Philosophy](#1-introduction--philosophy)
2. [Architecture & System Design](#2-architecture--system-design)
3. [Claude Code vs. Nexus: Feature Comparison Matrix](#3-claude-code-vs-nexus-feature-comparison-matrix)
4. [Installation & Quick Start](#4-installation--quick-start)
5. [Core Subsystems In-Depth](#5-core-subsystems-in-depth)
   - 5.1 Storage Engine & Append-Only WAL
   - 5.2 Subagent Orchestration & Swarm Topologies
   - 5.3 LSP, Cargo Check & Borrow-Checker Tutor
   - 5.4 Cross-Session Memory & BM25 Vector RAG
6. [CLI Commands & Interactive TUI](#6-cli-commands--interactive-tui)
7. [Configuration & Environment Variables](#7-configuration--environment-variables)
8. [Advanced Workflows & Crucible Test Batteries](#8-advanced-workflows--crucible-test-batteries)
9. [Security, Sandboxing & Invariants](#9-security-sandboxing--invariants)

---

## 1. Introduction & Philosophy

Traditional AI coding assistants operate as lightweight TypeScript or Python CLI wrappers around LLM APIs. While convenient, they often suffer from garbage collection pauses, naive file truncation persistence, fragile state management, and limited parallel subagent capabilities.

**Nexus** reimagines the developer harness as a high-performance, concurrent, and memory-safe Rust binary. By combining strict type-level invariants (`thiserror`, zero `.unwrap()` in production), atomic sequential WAL logging, and native multi-agent execution topologies, Nexus delivers a deterministic engineering experience for complex codebases.

---

## 2. Architecture & System Design

Nexus follows a modular, layered architecture adhering to the **Interface-First Contract** and **Deep Modules over Shallow Wrappers** principles.

```
+-----------------------------------------------------------------------------------+
|                              NEXUS CLI & TUI INTERFACE                            |
+-----------------------------------------------------------------------------------+
          |                     |                       |                     |
          v                     v                       v                     v
+------------------+  +------------------+    +-------------------+  +--------------+
| Subagent Swarm   |  | Tool Dispatcher  |    | Storage & WAL     |  | BM25 / RAG   |
| Orchestrator     |  | (Cargo/Bash/LSP) |    | (Atomic / Fsync)  |  | Memory Index |
+------------------+  +------------------+    +-------------------+  +--------------+
          |                     |                       |                     |
          +---------------------+-----------------------+---------------------+
                                |
                                v
                +---------------------------------+
                |      Native Rust Core Engine    |
                |   (`crates/`, Cargo Edition 2021)|
                +---------------------------------+
```

---

## 3. Claude Code vs. Nexus: Feature Comparison Matrix

| Dimension | Claude Code (Standard CLI) | Nexus 2.0 (Elite Rust Harness) | Engineering Advantage |
| :--- | :--- | :--- | :--- |
| **Implementation Stack** | TypeScript / Node.js | Native Rust (Edition 2021) | 10x lower memory footprint, zero runtime GC pauses, blazing-fast native compilation. |
| **State Persistence** | JSON files with `fs.writeFileSync` (truncate & rewrite) | Append-Only Write-Ahead Log (WAL) with CRC32 checksums & `fsync` | Zero risk of disk corruption or data loss during abrupt power failures or process crashes. |
| **Error Handling** | Exception throwing, undefined checks, runtime crashes | Strict `thiserror` enums, zero `.unwrap()` or `.expect()` in production | Compile-time error guarantees and graceful degradation across all failure modes. |
| **Subagent Execution** | Linear prompt pipelines or simple async tasks | Native `single`, `parallel`, and `chain` topologies with isolated context windows | True concurrent task execution across specialized worker pools (`scout`, `planner`, `reviewer`, `worker`). |
| **Code Verification** | Basic shell command invocation | Integrated LSP client (`rust-analyzer`) + Structured `cargo check` + Borrow-Checker Tutor | Instantaneous diagnostic feedback with automated compiler fix suggestions. |
| **Memory & Context** | Flat session history JSON files | BM25 relevance scoring, cross-session distillation, and persistent vector RAG | Superior long-term memory retrieval across extended autonomous coding sessions. |

---

## 4. Installation & Quick Start

### Prerequisites
- Rust 1.75+ (Edition 2021)
- Cargo build system
- Git

### Cloning & Building
```bash
git clone https://github.com/yokunal/nexus.git
cd nexus
cargo build --release
```

### Running Tests
Nexus includes a rigorous crucible test suite covering 68 distinct test vectors across safety, persistence, concurrency, and recovery:
```bash
cargo test --workspace
```

---

## 5. Core Subsystems In-Depth

### 5.1 Storage Engine & Append-Only WAL
Nexus prohibits naive file truncation (`truncate(true)`). The storage engine implements a bulletproof Write-Ahead Log:
1. **Sequential Append:** Every mutation (key-value write, memory distillation event, audit log) is appended sequentially as a binary frame to disk.
2. **Durability (`fsync`):** Buffered writes are immediately followed by explicit OS `fsync` syscalls to guarantee non-volatile persistence.
3. **Crash Recovery Replay:** During startup initialization, Nexus replays the WAL from offset 0, verifying CRC32 frame checksums and rebuilding in-memory B-Tree and hash indices.
4. **Compaction:** Background worker threads periodically compact historical WAL segments into immutable snapshot files while purging obsolete tombstones and expired records.

### 5.2 Subagent Orchestration & Swarm Topologies
Nexus provides advanced multi-agent execution workflows:
- **`scout` Agent:** Performs read-only reconnaissance, symbol searches, and dependency graphing across the codebase.
- **`planner` Agent:** Analyzes requirements and drafts rigorous architectural roadmaps and interface-first contracts.
- **`reviewer` Agent:** Audits code changes for security vulnerabilities, memory safety violations, and adherence to style invariants, invoking `cargo_check` automatically.
- **`worker` Agent:** Executes surgical edits, writes robust unit tests, and verifies correctness.

### 5.3 LSP, Cargo Check & Borrow-Checker Tutor
Unlike traditional LLMs that guess syntax fixes, Nexus hooks directly into compiler diagnostics:
- When a borrow-checker error (e.g., E0382 move semantics, E0502 shared vs mutable borrow) occurs, Nexus parses the diagnostic JSON.
- The **Borrow-Checker Tutor** analyzes lifetime bounds and mutability requirements, proposing precise lifetime annotations or reference refactorings.

### 5.4 Cross-Session Memory & BM25 Vector RAG
Nexus remembers important architectural decisions across distinct terminal sessions:
- **Distillation:** Important architectural insights are distilled into structured markdown memories.
- **BM25 Retrieval:** When a user queries past sessions, Nexus applies BM25 relevance scoring to surface exact context without overloading the LLM context window.

---

## 6. CLI Commands & Interactive TUI

Nexus provides a rich set of command-line flags and interactive TUI controls:

```bash
# Start an interactive coding session in the current workspace
nexus

# Continue the most recent session seamlessly
nexus -c

# Resume a specific session by 7-character hash (or UUID)
nexus -r 4419ae4

# Run in autonomous mode with an expanded 50-step ceiling and verbose tool output
nexus -a --max-steps 50 --expand

# Search cross-session memory and exit
nexus -s "write-ahead log"
```

### Interactive TUI Controls & Slash Commands
- **`Ctrl+O` / `/expand`** — Toggle between collapsed background shell execution and full stdout/stderr output
- **`/sessions` & `/resume <hash>`** — Browse recent sessions by 7-char hash and switch conversations live
- **`/doctor`**, **`/qa`**, **`/loop [goal]`**, **`/ship`** — Run diagnostics, automated QA loops, PAV execution, and pre-landing verification
- **`/compact [focus]`**, **`/context`**, **`/tokens`** — Inspect context window budget and compact session history
- **`/theme [name]`** — Switch terminal color palettes (`claude`, `cyberpunk`, `tokyo`, `amber`, `nordic`, `light`, `plain`)

---

## 7. Configuration & Environment Variables

Nexus is configured via `Settings.toml` and environment variables:

| Variable | Description | Default |
| :--- | :--- | :--- |
| `NEXUS_HOME` | Root configuration and storage directory | `~/.nexus` |
| `NEXUS_MODEL` | Default LLM model identifier | `claude-3-5-sonnet` |
| `NEXUS_LOG_LEVEL` | Logging verbosity (`debug`, `info`, `warn`, `error`) | `info` |
| `NEXUS_WAL_SYNC` | Enforce strict `fsync` on every WAL append | `true` |

---

## 8. Advanced Workflows & Crucible Test Batteries

Nexus undergoes continuous automated stress testing. The **NEXUS 2.0 Crucible Test Battery** verifies:
- **Concurrency Stress:** 1,000 parallel threads writing to the WAL simultaneously without data corruption.
- **Crash Recovery Simulation:** Simulated power cut mid-write, verifying 100% state restoration upon subsequent startup replay.
- **Token Efficiency:** Minimizing token consumption while maintaining deep code comprehension.

---

## 9. Security, Sandboxing & Invariants

- **Zero Arbitrary Execution:** Shell command execution is sandboxed and requires explicit confirmation in interactive mode.
- **Memory Safety:** Written entirely in Rust, eliminating buffer overflows, use-after-free bugs, and data races at compile time.
- **Immutable Audit Trails:** Every tool invocation and subagent state transition is immutably logged to the local WAL.

---

## License

Copyright © 2026 Kunal Jha (`yokunal`). **All Rights Reserved.**  
This repository contains proprietary software. See [LICENSE](./LICENSE) for full terms—no permission is granted to copy, modify, redistribute, or use this software without express written consent.

---

*Official Documentation — Nexus Elite Autonomous AI Coding Harness (v2.0)*
