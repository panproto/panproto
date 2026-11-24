//! Data migration engine for the VCS.
//!
//! Migrates data instances between schema versions using the lens
//! infrastructure. Complements are stored as VCS objects so backward
//! migration never loses data.


use panproto_inst::WInstance;
use panproto_inst::identity::{AnyComplement, ComplementShape, Encoding, decode_complements};
use panproto_schema::{Protocol, Schema};

use crate::environment::{ProtocolEnvironment, load_environment, load_protocol};
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, ComplementObject, CstComplementObject, DataSetObject, Object};
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

/// A point data lies over: a schema, and the protocol its records are records
/// under.
#[derive(Clone, Copy, Debug)]
pub struct Fibre<'a> {
    /// The object ID of the schema.
    pub schema_id: ObjectId,
    /// The schema.
    pub schema: &'a Schema,
    /// The object ID of the protocol, when it is recorded.
    pub protocol_id: Option<ObjectId>,
    /// The protocol.
    pub protocol: &'a Protocol,
}

/// Transport a data set from the fibre over `src` to the fibre over `tgt`.
///
/// Produces a new `DataSetObject` over `tgt` and a `ComplementObject` for
/// the way back. Both are stored in the object store.
///
/// Returns `(new_data_id, complement_id)`.
///
/// # Errors
///
/// Returns `VcsError::DataMigrationFailed` if lens generation,
/// deserialization, or migration fails, or if a migrated record is not a
/// record of `tgt.schema` under `tgt.protocol`.
/// Returns `VcsError::TypeMismatch` if the object is not a `DataSet`.
pub fn migrate_forward(
    store: &mut dyn Store,
    data_id: ObjectId,
    src: Fibre<'_>,
    tgt: Fibre<'_>,
) -> Result<(ObjectId, ObjectId), VcsError> {
    let data_obj = load_data_set(store, data_id)?;
    let instances: Vec<WInstance> = decode_instances(&data_obj.data)?;
    let lens = lens_between(src, tgt)?;

    let mut migrated_instances = Vec::with_capacity(instances.len());
    let mut all_complements = Vec::with_capacity(instances.len());
    for (index, instance) in instances.iter().enumerate() {
        let (view, complement) =
            panproto_lens::get(&lens, instance).map_err(|e| migration_failed("lens get", e))?;
        let reasons = crate::records::violations(tgt.schema, tgt.protocol, &view);
        if !reasons.is_empty() {
            return Err(VcsError::DataMigrationFailed {
                reason: format!(
                    "record {index} is not a record of the target schema: {}",
                    reasons.join("; ")
                ),
            });
        }
        migrated_instances.push(view);
        all_complements.push(complement);
    }

    let new_data_id = store.put(&Object::DataSet(DataSetObject {
        schema_id: tgt.schema_id,
        data: encode(&migrated_instances)?,
        record_count: migrated_instances.len() as u64,
        // The key identifies the record, not its schema version, so it
        // survives migration unchanged.
        key: data_obj.key,
        protocol_id: tgt.protocol_id,
    }))?;
    let complement_id = store.put(&Object::Complement(ComplementObject {
        migration_id: data_id,
        data_id,
        complement: encode(&all_complements)?,
    }))?;

    Ok((new_data_id, complement_id))
}

/// Carry a data set over `tgt` back to the fibre over `src`, using the
/// complement its transport produced.
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
    src: Fibre<'_>,
    tgt: Fibre<'_>,
) -> Result<ObjectId, VcsError> {
    let data_obj = load_data_set(store, data_id)?;
    let comp_obj = match store.get(&complement_id)? {
        Object::Complement(c) => c,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "Complement".into(),
                got: other.type_name().into(),
            });
        }
    };

    let instances: Vec<WInstance> = decode_instances(&data_obj.data)?;
    let complements: Vec<panproto_lens::Complement> = decode_complements(
        &comp_obj.complement,
        Encoding::MsgpackPositional,
        ComplementShape::WType,
    )
    .map_err(|e| migration_failed("deserialize complement", e))?
    .into_iter()
    .map(|decoded| match decoded {
        AnyComplement::WType(complement) => Ok(*complement),
        AnyComplement::Functor(_) => Err(VcsError::DataMigrationFailed {
            reason: "decoded a table complement where a tree complement was required".to_owned(),
        }),
        AnyComplement::Resolution(_) => Err(VcsError::DataMigrationFailed {
            reason: "decoded a resolution complement where a tree complement was required"
                .to_owned(),
        }),
    })
    .collect::<Result<Vec<_>, _>>()?;

    // The lens of the way forward, whose put is the way back.
    let lens = lens_between(src, tgt)?;
    let mut restored = Vec::with_capacity(instances.len());
    for (instance, complement) in instances.iter().zip(complements.iter()) {
        restored.push(
            panproto_lens::put(&lens, instance, complement)
                .map_err(|e| migration_failed("lens put", e))?,
        );
    }

    store.put(&Object::DataSet(DataSetObject {
        schema_id: src.schema_id,
        data: encode(&restored)?,
        record_count: restored.len() as u64,
        key: data_obj.key,
        protocol_id: src.protocol_id,
    }))
}

