//! Session authority primitives — transport-agnostic handshake lifecycle.
//!
//! Extracted from bolt-daemon (RC2-EXEC-E, AC-RC-07) into shared core so
//! that all consumers (daemon, future native apps, WASM adapters) share a
//! single canonical session state machine.
//!
//! # ARCH-01 Compliance
//!
//! This module is strictly transport/profile-agnostic:
//! - No WebRTC, DataChannel, WebSocket, SDP, ICE references.
//! - No serde, JSON, or wire-format encoding.
//! - No profile-envelope-v1, json-envelope-v1, or codec types.
//!
//! Profile-level codec/dispatch remains in the daemon adapter layer.

use crate::crypto::KeyPair;

// ── Session Lifecycle ─────────────────────────────────────────

/// Canonical session lifecycle states.
///
/// Every Bolt session transitions linearly: PreHello → PostHello → Closed.
/// No backward transitions are permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Before mutual HELLO completion. Only HELLO/ERROR/PING/PONG allowed.
    PreHello,
    /// After mutual HELLO completion. Full message set allowed.
    PostHello,
    /// Session terminated. No further messages.
    Closed,
}

// ── Exactly-Once HELLO Guard ──────────────────────────────────

/// Tracks whether the HELLO exchange has completed.
/// Rejects duplicate HELLOs (fail-closed).
#[derive(Default)]
pub struct HelloState {
    completed: bool,
}

impl HelloState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark HELLO as completed. Returns Err if already completed.
    pub fn mark_completed(&mut self) -> Result<(), &'static str> {
        if self.completed {
            return Err("[INTEROP-2_HELLO_FAIL] duplicate HELLO — exactly-once violation");
        }
        self.completed = true;
        Ok(())
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}

// ── HELLO-Phase Error Codes ───────────────────────────────────

/// Error codes for HELLO-phase protocol violations.
///
/// Wire codes align with PROTOCOL.md §10 and the canonical
/// `WIRE_ERROR_CODES` registry in [`crate::errors`].
#[derive(Debug)]
pub enum HelloError {
    /// HELLO outer frame unparseable (not UTF-8, not JSON, wrong outer type).
    ParseError(String),
    /// HELLO sealed payload fails decryption (wrong key, tampered).
    DecryptFail(String),
    /// HELLO inner payload missing required fields or wrong types.
    SchemaError(String),
    /// Identity key does not match pinned key (TOFU violation).
    KeyMismatch(String),
    /// Duplicate HELLO received after exchange already completed.
    DuplicateHello,
    /// Legacy downgrade attempt: raw `bolt-hello-v1` payload.
    DowngradeAttempt,
}

impl HelloError {
    /// Wire error code string aligned with PROTOCOL.md §10 registry.
    pub fn code(&self) -> &'static str {
        match self {
            HelloError::ParseError(_) => "HELLO_PARSE_ERROR",
            HelloError::DecryptFail(_) => "HELLO_DECRYPT_FAIL",
            HelloError::SchemaError(_) => "HELLO_SCHEMA_ERROR",
            HelloError::KeyMismatch(_) => "KEY_MISMATCH",
            HelloError::DuplicateHello => "DUPLICATE_HELLO",
            HelloError::DowngradeAttempt => "PROTOCOL_VIOLATION",
        }
    }
}

impl std::fmt::Display for HelloError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelloError::ParseError(detail) => write!(f, "HELLO parse error: {detail}"),
            HelloError::DecryptFail(detail) => write!(f, "HELLO decrypt failure: {detail}"),
            HelloError::SchemaError(detail) => write!(f, "HELLO schema error: {detail}"),
            HelloError::KeyMismatch(detail) => write!(f, "identity key mismatch: {detail}"),
            HelloError::DuplicateHello => write!(f, "duplicate HELLO — exactly-once violation"),
            HelloError::DowngradeAttempt => {
                write!(f, "legacy 'bolt-hello-v1' payload — downgrade refused")
            }
        }
    }
}

impl std::error::Error for HelloError {}

// ── Session Context ───────────────────────────────────────────

