# AsmJit Readiness Plan — rust_cargo_shepherd

Status: PREPARED_ONLY
Branch: `prep/asmjit-readiness-2026-09-19`
Decision: DO NOT INTEGRATE.

The daemon coordinates Cargo builds, CPU/RAM pressure, shared locks, and rust-analyzer contention. Runtime machine-code generation is unrelated to the governing bottlenecks.

## Preferred optimization path
Scheduler policy, process telemetry, queue fairness, lock coordination, resource limits, cache behavior, and observability.

## Revisit trigger
None under current scope; only a new standalone runtime compiler subsystem would alter this decision.

No implementation, dependency addition, PR, or merge on this branch.
