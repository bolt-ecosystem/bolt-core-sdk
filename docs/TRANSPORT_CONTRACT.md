# Bolt Transport Contract

**Status:** Frozen. Locked 2026-04-07.

Defines the abstraction boundary between the Bolt protocol layer and transport implementations.
Transport requirement changes (§1) require a spec amendment. New compliant
implementations (§2) may be added without a version bump.

**SDK Stability Alignment:** This document defines behavioral requirements
for transport implementations. [SDK_STABILITY.md](SDK_STABILITY.md) defines
API stability for the SDK package. Both documents together form the SDK
contract for consumers.

## 1. Transport Requirements

A compliant Bolt transport MUST provide:

### Ordered, reliable delivery

- Messages arrive in the order they were sent.
- No message is silently dropped. If delivery fails, the transport reports an error.
- Bolt does NOT handle reordering or retransmission at the protocol layer.

Any transport that provides ordered, reliable delivery of discrete messages satisfies this requirement. Examples: SCTP reliable-ordered streams (WebRTC DataChannel), TCP with message framing, QUIC unidirectional/bidirectional streams with application-level message boundaries.

Transports that are inherently unreliable (e.g., UDP datagrams, QUIC unreliable datagrams, SCTP unordered/unreliable mode) do NOT satisfy this requirement directly. An unreliable transport MAY be used if an additional ordering and reliability layer is added on top, but that layer is outside the scope of Bolt Core and must be validated independently.

### Connection lifecycle events

The transport MUST expose at minimum:

| Event | Meaning |
|-------|---------|
| `connected` | Transport channel is open and ready for data. |
| `message` | A complete message (byte sequence) has been received. |
| `error` | A transport-level error has occurred. |
| `closed` | The transport channel has been closed (graceful or abrupt). |

### Backpressure

- The transport SHOULD provide backpressure signaling when the send buffer is full.
- Implementations MAY use bufferedAmount monitoring (WebRTC) or equivalent mechanisms.
- The protocol layer does NOT enforce a specific backpressure strategy. Products implement this based on transport capabilities.

### Message framing

- The transport delivers discrete messages, not a byte stream.
- Each sealed payload (output of `sealBoxPayload`) is one transport message.
- The transport MUST NOT fragment a single sealed payload across multiple transport messages.
- The transport MUST NOT coalesce multiple sealed payloads into a single transport message.

### Maximum message size

- The protocol does not define a maximum message size.
- Transport implementations impose practical limits (e.g., SCTP max message size for WebRTC).
- `DEFAULT_CHUNK_SIZE` (16 KB) is chosen to stay well within typical transport limits.
- Products MUST NOT send sealed payloads larger than their transport supports.

## 2. Compliant Implementations

### Browser WebRTC DataChannel

- **Status**: Production. Used by localbolt, localbolt-app (web layer), localbolt-v3.
- **Ordered delivery**: `ordered: true` on DataChannel creation.
- **Reliability**: `reliable` mode (no maxRetransmits, no maxPacketLifeTime).
- **Message framing**: SCTP provides message boundaries natively.
- **Max message size**: Typically 256 KB (browser-dependent). 16 KB chunks are well within this.
- **Backpressure**: Monitor `dataChannel.bufferedAmount` before sending.

### libdatachannel (C/C++ library)

- **Status**: Planned. Target for localbolt-app (native) and bytebolt-app.
- **Ordered delivery**: Configured via DataChannel options.
- **Reliability**: Configured to reliable mode.
- **Message framing**: SCTP message boundaries.
- **Max message size**: Configurable. Must be >= `DEFAULT_CHUNK_SIZE` + box overhead (16 bytes) + nonce (24 bytes).
- **Interop**: Uses the same DTLS/SCTP stack as browser WebRTC. Interoperable with browser peers via standard SDP/ICE signaling.

### webrtc-rs (Rust library)

- **Status**: Candidate. Under evaluation for bolt-daemon and headless deployments.
- **Requirements**: Must satisfy the same ordered/reliable/message-framed contract.
- **Graduation criteria**: See `ECOSYSTEM_STRATEGY.md` § Headless Transport Lane.

### WebSocket Client Mode (daemon↔daemon)

- **Status**: Production. Current native↔native (app↔app) transport.
- **Flow**: Initiator daemon connects as WS client to acceptor daemon's WS server endpoint. Same session-key exchange, HELLO handshake, ProfileEnvelopeV1 envelope, and BTR ratchet as browser↔daemon WS.
- **Discovery**: Rendezvous signaling — acceptor sends `wsUrl` in `connection_accepted` signal payload (NATIVE-CONNECT-1).
- **Implementation**: `ws_endpoint::connect_to_remote_ws()` triggered via `connect_remote.signal` file watcher in WsEndpoint mode.
- **Future**: Will be superseded by QUIC when APP-TO-APP-QUIC-MIGRATION-1 completes. WS client mode will remain as fallback.

