//! Loader for the schema-v4 correctness inventory.
//!
//! The inventory is deliberately loaded without Serde so the test-only
//! reference crate retains its zero normal-dependency boundary. The loader
//! rejects duplicate object members, unknown gates and scope entries that
//! cite an unknown law anchor rather than silently normalizing any of them.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;

/// A staged correctness gate, in the order the gates are passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    /// Required of a release candidate.
    Candidate,
    /// Required once the edit-lens inventory joins the candidate.
    Extended,
    /// Required once the segment-execution inventory joins the candidate.
    Execution,
    /// Required of the release itself.
    Release,
}

/// One semantic law in the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Law {
    /// Globally unique semantic id.
    pub id: String,
    /// Stable book anchor, the law's public reference.
    pub anchor: String,
    /// Concrete reference-item paths, in manifest order.
    pub references: Vec<String>,
    /// First gate at which the law block is required.
    pub due_gate: Gate,
}

/// Completion state of a page's law inventory and verification scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryStatus {
    /// Every law is listed and the page's verification is complete.
    Complete,
    /// Every law is listed, but the page retains unresolved scope.
    Partial,
    /// A later independent audit must enumerate the laws and verify the scope.
    Pending,
}

/// One correctness page and its disjoint reference files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// Inventory status.
    pub inventory_status: InventoryStatus,
    /// Reference-semantics and enumerator files owned by this page.
    pub reference_files: Vec<String>,
    /// Laws, empty exactly when the inventory is pending.
    pub laws: Vec<Law>,
    /// Scope that a partial or pending inventory must still verify: a law
    /// anchor of this page, or a description of a part of the page that
    /// has no stated laws yet.
    pub open_scope: Vec<String>,
    /// First gate at which the page inventory is required.
    pub due_gate: Gate,
    /// Explicit sublaws deferred to the release gate. An entry that is a
    /// law anchor names that law; any other entry describes the sublaws.
    pub release_scope: Vec<String>,
}

/// The complete schema-v4 inventory.
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
    /// A scope entry cites a law anchor that no law of the inventory has.
    UnknownAnchor(String),
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

fn gate(value: &Value, where_: &str) -> Result<Gate, ManifestError> {
    match string(value, where_)?.as_str() {
        "candidate" => Ok(Gate::Candidate),
        "extended" => Ok(Gate::Extended),
        "execution" => Ok(Gate::Execution),
        "release" => Ok(Gate::Release),
        other => Err(ManifestError::Shape(format!(
            "{where_} has unknown gate {other}"
        ))),
    }
}

fn optional_strings(
    object: &BTreeMap<&str, &Value>,
    key: &str,
    where_: &str,
) -> Result<Vec<String>, ManifestError> {
    object.get(key).map_or_else(
        || Ok(Vec::new()),
        |value| strings(value, &format!("{where_}.{key}")),
    )
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
                due_gate: gate(
                    member(&item, "due_gate", &place)?,
                    &format!("{place}.due_gate"),
                )?,
            })
        })
        .collect()
}

fn validate_inventory_status(
    place: &str,
    status: InventoryStatus,
    laws: &[Law],
    open_scope_present: bool,
    open_scope: &[String],
) -> Result<(), ManifestError> {
    let message = match status {
        InventoryStatus::Complete if laws.is_empty() || open_scope_present => {
            Some("complete inventory must have laws and no open_scope")
        }
        InventoryStatus::Partial
            if laws.is_empty() || !open_scope_present || open_scope.is_empty() =>
        {
            Some("partial inventory must have laws and an open_scope")
        }
        InventoryStatus::Pending
            if !laws.is_empty() || !open_scope_present || open_scope.is_empty() =>
        {
            Some("pending inventory must have no laws and an open_scope")
        }
        _ => None,
    };
    message.map_or(Ok(()), |message| {
        Err(ManifestError::Shape(format!("{place} {message}")))
    })
}

/// Load and validate a schema-v4 inventory.
///
/// Besides JSON shape, this checks the invariants that make the inventory a
/// partition: semantic ids, anchors and reference files are globally unique;
/// complete pages have laws and no unresolved scope; partial pages have laws
/// and a nonempty unresolved scope; pending pages have no laws and a nonempty
/// scope. Every open-scope or release-scope entry that has the shape of a
/// law anchor (`L-` followed by the rest of the anchor) must be the anchor of
/// a law in the inventory.
///
/// # Errors
///
/// Returns [`ManifestError`] on invalid JSON, a malformed field, a duplicate
/// member, a violated uniqueness/partition invariant, or a scope entry that
/// cites an unknown anchor.
pub fn load(text: &str) -> Result<Manifest, ManifestError> {
    let root_value = crate::json::parse(text).ok_or(ManifestError::Json)?;
    let root = object(&root_value, "manifest")?;
    match member(&root, "schema_version", "manifest")? {
        Value::Number(version) if version == "4" => {}
        _ => {
            return Err(ManifestError::Shape(
                "manifest.schema_version is not 4".to_owned(),
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
        let inventory_status = match string(
            member(&item, "inventory_status", &place)?,
            &format!("{place}.inventory_status"),
        )?
        .as_str()
        {
            "complete" => InventoryStatus::Complete,
            "partial" => InventoryStatus::Partial,
            "pending" => InventoryStatus::Pending,
            other => {
                return Err(ManifestError::Shape(format!(
                    "{place}.inventory_status has unknown value {other}"
                )));
            }
        };
        let reference_files = strings(
            member(&item, "reference_files", &place)?,
            &format!("{place}.reference_files"),
        )?;
        let page_laws = laws(member(&item, "laws", &place)?, &format!("{place}.laws"))?;
        let open_scope_present = item.contains_key("open_scope");
        let open_scope = optional_strings(&item, "open_scope", &place)?;
        validate_inventory_status(
            &place,
            inventory_status,
            &page_laws,
            open_scope_present,
            &open_scope,
        )?;
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
            inventory_status,
            reference_files,
            laws: page_laws,
            open_scope,
            due_gate: gate(
                member(&item, "due_gate", &place)?,
                &format!("{place}.due_gate"),
            )?,
            release_scope: optional_strings(&item, "release_scope", &place)?,
        };
        if pages.insert(slug.clone(), page).is_some() {
            return Err(ManifestError::DuplicateMember(format!(
                "manifest.pages.{slug}"
            )));
        }
    }
    for (slug, page) in &pages {
        for entry in page.open_scope.iter().chain(&page.release_scope) {
            if entry.starts_with("L-")
                && !entry.contains(char::is_whitespace)
                && !anchors.contains(entry)
            {
                return Err(ManifestError::UnknownAnchor(format!(
                    "manifest.pages.{slug} cites {entry}"
                )));
            }
        }
    }
    Ok(Manifest { pages })
}
