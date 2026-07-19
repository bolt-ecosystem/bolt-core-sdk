# Bolt Ecosystem Security Posture

**Date:** 2026-02-24
**Applies to:** WebRTC DataChannel profile (LocalBolt, LocalBolt App, LocalBolt v3)
**Authority:** bolt-core-sdk

---

## Scope

This document covers the security properties of the Bolt Protocol as implemented in the WebRTC DataChannel transport (`@the9ines/bolt-transport-web` v0.6.0) consumed by all three product repos. It does not cover bolt-daemon (planned, Rust-native transport) or bytebolt-relay (commercial, planned).

---

## Security Guarantees

### 1. Confidentiality

- **Encryption:** NaCl box (Curve25519 ECDH + XSalsa20-Poly1305). Per-chunk random nonce.
- **Key exchange:** Ephemeral X25519 keypairs generated per session. Zeroed on disconnect.
- **Metadata protection:** Profile Envelope v1 wraps all post-handshake messages in encrypted envelope with profile name and version. Capability-gated (`bolt.envelope`).
- **Rendezvous untrusted:** Signaling server sees only opaque peer codes and ICE candidates. No plaintext content, no identity keys.

### 2. Authentication

- **TOFU (Trust On First Use):** Long-lived X25519 identity keypairs. Pinned on first contact via IndexedDB. Subsequent connections verify against pin. Fail-closed: `KeyMismatchError` triggers `ConnectionError` + `disconnect()`.
- **SAS (Short Authentication String):** 6-hex-char verification code derived from `SHA-256(sorted(localIdentityPub || remoteIdentityPub) || localEphemeralPub || remoteEphemeralPub)`. Displayed to users for out-of-band confirmation. Canonical implementation in `bolt-core` only.

### 3. Integrity

- **Per-chunk:** NaCl Poly1305 MAC on every encrypted chunk. Tampered chunks fail decryption.
- **Per-file:** SHA-256 file hash computed by sender, included on first chunk, verified by receiver after reassembly. Capability-gated (`bolt.file-hash`). Fail-closed: hash mismatch triggers `IntegrityError`, `INTEGRITY_FAILED` control message, and `disconnect()`. `onReceiveFile` is NOT called.

### 4. Replay and Duplication Protection

- **Transfer ID:** Each transfer identified by `transferId` (crypto.getRandomValues, 16 bytes, hex-encoded). Included in every chunk, pause, resume, and cancel message.
- **Chunk dedup:** Receiver tracks `receivedSet` per transfer. Duplicate `chunkIndex` silently ignored (logged `[REPLAY_DUP]`).
- **Bounds checking:** `chunkIndex` must be in `[0, totalChunks)`. `totalChunks` must be a finite positive integer. Violations rejected with `[REPLAY_OOB]`.
- **Sender binding:** `transferId` bound to sender identity key. Cross-peer reuse rejected with `[REPLAY_XFER_MISMATCH]`.
- **Legacy compatibility:** Peers without `transferId` accepted via unguarded path with `[REPLAY_UNGUARDED]` deprecation warning. Bounds checks still applied.

### 5. Handshake Gating

- **Pre-handshake rejection:** Any non-HELLO message received before `helloComplete` triggers `INVALID_STATE` error control message and `disconnect()`. Fail-closed.
- **HELLO timeout:** 5-second legacy timeout for peers that do not send HELLO. Session degrades to legacy mode (no TOFU, no SAS, no capabilities).

### 6. Capability Negotiation

- **Mechanism:** Capabilities advertised in encrypted HELLO payload. Intersection computed. Both peers must support a capability for it to activate.
- **Current capabilities:** `bolt.file-hash` (file integrity), `bolt.envelope` (metadata protection).
- **Missing field:** Treated as empty `[]` (backward compatible with pre-capability peers).

---

## Operational Constraints