/// The lens that transports records from `src` to `tgt`, generated under the
/// protocol the records are read under.
fn lens_between(src: Fibre<'_>, tgt: Fibre<'_>) -> Result<panproto_lens::Lens, VcsError> {
    let config = panproto_lens::AutoLensConfig::default();
    panproto_lens::auto_generate(src.schema, tgt.schema, src.protocol, &config)
        .map(|result| result.lens)
        .map_err(|e| migration_failed("lens generation", e))
}

/// The data set object at `id`.
pub(crate) fn load_data_set(store: &dyn Store, id: ObjectId) -> Result<DataSetObject, VcsError> {
    match store.get(&id)? {
        Object::DataSet(data_set) => Ok(data_set),
        other => Err(VcsError::TypeMismatch {
            expected: "DataSet".into(),
            got: other.type_name().into(),
        }),
    }
}

fn decode_instances(data: &[u8]) -> Result<Vec<WInstance>, VcsError> {
    rmp_serde::from_slice(data).map_err(|e| migration_failed("deserialize data", e))
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, VcsError> {
    rmp_serde::to_vec(value).map_err(|e| migration_failed("serialize", e))
}

fn migration_failed(step: &str, error: impl std::fmt::Display) -> VcsError {
    VcsError::DataMigrationFailed {
        reason: format!("{step}: {error}"),
    }
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
    src: Fibre<'_>,
    tgt: Fibre<'_>,
) -> Result<(), VcsError> {
    let lens = lens_between(src, tgt)?;

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
        let root = infer_root(src.schema);
        let instance =
            panproto_inst::parse_json(src.schema, src.protocol, &root, &data).map_err(|e| {
                VcsError::DataMigrationFailed {
                    reason: format!("parse instance: {e}"),
                }
            })?;

        let (view, _complement) = panproto_lens::get(&lens, &instance).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens get: {e}"),
            }
        })?;

        let output = panproto_inst::to_json(tgt.schema, &view).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("serialize view: {e}"),
            }
        })?;

        // Store migrated data as a DataSetObject for complement tracking
        let migrated_data = DataSetObject {
            schema_id: tgt.schema_id,
            data: rmp_serde::to_vec(&vec![view]).map_err(|e| VcsError::DataMigrationFailed {
                reason: format!("serialize: {e}"),
            })?,
            record_count: 1,
            // The source file path is this set's natural key, matching
            // how `Repository::add_data` keys a staged file by default.
            key: Some(path.to_string_lossy().into_owned()),
            protocol_id: tgt.protocol_id,
        };
        store.put(&Object::DataSet(migrated_data))?;

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

/// A data set transported between fibres, with what takes it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportedData {
    /// The data set over the source.
    pub base: ObjectId,
    /// Its image over the target.
    pub image: ObjectId,
    /// The complement of the transport.
    pub complement: ObjectId,
}

/// What a commit's data becomes under a change of its schema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiftedData {
    /// Every data set of the commit, in order, each one that lay over the
    /// source schema replaced by its image.
    pub data_ids: Vec<ObjectId>,
    /// The commit's complements, then one for each transported data set.
    pub complement_ids: Vec<ObjectId>,
    /// The data sets that were transported, in order.
    pub transported: Vec<TransportedData>,
}

/// The schema a commit's data is transported to, and the environment its
/// protocol name is read in.
#[derive(Clone, Copy, Debug)]
pub struct LiftTarget<'a> {
    /// The object ID of the schema.
    pub schema_id: ObjectId,
    /// The schema.
    pub schema: &'a Schema,
    /// The protocol environment of the commit that will hold the data.
    pub environment: &'a ProtocolEnvironment,
}