/// Transport-agnostic session state container.
///
/// Holds the HELLO outcome for post-handshake operations.
///
/// Invariants:
/// - `hello_state` is completed at construction time (HELLO already done).
/// - `state` transitions to `PostHello` at construction.
pub struct SessionContext {
    pub local_keypair: KeyPair,
    pub remote_public_key: [u8; 32],
    pub negotiated_capabilities: Vec<String>,
    hello_state: HelloState,
    state: SessionState,
}

impl SessionContext {
    /// Build a session context from the HELLO exchange outcome.
    ///
    /// `hello_state` is immediately marked completed — the caller has
    /// already finished the HELLO exchange before calling this.
    /// Returns Err if HelloState was already completed (internal invariant violation).
    pub fn new(
        local_keypair: KeyPair,
        remote_public_key: [u8; 32],
        negotiated_capabilities: Vec<String>,
    ) -> Result<Self, &'static str> {
        let mut hello_state = HelloState::new();
        hello_state.mark_completed()?;
        Ok(Self {
            local_keypair,
            remote_public_key,
            negotiated_capabilities,
            hello_state,
            state: SessionState::PostHello,
        })
    }

    /// Check if a specific capability was negotiated.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.negotiated_capabilities.iter().any(|c| c == cap)
    }

    /// Shorthand: was `bolt.profile-envelope-v1` negotiated?
    pub fn envelope_v1_negotiated(&self) -> bool {
        self.has_capability("bolt.profile-envelope-v1")
    }

    /// Whether the HELLO exchange has completed.
    pub fn is_hello_complete(&self) -> bool {
        self.hello_state.is_completed()
    }

    /// Current session lifecycle state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Transition to Closed state. Idempotent.
    pub fn close(&mut self) {
        self.state = SessionState::Closed;
    }
}

// ── Capability Negotiation ────────────────────────────────────

