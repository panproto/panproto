//! Loader for the schema-v2 correctness inventory.
//!
//! The inventory is deliberately loaded without Serde so the test-only
//! reference crate retains its zero normal-dependency boundary. The loader
//! rejects duplicate object members and unknown gates rather than silently
//! normalizing either one.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;

/// A staged correctness gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    /// Before phase 3 starts.
    Phase3,
    /// Before phase 3b starts.
    Phase3b,
    /// Before phase 4 starts.
    Phase4,
    /// At the final gate.
    Final,
}

/// One exact locator into the normative contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locator {
    /// Contract section.
    pub section: String,
    /// Clause within the section.
    pub clause: String,
}

/// One semantic law in the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Law {
    /// Globally unique semantic id.
    pub id: String,
    /// Stable book anchor.
    pub anchor: String,
    /// Exact contract locators, in manifest order.
    pub contract: Vec<Locator>,
    /// Concrete reference-item paths, in manifest order.
    pub references: Vec<String>,
    /// First gate at which the law block is required.
    pub due_gate: Gate,
}

/// Whether a page's law inventory is complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryStatus {
    /// Every law is listed.
    Complete,
    /// A later independent audit must enumerate the laws.
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
    /// Scope that a pending inventory must audit.
    pub contract_scope: Vec<Locator>,
    /// First gate at which the page inventory is required.
    pub due_gate: Gate,
    /// Explicit sublaws deferred to the final gate.
    pub final_gate_scope: Vec<Locator>,
}

/// The complete schema-v2 inventory.
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

fn gate(value: &Value, where_: &str) -> Result<Gate, ManifestError> {
    match string(value, where_)?.as_str() {
        "phase3" => Ok(Gate::Phase3),
        "phase3b" => Ok(Gate::Phase3b),
        "phase4" => Ok(Gate::Phase4),
        "final" => Ok(Gate::Final),
        other => Err(ManifestError::Shape(format!(
            "{where_} has unknown gate {other}"
        ))),
    }
}

fn locators(value: &Value, where_: &str) -> Result<Vec<Locator>, ManifestError> {
    let Value::Array(values) = value else {
        return Err(ManifestError::Shape(format!("{where_} is not an array")));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let place = format!("{where_}[{index}]");
            let item = object(value, &place)?;
            Ok(Locator {
                section: string(
                    member(&item, "section", &place)?,
                    &format!("{place}.section"),
                )?,
                clause: string(member(&item, "clause", &place)?, &format!("{place}.clause"))?,
            })
        })
        .collect()
}

fn optional_locators(
    object: &BTreeMap<&str, &Value>,
    key: &str,
    where_: &str,
) -> Result<Vec<Locator>, ManifestError> {
    object.get(key).map_or_else(
        || Ok(Vec::new()),
        |value| locators(value, &format!("{where_}.{key}")),
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
                contract: locators(
                    member(&item, "contract", &place)?,
                    &format!("{place}.contract"),
                )?,
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

/// Load and validate a schema-v2 inventory.
///
/// Besides JSON shape, this checks the invariants that make the inventory a
/// partition: semantic ids, anchors and reference files are globally unique;
/// complete pages have laws and no pending scope; pending pages have no laws
/// and a nonempty scope.
///
/// # Errors
///
/// Returns [`ManifestError`] on invalid JSON, a malformed field, a duplicate
/// member, or a violated uniqueness/partition invariant.
pub fn load(text: &str) -> Result<Manifest, ManifestError> {
    let root_value = crate::json::parse(text).ok_or(ManifestError::Json)?;
    let root = object(&root_value, "manifest")?;
    match member(&root, "schema_version", "manifest")? {
        Value::Number(version) if version == "2" => {}
        _ => {
            return Err(ManifestError::Shape(
                "manifest.schema_version is not 2".to_owned(),
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
        let contract_scope = optional_locators(&item, "contract_scope", &place)?;
        match inventory_status {
            InventoryStatus::Complete if page_laws.is_empty() || !contract_scope.is_empty() => {
                return Err(ManifestError::Shape(format!(
                    "{place} complete inventory must have laws and no contract_scope"
                )));
            }
            InventoryStatus::Pending if !page_laws.is_empty() || contract_scope.is_empty() => {
                return Err(ManifestError::Shape(format!(
                    "{place} pending inventory must have no laws and a contract_scope"
                )));
            }
            _ => {}
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
            inventory_status,
            reference_files,
            laws: page_laws,
            contract_scope,
            due_gate: gate(
                member(&item, "due_gate", &place)?,
                &format!("{place}.due_gate"),
            )?,
            final_gate_scope: optional_locators(&item, "final_gate_scope", &place)?,
        };
        if pages.insert(slug.clone(), page).is_some() {
            return Err(ManifestError::DuplicateMember(format!(
                "manifest.pages.{slug}"
            )));
        }
    }
    Ok(Manifest { pages })
}
