# Bolt Core SDK

Open reference implementation of the [Bolt Protocol](https://github.com/bolt-ecosystem/bolt-protocol) for encrypted device-to-device file transfer.

---

## What This Is

The SDK that all Bolt-based applications embed to speak Bolt Core. One codepath for pairing, TOFU pinning, handshake gating, envelope encryption, and the transfer state machine.

**Note:** The Bolt Protocol specification (PROTOCOL.md, LOCALBOLT_PROFILE.md) currently resides in this repository until [bolt-protocol](https://github.com/bolt-ecosystem/bolt-protocol) is fully separated.

---

## Specification Documents

| Document | Description |
|----------|-------------|
| [PROTOCOL.md](PROTOCOL.md) | Bolt Protocol Core v1. Transport-agnostic specification. |
| [LOCALBOLT_PROFILE.md](LOCALBOLT_PROFILE.md) | LocalBolt Profile v1. Transport binding for the browser peer channel. |

---

## SDK Goals

- Pluggable Profile bindings: apps provide Profile adapters (rendezvous + peer channel + encoding hooks)
- Deterministic behavior: canonical serialization per Profile, stable state machine, strict conformance checks
- Small surface area: minimal public API, strong invariants, reliable defaults
- Transport-agnostic at Core level

## SDK Non-Goals

- Does not implement discovery or routing
- Does not define UI or product policy
- Does not require a daemon

---

## Role in the Ecosystem

| Relationship | Repository |
|-------------|-----------|
| Specification | [bolt-protocol](https://github.com/bolt-ecosystem/bolt-protocol) |
| Depends on this | [localbolt](https://github.com/bolt-ecosystem/localbolt), [localbolt-app](https://github.com/bolt-ecosystem/localbolt-native), [localbolt-v3](https://github.com/bolt-ecosystem/localbolt-v3), [bytebolt-app](https://github.com/bolt-ecosystem/bytebolt-app), [bolt-daemon](https://github.com/bolt-ecosystem/bolt-daemon) |

## Packaging

- **Rust crates** (this repository): `bolt-core`, `bolt-btr`, `bolt-transfer-core`, `bolt-app-core`
- **TypeScript package**: `@the9ines/bolt-core` — published artifact consumed by the web apps; TS sources are not part of this tree.

---

## API Stability

As of version 0.1.0, the public API surface is considered stable under
[Semantic Versioning 2.0.0](https://semver.org/). See
[docs/SDK_STABILITY.md](docs/SDK_STABILITY.md) for the full stability
contract, versioning policy, and breaking change checklist.

---

## Spec Status

| Document | Version | Status |
|----------|---------|--------|
| Bolt Core v1 | 1.0.0 | Draft |
| LocalBolt Profile v1 | 1.0.0 | Draft |

## License

MIT
