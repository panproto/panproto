//! GAT-level validation for migrations and schemas.
//!
//! Provides type-checking and equation verification integrated into the
//! VCS staging and commit pipeline. Auto-derived migrations are validated
//! as theory morphisms on their mapped fragment. Schemas are validated
//! against their protocol's equations in one of two modes: against the
//! registered protocol theory when one is available (its equations are
//! checked), and against the structural theory extracted by
//! [`schema_to_theory`] otherwise (which carries no equations, so an
//! advisory note records that none were checked).

use panproto_gat::{
    CheckModelOptions, EquationViolation, Model, ModelValue, Theory, check_model_with_options,
    typecheck_theory,
};
use panproto_mig::Migration;
use panproto_schema::Schema;
use rustc_hash::FxHashMap;

pub use panproto_mig::schema_to_theory;

/// Diagnostics from GAT-level validation.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GatDiagnostics {
    /// Type-checking errors found in theory equations.
    pub type_errors: Vec<String>,
    /// Equation violations found in the schema model.
    pub equation_errors: Vec<String>,
    /// Migration structure violations that block staging and commit.
    #[serde(default)]
    pub migration_errors: Vec<String>,
    /// Migration structure warnings that do not block.
    pub migration_warnings: Vec<String>,
    /// Advisory notes about equation checking, e.g. that no protocol
    /// theory was registered so no equations were checked. These never
    /// block a commit.
    #[serde(default)]
    pub equation_notes: Vec<String>,
}

impl GatDiagnostics {
    /// Returns `true` if there are no blocking errors.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.type_errors.is_empty()
            && self.equation_errors.is_empty()
            && self.migration_errors.is_empty()
    }

    /// Returns `true` if there are any blocking errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.is_clean()
    }

    /// Merge another diagnostics bundle's findings into this one.
    pub fn extend(&mut self, other: Self) {
        self.type_errors.extend(other.type_errors);
        self.equation_errors.extend(other.equation_errors);
        self.migration_errors.extend(other.migration_errors);
        self.migration_warnings.extend(other.migration_warnings);
        self.equation_notes.extend(other.equation_notes);
    }

    /// Collect all blocking errors into a single Vec for display.
    #[must_use]
    pub fn all_errors(&self) -> Vec<String> {
        let mut errs = Vec::with_capacity(
            self.type_errors.len() + self.equation_errors.len() + self.migration_errors.len(),
        );
        for e in &self.type_errors {
            errs.push(format!("type error: {e}"));
        }
        for e in &self.equation_errors {
            errs.push(format!("equation violation: {e}"));
        }
        for e in &self.migration_errors {
            errs.push(format!("migration error: {e}"));
        }
        errs
    }
}

/// Validate a migration at the GAT level.
///
/// Structural violations are blocking (recorded in `migration_errors`):
///
/// * a vertex map entry referencing a vertex absent from either schema;
/// * an edge whose new endpoints are not the images of its own old
///   endpoints (surviving-identity convention when an endpoint is
///   unmapped);
/// * a mapped fragment that is not a theory morphism, as reported by
///   [`check_migration_morphism`](panproto_mig::check_migration_morphism).
///
/// A migration that maps zero vertices is recorded as an advisory
/// warning that does not block.
#[must_use]
pub fn validate_migration(old: &Schema, new: &Schema, migration: &Migration) -> GatDiagnostics {
    let mut diag = GatDiagnostics::default();

    if migration.vertex_map.is_empty() {
        diag.migration_warnings
            .push("migration maps zero vertices".to_owned());
    }

    // Mapped vertices must exist in their respective schemas.
    for (src_v, tgt_v) in &migration.vertex_map {
        if !old.vertices.contains_key(src_v) {
            diag.migration_errors.push(format!(
                "vertex map references source vertex '{src_v}' which does not exist in source schema"
            ));
        }
        if !new.vertices.contains_key(tgt_v) {
            diag.migration_errors.push(format!(
                "vertex map references target vertex '{tgt_v}' which does not exist in target schema"
            ));
        }
    }

    // Edge coherence: each mapped edge's new endpoints must be the images
    // of its own old endpoints under `vertex_map`, falling back to
    // identity only when an old endpoint is unmapped (the surviving-
    // identity convention used by `mig::compile`).
    for (old_edge, new_edge) in &migration.edge_map {
        let endpoint_ok = |old_v: &panproto_gat::Name, new_v: &panproto_gat::Name| -> bool {
            migration
                .vertex_map
                .get(old_v)
                .map_or_else(|| old_v == new_v, |mapped| mapped == new_v)
        };
        if !endpoint_ok(&old_edge.src, &new_edge.src) {
            diag.migration_errors.push(format!(
                "edge {}→{} maps to {}→{} but source vertex '{}' is not the image of '{}'",
                old_edge.src, old_edge.tgt, new_edge.src, new_edge.tgt, new_edge.src, old_edge.src
            ));
        }
        if !endpoint_ok(&old_edge.tgt, &new_edge.tgt) {
            diag.migration_errors.push(format!(
                "edge {}→{} maps to {}→{} but target vertex '{}' is not the image of '{}'",
                old_edge.src, old_edge.tgt, new_edge.src, new_edge.tgt, new_edge.tgt, old_edge.tgt
            ));
        }
    }

    // The mapped fragment must be a structure-preserving theory morphism.
    if let Err(e) = panproto_mig::check_migration_morphism(old, new, migration) {
        diag.migration_errors
            .push(format!("migration is not a theory morphism: {e}"));
    }

    diag
}

