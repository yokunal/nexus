# Product Requirements Document (PRD): Nexus 2.0
**Autonomous, Self-Correcting Micro-Engine with Plan-Act-Verify Loops, Dual-Model Failover, AST Semantic Safety Filters, and Circular WAL Persistence**

---

## 1. Executive Summary & Vision

Nexus 2.0 represents the next evolutionary milestone in autonomous software engineering harnesses. While Nexus 1.x established robust foundations in strict path containment, turn-level git snapshots, subagent orchestration, and declarative storage, Nexus 2.0 transitions the engine into a **self-correcting micro-engine**. 

It eliminates LLM hallucination cascades and brittle patch application through three core pillars:
1. **Rigorous Plan-Act-Verify (PAV) Loops**: Every user prompt is decomposed into structured plans, executed incrementally via atomic tool calls, and verified against compiler diagnostics, unit tests, and runtime invariants before proceeding.
2. **Dual-Model Intelligent Failover**: Zero single-point-of-failure LLM orchestration. Primary and fallback models seamlessly fail over on rate-limits, context overflows, or semantic generation failures without dropping state.
3. **AST Semantic Safety Filters**: Pre-execution syntax and type-checking filters combined with precise 3-tier patch matching that reject ambiguous modifications before disk writes occur.
4. **Circular Write-Ahead Log (WAL) Persistence**: A high-performance, bounded circular storage engine guaranteeing atomic sequential appends, crash recovery, and automated log compaction.

---

## 2. Core Architectural Invariants

Nexus 2.0 strictly upholds the declarative architecture invariants of the Nexus ecosystem:
- **Interface-First Contract**: All public modules, trait definitions, and error enums (`nexus-v2/src/errors.rs`, `traits.rs`) must be fully defined before concrete implementation bodies are written.
- **Explicit Error Domains**: Domain error enums using `thiserror` (`NexusV2Error`) with zero `.unwrap()` or `.expect()` calls in non-test production code paths.
- **Modularity Invariant**: Deep modules over shallow wrappers. Clear separation between orchestrator, PAV loop runner, model router, AST validator, and circular WAL storage.
- **Test Coverage Contract**: Every public trait and subsystem must include comprehensive unit and integration test suites covering happy paths, failure injections, and recovery modes.
- **Storage Persistence Invariant**: Circular WAL storage guarantees atomic sequential appends with `fsync`, crash recovery replay, and ring-buffer compaction without truncation vulnerabilities.

---

## 3. Subsystem Specifications

### 3.1 Plan-Act-Verify (PAV) Engine (`pav::*`)
- **Plan Phase**: Analyzes the user intent, inspects repository state via LSP/grep, and generates a structured directed acyclic graph (DAG) or sequential list of step items (`TaskStep`).
- **Act Phase**: Executes tool operations (file editing, shell execution, test runs) under strict sandbox containment and turn-level git snapshotting.
- **Verify Phase**: Automatically triggers `cargo_check`, test suites, or custom linters. If verification fails, the engine feeds compiler/test diagnostics back into the model prompt for autonomous self-correction (up to a max configured retry limit).

### 3.2 Dual-Model Failover Router (`router::*`)
- **Primary / Fallback Configuration**: Configurable primary LLM (e.g. Anthropic Claude 3.5 Sonnet / OpenAI GPT-4o) and secondary fallback LLM.
- **Health Tracking & Circuit Breaker**: Automatically tracks latency, HTTP 429 rate limits, context window limit errors, and schema generation failures.
- **Seamless State Preservation**: Failover occurs transparently mid-turn without losing conversation history, tool outputs, or step progress.

### 3.3 AST Semantic Safety Filters (`safety::*`)
- **Syntactic & Structural Validation**: Parses generated diffs/patches using robust syntax analyzers before disk writing.
- **Ambiguity Guard**: Rejects any patch where match count > 1 to prevent silent multi-location file corruption.
- **Injection & Escape Prevention**: Sanitizes all tool inputs against indirect prompt injection, path traversal (`../`), and forbidden system command patterns.

### 3.4 Circular Write-Ahead Log (WAL) Persistence (`storage::*`)
- **Ring-Buffer WAL Architecture**: Bounded-size circular log files with sequence numbers and CRC32 checksums.
- **Atomic Operations**: Appends state transitions atomically with synchronous `fsync`.
- **Crash Recovery**: On startup, replays valid WAL frames sequentially to reconstruct in-memory task states and agent sessions instantly.
- **Background Compactor**: Automatically prunes stale checkpoints and compacted frames when the ring buffer reaches capacity.

---

## 4. Module Structure for `crates/nexus-v2`

The target crate structure inside the Nexus workspace (`crates/nexus-v2`) is organized as follows:

