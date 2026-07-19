# Core Extraction Map

**Date:** 2026-02-21
**Phase:** 0 (Audit and Map)
**Status:** Complete

---

## 1. Executive Summary

All three product repos (localbolt, localbolt-app, localbolt-v3) share **identical** core protocol implementations. The sole difference is a single JSDoc comment wording on line 12 of `crypto-utils.ts` in localbolt-app — zero behavioral divergence.

This means extraction is a clean lift: one canonical source, no reconciliation needed.

### Current State

| Concern | Implemented | Spec-Compliant | Gap |
|---------|:-:|:-:|-----|
| Envelope encrypt/decrypt | Yes | Partial | No envelope wrapper object; nonce+ciphertext concatenated inline |
| Nonce generation (24-byte CSPRNG) | Yes | Yes | None |
| Ephemeral key lifecycle | Yes | Partial | No identity key — ephemeral only |
| HELLO handshake | No | No | No encrypted HELLO; public keys exchanged in plaintext signaling |
| TOFU pinning | No | No | No persistent identity keys, no key pinning store |
| KEY_MISMATCH | No | No | Decryption failure is only detection path |
| SAS computation | Partial | Partial | Binds ephemeral keys only (spec requires identity + ephemeral) |
| Message schemas | Partial | Partial | Only `file-chunk` type; no FILE_OFFER/FILE_ACCEPT/FILE_FINISH/ERROR |
| Transfer chunking | Yes | Partial | No transfer_id, no FILE_OFFER/ACCEPT flow, no replay detection |
| File hash verification | No | No | SHA-256 utility exists but not wired into transfer |

---

## 2. Source File Inventory

All paths relative to workspace root `~/Desktop/the9ines.com/bolt-ecosystem/`.

### 2.1 TypeScript — Core Protocol Logic

#### WebRTCService.ts (PRIMARY — 680 lines)

| Repo | Path |
|------|------|
| localbolt | `localbolt/web/src/services/webrtc/WebRTCService.ts` |
| localbolt-app | `localbolt-app/web/src/services/webrtc/WebRTCService.ts` |
| localbolt-v3 | `localbolt-v3/packages/localbolt-web/src/services/webrtc/WebRTCService.ts` |

**Status:** Identical across all 3 repos.

| Lines | Responsibility | Core vs Profile |
|-------|---------------|-----------------|
| 1-2 | TweetNaCl imports (`box`, `randomBytes`, `encodeBase64`, `decodeBase64`) | **Core** |
| 30-41 | `FileChunkMessage` interface | **Core** (message schema) |
| 45-57 | `encryptChunk()` — NaCl box + nonce prepend + base64 | **Core** (envelope seal) |
| 59-70 | `decryptChunk()` — base64 decode + nonce split + NaCl box.open | **Core** (envelope open) |
| 74 | `CHUNK_SIZE = 16384` constant | **Core** (protocol constant) |
| 85-86 | `keyPair` / `remotePublicKey` instance vars | **Core** (ephemeral key state) |
| 121 | `this.keyPair = box.keyPair()` — ephemeral key generation | **Core** (key lifecycle) |
| 135-158 | `handleSignal()` — dispatches offer/answer/ice-candidate | **Profile** (WebRTC signaling) |
| 160-179 | `handleOffer()` — SDP + pubkey exchange | **Profile** (WebRTC) |
| 181-194 | `handleAnswer()` — SDP + pubkey exchange | **Profile** (WebRTC) |
| 196-258 | ICE candidate handling + relay blocking | **Profile** (WebRTC connectivity policy) |
| 260-355 | Same-network validation + connection state | **Profile** (LocalBolt scope policy) |
| 357-394 | `connect()` — WebRTC offer creation | **Profile** (WebRTC) |
| 396-423 | `disconnect()` — cleanup + key discard | Mixed (key discard = Core, RTC cleanup = Profile) |
| 427-494 | `sendFile()` — chunking + encrypt + send | Mixed (chunking = Core, DataChannel send = Profile) |
| 498-524 | `handleMessage()` — message routing | Mixed |
| 533-569 | `processChunk()` — decrypt + reassemble | Mixed (decrypt = Core, Blob assembly = Profile) |
| 571-601 | `pauseTransfer()` / `resumeTransfer()` / `cancelTransfer()` | Mixed |
| 653-676 | `getVerificationCode()` — SAS computation | **Core** |

