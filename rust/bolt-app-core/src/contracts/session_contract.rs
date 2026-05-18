//! Bolt Session/Transfer State Contract v1.
//!
//! Canonical types and transition validators for session lifecycle, transfer
//! lifecycle, and verification state. Products consume these types and
//! validators to ensure conformance with the shared contract.
//!
//! Spec: `docs/SESSION_CONTRACT.md`
//! Schema: `contracts/session_contract.schema.json`

use serde::{Deserialize, Serialize};

// ── Session Phases ─────────────────────────────────────────────

/// Canonical session phases (v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Requesting,
    IncomingRequest,
    Connecting,
    Connected,
}

/// Returns true if the transition from `from` to `to` is a legal session
/// phase transition per the v1 contract.
pub fn is_valid_session_transition(from: SessionPhase, to: SessionPhase) -> bool {
    use SessionPhase::*;
    matches!(
        (from, to),
        (Idle, Requesting)
            | (Idle, IncomingRequest)
            | (Requesting, Connecting)
            | (Requesting, Idle)
            | (IncomingRequest, Connecting)
            | (IncomingRequest, Idle)
            | (Connecting, Connected)
            | (Connecting, Idle)
            | (Connected, Idle)
    )
}

// ── Transfer Phases ────────────────────────────────────────────

/// Canonical transfer phases (v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferPhase {
    Idle,
    Sending,
    Receiving,
    Complete,
    Failed,
}

/// Returns true if the transition from `from` to `to` is a legal transfer
/// phase transition per the v1 contract.
pub fn is_valid_transfer_transition(from: TransferPhase, to: TransferPhase) -> bool {
    use TransferPhase::*;
    matches!(
        (from, to),
        // Normal flow
        (Idle, Sending)
            | (Idle, Receiving)
            | (Sending, Complete)
            | (Sending, Failed)
            | (Receiving, Complete)
            | (Receiving, Failed)
            | (Complete, Idle)
            | (Failed, Idle)
            // Session disconnect mandatory cleanup
            | (Sending, Idle)
            | (Receiving, Idle)
    )
}

// ── Verification States ────────────────────────────────────────

/// Canonical verification states (v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Unverified,
    Verified,
    Legacy,
}

/// Returns true if file transfer is allowed given the current connection
/// status and verification state (policy P1).
pub fn is_transfer_allowed(connected: bool, verification: VerificationState) -> bool {
    connected
        && matches!(
            verification,
            VerificationState::Verified | VerificationState::Legacy
        )
}

// ── Constants ──────────────────────────────────────────────────

/// All canonical session phases.
pub const SESSION_PHASES: [SessionPhase; 5] = [
    SessionPhase::Idle,
    SessionPhase::Requesting,
    SessionPhase::IncomingRequest,
    SessionPhase::Connecting,
    SessionPhase::Connected,
];

/// All canonical transfer phases.
pub const TRANSFER_PHASES: [TransferPhase; 5] = [
    TransferPhase::Idle,
    TransferPhase::Sending,
    TransferPhase::Receiving,
    TransferPhase::Complete,
    TransferPhase::Failed,
];

