//! Protolenses: schema-parameterized families of lenses.
//!
//! A protolens is a natural transformation between theory endofunctors whose
//! components are lenses. Unlike a [`Lens`] (bound to two specific
//! schemas), a `Protolens` is a *family of lenses* parameterized by schema.
//!
//! In GAT terms: `η : Π(S : Schema | P(S)). Lens(F(S), G(S))`
//!
//! For any schema `S` satisfying the precondition, [`Protolens::instantiate`]
//! produces a concrete `Lens(F(S), G(S))`.
//!
//! # Elementary constructors
//!
//! The [`elementary`] module provides atomic protolens constructors — the
//! "atoms" from which all protolenses are composed:
//!
//! - [`elementary::add_sort`]: `Id ⟹ AddSort(τ)`
//! - [`elementary::drop_sort`]: `Id ⟹ DropSort(τ)`
//! - [`elementary::rename_sort`]: `Id ⟹ RenameSort(old, new)`
//! - [`elementary::add_op`]: `Id ⟹ AddOp(op)`
//! - [`elementary::drop_op`]: `Id ⟹ DropOp(op)`
//! - [`elementary::rename_op`]: `Id ⟹ RenameOp(old, new)`
//! - [`elementary::add_equation`]: `Id ⟹ AddEquation(eq)`
//! - [`elementary::drop_equation`]: `Id ⟹ DropEquation(eq)`
//! - [`elementary::pullback`]: `Id ⟹ Pullback(φ)`
//!
//! # Composition
//!
//! Protolenses compose vertically (sequential) and horizontally (parallel):
//!
//! - [`vertical_compose`]: `(η : F ⟹ G, θ : G ⟹ H) ↦ θ ∘ η : F ⟹ H`
//! - [`horizontal_compose`]: `(η : F ⟹ G, θ : F' ⟹ G') ↦ η * θ : F∘F' ⟹ G∘G'`

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use panproto_gat::{Name, Operation, Sort, Theory, TheoryEndofunctor, TheoryTransform};
use panproto_inst::CompiledMigration;
use panproto_schema::{Edge, Protocol, Schema, Vertex};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::Lens;
use crate::error::LensError;

// ---------------------------------------------------------------------------
// Helper: extract the inner Arc<str> from a Name
// ---------------------------------------------------------------------------

/// Clone the inner `Arc<str>` from a `Name`.
#[inline]
fn name_arc_clone(n: &Name) -> Arc<str> {
    Arc::clone(&n.0)
}

/// How the complement type depends on the schema at instantiation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplementConstructor {
    /// Complement is always empty (lossless protolens).
    Empty,
    /// Complement captures dropped sort data.
    DroppedSortData {
        /// The sort whose data is captured.
        sort: Name,
    },
    /// Complement captures dropped edge data.
    DroppedOpData {
        /// The operation whose data is captured.
        op: Name,
    },
    /// Complement is the kernel of a natural transformation.
    NatTransKernel {
        /// Name of the natural transformation.
        nat_trans_name: Name,
    },
    /// Composite complement from a chain.
    Composite(Vec<Self>),
}

/// A protolens: a natural transformation `η : F ⟹ G` between theory
/// endofunctors, where each component `η_S` is a lens.
///
/// Unlike a [`Lens`] (bound to two specific schemas), a
/// `Protolens` is a *schema-parameterized family of lenses*. For any
/// schema `S` satisfying the precondition, [`instantiate`](Self::instantiate)
/// produces a concrete `Lens(F(S), G(S))`.
///
/// In GAT terms: `η : Π(S : Schema | P(S)). Lens(F(S), G(S))`
#[derive(Debug, Clone)]
pub struct Protolens {
    /// Human-readable name.
    pub name: Name,
    /// Source endofunctor `F`.
    pub source: TheoryEndofunctor,
    /// Target endofunctor `G`.
    pub target: TheoryEndofunctor,
    /// How the complement type depends on the schema.
    pub complement_constructor: ComplementConstructor,
}