**Extraction action:** Split into Core functions (seal/open envelope, SAS, chunking logic, message types) and Profile adapter (WebRTC transport, DataChannel framing, ICE policy).

---

#### crypto-utils.ts (85 lines)

| Repo | Path |
|------|------|
| localbolt | `localbolt/web/src/lib/crypto-utils.ts` |
| localbolt-app | `localbolt-app/web/src/lib/crypto-utils.ts` |
| localbolt-v3 | `localbolt-v3/packages/localbolt-web/src/lib/crypto-utils.ts` |

**Status:** localbolt-app has 1 comment-only diff on line 12. Code identical.

| Lines | Responsibility | Core vs Profile |
|-------|---------------|-----------------|
| 1-3 | `ALPHABET` constant (32-char unambiguous) | **Core** (protocol constant) |
| 5-24 | `generateSecurePeerCode()` — 6-char peer code | **Core** (peer code generation) |
| 26-45 | `generateLongPeerCode()` — 8-char XXXX-XXXX | **Core** (peer code generation) |
| 47-62 | `isValidPeerCode()` — validation | **Core** (peer code validation) |
| 64-66 | `sha256()` — Web Crypto SHA-256 | **Core** (hash primitive) |
| 68-78 | `toHex()` / `fromHex()` — encoding helpers | **Core** (encoding utilities) |
| 80-85 | `hashFile()` — file-level SHA-256 | **Core** (file integrity) |

**Extraction action:** Entire file moves to Core. No Profile-specific logic.

---

#### webrtc-errors.ts (36 lines)

| Repo | Path |
|------|------|
| localbolt | `localbolt/web/src/types/webrtc-errors.ts` |
| localbolt-app | `localbolt-app/web/src/types/webrtc-errors.ts` |
| localbolt-v3 | `localbolt-v3/packages/localbolt-web/src/types/webrtc-errors.ts` |

**Status:** Identical across all 3 repos.

| Lines | Responsibility | Core vs Profile |
|-------|---------------|-----------------|
| 1-8 | `WebRTCError` base class | Mixed — rename to `BoltError` |
| 10-14 | `ConnectionError` | **Core** (maps to `CONNECTION_LOST`) |
| 16-20 | `SignalingError` | **Profile** (signaling-specific) |
| 22-28 | `TransferError` | **Core** (maps to `TRANSFER_FAILED`) |
| 30-36 | `EncryptionError` | **Core** (maps to `ENCRYPTION_FAILED`) |

**Extraction action:** Extract Core error types aligned to PROTOCOL.md error codes. Profile keeps `SignalingError` and any WebRTC-specific errors.

---

#### SignalingProvider.ts (41 lines)

| Repo | Path |
|------|------|
| localbolt | `localbolt/web/src/services/signaling/SignalingProvider.ts` |
| localbolt-app | `localbolt-app/web/src/services/signaling/SignalingProvider.ts` |
| localbolt-v3 | `localbolt-v3/packages/localbolt-web/src/services/signaling/SignalingProvider.ts` |

**Status:** Identical across all 3 repos.

| Lines | Responsibility | Core vs Profile |
|-------|---------------|-----------------|
| 1-3 | `DeviceType` type union | **Core** (protocol constant) |
| 5-10 | `SignalMessage` interface | **Profile** (signaling message shape) |
| 12-16 | `DiscoveredDevice` interface | **Profile** (discovery) |
| 18-41 | `SignalingProvider` interface | **Profile** (transport abstraction) |

**Extraction action:** `DeviceType` moves to Core. `SignalMessage`, `DiscoveredDevice`, `SignalingProvider` stay in Profile.

---

### 2.2 TypeScript — Profile-Only Files (NOT extracted)

These files remain in product repos entirely:

| File | Responsibility |
|------|---------------|
| `services/signaling/WebSocketSignaling.ts` | WebSocket transport implementation |
| `services/signaling/DualSignaling.ts` | Local + cloud signaling merge |
| `services/signaling/device-detect.ts` | User-agent device detection |
| `components/peer-connection.ts` | Connection approval UI + orchestration |
| `components/file-upload.ts` | File picker + drag-drop UI |
| `components/transfer-progress.ts` | Transfer progress UI |
| `lib/platform-utils.ts` | ICE config, platform detection, IP validation |
| `lib/sanitize.ts` | XSS prevention |
| `state/store.ts` | Application state management |
| `ui/icons.ts` | SVG icons |

---

### 2.3 Rust — Signal Server

