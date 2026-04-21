# Phase 8: Production Trust Boundary

This document outlines the formalized requirements to physically seal the RigOS node, transitioning the runtime from a structurally sound test environment to a zero-trust production deployment.

## Core Directives

The architecture must now enforce the following strict boundaries deterministically:

1. **WASM Expected Hash Validation (Highest Priority)**
   - The configuration must demand an explicit, pre-calculated cryptographic `SHA256` hash for every loaded tool.
   - The `ToolRegistry` must compute the binary hash of the `.wasm` file at boot.
   - If the read hash does not perfectly match the expected configuration manifest exactly, the Daemon must **abort startup immediately (Fail-Closed)**.
   - This completes the physical chain of trust natively, preventing injection of tampered modules without relying on external network infrastructure.

2. **gRPC mTLS (Mutual TLS) Enforcement**
   - Standard Server TLS is insufficient for the Trust Boundary.
   - The daemon must require a validated **Client Certificate** so that external actors cannot ping the gRPC port without authorized mTLS keys.

3. **OPA/OPAL E2E Test Harness**
   - Build a realistic local fixture or mock test integration validating the end-to-end daemon flow:
     - `allow` securely executes the registered Tool.
     - `deny` explicitly blocks execution.
     - `timeout` traps the execution gracefully.
     - Corrupted JSON structures trigger Fail-Closed aborts implicitly.
     - Unknown tools never query the `Wasmtime` Sandbox.

4. **Config Validation Gate**
   - Instantiation of the Runtime must execute explicit rule boundaries matching environment parity:
     - **Production Gate:** The server physically cannot boot without a verified OPA URL, full mTLS bounds, and Explicit SHA256 Tool Hashes. Bypasses are illegal.
     - Separates DEV from PROD forcefully. 

5. **Audit Evidence Storage**
   - Graph Memory (`AgentStep`) must now explicitly store minimum cryptographically sound evidence:
     - `Tool Name`
     - `Artifact SHA256 Hash`
     - `Policy Path Evaluated`
     - `Policy Result`
     - `Trace ID`
     - `Transport Origin`

## Execution Priority
As required by the Codex Audit: **Point 1 (WASM Hash Provenance)** is the immediate next step to execute, effectively locking the root execution trust chain natively before integrating further network topologies.
