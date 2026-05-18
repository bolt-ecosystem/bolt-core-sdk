//! Golden vector generation + determinism tests for core vectors.
//!
//! Generates core test vector JSON files to `test-vectors/core/` and verifies
//! they are deterministic (running twice produces identical output).
//!
//! Parallel to `bolt-btr/tests/vector_golden.rs` for BTR vectors.
//!
//! ## Authority (AC-RC-08)
//! These are the CANONICAL vectors for box-payload, framing, SAS, HELLO-open,
//! and envelope-open. Both Rust and TS test suites consume from this location.

#![cfg(feature = "vectors")]

use bolt_core::vectors;
use std::path::PathBuf;

fn vector_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("test-vectors").join("core")
}

fn write_and_verify(filename: &str, generator: fn() -> String) {
    let path = vector_dir().join(filename);

    // Generate twice and verify determinism
    let first = generator();
    let second = generator();
    assert_eq!(
        first, second,
        "vector generation is not deterministic: {filename}"
    );

    // Write to disk
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &first).unwrap_or_else(|e| {
        panic!("failed to write {}: {e}", path.display());
    });

    // Read back and verify
    let read_back = std::fs::read_to_string(&path).unwrap();
    assert_eq!(read_back, first, "write/read mismatch for {filename}");
}

#[test]
fn generate_box_payload_vectors() {
    write_and_verify(
        "box-payload.vectors.json",
        vectors::generate_box_payload_json,
    );
}

#[test]
fn generate_framing_vectors() {
    write_and_verify("framing.vectors.json", vectors::generate_framing_json);
}

#[test]
fn generate_sas_vectors() {
    write_and_verify("sas.vectors.json", vectors::generate_sas_json);
}

#[test]
fn generate_hello_open_vectors() {
    write_and_verify(
        "web-hello-open.vectors.json",
        vectors::generate_hello_open_json,
    );
}

#[test]
fn generate_envelope_open_vectors() {
    write_and_verify(
        "envelope-open.vectors.json",
        vectors::generate_envelope_open_json,
    );
}

/// Generates all vector files and then verifies they all exist.
#[test]
fn all_core_vector_files_present() {
    let dir = vector_dir();
    let generators: Vec<(&str, fn() -> String)> = vec![
        (
            "box-payload.vectors.json",
            vectors::generate_box_payload_json,
        ),
        ("framing.vectors.json", vectors::generate_framing_json),
        ("sas.vectors.json", vectors::generate_sas_json),
        (
            "web-hello-open.vectors.json",
            vectors::generate_hello_open_json,
        ),
        (
            "envelope-open.vectors.json",
            vectors::generate_envelope_open_json,
        ),
    ];
    for (filename, gen) in &generators {
        let path = dir.join(filename);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, gen()).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", path.display());
        });
    }
    for (filename, _) in &generators {
        let path = dir.join(filename);
        assert!(path.exists(), "missing vector file: {}", path.display());
    }
}