| File | Repos | Core vs Profile |
|------|-------|-----------------|
| `protocol.rs` (166 lines) | All 3 identical | **Profile** — rendezvous wire format |
| `server.rs` (~320 lines) | All 3 identical | **Profile** — WebSocket server, IP grouping |
| `room.rs` (~190 lines) | All 3 identical | **Profile** — IP-based room management |
| `lib.rs` | All 3 identical | **Profile** — server entry |
| `main.rs` | All 3 identical | **Profile** — CLI entry |

**Extraction action:** Signal server is entirely Profile. Nothing extracted to Core.

---

### 2.4 Test Files

Tests exist **only** in the `localbolt` repo (10 files):

| Test File | Tests Core Logic? |
|-----------|:-:|
| `encryption.test.ts` (11 tests) | **Yes** — NaCl box, nonces, key pairs, chunk encrypt/decrypt |
| `crypto-utils.test.ts` (15 tests) | **Yes** — peer code gen, validation, SHA-256, hex encoding |
| `DualSignaling.test.ts` (6 tests) | No — Profile (dual signaling) |
| `device-detect.test.ts` | No — Profile |
| `store.test.ts` | No — Profile |
| `webrtc-errors.test.ts` | Partial — error class hierarchy |
| `transfer-progress.test.ts` | No — UI |
| `platform-utils.test.ts` | No — Profile |
| `sanitize.test.ts` | No — Profile |
| `icons.test.ts` | No — UI |

**Extraction action:** `encryption.test.ts` and `crypto-utils.test.ts` form the basis for SDK conformance tests.

---

## 3. Core vs Profile Boundary

### Core (bolt-core-sdk owns)

| Concern | Current Location | SDK Target |
|---------|-----------------|------------|
| NaCl box seal/open | `WebRTCService.ts:45-70` | `ts/bolt-core/envelope.ts` |
| Ephemeral keypair generation | `WebRTCService.ts:121` | `ts/bolt-core/keys.ts` |
| Nonce generation (24-byte CSPRNG) | `WebRTCService.ts:50` | `ts/bolt-core/envelope.ts` |
| SAS computation | `WebRTCService.ts:653-676` | `ts/bolt-core/sas.ts` |
| Peer code generation + validation | `crypto-utils.ts:1-62` | `ts/bolt-core/peer-code.ts` |
| SHA-256 hash + file hash | `crypto-utils.ts:64-85` | `ts/bolt-core/hash.ts` |
| Hex/base64 encoding helpers | `crypto-utils.ts:68-78` | `ts/bolt-core/encoding.ts` |
| Protocol constants | Scattered | `ts/bolt-core/constants.ts` |
| Error codes (protocol-level) | `webrtc-errors.ts` subset | `ts/bolt-core/errors.ts` |
| File chunk message schema | `WebRTCService.ts:30-41` | `ts/bolt-core/messages.ts` |
| Chunk size calculation | `WebRTCService.ts:74` | `ts/bolt-core/constants.ts` |

### NEW Core (not yet implemented, required by spec)

| Concern | Spec Reference | SDK Target |
|---------|---------------|------------|
| Identity key management | PROTOCOL.md s2 | `ts/bolt-core/identity.ts` |
| TOFU key pinning store | PROTOCOL.md s2 | `ts/bolt-core/tofu.ts` |
| Encrypted HELLO message | PROTOCOL.md s3, s4 | `ts/bolt-core/handshake.ts` |
| Handshake state machine | PROTOCOL.md s9 | `ts/bolt-core/state-machine.ts` |
| Transfer state machine | PROTOCOL.md s9 | `ts/bolt-core/state-machine.ts` |
| Full message model (all types) | PROTOCOL.md s7 | `ts/bolt-core/messages.ts` |
| Capability negotiation | PROTOCOL.md s4 | `ts/bolt-core/handshake.ts` |
| FILE_OFFER/ACCEPT/FINISH flow | PROTOCOL.md s7-8 | `ts/bolt-core/transfer.ts` |
| Replay detection | PROTOCOL.md s8 | `ts/bolt-core/transfer.ts` |
| Resource limit enforcement | PROTOCOL.md s8 | `ts/bolt-core/limits.ts` |
| Error taxonomy (full codes) | PROTOCOL.md s10 | `ts/bolt-core/errors.ts` |

### Profile (product repos own)

