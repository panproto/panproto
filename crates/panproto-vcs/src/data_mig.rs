//! Data migration engine for the VCS.
//!
//! Migrates data instances between schema versions using the lens
//! infrastructure. Complements are stored as VCS objects so backward
//! migration never loses data.

use panproto_inst::WInstance;
use panproto_schema::{Protocol, Schema};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{ComplementObject, CstComplementObject, DataSetObject, Object};
use crate::store::Store;

/// A data set that is stale relative to the current schema.
#[derive(Debug, Clone)]
pub struct StaleData {
    /// The object ID of the stale data set.
    pub data_id: ObjectId,
    /// The schema ID the data was written against.
    pub data_schema_id: ObjectId,
    /// The schema ID at HEAD.
    pub head_schema_id: ObjectId,
}

/// Build a default protocol suitable for lens generation.
///
/// When no protocol definition is stored in the repository, this
/// constructs a minimal protocol from the schema's protocol name.
fn default_protocol(name: &str) -> Protocol {
    Protocol {
        name: name.into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// Migrate a data set forward through a schema change.
///
/// Produces a new `DataSetObject` at the target schema and a
/// `ComplementObject` for backward migration. Both are stored in
/// the object store.
///
/// Returns `(new_data_id, complement_id)`.
///
/// # Errors
///
/// Returns `VcsError::DataMigrationFailed` if lens generation,
/// deserialization, or migration fails.
/// Returns `VcsError::TypeMismatch` if the object is not a `DataSet`.
pub fn migrate_forward(
    store: &mut dyn Store,
    data_id: ObjectId,
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &Protocol,
) -> Result<(ObjectId, ObjectId), VcsError> {
    // 1. Load the data set
    let data_obj = match store.get(&data_id)? {
        Object::DataSet(ds) => ds,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "DataSet".into(),
                got: other.type_name().into(),
            });
        }
    };

    // 2. Deserialize the instances
    let instances: Vec<WInstance> =
        rmp_serde::from_slice(&data_obj.data).map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("deserialize: {e}"),
        })?;

    // 3. Generate lens between schemas
    let config = panproto_lens::AutoLensConfig::default();
    let result =
        panproto_lens::auto_generate(src_schema, tgt_schema, protocol, &config).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens generation: {e}"),
            }
        })?;

    // 4. Apply get to each instance, collecting views and complements
    let mut migrated_instances = Vec::new();
    let mut all_complements = Vec::new();
    for instance in &instances {
        let (view, complement) = panproto_lens::get(&result.lens, instance).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens get: {e}"),
            }
        })?;
        migrated_instances.push(view);
        all_complements.push(complement);
    }

    // 5. Store new DataSetObject
    let tgt_schema_id = crate::hash::hash_schema(tgt_schema)?;
    let new_data = DataSetObject {
        schema_id: tgt_schema_id,
        data: rmp_serde::to_vec(&migrated_instances).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("serialize: {e}"),
            }
        })?,
        record_count: migrated_instances.len() as u64,
    };
    let new_data_id = store.put(&Object::DataSet(new_data))?;

    // 6. Store ComplementObject
    let comp = ComplementObject {
        migration_id: data_id,
        data_id,
        complement: rmp_serde::to_vec(&all_complements).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("serialize complement: {e}"),
            }
        })?,
    };
    let complement_id = store.put(&Object::Complement(comp))?;

    Ok((new_data_id, complement_id))
}

/// Migrate a data set backward using a stored complement.
///
/// # Errors
///
/// Returns `VcsError::DataMigrationFailed` if lens generation,
/// deserialization, or migration fails.
/// Returns `VcsError::TypeMismatch` if the object is not the expected type.
pub fn migrate_backward(
    store: &mut dyn Store,
    data_id: ObjectId,
    complement_id: ObjectId,
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &Protocol,
) -> Result<ObjectId, VcsError> {
    // 1. Load data and complement
    let data_obj = match store.get(&data_id)? {
        Object::DataSet(ds) => ds,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "DataSet".into(),
                got: other.type_name().into(),
            });
        }
    };
    let comp_obj = match store.get(&complement_id)? {
        Object::Complement(c) => c,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "Complement".into(),
                got: other.type_name().into(),
            });
        }
    };

    // 2. Deserialize
    let instances: Vec<WInstance> =
        rmp_serde::from_slice(&data_obj.data).map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("deserialize data: {e}"),
        })?;
    let complements: Vec<panproto_lens::Complement> =
        rmp_serde::from_slice(&comp_obj.complement).map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("deserialize complement: {e}"),
        })?;

    // 3. Generate lens (same direction as forward -- we use put for backward)
    let config = panproto_lens::AutoLensConfig::default();
    let result =
        panproto_lens::auto_generate(src_schema, tgt_schema, protocol, &config).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens generation: {e}"),
            }
        })?;

    // 4. Apply put to each instance with its complement
    let mut restored = Vec::new();
    for (inst, comp) in instances.iter().zip(complements.iter()) {
        let r = panproto_lens::put(&result.lens, inst, comp).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens put: {e}"),
            }
        })?;
        restored.push(r);
    }

    // 5. Store restored DataSetObject
    let src_schema_id = crate::hash::hash_schema(src_schema)?;
    let restored_data = DataSetObject {
        schema_id: src_schema_id,
        data: rmp_serde::to_vec(&restored).map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("serialize: {e}"),
        })?,
        record_count: restored.len() as u64,
    };
    let restored_id = store.put(&Object::DataSet(restored_data))?;

    Ok(restored_id)
}

