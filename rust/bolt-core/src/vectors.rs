//! Deterministic golden vector generator — CANONICAL SOURCE.
//!
//! This module is the single source of truth for all core golden test
//! vectors in the Bolt ecosystem. Generated vectors are consumed by both
//! the Rust and TypeScript SDKs for cross-implementation verification.
//!
//! Covers: box-payload, framing, SAS, HELLO-open, envelope-open.
//! Output directory: `test-vectors/core/` (parallel to `test-vectors/btr/`).
//!
//! Uses fixed keypairs, nonces, and plaintexts for full determinism.
//! TEST FIXTURES ONLY — keypairs are publicly known and MUST NOT be used
//! in production.
//!
//! ## Authority (AC-RC-08)
//! Rust is canonical. TS vector generation is deprecated (AC-RC-09).
//! See `VECTOR_AUTHORITY.md` for migration details.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto_box::{
    aead::{Aead, Payload},
    Nonce, SalsaBox, SecretKey,
};
use serde::Serialize;

// ── Helpers ─────────────────────────────────────────────────────────

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn make_secret(offset: u8) -> [u8; 32] {
    let mut s = [0u8; 32];
    for (i, b) in s.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(offset);
    }
    s
}

fn make_nonce(offset: u8) -> [u8; 24] {
    let mut n = [0u8; 24];
    for (i, b) in n.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(offset);
    }
    n
}

fn seal_with_fixed_nonce(
    plaintext: &[u8],
    nonce_bytes: &[u8; 24],
    receiver_pk: &crypto_box::PublicKey,
    sender_sk: &SecretKey,
) -> String {
    let salsa_box = SalsaBox::new(receiver_pk, sender_sk);
    let nonce = Nonce::from_slice(nonce_bytes);
    let ciphertext = salsa_box
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &[],
            },
        )
        .expect("encryption must not fail with valid inputs");
    let mut combined = Vec::with_capacity(24 + ciphertext.len());
    combined.extend_from_slice(nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    STANDARD.encode(&combined)
}

// ── Fixed inputs ────────────────────────────────────────────────────

fn keypair_from_offset(offset: u8) -> ([u8; 32], SecretKey, crypto_box::PublicKey) {
    let secret = make_secret(offset);
    let sk = SecretKey::from(secret);
    let pk = sk.public_key();
    (secret, sk, pk)
}

fn plain_256() -> [u8; 256] {
    let mut p = [0u8; 256];
    for (i, b) in p.iter_mut().enumerate() {
        *b = i as u8;
    }
    p
}

// ── box-payload schema ──────────────────────────────────────────────

#[derive(Serialize)]
struct BoxPayloadVectors {
    #[serde(rename = "_WARNING")]
    warning: String,
    description: String,
    sender: KeypairData,
    receiver: KeypairData,
    eve: KeypairData,
    vectors: Vec<BoxVector>,
    corrupt_vectors: Vec<CorruptVector>,
}

#[derive(Serialize)]
struct KeypairData {
    #[serde(rename = "publicKey_base64")]
    public_key_base64: String,
    #[serde(rename = "secretKey_base64")]
    secret_key_base64: String,
    #[serde(rename = "publicKey_hex")]
    public_key_hex: String,
    #[serde(rename = "secretKey_hex")]
    secret_key_hex: String,
}

#[derive(Serialize)]
struct BoxVector {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plaintext_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plaintext_hex: Option<String>,
    nonce_hex: String,
    sealed_base64: String,
}

#[derive(Serialize)]
struct CorruptVector {
    id: String,
    description: String,
    sealed_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_eve_as_sender: Option<bool>,
    expected_error: String,
}

impl KeypairData {
    fn from_parts(pk: &crypto_box::PublicKey, sk_bytes: &[u8; 32]) -> Self {
        Self {
            public_key_base64: STANDARD.encode(pk.as_bytes()),
            secret_key_base64: STANDARD.encode(sk_bytes),
            public_key_hex: to_hex(pk.as_bytes()),
            secret_key_hex: to_hex(sk_bytes),
        }
    }
}

// ── framing schema ──────────────────────────────────────────────────

#[derive(Serialize)]
struct FramingVectors {
    #[serde(rename = "_WARNING")]
    warning: String,
    description: String,
    constants: FramingConstants,
    vectors: Vec<FramingVector>,
}

#[derive(Serialize)]
struct FramingConstants {
    nonce_length: usize,
    box_overhead: usize,
}