/// Transport a commit's data through a change of its schema.
///
/// A data set is transported when it lies over `src_schema`, the schema of
/// `commit`, and `src_schema` is not the target already. Any other data set
/// of the commit lies over a schema the change does not act on, and is
/// carried as it is. The transport starts from the protocol the data set
/// records, or, for data that records none, from the one the schema names in
/// the commit's own environment.
///
/// `cst_complement_ids` are deliberately not returned here: CST
/// complements capture formatting keyed by content, not schema, so
/// callers propagate them unchanged.
///
/// # Errors
///
/// The errors of [`migrate_forward`], so the caller aborts loudly rather
/// than dropping data, and [`VcsError::UnknownProtocol`] when a protocol
/// name that has to be read means nothing.
pub fn lift_commit_data(
    store: &mut dyn Store,
    commit: &CommitObject,
    src_schema: &Schema,
    target: LiftTarget<'_>,
) -> Result<LiftedData, VcsError> {
    let mut lifted = LiftedData {
        data_ids: commit.data_ids.clone(),
        complement_ids: commit.complement_ids.clone(),
        transported: Vec::new(),
    };
    let src_identity = crate::hash::hash_schema(src_schema)?;
    if commit.data_ids.is_empty() || src_identity == crate::hash::hash_schema(target.schema)? {
        return Ok(lifted);
    }

    let mut tgt_protocol = None;
    for slot in &mut lifted.data_ids {
        let base = load_data_set(store, *slot)?;
        // A schema is stored as a tree or, as a migration endpoint, flat.
        if base.schema_id != commit.schema_id && base.schema_id != src_identity {
            continue;
        }
        let src_protocol = data_protocol(store, commit, src_schema, base.protocol_id)?;
        let (tgt_protocol_id, tgt_protocol) = match &tgt_protocol {
            Some(found) => found,
            None => {
                let protocol = target
                    .environment
                    .resolve(store, &target.schema.protocol)?;
                let id = store.put(&Object::Protocol(Box::new(protocol.clone())))?;
                tgt_protocol.insert((id, protocol))
            }
        };
        let src = Fibre {
            schema_id: base.schema_id,
            schema: src_schema,
            protocol_id: base.protocol_id,
            protocol: &src_protocol,
        };
        let tgt = Fibre {
            schema_id: target.schema_id,
            schema: target.schema,
            protocol_id: Some(*tgt_protocol_id),
            protocol: tgt_protocol,
        };
        let (image, complement) = migrate_forward(store, *slot, src, tgt)?;
        lifted.transported.push(TransportedData {
            base: *slot,
            image,
            complement,
        });
        lifted.complement_ids.push(complement);
        *slot = image;
    }
    Ok(lifted)
}

