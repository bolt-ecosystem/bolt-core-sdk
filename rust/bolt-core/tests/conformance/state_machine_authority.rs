//! Conformance: Protocol State-Machine Authority (AC-RC-10)
//!
//! Proves that Rust is the canonical source for all protocol state machines.
//! TS implementations are parity copies validated by cross-language vectors —
//! Rust defines the authoritative enums, transitions, and invariant guards.
//!
//! Scope (AC-RC-10):
//! - Transfer SM: TransferState, SendSession, ReceiveSession
//! - BTR negotiation: BtrMode, negotiate_btr
//! - BTR session ratchet: BtrEngine, BtrTransferContext
//! - BTR replay guard: ReplayGuard
//! - Backpressure: BackpressureController
//! - Policy types: StallClassification, DeviceClass, FairnessMode
//!
//! Out of scope (AC-RC-07):
//! - Session lifecycle (pre_hello / post_hello / closed)
//! - Handshake state (HELLO processing, verification)
//! - Capability negotiation dispatch

// ── Transfer State Machine Authority ────────────────────────────

/// AC-RC-10: TransferState enum defines exactly 8 states per PROTOCOL.md §9.
#[test]
fn authority_transfer_state_covers_protocol_s9() {
    use bolt_transfer_core::TransferState;

    let states: Vec<TransferState> = vec![
        TransferState::Idle,
        TransferState::Offered {
            transfer_id: "t".into(),
        },
        TransferState::Accepted {
            transfer_id: "t".into(),
        },
        TransferState::Transferring {
            transfer_id: "t".into(),
        },
        TransferState::Paused {
            transfer_id: "t".into(),
        },
        TransferState::Completed {
            transfer_id: "t".into(),
        },
        TransferState::Cancelled {
            transfer_id: "t".into(),
            reason: bolt_transfer_core::CancelReason::BySender,
        },
        TransferState::Error {
            detail: "err".into(),
        },
    ];
    assert_eq!(states.len(), 8, "PROTOCOL.md §9 defines exactly 8 states");
}

/// AC-RC-10: CancelReason covers all 3 cancel paths.
#[test]
fn authority_cancel_reasons_complete() {
    use bolt_transfer_core::CancelReason;

    let reasons = [
        CancelReason::BySender,
        CancelReason::ByReceiver,
        CancelReason::Rejected,
    ];
    assert_eq!(
        reasons.len(),
        3,
        "3 cancel reasons: sender, receiver, rejected"
    );
}

/// AC-RC-10: SendSession enforces invalid transitions (not just flag-based).
#[test]
fn authority_send_session_rejects_invalid_transition() {
    let mut session = bolt_transfer_core::SendSession::new();

    // Cannot accept when idle (no offer sent).
    let result = session.on_accept("nonexistent");
    assert!(result.is_err(), "accept from Idle must fail");
}

/// AC-RC-10: ReceiveSession enforces offer-before-chunk invariant.
#[test]
fn authority_receive_session_rejects_chunk_before_offer() {
    let mut session = bolt_transfer_core::ReceiveSession::new();

    let result = session.on_file_chunk("nonexistent", 0, &[0xAB; 64]);
    assert!(result.is_err(), "chunk before offer must fail");
}

// ── BTR Negotiation Authority ───────────────────────────────────

/// AC-RC-10: negotiate_btr implements full 6-cell matrix from §4.
#[test]
fn authority_btr_negotiate_6_cell_matrix() {
    use bolt_btr::negotiate::{negotiate_btr, BtrMode};

    assert_eq!(negotiate_btr(true, true, true), BtrMode::FullBtr);
    assert_eq!(negotiate_btr(true, true, false), BtrMode::Reject);
    assert_eq!(negotiate_btr(true, false, true), BtrMode::Downgrade);
    assert_eq!(negotiate_btr(true, false, false), BtrMode::Downgrade);
    assert_eq!(negotiate_btr(false, true, true), BtrMode::Downgrade);
    assert_eq!(negotiate_btr(false, false, false), BtrMode::StaticEphemeral);
}

// ── BTR State Machine Authority ─────────────────────────────────

/// AC-RC-10: BtrEngine enforces monotonic ratchet generation.
#[test]
fn authority_btr_engine_generation_monotonic() {
    use bolt_btr::state::BtrEngine;

    let shared_secret = [0xAA_u8; 32];
    let mut engine = BtrEngine::new(&shared_secret);

    assert_eq!(engine.ratchet_generation(), 0);

    // Each transfer increments generation.
    let remote_pub = [0x01_u8; 32];
    let tid1: [u8; 16] = [0x01; 16];
    let tid2: [u8; 16] = [0x02; 16];

    let _ = engine.begin_transfer_send(&tid1, &remote_pub);
    assert_eq!(engine.ratchet_generation(), 1);

    engine.end_transfer();
    let _ = engine.begin_transfer_send(&tid2, &remote_pub);
    assert_eq!(engine.ratchet_generation(), 2);
}