/// Compute the intersection of local and remote capability sets.
///
/// Pure set-intersection logic — no profile/transport awareness.
pub fn negotiate_capabilities(local: &[String], remote: &[String]) -> Vec<String> {
    local
        .iter()
        .filter(|cap| remote.contains(cap))
        .cloned()
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_identity_keypair;

    // ── SessionState tests ──────────────────────────────────

    #[test]
    fn session_state_values() {
        assert_ne!(SessionState::PreHello, SessionState::PostHello);
        assert_ne!(SessionState::PostHello, SessionState::Closed);
        assert_ne!(SessionState::PreHello, SessionState::Closed);
    }

    // ── HelloState tests ────────────────────────────────────

    #[test]
    fn hello_state_first_completion_succeeds() {
        let mut state = HelloState::new();
        assert!(!state.is_completed());
        assert!(state.mark_completed().is_ok());
        assert!(state.is_completed());
    }

    #[test]
    fn hello_state_duplicate_rejected() {
        let mut state = HelloState::new();
        state.mark_completed().unwrap();
        let result = state.mark_completed();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly-once"));
    }

    // ── HelloError tests ────────────────────────────────────

    #[test]
    fn hello_error_codes_are_canonical() {
        use crate::errors::is_valid_wire_error_code;
        assert!(is_valid_wire_error_code(
            HelloError::ParseError("".into()).code()
        ));
        assert!(is_valid_wire_error_code(
            HelloError::DecryptFail("".into()).code()
        ));
        assert!(is_valid_wire_error_code(
            HelloError::SchemaError("".into()).code()
        ));
        assert!(is_valid_wire_error_code(
            HelloError::KeyMismatch("".into()).code()
        ));
        assert!(is_valid_wire_error_code(HelloError::DuplicateHello.code()));
        assert!(is_valid_wire_error_code(
            HelloError::DowngradeAttempt.code()
        ));
    }

    #[test]
    fn hello_error_display() {
        let err = HelloError::ParseError("bad json".into());
        assert!(err.to_string().contains("HELLO parse error"));
        assert!(err.to_string().contains("bad json"));

        let err = HelloError::DuplicateHello;
        assert!(err.to_string().contains("exactly-once"));

        let err = HelloError::DowngradeAttempt;
        assert!(err.to_string().contains("downgrade"));
    }

    // ── SessionContext tests ────────────────────────────────

    fn make_ctx(caps: Vec<String>) -> SessionContext {
        let kp = generate_identity_keypair();
        let remote_pk = generate_identity_keypair().public_key;
        SessionContext::new(kp, remote_pk, caps).unwrap()
    }

    #[test]
    fn new_returns_ok_for_fresh_state() {
        let kp = generate_identity_keypair();
        let remote_pk = generate_identity_keypair().public_key;
        assert!(SessionContext::new(kp, remote_pk, vec![]).is_ok());
    }

    #[test]
    fn hello_state_completed_after_new() {
        let ctx = make_ctx(vec![]);
        assert!(ctx.is_hello_complete());
    }

    #[test]
    fn state_is_post_hello_after_new() {
        let ctx = make_ctx(vec![]);
        assert_eq!(ctx.state(), SessionState::PostHello);
    }

    #[test]
    fn close_transitions_to_closed() {
        let mut ctx = make_ctx(vec![]);
        ctx.close();
        assert_eq!(ctx.state(), SessionState::Closed);
    }

    #[test]
    fn envelope_v1_negotiated_true_when_cap_present() {
        let ctx = make_ctx(vec!["bolt.profile-envelope-v1".to_string()]);
        assert!(ctx.envelope_v1_negotiated());
    }

    #[test]
    fn envelope_v1_negotiated_false_when_cap_absent() {
        let ctx = make_ctx(vec![]);
        assert!(!ctx.envelope_v1_negotiated());
    }

    #[test]
    fn envelope_v1_negotiated_false_with_other_caps() {
        let ctx = make_ctx(vec!["bolt.file-hash".to_string()]);
        assert!(!ctx.envelope_v1_negotiated());
    }

    #[test]
    fn has_capability_works() {
        let ctx = make_ctx(vec![
            "bolt.profile-envelope-v1".to_string(),
            "bolt.file-hash".to_string(),
        ]);
        assert!(ctx.has_capability("bolt.profile-envelope-v1"));
        assert!(ctx.has_capability("bolt.file-hash"));
        assert!(!ctx.has_capability("bolt.nonexistent"));
    }

    #[test]
    fn stores_remote_pk() {
        let kp = generate_identity_keypair();
        let remote_kp = generate_identity_keypair();
        let remote_pk = remote_kp.public_key;
        let ctx = SessionContext::new(kp, remote_pk, vec![]).unwrap();
        assert_eq!(ctx.remote_public_key, remote_pk);
    }

    #[test]
    fn stores_negotiated_caps() {
        let caps = vec![
            "bolt.profile-envelope-v1".to_string(),
            "bolt.file-hash".to_string(),
        ];
        let ctx = make_ctx(caps.clone());
        assert_eq!(ctx.negotiated_capabilities, caps);
    }

    #[test]
    fn stores_local_keypair() {
        let kp = generate_identity_keypair();
        let pk = kp.public_key;
        let remote_pk = generate_identity_keypair().public_key;
        let ctx = SessionContext::new(kp, remote_pk, vec![]).unwrap();
        assert_eq!(ctx.local_keypair.public_key, pk);
    }

    // ── negotiate_capabilities tests ────────────────────────

    #[test]
    fn negotiate_full_overlap() {
        let local = vec!["a".to_string(), "b".to_string()];
        let remote = vec!["a".to_string(), "b".to_string()];
        let result = negotiate_capabilities(&local, &remote);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn negotiate_partial_overlap() {
        let local = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let remote = vec!["b".to_string(), "d".to_string()];
        let result = negotiate_capabilities(&local, &remote);
        assert_eq!(result, vec!["b"]);
    }

    #[test]
    fn negotiate_no_overlap() {
        let local = vec!["a".to_string()];
        let remote = vec!["b".to_string()];
        let result = negotiate_capabilities(&local, &remote);
        assert!(result.is_empty());
    }

    #[test]
    fn negotiate_empty_local() {
        let result = negotiate_capabilities(&[], &["a".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn negotiate_empty_remote() {
        let local = vec!["a".to_string()];
        let result = negotiate_capabilities(&local, &[]);
        assert!(result.is_empty());
    }
}
