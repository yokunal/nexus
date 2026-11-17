# Product Requirements Document (PRD): Distributed Service Health Sentinel

## 1. Executive Summary
The Health Sentinel is a lightweight, low-footprint background health probing subsystem designed to monitor microservices and micro-endpoints. It periodically probes registered target endpoints (HTTP, TCP, gRPC), computes statistical latency distributions (P50, P90, P99), records failures in an embedded SQLite circular ring buffer, and triggers alert callbacks when SLAs are breached.

## 2. Target Personas & Core Use Case
- **Site Reliability Engineers (SREs)**: Need sub-millisecond alerting on endpoint degradation without deploying heavyweight agent daemons (Datadog/Prometheus).
- **Embedded / Edge Developers**: Require a monitoring agent that consumes less than 5MB of RAM and halts writes if disk headroom falls below 500MB.

## 3. Functional Requirements
- **FR-1: Multi-Protocol Probing**: Probe HTTP (GET/POST with status code assertions), raw TCP handshake latency, and gRPC health check service.
- **FR-2: Circular Ring Buffer Storage**: Store probe outcomes in SQLite (`probe_history` table) with an automatic 7-day or 100,000-row circular eviction limit to prevent unbounded disk growth.
- **FR-3: Dynamic Adaptive Anomaly Detection**: Trigger a degradation alert if the moving average latency exceeds `Mean + 3 * StdDev` over a sliding window of 50 samples.
- **FR-4: Outage Circuit Breaker**: If 3 consecutive probes fail, transition endpoint status from `Healthy` to `Degraded`, and after 5 to `Outage`. Suppress duplicate alert floods during prolonged outages.
- **FR-5: REST Diagnostic API**: Expose a local `/healthz` and `/metrics/sentinel` endpoint returning Prometheus-compatible text metrics and JSON status summary.

## 4. Non-Functional Invariants
- **NFR-1 (Memory Ceiling)**: Total resident memory must stay under 8MB at all times.
- **NFR-2 (Storage Safety Guardrail)**: Automatically freeze disk writes if available filesystem space drops below 500MB.
- **NFR-3 (CPU Efficiency)**: Idle probe loop must consume < 0.1% CPU when no active probes are scheduled.
- **NFR-4 (Crash Resilience)**: SQLite must operate in WAL mode with `PRAGMA synchronous = NORMAL` and recovery on process restart.

## 5. Critical Edge Cases & Failure Domains
1. **Slow DNS / Network Hanging**: Endpoints taking longer than 2000ms to resolve must be timed out with `ProbeError::Timeout` and not block concurrent probes.
2. **Flapping Endpoints**: Rapid oscillation between UP and DOWN must not flood alerts (hysteresis window of at least 3 samples required).
3. **Corrupted WAL / Abrupt Power Cut**: System must auto-recover SQLite database without human intervention.
