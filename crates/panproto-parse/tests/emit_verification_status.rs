//! Verification-tier API tests.
//!
//! `ParserRegistry::emit_verification_status` classifies each protocol
//! into one of three tiers (Verified, Generic, Unsupported). Downstream
//! tooling — quivers's transpile pipeline most prominently — uses this
//! API to decide whether to trust `emit_pretty` for a given backend.

#![cfg(feature = "grammars")]

use panproto_parse::{EmitVerificationStatus, ParserRegistry};

#[test]
fn unsupported_for_unregistered_protocol() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("definitely-not-a-language"),
        EmitVerificationStatus::Unsupported
    );
}

#[test]
#[cfg(feature = "lang-python")]
fn verified_for_quivers_python_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("python"),
        EmitVerificationStatus::Verified,
        "python must be Verified — every quivers Python-family backend \
         (NumPyro / Pyro / PyMC / Edward2) emits to this protocol"
    );
}

#[test]
#[cfg(feature = "lang-stan")]
fn verified_for_quivers_stan_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("stan"),
        EmitVerificationStatus::Verified
    );
}

#[test]
#[cfg(feature = "lang-bugs")]
fn verified_for_quivers_bugs_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("bugs"),
        EmitVerificationStatus::Verified
    );
}

#[test]
#[cfg(feature = "lang-jags")]
fn verified_for_quivers_jags_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("jags"),
        EmitVerificationStatus::Verified
    );
}

#[test]
#[cfg(feature = "lang-julia")]
fn verified_for_quivers_julia_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("julia"),
        EmitVerificationStatus::Verified
    );
}

#[test]
#[cfg(feature = "lang-scheme")]
fn verified_for_quivers_scheme_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("scheme"),
        EmitVerificationStatus::Verified
    );
}

#[test]
#[cfg(feature = "lang-javascript")]
fn verified_for_quivers_javascript_backend() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("javascript"),
        EmitVerificationStatus::Verified
    );
}

/// Protocols whose grammar is registered but whose `emit_pretty` has
/// no per-language test in panproto's suite. The API must report
/// `Generic` so downstream tooling can opt into the safety check.
#[test]
#[cfg(feature = "lang-haskell")]
fn generic_for_untested_haskell() {
    let reg = ParserRegistry::new();
    assert_eq!(
        reg.emit_verification_status("haskell"),
        EmitVerificationStatus::Generic
    );
}