/// Check which data sets in a commit are stale relative to its schema.
///
/// A data set is stale when its `schema_id` differs from the commit's
/// `schema_id`, meaning the data was written against an older schema
/// version and needs migration.
///
/// # Errors
///
/// Returns `VcsError` if any data object cannot be loaded from the store.
pub fn detect_staleness(
    store: &dyn Store,
    commit: &crate::object::CommitObject,
) -> Result<Vec<StaleData>, VcsError> {
    let mut stale = Vec::new();
    for data_id in &commit.data_ids {
        let Object::DataSet(data_obj) = store.get(data_id)? else {
            continue;
        };
        if data_obj.schema_id != commit.schema_id {
            stale.push(StaleData {
                data_id: *data_id,
                data_schema_id: data_obj.schema_id,
                head_schema_id: commit.schema_id,
            });
        }
    }
    Ok(stale)
}

/// Migrate all JSON files in a directory from one schema to another.
///
/// Each `.json` file is parsed as an instance of `src_schema`, migrated
/// forward through a lens to `tgt_schema`, and written back in place.
///
/// # Errors
///
/// Returns `VcsError::DataMigrationFailed` if lens generation, parsing,
/// or migration fails. Returns `VcsError::IoError` on filesystem errors.
pub fn migrate_data_directory(
    store: &mut dyn Store,
    data_dir: &std::path::Path,
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &Protocol,
) -> Result<(), VcsError> {
    let config = panproto_lens::AutoLensConfig::default();
    let result =
        panproto_lens::auto_generate(src_schema, tgt_schema, protocol, &config).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens: {e}"),
            }
        })?;

    for entry in std::fs::read_dir(data_dir).map_err(|e| VcsError::IoError(e.to_string()))? {
        let entry = entry.map_err(|e| VcsError::IoError(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).map_err(|e| VcsError::IoError(e.to_string()))?,
        )
        .map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("parse {}: {e}", path.display()),
        })?;

        // Parse, migrate, write back
        let root = infer_root(src_schema);
        let instance = panproto_inst::parse_json(src_schema, &root, &data).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("parse instance: {e}"),
            }
        })?;

        let (view, _complement) = panproto_lens::get(&result.lens, &instance).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens get: {e}"),
            }
        })?;

        // Store migrated data as a DataSetObject for complement tracking
        let tgt_schema_id = crate::hash::hash_schema(tgt_schema)?;
        let migrated_data = DataSetObject {
            schema_id: tgt_schema_id,
            data: rmp_serde::to_vec(&vec![view.clone()]).map_err(|e| {
                VcsError::DataMigrationFailed {
                    reason: format!("serialize: {e}"),
                }
            })?,
            record_count: 1,
        };
        store.put(&Object::DataSet(migrated_data))?;

        let output = panproto_inst::to_json(tgt_schema, &view);
        let pretty =
            serde_json::to_string_pretty(&output).map_err(|e| VcsError::DataMigrationFailed {
                reason: format!("serialize: {e}"),
            })?;

        std::fs::write(&path, pretty).map_err(|e| VcsError::IoError(e.to_string()))?;
    }

    Ok(())
}

/// Infer the root vertex of a schema.
///
/// Finds a vertex with no incoming edges; falls back to the first vertex
/// in iteration order if all vertices have incoming edges.
fn infer_root(schema: &Schema) -> String {
    for id in schema.vertices.keys() {
        let has_incoming = schema
            .incoming
            .get(id)
            .is_some_and(|edges| !edges.is_empty());
        if !has_incoming {
            return id.to_string();
        }
    }
    schema
        .vertices
        .keys()
        .next()
        .map_or_else(|| "root".to_owned(), std::string::ToString::to_string)
}

/// Construct a protocol definition from a schema's protocol name.
///
/// Returns a minimal protocol suitable for lens generation. VCS
/// callers should prefer loading a stored `Protocol` object when
/// available.
#[must_use]
pub fn protocol_for_schema(schema: &Schema) -> Protocol {
    default_protocol(&schema.protocol)
}

// ── CST complement pass-through ───────────────────────────────────────