| Concern | Stays In |
|---------|----------|
| WebRTC DataChannel setup | Product repo |
| WebSocket signaling transport | Product repo |
| ICE candidate filtering + relay blocking | Product repo |
| Same-network heuristic | Product repo |
| IP-based room grouping | Product repo |
| Dual signaling (local + cloud) | Product repo |
| Connection approval UI | Product repo |
| File picker / drag-drop UI | Product repo |
| Transfer progress UI | Product repo |
| Device detection (user-agent) | Product repo |
| Platform-specific RTC config | Product repo |
| Native product shell | Product repo |
| Netlify/Fly.io deployment | Product repo |

---

## 4. Divergence Report

### Between Repos

| Item | localbolt | localbolt-app | localbolt-v3 | Resolution |
|------|:-:|:-:|:-:|------------|
| WebRTCService.ts | A | A | A | Identical — use as-is |
| crypto-utils.ts | A | A* | A | *1 comment diff. Use localbolt version. |
| SignalingProvider.ts | A | A | A | Identical |
| webrtc-errors.ts | A | A | A | Identical |
| protocol.rs | A | A | A | Identical |
| Test coverage | 10 files | 0 files | 0 files | localbolt is canonical test source |

### Between Implementation and Spec

| Spec Requirement | Implementation Status | Priority |
|-----------------|----------------------|----------|
| Persistent identity keys (TOFU) | **Not implemented** | P1 — required for conformance |
| Encrypted HELLO as first message | **Not implemented** — keys in plaintext signaling | P1 |
| KEY_MISMATCH error + session close | **Not implemented** | P1 |
| SAS binds identity + ephemeral keys | **Partial** — binds ephemeral only | P1 |
| transfer_id on all transfer messages | **Not implemented** | P2 |
| FILE_OFFER/FILE_ACCEPT/FILE_FINISH | **Not implemented** — chunks sent directly | P2 |
| Replay detection per (transfer_id, chunk_index) | **Not implemented** | P2 |
| bolt.file-hash capability + verification | **Not implemented** — utility exists | P2 |
| Resource limit enforcement | **Not implemented** | P3 |
| Handshake gating (reject pre-HELLO messages) | **Not implemented** | P2 |
| Envelope wrapper object (type: "bolt-envelope") | **Not implemented** — raw nonce+ct concat | P2 |

---

## 5. Proposed SDK Layout

```
bolt-core-sdk/
├── PROTOCOL.md                    # (existing) Bolt Core v1 spec
├── LOCALBOLT_PROFILE.md           # (existing) LocalBolt Profile v1 spec
├── README.md                      # (existing, update)
├── docs/
│   ├── CORE_EXTRACTION_MAP.md     # (this file)
│   └── MIGRATION_NOTES.md         # (Phase 2 — divergences and decisions)
├── ts/
│   └── bolt-core/
│       ├── package.json           # @the9ines/bolt-core
│       ├── tsconfig.json
│       ├── src/
│       │   ├── index.ts           # Public API barrel export
│       │   ├── constants.ts       # NONCE_LENGTH, CHUNK_SIZE, PEER_CODE_ALPHABET, etc.
│       │   ├── envelope.ts        # seal_envelope(), open_envelope()
│       │   ├── keys.ts            # generate_ephemeral_keypair(), EphemeralKey, IdentityKey types
│       │   ├── sas.ts             # compute_sas()
│       │   ├── peer-code.ts       # generate_peer_code(), validate_peer_code()
│       │   ├── hash.ts            # sha256(), hash_file()
│       │   ├── encoding.ts        # toHex(), fromHex(), toBase64(), fromBase64()
│       │   ├── messages.ts        # Message types, schemas, serialization
│       │   ├── errors.ts          # BoltError, error codes from PROTOCOL.md s10
│       │   ├── handshake.ts       # handshake_step(), HELLO construction/validation
│       │   ├── transfer.ts        # offer(), accept(), chunk(), reassemble(), finish()
│       │   ├── state-machine.ts   # PeerConnectionState, TransferState enums + transitions
│       │   ├── identity.ts        # IdentityStore interface, TOFU check
│       │   ├── tofu.ts            # TOFU pinning logic, KEY_MISMATCH detection
│       │   └── limits.ts          # Resource limit types + enforcement
│       └── __tests__/
│           ├── envelope.test.ts   # Seal/open, nonce uniqueness, key mismatch decrypt fail
│           ├── sas.test.ts        # SAS test vectors (cross-language)
│           ├── peer-code.test.ts  # Generation, validation, alphabet
│           ├── hash.test.ts       # SHA-256 vectors
│           ├── handshake.test.ts  # Gating rules, state machine
│           ├── transfer.test.ts   # Chunking, reassembly, replay detection
│           └── messages.test.ts   # Serialization roundtrip
├── rust/
│   └── bolt-core/
│       ├── Cargo.toml             # bolt-core crate
│       └── src/
│           ├── lib.rs
│           ├── constants.rs
│           ├── envelope.rs
│           ├── keys.rs
│           ├── sas.rs
│           ├── peer_code.rs
│           ├── hash.rs
│           ├── encoding.rs
│           ├── messages.rs
│           ├── errors.rs
│           ├── handshake.rs
│           ├── transfer.rs
│           ├── state_machine.rs
│           ├── identity.rs
│           ├── tofu.rs
│           └── limits.rs
└── tests/
    └── vectors/
        ├── sas_vectors.json       # Known key pairs -> expected SAS output
        ├── envelope_vectors.json  # Known plaintext + keys + nonce -> expected ciphertext
        └── hash_vectors.json      # Known data -> expected SHA-256
```