impl Protolens {
    /// Check if this protolens can be instantiated at the given schema.
    ///
    /// A protolens applies when the source endofunctor's precondition
    /// is satisfied by the schema's implicit theory (vertex kinds as
    /// sorts, edge kinds as operations).
    #[must_use]
    pub fn applicable_to(&self, schema: &Schema) -> bool {
        let implicit_theory = schema_to_implicit_theory(schema);
        self.source.precondition.satisfied_by(&implicit_theory)
    }

    /// Instantiate this protolens at a specific schema, producing a concrete
    /// [`Lens`].
    ///
    /// This is Π-type elimination: applying the dependent function to a
    /// specific schema.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if either endofunctor's
    /// transform fails to apply.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        // 1. Compute source schema: F(S)
        let src_schema = if matches!(self.source.transform, TheoryTransform::Identity) {
            schema.clone()
        } else {
            apply_theory_transform_to_schema(&self.source.transform, schema, protocol)?
        };

        // 2. Compute target schema: G(S)
        let tgt_schema =
            apply_theory_transform_to_schema(&self.target.transform, schema, protocol)?;

        // 3. Compute the migration from F(S) to G(S)
        let compiled = compute_migration_between(&src_schema, &tgt_schema);

        Ok(Lens {
            compiled,
            src_schema,
            tgt_schema,
        })
    }

    /// Compute the target schema without building a full lens.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if the target transform
    /// cannot be applied.
    pub fn target_schema(&self, schema: &Schema, protocol: &Protocol) -> Result<Schema, LensError> {
        apply_theory_transform_to_schema(&self.target.transform, schema, protocol)
    }

    /// Returns `true` if this protolens produces lossless lenses
    /// (empty complement).
    #[must_use]
    pub const fn is_lossless(&self) -> bool {
        matches!(self.complement_constructor, ComplementConstructor::Empty)
    }
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

/// Vertical composition of protolenses: given `η : F ⟹ G` and
/// `θ : G ⟹ H`, produce `θ ∘ η : F ⟹ H`.
///
/// The target endofunctor of `eta` must match the source of `theta`.
/// This is checked dynamically at instantiation time.
///
/// # Errors
///
/// Currently infallible, but returns `Result` for future compatibility
/// with static compatibility checks.
pub fn vertical_compose(eta: &Protolens, theta: &Protolens) -> Result<Protolens, LensError> {
    let complement = ComplementConstructor::Composite(vec![
        eta.complement_constructor.clone(),
        theta.complement_constructor.clone(),
    ]);

    Ok(Protolens {
        name: Name::from(format!("{}.{}", theta.name, eta.name)),
        source: eta.source.clone(),
        target: theta.target.clone(),
        complement_constructor: complement,
    })
}

/// Horizontal composition of protolenses: given `η : F ⟹ G` and
/// `θ : F' ⟹ G'`, produce `η * θ : F∘F' ⟹ G∘G'`.
///
/// # Errors
///
/// Currently infallible, but returns `Result` for future compatibility
/// with static compatibility checks.
pub fn horizontal_compose(eta: &Protolens, theta: &Protolens) -> Result<Protolens, LensError> {
    let source = eta.source.compose(&theta.source);
    let target = eta.target.compose(&theta.target);
    let complement = ComplementConstructor::Composite(vec![
        eta.complement_constructor.clone(),
        theta.complement_constructor.clone(),
    ]);

    Ok(Protolens {
        name: Name::from(format!("{}*{}", theta.name, eta.name)),
        source,
        target,
        complement_constructor: complement,
    })
}

// ---------------------------------------------------------------------------
// ProtolensChain
// ---------------------------------------------------------------------------

/// A chain of protolenses for vertical composition.
///
/// Each step's target endofunctor feeds into the next step's source.
/// Instantiating the chain at a schema produces a composed lens.
#[derive(Debug, Clone)]
pub struct ProtolensChain {
    /// The individual protolens steps.
    pub steps: Vec<Protolens>,
}

