# Nexus: Architecture, Design, and Internal Specifications

> **Version:** 2.0.0  
> **Repository:** Private Local & Remote Git Workspace (`/home/ubuntu/projects/nexus`)  
> **Classification:** Elite Autonomous AI Coding Harness in Native Rust  

---

## 1. Executive Summary

**Nexus** is an elite, production-grade autonomous AI coding harness built from the ground up in native Rust. Designed for ultra-low latency, strict compile-time safety, modular deep architecture, and uncompromising persistence guarantees, Nexus surpasses traditional AI coding environments (such as Claude Code or custom LLM wrappers) by combining native multi-agent orchestration, structured append-only write-ahead logging (WAL), multi-backend storage engines, and deterministic code inspection/verification loops.

This document outlines every minute detail of Nexus's architecture, design invariants, subsystem internals, and operational workflows.

---

## 2. Core Architectural Philosophy & Invariants

Nexus is built on four immutable architectural pillars enforced by the compiler and test harnesses:

1. **Interface-First Contract:** All subsystems (storage, agent orchestration, tool execution, memory retrieval) define explicit Rust traits (`trait`) and public types before concrete implementations are written.
2. **Explicit Error Domains:** Zero `.unwrap()` or `.expect()` calls in non-test production code. All errors are channeled through domain-specific `thiserror` enums with rich context.
3. **Modularity Invariant:** Deep modules over shallow wrappers. Clear encapsulation boundaries prevent cognitive leakage between agents, storage layers, and CLI harnesses.
4. **Persistence & Storage Architecture Invariant:** Never implement naive full-file rewrite or truncation persistence (`truncate(true)`). All storage engines guarantee:
   - Atomic sequential append-only writes with `fsync` flushing.
   - Crash-recovery sequential replay on startup that reconstructs in-memory state.
   - Background or explicit log compaction to prune obsolete tombstones and expired entries.

---

## 3. High-Level System Architecture

```
+---------------------------------------------------------------------------------+
|                                 NEXUS CLI & TUI                                 |
+---------------------------------------------------------------------------------+
       |                  |                    |                  |
       v                  v                    v                  v
+--------------+  +---------------+  +------------------+  +----------------------+
| Subagent     |  | Tool Dispatch |  | Storage Engine   |  | Memory & Vector RAG  |
| Orchestrator |  | (Cargo/Bash)  |  | (Append-Only WAL)|  | (BM25 & Embeddings)  |
+--------------+  +---------------+  +------------------+  +----------------------+
       |                  |                    |                  |
       +------------------+--------------------+------------------+
                          |
                          v
        +-----------------------------------+
        | Native Rust Core Workspace        |
        | (`crates/`, `src/`, Cargo 2021)   |
        +-----------------------------------+
```

---

## 4. Subsystem Deep Dives

### 4.1. Storage Engine & WAL (`crates/nexus-storage` or internal crates)
- **Append-Only Write-Ahead Log (WAL):** Records every key-value mutation, memory distillation, and session event sequentially as binary frames with CRC32 checksums.
- **Atomic Operations:** Uses buffered writer flushes followed by explicit `fsync` syscalls to guarantee durability across power failures.
- **Crash-Recovery Replay:** On startup, the engine scans the WAL from offset 0, verifying checksums and rebuilding the in-memory hash index and B-Tree structures.
- **Log Compaction:** Background threads merge log segments, discarding overwritten keys and tombstones, and generate compacted snapshot files.

### 4.2. Subagent Orchestration & Swarm (`crates/nexus-agent`)
- **Topologies:** Supports `single`, `parallel` (concurrent task execution across worker pools), and `chain` (sequential data pipeline where output $N$ feeds into $N+1$).
- **Specialized Roles:**
  - `scout`: Read-only reconnaissance and codebase mapping.
  - `planner`: Architecture roadmap generation.
  - `reviewer`: Code auditing, safety verification, and `cargo check` integration.
  - `worker`: Full implementation and surgical edits.

### 4.3. Code Inspection, AST & LSP (`nexus-lsp` / `cargo_check`)
- Integrates directly with `rust-analyzer` LSP protocols and structured `cargo check`, `cargo clippy`, and `cargo test` diagnostic parsers.
- Provides borrow-checker tutor advice and automated fix suggestions for common compilation errors (E0382, E0502, E0499, etc.).

### 4.4. Memory & RAG Subsystem
- **Cross-Session Distillation:** Allows persisting permanent insights into long-term memory.
- **BM25 Relevance Scoring:** Fast in-memory keyword search over historical conversation turns and distilled insights.

---

## 5. Comparison: Nexus vs. Claude Code

| Feature / Dimension | Claude Code (Standard CLI Wrapper) | Nexus Elite Rust Harness | Advantage / Improvement |
| :--- | :--- | :--- | :--- |
| **Implementation Language** | TypeScript / Node.js | Native Rust (Edition 2021) | 10x lower memory overhead, zero GC pauses, maximum CPU throughput. |
| **Storage & WAL Durability** | JSON files with truncate-and-rewrite | Append-Only WAL with `fsync` & crash replay | Zero data corruption risk on sudden termination or power loss. |
| **Error Handling** | Exception throwing / undefined checks | Dedicated `thiserror` enums, zero `.unwrap()` | Robust compile-time guarantees and graceful failure modes. |
| **Multi-Agent Topologies** | Linear / prompt-chained wrappers | Native Single, Parallel, and Chain topologies | True concurrent subagent swarms with isolated context windows. |
| **Verification Loop** | External tool calls | Integrated LSP + Cargo Check + Test Harness | Instant feedback loop with automated borrow-checker tutoring. |

---

## 6. Git Repository & Version Control

This repository is initialized locally at `/home/ubuntu/projects/nexus` with a pristine commit history tracking all core crates, benchmarks, test reports (`NEXUS_2.0_68_TEST_BATTERY_REPORT.md`), and this architectural documentation.

---
*End of Document — Nexus Internal Specification v2.0*
