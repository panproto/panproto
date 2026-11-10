//! Loader for the correctness inventory.
//!
//! The inventory is deliberately loaded without Serde so the test-only
//! reference crate retains its zero normal-dependency boundary. The loader
//! rejects duplicate object members rather than silently normalizing them.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;

/// One semantic law in the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Law {
    /// Globally unique semantic id.
    pub id: String,
    /// Stable book anchor, the law's public reference.
    pub anchor: String,
    /// Concrete reference-item paths, in manifest order.
    pub references: Vec<String>,
}

/// One correctness page and its disjoint reference files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// Reference-semantics and enumerator files owned by this page.
    pub reference_files: Vec<String>,
    /// Laws the page proves; never empty.
    pub laws: Vec<Law>,
}

/// The complete inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Pages keyed by their path below `correctness/`, without `.md`.
    pub pages: BTreeMap<String, Page>,
}

/// Why an inventory cannot be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// The text is not one JSON value.
    Json,
    /// A value has the wrong JSON shape or an unknown enum value.
    Shape(String),
    /// One object repeats a member name.
    DuplicateMember(String),
    /// Two entries violate a global uniqueness or ownership invariant.
    DuplicateValue(String),
}

fn object<'a>(
    value: &'a Value,
    where_: &str,
) -> Result<BTreeMap<&'a str, &'a Value>, ManifestError> {
    let Value::Object(members) = value else {
        return Err(ManifestError::Shape(format!("{where_} is not an object")));
    };
    let mut out = BTreeMap::new();
    for (key, value) in members {
        if out.insert(key.as_str(), value).is_some() {
            return Err(ManifestError::DuplicateMember(format!("{where_}.{key}")));
        }
    }
    Ok(out)
}

fn member<'a>(
    object: &'a BTreeMap<&str, &'a Value>,
    key: &str,
    where_: &str,
) -> Result<&'a Value, ManifestError> {
    object
        .get(key)
        .copied()
        .ok_or_else(|| ManifestError::Shape(format!("{where_} has no {key}")))
}

fn string(value: &Value, where_: &str) -> Result<String, ManifestError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        _ => Err(ManifestError::Shape(format!("{where_} is not a string"))),
    }
}

fn strings(value: &Value, where_: &str) -> Result<Vec<String>, ManifestError> {
    let Value::Array(values) = value else {
        return Err(ManifestError::Shape(format!("{where_} is not an array")));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| string(value, &format!("{where_}[{index}]")))
        .collect()
}

fn laws(value: &Value, where_: &str) -> Result<Vec<Law>, ManifestError> {
    let Value::Array(values) = value else {
        return Err(ManifestError::Shape(format!("{where_} is not an array")));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let place = format!("{where_}[{index}]");
            let item = object(value, &place)?;
            Ok(Law {
                id: string(member(&item, "id", &place)?, &format!("{place}.id"))?,
                anchor: string(member(&item, "anchor", &place)?, &format!("{place}.anchor"))?,
                references: strings(
                    member(&item, "references", &place)?,
                    &format!("{place}.references"),
                )?,
            })
        })
        .collect()
}

/// Load and validate an inventory.
///
/// Besides JSON shape, this checks the invariants that make the inventory a
/// partition: semantic ids, anchors and reference files are globally unique,
/// and every page has at least one law.
///
/// # Errors
///
/// Returns [`ManifestError`] on invalid JSON, a malformed field, a duplicate
/// member, a page without laws, or a violated uniqueness invariant.
pub fn load(text: &str) -> Result<Manifest, ManifestError> {
    let root_value = crate::json::parse(text).ok_or(ManifestError::Json)?;
    let root = object(&root_value, "manifest")?;
    match member(&root, "schema_version", "manifest")? {
        Value::Number(version) if version == "5" => {}
        _ => {
            return Err(ManifestError::Shape(
                "manifest.schema_version is not 5".to_owned(),
            ));
        }
    }
    let page_value = member(&root, "pages", "manifest")?;
    let Value::Object(page_members) = page_value else {
        return Err(ManifestError::Shape(
            "manifest.pages is not an object".to_owned(),
        ));
    };
    let mut pages = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    let mut files = BTreeSet::new();
    for (slug, value) in page_members {
        let place = format!("manifest.pages.{slug}");
        let item = object(value, &place)?;
        let reference_files = strings(
            member(&item, "reference_files", &place)?,
            &format!("{place}.reference_files"),
        )?;
        let page_laws = laws(member(&item, "laws", &place)?, &format!("{place}.laws"))?;
        if page_laws.is_empty() {
            return Err(ManifestError::Shape(format!("{place} has no laws")));
        }
        for file in &reference_files {
            if !files.insert(file.clone()) {
                return Err(ManifestError::DuplicateValue(format!(
                    "reference file {file}"
                )));
            }
        }
        for law in &page_laws {
            if !ids.insert(law.id.clone()) {
                return Err(ManifestError::DuplicateValue(format!("law id {}", law.id)));
            }
            if !anchors.insert(law.anchor.clone()) {
                return Err(ManifestError::DuplicateValue(format!(
                    "anchor {}",
                    law.anchor
                )));
            }
        }
        let page = Page {
            reference_files,
            laws: page_laws,
        };
        if pages.insert(slug.clone(), page).is_some() {
            return Err(ManifestError::DuplicateMember(format!(
                "manifest.pages.{slug}"
            )));
        }
    }
    Ok(Manifest { pages })
}
