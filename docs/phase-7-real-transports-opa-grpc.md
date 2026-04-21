# Phase 7: Real Remote Transports, Fail-Closed OPA, and Physical gRPC

## Objective

Replace the remaining scaffold integrations with real production-shaped transports:

1. Replace the local OPA placeholder with a configurable remote OPA/OPAL HTTP client.
2. Replace the gRPC pending stub with a real Tonic service generated from `.proto`.
3. Preserve the existing enterprise controls:
   - MCP must fail closed.
   - Tool execution must go through `ToolRegistry`.
   - OPA must authorize before sandbox execution.
   - Tool artifact SHA256 and policy evidence must be persisted in `AgentStep`.
   - No default allow behavior is permitted.

This phase does not claim full production enterprise readiness. The accepted target is:

> Real OPA transport plus real gRPC transport plus fail-closed behavior plus red-team tests passing.

Full production readiness still requires artifact signatures, schema validation, OpenTelemetry audit schema,
TLS or mTLS hardening, and supply-chain attestations.

## Non-Negotiable Security Requirements

### 1. OPA Must Fail Closed

The system must deny execution when:

- OPA URL is missing.
- OPA endpoint is unreachable.
- OPA request times out.
- OPA returns non-2xx.
- OPA returns malformed JSON.
- OPA response does not contain an explicit boolean allow or deny decision.
- OPA response contains unexpected structure.
- OPA bearer token is configured but rejected.

There must be no fallback to allow.

### 2. No Default Production Bypass

The daemon must not silently use an allow-all OPA client.

A development bypass may exist only if:

- It is explicitly enabled with an environment variable like `RIGOS_DEV_ALLOW_OPA_BYPASS=true`.
- It logs a `warn!`.
- It is clearly isolated from the production path.
- Tests prove the default posture is deny.

### 3. gRPC Must Use the Same Execution Path

gRPC must call the same `McpRouter::dispatch` path used by HTTP.

No separate execution path is allowed.

The gRPC flow must preserve:

- Tool registry resolution.
- OPA authorization.
- Wasmtime sandbox execution.
- State persistence.
- Artifact SHA256 evidence.
- Policy evidence.

### 4. No TLS Claims Without TLS Config

Do not claim TLS or mTLS is implemented unless the following exist:

- Server certificate config.
- Server key config.
- Optional CA config.
- Test or documented startup path.
- Clear behavior for invalid certificate config.

For Phase 7, plain TCP gRPC is acceptable. TLS or mTLS should be a later phase unless implemented fully.

## Proposed Implementation

### 1. Add Remote OPA Client

Create a real remote policy client in `rigos-mcp`, for example:

```text
crates/rigos-mcp/src/remote_opa.rs
```

It should implement:

```rust
#[async_trait::async_trait]
impl OpaPolicyClient for RemoteOpaClient {
    async fn evaluate(
        &self,
        policy_path: &str,
        input: &serde_json::Value,
    ) -> anyhow::Result<PolicyDecision>;
}
```

The client should use:

- `reqwest::Client`
- configurable base URL
- configurable timeout
- optional bearer token
- JSON request body
- strict JSON response parsing

Suggested environment and config keys:

```text
RIGOS_OPA_URL
RIGOS_OPA_BEARER_TOKEN
RIGOS_OPA_TIMEOUT_MS
RIGOS_DEV_ALLOW_OPA_BYPASS
```

Expected request body:

```json
{
  "policy_path": "mcp/tool/allow",
  "input": {
    "tool_name": "...",
    "payload": {}
  }
}
```

Accepted response shapes should be explicit and documented. Prefer one strict shape:

```json
{
  "allowed": true,
  "reason": "policy matched"
}
```

If OPAL or OPA returns native OPA format, normalize it deliberately:

```json
{
  "result": {
    "allowed": true,
    "reason": "policy matched"
  }
}
```

Do not accept ambiguous responses.

### 2. Wire OPA Into the Daemon

Replace the current local default OPA behavior in:

```text
crates/rigos-server/src/main.rs
```

Startup behavior:

- If `RIGOS_OPA_URL` exists, use `RemoteOpaClient`.
- If `RIGOS_OPA_URL` is missing:
  - default: deny all policy checks.
  - if `RIGOS_DEV_ALLOW_OPA_BYPASS=true`: allow only in explicit dev mode and log `warn!`.

Preferred architecture:

```rust
enum RuntimeOpaClient {
    Remote(RemoteOpaClient),
    DenyAll(DenyAllOpaClient),
    DevAllow(DevAllowOpaClient),
}
```

This keeps the runtime explicit and auditable.

### 3. Add Real gRPC Proto

Add a real proto file:

```text
crates/rigos-server/proto/rigos_agent.proto
```

