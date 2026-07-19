# Bolt Core SDK — Unified Rust API Surface

Canonical registry of the public Rust API consumed by downstream products
and infrastructure. This document is the AC-RC-05 deliverable.

Keywords: RFC 2119 (MUST, MUST NOT, REQUIRED, SHALL, SHOULD, MAY).

## Integration Policy

**Direct multi-crate dependency** (LOCKED, RC2-GOV 2026-03-13).

Consumers import specific crates directly. There is no facade or umbrella
crate. This matches the established pattern (bolt-daemon already uses this
model) and avoids coupling unrelated domains.

```toml
# Example consumer Cargo.toml
[dependencies]
bolt-core          = { path = "../bolt-core-sdk/rust/bolt-core" }
bolt-btr           = { path = "../bolt-core-sdk/rust/bolt-btr" }
bolt-transfer-core = { path = "../bolt-core-sdk/rust/bolt-transfer-core" }
```

## Crate Registry

| Crate | Version | Domain | Zero-dep | WASM-safe |
|-------|---------|--------|:--------:|:---------:|
| `bolt-core` | 0.4.0 | Crypto, identity, SAS, encoding, errors, constants | No (NaCl) | No |
| `bolt-btr` | 0.1.0 | BTR ratchet, key derivation, symmetric encryption | No (NaCl) | No |
| `bolt-transfer-core` | 0.1.0 | Transfer state machines, backpressure, policy | **Yes** | **Yes** |
| `bolt-transfer-policy-wasm` | 0.1.0 | WASM thin wrapper for policy | No (wasm-bindgen) | Target |

Dependency graph:
```
bolt-core (standalone)
    ↓
bolt-btr (depends on bolt-core)

bolt-transfer-core (standalone, zero deps)
    ↓
bolt-transfer-policy-wasm (depends on bolt-transfer-core + wasm-bindgen)
```

---

## bolt-core (v0.4.0) — Crypto Primitives and Protocol Constants

### constants

| Export | Type | Value |
|--------|------|-------|
| `NONCE_LENGTH` | `usize` | 24 |
| `PUBLIC_KEY_LENGTH` | `usize` | 32 |
| `SECRET_KEY_LENGTH` | `usize` | 32 |
| `DEFAULT_CHUNK_SIZE` | `usize` | 16384 |
| `PEER_CODE_LENGTH` | `usize` | 6 |
| `PEER_CODE_ALPHABET` | `&str` | `ABCDEFGHJKMNPQRSTUVWXYZ23456789` |
| `SAS_LENGTH` | `usize` | 6 |
| `BOX_OVERHEAD` | `usize` | 16 |
| `TRANSFER_ID_LENGTH` | `usize` | 16 |
| `SAS_ENTROPY` | `usize` | 24 |
| `FILE_HASH_ALGORITHM` | `&str` | `SHA-256` |
| `FILE_HASH_LENGTH` | `usize` | 32 |
| `BOLT_VERSION` | `usize` | 1 |
| `CAPABILITY_NAMESPACE` | `&str` | `bolt.` |

### errors

| Export | Kind | Notes |
|--------|------|-------|
| `BoltError` | enum | Variants: `Encryption`, `Connection`, `Transfer`, `Integrity`, `Encoding` |
| `WIRE_ERROR_CODES` | `[&str; 26]` | Canonical 26-code registry (11 PROTOCOL + 11 ENFORCEMENT + 4 BTR) |
| `is_valid_wire_error_code(code: &str) -> bool` | fn | Registry lookup |

### encoding

| Export | Signature |
|--------|-----------|
| `to_base64(data: &[u8]) -> String` | Encode bytes to base64 |
| `from_base64(encoded: &str) -> Result<Vec<u8>, BoltError>` | Decode base64 |
| `to_hex(data: &[u8]) -> String` | Encode bytes to hex |
| `from_hex(encoded: &str) -> Result<Vec<u8>, BoltError>` | Decode hex |

### crypto

| Export | Kind | Notes |
|--------|------|-------|
| `KeyPair` | struct | Fields: `public_key: [u8; 32]`, `secret_key: [u8; 32]`. Zeroize-on-drop. No Clone. |
| `generate_ephemeral_keypair() -> KeyPair` | fn | Fresh X25519 keypair |
| `seal_box_payload(plaintext, remote_pk, sender_sk) -> Result<String, BoltError>` | fn | Returns base64(nonce ‖ ciphertext) |
| `open_box_payload(sealed, sender_pk, receiver_sk) -> Result<Vec<u8>, BoltError>` | fn | Decrypts sealed box |

### hash

| Export | Signature |
|--------|-----------|
| `sha256(data: &[u8]) -> [u8; 32]` | Raw SHA-256 digest |
| `sha256_hex(data: &[u8]) -> String` | Hex-encoded SHA-256 |
| `buffer_to_hex(data: &[u8]) -> String` | Generic hex encoder |

### identity

| Export | Kind | Notes |
|--------|------|-------|
| `IdentityKeyPair` | type alias | `= KeyPair` |
| `generate_identity_keypair() -> IdentityKeyPair` | fn | Long-lived identity key |
| `KeyMismatchError` | struct | Fields: `peer_code`, `expected`, `received`. Display + Error. |