```
crates/nexus-v2/
├── Cargo.toml
├── src/
│   ├── lib.rs                 # Crate root re-exporting public API
│   ├── errors.rs              # Dedicated thiserror domain error enums
│   ├── traits.rs              # Core async traits (Orchestrator, Model, Store, Safety)
│   ├── pav/
│   │   mod.rs                 # Plan-Act-Verify orchestrator loop
│   │   planner.rs             # Intent decomposition & DAG generation
│   │   actor.rs               # Tool execution & sandbox runner
│   │   verifier.rs            # Compiler & test verification runner
│   ├── router/
│   │   mod.rs                 # Dual-model router & circuit breaker
│   │   client.rs              # LLM client abstractions
│   │   failover.rs            # Failover policy & health metrics
│   ├── safety/
│   │   mod.rs                 # AST semantic safety & patch validator
│   │   patch_engine.rs        # 3-tier unambiguous patch matcher
│   │   sandbox.rs             # Strict path containment & quarantine
│   └── storage/
│       mod.rs                 # Storage engine module root
│       wal.rs                 # Circular write-ahead log & ring buffer
│       compactor.rs           # Background log compaction & snapshotting
└── tests/
    ├── pav_integration_test.rs    # End-to-end PAV loop tests
    ├── failover_test.rs           # Dual-model failover & circuit breaker tests
    ├── safety_ast_test.rs         # Ambiguity guard & AST filter tests
    └── circular_wal_test.rs       # Circular WAL crash recovery & append tests
```

---

## 5. API Contracts & Trait Definitions

To adhere to the Interface-First Contract, all core crates and modules implement the following foundational traits:

### 5.1 Core Traits (`nexus-v2/src/traits.rs`)

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::errors::NexusV2Error;

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn complete(&self, prompt: &str, context: &[Message]) -> Result<CompletionResponse, NexusV2Error>;
    async fn health_check(&self) -> Result<bool, NexusV2Error>;
}

#[async_trait]
pub trait OrchestratorEngine: Send + Sync {
    async fn execute_task(&mut self, goal: &str) -> Result<TaskResult, NexusV2Error>;
}

#[async_trait]
pub trait SafetyFilter: Send + Sync {
    fn validate_patch(&self, target_path: &Path, original: &str, patch: &str) -> Result<(), NexusV2Error>;
    fn validate_command(&self, cmd: &str) -> Result<(), NexusV2Error>;
}

#[async_trait]
pub trait CircularWalStore: Send + Sync {
    async fn append_frame(&mut self, event_type: u8, payload: &[u8]) -> Result<u64, NexusV2Error>;
    async fn recover(&mut self) -> Result<Vec<WalRecord>, NexusV2Error>;
    async fn compact(&mut self) -> Result<(), NexusV2Error>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub summary: String,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalRecord {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: u8,
    pub payload: Vec<u8>,
}
```

### 5.2 Error Domain (`nexus-v2/src/errors.rs`)

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NexusV2Error {
    #[error("Plan-Act-Verify loop failed after {max_iterations} iterations: {reason}")]
    PavLoopExhausted { max_iterations: u32, reason: String },

    #[error("Primary and fallback models both failed: primary={primary_err}, fallback={fallback_err}")]
    DualModelFailure { primary_err: String, fallback_err: String },

    #[error("AST Semantic Safety violation: {0}")]
    SafetyViolation(String),

    #[error("Ambiguous patch match detected (count={count}): target requires unique placement")]
    AmbiguousPatch { count: usize },

    #[error("Path containment violation: path {0} escapes workspace root")]
    PathEscape(String),

    #[error("Circular WAL storage error: {0}")]
    StorageError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

---

## 6. Verification and Test Plan

Nexus 2.0 requires 100% adherence to testing invariants before merging into production:
1. **PAV Loop Integration Test (`pav_integration_test.rs`)**: Simulates a deliberate bug injection, runs the PAV loop, and verifies that the engine plans a fix, applies the patch, runs cargo check/test, and successfully self-corrects.
2. **Failover Circuit Breaker Test (`failover_test.rs`)**: Mocks primary model failures (HTTP 429 / timeout) and verifies instantaneous failover to the secondary model with zero state loss.
3. **AST Safety Filter Test (`safety_ast_test.rs`)**: Feeds ambiguous patches (multiple identical context lines) and validates that the 3-tier patch engine immediately rejects them with an `AmbiguousPatch` error, preventing file corruption.
4. **Circular WAL Crash Recovery Test (`circular_wal_test.rs`)**: Writes sequence frames into the circular log, simulates abrupt crash termination, and verifies that sequential replay accurately restores the active task state.

---

## 7. Delivery Roadmap

- **Phase 1**: Crate scaffolding (`crates/nexus-v2`), `errors.rs`, and `traits.rs` definition.
- **Phase 2**: Implementation of Circular WAL storage and background compactor (`storage/*`).
- **Phase 3**: Implementation of Dual-Model router with circuit breaker (`router/*`).
- **Phase 4**: Implementation of AST safety filters and 3-tier unambiguous patch matcher (`safety/*`).
- **Phase 5**: Implementation of Plan-Act-Verify (PAV) loop engine (`pav/*`).
- **Phase 6**: Comprehensive integration test suites and binary CLI integration.
