# RigOS Enterprise Audit Architecture

## Purpose

This document defines the audit architecture used to evaluate RigOS against its PRD claims.
Every subsystem must be audited with the same hard standard:

1. PRD claim.
2. Code evidence.
3. Control mapping.
4. Threat model.
5. Adversarial tests.
6. Production readiness verdict.

No feature is considered enterprise-ready because it compiles. A feature is enterprise-ready only
when it has enforceable controls, durable evidence, negative tests, and operational observability.

## Source Standards

- NIST Cybersecurity Framework: governance and risk lifecycle.
- NIST SP 800-218 SSDF: secure software development practices.
- NIST SP 800-53 Rev. 5: control families for access control, audit, incident response, system integrity, and supply chain risk.
- NIST SP 800-53A Rev. 5: assessment procedures for validating controls.
- OWASP ASVS: application security verification requirements.
- Open Policy Agent decision logs: auditable policy decisions and offline debugging.
- OpenTelemetry logs, traces, and metrics: correlated telemetry using trace context and resource context.
- SLSA provenance: verifiable build provenance for software artifacts.
- Sigstore and in-toto attestations: signed artifact verification and policy-checked attestations.

## Audit Verdict Levels

- `Implemented`: production-grade behavior exists, is tested, and has audit evidence.
- `Partially Implemented`: the shape exists, but one or more enterprise controls are missing.
- `Scaffold`: interfaces or placeholders exist, but production behavior is not implemented.
- `Not Implemented`: no meaningful implementation exists.
- `Unsafe`: current behavior violates fail-closed, provenance, or isolation expectations.

## Universal Enterprise Gates

Every block must pass these gates:

1. Fail closed: missing policy, missing tool, missing identity, corrupt state, or invalid provenance must deny execution.
2. No silent fallback: security-critical parsing must not synthesize identities, traces, tools, policies, or permissions.
3. Complete audit event: every sensitive action must emit who, what, when, where, why, input hash, output hash, decision ID, trace ID, and policy version.
4. Deterministic replay: state transitions must be reconstructable without depending on mutable external state.
5. Least privilege: every tool and runtime capability must be explicitly allowed.
6. Bounded execution: every agent loop, tool call, LLM request, and sandbox execution must have time, step, memory, and fuel limits.
7. Tamper evidence: artifacts, skills, policies, WASM modules, and state transitions must carry verifiable hashes or signatures.
8. Negative tests: every allow path must have a deny test, malformed input test, replay test, and resource exhaustion test.
9. Observable failure: denials, policy failures, sandbox failures, and integrity failures must be structured logs with trace correlation.
10. Separation of duties: policy authoring, artifact publishing, runtime execution, and audit review must be separable.

## Block Audit Matrix

### 1. Agent Harness

Required controls:

- Max step count per run.
- Wall-clock timeout per run.
- Cancellation token.
- Token and tool-call budget.
- Human-in-the-loop pause state.
- Structured transition events.

Required adversarial tests:

- Agent never sets `is_finished`.
- Agent repeatedly requests dangerous tools.
- Agent mutates state into an invalid approval state.
- Agent exceeds step budget.
- Agent resumes from a paused HITL checkpoint without authorization.

Verdict rule:

- Without execution budgets, the harness is `Partially Implemented` at best.

### 2. LLM Connector Layer

Required controls:

- Provider abstraction for Ollama, OpenAI, Claude, and future providers.
- Request and response hashing.
- Prompt redaction policy.
- Model allowlist.
- Per-tenant model policy.
- Timeout and retry policy.
- Streaming trace spans.

Required adversarial tests:

- Unknown model.
- Provider timeout.
- Prompt contains secret-like data.
- Tool-call injection from model output.
- Oversized response stream.

Verdict rule:

- Hardcoded providers or missing provider abstraction are `Scaffold`.

### 3. MCP Router

Required controls:

- Tool registry.
- Unknown tools fail closed.
- OPA decision before execution.
- OPA decision ID persisted with the agent step.
- Tool input schema validation.
- Tool output schema validation.
- Tool artifact hash recorded.

Required adversarial tests:

- Unknown tool with allow policy.
- Known tool with deny policy.
- Missing policy engine.
- Malformed payload.
- Tool output violates schema.
- Replay a prior allowed decision with a modified payload.

Verdict rule:

- If unknown tools execute, the block is `Unsafe`.

### 4. Wasmtime Sandbox

Required controls:

- Fuel limit.
- Wall-clock timeout.
- Memory limiter.
- No ambient host filesystem access.
- No ambient network access.
- WASI capabilities disabled by default.
- Per-tool capability policy.
- Module hash/signature verification before instantiate.

