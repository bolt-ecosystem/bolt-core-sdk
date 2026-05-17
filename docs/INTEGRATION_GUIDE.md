# Bolt Ecosystem Integration Guide

External adopters: how to build a Bolt-conformant application.

**Audience:** Developers building apps outside the core Bolt ecosystem who need to
interoperate with LocalBolt, bolt-daemon, or other Bolt-protocol peers.

**Primary authority:**
- Ecosystem interop contract: `docs/SESSION_CONTRACT.md` + `rust/bolt-app-core/contracts/`
- Transport requirements: `docs/TRANSPORT_CONTRACT.md`

**Lower-level protocol detail:**
- Wire formats and protocol conformance: `bolt-protocol/PROTOCOL.md`
- Crypto primitives and golden vectors: `rust/bolt-core/` + `rust/bolt-core/test-vectors/`
- SDK authority model: `docs/SDK_AUTHORITY.md`

---

## 1. What Bolt Canonical Authority Is

### 1.1 Session Lifecycle

The ecosystem interoperability contract (`docs/SESSION_CONTRACT.md`) defines
five session phases and nine legal transitions:

```
idle ──────────────► requesting         (user selects peer)
idle ──────────────► incoming_request   (signal received from remote)
requesting ────────► connecting         (remote accepted)
requesting ────────► idle               (remote declined or timeout)
incoming_request ──► connecting         (local user accepted)
incoming_request ──► idle               (local user declined)
connecting ────────► connected          (handshake complete)
connecting ────────► idle               (handshake failed)
connected ─────────► idle               (disconnect, error, or remote close)
```

All other session phase transitions are **illegal**. Adopters MUST reject them.

The Rust validators in `rust/bolt-app-core/src/contracts/session_contract.rs`
are the executable authority. `is_valid_session_transition(from, to)` enforces
the 9 legal pairs and rejects the 16 illegal pairs (full N×N matrix coverage).

**Wire-level detail:** `PROTOCOL.md` §9 defines a more granular 7-state peer
connection model (IDLE → REQUEST_SENT → APPROVED → TRANSPORT_CONNECTING →
HANDSHAKING → CONNECTED → DISCONNECTED). The contract phases map onto these
states — see `SESSION_CONTRACT.md` § Layer Relationship for the mapping.

**Wire-level detail:** `rust/bolt-core/src/session.rs` provides the lower-level
handshake lifecycle (`SessionState`: PreHello → PostHello → Closed), `HelloState`
(exactly-once HELLO guard), and `SessionContext` (post-handshake state container).

### 1.2 Transfer Lifecycle

The ecosystem contract defines five transfer phases and ten legal transitions:

```
idle ──────► sending    (user initiates send)
idle ──────► receiving  (remote initiates send)
sending ───► complete   (all chunks acknowledged)
sending ───► failed     (error or session lost)
receiving ─► complete   (all chunks received and saved)
receiving ─► failed     (error or session lost)
complete ──► idle       (user dismisses or session disconnect)
failed ────► idle       (user dismisses or session disconnect)
sending ───► idle       (session disconnect — mandatory cleanup)
receiving ─► idle       (session disconnect — mandatory cleanup)
```

`is_valid_transfer_transition(from, to)` enforces 10 legal pairs and rejects
15 illegal pairs.

**Wire-level detail:** `PROTOCOL.md` §9 defines a more granular 8-state
transfer model (Idle, Offered, Accepted, Transferring, Paused, Completed,
Cancelled, Error). The contract collapses these into direction-aware phases —
see `SESSION_CONTRACT.md` § Layer Relationship. The wire-level `TransferState`
in `rust/bolt-transfer-core/src/state.rs` and `SendSession`/`ReceiveSession`
enforce the granular model for implementations that need it.

### 1.3 Verification and Transfer Gating

The ecosystem contract defines three verification states:

| State | Meaning | Transfer allowed? |
|-------|---------|-------------------|
| `unverified` | SAS received, pending user confirmation | NO |
| `verified` | User confirmed SAS match | YES |
| `legacy` | Peer lacks identity support (no SAS) | YES |

Transfer is gated by policy P1:

```
transfer_allowed = (session_phase == connected) AND
                   (verification_state == verified OR verification_state == legacy)
```

`is_transfer_allowed(connected, verification)` enforces this policy.

SAS computation (PROTOCOL.md §3):

```
SAS_input = SHA-256( sort32(identity_A, identity_B) || sort32(ephemeral_A, ephemeral_B) )
SAS = uppercase(hex(SAS_input[0..3]))
```

6 characters, uppercase hex, 24 bits entropy, symmetric, binds identity + ephemeral keys.

### 1.4 Parity Model