impl ProtolensChain {
    /// Create a new chain from steps.
    #[must_use]
    pub const fn new(steps: Vec<Protolens>) -> Self {
        Self { steps }
    }

    /// Check if the chain can be instantiated at the given schema.
    ///
    /// An empty chain (identity) is applicable to any schema. Otherwise,
    /// the first step must be applicable.
    #[must_use]
    pub fn applicable_to(&self, schema: &Schema) -> bool {
        if self.steps.is_empty() {
            return true;
        }
        self.steps[0].applicable_to(schema)
    }

    /// Instantiate the chain at a specific schema, producing a composed
    /// [`Lens`].
    ///
    /// Each step is instantiated at the current schema, and the resulting
    /// lenses are composed sequentially.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if any step fails, or
    /// [`LensError::CompositionMismatch`] if lens composition fails.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        if self.steps.is_empty() {
            return Ok(identity_lens(schema));
        }

        let mut current_schema = schema.clone();
        let mut lenses = Vec::new();

        for step in &self.steps {
            let lens = step.instantiate(&current_schema, protocol)?;
            current_schema = lens.tgt_schema.clone();
            lenses.push(lens);
        }

        // Compose all lenses
        let mut result = lenses.remove(0);
        for lens in lenses {
            result = crate::compose::compose(&result, &lens)?;
        }

        Ok(result)
    }

    /// Returns `true` if the chain is empty (identity).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

// ---------------------------------------------------------------------------
// Elementary protolens constructors
// ---------------------------------------------------------------------------

/// Built-in protolens constructors — the "atoms" from which all
/// protolenses are composed.
pub mod elementary {
    use panproto_gat::{
        Equation, Name, Operation, Sort, TheoryConstraint, TheoryEndofunctor, TheoryMorphism,
        TheoryTransform,
    };
    use panproto_inst::value::Value;
    use std::sync::Arc;

    use super::{ComplementConstructor, Protolens, name_arc_clone};

