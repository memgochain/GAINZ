//! Test-only crate. The suite lives in `tests/sandbox_test.rs`; this library
//! target exists so Cargo has something to build for the package itself.

/// Path to the contract crate, resolved at compile time.
pub fn contract_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../guard_vault")
}