#[derive(Serialize)]
struct FramingVector {
    id: String,
    sealed_base64: String,
    expected_decoded_length: usize,
    expected_nonce_hex: String,
    expected_ciphertext_length: usize,
    plaintext_length: usize,
}

// ── Generators ──────────────────────────────────────────────────────

/// Generate the box-payload vectors JSON string.
pub fn generate_box_payload_json() -> String {
    let (sender_sk_bytes, sender_sk, sender_pk) = keypair_from_offset(1);
    let (receiver_sk_bytes, _, receiver_pk) = keypair_from_offset(33);
    let (eve_sk_bytes, _, eve_pk) = keypair_from_offset(65);

    let nonce_a = make_nonce(0);
    let nonce_b = make_nonce(24);

    let plain_hello = b"Hello, Bolt!";
    let plain_empty: &[u8] = &[];
    let plain_one_byte: &[u8] = &[0xff];
    let plain_256 = plain_256();

    let sealed_hello = seal_with_fixed_nonce(plain_hello, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_empty = seal_with_fixed_nonce(plain_empty, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_one_byte = seal_with_fixed_nonce(plain_one_byte, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_256 = seal_with_fixed_nonce(&plain_256, &nonce_b, &receiver_pk, &sender_sk);

    // Corrupt: modified — flip last bit of last byte
    let corrupt_modified = {
        let decoded = STANDARD.decode(&sealed_hello).unwrap();
        let mut copy = decoded.clone();
        let last = copy.len() - 1;
        copy[last] ^= 0x01;
        STANDARD.encode(&copy)
    };

    // Corrupt: truncated — remove last 4 bytes
    let corrupt_truncated = {
        let decoded = STANDARD.decode(&sealed_hello).unwrap();
        STANDARD.encode(&decoded[..decoded.len() - 4])
    };

    // Corrupt: nonce-only
    let corrupt_nonce_only = STANDARD.encode(nonce_a);

    let data = BoxPayloadVectors {
        warning: "TEST FIXTURES ONLY \u{2014} NEVER USE IN PRODUCTION. All keypairs are deterministic, publicly known, and not valid cryptographic material for real deployments.".to_string(),
        description: "Deterministic NaCl box payload test vectors for @the9ines/bolt-core. Generated by Rust canonical vector generator (bolt-core, vectors feature).".to_string(),
        sender: KeypairData::from_parts(&sender_pk, &sender_sk_bytes),
        receiver: KeypairData::from_parts(&receiver_pk, &receiver_sk_bytes),
        eve: KeypairData::from_parts(&eve_pk, &eve_sk_bytes),
        vectors: vec![
            BoxVector {
                id: "hello-bolt".to_string(),
                plaintext_utf8: Some("Hello, Bolt!".to_string()),
                plaintext_hex: Some(to_hex(plain_hello)),
                nonce_hex: to_hex(&nonce_a),
                sealed_base64: sealed_hello.clone(),
            },
            BoxVector {
                id: "empty-payload".to_string(),
                plaintext_utf8: Some(String::new()),
                plaintext_hex: Some(String::new()),
                nonce_hex: to_hex(&nonce_a),
                sealed_base64: sealed_empty.clone(),
            },
            BoxVector {
                id: "single-byte-ff".to_string(),
                plaintext_utf8: None,
                plaintext_hex: Some(to_hex(plain_one_byte)),
                nonce_hex: to_hex(&nonce_a),
                sealed_base64: sealed_one_byte.clone(),
            },
            BoxVector {
                id: "256-byte-pattern".to_string(),
                plaintext_utf8: None,
                plaintext_hex: Some(to_hex(&plain_256)),
                nonce_hex: to_hex(&nonce_b),
                sealed_base64: sealed_256.clone(),
            },
        ],
        corrupt_vectors: vec![
            CorruptVector {
                id: "modified-ciphertext".to_string(),
                description: "Last byte of sealed payload flipped".to_string(),
                sealed_base64: corrupt_modified,
                use_eve_as_sender: None,
                expected_error: "Decryption failed".to_string(),
            },
            CorruptVector {
                id: "truncated-ciphertext".to_string(),
                description: "Last 4 bytes removed from sealed payload".to_string(),
                sealed_base64: corrupt_truncated,
                use_eve_as_sender: None,
                expected_error: "Decryption failed".to_string(),
            },
            CorruptVector {
                id: "wrong-sender-key".to_string(),
                description: "Receiver opens with eve's public key instead of sender's"
                    .to_string(),
                sealed_base64: sealed_hello,
                use_eve_as_sender: Some(true),
                expected_error: "Decryption failed".to_string(),
            },
            CorruptVector {
                id: "nonce-only".to_string(),
                description:
                    "Payload contains only a 24-byte nonce with no ciphertext (empty ciphertext)"
                        .to_string(),
                sealed_base64: corrupt_nonce_only,
                use_eve_as_sender: None,
                expected_error: "Decryption failed".to_string(),
            },
        ],
    };

    serde_json::to_string_pretty(&data).unwrap() + "\n"
}

/// Generate the framing vectors JSON string.
pub fn generate_framing_json() -> String {
    let (_, sender_sk, _) = keypair_from_offset(1);
    let (_, _, receiver_pk) = keypair_from_offset(33);

    let nonce_a = make_nonce(0);
    let nonce_b = make_nonce(24);

    let plain_hello = b"Hello, Bolt!";
    let plain_empty: &[u8] = &[];
    let plain_one_byte: &[u8] = &[0xff];
    let plain_256 = plain_256();

    let sealed_hello = seal_with_fixed_nonce(plain_hello, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_empty = seal_with_fixed_nonce(plain_empty, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_one_byte = seal_with_fixed_nonce(plain_one_byte, &nonce_a, &receiver_pk, &sender_sk);
    let sealed_256 = seal_with_fixed_nonce(&plain_256, &nonce_b, &receiver_pk, &sender_sk);

    let framing_entry = |id: &str, sealed: &str, plaintext_len: usize| -> FramingVector {
        let decoded = STANDARD.decode(sealed).unwrap();
        FramingVector {
            id: id.to_string(),
            sealed_base64: sealed.to_string(),
            expected_decoded_length: decoded.len(),
            expected_nonce_hex: to_hex(&decoded[..24]),
            expected_ciphertext_length: plaintext_len + 16,
            plaintext_length: plaintext_len,
        }
    };

    let data = FramingVectors {
        warning: "TEST FIXTURES ONLY \u{2014} NEVER USE IN PRODUCTION. Sealed payloads use deterministic, publicly known test keypairs.".to_string(),
        description: "Wire format framing test vectors for @the9ines/bolt-core. Verifies nonce||ciphertext layout. Generated by Rust canonical vector generator.".to_string(),
        constants: FramingConstants {
            nonce_length: 24,
            box_overhead: 16,
        },
        vectors: vec![
            framing_entry("hello-bolt-framing", &sealed_hello, plain_hello.len()),
            framing_entry("empty-framing", &sealed_empty, 0),
            framing_entry("single-byte-framing", &sealed_one_byte, 1),
            framing_entry("256-byte-framing", &sealed_256, 256),
        ],
    };

    serde_json::to_string_pretty(&data).unwrap() + "\n"
}

// ── SAS vector schema ─────────────────────────────────────────────

#[derive(Serialize)]
struct SasVectors {
    version: u32,
    #[serde(rename = "_WARNING")]
    warning: String,
    description: String,
    cases: Vec<SasCase>,
}

#[derive(Serialize)]
struct SasCase {
    name: String,
    description: String,
    identity_a_hex: String,
    identity_b_hex: String,
    ephemeral_a_hex: String,
    ephemeral_b_hex: String,
    expected_sas: String,
}

/// Generate the SAS golden vectors JSON string.
///
/// Uses `crate::sas::compute_sas` to produce expected SAS values.
/// Same fixed keypairs as box-payload vectors plus sparse test keys.
pub fn generate_sas_json() -> String {
    let (_, _, sender_pk) = keypair_from_offset(1);
    let (_, _, receiver_pk) = keypair_from_offset(33);
    let (_, _, eve_pk) = keypair_from_offset(65);

    // Sparse test keys (matching TS generate-h3-vectors.mjs)
    let mut key_a = [0u8; 32];
    key_a[0] = 0x01;
    key_a[31] = 0xAA;

    let mut key_b = [0u8; 32];
    key_b[0] = 0x02;
    key_b[31] = 0xBB;

    let mut eph_a = [0u8; 32];
    eph_a[0] = 0x10;
    eph_a[31] = 0xCC;

    let mut eph_b = [0u8; 32];
    eph_b[0] = 0x20;
    eph_b[31] = 0xDD;

    let sender_pk_bytes: [u8; 32] = *sender_pk.as_bytes();
    let receiver_pk_bytes: [u8; 32] = *receiver_pk.as_bytes();
    let eve_pk_bytes: [u8; 32] = *eve_pk.as_bytes();

    let data = SasVectors {
        version: 1,
        warning: "TEST FIXTURES ONLY \u{2014} NEVER USE IN PRODUCTION. All keys are deterministic test fixtures.".to_string(),
        description: "SAS (Short Authentication String) golden vectors. Generated by Rust canonical vector generator (bolt-core, vectors feature).".to_string(),
        cases: vec![
            SasCase {
                name: "sas_sparse_keys".to_string(),
                description: "Sparse test keys (mostly zeros with sentinel bytes at positions 0 and 31)".to_string(),
                identity_a_hex: to_hex(&key_a),
                identity_b_hex: to_hex(&key_b),
                ephemeral_a_hex: to_hex(&eph_a),
                ephemeral_b_hex: to_hex(&eph_b),
                expected_sas: crate::sas::compute_sas(&key_a, &key_b, &eph_a, &eph_b),
            },
            SasCase {
                name: "sas_box_payload_keys_symmetric".to_string(),
                description: "Uses sender + receiver public keys from box-payload.vectors.json for both identity and ephemeral".to_string(),
                identity_a_hex: to_hex(&sender_pk_bytes),
                identity_b_hex: to_hex(&receiver_pk_bytes),
                ephemeral_a_hex: to_hex(&sender_pk_bytes),
                ephemeral_b_hex: to_hex(&receiver_pk_bytes),
                expected_sas: crate::sas::compute_sas(&sender_pk_bytes, &receiver_pk_bytes, &sender_pk_bytes, &receiver_pk_bytes),
            },
            SasCase {
                name: "sas_mixed_keys".to_string(),
                description: "Identity: receiver + eve; ephemeral: sender + receiver from box-payload.vectors.json".to_string(),
                identity_a_hex: to_hex(&receiver_pk_bytes),
                identity_b_hex: to_hex(&eve_pk_bytes),
                ephemeral_a_hex: to_hex(&sender_pk_bytes),
                ephemeral_b_hex: to_hex(&receiver_pk_bytes),
                expected_sas: crate::sas::compute_sas(&receiver_pk_bytes, &eve_pk_bytes, &sender_pk_bytes, &receiver_pk_bytes),
            },
            SasCase {
                name: "sas_all_different_keys".to_string(),
                description: "All four keys are distinct: sender_pk, receiver_pk, eve_pk, and sparse keyA".to_string(),
                identity_a_hex: to_hex(&sender_pk_bytes),
                identity_b_hex: to_hex(&eve_pk_bytes),
                ephemeral_a_hex: to_hex(&receiver_pk_bytes),
                ephemeral_b_hex: to_hex(&key_a),
                expected_sas: crate::sas::compute_sas(&sender_pk_bytes, &eve_pk_bytes, &receiver_pk_bytes, &key_a),
            },
        ],
    };

    serde_json::to_string_pretty(&data).unwrap() + "\n"
}

// ── HELLO-open vector schema ──────────────────────────────────────

#[derive(Serialize)]
struct HelloOpenVectors {
    version: u32,
    #[serde(rename = "_WARNING")]
    warning: String,
    description: String,
    cases: Vec<HelloOpenCase>,
}

#[derive(Serialize)]
struct HelloOpenCase {
    name: String,
    description: String,
    sender_public_hex: String,
    receiver_secret_hex: String,
    sealed_payload_base64: String,
    expected_inner: serde_json::Value,
}

/// Generate the HELLO-open golden vectors JSON string.
///
/// Produces encrypted HELLO messages using fixed keypairs and nonces.
/// Each case provides the sealed payload and the expected decrypted inner JSON.
pub fn generate_hello_open_json() -> String {
    let (sender_sk_bytes, sender_sk, sender_pk) = keypair_from_offset(1);
    let (receiver_sk_bytes, receiver_sk, receiver_pk) = keypair_from_offset(33);
    let (_, eve_sk, eve_pk) = keypair_from_offset(65);

    let sender_pk_bytes: [u8; 32] = *sender_pk.as_bytes();
    let receiver_pk_bytes: [u8; 32] = *receiver_pk.as_bytes();
    let eve_pk_bytes: [u8; 32] = *eve_pk.as_bytes();

    // Fixed nonces (matching TS: nonceC=48, nonceD=72, nonceE=96)
    let nonce_c = make_nonce(48);
    let nonce_d = make_nonce(72);
    let nonce_e = make_nonce(96);

    // HELLO inner payloads — construct as serde_json::Value for expected_inner,
    // then serialize to compact string for encryption plaintext.
    let sender_pk_b64 = STANDARD.encode(sender_pk.as_bytes());
    let receiver_pk_b64 = STANDARD.encode(receiver_pk.as_bytes());
    let eve_pk_b64 = STANDARD.encode(eve_pk.as_bytes());

    let hello_inner_1 = serde_json::json!({
        "type": "hello",
        "version": 1,
        "identityPublicKey": sender_pk_b64,
        "capabilities": ["bolt.file-hash", "bolt.profile-envelope-v1"]
    });
    let hello_inner_2 = serde_json::json!({
        "type": "hello",
        "version": 1,
        "identityPublicKey": receiver_pk_b64,
        "capabilities": ["bolt.profile-envelope-v1"]
    });
    let hello_inner_3 = serde_json::json!({
        "type": "hello",
        "version": 1,
        "identityPublicKey": eve_pk_b64,
        "capabilities": []
    });

    let plain_1 = serde_json::to_string(&hello_inner_1).unwrap();
    let plain_2 = serde_json::to_string(&hello_inner_2).unwrap();
    let plain_3 = serde_json::to_string(&hello_inner_3).unwrap();

    // Sender seals for receiver
    let sealed_1 = seal_with_fixed_nonce(plain_1.as_bytes(), &nonce_c, &receiver_pk, &sender_sk);
    // Receiver seals for sender
    let sealed_2 = seal_with_fixed_nonce(plain_2.as_bytes(), &nonce_d, &sender_pk, &receiver_sk);
    // Eve seals for receiver
    let sealed_3 = seal_with_fixed_nonce(plain_3.as_bytes(), &nonce_e, &receiver_pk, &eve_sk);

    let data = HelloOpenVectors {
        version: 1,
        warning: "TEST FIXTURES ONLY \u{2014} NEVER USE IN PRODUCTION. All keypairs are deterministic test fixtures.".to_string(),
        description: "Web HELLO open/decode golden vectors. Tests openBoxPayload + JSON parse of inner HELLO. Generated by Rust canonical vector generator (bolt-core, vectors feature).".to_string(),
        cases: vec![
            HelloOpenCase {
                name: "hello_open_sender_to_receiver".to_string(),
                description: "Sender encrypts HELLO for receiver with full capabilities".to_string(),
                sender_public_hex: to_hex(&sender_pk_bytes),
                receiver_secret_hex: to_hex(&receiver_sk_bytes),
                sealed_payload_base64: sealed_1,
                expected_inner: hello_inner_1,
            },
            HelloOpenCase {
                name: "hello_open_receiver_to_sender".to_string(),
                description: "Receiver encrypts HELLO for sender with envelope capability only".to_string(),
                sender_public_hex: to_hex(&receiver_pk_bytes),
                receiver_secret_hex: to_hex(&sender_sk_bytes),
                sealed_payload_base64: sealed_2,
                expected_inner: hello_inner_2,
            },
            HelloOpenCase {
                name: "hello_open_eve_to_receiver".to_string(),
                description: "Eve encrypts HELLO for receiver with empty capabilities (legacy peer)".to_string(),
                sender_public_hex: to_hex(&eve_pk_bytes),
                receiver_secret_hex: to_hex(&receiver_sk_bytes),
                sealed_payload_base64: sealed_3,
                expected_inner: hello_inner_3,
            },
        ],
    };

    serde_json::to_string_pretty(&data).unwrap() + "\n"
}

// ── Envelope-open vector schema ───────────────────────────────────

#[derive(Serialize)]
struct EnvelopeOpenVectors {
    version: u32,
    #[serde(rename = "_WARNING")]
    warning: String,
    description: String,
    cases: Vec<EnvelopeOpenCase>,
}

#[derive(Serialize)]
struct EnvelopeOpenCase {
    name: String,
    description: String,
    sender_public_hex: String,
    receiver_secret_hex: String,
    envelope_json: EnvelopeFrameOut,
    expected_inner: serde_json::Value,
}

#[derive(Serialize)]
struct EnvelopeFrameOut {
    #[serde(rename = "type")]
    msg_type: String,
    version: u32,
    encoding: String,
    payload: String,
}

/// Generate the envelope-open golden vectors JSON string.
///
/// Produces encrypted messages wrapped in ProfileEnvelopeV1 frames.
/// Each case provides the envelope JSON and the expected decrypted inner message.
pub fn generate_envelope_open_json() -> String {
    let (sender_sk_bytes, sender_sk, sender_pk) = keypair_from_offset(1);
    let (receiver_sk_bytes, receiver_sk, receiver_pk) = keypair_from_offset(33);

    let sender_pk_bytes: [u8; 32] = *sender_pk.as_bytes();
    let receiver_pk_bytes: [u8; 32] = *receiver_pk.as_bytes();

    // Fixed nonces (matching TS: nonceC=48, nonceD=72, nonceF=120)
    let nonce_c = make_nonce(48);
    let nonce_d = make_nonce(72);
    let nonce_f = make_nonce(120);

    // Inner messages
    let ping_inner = serde_json::json!({
        "type": "ping",
        "ts_ms": 1_700_000_000_000_i64
    });
    let pong_inner = serde_json::json!({
        "type": "pong",
        "ts_ms": 1_700_000_001_000_i64,
        "reply_to_ms": 1_700_000_000_000_i64
    });
    let app_msg_inner = serde_json::json!({
        "type": "app_message",
        "text": "Hello from vector test"
    });

    let plain_ping = serde_json::to_string(&ping_inner).unwrap();
    let plain_pong = serde_json::to_string(&pong_inner).unwrap();
    let plain_app = serde_json::to_string(&app_msg_inner).unwrap();

    // Seal inner messages
    let sealed_ping =
        seal_with_fixed_nonce(plain_ping.as_bytes(), &nonce_c, &receiver_pk, &sender_sk);
    let sealed_pong =
        seal_with_fixed_nonce(plain_pong.as_bytes(), &nonce_d, &sender_pk, &receiver_sk);
    let sealed_app =
        seal_with_fixed_nonce(plain_app.as_bytes(), &nonce_f, &receiver_pk, &sender_sk);

    let data = EnvelopeOpenVectors {
        version: 1,
        warning: "TEST FIXTURES ONLY \u{2014} NEVER USE IN PRODUCTION. All keypairs are deterministic test fixtures.".to_string(),
        description: "ProfileEnvelopeV1 open/decode golden vectors. Tests envelope parsing + openBoxPayload + JSON parse of inner message. Generated by Rust canonical vector generator (bolt-core, vectors feature).".to_string(),
        cases: vec![
            EnvelopeOpenCase {
                name: "envelope_open_ping".to_string(),
                description: "Sender sends ping wrapped in ProfileEnvelopeV1 to receiver".to_string(),
                sender_public_hex: to_hex(&sender_pk_bytes),
                receiver_secret_hex: to_hex(&receiver_sk_bytes),
                envelope_json: EnvelopeFrameOut {
                    msg_type: "profile-envelope".to_string(),
                    version: 1,
                    encoding: "base64".to_string(),
                    payload: sealed_ping,
                },
                expected_inner: ping_inner,
            },
            EnvelopeOpenCase {
                name: "envelope_open_pong".to_string(),
                description: "Receiver sends pong wrapped in ProfileEnvelopeV1 to sender".to_string(),
                sender_public_hex: to_hex(&receiver_pk_bytes),
                receiver_secret_hex: to_hex(&sender_sk_bytes),
                envelope_json: EnvelopeFrameOut {
                    msg_type: "profile-envelope".to_string(),
                    version: 1,
                    encoding: "base64".to_string(),
                    payload: sealed_pong,
                },
                expected_inner: pong_inner,
            },
            EnvelopeOpenCase {
                name: "envelope_open_app_message".to_string(),
                description: "Sender sends app_message wrapped in ProfileEnvelopeV1 to receiver".to_string(),
                sender_public_hex: to_hex(&sender_pk_bytes),
                receiver_secret_hex: to_hex(&receiver_sk_bytes),
                envelope_json: EnvelopeFrameOut {
                    msg_type: "profile-envelope".to_string(),
                    version: 1,
                    encoding: "base64".to_string(),
                    payload: sealed_app,
                },
                expected_inner: app_msg_inner,
            },
        ],
    };

    serde_json::to_string_pretty(&data).unwrap() + "\n"
}