**Ecosystem interop:** The Rust validators in `bolt-app-core/contracts/`
are the executable authority for session/transfer/verification state.
The parity fixture (`contracts/parity_fixture.json`) provides a
machine-consumable test fixture for cross-product conformance checks.

**Wire-level crypto:** The Rust crates (`rust/bolt-core/`, `rust/bolt-btr/`,
`rust/bolt-transfer-core/`) are the reference crypto implementation.
Golden test vectors in `rust/bolt-core/test-vectors/` verify parity:

| Suite | Location | Purpose |
|-------|----------|---------|
| Box payload | `test-vectors/core/box-payload.vectors.json` | seal/open correctness |
| Framing | `test-vectors/core/framing.vectors.json` | nonce‖ciphertext layout |
| SAS | `test-vectors/core/sas.vectors.json` | SAS computation |
| HELLO-open | `test-vectors/core/web-hello-open.vectors.json` | HELLO envelope |
| Envelope-open | `test-vectors/core/envelope-open.vectors.json` | Generic envelope |
| BTR (10 files) | `test-vectors/btr/*.vectors.json` | BTR ratchet + key derivation |

---

## 2. What Adopters Must Implement

### 2.1 Session State Machine

Model the five session phases from the ecosystem contract. You do not need
to use the canonical names internally, but your state machine must represent
the same semantics and enforce only the nine legal transitions:

| Canonical | Meaning | Example internal name |
|-----------|---------|----------------------|
| `idle` | No session activity | `disconnected`, `ready` |
| `requesting` | Outbound request pending | `connecting_outbound` |
| `incoming_request` | Inbound request pending | `pairing_pending` |
| `connecting` | Handshake in progress | `handshaking` |
| `connected` | Session active, transfer-ready | `active`, `paired` |

Use `is_valid_session_transition(from, to)` from `bolt-app-core::contracts`
to validate transitions (Rust consumers), or validate against the
`parity_fixture.json` transition pairs (non-Rust consumers).

### 2.2 Transfer State Machine

Model the five transfer phases. Enforce the ten legal transitions.

Use `is_valid_transfer_transition(from, to)` from `bolt-app-core::contracts`,
or validate against `parity_fixture.json`.

Key wire-protocol conformance requirements (PROTOCOL.md §13):
- MUST use `transfer_id` to identify transfers
- MUST reject duplicate `chunk_index` (scoped per `(transfer_id, chunk_index)`)
- MUST reject `chunk_index >= total_chunks`
- MUST complete handshake before sending transfer messages
- MUST verify `file_hash` after reassembly when `bolt.file-hash` negotiated

Session disconnect MUST reset transfer phase to `idle` (INV-1).

### 2.3 Handshake Requirements

PROTOCOL.md §15 defines handshake invariants. Key requirements for adopters:

- **Exactly one HELLO per connection** — enforced by `HelloState` in `session.rs`
- **Ephemeral-first keying** — fresh ephemeral keypair per connection; MUST NOT rotate mid-session
- **Identity binding** — SAS binds identity AND ephemeral keys
- **Handshake gating** — reject protected messages before handshake completion with `ERROR(INVALID_STATE)`
- **Post-handshake envelope** — all non-PING/PONG messages MUST be sent inside encrypted envelopes after HELLO

### 2.4 Transport Requirements

Your transport MUST provide:

- **Ordered delivery** — messages arrive in the order sent
- **Reliable delivery** — no silent drops; failures are reported
- **Message framing** — one sealed payload = one transport message (no fragmentation, no coalescing)

See `docs/TRANSPORT_CONTRACT.md §1` for the full requirement set.

Each sealed payload is a NaCl box output:

```
sealed_payload = base64( nonce(24 bytes) || ciphertext(plaintext_len + 16 bytes) )
```

Base64 encoding is **RFC 4648 standard** (not URL-safe). `+` and `/` are used,
not `-` and `_`. Padding with `=` is required.

Use the transport path that matches your deployment (see §4).

### 2.5 Crypto Primitives

Adopters MUST NOT reimplement the envelope, handshake, or SAS logic.
Use the canonical crates:

**Rust consumers:**
```toml
[dependencies]
bolt-core          = { path = "../bolt-core-sdk/rust/bolt-core" }          # crypto, identity, SAS
bolt-btr           = { path = "../bolt-core-sdk/rust/bolt-btr" }            # BTR ratchet (if needed)
bolt-transfer-core = { path = "../bolt-core-sdk/rust/bolt-transfer-core" }  # transfer SM
```

**Browser consumers:**
The `@the9ines/bolt-core` npm package provides equivalent primitives for
browser environments. It is an adapter that passes the same golden vectors
as the Rust implementation.

See `docs/BOUNDARY_CONTRACT.md` for the full consumer boundary specification
(Rust-direct, WASM, and Tauri IPC patterns).

### 2.6 Signaling

