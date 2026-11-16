# Nexus Local Offline Benchmark Report

> Generated on `2026-11-16 01:12:14 UTC`
> Run Environment: **100% Local Offline (Zero Internet Exposure)**

## Summary Scorecard

| Metric | Score | Status |
|---|---|---|
| **Total Test Cases** | **11** | Verified |
| **Passed Cases** | **11 / 11** | ✅ 100% Green |
| **Pass Rate** | **100.0%** | Target: 100% |
| **Average Task Latency** | **0.02 ms** | Sub-millisecond Execution |
| **Resident Memory** | **~3.9 MB** | Featherweight Native Rust |

---

## Detailed Test Breakdown

| Category | Benchmark Name | Latency | Status | Details |
|---|---|---|---|---|
| `Patcher` | Tier 1: Exact Match Patching | `0.05 ms` | ✅ PASS | Patch verified cleanly |
| `Patcher` | Tier 2: Whitespace & Indentation Invariance | `0.07 ms` | ✅ PASS | Patch verified cleanly |
| `Patcher` | Tier 3: Multi-line Anchor Context Patching | `0.04 ms` | ✅ PASS | Patch verified cleanly |
| `Patcher` | Ambiguity Guard: Collision Prevention | `0.00 ms` | ✅ PASS | Ambiguity Guard intercepted 2 duplicate occurrences |
| `RustSpecialist` | Rust Doctor: Borrow of Moved Value (E0382) | `0.00 ms` | ✅ PASS | Generated compiler repair advice for E0382 |
| `RustSpecialist` | Rust Doctor: Simultaneous Borrow Conflict (E0502) | `0.00 ms` | ✅ PASS | Generated compiler repair advice for E0502 |
| `Security` | Path Traversal Containment (../../etc/shadow) | `0.00 ms` | ✅ PASS | Containment boundary verified |
| `Security` | Destructive Blacklist (rm -rf /) | `0.00 ms` | ✅ PASS | Containment boundary verified |
| `Security` | Forkbomb Interception (:(){ :|:& };:) | `0.00 ms` | ✅ PASS | Containment boundary verified |
| `Security` | Secret Scrubbing (GITHUB_TOKEN, SSH_AUTH_SOCK) | `0.00 ms` | ✅ PASS | Containment boundary verified |
| `Memory` | SQLite WAL BM25 Recall Speed | `0.05 ms` | ✅ PASS | Sub-millisecond query latency verified (<100 µs) |

---

## Key Invariant Verifications
1. **Tier 1 Exact Match**: Instant byte-for-byte replacement.
2. **Tier 2 Whitespace Normalization**: Immune to tab vs space indentation differences.
3. **Tier 3 Multi-line Anchor Context**: Preserves enclosing function framing without overwriting whole files.
4. **Ambiguity Guard**: Strictly rejects ambiguous edits with >1 occurrences to protect user source trees from silent corruption.
5. **Rust Borrow Tutor**: Explains AST diagnostic codes (`E0382`, `E0502`) with actionable repair instructions.
6. **Air-Gapped Privacy**: 0 bytes transmitted outside the local machine during this benchmark run.