### sas

| Export | Signature | Notes |
|--------|-----------|-------|
| `compute_sas(identity_a, identity_b, ephemeral_a, ephemeral_b) -> String` | fn | 6-char uppercase hex (24 bits). Canonical — no shadow SAS permitted. |

### peer_code

| Export | Signature |
|--------|-----------|
| `generate_secure_peer_code() -> String` | 6-char code |
| `generate_long_peer_code() -> String` | 8-char with dash (XXXX-XXXX) |
| `is_valid_peer_code(code: &str) -> bool` | Validation |
| `normalize_peer_code(code: &str) -> String` | Normalize to uppercase |

### vectors (feature-gated: `vectors`)

Test-only golden vector generator. Not part of the stable API surface.

---

## bolt-btr (v0.1.0) — Bolt Transfer Ratchet

### Root re-exports

```rust
pub use errors::BtrError;
pub use negotiate::{negotiate_btr, BtrMode};
pub use state::{BtrEngine, BtrTransferContext};
```

### constants

| Export | Type |
|--------|------|
| `BTR_SESSION_ROOT_INFO` | `&[u8]` |
| `BTR_TRANSFER_ROOT_INFO` | `&[u8]` |
| `BTR_MESSAGE_KEY_INFO` | `&[u8]` |
| `BTR_CHAIN_ADVANCE_INFO` | `&[u8]` |
| `BTR_DH_RATCHET_INFO` | `&[u8]` |
| `BTR_KEY_LENGTH` | `usize` (32) |
| `BTR_WIRE_ERROR_CODES` | `[&str; 4]` |

### errors

| Export | Kind | Notes |
|--------|------|-------|
| `BtrError` | enum | Variants: `RatchetStateError`, `RatchetChainError`, `RatchetDecryptFail`, `RatchetDowngradeRejected` |
| `BtrError::wire_code(&self) -> &'static str` | method | Maps to wire error code |
| `BtrError::requires_disconnect(&self) -> bool` | method | Severity routing |

### key_schedule

| Export | Signature |
|--------|-----------|
| `derive_session_root(ephemeral_shared_secret) -> [u8; 32]` | Session root from handshake |
| `derive_transfer_root(session_root_key, transfer_id) -> [u8; 32]` | Per-transfer root |
| `chain_advance(chain_key) -> ChainAdvanceOutput` | Symmetric chain step |
| `ChainAdvanceOutput` | struct (Zeroize+Drop): `message_key`, `next_chain_key` |

### ratchet

| Export | Kind | Notes |
|--------|------|-------|
| `RatchetKeypair` | struct | `public_key: [u8; 32]`, `secret: Option<EphemeralSecret>` |
| `RatchetKeypair::generate() -> Self` | fn | Fresh DH keypair |
| `RatchetKeypair::diffie_hellman(self, remote_pk) -> [u8; 32]` | fn | Consumes secret (move-only) |
| `derive_ratcheted_session_root(current_srk, dh_output) -> [u8; 32]` | fn | DH ratchet step |

### encrypt

| Export | Signature |
|--------|-----------|
| `btr_seal(message_key, plaintext) -> Result<Vec<u8>, BtrError>` | Returns nonce ‖ ciphertext |
| `btr_open(message_key, sealed) -> Result<Vec<u8>, BtrError>` | Decrypts sealed chunk |

### negotiate

| Export | Kind |
|--------|------|
| `BtrMode` | enum: `FullBtr`, `Downgrade`, `StaticEphemeral`, `Reject` |
| `negotiate_btr(local_supports, remote_supports, remote_well_formed) -> BtrMode` | fn |
| `btr_log_token(mode) -> Option<&'static str>` | fn |

### state

| Export | Kind | Notes |
|--------|------|-------|
| `BtrEngine` | struct | Session/transfer lifecycle manager |
| `BtrEngine::new(ephemeral_shared_secret) -> Self` | fn | Init from handshake |
| `BtrEngine::begin_transfer_send(transfer_id, remote_ratchet_pub) -> Result<(BtrTransferContext, [u8; 32]), BtrError>` | fn | Start send-side transfer |
| `BtrEngine::begin_transfer_receive(transfer_id, remote_ratchet_pub) -> Result<(BtrTransferContext, [u8; 32]), BtrError>` | fn | Start receive-side transfer |
| `BtrEngine::check_replay(transfer_id, generation, chain_index) -> Result<(), BtrError>` | fn | Replay guard |
| `BtrEngine::end_transfer(&mut self)` | fn | Cleanup |
| `BtrEngine::cleanup_disconnect(&mut self)` | fn | Session teardown |
| `BtrTransferContext` | struct (Zeroize+Drop) | Per-transfer chain state |
| `BtrTransferContext::seal_chunk(plaintext) -> Result<(u32, Vec<u8>), BtrError>` | fn | Encrypt chunk |
| `BtrTransferContext::open_chunk(chain_index, sealed) -> Result<Vec<u8>, BtrError>` | fn | Decrypt chunk |

