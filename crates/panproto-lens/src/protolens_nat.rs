//! Derive a categorical [`NaturalTransformation`] from a [`Protolens`] and
//! check it with the GAT naturality checker.
//!
//! A [`Protolens`] is a natural transformation `η : F ⟹ G` between the two
//! theory endofunctors it carries (`source` = `F`, `target` = `G`). That
//! naturality is asserted "by construction" but is not otherwise verified at
//! runtime. This module closes that gap for the **operation vocabulary**:
//! given a sampled base [`Theory`] (for instance the implicit theory of a
//! schema), it builds the theory-level naturality square induced by the
//! protolens's endofunctors and runs
//! [`panproto_gat::check_natural_transformation`] on it.
//!
//! # What is checked
//!
//! The GAT checker verifies naturality squares over the *operations* of the
//! domain theory. The square derived here uses:
//!
//! - **domain** `T1`: the base theory, restricted to the operations that
//!   survive both endofunctors (an op that `G` drops is not in the shared
//!   domain of the square);
//! - **codomain** `T2`: the base operations, extended with the renamed and
//!   added operations the endofunctors introduce, together with an equation
//!   `old(x) = new(x)` for every operation rename (a rename names the *same*
//!   operation, so the two names denote equal terms);
//! - **F, G**: theory morphisms carrying each endofunctor's operation
//!   renaming into `T2`;
//! - **components** `α_S`: the identity term `x` at every sort — the carried
//!   element is transported unchanged, only its operation labels are
//!   relabeled.
//!
//! Under this square the naturality condition for a renamed operation reduces
//! to `old(x) = new(x)`, which the checker discharges by normalizing with the
//! codomain's equations; for every other operation both legs coincide
//! syntactically. Sort-level actions (add / drop / rename a sort, coerce,
//! merge, edge-metadata relabeling) leave the operation vocabulary — and
//! hence the term-level square — unchanged, so they check as identity
//! squares.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    DirectedEquation, GatError, NaturalTransformation, Operation, Term, Theory, TheoryMorphism,
    TheoryTransform, check_natural_transformation,
};

use crate::protolens::Protolens;

/// The theory-level naturality square derived from a [`Protolens`].
///
/// Feed the fields to [`panproto_gat::check_natural_transformation`] — or use
/// [`check_protolens_naturality`], which does exactly that.
#[derive(Debug, Clone)]
pub struct ProtolensNaturalitySquare {
    /// The derived natural transformation `α : F ⟹ G`.
    pub nat_trans: NaturalTransformation,
    /// The source morphism `F` (from the protolens's `source` endofunctor).
    pub f: TheoryMorphism,
    /// The target morphism `G` (from the protolens's `target` endofunctor).
    pub g: TheoryMorphism,
    /// The domain theory `T1`: base ops surviving both endofunctors.
    pub domain: Theory,
    /// The codomain theory `T2`: base ops plus renamed / added ops and the
    /// rename equations.
    pub codomain: Theory,
}

/// How a [`TheoryTransform`] acts on the operation vocabulary of a theory.
///
/// Only three transforms touch operation *names*; every other variant acts
/// on sorts, edges, or equations and leaves this action empty.
#[derive(Debug, Default)]
struct OpVocabAction {
    /// Operation renamings `old ↦ new`.
    renames: Vec<(Arc<str>, Arc<str>)>,
    /// Operations removed from the theory.
    dropped: Vec<Arc<str>>,
    /// Operations introduced by the transform.
    added: Vec<Operation>,
}

/// Read the operation-vocabulary action of a single transform.
fn op_vocab_action(transform: &TheoryTransform) -> OpVocabAction {
    match transform {
        TheoryTransform::RenameOp { old, new } => OpVocabAction {
            renames: vec![(Arc::clone(old), Arc::clone(new))],
            ..OpVocabAction::default()
        },
        TheoryTransform::DropOp(name) => OpVocabAction {
            dropped: vec![Arc::clone(name)],
            ..OpVocabAction::default()
        },
        TheoryTransform::AddOp(op) => OpVocabAction {
            added: vec![op.clone()],
            ..OpVocabAction::default()
        },
        // Every other transform leaves the operation vocabulary fixed.
        _ => OpVocabAction::default(),
    }
}

/// Look up an operation by name in a theory.
fn find_op<'a>(theory: &'a Theory, name: &str) -> Option<&'a Operation> {
    theory.ops.iter().find(|op| &*op.name == name)
}