Signaling is handled by `bolt-rendezvous`. It is **untrusted by design** —
it relays opaque signaling payloads and provides presence notifications.
It does not see file contents or encryption keys.

Adopters may:
- Use a hosted `bolt-rendezvous` endpoint
- Run their own `bolt-rendezvous` instance
- Use a different signaling mechanism that delivers equivalent information
  (connection request/accept/decline, peer identification)

The signaling channel is not part of the Bolt security model. All sensitive
data is protected by the protocol's encryption layer regardless of signaling
channel security.

---

## 3. What Adopters Do NOT Need to Implement

| What | Why not required |
|------|-----------------|
| Product-specific UI (localbolt UX, bolt-ui shell) | Contract is behavioral; UI is product-specific |
| Same runtime architecture (Tauri, bolt-daemon sidecar, egui, SwiftUI) | Architecture choices are product-specific |
| bolt-daemon itself | Required only for native apps needing the daemon-centered transport path |
| WebRTC transport | Required only for browser↔browser peers |
| Native QUIC/quinn | Required only for native↔native path |
| WebTransport | Required only for HTTPS web↔native path |
| Specific discovery mechanism (mDNS, cloud signaling) | Signaling is pluggable |
| BTR ratchet | Only required for sessions negotiating `bolt.transfer-ratchet-v1` |

You MUST NOT reimplement any crypto primitive (NaCl box, SAS computation, BTR
key derivation) from scratch. Use the canonical crates or `@the9ines/bolt-core`.

---

## 4. Transport Matrix

### Supported Paths

| Endpoint Pair | Transport | Status |
|--------------|-----------|--------|
| native↔native (app↔app) | QUIC via quinn (Rust) | Production |
| native↔native (fallback) | WebSocket client mode | Fallback |
| browser↔browser | WebRTC DataChannel | Production (G1 invariant — immutable) |
| HTTPS web↔native | WebTransport (HTTP/3) | Production |
| HTTP/localhost web↔native | WebSocket-direct | Supported (dev/LAN only) |

**G1 invariant:** browser↔browser always uses WebRTC. This is not negotiable.

**Native↔native (QUIC)** is the production app-to-app path for native builds
that enable the daemon `native-full` feature. Requirements:
- Rendezvous signaling supplies complete `quicAddr` and `quicCertHash`
  metadata
- Daemons use mutual certificate-hash pinning and fail closed on mismatch
- QUIC streams run the same HELLO, ProfileEnvelopeV1, BTR, pairing trust,
  transfer IPC, and disconnect lifecycle as WS
- WS client mode remains available as fallback when QUIC metadata is missing or
  QUIC connection setup fails

**HTTPS web↔native (WebTransport)** is the production path for browser-to-native
communication over HTTPS origins. Requirements:
- Daemon serves a WebTransport/HTTP3 endpoint with TLS
- Browser has WebTransport API support (Chrome, Edge, Firefox — not Safari)
- Capability negotiated: `bolt.transport-webtransport-v1`

**HTTP/localhost web↔native (WS-direct)** is for development and LAN testing
only. No TLS is required. This path applies when the browser origin is HTTP
(not HTTPS).

### Explicitly Unsupported

| Path | Why |
|------|-----|
| Native WebRTC (app↔app or app↔browser via WebRTC) | Native↔native uses QUIC with WS fallback. Web↔native uses WebTransport. WebRTC is browser↔browser only. |
| HTTPS web → plain ws:// native | Mixed content — browsers block ws:// from HTTPS origins. Impossible. |
| Raw QUIC from browsers | Browsers do not expose raw QUIC sockets; WebTransport is the browser QUIC surface |
| UDP datagrams without reliability layer | Violates transport contract §1 (ordered + reliable required) |
| webrtc-rs | Candidate only; must pass graduation criteria (`docs/ECOSYSTEM_STRATEGY.md §7`) |

---

## 5. Conformance Expectations

### 5.1 Ecosystem Contract Conformance (Primary)

All adopters MUST satisfy the ecosystem interoperability contract:

- Implement all five session phases, enforce nine legal transitions
- Implement all five transfer phases, enforce ten legal transitions
- Implement three verification states and transfer gating policy P1
- Enforce required invariants INV-1 (disconnect resets transfer), INV-2
  (transfer gating), INV-3 (single session)

Validate against the Rust validators in `bolt-app-core::contracts` or the
parity fixture (`contracts/parity_fixture.json`).

### 5.2 Wire Protocol Conformance

All adopters MUST also satisfy PROTOCOL.md §13 conformance requirements
(refer to §13 for the full list). Key requirements:

- Persistent device identity (TOFU)
- Fail-closed on key mismatch (`ERROR(KEY_MISMATCH)`)
- Fresh ephemeral keys per connection
- Envelope encryption for all protected messages
- Handshake completion before transfer messages
- `transfer_id` scoped replay protection
- File hash verification when `bolt.file-hash` negotiated

