//! Small, bounded XML reference operations.

use std::collections::BTreeMap;

/// Maximum recursively expanded bytes.
pub const MAX_XML_ENTITY_EXPANSION_BYTES: usize = 1_048_576;
/// Maximum recursive entity nesting.
pub const MAX_XML_ENTITY_DEPTH: usize = 16;

/// Why bounded entity expansion is undefined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityError {
    /// An entity name has no declaration.
    Undeclared(String),
    /// The declaration graph is recursive.
    Recursive(String),
    /// Nesting exceeds [`MAX_XML_ENTITY_DEPTH`].
    Depth,
    /// Expanded bytes exceed [`MAX_XML_ENTITY_EXPANSION_BYTES`].
    Bytes,
    /// A reference is syntactically incomplete.
    Malformed,
}

fn expand(
    text: &str,
    declarations: &BTreeMap<String, String>,
    stack: &mut Vec<String>,
    output: &mut String,
) -> Result<(), EntityError> {
    if stack.len() > MAX_XML_ENTITY_DEPTH {
        return Err(EntityError::Depth);
    }
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            return Err(EntityError::Malformed);
        };
        let name = &after[..end];
        let replacement = match name {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "apos" => "'",
            "quot" => "\"",
            _ => declarations
                .get(name)
                .ok_or_else(|| EntityError::Undeclared(name.to_owned()))?,
        };
        if stack.iter().any(|item| item == name) {
            return Err(EntityError::Recursive(name.to_owned()));
        }
        stack.push(name.to_owned());
        expand(replacement, declarations, stack, output)?;
        stack.pop();
        if output.len() > MAX_XML_ENTITY_EXPANSION_BYTES {
            return Err(EntityError::Bytes);
        }
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    (output.len() <= MAX_XML_ENTITY_EXPANSION_BYTES)
        .then_some(())
        .ok_or(EntityError::Bytes)
}

/// Expand internal general entities with explicit byte and depth bounds.
///
/// # Errors
///
/// [`EntityError`] for an undeclared, recursive, malformed or over-budget
/// expansion.
pub fn expand_entities(
    text: &str,
    declarations: &BTreeMap<String, String>,
) -> Result<String, EntityError> {
    let mut output = String::new();
    expand(text, declarations, &mut Vec::new(), &mut output)?;
    Ok(output)
}

/// The CST preserving byte round trip.
pub use crate::value_io::preserve as round_trip_cst;
/// The CST expanded-name function.
pub use crate::value_io::xml_expanded_name as expanded_name_cst;
/// The CST bounded-expansion operation.
pub use expand_entities as bounded_expansion_cst;