### QUIC via quinn (Rust)

- **Status**: Reference (RC3). Strategic target for native↔native (app↔app) — see APP-TO-APP-QUIC-MIGRATION-1.
- **Current production path**: Native↔native currently uses WebSocket client mode (see above). QUIC is the intended replacement but is not yet wired into WsEndpoint mode, signaling, or IPC pairing.
- **RC3 limitations**: Self-signed certs with no verification (`Rc3SkipVerification`). No identity-key binding to QUIC TLS. No signaling/discovery integration. Feature-gated behind `transport-quic` (not in default features).
- **Ordered delivery**: QUIC bidirectional streams provide ordered delivery natively.
- **Reliability**: QUIC streams are reliable by default.
- **Message framing**: Application-level length-prefix framing required (QUIC streams are byte-oriented).
- **Backpressure**: QUIC flow control provides stream-level and connection-level backpressure.
- **Graduation criteria**: APP-TO-APP-QUIC-MIGRATION-1 (bolt-ecosystem ROADMAP.md).

### WebTransport (HTTP/3)

- **Status**: Supported. Production path for HTTPS web↔native communication.
- **Ordered delivery**: WebTransport bidirectional streams provide ordered delivery.
- **Reliability**: Reliable streams mode.
- **Message framing**: Application-level message boundaries required.
- **Requirements**: Daemon serves a WebTransport/HTTP3 endpoint with TLS. Browser requires WebTransport API support (Chrome, Edge, Firefox — not Safari as of 2026).
- **Capability**: Negotiated via `bolt.transport-webtransport-v1`.

### WebSocket-direct (dev/LAN only)

- **Status**: Supported. Development and LAN testing path for HTTP/localhost web↔native.
- **Ordered delivery**: WebSocket provides ordered delivery natively.
- **Reliability**: TCP-backed, reliable.
- **Message framing**: WebSocket frames provide message boundaries.
- **Scope**: HTTP origins only (not HTTPS). No TLS required. Not for production deployment.

### Transport Path Summary

| Endpoint Pair | Transport | Status |
|--------------|-----------|--------|
| native↔native (app↔app) | WebSocket client mode | Production (current) |
| native↔native (target) | QUIC via quinn (Rust) | Reference (RC3, strategic target) |
| browser↔browser | WebRTC DataChannel | Production (G1 invariant — immutable) |
| HTTPS web↔native | WebTransport (HTTP/3) | Production |
| HTTP/localhost web↔native | WebSocket-direct | Supported (dev/LAN only) |

**G1 invariant:** browser↔browser always uses WebRTC. This is not negotiable.

**N1 invariant:** native↔native uses WS client mode today. QUIC is the strategic target but not yet production. See APP-TO-APP-QUIC-MIGRATION-1.

## 3. Binary Encoding Rule

Any transport that serializes Bolt sealed payloads to text (e.g., JSON wire format) MUST use:

- **RFC 4648 standard base64** (alphabet `A-Za-z0-9+/`).
- Padding with `=` is **required**.
- **Not base64url.** The `+` and `/` characters are used, not `-` and `_`.
- Case-sensitive. `A` and `a` are distinct.
- No whitespace, line breaks, or other formatting characters within the encoded string.

This applies to all sealed payloads, public key encodings, and any other binary-to-text serialization within the Bolt protocol. Switching to base64url or any other encoding variant requires an explicit decision in `PROTOCOL.md` and a protocol version bump.

## 4. Transport-Agnostic Protocol Messages

All Bolt protocol messages (handshake, payload, control) are transport-agnostic:

- They are defined as byte sequences (sealed payloads).
- They carry no transport-specific metadata.
- The same sealed payload can be sent over any compliant transport.

**Prohibited**: Embedding transport identifiers (e.g., DataChannel labels, SCTP stream IDs) inside sealed payloads.

## 5. LAN-Only Mode

LAN-only operation is enforced at the **signaling and ICE layer**, not by modifying the protocol.

### ICE policy for LAN-only

