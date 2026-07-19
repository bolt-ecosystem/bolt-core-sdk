# Bolt Protocol Ecosystem Strategy

Phase 2B governance document. Defines product boundaries, repo roles, release discipline, and consolidation decisions.

## 1. Product Taxonomy

### Protocol & SDK Layer

| Repo | Role | Audience |
|------|------|----------|
| `bolt-protocol` | Normative specification (PROTOCOL.md, profiles). No code. | Protocol reviewers, spec readers |
| `bolt-core-sdk` | Reference implementation of Bolt Core. TypeScript package `@the9ines/bolt-core` published to GitHub Packages. Future: Rust SDK. | Product developers consuming the SDK |

**Source-of-truth rules:**
- Protocol semantics are defined in `bolt-protocol` (or temporarily in `bolt-core-sdk` until separation).
- Conformance tests live in `bolt-core-sdk`.
- Products must not reimplement crypto primitives; they import from `@the9ines/bolt-core`.

### Infrastructure Layer

| Repo | Role |
|------|------|
| `bolt-rendezvous` | Canonical signaling/rendezvous server (Rust). Bundled into localbolt and localbolt-app via git subtree. |
| `bolt-daemon` | Background Rust service for session management. Used by native apps (localbolt-app, bytebolt-app). |
| `bytebolt-relay` | Commercial managed relay. Not open-source. |

### Product Layer

| Repo | Product | Transport | Platform | Rendezvous | Daemon |
|------|---------|-----------|----------|------------|--------|
| `localbolt` | Open-source lite app | WebRTC | Browser | Bundled (subtree) | No |
| `localbolt-app` | Open-source native app | WebRTC + daemon transports | SwiftUI macOS shell over Rust core | Bundled (subtree) | Yes |
| `localbolt-v3` | Web app (Netlify) | WebRTC | Browser | Hosted endpoint | No |

### Legacy (read-only)

- `localbolt-v1-main-legacy` — archived.
- `localbolt-v2-main-legacy` — archived.

## 2. Consolidation Decision

### Decision Matrix

| Option | Pros | Cons | Risk |
|--------|------|------|------|
| **A: Keep all 3, define lanes** | Each repo has a clear deployment target. localbolt-app owns native shells that cannot merge into a pure-web repo. | Three repos with related product surfaces. Drift risk. | Medium: divergence over time |
| **B: Deprecate localbolt** | Reduces to 2 active products. localbolt-app already has a superset (web + native). | localbolt has the only test suite and CI coverage. Loss of the lightweight standalone. | High: lose test coverage, standalone use case |
| **C: Deprecate localbolt-v3** | localbolt is the canonical open-source lite product. v3 was a stepping stone. | v3 is the live Netlify deployment. Migration required. | Medium: deployment migration needed |
| **D: Merge localbolt + localbolt-app** | Single canonical product repo. Shared test suite. | Native app packaging + web-only use case in same repo. Build complexity increases. | High: structural work, not just governance |

### Decision: **Option A — Keep all 3, define lanes**

**Rationale:**
1. localbolt and localbolt-app serve different deployment targets (standalone browser vs native app). Merging requires structural work beyond this phase's scope.
2. localbolt-v3 is the live production deployment on Netlify with landing page content that the other repos lack.
3. localbolt is the only repo with a test suite (237 tests). It serves as the canonical test bed.
4. No repo is dead weight — each has a distinct deployment target.

**Lane definitions:**

- **localbolt**: Canonical open-source web app. Standalone browser deployment. Owns the test suite. Bundled rendezvous for fully offline operation.
- **localbolt-app**: Native application. Adds native file system access, background transfers, and bolt-daemon integration through SwiftUI + Rust wrappers.
- **localbolt-v3**: Netlify-deployed web app with landing page. Production web deployment. No bundled infrastructure.

**Constraints enforced by this decision:**
- Web source changes should originate in localbolt (which has tests), then propagate to localbolt-app and localbolt-v3.
- localbolt-app owns native-only code. localbolt-v3 owns landing page sections.
- No new product repos until protocol and SDK reach 1.0.

## 3. Release Train

### SDK → Product Pinning Policy

- Products pin to an exact SDK version (e.g. `"@the9ines/bolt-core": "0.0.5"`). No ranges.
- SDK upgrades are explicit: bump version, regenerate lockfile, verify, commit.
- All three product repos must upgrade within the same release cycle. No version skew across products.

### Upgrade Cadence

- SDK patch releases (bug fixes): upgrade products within 1 business day.
- SDK minor releases (API changes, pre-1.0): upgrade products in the same session.
- No product may ship with a different SDK version than the other two.

### Process

See `docs/RELEASE_PLAYBOOK.md` for step-by-step procedure.

## 4. Deprecation Policy

### Criteria for deprecating a product repo

A product repo is a candidate for deprecation when ALL of the following are true:
1. Another repo fully covers its deployment target.
2. Its unique functionality (tests, config, landing page) has been migrated.
3. No active users depend on it as a deployment artifact.
4. The team explicitly approves deprecation.

### Current status

No repo meets deprecation criteria today.

### Process