---

## 6. Proposed Extraction Increments (Phase 2)

Each increment is a self-contained step. Products stay green after each.

### Increment 1: Constants + Types + Encoding Helpers

**Extract:**
- Protocol constants (`NONCE_LENGTH`, `CHUNK_SIZE`, `PEER_CODE_ALPHABET`, etc.)
- `DeviceType` type
- `toHex()`, `fromHex()` encoding helpers
- Error code enum (string union from PROTOCOL.md s10)

**From:** `crypto-utils.ts`, `webrtc-errors.ts`, `SignalingProvider.ts`, hardcoded values in `WebRTCService.ts`

**Risk:** Zero — pure types and constants, no behavior change.

**Product integration:** Products import constants from `@the9ines/bolt-core`.

---

### Increment 2: Peer Code Generation + Validation

**Extract:**
- `generateSecurePeerCode()`
- `generateLongPeerCode()`
- `isValidPeerCode()`
- `ALPHABET` constant

**From:** `crypto-utils.ts:1-62`

**Risk:** Low — pure functions, well-tested (15 tests in localbolt).

**Product integration:** Replace import from `lib/crypto-utils` with `@the9ines/bolt-core`.

---

### Increment 3: SHA-256 + File Hashing

**Extract:**
- `sha256()`
- `hashFile()`

**From:** `crypto-utils.ts:64-85`

**Risk:** Low — thin wrapper over Web Crypto API.

**Product integration:** Replace import.

---

### Increment 4: Envelope Seal/Open (the critical move)

**Extract:**
- `seal_envelope(plaintext, nonce, receiverPub, senderSec)` — replaces `encryptChunk()`
- `open_envelope(sealed, senderPub, receiverSec)` — replaces `decryptChunk()`
- `generate_ephemeral_keypair()` — wraps `box.keyPair()`
- Nonce generation (`randomBytes(NONCE_LENGTH)`)

**From:** `WebRTCService.ts:45-70, 85-86, 121`

**Risk:** Medium — this is the heart of security. Must preserve exact NaCl box semantics.

**Test vectors:** Create known plaintext + known keys + known nonce -> expected ciphertext. Verify in both TS and Rust.

**Product integration:** `WebRTCService.ts` calls `seal_envelope()` / `open_envelope()` instead of inline TweetNaCl.

---

### Increment 5: SAS Computation

**Extract:**
- `compute_sas(identityA, identityB, ephemeralA, ephemeralB)` — spec-compliant version
- Upgrade from current (ephemeral-only) to spec (identity + ephemeral binding)

**From:** `WebRTCService.ts:653-676`

**Risk:** Medium — SAS output will CHANGE because spec requires identity keys in the hash input. This is a breaking change from current behavior, but current behavior is non-compliant. Products currently don't display SAS in UI, so this is safe.

**Decision:** Use spec-compliant SAS (identity + ephemeral). Record in MIGRATION_NOTES.md.

**Test vectors:** Fixed key bytes -> expected 6-char hex. Must match across TS and Rust.

---

### Increment 6: Message Model

**Extract:**
- All message type definitions from PROTOCOL.md s7
- `HELLO`, `FILE_OFFER`, `FILE_ACCEPT`, `FILE_CHUNK`, `FILE_FINISH`
- `PAUSE`, `RESUME`, `CANCEL`, `ERROR`
- `PING`, `PONG`
- Serialization/deserialization for `json-envelope-v1` encoding