/// Build a set-theoretic [`Model`] of `theory` from `schema`.
///
/// Each theory sort's carrier is the set of vertex IDs whose `kind`
/// equals the sort name (as [`ModelValue::Str`]); a sort with no
/// matching vertex gets an empty carrier, which makes any equation over
/// it vacuously satisfied. Each theory operation is interpreted from the
/// schema's edges: a unary operation named `o` sends a vertex `v` to the
/// target of the edge out of `v` whose name or kind is `o`, and acts as
/// the identity where no such edge exists. This lets
/// [`validate_schema_equations`] check whether the schema, read as this
/// model, satisfies the protocol theory's equations.
#[must_use]
pub fn schema_to_model(schema: &Schema, theory: &Theory) -> Model {
    let mut model = Model::new(theory.name.as_ref());

    for sort in &theory.sorts {
        let mut carrier: Vec<ModelValue> = schema
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == sort.name.as_ref())
            .map(|v| ModelValue::Str(v.id.to_string()))
            .collect();
        carrier.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        model.add_sort(sort.name.to_string(), carrier);
    }

    for op in &theory.ops {
        // The edge relation this operation names: source vertex ID to
        // target vertex ID for every edge whose name or kind is `op`.
        let mut relation: FxHashMap<String, String> = FxHashMap::default();
        for edge in schema.edges.keys() {
            let matches = edge.name.as_deref() == Some(op.name.as_ref())
                || edge.kind.as_ref() == op.name.as_ref();
            if matches {
                relation
                    .entry(edge.src.to_string())
                    .or_insert_with(|| edge.tgt.to_string());
            }
        }
        model.add_op(op.name.to_string(), move |args: &[ModelValue]| {
            match args.first() {
                Some(ModelValue::Str(v)) => relation
                    .get(v)
                    .map_or_else(|| Ok(args[0].clone()), |w| Ok(ModelValue::Str(w.clone()))),
                Some(other) => Ok(other.clone()),
                None => Ok(ModelValue::Null),
            }
        });
    }

    model
}

/// Validate a schema against a theory's equations.
///
/// Builds the schema's model with [`schema_to_model`] and checks it
/// against `theory`'s equations. A theory with no equations yields clean
/// diagnostics.
#[must_use]
pub fn validate_schema_against_theory(schema: &Schema, theory: &Theory) -> GatDiagnostics {
    let model = schema_to_model(schema, theory);
    validate_schema_equations(schema, theory, &model)
}

/// Validate a protocol theory's equations are well-typed.
///
/// Runs `typecheck_theory` on the given theory and collects any errors.
#[must_use]
pub fn validate_theory_equations(theory: &Theory) -> GatDiagnostics {
    let mut diag = GatDiagnostics::default();
    if let Err(e) = typecheck_theory(theory) {
        diag.type_errors.push(e.to_string());
    }
    diag
}

/// Validate a schema's model against a theory's equations.
///
/// Builds a model from the schema and checks all theory equations.
/// Returns diagnostics with any violations found.
///
/// This uses a bounded check to avoid combinatorial explosion on
/// large schemas.
#[must_use]
pub fn validate_schema_equations(
    _schema: &Schema,
    theory: &Theory,
    model: &panproto_gat::Model,
) -> GatDiagnostics {
    let mut diag = GatDiagnostics::default();

    let options = CheckModelOptions {
        max_assignments: 10_000,
    };

    match check_model_with_options(model, theory, &options) {
        Ok(violations) => {
            for v in violations {
                diag.equation_errors.push(format_violation(&v));
            }
        }
        Err(e) => {
            // Limit exceeded or missing carrier: report as warning, not hard error.
            diag.equation_errors
                .push(format!("equation check incomplete: {e}"));
        }
    }

    diag
}