Suggested schema:

```proto
syntax = "proto3";

package rigos.agent.v1;

service RigOsAgent {
  rpc DispatchMcp (DispatchMcpRequest) returns (DispatchMcpResponse);
  rpc Health (HealthRequest) returns (HealthResponse);
}

message HealthRequest {}

message HealthResponse {
  string status = 1;
}

message DispatchMcpRequest {
  string tenant_id = 1;
  string trace_id = 2;
  string tool_name = 3;
  string payload_json = 4;
}

message DispatchMcpResponse {
  string trace_id = 1;
  bool success = 2;
  string output_json = 3;
  string error_msg = 4;
}
```

Add:

```text
crates/rigos-server/build.rs
```

Use `tonic-build` to generate the service.

### 4. Replace gRPC Stub

Replace the pending stub in:

```text
crates/rigos-server/src/grpc.rs
```

With a real `tonic::transport::Server`.

Required behavior:

- Bind to configured gRPC address.
- Register generated `RigOsAgentServer`.
- Convert gRPC requests into `McpRequest`.
- Parse `payload_json` as JSON.
- Validate `trace_id`; if missing or invalid, generate or reject explicitly.
- Call `McpRouter::dispatch`.
- Return structured response.

Do not use:

```rust
std::future::pending()
```

Do not use local trait-only pseudo-gRPC as the final implementation.

### 5. Config Updates

Extend `RigOSConfig` with:

```rust
pub struct OpaConfig {
    pub url: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
    pub dev_allow_bypass: bool,
}

pub struct GrpcConfig {
    pub addr: String,
    pub enabled: bool,
}
```

Suggested defaults:

```text
opa.url = None
opa.timeout_ms = 1500
opa.dev_allow_bypass = false

grpc.addr = "0.0.0.0:50051"
grpc.enabled = true
```

Environment overrides:

```text
RIGOS_OPA_URL
RIGOS_OPA_BEARER_TOKEN
RIGOS_OPA_TIMEOUT_MS
RIGOS_DEV_ALLOW_OPA_BYPASS
RIGOS_GRPC_ADDR
RIGOS_GRPC_ENABLED
```

## Required Tests

### OPA Client Tests

Use a local mock HTTP server or a test service abstraction.

Required tests:

1. `remote_opa_allows_explicit_true`
2. `remote_opa_denies_explicit_false`
3. `remote_opa_fails_closed_on_timeout`
4. `remote_opa_fails_closed_on_non_2xx`
5. `remote_opa_fails_closed_on_malformed_json`
6. `remote_opa_fails_closed_on_missing_allowed_field`
7. `missing_opa_url_defaults_to_deny_all`
8. `dev_bypass_requires_explicit_env_or_config`

### MCP Integration Tests

Required tests:

1. Known tool plus OPA allow executes.
2. Known tool plus OPA deny does not execute.
3. Unknown tool plus OPA allow does not execute.
4. Known tool plus OPA error does not execute.
5. Successful execution persists:
   - tool name
   - artifact SHA256
   - policy path
   - policy decision
   - policy reason

### gRPC Tests

Required tests:

1. gRPC health returns OK.
2. gRPC dispatch with unknown tool fails closed.
3. gRPC dispatch with malformed payload JSON returns structured error.
4. gRPC dispatch calls same `McpRouter` path.
5. gRPC invalid trace ID is handled explicitly.

### Regression Tests

Existing tests must remain green:

```bash
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo test -p rigos-sandbox -- --nocapture
```

## Acceptance Criteria

Phase 7 is accepted only if:

- `cargo fmt --all --check` passes.
- `cargo check --workspace` passes.
- `cargo test --workspace` passes.
- No tests are ignored.
- No `unwrap()` or `expect()` exists in production code.
- No allow-all OPA client is used by default.
- Missing OPA config denies by default.
- Unknown MCP tools fail closed.
- Missing WASM paths abort startup.
- OPA network errors fail closed.
- gRPC no longer uses `pending()`.
- gRPC has real `.proto` generated code.
- HTTP and gRPC share the same `McpRouter` dispatch path.

## Out of Scope for Phase 7

Do not claim these as complete unless separately implemented:

- TLS or mTLS for gRPC.
- Sigstore or cosign verification.
- in-toto attestations.
- WASM artifact signatures.
- JSON schema validation for MCP payloads.
- Full OpenTelemetry audit schema.
- Real tenant RBAC model.
- Vault or KMS integration.

These should be Phase 8 or later.

## Final Codex Audit Rule

After implementation, Codex will audit with this question:

> Can a request reach Wasmtime without passing through ToolRegistry resolution and an explicit OPA decision?

The only acceptable answer is:

> No.

If any HTTP or gRPC path can bypass either control, Phase 7 fails.