    /// `η : Id ⟹ AddSort(τ, d)` — for each `S`, `η_S` is a lens
    /// `S → S+{τ}` that adds a vertex kind with default.
    #[must_use]
    pub fn add_sort(
        sort_name: impl Into<Name>,
        _vertex_kind: impl Into<Name>,
        _default: Value,
    ) -> Protolens {
        let sort_name = sort_name.into();
        Protolens {
            name: Name::from(format!("add_sort_{sort_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{sort_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddSort(Sort::simple(name_arc_clone(&sort_name))),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ DropSort(τ)` — for each `S` containing sort `τ`,
    /// `η_S` is a lens `S → S \ {τ}`.
    #[must_use]
    pub fn drop_sort(sort_name: impl Into<Name>) -> Protolens {
        let sort_name = sort_name.into();
        let arc = name_arc_clone(&sort_name);
        Protolens {
            name: Name::from(format!("drop_sort_{sort_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{sort_name}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::DropSort(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::DroppedSortData { sort: sort_name },
        }
    }

    /// `η : Id ⟹ RenameSort(old, new)` — for each `S` containing sort
    /// `old`, `η_S` is a lossless lens `S → S[old↦new]`.
    #[must_use]
    pub fn rename_sort(old: impl Into<Name>, new: impl Into<Name>) -> Protolens {
        let old = old.into();
        let new = new.into();
        let old_arc = name_arc_clone(&old);
        let new_arc = name_arc_clone(&new);
        Protolens {
            name: Name::from(format!("rename_sort_{old}_{new}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&old_arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("rename_{old}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&old_arc)),
                transform: TheoryTransform::RenameSort {
                    old: old_arc,
                    new: new_arc,
                },
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ AddOp(op)` — adds an operation to the theory.
    #[must_use]
    pub fn add_op(
        op_name: impl Into<Name>,
        src_sort: impl Into<Name>,
        tgt_sort: impl Into<Name>,
        _kind: impl Into<Name>,
    ) -> Protolens {
        let op_name = op_name.into();
        let src_sort = src_sort.into();
        let tgt_sort = tgt_sort.into();
        Protolens {
            name: Name::from(format!("add_op_{op_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{op_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddOp(Operation::unary(
                    name_arc_clone(&op_name),
                    "x",
                    name_arc_clone(&src_sort),
                    name_arc_clone(&tgt_sort),
                )),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ DropOp(op)` — drops an operation from the theory.
    #[must_use]
    pub fn drop_op(op_name: impl Into<Name>) -> Protolens {
        let op_name = op_name.into();
        let arc = name_arc_clone(&op_name);
        Protolens {
            name: Name::from(format!("drop_op_{op_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasOp(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{op_name}")),
                precondition: TheoryConstraint::HasOp(Arc::clone(&arc)),
                transform: TheoryTransform::DropOp(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::DroppedOpData { op: op_name },
        }
    }

    /// `η : Id ⟹ RenameOp(old, new)` — renames an operation.
    #[must_use]
    pub fn rename_op(old: impl Into<Name>, new: impl Into<Name>) -> Protolens {
        let old = old.into();
        let new = new.into();
        let old_arc = name_arc_clone(&old);
        let new_arc = name_arc_clone(&new);
        Protolens {
            name: Name::from(format!("rename_op_{old}_{new}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasOp(Arc::clone(&old_arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("rename_{old}")),
                precondition: TheoryConstraint::HasOp(Arc::clone(&old_arc)),
                transform: TheoryTransform::RenameOp {
                    old: old_arc,
                    new: new_arc,
                },
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ AddEquation(eq)` — adds an equation (constraint).
    #[must_use]
    pub fn add_equation(eq: Equation) -> Protolens {
        let eq_name = Arc::clone(&eq.name);
        Protolens {
            name: Name::from(format!("add_eq_{eq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{eq_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddEquation(eq),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ DropEquation(eq_name)` — drops an equation.
    #[must_use]
    pub fn drop_equation(eq_name: impl Into<Name>) -> Protolens {
        let eq_name = eq_name.into();
        let arc = name_arc_clone(&eq_name);
        Protolens {
            name: Name::from(format!("drop_eq_{eq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasEquation(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{eq_name}")),
                precondition: TheoryConstraint::HasEquation(Arc::clone(&arc)),
                transform: TheoryTransform::DropEquation(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// Pullback along a theory morphism.
    #[must_use]
    pub fn pullback(morphism: TheoryMorphism) -> Protolens {
        let morph_name = Arc::clone(&morphism.name);
        Protolens {
            name: Name::from(format!("pullback_{morph_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("pullback_{morph_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Pullback(morphism),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build a [`CompiledMigration`] between two schemas by comparing their
/// structures.
fn compute_migration_between(src: &Schema, tgt: &Schema) -> CompiledMigration {
    let surviving_verts: HashSet<Name> = src
        .vertices
        .keys()
        .filter(|v| tgt.vertices.contains_key(&**v))
        .cloned()
        .collect();

    let surviving_edges: HashSet<Edge> = src
        .edges
        .keys()
        .filter(|e| tgt.edges.contains_key(*e))
        .cloned()
        .collect();

    // Build vertex remap: vertices in src that were renamed in tgt.
    let mut vertex_remap = HashMap::new();
    let unmapped_src: Vec<&Name> = src
        .vertices
        .keys()
        .filter(|v| !tgt.vertices.contains_key(&**v))
        .collect();
    let unmapped_tgt: Vec<&Name> = tgt
        .vertices
        .keys()
        .filter(|v| !src.vertices.contains_key(&**v))
        .collect();

    // Match unmapped vertices by structural similarity (same kind).
    for src_id in &unmapped_src {
        if let Some(src_v) = src.vertices.get(*src_id) {
            for tgt_id in &unmapped_tgt {
                if let Some(tgt_v) = tgt.vertices.get(*tgt_id) {
                    if src_v.kind == tgt_v.kind
                        && !vertex_remap.values().any(|v: &Name| v == *tgt_id)
                    {
                        vertex_remap.insert((*src_id).clone(), (*tgt_id).clone());
                        break;
                    }
                }
            }
        }
    }

    // Include remapped vertices in surviving set
    let mut final_surviving = surviving_verts;
    for src_id in vertex_remap.keys() {
        final_surviving.insert(src_id.clone());
    }

    // Build resolver for edges between surviving vertices in target
    let mut resolver = HashMap::new();
    for edge in tgt.edges.keys() {
        let src_in =
            final_surviving.contains(&edge.src) || vertex_remap.values().any(|v| *v == edge.src);
        let tgt_in =
            final_surviving.contains(&edge.tgt) || vertex_remap.values().any(|v| *v == edge.tgt);
        if src_in && tgt_in {
            resolver.insert((edge.src.clone(), edge.tgt.clone()), edge.clone());
        }
    }

    CompiledMigration {
        surviving_verts: final_surviving,
        surviving_edges,
        vertex_remap,
        edge_remap: HashMap::new(),
        resolver,
        hyper_resolver: HashMap::new(),
    }
}

/// Apply a theory transform to a schema, producing a new schema.
///
/// This is the bridge between GAT-level (Theory) and schema-level (Schema).
/// The `protocol` parameter is threaded through for recursive calls but
/// is not directly consulted by the current transform implementations.
#[allow(clippy::too_many_lines, clippy::only_used_in_recursion)]
fn apply_theory_transform_to_schema(
    transform: &TheoryTransform,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, LensError> {
    match transform {
        TheoryTransform::Identity => Ok(schema.clone()),
        TheoryTransform::RenameSort { old, new } => {
            Ok(apply_rename_sort_to_schema(schema, old, new))
        }
        TheoryTransform::RenameOp { old, new } => Ok(apply_rename_op_to_schema(schema, old, new)),
        TheoryTransform::DropSort(name) => Ok(apply_drop_sort_from_schema(schema, name)),
        TheoryTransform::AddSort(sort) => {
            let mut new_schema = schema.clone();
            let vertex = Vertex {
                id: Name::from(&*sort.name),
                kind: Name::from(&*sort.name),
                nsid: None,
            };
            new_schema.vertices.insert(Name::from(&*sort.name), vertex);
            Ok(new_schema)
        }
        TheoryTransform::DropOp(name) => Ok(apply_drop_op_from_schema(schema, name)),
        TheoryTransform::AddOp(_op) => {
            // Adding an op adds edges — but we need source/target vertices
            // to already exist. For now, this is a no-op at schema level
            // if the required vertices don't exist.
            Ok(schema.clone())
        }
        TheoryTransform::AddEquation(_) | TheoryTransform::DropEquation(_) => {
            // Equations don't change schema structure, only constraints.
            Ok(schema.clone())
        }
        TheoryTransform::Pullback(morphism) => {
            let mut result = schema.clone();
            for (old, new) in &morphism.sort_map {
                if old != new {
                    result = apply_rename_sort_to_schema(&result, old, new);
                }
            }
            for (old, new) in &morphism.op_map {
                if old != new {
                    result = apply_rename_op_to_schema(&result, old, new);
                }
            }
            Ok(result)
        }
        TheoryTransform::Compose(first, second) => {
            let intermediate = apply_theory_transform_to_schema(first, schema, protocol)?;
            apply_theory_transform_to_schema(second, &intermediate, protocol)
        }
    }
}

/// Rename a sort (vertex kind) within a schema.
fn apply_rename_sort_to_schema(schema: &Schema, old: &Arc<str>, new: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let mut new_vertices = HashMap::new();
    for (id, vertex) in &new_schema.vertices {
        let mut v = vertex.clone();
        if *v.kind == **old {
            v.kind = Name::from(&**new);
        }
        new_vertices.insert(id.clone(), v);
    }
    new_schema.vertices = new_vertices;
    new_schema
}

/// Rename an operation (edge kind) within a schema.
fn apply_rename_op_to_schema(schema: &Schema, old: &Arc<str>, new: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let mut new_edges = HashMap::new();
    for (edge, kind) in &new_schema.edges {
        let mut e = edge.clone();
        if *e.kind == **old {
            e.kind = Name::from(&**new);
        }
        let k = if **kind == **old {
            Name::from(&**new)
        } else {
            kind.clone()
        };
        new_edges.insert(e, k);
    }
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Drop a sort (vertex kind) and all dependent edges from a schema.
fn apply_drop_sort_from_schema(schema: &Schema, name: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let to_remove: Vec<Name> = new_schema
        .vertices
        .iter()
        .filter(|(_, v)| *v.kind == **name)
        .map(|(id, _)| id.clone())
        .collect();
    for id in &to_remove {
        new_schema.vertices.remove(id);
    }
    let new_edges: HashMap<Edge, Name> = new_schema
        .edges
        .iter()
        .filter(|(e, _)| !to_remove.contains(&e.src) && !to_remove.contains(&e.tgt))
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Drop an operation (edge kind) from a schema.
fn apply_drop_op_from_schema(schema: &Schema, name: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let new_edges: HashMap<Edge, Name> = new_schema
        .edges
        .iter()
        .filter(|(e, _)| *e.kind != **name)
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Build an implicit theory from a schema (sorts = vertex kinds,
/// ops = edge kinds).
pub(crate) fn schema_to_implicit_theory(schema: &Schema) -> Theory {
    let mut sort_names: HashSet<&str> = HashSet::new();
    let mut sorts = Vec::new();
    for vertex in schema.vertices.values() {
        if sort_names.insert(&vertex.kind) {
            sorts.push(Sort::simple(name_arc_clone(&vertex.kind)));
        }
    }

    let mut op_names: HashSet<&str> = HashSet::new();
    let mut ops = Vec::new();
    for edge in schema.edges.keys() {
        if op_names.insert(&edge.kind) {
            let src_kind = schema
                .vertices
                .get(&edge.src)
                .map_or_else(|| Arc::from("unknown"), |v| name_arc_clone(&v.kind));
            let tgt_kind = schema
                .vertices
                .get(&edge.tgt)
                .map_or_else(|| Arc::from("unknown"), |v| name_arc_clone(&v.kind));
            ops.push(Operation::unary(
                name_arc_clone(&edge.kind),
                "x",
                src_kind,
                tgt_kind,
            ));
        }
    }

    Theory::new("implicit", sorts, ops, Vec::new())
}

/// Rebuild the precomputed adjacency indices on a schema after mutating
/// vertices/edges.
fn rebuild_indices(schema: &mut Schema) {
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for edge in schema.edges.keys() {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    schema.outgoing = outgoing;
    schema.incoming = incoming;
    schema.between = between;
}

/// Build an identity lens for the given schema.
fn identity_lens(schema: &Schema) -> Lens {
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges = schema.edges.keys().cloned().collect();

    Lens {
        compiled: CompiledMigration {
            surviving_verts,
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use panproto_inst::value::Value;
    use panproto_schema::Protocol;

    use super::{
        ComplementConstructor, ProtolensChain, elementary, horizontal_compose, identity_lens,
        schema_to_implicit_theory, vertical_compose,
    };
    use crate::tests::three_node_schema;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "array".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn elementary_rename_sort_applicable() {
        let schema = three_node_schema();
        let p = elementary::rename_sort("string", "text");
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_rename_sort_not_applicable() {
        let schema = three_node_schema();
        let p = elementary::rename_sort("nonexistent", "text");
        assert!(!p.applicable_to(&schema));
    }

    #[test]
    fn elementary_drop_sort_applicable() {
        let schema = three_node_schema();
        let p = elementary::drop_sort("string");
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_add_sort_always_applicable() {
        let schema = three_node_schema();
        let p = elementary::add_sort("tags", "array", Value::Null);
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_rename_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::rename_sort("string", "text");
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_ne!(lens.src_schema.vertices.len(), 0);
    }

    #[test]
    fn chain_empty_is_identity() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![]);
        let lens = chain.instantiate(&schema, &protocol).unwrap();
        assert_eq!(
            lens.src_schema.vertices.len(),
            lens.tgt_schema.vertices.len()
        );
    }

    #[test]
    fn chain_single_step() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![elementary::add_sort("tags", "array", Value::Null)]);
        let lens = chain.instantiate(&schema, &protocol).unwrap();
        assert_eq!(
            lens.tgt_schema.vertices.len(),
            lens.src_schema.vertices.len() + 1
        );
    }

    #[test]
    fn vertical_compose_works() {
        let p1 = elementary::rename_sort("string", "text");
        let p2 = elementary::add_sort("tags", "array", Value::Null);
        let composed = vertical_compose(&p1, &p2).unwrap();
        assert_eq!(&*composed.name, "add_sort_tags.rename_sort_string_text");
    }

    #[test]
    fn is_lossless() {
        assert!(elementary::rename_sort("a", "b").is_lossless());
        assert!(elementary::rename_op("a", "b").is_lossless());
        assert!(elementary::add_sort("a", "b", Value::Null).is_lossless());
        assert!(!elementary::drop_sort("a").is_lossless());
        assert!(!elementary::drop_op("a").is_lossless());
    }

    #[test]
    fn complement_constructor_types() {
        assert!(matches!(
            elementary::rename_sort("a", "b").complement_constructor,
            ComplementConstructor::Empty
        ));
        assert!(matches!(
            elementary::drop_sort("a").complement_constructor,
            ComplementConstructor::DroppedSortData { .. }
        ));
        assert!(matches!(
            elementary::drop_op("a").complement_constructor,
            ComplementConstructor::DroppedOpData { .. }
        ));
    }

    #[test]
    fn protolens_chain_applicable() {
        let schema = three_node_schema();
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        assert!(chain.applicable_to(&schema));
    }

    #[test]
    fn schema_to_theory_extracts_kinds() {
        let schema = three_node_schema();
        let theory = schema_to_implicit_theory(&schema);
        assert!(theory.has_sort("object"));
        assert!(theory.has_sort("string"));
        assert!(theory.has_op("prop"));
    }

    #[test]
    fn horizontal_compose_works() {
        let p1 = elementary::rename_sort("a", "b");
        let p2 = elementary::rename_sort("c", "d");
        let composed = horizontal_compose(&p1, &p2).unwrap();
        assert!(composed.name.contains('*'));
    }

    #[test]
    fn chain_len_and_is_empty() {
        let empty = ProtolensChain::new(vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let chain = ProtolensChain::new(vec![elementary::rename_sort("a", "b")]);
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn drop_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::drop_sort("string");
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_eq!(lens.src_schema.vertices.len(), 3);
        assert_eq!(lens.tgt_schema.vertices.len(), 1);
    }

    #[test]
    fn add_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::add_sort("tags", "array", Value::Null);
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_eq!(lens.src_schema.vertices.len(), 3);
        assert_eq!(lens.tgt_schema.vertices.len(), 4);
        assert!(lens.tgt_schema.vertices.contains_key("tags"));
    }

    #[test]
    fn target_schema_without_full_lens() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::add_sort("tags", "array", Value::Null);
        let tgt = p.target_schema(&schema, &protocol).unwrap();
        assert_eq!(tgt.vertices.len(), 4);
    }

    #[test]
    fn identity_lens_preserves_schema() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        assert_eq!(
            lens.src_schema.vertices.len(),
            lens.tgt_schema.vertices.len()
        );
        assert_eq!(lens.src_schema.edges.len(), lens.tgt_schema.edges.len());
    }
}
