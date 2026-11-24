//! Records of a schema under a protocol: reading them and checking them.
//!
//! The records of a schema `S` under a protocol `P` are the instances of `S`
//! whose validation report under `P` has passed. A committed data set is a
//! list of such records together with the `S` and `P` it lies over, so
//! [`violations`] is the one test of membership, applied when data is staged
//! and when a migration produces it.

use std::path::Path;

use panproto_inst::WInstance;
use panproto_schema::{Protocol, Schema};

use crate::error::VcsError;

/// What a record is held to as it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Check {
    /// Nothing: the record is parsed and left unchecked.
    Skipped,
    /// That it is a record of its schema under its protocol.
    Complete,
    /// That its schema alone does not rule it out ([`schema_violations`]),
    /// when the protocol it was recorded under is not known.
    SchemaAlone,
}

impl Check {
    /// What this check finds wrong with `instance`.
    pub(crate) fn violations(
        self,
        schema: &Schema,
        protocol: &Protocol,
        instance: &WInstance,
    ) -> Vec<String> {
        match self {
            Self::Skipped => Vec::new(),
            Self::Complete => violations(schema, protocol, instance),
            Self::SchemaAlone => schema_violations(schema, instance),
        }
    }
}

/// Everything wrong with `instance` as a record of `schema` under
/// `protocol`.
///
/// No reason when the validation report's status is `Passed`; otherwise the
/// message of every required diagnostic, in report order, then one per
/// required check left undecided. Advisory entries never decide the status
/// and give no reason.
pub(crate) fn violations(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
) -> Vec<String> {
    let report = panproto_inst::validate_wtype(instance, schema, protocol);
    if report.status == panproto_gat::CheckStatus::Passed {
        return Vec::new();
    }
    let required = panproto_inst::validation::Requirement::Required;
    report
        .diagnostics
        .iter()
        .filter(|d| d.requirement == required)
        .map(ToString::to_string)
        .chain(
            report
                .incomplete
                .iter()
                .filter(|i| i.requirement == required)
                .map(ToString::to_string),
        )
        .collect()
}

/// What is wrong with `instance` as a record of `schema` whatever its
/// protocol.
///
/// The message of every required diagnostic of validation under the protocol
/// that gives no vertex kind and no constraint a meaning, other than an
/// attribute the schema does not declare. The checks that protocol leaves
/// undecided are the ones only the record's own protocol decides, and an
/// undeclared attribute was no violation when data without a recorded
/// protocol was staged.
fn schema_violations(schema: &Schema, instance: &WInstance) -> Vec<String> {
    use panproto_inst::validation::{Requirement, Violation};

    panproto_inst::validate_wtype(instance, schema, &Protocol::default())
        .diagnostics
        .iter()
        .filter(|d| {
            d.requirement == Requirement::Required
                && !matches!(d.violation, Violation::UndeclaredAttribute { .. })
        })
        .map(ToString::to_string)
        .collect()
}

/// The vertices a record of `schema` may be rooted at.
///
/// A schema is pointed by a *family* of basepoints, not by one: its
/// declared entries are the sorts at which an instance may be rooted,
/// and which of them a given record belongs to is a property of the
/// record. So a record is read at whichever entry accepts it rather
/// than at a single vertex chosen in advance.
///
/// Falls back to [`panproto_schema::primary_entry`] for a schema that
/// declares no entries, which is that function's own documented
/// fallback for schemas predating declared entries.
fn candidate_roots(schema: &Schema) -> Vec<&panproto_gat::Name> {
    let declared = schema.entry_vertices();
    if declared.is_empty() {
        panproto_schema::primary_entry(schema).into_iter().collect()
    } else {
        declared.iter().collect()
    }
}

/// Read a data file's records as W-type instances of `schema`.
///
/// A top-level JSON array is a set of records; anything else is one
/// record. Each is lifted at one of the schema's entry vertices and held
/// to `check`. A record is accepted at the first entry that both reads it and
/// passes the check; a record no entry accepts is reported with what went
/// wrong at each, since which entry was meant is exactly what is in
/// question.
///
/// Counting bytes instead would report a file which is not JSON at all
/// as holding one record, which is wrong under any policy: nothing would
/// have read the bytes.
pub(crate) fn lift_records(
    data: &[u8],
    path: &Path,
    schema: &Schema,
    protocol: &Protocol,
    check: Check,
) -> Result<Vec<WInstance>, VcsError> {
    let display = path.display().to_string();
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|e| VcsError::DataParseFailed {
            path: display.clone(),
            reason: format!("not JSON: {e}"),
        })?;

    let records: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };

    let roots = candidate_roots(schema);
    if roots.is_empty() {
        return Err(VcsError::DataParseFailed {
            path: display,
            reason: "the schema has no vertex to root a record at".to_owned(),
        });
    }

    let mut instances = Vec::with_capacity(records.len());
    for (i, record) in records.iter().enumerate() {
        let mut accepted = None;
        let mut rejections: Vec<String> = Vec::with_capacity(roots.len());

        for root in &roots {
            match panproto_inst::parse_json(schema, protocol, root.as_str(), record) {
                Err(e) => rejections.push(format!("at {root}: {e}")),
                Ok(instance) => {
                    let reasons = check.violations(schema, protocol, &instance);
                    if reasons.is_empty() {
                        accepted = Some(instance);
                        break;
                    }
                    rejections.push(format!("at {root}: {}", reasons.join("; ")));
                }
            }
        }

        let Some(instance) = accepted else {
            return Err(VcsError::DataValidationFailed {
                path: display,
                reasons: rejections
                    .into_iter()
                    .map(|r| format!("record {i} {r}"))
                    .collect(),
            });
        };
        instances.push(instance);
    }

    Ok(instances)
}