/// Build the directed rename equation `new(x0, …) → old(x0, …)`, matching
/// the arity of `template`.
///
/// The rewrite is directed (new name reduces to old) so that normalization
/// terminates: an undirected `old = new` would rewrite in both directions
/// and loop. With this single rule both legs of the renamed-op square
/// normalize to the `old`-rooted term.
fn rename_equation(old: &Arc<str>, new: &Arc<str>, template: &Operation) -> DirectedEquation {
    let args: Vec<Term> = if template.inputs.len() <= 1 {
        vec![Term::var("x")]
    } else {
        (0..template.inputs.len())
            .map(|i| Term::var(Arc::from(format!("x{i}"))))
            .collect()
    };
    DirectedEquation::new(
        Arc::from(format!("rename_{old}_{new}")),
        Term::app(Arc::clone(new), args.clone()),
        Term::app(Arc::clone(old), args),
        panproto_expr::Expr::Var("__id__".into()),
    )
}

const DOMAIN_NAME: &str = "protolens_nat_domain";
const CODOMAIN_NAME: &str = "protolens_nat_codomain";

/// Build the naturality square induced by a [`Protolens`] at a base theory.
///
/// The base theory is typically the implicit theory of a schema the
/// protolens is applicable to. The returned square is well-formed for
/// [`panproto_gat::check_natural_transformation`]; see the module docs for
/// exactly what its naturality asserts.
#[must_use]
pub fn protolens_naturality_square(
    protolens: &Protolens,
    base: &Theory,
) -> ProtolensNaturalitySquare {
    let f_action = op_vocab_action(&protolens.source.transform);
    let g_action = op_vocab_action(&protolens.target.transform);

    // Domain: base operations that survive both endofunctors. An op dropped
    // by either side is not part of the shared square.
    let dropped: Vec<&Arc<str>> = f_action
        .dropped
        .iter()
        .chain(g_action.dropped.iter())
        .collect();
    let domain_ops: Vec<Operation> = base
        .ops
        .iter()
        .filter(|op| !dropped.iter().any(|d| ***d == *op.name))
        .cloned()
        .collect();

    let domain = Theory::new(
        DOMAIN_NAME,
        base.sorts.clone(),
        domain_ops.clone(),
        Vec::new(),
    );

    // Codomain: surviving base ops, plus renamed-op and added-op images, plus
    // one directed equation per rename tying the two names together.
    let mut codomain_ops = domain_ops;
    let mut codomain_directed_eqs = Vec::new();
    let mut seen: std::collections::HashSet<Arc<str>> =
        codomain_ops.iter().map(|op| Arc::clone(&op.name)).collect();

    for (old, new) in f_action.renames.iter().chain(g_action.renames.iter()) {
        // A rename only bears on the square when the source op is present.
        if let Some(template) = find_op(base, old) {
            if seen.insert(Arc::clone(new)) {
                let mut renamed = template.clone();
                renamed.name = Arc::clone(new);
                codomain_ops.push(renamed);
            }
            codomain_directed_eqs.push(rename_equation(old, new, template));
        }
    }
    for op in f_action.added.iter().chain(g_action.added.iter()) {
        if seen.insert(Arc::clone(&op.name)) {
            codomain_ops.push(op.clone());
        }
    }

    let codomain = Theory::full(
        CODOMAIN_NAME,
        Vec::new(),
        base.sorts.clone(),
        codomain_ops,
        Vec::new(),
        codomain_directed_eqs,
        Vec::new(),
    );

    // Morphisms F and G: identity on sorts; op-map applies each endofunctor's
    // renames (identity for every other op).
    let f = build_morphism("F", &domain, &f_action);
    let g = build_morphism("G", &domain, &g_action);

    // Components: the identity term `x` at each base sort — the carried
    // element is transported unchanged.
    let components: HashMap<Arc<str>, Term> = base
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Term::var("x")))
        .collect();
    let nat_trans = NaturalTransformation {
        name: Arc::from(&*format!("nat_{}", protolens.name)),
        source: Arc::clone(&f.name),
        target: Arc::clone(&g.name),
        components,
    };

    ProtolensNaturalitySquare {
        nat_trans,
        f,
        g,
        domain,
        codomain,
    }
}

/// Build a theory morphism `domain → codomain` whose op-map applies the
/// given vocabulary action's renames and is the identity elsewhere.
fn build_morphism(name: &str, domain: &Theory, action: &OpVocabAction) -> TheoryMorphism {
    let sort_map: HashMap<Arc<str>, Arc<str>> = domain
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
        .collect();
    let mut op_map: HashMap<Arc<str>, Arc<str>> = domain
        .ops
        .iter()
        .map(|o| (Arc::clone(&o.name), Arc::clone(&o.name)))
        .collect();
    for (old, new) in &action.renames {
        if op_map.contains_key(old) {
            op_map.insert(Arc::clone(old), Arc::clone(new));
        }
    }
    TheoryMorphism::new(name, DOMAIN_NAME, CODOMAIN_NAME, sort_map, op_map)
}