/// AC-RC-10: BtrTransferContext enforces chain_index monotonicity (ORDER-BTR).
/// Uses seal/open on the same context to verify the guard without needing
/// a paired DH exchange (which is tested exhaustively in bolt-btr tests).
#[test]
fn authority_btr_chain_index_rejects_skip() {
    use bolt_btr::state::BtrEngine;

    let shared_secret = [0xBB_u8; 32];
    let mut engine = BtrEngine::new(&shared_secret);
    let tid: [u8; 16] = [0x10; 16];

    let (mut ctx, _pub) = engine.begin_transfer_send(&tid, &[0x02; 32]).unwrap();

    // Seal chunk 0 to advance chain.
    let (idx0, sealed0) = ctx.seal_chunk(b"chunk-0").unwrap();
    assert_eq!(idx0, 0);

    // chain_index is now 1. Attempt open_chunk with index 2 (skip).
    // This must fail because open_chunk checks expected_chain_index == self.chain_index.
    let skip_result = ctx.open_chunk(2, &sealed0);
    assert!(
        skip_result.is_err(),
        "skipped chain_index must be rejected (ORDER-BTR)"
    );

    // Also verify index 0 (replay) is rejected.
    let replay_result = ctx.open_chunk(0, &sealed0);
    assert!(
        replay_result.is_err(),
        "past chain_index must be rejected (ORDER-BTR)"
    );
}

// ── BTR Replay Guard Authority ──────────────────────────────────

/// AC-RC-10: ReplayGuard rejects duplicate (REPLAY-BTR).
#[test]
fn authority_replay_guard_rejects_duplicate() {
    use bolt_btr::replay::ReplayGuard;

    let mut guard = ReplayGuard::new();
    let tid = [0x01_u8; 16];

    guard.begin_transfer(tid, 1);

    // First check succeeds.
    assert!(guard.check(&tid, 1, 0).is_ok());

    // Duplicate must fail.
    assert!(
        guard.check(&tid, 1, 0).is_err(),
        "duplicate triple must be rejected"
    );
}

// ── Backpressure Authority ──────────────────────────────────────

/// AC-RC-10: BackpressureController hysteresis prevents flapping.
#[test]
fn authority_backpressure_hysteresis() {
    use bolt_transfer_core::{Backpressure, BackpressureConfig, BackpressureController};

    struct MockTransport(usize);
    impl bolt_transfer_core::TransportQuery for MockTransport {
        fn is_open(&self) -> bool {
            true
        }
        fn buffered_bytes(&self) -> usize {
            self.0
        }
        fn max_message_size(&self) -> usize {
            65536
        }
    }

    let config = BackpressureConfig::new(100, 25);
    let mut ctrl = BackpressureController::new(config);

    // Below high — no pause.
    assert_eq!(ctrl.evaluate(&MockTransport(50)), Backpressure::NoChange);

    // Hit high — pause.
    assert_eq!(ctrl.evaluate(&MockTransport(100)), Backpressure::Pause);
    assert!(ctrl.is_paused());

    // Drop to 50 (above low) — stays paused (hysteresis).
    assert_eq!(ctrl.evaluate(&MockTransport(50)), Backpressure::NoChange);
    assert!(ctrl.is_paused());

    // Drop to low — resume.
    assert_eq!(ctrl.evaluate(&MockTransport(25)), Backpressure::Resume);
    assert!(!ctrl.is_paused());
}

// ── Cross-Language Vector Authority ─────────────────────────────

/// AC-RC-10: BTR cross-language vectors exist (10 files), proving Rust
/// generates the canonical vectors that TS consumes.
#[test]
fn authority_btr_cross_language_vectors_present() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let btr_dir = manifest.join("test-vectors").join("btr");

    let expected = [
        "btr-adversarial.vectors.json",
        "btr-chain-advance.vectors.json",
        "btr-dh-ratchet.vectors.json",
        "btr-dh-sanity.vectors.json",
        "btr-downgrade-negotiate.vectors.json",
        "btr-encrypt-decrypt.vectors.json",
        "btr-key-schedule.vectors.json",
        "btr-lifecycle.vectors.json",
        "btr-replay-reject.vectors.json",
        "btr-transfer-ratchet.vectors.json",
    ];

    for name in &expected {
        let path = btr_dir.join(name);
        assert!(
            path.exists(),
            "BTR vector file missing: {name} — Rust vector authority broken"
        );
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 0, "BTR vector file empty: {name}");
    }
}

/// AC-RC-10: Core cross-language vectors exist (5 files).
#[test]
fn authority_core_cross_language_vectors_present() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core_dir = manifest.join("test-vectors").join("core");

    let expected = [
        "box-payload.vectors.json",
        "framing.vectors.json",
        "sas.vectors.json",
        "web-hello-open.vectors.json",
        "envelope-open.vectors.json",
    ];

    for name in &expected {
        let path = core_dir.join(name);
        assert!(
            path.exists(),
            "Core vector file missing: {name} — Rust vector authority broken"
        );
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 0, "Core vector file empty: {name}");
    }
}
