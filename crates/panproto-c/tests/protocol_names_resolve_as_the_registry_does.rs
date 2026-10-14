//! A protocol name reaches the same protocol through the C ABI as through the
//! registry.
//!
//! `panproto_protocols::registry` is the one record of the built-in protocols:
//! each has a canonical name (`rss-atom`, `json-schema`), possibly some aliases,
//! and the underscore spelling of either is accepted as the same name. A schema
//! names its protocol by any of those, and the ABI resolves that name both when
//! a host asks for a protocol (`pp_registry_get_builtin`) and when an operation
//! needs the protocol of a schema it was handed. A spelling the registry knows
//! and the ABI does not makes the second fall back to a default protocol with
//! none of the real one's rules, silently.
//!
//! Every spelling of every registry protocol is checked against both paths.

use panproto_c::api::helpers::protocol_for_schema;
use panproto_c::api::{pp_buf_free, pp_registry_get_builtin, pp_registry_list_builtin};
use panproto_c::canonical::decode;
use panproto_c::error::PpStatus;
use panproto_core::protocols::registry;
use panproto_core::schema::{Protocol, Schema};
use safer_ffi::prelude::*;

/// Every spelling the registry accepts for `descriptor`: the canonical name,
/// each alias, and the underscore form of each.
fn spellings(descriptor: &registry::ProtocolDescriptor) -> Vec<String> {
    let mut names: Vec<String> = std::iter::once(descriptor.name)
        .chain(descriptor.aliases.iter().copied())
        .flat_map(|name| [name.to_owned(), name.replace('-', "_")])
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The protocol a host gets for `name`, or `None` if the ABI refuses it.
fn through_the_abi(name: &str) -> Option<Protocol> {
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let status = pp_registry_get_builtin(name.as_bytes().into(), &mut out);
    let protocol = (status == PpStatus::Ok as i32).then(|| {
        decode::<Protocol>(&out).unwrap_or_else(|e| panic!("{name}: undecodable protocol: {e}"))
    });
    pp_buf_free(out);
    protocol
}

#[test]
fn every_registry_spelling_reaches_the_registry_protocol() {
    let mut wrong: Vec<String> = Vec::new();
    for descriptor in registry::descriptors() {
        let expected = (descriptor.protocol)();
        for name in spellings(descriptor) {
            match through_the_abi(&name) {
                Some(found) if found == expected => {}
                Some(found) => wrong.push(format!(
                    "pp_registry_get_builtin({name:?}) answered {:?}, the registry {:?}",
                    found.name, expected.name
                )),
                None => wrong.push(format!("pp_registry_get_builtin({name:?}) refused it")),
            }

            let schema = Schema {
                protocol: name.clone(),
                ..Default::default()
            };
            let resolved = protocol_for_schema(&schema);
            if resolved != expected {
                wrong.push(format!(
                    "a schema naming {name:?} resolved to {:?} (theory {:?}), the registry to {:?}",
                    resolved.name, resolved.schema_theory, expected.name
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_listed_name_is_a_registry_name() {
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_registry_list_builtin(&mut out), PpStatus::Ok as i32);
    let listed: Vec<String> =
        decode(&out).unwrap_or_else(|e| panic!("undecodable protocol listing: {e}"));
    pp_buf_free(out);

    assert_eq!(
        listed.len(),
        registry::descriptors().len(),
        "the ABI lists {} protocols and the registry has {}",
        listed.len(),
        registry::descriptors().len()
    );
    let unknown: Vec<&String> = listed
        .iter()
        .filter(|name| registry::descriptor(name).is_none())
        .collect();
    assert!(
        unknown.is_empty(),
        "listed but not in the registry: {unknown:?}"
    );
}

#[test]
fn a_name_the_registry_does_not_know_is_refused() {
    assert!(through_the_abi("not-a-protocol").is_none());
    let schema = Schema {
        protocol: "not-a-protocol".into(),
        ..Default::default()
    };
    let fallback = protocol_for_schema(&schema);
    assert_eq!(fallback.name, "not-a-protocol");
    assert_eq!(fallback.schema_theory, "ThGraph");
}