Required adversarial tests:

- Infinite loop WASM.
- Memory exhaustion WASM.
- WASI filesystem attempt.
- WASI network attempt.
- Invalid module.
- Unsigned module.
- Valid signature but wrong tenant/tool binding.

Verdict rule:

- Fuel alone is not sufficient for enterprise sandbox readiness.

### 5. Skill Engine

Required controls:

- Source trace ID must be valid and existing.
- Skill checksum must cover procedure, trigger, source trace, tenant, and version.
- Skill extraction must fail on corrupt provenance.
- Skill routing must verify checksum before use.
- Semantic routing must be tested if claimed.
- Skills must be tenant-scoped.

Required adversarial tests:

- Corrupt source trace ID.
- Tampered checksum.
- Cross-tenant skill retrieval.
- Hash collision attempt by altered prompt.
- Skill procedure references unavailable tool.
- Skill says no approval needed for sensitive action.

Verdict rule:

- Synthetic fallback provenance makes the block `Partially Implemented` or `Unsafe`, depending on whether the skill can execute.

### 6. Time-Travel State Store

Required controls:

- Append-only writes.
- Trace row created before step row.
- Branches modeled without step-number collisions.
- Rewind and branch operations authorized by OPA.
- State payload schema versioning.
- Step hash chain or Merkle-style tamper evidence.
- Transactional write path.

Required adversarial tests:

- Insert step without trace.
- Duplicate branch step number.
- Rewind without authorization.
- Branch from nonexistent parent.
- Corrupt payload cannot deserialize.
- Attempt to update or delete historical step.

Verdict rule:

- Tables alone do not prove time-travel readiness.

### 7. Daemon HTTP/gRPC API

Required controls:

- Authn/authz for every non-health endpoint.
- Tenant resolution must not silently default in production.
- Request body limits.
- Idempotency keys for mutation endpoints.
- Structured API errors.
- gRPC service must bind a real Tonic service.
- Graceful shutdown.

Required adversarial tests:

- Missing tenant.
- Empty tenant.
- Cross-tenant trace access.
- Oversized request.
- Invalid UUID.
- Concurrent dispatch.
- gRPC startup and method invocation.

Verdict rule:

- Placeholder gRPC is `Scaffold`.

### 8. Observability and Audit Evidence

Required controls:

- OpenTelemetry trace context on every request.
- Correlated logs with trace ID and span ID.
- Audit event schema.
- OPA decision logs or equivalent persisted decision evidence.
- Sandbox execution metrics.
- Security denial metrics.
- Redaction of secrets in logs.

Required adversarial tests:

- Denial includes trace ID.
- Tool execution includes policy decision ID.
- Secret-like input is redacted.
- Error paths emit structured logs.

Verdict rule:

- Logging startup messages is not SOC2-ready observability.

### 9. Supply Chain and Build Integrity

Required controls:

- SBOM generation.
- Dependency vulnerability checks.
- License policy.
- SLSA provenance for release artifacts.
- Sigstore signatures for release artifacts and WASM tools.
- in-toto attestations for build and policy checks.
- Reproducible release process where feasible.

Required adversarial tests:

- Dependency with denied license.
- Vulnerable dependency.
- Unsigned release artifact.
- Artifact signature identity mismatch.
- Missing provenance.
- Provenance builder mismatch.

Verdict rule:

- A `Cargo.lock` is not supply-chain assurance.

### 10. Secrets and Vault/KMS

Required controls:

- No secrets in config files.
- Vault/KMS client abstraction.
- Secret material never persisted in state payload.
- Secret redaction in telemetry.
- Short-lived secret injection.
- Tenant-scoped secret access policy.

Required adversarial tests:

- Secret appears in payload.
- Secret appears in log.
- Cross-tenant secret request.
- Missing Vault/KMS policy.
- Expired token reuse.

Verdict rule:

- Vault in the PRD without runtime integration is `Not Implemented`.

## Audit Workflow Per PRD Claim

For every PRD claim, produce this record:

```text
Claim:
Evidence:
Missing Evidence:
Threats:
Controls:
Tests Added:
Tests Still Missing:
Verdict:
Fix Priority:
```

## Priority Order for RigOS

1. Fail-closed MCP tool registry.
2. Real OPA/OPAL client and decision evidence.
3. Agent execution budgets.
4. Skill provenance integrity.
5. Time-travel branch correctness and authorization.
6. Sandbox memory/resource limiter.
7. Real gRPC service.
8. OpenTelemetry audit schema.
9. Supply-chain attestations and artifact signatures.
10. Vault/KMS integration.