1. Mark the repo README with a deprecation notice.
2. Archive the GitHub repo (read-only).
3. Remove from CI monitoring.
4. Do not delete — preserve history.

## 5. Anti-Sprawl Constraints

The following MUST NOT be started until the SDK reaches 1.0:

- No new product repositories.
- No new protocol profiles beyond LocalBolt.
- No new infrastructure services (bolt-daemon, bytebolt-relay are defined but may remain unstarted).
- No splitting bolt-core-sdk into multiple packages.

The following are permitted:
- Bug fixes and security patches to any existing repo.
- SDK patch releases.
- CI and governance improvements.
- Documentation.

## 6. CI Contract

### Permissions (least-privilege)

| Repo type | permissions block |
|-----------|------------------|
| Product CI | `contents: read`, `packages: read` |
| SDK publish | `contents: read`, `packages: write` |

No other permissions should be granted. No custom secrets.

### Node setup pattern (all product repos)

```yaml
- uses: actions/setup-node@<pinned-sha> # v4
  with:
    node-version: 20
    registry-url: https://npm.pkg.github.com
    scope: '@the9ines'
    cache: npm
    cache-dependency-path: <path-to-lockfile>
- run: npm ci
  env:
    NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### .npmrc placement

| Repo | .npmrc location | Reason |
|------|-----------------|--------|
| bolt-core-sdk | `ts/bolt-core/.npmrc` | Package directory (publisher) |
| localbolt | `web/.npmrc` | Package directory (npm ci runs in web/) |
| localbolt-app | `web/.npmrc` | Package directory (npm ci runs in web/) |
| localbolt-v3 | `.npmrc` (repo root) | Monorepo (npm ci runs at root) |

Contents: `@the9ines:registry=https://npm.pkg.github.com` — no auth tokens.

### Tag discipline

- SDK publish triggers on strict semver tags only: `sdk-v<MAJOR>.<MINOR>.<PATCH>`.
- Suffix tags (e.g. `sdk-v0.0.5-phase2a`) must NOT trigger publish.
- Product tags follow repo-specific release conventions.

## 7. Headless Transport Lane

### Why headless matters

Server-side and daemon deployments (bolt-daemon, future CLI tools, automated relay nodes) cannot use browser WebRTC APIs. These deployments need a Rust-native WebRTC implementation that:

- Runs without a browser or GUI.
- Integrates with Tokio or async-std runtimes.
- Supports the same DTLS/SCTP/DataChannel stack that browser WebRTC provides.
- Can interoperate with browser WebRTC peers (standard SDP/ICE signaling).

### Current status

**libdatachannel** (via Rust FFI, `datachannel` crate) is the standardized
headless transport. bolt-daemon uses it in production for all signaling modes
(file and rendezvous). It is proven for LAN, overlay, and global scopes.

**webrtc-rs** (`webrtc` crate) remains an optional evaluation lane only. It is
not required for any current product or milestone. If evaluated, it must meet
the graduation criteria below before adoption.

### Relay lane (future, not MVP)

Relay infrastructure (bytebolt-relay) is a future transport lane for ByteBolt
as a paid reliability feature. It is not required for LocalBolt, bolt-daemon,
or any current product. See `TRANSPORT_CONTRACT.md` §8 (P2P-First Policy)
for the normative constraints on relay optionality.

### Graduation criteria

Before webrtc-rs (or any headless transport) is adopted for production use, it MUST:

1. **Pass deterministic vector verification.** Open all valid vectors in `__tests__/vectors/box-payload.vectors.json`. Reject all corrupt vectors. Pass all framing assertions in `framing.vectors.json`.

2. **Pass live interop matrix.** Successfully complete the deterministic message exchange scenario (see `docs/INTEROP_TEST_PLAN.md` §3) against:
   - Browser WebRTC peer (P0, required)
   - libdatachannel peer (P1, required if libdatachannel is in production)

3. **Pass LAN-only ICE policy.** Demonstrate that ICE candidate gathering can be restricted to private/link-local addresses only. No TURN. Optionally no public STUN. Verify by inspecting gathered candidates in test transcript.

4. **Demonstrate stable SCTP behavior.** Transfer at least 100 MB of data across 10 sequential connections without:
   - SCTP association failures
   - DataChannel premature closure
   - Message reordering or loss
   - Memory leaks (RSS stable within 10% over test duration)

5. **No protocol changes.** The webrtc-rs transport MUST work with the existing Bolt protocol layer unchanged. If protocol changes are required, the transport is not ready.

### Non-goals

- webrtc-rs does NOT change the protocol. It is a transport implementation.
- webrtc-rs does NOT replace browser WebRTC for browser-based products.
- webrtc-rs evaluation does NOT block SDK 1.0 or any current product release.

### Decision gate

Adoption of webrtc-rs requires:

1. All graduation criteria pass in CI.
2. At least one week of integration testing with bolt-daemon.
3. Team review and explicit approval.
4. Documentation update to `TRANSPORT_CONTRACT.md` adding webrtc-rs as a production transport.

Until the decision gate is passed, webrtc-rs remains "candidate" status. No product may depend on it.
