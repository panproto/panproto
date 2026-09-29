//! The codec registry and the protocol registry agree on names.
//!
//! The protocol registry of `panproto-protocols` is the one record of a
//! protocol's canonical name and aliases, and it treats `_` and `-` as the
//! same separator. A codec registered under any other spelling is a codec
//! no listing can reach, so every codec is registered under the canonical
//! name of its protocol and every spelling the protocol registry accepts
//! reaches it.

#![allow(clippy::expect_used)]

use panproto_io::default_registry;
use panproto_protocols::registry::descriptor;

/// Every spelling of `name` that the protocol registry resolves: the
/// canonical name, its underscore form, and each alias in both forms.
fn spellings(name: &str) -> Vec<String> {
    let d = descriptor(name).expect("a codec name resolves to a protocol");
    let mut out = Vec::new();
    for base in std::iter::once(d.name).chain(d.aliases.iter().copied()) {
        out.push(base.to_owned());
        out.push(base.replace('-', "_"));
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[test]
fn every_codec_name_is_a_protocol_name() {
    let registry = default_registry();
    for name in registry.protocol_names() {
        let d = descriptor(name)
            .unwrap_or_else(|| panic!("codec {name:?} names no protocol in the registry"));
        assert_eq!(
            d.name, name,
            "codec {name:?} is registered under a spelling other than its protocol's \
             canonical name {:?}",
            d.name
        );
    }
}

#[test]
fn every_codec_resolves_by_each_spelling_of_its_protocol() {
    let registry = default_registry();
    for name in registry.protocol_names() {
        let native = registry.native_repr(name).expect("a listed name resolves");
        for spelling in spellings(name) {
            assert_eq!(
                registry
                    .native_repr(&spelling)
                    .unwrap_or_else(|e| panic!("{spelling:?} does not reach {name:?}: {e}")),
                native,
                "{spelling:?} reaches a different codec than {name:?}"
            );
            assert_eq!(
                registry.supports_table(&spelling),
                registry.supports_table(name),
                "{spelling:?} and {name:?} disagree on the table entries"
            );
        }
    }
}

/// The nine protocols whose codecs used to be registered under underscore
/// names answer to their canonical names.
#[test]
fn the_canonical_names_of_the_underscore_registrations_resolve() {
    let registry = default_registry();
    for name in [
        "k8s-crd",
        "edi-x12",
        "swift-mt",
        "iso-space",
        "laf-graf",
        "web-annotation",
        "msgpack-schema",
        "rss-atom",
        "vcard-ical",
    ] {
        assert!(
            registry.native_repr(name).is_ok(),
            "{name:?} does not resolve"
        );
        assert!(
            registry.protocol_names().any(|n| n == name),
            "{name:?} is not listed"
        );
    }
}

/// `uima` is listed under its canonical name and reached through its
/// `uima-cas` alias, the name its `Protocol` carries.
#[test]
fn the_uima_codec_answers_to_its_alias() {
    let registry = default_registry();
    let canonical = registry.native_repr("uima").expect("uima resolves");
    for alias in ["uima-cas", "uima_cas"] {
        assert_eq!(
            registry.native_repr(alias).expect("the alias resolves"),
            canonical
        );
    }
}