/// Derive the naturality square from a protolens and check it with the GAT
/// naturality checker.
///
/// This is the runtime verification the [`Protolens`] documentation defers:
/// it confirms that the protolens's operation-level action commutes as a
/// natural transformation between its two endofunctors at the given base
/// theory.
///
/// # Errors
///
/// Returns the [`GatError`] reported by
/// [`panproto_gat::check_natural_transformation`] when the derived square
/// fails to commute.
pub fn check_protolens_naturality(protolens: &Protolens, base: &Theory) -> Result<(), GatError> {
    let square = protolens_naturality_square(protolens, base);
    check_natural_transformation(
        &square.nat_trans,
        &square.f,
        &square.g,
        &square.domain,
        &square.codomain,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::protolens::{elementary, schema_to_implicit_theory};
    use panproto_gat::Name;
    use panproto_gat::{Operation, Sort, Theory};
    use panproto_schema::{Edge, Schema, Vertex};
    use smallvec::SmallVec;
    use std::collections::HashMap;

    /// A small base theory: sorts `A`, `B` and a unary op `f : A → B`.
    fn base_theory() -> Theory {
        Theory::new(
            "base",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        )
    }

    /// A tiny schema: `root --prop--> child`, used to sample a base theory
    /// via [`schema_to_implicit_theory`] (the "schema square" framing).
    fn sample_schema() -> Schema {
        let edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("child"),
            kind: Name::from("prop"),
            name: Some(Name::from("child")),
        };
        let mut vertices = HashMap::new();
        for (id, kind) in [("root", "object"), ("child", "string")] {
            vertices.insert(
                Name::from(id),
                Vertex {
                    id: Name::from(id),
                    kind: Name::from(kind),
                    nsid: None,
                },
            );
        }
        let mut edges = HashMap::new();
        edges.insert(edge.clone(), edge.kind.clone());
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        incoming.entry(edge.tgt.clone()).or_default().push(edge);

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
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
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn rename_sort_protolens_is_natural() {
        // Sort-level rename: the operation vocabulary is unchanged, so the
        // square is the identity square and commutes.
        let protolens = elementary::rename_sort("A", "A2");
        let base = base_theory();
        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }

    #[test]
    fn add_sort_protolens_is_natural() {
        let protolens = elementary::add_sort("C", "object", panproto_inst::value::Value::Null);
        let base = base_theory();
        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }

    #[test]
    fn drop_sort_protolens_is_natural() {
        let protolens = elementary::drop_sort("B");
        let base = base_theory();
        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }

    #[test]
    fn rename_op_protolens_is_natural_via_equation() {
        // Operation-level rename: the naturality square for `f` reduces to
        // `f(x) = g(x)`, which the checker discharges via the rename
        // equation added to the codomain. This exercises the checker's
        // equational normalization, not just a syntactic identity.
        let protolens = elementary::rename_op("f", "g");
        let base = base_theory();
        let square = protolens_naturality_square(&protolens, &base);

        // The target morphism genuinely renames the op; the source does not.
        assert_eq!(
            square.g.op_map.get(&Arc::from("f")).and_then(|a| a.as_op()),
            Some(&Arc::from("g"))
        );
        assert_eq!(
            square.f.op_map.get(&Arc::from("f")).and_then(|a| a.as_op()),
            Some(&Arc::from("f"))
        );
        assert!(
            !square.codomain.directed_eqs.is_empty(),
            "rename equation present"
        );

        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }

    #[test]
    fn drop_op_protolens_is_natural() {
        // Dropping `f` removes it from the shared domain; naturality holds
        // over the surviving (empty-op) structure.
        let protolens = elementary::drop_op("f");
        let base = base_theory();
        let square = protolens_naturality_square(&protolens, &base);
        assert!(square.domain.ops.is_empty(), "dropped op leaves the domain");
        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }

    #[test]
    fn schema_derived_base_theory_is_natural() {
        // Sample the base theory from a schema's implicit theory, then check
        // an elementary protolens over it.
        let base = schema_to_implicit_theory(&sample_schema());
        let protolens = elementary::rename_op("prop", "prop2");
        assert!(check_protolens_naturality(&protolens, &base).is_ok());
    }
}
