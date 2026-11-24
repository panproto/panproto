//! Python bindings for the built-in protocol registry.
//!
//! Provides access to the remaining semantic protocol definitions across
//! 9 categories (annotation, api, config, `data_schema`, `data_science`,
//! database, domain, serialization, `web_document`).
//!
//! Programming language and data format protocols are handled by tree-sitter
//! grammars via `panproto-grammars`.

use panproto_core::protocols;
use panproto_core::schema::Protocol;
use pyo3::prelude::*;

use crate::schema::PyProtocol;

fn grammar_protocol(name: &str) -> Option<Protocol> {
    if !panproto_grammars::has_grammar(name) {
        return None;
    }
    Some(Protocol {
        name: name.to_owned(),
        schema_theory: name.to_owned(),
        instance_theory: name.to_owned(),
        ..Default::default()
    })
}

/// Look up a built-in protocol by name.
///
/// Resolves through `panproto_core::protocols::registry`, so every spelling
/// the registry accepts reaches the protocol: the canonical name
/// (`rss-atom`), its underscore form (`rss_atom`), and each alias.
/// Returns ``None`` if the name is not recognized.
fn lookup(name: &str) -> Option<Protocol> {
    protocols::registry::descriptor(name).map(|descriptor| (descriptor.protocol)())
}

/// The protocols schemas were built with through `SchemaBuilder.build()`,
/// keyed by protocol name; the latest build under a name wins.
static BUILT_PROTOCOLS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Protocol>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Record `protocol` as the protocol a schema was built with, so every
/// later operation on a schema naming it reads values under it.
pub fn record_built_protocol(protocol: &Protocol) {
    BUILT_PROTOCOLS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(protocol.name.clone(), protocol.clone());
}

/// The protocol a schema names: the one a schema naming it was last built
/// with, else the built-in or grammar protocol of that name, or, for a name
/// none of these recognizes, a protocol that classifies no vertex kind.
///
/// Records are parsed and serialized under this one protocol, so a value
/// kind the protocol classifies (an `object` vertex holding a record value,
/// say) reads back as the value a lens wrote rather than as structure.
pub fn protocol_for_schema(schema: &panproto_core::schema::Schema) -> Protocol {
    BUILT_PROTOCOLS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&schema.protocol)
        .cloned()
        .or_else(|| lookup(&schema.protocol))
        .or_else(|| grammar_protocol(&schema.protocol))
        .unwrap_or_else(|| Protocol {
            name: schema.protocol.clone(),
            ..Protocol::default()
        })
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// List all built-in protocol names.
#[pyfunction]
pub fn list_builtin_protocols() -> Vec<String> {
    let mut names: Vec<String> = protocols::registry::protocol_names()
        .into_iter()
        .map(String::from)
        .collect();
    for g in panproto_grammars::grammars() {
        if !names.iter().any(|n| n == g.name) {
            names.push(g.name.to_owned());
        }
    }
    names
}

/// Get a built-in protocol by name.
///
/// Parameters
/// ----------
/// name : str
///     Protocol name (e.g., ``"atproto"``, ``"brat"``).
///
/// Returns
/// -------
/// Protocol
///     The protocol specification.
///
/// Raises
/// ------
/// `KeyError`
///     If the protocol name is not recognized.
#[pyfunction]
pub fn get_builtin_protocol(name: &str) -> PyResult<PyProtocol> {
    lookup(name)
        .or_else(|| grammar_protocol(name))
        .map(|p| PyProtocol { inner: p })
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("unknown protocol: {name}")))
}

/// Define a custom protocol from a dict specification.
///
/// Parameters
/// ----------
/// spec : dict
///     Protocol specification with keys: ``name``, ``schema_theory``,
///     ``instance_theory``, ``edge_rules``, ``obj_kinds``,
///     ``constraint_sorts``.
///
/// Returns
/// -------
/// Protocol
///     The custom protocol.
#[pyfunction]
pub fn define_protocol(spec: &Bound<'_, pyo3::types::PyAny>) -> PyResult<PyProtocol> {
    let protocol: Protocol = crate::convert::from_python(spec)?;
    Ok(PyProtocol { inner: protocol })
}

/// Register protocol functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(list_builtin_protocols, parent)?)?;
    parent.add_function(wrap_pyfunction!(get_builtin_protocol, parent)?)?;
    parent.add_function(wrap_pyfunction!(define_protocol, parent)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn semantic_protocol_prefix_is_the_registry_listing() {
        let listed = list_builtin_protocols();
        let expected: Vec<String> = protocols::registry::protocol_names()
            .into_iter()
            .map(String::from)
            .collect();

        assert_eq!(&listed[..expected.len()], expected.as_slice());
        assert_eq!(
            listed.len(),
            listed.iter().collect::<HashSet<_>>().len(),
            "semantic protocols and appended grammars must not duplicate names"
        );
    }

    #[test]
    fn a_schema_resolves_to_the_protocol_it_was_built_with()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = Protocol {
            name: "py-built-protocol-resolution".into(),
            obj_kinds: vec!["object".into()],
            value_kinds: std::collections::BTreeMap::from([(
                "object".to_owned(),
                panproto_core::schema::ScalarKind::Any,
            )]),
            ..Protocol::default()
        };
        let schema = panproto_core::schema::SchemaBuilder::new(&protocol)
            .vertex("r", "object", None)
            .and_then(panproto_core::schema::SchemaBuilder::build)?;
        assert_eq!(
            protocol_for_schema(&schema).value_kinds,
            std::collections::BTreeMap::default()
        );
        record_built_protocol(&protocol);
        assert_eq!(protocol_for_schema(&schema), protocol);
        Ok(())
    }
}