/// The protocol data of `commit` over `schema` is read under: the one it
/// records, else the one the schema names in the commit's environment.
pub(crate) fn data_protocol(
    store: &dyn Store,
    commit: &CommitObject,
    schema: &Schema,
    recorded: Option<ObjectId>,
) -> Result<Protocol, VcsError> {
    match recorded {
        Some(id) => load_protocol(store, id),
        None => {
            load_environment(store, commit.environment_id)?.resolve(store, &schema.protocol)
        }
    }
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
            entries: Vec::new(),
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
            ..Default::default()
        }
    }

    /// The protocol the tests' schemas name, which no registry declares.
    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "record".into()],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    /// The registered protocols the tests resolve "test" through.
    fn registered() -> HashMap<String, Protocol> {
        HashMap::from([("test".to_owned(), test_protocol())])
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
            key: None,
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
            key: None,
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
        let schema_id = crate::tree::store_schema_as_tree(&mut store, schema.clone())?;

        let protocol = test_protocol();
        let result = migrate_forward(&mut store, schema_id, &schema, &schema, &protocol);

        assert!(result.is_err());
        if let Err(VcsError::TypeMismatch { expected, got }) = result {
            assert_eq!(expected, "DataSet");
            assert_eq!(got, "schema_tree");
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
    fn a_protocol_name_resolves_in_order() -> Result<(), VcsError> {
        let schema = make_schema(&[("a", "object")]);
        let none = HashMap::new();
        let stored = Protocol {
            obj_kinds: vec!["stored".into()],
            ..test_protocol()
        };
        // A stored protocol of that name comes first.
        let found = protocol_for_schema(
            &schema,
            ProtocolSources {
                stored: Some(&stored),
                registered: &registered(),
            },
        )?;
        assert_eq!(found, stored);
        // A stored protocol of another name is passed over for the
        // registered one.
        let other = Protocol {
            name: "other".into(),
            ..test_protocol()
        };
        let found = protocol_for_schema(
            &schema,
            ProtocolSources {
                stored: Some(&other),
                registered: &registered(),
            },
        )?;
        assert_eq!(found, test_protocol());
        // A name only the registry declares resolves to its protocol.
        let atproto = resolve_protocol(
            "atproto",
            ProtocolSources {
                stored: None,
                registered: &none,
            },
        )?;
        assert_eq!(
            atproto,
            (panproto_protocols::registry::descriptor("atproto")
                .ok_or(VcsError::NothingStaged)?
                .protocol)()
        );
        Ok(())
    }

    #[test]
    fn data_migration_refuses_an_unknown_protocol() -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::Node;

        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds_id = store.put(&Object::DataSet(DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: None,
        }))?;
        let commit = make_commit(crate::hash::hash_schema(&src)?, vec![ds_id]);
        match lift_commit_data(&mut store, &commit, &src, &tgt, &HashMap::new()) {
            Err(VcsError::UnknownProtocol { name }) => assert_eq!(name, "test"),
            other => panic!("an unknown protocol name was accepted: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn data_migration_uses_the_resolved_protocol() -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::Node;

        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds_id = store.put(&Object::DataSet(DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: None,
        }))?;
        // The commit records the protocol object, so it resolves the name
        // with nothing registered.
        let protocol_id = store.put(&Object::Protocol(Box::new(test_protocol())))?;
        let commit = CommitObject::builder(crate::hash::hash_schema(&src)?, "test", "test", "c")
            .timestamp(0)
            .data_ids(vec![ds_id])
            .protocol_id(protocol_id)
            .build();
        let (data_ids, complement_ids) =
            lift_commit_data(&mut store, &commit, &src, &tgt, &HashMap::new())?;
        assert_eq!(data_ids.len(), 1);
        assert_eq!(complement_ids.len(), 1);
        // The lift is the migration under that protocol.
        let mut expected = MemStore::new();
        let expected_ds = expected.put(&store.get(&ds_id)?)?;
        let (expected_view, _) =
            migrate_forward(&mut expected, expected_ds, &src, &tgt, &test_protocol())?;
        assert_eq!(data_ids[0], expected_view);
        Ok(())
    }

    #[test]
    fn lift_commit_data_lifts_through_schema_change() -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::Node;

        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);

        // A single-record data set valid against `src`.
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds = DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: Some("rec-key".to_owned()),
        };
        let ds_id = store.put(&Object::DataSet(ds))?;
        let commit = make_commit(crate::hash::hash_schema(&src)?, vec![ds_id]);

        let (data_ids, complement_ids) =
            lift_commit_data(&mut store, &commit, &src, &tgt, &registered())?;
        assert_eq!(data_ids.len(), 1);
        assert_eq!(complement_ids.len(), 1);
        assert_ne!(data_ids[0], ds_id, "data set lifted to a fresh object");

        match store.get(&data_ids[0])? {
            Object::DataSet(lifted) => {
                assert_eq!(lifted.record_count, 1);
                assert_eq!(lifted.key.as_deref(), Some("rec-key"));
                let insts: Vec<WInstance> = rmp_serde::from_slice(&lifted.data)?;
                assert_eq!(insts.len(), 1);
            }
            other => panic!("expected DataSet, got {}", other.type_name()),
        }
        Ok(())
    }

    #[test]
    fn a_stored_complement_is_decoded_whole_or_refused() -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::Node;

        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);
        let protocol = test_protocol();
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds_id = store.put(&Object::DataSet(DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: None,
        }))?;

        let (view_id, comp_id) = migrate_forward(&mut store, ds_id, &src, &tgt, &protocol)?;
        migrate_backward(&mut store, view_id, comp_id, &src, &tgt, &protocol)?;

        // The same complement with a byte left over after it.
        let Object::Complement(mut comp) = store.get(&comp_id)? else {
            panic!("expected a complement object");
        };
        comp.complement.push(0);
        let padded_id = store.put(&Object::Complement(comp))?;
        match migrate_backward(&mut store, view_id, padded_id, &src, &tgt, &protocol) {
            Err(VcsError::DataMigrationFailed { reason }) => {
                assert!(reason.contains("trailing byte"), "{reason}");
            }
            other => panic!("a padded complement was accepted: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn lift_commit_data_identity_schema_returns_ids_verbatim()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let ds = DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: vec![],
            record_count: 0,
            key: None,
        };
        let ds_id = store.put(&Object::DataSet(ds))?;
        let commit = make_commit(crate::hash::hash_schema(&src)?, vec![ds_id]);

        // Equal src and tgt: ids returned verbatim with no new objects.
        let (data_ids, complement_ids) =
            lift_commit_data(&mut store, &commit, &src, &src, &HashMap::new())?;
        assert_eq!(data_ids, vec![ds_id]);
        assert_eq!(complement_ids, []);
        Ok(())
    }
}