- **Private IP filtering**: Only gather ICE candidates with private/link-local IP addresses.
  - IPv4: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`
  - IPv6: `fe80::/10`, `fd00::/8`
- **No TURN**: Do not configure TURN servers. No relay candidates.
- **No STUN to public servers**: Optionally omit public STUN servers to prevent IP leakage.
- **mDNS candidates**: Browser-generated mDNS candidates (e.g., `abc123.local`) are acceptable for LAN discovery.

### Implementation notes

- Browser WebRTC: Use `iceTransportPolicy: 'all'` with filtered ICE servers (no TURN, optionally no STUN). Filter candidates in the `onicecandidate` handler.
- libdatachannel: Configure ICE agent with private-only candidate policy.
- webrtc-rs: Same approach — filter at ICE gathering level.

### Important

LAN-only is a **deployment policy**, not a protocol feature. The Bolt protocol itself is network-agnostic. A product may choose to enforce LAN-only via signaling policy, or it may allow WAN connectivity via TURN. The sealed payloads are identical in both cases.

## 6. Signaling

Signaling (SDP offer/answer exchange, ICE candidate relay) is out of scope for the Bolt Core protocol. It is handled by:

- `bolt-rendezvous` (Rust signaling server, bundled or hosted)
- Product-specific signaling logic

The signaling channel is **untrusted**. All sensitive data (identity keys, file contents) is protected by the Bolt encryption layer, not by signaling channel security.

## 7. Connection Lifecycle

```
[Signaling] → ICE gathering → DTLS handshake → SCTP association → DataChannel open
     ↓
[Bolt Handshake] → HELLO exchange (encrypted) → SAS verification → Ready
     ↓
[Data Transfer] → Sealed chunks → ... → Transfer complete
     ↓
[Close] → DataChannel close → SCTP shutdown → ICE teardown
```

The Bolt handshake (HELLO exchange, SAS verification) occurs AFTER the transport channel is established. The transport is just a pipe — Bolt does not rely on transport-level authentication (DTLS fingerprints are not used for peer identity).

## 8. P2P-First Policy and Relay Optionality

### Baseline: Direct P2P

All compliant transport implementations MUST support a direct peer-to-peer
data path as the baseline operating mode. The Bolt protocol assumes that
file payload bytes flow directly between peers, never through infrastructure
operated by us.

### Signaling Is Coordination Only

Signaling servers (bolt-rendezvous) are metadata coordination infrastructure.
They relay opaque signaling payloads (SDP offers/answers, ICE candidates) and
provide presence notifications. Signaling servers MUST NOT be required to
store, inspect, or forward file payload bytes.

### Relay Is Optional and Pluggable

Relay transport (e.g., TURN, a managed relay service) is an OPTIONAL
reliability enhancement. The following constraints apply:

- Relay support MUST be pluggable. Adding or removing relay capability MUST NOT
  require changes to the Bolt protocol layer or to sealed payload format.
- Relays MUST forward opaque ciphertext only. A relay MUST NOT require
  plaintext access to file contents, encryption keys, or transfer metadata.
- The transport contract MUST NOT hardcode a specific relay vendor or service.
  SDK consumers MAY run their own relay infrastructure.
- Products MAY offer relay as a paid feature (e.g., ByteBolt managed relay)
  but MUST NOT require it for basic P2P operation.

### SDK Consumer Freedom

SDK consumers are free to choose any combination:
- P2P only (default, no relay infrastructure needed)
- P2P with consumer-operated relay fallback
- P2P with a managed relay service (e.g., future ByteBolt relay)

The SDK and protocol layers MUST remain agnostic to this choice.

## 9. Transport Implementations (Non-Normative)

This section lists known and candidate transport implementations. It is informational and does not constrain the protocol.

**Invariant: No transport implementation may require Bolt protocol modification.** If a transport cannot satisfy the requirements in §1 without protocol changes, it is not a compliant transport.

### Browser WebRTC DataChannel (Baseline)

| Property | Value |
|----------|-------|
| Status | Production |
| Ordered | Yes (`ordered: true`) |
| Reliable | Yes (no `maxRetransmits`/`maxPacketLifeTime`) |
| Message framing | SCTP native |
| Products | localbolt, localbolt-app (web), localbolt-v3 |

### libdatachannel (C++/Rust FFI)

| Property | Value |
|----------|-------|
| Status | Planned |
| Ordered | Yes (configurable) |
| Reliable | Yes (configurable) |
| Message framing | SCTP native |
| Interop | Standard SDP/ICE — interoperable with browser WebRTC |
| Products | localbolt-app (native), bytebolt-app |

### webrtc-rs (Rust-native)

| Property | Value |
|----------|-------|
| Status | Candidate (under evaluation) |
| Ordered | Yes (configurable) |
| Reliable | Yes (configurable) |
| Message framing | SCTP native |
| Interop | Standard SDP/ICE — must demonstrate interop with browser WebRTC |
| Products | bolt-daemon (target) |
| Graduation | See `ECOSYSTEM_STRATEGY.md` § Headless Transport Lane |

### QUIC Stream Transport (Future Possibility)

| Property | Value |
|----------|-------|
| Status | Not planned — listed for completeness |
| Ordered | Yes (per-stream) |
| Reliable | Yes (per-stream) |
| Message framing | Application-level (QUIC provides byte streams, not messages) |
| Notes | Would require application-level message length prefixing. No ICE/DTLS — different signaling model. |

QUIC unreliable datagrams do NOT satisfy the Bolt transport contract without an additional ordering and reliability layer.
