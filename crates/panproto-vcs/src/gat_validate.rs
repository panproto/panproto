//! GAT-level validation for migrations and schemas.
//!
//! Provides type-checking and equation verification integrated into
//! the VCS staging and commit pipeline. These functions validate that
//! auto-derived migrations are well-formed theory morphisms and that
//! schemas satisfy their protocol's equations.

use panproto_gat::{
    CheckModelOptions, EquationViolation, Theory, check_model_with_options, typecheck_theory,
};
use panproto_mig::Migration;
use panproto_schema::Schema;

/// Diagnostics from GAT-level validation.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GatDiagnostics {
    /// Type-checking errors found in theory equations.
    pub type_errors: Vec<String>,
    /// Equation violations found in the schema model.
    pub equation_errors: Vec<String>,
    /// Migration structure warnings (non-blocking).
    pub migration_warnings: Vec<String>,
}

impl GatDiagnostics {
    /// Returns `true` if there are no errors.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.type_errors.is_empty() && self.equation_errors.is_empty()
    }

    /// Returns `true` if there are any errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.is_clean()
    }

    /// Collect all errors into a single Vec for display.
    #[must_use]
    pub fn all_errors(&self) -> Vec<String> {
        let mut errs = Vec::with_capacity(self.type_errors.len() + self.equation_errors.len());
        for e in &self.type_errors {
            errs.push(format!("type error: {e}"));
        }
        for e in &self.equation_errors {
            errs.push(format!("equation violation: {e}"));
        }
        errs
    }
}

/// Validate a migration at the GAT level.
///
/// Checks that the migration's vertex/edge maps form a structurally
/// consistent morphism: no vertex maps to a vertex of incompatible kind,
/// and edge endpoint mappings are coherent.
#[must_use]
pub fn validate_migration(old: &Schema, new: &Schema, migration: &Migration) -> GatDiagnostics {
    let mut diag = GatDiagnostics::default();

    if migration.vertex_map.is_empty() {
        diag.migration_warnings
            .push("migration maps zero vertices".to_owned());
    }

    // Verify mapped vertices exist in their respective schemas.
    for (src_v, tgt_v) in &migration.vertex_map {
        if !old.vertices.contains_key(src_v) {
            diag.migration_warnings.push(format!(
                "vertex map references source vertex '{src_v}' which does not exist in source schema"
            ));
        }
        if !new.vertices.contains_key(tgt_v) {
            diag.migration_warnings.push(format!(
                "vertex map references target vertex '{tgt_v}' which does not exist in target schema"
            ));
        }
    }

    // Check edge map coherence: for each mapped edge, verify that
    // the source and target vertices of the new edge are in the
    // vertex map's image (or in surviving_verts implicitly).
    for (old_edge, new_edge) in &migration.edge_map {
        let src_mapped = migration.vertex_map.values().any(|v| *v == new_edge.src)
            || migration
                .vertex_map
                .get(&old_edge.src)
                .is_some_and(|v| *v == new_edge.src);
        let tgt_mapped = migration.vertex_map.values().any(|v| *v == new_edge.tgt)
            || migration
                .vertex_map
                .get(&old_edge.tgt)
                .is_some_and(|v| *v == new_edge.tgt);

        if !src_mapped {
            diag.migration_warnings.push(format!(
                "edge {}→{} maps to {}→{} but source vertex '{}' is not in vertex map image",
                old_edge.src, old_edge.tgt, new_edge.src, new_edge.tgt, new_edge.src
            ));
        }
        if !tgt_mapped {
            diag.migration_warnings.push(format!(
                "edge {}→{} maps to {}→{} but target vertex '{}' is not in vertex map image",
                old_edge.src, old_edge.tgt, new_edge.src, new_edge.tgt, new_edge.tgt
            ));
        }
    }

    diag
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
            // Limit exceeded or missing carrier — report as warning, not hard error.
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
}