### replay

| Export | Kind |
|--------|------|
| `ReplayGuard` | struct |
| Methods: `new()`, `begin_transfer()`, `check()`, `end_transfer()`, `reset()` | — |

---

## bolt-transfer-core (v0.1.0) — Transfer State Machine

**Zero dependencies.** Pure logic, WASM-compatible.

### Root re-exports

```rust
pub use backpressure::{BackpressureConfig, BackpressureController};
pub use error::TransferError;
pub use policy::{decide, Backpressure, ChunkId, DeviceClass, FairnessMode,
    LinkStats, PolicyInput, PressureState, ScheduleDecision, TransferConstraints};
pub use receive::ReceiveSession;
pub use send::{SendChunk, SendOffer, SendSession};
pub use state::{CancelReason, TransferState};
pub use transport::{IntegrityVerifier, TransportQuery};
```

### state

| Export | Kind |
|--------|------|
| `TransferState` | enum: `Idle`, `Offered`, `Accepted`, `Transferring`, `Paused`, `Completed`, `Cancelled`, `Error` |
| `CancelReason` | enum: `BySender`, `ByReceiver`, `Rejected` |

### error

| Export | Kind |
|--------|------|
| `TransferError` | enum: `InvalidTransition(String)`, `IntegrityFailed(String)` |

### send

| Export | Kind |
|--------|------|
| `SendSession` | struct — §9 send-side state machine |
| `SendOffer` | struct — offer parameters |
| `SendChunk` | struct — chunk payload container |
| `DEFAULT_CHUNK_SIZE` | `usize` (16384) |

### receive

| Export | Kind |
|--------|------|
| `ReceiveSession` | struct — §9 receive-side state machine |
| `MAX_TRANSFER_BYTES` | `u64` (256 MiB) |

### backpressure

| Export | Kind |
|--------|------|
| `BackpressureConfig` | struct: `high_watermark`, `low_watermark` |
| `BackpressureController` | struct — watermark-based flow control |

### transport

| Export | Kind |
|--------|------|
| `TransportQuery` | trait: `is_open()`, `buffered_bytes()`, `max_message_size()` |
| `IntegrityVerifier` | trait: `verify(data, expected_hash) -> bool` |

### policy

| Export | Kind |
|--------|------|
| `decide(input: &PolicyInput) -> ScheduleDecision` | fn — pure scheduling |
| `detect_stall(input: &StallInput) -> StallClassification` | fn |
| `progress_cadence(...) -> Option<ProgressReport>` | fn |
| `PolicyInput`, `ScheduleDecision`, `LinkStats`, `TransferConstraints` | structs |
| `DeviceClass`, `FairnessMode`, `PressureState`, `Backpressure`, `StallClassification` | enums |
| `ProgressConfig`, `ProgressReport`, `StallInput` | structs |

---

## bolt-transfer-policy-wasm (v0.1.0) — Browser WASM Boundary

Thin wrapper. All logic delegates to `bolt-transfer-core`. Boundary
serialization only.

### Exported functions (wasm-bindgen)

| JS Name | Rust Name | Purpose |
|---------|-----------|---------|
| `policyDecide` | `policy_decide` | Chunk scheduling decision |
| `policyDetectStall` | `policy_detect_stall` | Stall classification |
| `policyProgressCadence` | `policy_progress_cadence` | Progress emission gate |

### Exported types (wasm-bindgen)

| Type | Maps To |
|------|---------|
| `WasmDeviceClass` | `DeviceClass` |
| `WasmFairnessMode` | `FairnessMode` |
| `WasmPressureState` | `PressureState` |
| `WasmBackpressure` | `Backpressure` |
| `WasmScheduleDecision` | `ScheduleDecision` |
| `WasmStallResult` | `StallClassification` |
| `WasmProgressResult` | `ProgressReport` |

---

## Stability Classification

All exports in this document are part of the **operational API surface** —
they are consumed by at least one downstream product (bolt-daemon, localbolt-v3,
or localbolt-app via WASM).

Stability guarantees are governed by SemVer per crate:
- **Patch** (0.x.Y): Bug fixes, no API changes.
- **Minor** (0.X.0): Additive changes, no breaking changes to existing exports.
- **Major** (X.0.0): Breaking changes require coordination with all consumers.

Feature-gated modules (`vectors`) are explicitly **unstable** and excluded
from SemVer guarantees.

---

## Consumer Matrix

| Consumer | Crates Used | Boundary | Reference |
|----------|-------------|----------|-----------|
| bolt-daemon | `bolt-core`, `bolt-transfer-core` | Rust path dep | `bolt-daemon/Cargo.toml` |
| localbolt-v3 (browser) | `bolt-transfer-policy-wasm` | wasm-bindgen | `localbolt-v3/packages/` |
| localbolt-app native shell | Rust bridge + daemon IPC | C-ABI + NDJSON IPC | `docs/BOUNDARY_CONTRACT.md` |
| localbolt (browser) | None (TS SDK only) | N/A | — |

See `docs/BOUNDARY_CONTRACT.md` for full boundary contract details.