| Constraint | Behavior |
|------------|----------|
| Mixed-peer (new + old) | Graceful degradation: capabilities not negotiated, legacy paths used, deprecation warnings logged |
| TOFU violation | Fail-closed: disconnect, no retry |
| Integrity mismatch | Fail-closed: disconnect, file not delivered |
| Pre-handshake messages | Fail-closed: disconnect |
| Relay ICE candidates | Blocked (same-network policy) |
| Signaling compromise | Cannot decrypt content or forge identity (rendezvous untrusted by design) |

---

## Non-Goals and Deferred Items

| Item | Reason |
|------|--------|
| **I2: Daemon/Web NaCl interop** | bolt-daemon is planned/minimal. No TS consumers. Deferred until daemon development begins. |
| **I4: Protocol-level bolt-envelope** | Profile Envelope v1 covers the WebRTC DataChannel transport. Full protocol-level standardization across all transports (daemon, relay) deferred to bolt-protocol specification work. |
| **Q4: localbolt-app test suite** | Build-only gate. App is scaffold-stage. Deferred until app matures. |
| **Forward secrecy** | Not a protocol goal. Ephemeral keys rotate per session but TOFU identity keys are long-lived. Key compromise does not expose past sessions (no session keys stored), but a compromised identity key enables MITM on future sessions until the pin is reset. |
| **Multi-device identity** | Single identity per browser origin (IndexedDB). No cross-device identity federation. |
| **Denial of service** | Peer code collision detection on signal server. No rate limiting on DataChannel (trusted after handshake). |

---

## Residual Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| TOFU bootstrap MITM | MEDIUM | SAS verification available for out-of-band confirmation. Users should verify SAS on first connection to a new peer. |
| Legacy peer downgrade | LOW | Legacy sessions skip TOFU/SAS/replay/integrity. `[REPLAY_UNGUARDED]` and `isLegacySession()` logged. Future versions may make HELLO mandatory (fail-closed). |
| Base64 overhead on DataChannel | LOW | Performance impact, not security. 33% bandwidth inflation per chunk. Binary framing is a future optimization. |
| 16KB chunk granularity | LOW | Crypto overhead scales with chunk count. Acceptable for current file sizes. Chunk size increase is backward-compatible (receiver handles any size). |
| IndexedDB pin store | LOW | Pins stored in browser-origin-scoped IndexedDB. Clearing browser data resets all pins (TOFU restarts). No export/backup mechanism. |
| Signal server IP grouping | LOW | Peers on same public IP auto-discover. Shared IP environments (CGNAT, corporate NAT) may expose peer presence to unrelated users. Mitigated by approval-based connection flow (request/accept/decline). |

---

## Enforcement Posture (H0–H3)

As of H2, `WebRTCService` enforces:
- **Exactly-once HELLO:** Duplicate HELLO triggers `DUPLICATE_HELLO` error + disconnect.
- **Envelope-required mode:** Post-HELLO plaintext rejected with `ENVELOPE_REQUIRED` + disconnect when `bolt.envelope` negotiated.
- **Fail-closed:** Every protocol error in Section 3 of `PROTOCOL_ENFORCEMENT.md` triggers error frame + disconnect.
- **Downgrade resistance:** No runtime path disables enforcement post-negotiation.

As of H3, cross-implementation parity is verified by golden vectors (SAS, HELLO-open, envelope-open) across TS SDK, Rust SDK, and bolt-daemon.

H2 and H3 are on feature branches, not yet merged to main.

Full enforcement specification: `bolt-ecosystem/docs/PROTOCOL_ENFORCEMENT.md`

## Audit Trail

Detailed internal audit trackers are kept outside the public SDK tree.

Conformance mapping (spec to test): [`docs/conformance/LOCALBOLT_CONFORMANCE.md`](conformance/LOCALBOLT_CONFORMANCE.md)

ADR for signaling integration: localbolt-v3 `docs/adr/ADR-0001-signaling-integration-model.md`