/// Format an equation violation for human-readable display.
fn format_violation(v: &EquationViolation) -> String {
    let assignment_str: String = v
        .assignment
        .iter()
        .map(|(var, val)| format!("{var}={val:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "equation '{}' violated when {}: LHS={:?}, RHS={:?}",
        v.equation, assignment_str, v.lhs_value, v.rhs_value
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_mig::Migration;
    use panproto_schema::{Edge, Vertex};
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
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
        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
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
        }
    }

    #[test]
    fn validate_identity_migration() {
        let schema = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let migration = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("b"), Name::from("b")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };
        let diag = validate_migration(&schema, &schema, &migration);
        assert!(diag.is_clean());
        assert!(diag.migration_warnings.is_empty());
    }

    #[test]
    fn validate_empty_migration_warns() {
        let schema = make_schema(&[("a", "object")], &[]);
        let migration = Migration {
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        };
        let diag = validate_migration(&schema, &schema, &migration);
        assert!(!diag.migration_warnings.is_empty());
    }

    #[test]
    fn validate_theory_typecheck() {
        use panproto_gat::{Equation, Operation, Sort, Term, Theory};

        let theory = Theory::new(
            "Good",
            vec![Sort::simple("S")],
            vec![Operation::unary("f", "x", "S", "S")],
            vec![Equation::new(
                "involution",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::var("x"),
            )],
        );
        let diag = validate_theory_equations(&theory);
        assert!(diag.is_clean());
    }

    fn migration(vmap: &[(&str, &str)], emap: &[(Edge, Edge)]) -> Migration {
        Migration {
            vertex_map: vmap
                .iter()
                .map(|(a, b)| (Name::from(*a), Name::from(*b)))
                .collect(),
            edge_map: emap.iter().cloned().collect(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        }
    }

    #[test]
    fn stage_blocks_on_invalid_migration_structure() {
        // A vertex map referencing a nonexistent source vertex is a
        // blocking structural error, so staging would mark it Invalid.
        let old = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let new = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let bad = migration(&[("a", "a"), ("ghost", "b")], &[]);

        let diag = validate_migration(&old, &new, &bad);
        assert!(!diag.is_clean(), "structural violation must block");
        assert!(
            diag.migration_errors
                .iter()
                .any(|e| e.contains("ghost") && e.contains("does not exist in source")),
            "expected a source-vertex error, got: {:?}",
            diag.migration_errors
        );
    }

    #[test]
    fn edge_coherence_rejects_crossed_endpoints() {
        // edge a->b is mapped to c2->b2, whose source is not the image of a.
        let edge_ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let edge_c2b2 = Edge {
            src: "c2".into(),
            tgt: "b2".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let old = make_schema(
            &[("a", "object"), ("b", "string"), ("c", "object")],
            std::slice::from_ref(&edge_ab),
        );
        let new = make_schema(
            &[("a2", "object"), ("b2", "string"), ("c2", "object")],
            std::slice::from_ref(&edge_c2b2),
        );
        let mig = migration(
            &[("a", "a2"), ("b", "b2"), ("c", "c2")],
            &[(edge_ab, edge_c2b2)],
        );

        let diag = validate_migration(&old, &new, &mig);
        assert!(!diag.is_clean(), "crossed edge must block");
        assert!(
            diag.migration_errors.iter().any(|e| e.contains("a→b")),
            "expected a coherence error naming edge a→b, got: {:?}",
            diag.migration_errors
        );
    }

    #[test]
    fn schema_equation_violation_is_detected() {
        // Theory: unary op f on Node with the equation f(x) = x. The
        // schema has an edge named `f` that is not the identity, so its
        // model violates the equation.
        use panproto_gat::{Equation, Operation, Sort, Term, Theory};

        let theory = Theory::new(
            "P",
            vec![Sort::simple("Node")],
            vec![Operation::unary("f", "x", "Node", "Node")],
            vec![Equation::new(
                "f_is_identity",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
            )],
        );

        let edge_rf = Edge {
            src: "root".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("f".into()),
        };
        let schema = make_schema(
            &[("root", "Node"), ("a", "Node")],
            std::slice::from_ref(&edge_rf),
        );

        let diag = validate_schema_against_theory(&schema, &theory);
        assert!(
            !diag.equation_errors.is_empty(),
            "expected an equation violation, got: {diag:?}"
        );

        // An equation-free theory yields clean diagnostics.
        let no_eq = Theory::new("P0", vec![Sort::simple("Node")], vec![], vec![]);
        assert!(validate_schema_against_theory(&schema, &no_eq).is_clean());
    }
}