### 5.3 Golden Vector Compliance

Any implementation performing crypto operations MUST pass the vector suites:

1. Open all valid entries in `test-vectors/core/box-payload.vectors.json`
2. Reject all corrupt entries in the same file
3. Pass all framing assertions in `test-vectors/core/framing.vectors.json`
4. Produce correct SAS output for `test-vectors/core/sas.vectors.json` inputs

If you delegate crypto to the canonical crates or `@the9ines/bolt-core`,
vector compliance is handled by the library.

### 5.4 Document Your Conformance

Create a conformance mapping document (model:
`docs/conformance/LOCALBOLT_CONFORMANCE.md`) that traces each required behavior
to a test case. This is the evidence required for NONCORE-ADOPTER-1 acceptance.

---

## 6. Minimal Adopter Path

### Step 1: Add SDK dependencies

**Rust:**
```toml
[dependencies]
bolt-core     = { path = "../bolt-core-sdk/rust/bolt-core" }
bolt-app-core = { path = "../bolt-core-sdk/rust/bolt-app-core" }
```

**Browser (npm):**
```
npm install @the9ines/bolt-core
```

Non-Rust consumers: load `rust/bolt-app-core/contracts/parity_fixture.json`
for the machine-readable contract (session/transfer phases, legal transitions,
verification gating).

### Step 2: Implement the session state machine

Model the five session phases and enforce the nine legal transitions.

Rust consumers use the validators directly:
```rust
use bolt_app_core::contracts::session_contract::{
    SessionPhase, is_valid_session_transition,
};

if !is_valid_session_transition(current, next) {
    return Err(/* illegal transition */);
}
```

Non-Rust consumers validate against `parity_fixture.json`
`session_transitions_legal`.

### Step 3: Implement the transfer state machine

Model the five transfer phases and enforce the ten legal transitions.
Ensure session disconnect resets transfer to `idle` (INV-1).

### Step 4: Enforce transfer gating

```rust
use bolt_app_core::contracts::session_contract::{
    VerificationState, is_transfer_allowed,
};

if !is_transfer_allowed(is_connected, verification_state) {
    return Err(/* transfer not allowed */);
}
```

### Step 5: Choose a supported transport

Pick the transport matching your deployment from the matrix in §4.

### Step 6: Verify golden vectors

```bash
cd rust/bolt-core && cargo test
```

Non-Rust: validate against `rust/bolt-core/test-vectors/core/` JSON files.

### Step 7: Document conformance

Create a conformance mapping document (model:
`docs/conformance/LOCALBOLT_CONFORMANCE.md`) tracing required behaviors
to test cases.

---

## 7. Reference Documents

### Primary Ecosystem Authority

| Document | Location | Purpose |
|----------|----------|---------|
| **Session/transfer contract** | `docs/SESSION_CONTRACT.md` | Primary ecosystem interop authority (5+5 phases, transitions, invariants) |
| **Contract validators (Rust)** | `rust/bolt-app-core/src/contracts/session_contract.rs` | Executable authority (transition validators, gating policy) |
| **Contract instance (JSON)** | `rust/bolt-app-core/contracts/session_contract.v1.json` | Machine-readable contract |
| **Parity fixture** | `rust/bolt-app-core/contracts/parity_fixture.json` | Cross-product conformance test fixture |
| **Contract schema** | `rust/bolt-app-core/contracts/session_contract.schema.json` | JSON Schema for structural validation |
| Transport contract | `docs/TRANSPORT_CONTRACT.md` | Transport requirements (ordered, reliable, message-framed) |
| Conformance example | `docs/conformance/LOCALBOLT_CONFORMANCE.md` | Conformance mapping pattern |

### Wire-Level Protocol Detail

| Document | Location | Purpose |
|----------|----------|---------|
| Protocol spec | `bolt-protocol/PROTOCOL.md` | Wire formats, §9 state machines, §13 conformance |
| LocalBolt profile | `bolt-protocol/LOCALBOLT_PROFILE.md` | Profile-level requirements |
| Handshake types (Rust) | `rust/bolt-core/src/session.rs` | SessionState, HelloState, SessionContext |
| Transfer types (Rust) | `rust/bolt-transfer-core/src/state.rs` | TransferState (8-state wire model) |
| Golden vectors | `rust/bolt-core/test-vectors/` | 16 vector files (core + BTR) |
| SDK authority model | `docs/SDK_AUTHORITY.md` | Source of truth hierarchy |
| API surface (Rust) | `docs/API_SURFACE.md` | Unified Rust crate registry |
| Boundary contract | `docs/BOUNDARY_CONTRACT.md` | Consumer boundary types |
