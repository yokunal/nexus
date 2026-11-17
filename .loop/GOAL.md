# Goal: Nexus 2.0 Complete 68-Test Battery & Autonomous Hardening

Port, implement, and rigorously verify all 68 tests from the original Nexus into the next-generation `crates/nexus-v2` autonomous micro-engine. Execute the full test suite until 100% of all 68 tests pass green, maintaining strict compliance with the Nexus 2.0 PRD and architectural invariants (<8MB RAM, zero `.unwrap()` in production, zero disk bloat).

## Acceptance Criteria
1. `crates/nexus-v2` includes comprehensive test suites covering all 68 engineering scenarios across Safety, Circular WAL, Dual-Model Failover, PAV Loops, and Chaos Crucible.
2. `cargo test -p nexus-v2` executes all 68 tests with 0 failures and 0 compiler warnings.
3. Memory footprint during test execution remains strictly under 10MB RAM.
4. `scripts/verify.sh` exits 0 in full mode.
5. All atomic changes committed and pushed to `origin/main`.
6. Final morning report generated in `NEXUS_2.0_68_TEST_BATTERY_REPORT.md`.

## Constraints & Invariants
- Stack: Rust 1.85+ in workspace `crates/nexus-v2`.
- Zero `unwrap()` or `expect()` in non-test production code (`thiserror` only).
- Never allow `/dev/root` to drop below 500MB headroom (clean `target/debug` after heavy runs).
- Zero paid API spend during automated test runs (use deterministic mocks and verified free-tier models).
- Safety Tier: Tier 1 (Safe offline repository work).
- Overnight Budgets: 8 hours wall-clock, 50 iterations, 3 attempts per task.