/// All canonical verification states.
pub const VERIFICATION_STATES: [VerificationState; 3] = [
    VerificationState::Unverified,
    VerificationState::Verified,
    VerificationState::Legacy,
];

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Session transition exhaustive matrix ────────────────

    /// All 9 legal session transitions.
    const LEGAL_SESSION: [(SessionPhase, SessionPhase); 9] = [
        (SessionPhase::Idle, SessionPhase::Requesting),
        (SessionPhase::Idle, SessionPhase::IncomingRequest),
        (SessionPhase::Requesting, SessionPhase::Connecting),
        (SessionPhase::Requesting, SessionPhase::Idle),
        (SessionPhase::IncomingRequest, SessionPhase::Connecting),
        (SessionPhase::IncomingRequest, SessionPhase::Idle),
        (SessionPhase::Connecting, SessionPhase::Connected),
        (SessionPhase::Connecting, SessionPhase::Idle),
        (SessionPhase::Connected, SessionPhase::Idle),
    ];

    #[test]
    fn session_legal_transitions_accepted() {
        for (from, to) in &LEGAL_SESSION {
            assert!(
                is_valid_session_transition(*from, *to),
                "expected legal: {from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn session_illegal_transitions_rejected() {
        let mut illegal_count = 0;
        for from in &SESSION_PHASES {
            for to in &SESSION_PHASES {
                if !LEGAL_SESSION.contains(&(*from, *to)) {
                    assert!(
                        !is_valid_session_transition(*from, *to),
                        "expected illegal: {from:?} -> {to:?}"
                    );
                    illegal_count += 1;
                }
            }
        }
        // 5x5 = 25 total, 9 legal, 16 illegal
        assert_eq!(illegal_count, 16);
    }

    #[test]
    fn session_self_transitions_illegal() {
        for phase in &SESSION_PHASES {
            assert!(
                !is_valid_session_transition(*phase, *phase),
                "self-transition should be illegal: {phase:?} -> {phase:?}"
            );
        }
    }

    // ── Transfer transition exhaustive matrix ───────────────

    /// All 10 legal transfer transitions (12 in spec, but complete->idle
    /// and failed->idle each appear once covering both user-dismiss and
    /// session-disconnect triggers).
    const LEGAL_TRANSFER: [(TransferPhase, TransferPhase); 10] = [
        (TransferPhase::Idle, TransferPhase::Sending),
        (TransferPhase::Idle, TransferPhase::Receiving),
        (TransferPhase::Sending, TransferPhase::Complete),
        (TransferPhase::Sending, TransferPhase::Failed),
        (TransferPhase::Receiving, TransferPhase::Complete),
        (TransferPhase::Receiving, TransferPhase::Failed),
        (TransferPhase::Complete, TransferPhase::Idle),
        (TransferPhase::Failed, TransferPhase::Idle),
        (TransferPhase::Sending, TransferPhase::Idle),
        (TransferPhase::Receiving, TransferPhase::Idle),
    ];

    #[test]
    fn transfer_legal_transitions_accepted() {
        for (from, to) in &LEGAL_TRANSFER {
            assert!(
                is_valid_transfer_transition(*from, *to),
                "expected legal: {from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn transfer_illegal_transitions_rejected() {
        let mut illegal_count = 0;
        for from in &TRANSFER_PHASES {
            for to in &TRANSFER_PHASES {
                if !LEGAL_TRANSFER.contains(&(*from, *to)) {
                    assert!(
                        !is_valid_transfer_transition(*from, *to),
                        "expected illegal: {from:?} -> {to:?}"
                    );
                    illegal_count += 1;
                }
            }
        }
        // 5x5 = 25 total, 10 legal, 15 illegal
        assert_eq!(illegal_count, 15);
    }

    #[test]
    fn transfer_self_transitions_illegal() {
        for phase in &TRANSFER_PHASES {
            assert!(
                !is_valid_transfer_transition(*phase, *phase),
                "self-transition should be illegal: {phase:?} -> {phase:?}"
            );
        }
    }

    // ── Transfer gating policy ──────────────────────────────

    #[test]
    fn transfer_allowed_when_connected_and_verified() {
        assert!(is_transfer_allowed(true, VerificationState::Verified));
    }

    #[test]
    fn transfer_allowed_when_connected_and_legacy() {
        assert!(is_transfer_allowed(true, VerificationState::Legacy));
    }

    #[test]
    fn transfer_blocked_when_connected_and_unverified() {
        assert!(!is_transfer_allowed(true, VerificationState::Unverified));
    }

    #[test]
    fn transfer_blocked_when_not_connected() {
        assert!(!is_transfer_allowed(false, VerificationState::Verified));
        assert!(!is_transfer_allowed(false, VerificationState::Legacy));
        assert!(!is_transfer_allowed(false, VerificationState::Unverified));
    }

    #[test]
    fn transfer_gating_exhaustive() {
        // 2 connected states x 3 verification states = 6 combinations
        let expected = [
            (true, VerificationState::Unverified, false),
            (true, VerificationState::Verified, true),
            (true, VerificationState::Legacy, true),
            (false, VerificationState::Unverified, false),
            (false, VerificationState::Verified, false),
            (false, VerificationState::Legacy, false),
        ];
        for (connected, verification, allowed) in &expected {
            assert_eq!(
                is_transfer_allowed(*connected, *verification),
                *allowed,
                "connected={connected}, verification={verification:?}"
            );
        }
    }

    // ── Serde roundtrip ─────────────────────────────────────

    #[test]
    fn session_phase_serde_roundtrip() {
        for phase in &SESSION_PHASES {
            let json = serde_json::to_string(phase).unwrap();
            let decoded: SessionPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(*phase, decoded);
        }
    }

    #[test]
    fn transfer_phase_serde_roundtrip() {
        for phase in &TRANSFER_PHASES {
            let json = serde_json::to_string(phase).unwrap();
            let decoded: TransferPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(*phase, decoded);
        }
    }

    #[test]
    fn verification_state_serde_roundtrip() {
        for state in &VERIFICATION_STATES {
            let json = serde_json::to_string(state).unwrap();
            let decoded: VerificationState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, decoded);
        }
    }

    #[test]
    fn session_phase_snake_case_serialization() {
        assert_eq!(
            serde_json::to_string(&SessionPhase::IncomingRequest).unwrap(),
            "\"incoming_request\""
        );
        assert_eq!(
            serde_json::to_string(&SessionPhase::Idle).unwrap(),
            "\"idle\""
        );
    }

    // ── JSON instance ↔ Rust authority parity ───────────────

    /// Verify that every transition declared in the JSON instance is legal
    /// per the Rust validators (Rust is authoritative; JSON must not
    /// declare transitions that Rust rejects).
    #[test]
    fn json_instance_session_transitions_match_rust() {
        let json_str = include_str!("../../contracts/session_contract.v1.json");
        let doc: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let transitions = doc["session_transitions"].as_array().unwrap();
        for t in transitions {
            let from: SessionPhase = serde_json::from_value(t["from"].clone()).unwrap();
            let to: SessionPhase = serde_json::from_value(t["to"].clone()).unwrap();
            assert!(
                is_valid_session_transition(from, to),
                "JSON declares session transition {from:?} -> {to:?} but Rust rejects it"
            );
        }
    }

    #[test]
    fn json_instance_transfer_transitions_match_rust() {
        let json_str = include_str!("../../contracts/session_contract.v1.json");
        let doc: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let transitions = doc["transfer_transitions"].as_array().unwrap();
        for t in transitions {
            let from: TransferPhase = serde_json::from_value(t["from"].clone()).unwrap();
            let to: TransferPhase = serde_json::from_value(t["to"].clone()).unwrap();
            assert!(
                is_valid_transfer_transition(from, to),
                "JSON declares transfer transition {from:?} -> {to:?} but Rust rejects it"
            );
        }
    }

    #[test]
    fn json_instance_verification_policy_matches_rust() {
        let json_str = include_str!("../../contracts/session_contract.v1.json");
        let doc: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let states = doc["verification_states"].as_array().unwrap();
        for s in states {
            let id: VerificationState = serde_json::from_value(s["id"].clone()).unwrap();
            let allowed = s["transfer_allowed"].as_bool().unwrap();
            assert_eq!(
                is_transfer_allowed(true, id),
                allowed,
                "JSON says transfer_allowed={allowed} for {id:?} but Rust disagrees"
            );
        }
    }

    /// Verify the JSON instance declares exactly the canonical phase/state sets.
    #[test]
    fn json_instance_contains_exact_canonical_sets() {
        let json_str = include_str!("../../contracts/session_contract.v1.json");
        let doc: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Session phases
        let session_ids: Vec<SessionPhase> = doc["session_phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| serde_json::from_value(e["id"].clone()).unwrap())
            .collect();
        assert_eq!(session_ids.len(), SESSION_PHASES.len());
        for phase in &SESSION_PHASES {
            assert!(
                session_ids.contains(phase),
                "missing session phase: {phase:?}"
            );
        }

        // Transfer phases
        let transfer_ids: Vec<TransferPhase> = doc["transfer_phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| serde_json::from_value(e["id"].clone()).unwrap())
            .collect();
        assert_eq!(transfer_ids.len(), TRANSFER_PHASES.len());
        for phase in &TRANSFER_PHASES {
            assert!(
                transfer_ids.contains(phase),
                "missing transfer phase: {phase:?}"
            );
        }

        // Verification states
        let verification_ids: Vec<VerificationState> = doc["verification_states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| serde_json::from_value(e["id"].clone()).unwrap())
            .collect();
        assert_eq!(verification_ids.len(), VERIFICATION_STATES.len());
        for state in &VERIFICATION_STATES {
            assert!(
                verification_ids.contains(state),
                "missing verification state: {state:?}"
            );
        }
    }

    // ── Parity fixture validation ───────────────────────────

    /// Validate the parity fixture (consumed by web/native tests) matches
    /// the Rust validators exactly. If this test fails, the fixture drifted.
    #[test]
    fn parity_fixture_matches_rust_validators() {
        let json_str = include_str!("../../contracts/parity_fixture.json");
        let doc: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Session phases
        let phases: Vec<String> = doc["session_phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(phases.len(), SESSION_PHASES.len());
        for phase in &SESSION_PHASES {
            let s = serde_json::to_value(phase).unwrap();
            let name = s.as_str().unwrap();
            assert!(
                phases.contains(&name.to_string()),
                "missing session phase: {name}"
            );
        }

        // Session transitions: every pair in fixture must be legal, and count must match
        let legal: Vec<(String, String)> = doc["session_transitions_legal"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                let arr = pair.as_array().unwrap();
                (
                    arr[0].as_str().unwrap().to_string(),
                    arr[1].as_str().unwrap().to_string(),
                )
            })
            .collect();
        for (from_s, to_s) in &legal {
            let from: SessionPhase =
                serde_json::from_value(serde_json::Value::String(from_s.clone())).unwrap();
            let to: SessionPhase =
                serde_json::from_value(serde_json::Value::String(to_s.clone())).unwrap();
            assert!(
                is_valid_session_transition(from, to),
                "fixture declares {from_s} -> {to_s} legal but Rust rejects it"
            );
        }
        assert_eq!(
            legal.len(),
            9,
            "fixture must declare exactly 9 legal session transitions"
        );

        // Transfer transitions
        let t_legal: Vec<(String, String)> = doc["transfer_transitions_legal"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                let arr = pair.as_array().unwrap();
                (
                    arr[0].as_str().unwrap().to_string(),
                    arr[1].as_str().unwrap().to_string(),
                )
            })
            .collect();
        for (from_s, to_s) in &t_legal {
            let from: TransferPhase =
                serde_json::from_value(serde_json::Value::String(from_s.clone())).unwrap();
            let to: TransferPhase =
                serde_json::from_value(serde_json::Value::String(to_s.clone())).unwrap();
            assert!(
                is_valid_transfer_transition(from, to),
                "fixture declares {from_s} -> {to_s} legal but Rust rejects it"
            );
        }
        assert_eq!(
            t_legal.len(),
            10,
            "fixture must declare exactly 10 legal transfer transitions"
        );

        // Transfer gating
        let gating = doc["transfer_gating"].as_object().unwrap();
        for (state_s, allowed_v) in gating {
            let state: VerificationState =
                serde_json::from_value(serde_json::Value::String(state_s.clone())).unwrap();
            let allowed = allowed_v.as_bool().unwrap();
            assert_eq!(
                is_transfer_allowed(true, state),
                allowed,
                "fixture says {state_s} -> {allowed} but Rust disagrees"
            );
        }
    }
}