/// Pass a CST complement through a schema migration.
///
/// CST complements are orthogonal to schema migrations: they capture
/// formatting information that is independent of schema structure.
/// During forward migration, the complement is re-stored with its
/// `data_id` updated to point to the new (migrated) data set.
///
/// # Errors
///
/// Returns `VcsError` if loading or storing fails.
pub fn pass_through_cst_complement(
    store: &mut dyn Store,
    old_cst_complement_id: ObjectId,
    new_data_id: ObjectId,
) -> Result<ObjectId, VcsError> {
    let old_comp = match store.get(&old_cst_complement_id)? {
        Object::CstComplement(c) => c,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "CstComplement".into(),
                got: other.type_name().into(),
            });
        }
    };

    // Re-store with updated data_id
    let new_comp = CstComplementObject {
        data_id: new_data_id,
        cst_complement: old_comp.cst_complement,
    };
    store.put(&Object::CstComplement(new_comp))
}

/// Store a CST complement alongside a data set in the VCS.
///
/// This is called during the initial ingest of data with format
/// preservation enabled. The complement captures the full CST Schema
/// for format-preserving emission later.
///
/// # Errors
///
/// Returns `VcsError` if serialization or storage fails.
pub fn store_cst_complement(
    store: &mut dyn Store,
    data_id: ObjectId,
    cst_complement_bytes: Vec<u8>,
) -> Result<ObjectId, VcsError> {
    let obj = CstComplementObject {
        data_id,
        cst_complement: cst_complement_bytes,
    };
    store.put(&Object::CstComplement(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_store::MemStore;
    use crate::object::CommitObject;
    use panproto_gat::Name;
    use panproto_schema::Vertex;
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)]) -> Schema {
        let mut vert_map = HashMap::new();
        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn make_commit(schema_id: ObjectId, data_ids: Vec<ObjectId>) -> CommitObject {
        CommitObject::builder(schema_id, "test", "test", "test")
            .timestamp(0)
            .data_ids(data_ids)
            .build()
    }

    #[test]
    fn staleness_detection() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let schema_old = make_schema(&[("a", "object")]);
        let schema_new = make_schema(&[("a", "object"), ("b", "string")]);

        let old_schema_id = crate::hash::hash_schema(&schema_old)?;
        let new_schema_id = crate::hash::hash_schema(&schema_new)?;

        // Store a data set against the old schema
        let ds = DataSetObject {
            schema_id: old_schema_id,
            data: vec![],
            record_count: 0,
        };
        let data_id = store.put(&Object::DataSet(ds))?;

        // Commit references the new schema but the old data
        let commit = make_commit(new_schema_id, vec![data_id]);

        let stale = detect_staleness(&store, &commit)?;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].data_id, data_id);
        assert_eq!(stale[0].data_schema_id, old_schema_id);
        assert_eq!(stale[0].head_schema_id, new_schema_id);
        Ok(())
    }

    #[test]
    fn staleness_detection_no_stale() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let schema = make_schema(&[("a", "object")]);
        let schema_id = crate::hash::hash_schema(&schema)?;

        let ds = DataSetObject {
            schema_id,
            data: vec![],
            record_count: 0,
        };
        let data_id = store.put(&Object::DataSet(ds))?;

        let commit = make_commit(schema_id, vec![data_id]);

        let stale = detect_staleness(&store, &commit)?;
        assert!(stale.is_empty());
        Ok(())
    }

    #[test]
    fn empty_data_set_staleness() -> Result<(), Box<dyn std::error::Error>> {
        let store = MemStore::new();
        let schema_id = ObjectId::ZERO;
        let commit = make_commit(schema_id, vec![]);

        let stale = detect_staleness(&store, &commit)?;
        assert!(stale.is_empty());
        Ok(())
    }

    #[test]
    fn type_mismatch_on_non_dataset() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let schema = make_schema(&[("a", "object")]);
        let schema_id = store.put(&Object::Schema(Box::new(schema.clone())))?;

        let protocol = default_protocol("test");
        let result = migrate_forward(&mut store, schema_id, &schema, &schema, &protocol);

        assert!(result.is_err());
        if let Err(VcsError::TypeMismatch { expected, got }) = result {
            assert_eq!(expected, "DataSet");
            assert_eq!(got, "schema");
        } else {
            panic!("expected TypeMismatch error variant");
        }
        Ok(())
    }

    #[test]
    fn infer_root_finds_vertex_without_incoming() {
        use panproto_schema::Edge;

        let mut schema = make_schema(&[("root", "object"), ("child", "string")]);

        let edge = Edge {
            src: "root".into(),
            tgt: "child".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        schema.edges.insert(edge.clone(), Name::from("prop"));
        schema
            .incoming
            .entry(Name::from("child"))
            .or_default()
            .push(edge);

        let root = infer_root(&schema);
        assert_eq!(root, "root");
    }

    #[test]
    fn protocol_for_schema_uses_protocol_name() {
        let schema = make_schema(&[("a", "object")]);
        let protocol = protocol_for_schema(&schema);
        assert_eq!(protocol.name, "test");
    }
}