**From:** `WebRTCService.ts:30-41` (partial `FileChunkMessage`) + new code from spec

**Risk:** Medium — this adds new message types that don't exist today. Products need adapter code to bridge current `file-chunk` messages to the full model.

**Product integration:** Introduce gradually. Products can initially wrap old messages in new types.

---

### Increment 7: Handshake State Machine

**Extract:**
- `PeerConnectionState` enum with transitions
- `handshake_step()` — processes HELLO, validates state, returns next state
- Handshake gating rules (reject non-HELLO before completion)

**From:** New code (not currently implemented in products)

**Risk:** High — this is new behavior. Products currently have no handshake gating.

**Product integration:** Wire into connection flow. Initially permissive (warn but don't reject).

---

### Increment 8: Transfer State Machine + Replay Detection

**Extract:**
- `TransferState` enum with transitions
- `transfer_id` generation
- Chunk index tracking + duplicate rejection
- `offer()`, `accept()`, `chunk()`, `reassemble()`, `finish()` functions

**From:** `WebRTCService.ts:427-569` (chunking logic) + new spec-required behavior

**Risk:** High — introduces transfer_id and offer/accept flow that products don't have today.

**Product integration:** Largest adaptation. Products must adopt FILE_OFFER/ACCEPT flow.

---

### Increment 9: Identity + TOFU

**Extract:**
- `IdentityStore` interface (abstract — product provides storage backend)
- `check_tofu(remoteIdentityKey, store)` — returns `match` | `first_contact` | `mismatch`
- `KEY_MISMATCH` error generation

**From:** New code (not currently implemented in products)

**Risk:** High — new feature. Products have no persistent identity today.

**Product integration:** Products implement `IdentityStore` (localStorage for web, native filesystem/keychain storage for native shells). Initially optional; required for spec conformance.

---

## 7. Recommended Execution Order

```
Phase 2 Increments:

  [1] Constants/Types ──► [2] Peer Codes ──► [3] Hashing
         │                                        │
         └──────────────────┬─────────────────────┘
                            ▼
                   [4] Envelope Seal/Open  ◄── Critical path
                            │
                    ┌───────┴───────┐
                    ▼               ▼
              [5] SAS         [6] Messages
                    │               │
                    └───────┬───────┘
                            ▼
                   [7] Handshake SM
                            │
                            ▼
                   [8] Transfer SM
                            │
                            ▼
                    [9] Identity/TOFU
```

**Increments 1-4** are the minimum viable SDK (products can consume immediately).

**Increments 5-6** complete the security and message model.

**Increments 7-9** bring full spec compliance (can be phased in with product updates).

---

## 8. Dependency Strategy (Phase 4 Preview)

**Recommended:** Local path dependency via npm workspaces (TS) and Cargo workspace (Rust).

- Products add `"@the9ines/bolt-core": "file:../../bolt-core-sdk/ts/bolt-core"` to `package.json`
- Rust products add `bolt-core = { path = "../../bolt-core-sdk/rust/bolt-core" }` to `Cargo.toml`
- No git submodule complexity needed while repos are in the same workspace
- Publishing to npm/crates.io deferred to Phase 1.4 milestone completion

---

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| TweetNaCl API incompatibility in SDK | Low | High | Wrap TweetNaCl directly; do not abstract crypto primitives |
| SAS output changes break users | Low | Low | SAS not displayed in UI today |
| Products fail to build after extraction | Medium | High | Extract in small increments; run product builds after each |
| Envelope format change breaks interop | Low | Critical | Preserve exact wire format; test with known vectors |
| Identity/TOFU disrupts existing UX | Medium | Medium | Make identity optional initially; enforce later |

---

## 10. Open Questions for Phase 1

1. **TweetNaCl vs libsodium for Rust crate?** TweetNaCl-compatible output is required. Options: `crypto_box` from `crypto_secretbox` crate, `sodiumoxide`, or `xsalsa20poly1305` crate.
2. **Should Increment 5 (SAS) change wire output immediately or remain backward-compatible?** Recommendation: change to spec-compliant (identity + ephemeral) since SAS is not in UI.
3. **Should bolt-core-sdk house the json-envelope-v1 serialization or keep it in Profile?** Recommendation: SDK provides it as a Profile helper module since it's defined in LOCALBOLT_PROFILE.md, not Core.
