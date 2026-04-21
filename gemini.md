# Gemini Enterprise Rules (Strict Mode 2.0)

## 1. Zero Technical Debt
* **No `TODO`/`FIXME` patches.** Implement the architecture properly via abstractions (`Traits`) if the full implementation is scheduled for later.
* **No `unwrap()` or `expect()`.** Always use proper error bubbling (`anyhow::Result`) or graceful defaults.
* **Explicit Resource Lifetimes:** Memory allocations and closures must not rely on panic! recovery.

## 2. Inversion of Control & Injections
* External layers (Database, LLMs, Tool SDKs) must not be hardcoded. Rely on dependency injection. If the `SkillRouter` needs an index database, it takes a `SkillStore` trait parameter, preventing dead "mock" code.

## 3. Total Observability & Security (SOC2 Ready & RBAC)
* Every state change must emit spans or structured logs via `tracing::info!`, `warn!`, or `error!`.
* **Zero Trust & Policy-as-Code (OPAL / OPA):** All critical agent actions (Tool invocation, Wasm instantiation, state rollbacks) MUST be wrapped in Authorization checkpoints. Hardcoded RBAC is banned. We assume an OPAL runtime is governing permissions.
* Security audits: Code must be structured to pass local checks for dependency vulnerabilities (`cargo-deny` / `cargo-audit` compliance).

## 4. English Codebase
* Spanish is for human collaboration only. 100% of the project's codebase, commit logs, variables, and docstrings must strictly remain in Enterprise-level English. (I will verify my strings before executing).

## 5. Safe Graph & Procedural Memory
* Time-travel implementations must remain purely append-only (Immutable).
* Procedural memory must maintain strict cryptographic SHA256 checksums to avoid tampering.
* **Wasmtime Guardrails:** Any agent-generated or imported tools must run in an isolated Wasm environment with strict fuel (instruction counting) limits. No native memory/disk access unless strictly permitted by an OPA policy.
