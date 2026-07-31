# Bolt Core SDK — API Stability Contract

Defines the public API surface, versioning policy, and breaking change
discipline for `@the9ines/bolt-core`. Consumers of this SDK MAY depend on
the stability guarantees documented here.

Keywords: RFC 2119 (MUST, MUST NOT, REQUIRED, SHALL, SHOULD, MAY).

## 1. Public API Surface

The public API consists of all symbols exported from `src/index.ts`:

### Constants

| Export | Module | Type |
|--------|--------|------|
| `NONCE_LENGTH` | constants | number |
| `PUBLIC_KEY_LENGTH` | constants | number |
| `SECRET_KEY_LENGTH` | constants | number |
| `DEFAULT_CHUNK_SIZE` | constants | number |
| `PEER_CODE_LENGTH` | constants | number |
| `PEER_CODE_ALPHABET` | constants | string |
| `SAS_LENGTH` | constants | number |
| `BOLT_VERSION` | constants | string |

### Encoding

| Export | Module | Signature |
|--------|--------|-----------|
| `toBase64` | encoding | `(bytes: Uint8Array) => string` |
| `fromBase64` | encoding | `(str: string) => Uint8Array` |

### Crypto Primitives

| Export | Module | Purpose |
|--------|--------|---------|
| `generateEphemeralKeyPair` | crypto | Generate NaCl box keypair |
| `sealBoxPayload` | crypto | Encrypt payload with NaCl box |
| `openBoxPayload` | crypto | Decrypt payload with NaCl box |

### Peer Codes

| Export | Module | Purpose |
|--------|--------|---------|
| `generateSecurePeerCode` | peer-code | Generate cryptographically random peer code |
| `generateLongPeerCode` | peer-code | Generate extended peer code |
| `isValidPeerCode` | peer-code | Validate peer code format |
| `normalizePeerCode` | peer-code | Normalize peer code case |

### Hashing

| Export | Module | Purpose |
|--------|--------|---------|
| `sha256` | hash | SHA-256 hash |
| `bufferToHex` | hash | Uint8Array to hex string |
| `hashFile` | hash | Hash file contents |

### SAS

| Export | Module | Purpose |
|--------|--------|---------|
| `computeSas` | sas | Compute Short Authentication String from shared secret |

### Error Types

| Export | Module | Purpose |
|--------|--------|---------|
| `BoltError` | errors | Base error class |
| `EncryptionError` | errors | Encryption/decryption failure |
| `ConnectionError` | errors | Connection failure |
| `TransferError` | errors | Transfer failure |

### What Is NOT Public API

Internal module functions that are not re-exported via `index.ts` are NOT
part of this stability contract. Consumers MUST NOT import directly from
internal module paths (e.g., `@the9ines/bolt-core/dist/crypto`).

## 2. Versioning Policy (SemVer)

This SDK follows [Semantic Versioning 2.0.0](https://semver.org/).

### MAJOR (breaking)

A MAJOR version bump is REQUIRED when any of the following occurs:

- Removal or rename of any public export listed in §1
- Change to a public function signature (parameters, return type)
- Change to error class hierarchy
- Change to constant values that alters protocol behavior
- Change to encoding format (e.g., base64 variant)
- Change to required behavior defined in `TRANSPORT_CONTRACT.md`
- Alteration of encryption, framing, or wire semantics

### MINOR (additive)

A MINOR version bump is appropriate for:

- New public exports added to `index.ts`
- New optional parameters with backward-compatible defaults
- New error subclasses
- New utility functions that do not alter existing behavior

### PATCH (fixes)

A PATCH version bump is appropriate for:

- Bug fixes that correct behavior to match documented contracts
- Documentation updates
- CI/tooling changes
- Performance improvements with no behavioral change
- Test additions

## 3. Breaking Change Checklist

Before any MAJOR version bump, the following MUST be reviewed:

- [ ] Did we modify a public function signature?
- [ ] Did we remove or rename a public export?
- [ ] Did we change a constant value that affects protocol behavior?
- [ ] Did we change the base64 encoding variant?
- [ ] Did we alter `sealBoxPayload` or `openBoxPayload` wire format?
- [ ] Did we change required behavior defined in `TRANSPORT_CONTRACT.md`?
- [ ] Did we alter `computeSas` output for the same inputs?
- [ ] Did we change peer code format or validation rules?
- [ ] Were existing test vectors invalidated?

If any box is checked, a MAJOR version bump is REQUIRED.

## 4. Transport Interface Stability

The SDK defines crypto primitives and payload sealing. Transport
implementations (WebRTC DataChannel, libdatachannel, future relay) consume
sealed payloads as opaque byte sequences.

- Direct P2P via libdatachannel is the baseline transport (see
  `TRANSPORT_CONTRACT.md` §8).
- Relay implementations (future) MUST conform to the same transport
  requirements without requiring SDK API changes.
- The SDK MUST remain transport-agnostic. No transport-specific types
  are part of the public API.

## 5. Compatibility with bolt-daemon

bolt-daemon consumes SDK-compatible types under this contract:

- `SignalBundle` (SDP + ICE candidates) uses the same JSON wire format
- Sealed payloads follow the same NaCl box format
- Peer codes follow `PEER_CODE_ALPHABET` and `PEER_CODE_LENGTH`

The daemon operator surface is defined in bolt-daemon's
[DAEMON_CONTRACT.md](https://github.com/bolt-ecosystem/bolt-daemon/blob/main/docs/DAEMON_CONTRACT.md)
(`daemon-v0.1.0-daemon-contract` and later).

Changes to this SDK that would break bolt-daemon compatibility MUST be
coordinated as a MAJOR version bump with corresponding daemon updates.

## 6. Authority

Canonical behavior is defined by the Rust crate (`rust/bolt-core/`),
golden test vectors, and protocol contracts (`PROTOCOL.md`,
`TRANSPORT_CONTRACT.md`).

The TypeScript SDK (`@the9ines/bolt-core`) is a supported adapter
implementation. It MUST produce identical wire-format outputs for
identical inputs, verified by shared golden vectors.

See [SDK_AUTHORITY.md](SDK_AUTHORITY.md) for the full authority model,
interop gates, and versioning rules for both implementations.
