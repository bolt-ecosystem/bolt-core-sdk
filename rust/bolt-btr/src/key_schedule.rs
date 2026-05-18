//! Key schedule — HKDF-SHA256 derivation chain (§16.3).
//!
//! All derivations use HKDF-SHA256 with info strings from §14.
//! Output length is always 32 bytes (BTR_KEY_LENGTH).

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::constants::{
    BTR_CHAIN_ADVANCE_INFO, BTR_KEY_LENGTH, BTR_MESSAGE_KEY_INFO, BTR_SESSION_ROOT_INFO,
    BTR_TRANSFER_ROOT_INFO,
};

/// Derive session root key from ephemeral shared secret (§16.3).
///
/// ```text
/// session_root_key = HKDF-SHA256(
///   salt  = empty,
///   ikm   = ephemeral_shared_secret,
///   info  = "bolt-btr-session-root-v1",
///   len   = 32
/// )
/// ```
pub fn derive_session_root(ephemeral_shared_secret: &[u8; 32]) -> [u8; BTR_KEY_LENGTH] {
    let hk = Hkdf::<Sha256>::new(None, ephemeral_shared_secret);
    let mut okm = [0u8; BTR_KEY_LENGTH];
    hk.expand(BTR_SESSION_ROOT_INFO, &mut okm)
        .expect("HKDF expand with 32-byte output must not fail");
    okm
}

/// Derive transfer root key from session root key and transfer_id (§16.3).
///
/// ```text
/// transfer_root_key = HKDF-SHA256(
///   salt  = transfer_id (16 bytes),
///   ikm   = current_session_root_key,
///   info  = "bolt-btr-transfer-root-v1",
///   len   = 32
/// )
/// ```
///
/// The initial chain_key for the transfer equals transfer_root_key.
pub fn derive_transfer_root(
    session_root_key: &[u8; BTR_KEY_LENGTH],
    transfer_id: &[u8; 16],
) -> [u8; BTR_KEY_LENGTH] {
    let hk = Hkdf::<Sha256>::new(Some(transfer_id), session_root_key);
    let mut okm = [0u8; BTR_KEY_LENGTH];
    hk.expand(BTR_TRANSFER_ROOT_INFO, &mut okm)
        .expect("HKDF expand with 32-byte output must not fail");
    okm
}

/// Advance the symmetric chain: derive message_key and next_chain_key (§16.3).
///
/// ```text
/// message_key = HKDF-SHA256(salt=empty, ikm=chain_key, info="bolt-btr-message-key-v1", len=32)
/// next_chain_key = HKDF-SHA256(salt=empty, ikm=chain_key, info="bolt-btr-chain-advance-v1", len=32)
/// ```
///
/// Returns `(message_key, next_chain_key)`. Caller MUST:
/// - Zeroize old chain_key immediately
/// - Zeroize message_key after single use
pub fn chain_advance(chain_key: &[u8; BTR_KEY_LENGTH]) -> ChainAdvanceOutput {
    let hk = Hkdf::<Sha256>::new(None, chain_key);

    let mut message_key = [0u8; BTR_KEY_LENGTH];
    hk.expand(BTR_MESSAGE_KEY_INFO, &mut message_key)
        .expect("HKDF expand with 32-byte output must not fail");

    let mut next_chain_key = [0u8; BTR_KEY_LENGTH];
    hk.expand(BTR_CHAIN_ADVANCE_INFO, &mut next_chain_key)
        .expect("HKDF expand with 32-byte output must not fail");

    ChainAdvanceOutput {
        message_key,
        next_chain_key,
    }
}

/// Output of a single chain advance step.
///
/// Both fields are zeroized on drop.
pub struct ChainAdvanceOutput {
    /// Key for encrypting/decrypting one chunk. Single-use, zeroize after.
    pub message_key: [u8; BTR_KEY_LENGTH],
    /// Replacement chain key for the next advance step.
    pub next_chain_key: [u8; BTR_KEY_LENGTH],
}

impl Drop for ChainAdvanceOutput {
    fn drop(&mut self) {
        self.message_key.zeroize();
        self.next_chain_key.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_root_deterministic() {
        let secret = [0xABu8; 32];
        let a = derive_session_root(&secret);
        let b = derive_session_root(&secret);
        assert_eq!(a, b);
        assert_ne!(a, [0u8; 32], "must not be all zeros");
    }

    #[test]
    fn session_root_different_secrets_differ() {
        let a = derive_session_root(&[0x01; 32]);
        let b = derive_session_root(&[0x02; 32]);
        assert_ne!(a, b);
    }

    #[test]
    fn transfer_root_deterministic() {
        let srk = [0xAB; 32];
        let tid = [0x01; 16];
        let a = derive_transfer_root(&srk, &tid);
        let b = derive_transfer_root(&srk, &tid);
        assert_eq!(a, b);
    }

    #[test]
    fn transfer_root_different_ids_differ() {
        let srk = [0xAB; 32];
        let a = derive_transfer_root(&srk, &[0x01; 16]);
        let b = derive_transfer_root(&srk, &[0x02; 16]);
        assert_ne!(a, b);
    }

    #[test]
    fn transfer_root_binds_to_session_root() {
        let tid = [0x01; 16];
        let a = derive_transfer_root(&[0x01; 32], &tid);
        let b = derive_transfer_root(&[0x02; 32], &tid);
        assert_ne!(a, b);
    }

    #[test]
    fn chain_advance_deterministic() {
        let ck = [0xAB; 32];
        let a = chain_advance(&ck);
        let b = chain_advance(&ck);
        assert_eq!(a.message_key, b.message_key);
        assert_eq!(a.next_chain_key, b.next_chain_key);
    }

    #[test]
    fn chain_advance_message_key_differs_from_chain_key() {
        let ck = [0xAB; 32];
        let out = chain_advance(&ck);
        assert_ne!(out.message_key, ck);
        assert_ne!(out.next_chain_key, ck);
        assert_ne!(out.message_key, out.next_chain_key);
    }

    #[test]
    fn chain_advance_successive_steps_differ() {
        let ck0 = [0xAB; 32];
        let out0 = chain_advance(&ck0);
        let out1 = chain_advance(&out0.next_chain_key);
        assert_ne!(out0.message_key, out1.message_key);
        assert_ne!(out0.next_chain_key, out1.next_chain_key);
    }

    #[test]
    fn chain_advance_output_zeroized_on_drop() {
        let ck = [0xAB; 32];
        let mut out = std::mem::ManuallyDrop::new(chain_advance(&ck));
        let mk_ptr = out.message_key.as_ptr();
        let nck_ptr = out.next_chain_key.as_ptr();
        unsafe {
            std::ptr::drop_in_place(&mut *out);
        }
        for i in 0..32 {
            let mk_byte = unsafe { std::ptr::read_volatile(mk_ptr.add(i)) };
            let nck_byte = unsafe { std::ptr::read_volatile(nck_ptr.add(i)) };
            assert_eq!(mk_byte, 0, "message_key byte {i} not zeroed");
            assert_eq!(nck_byte, 0, "next_chain_key byte {i} not zeroed");
        }
    }

    #[test]
    fn chain_advance_5_steps_all_unique() {
        let mut ck = [0x01; 32];
        let mut seen_mk = std::collections::HashSet::new();
        let mut seen_ck = std::collections::HashSet::new();
        for _ in 0..5 {
            let out = chain_advance(&ck);
            assert!(seen_mk.insert(out.message_key), "duplicate message_key");
            assert!(seen_ck.insert(out.next_chain_key), "duplicate chain_key");
            ck = out.next_chain_key;
        }
    }
}
