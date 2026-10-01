//! A total, deterministic byte encoding of a [`Theory`] and of a
//! [`Composition`], and the digests over them.
//!
//! [`canonical_theory_bytes`] depends on a theory's content only: the
//! theory's name, its `extends` list, then its sorts, operations,
//! equations, directed equations and policies, each vector written in
//! ascending name order (elements that share a name in ascending order of
//! their encodings), so two theories that declare the same elements in
//! different orders encode alike. Every element is written with each of its
//! fields in declaration order; terms, sort expressions and the other sums
//! are written as a one-byte discriminant before their payload, by
//! exhaustive matches; [`panproto_expr::Expr`] payloads go through
//! [`CanonicalWriter::expr`].
//!
//! [`canonical_composition_bytes`] writes a composition as its theory's
//! bytes, then its inclusions in node order with each morphism's sort and
//! operation maps in ascending key order, then its provenance maps in key
//! order, then its report. A `HashMap` inside a morphism therefore never
//! leaks its iteration order into the bytes.
//!
//! The encodings follow the rules of [`crate::canonical`]: every
//! variable-length run is length-prefixed, and every map is sorted before
//! it is written.
//!
//! ## Digests
//!
//! | Context                 | Input                              |
//! |-------------------------|------------------------------------|
//! | [`THEORY_CONTEXT`]      | [`canonical_theory_bytes`]         |
//! | [`COMPOSITION_CONTEXT`] | [`canonical_composition_bytes`]    |
//!
//! [`theory_digest`] and [`composition_digest`] are blake3 key derivations
//! under these contexts. Neither is part of a schema's canonical bytes, so
//! no schema or lens identity depends on them.

use std::sync::Arc;

use panproto_gat::{
    AddedEquation, CaseBranch, CheckOutcome, CheckRecord, CheckStatus, CheckSubject, CoercionClass,
    Composition, CompositionCheckKind, CompositionIncomplete, CompositionMode, CompositionReport,
    ConflictPolicy, ConflictStrategy, DirectedEquation, Equation, Freshening, Implicit,
    InapplicableReason, OpAssignment, Operation, Provenance, Sort, SortClosure, SortExpr, SortKind,
    SortParam, SymKind, SymRef, Term, Theory, TheoryMorphism, ValueKind,
};

use crate::canonical::CanonicalWriter;

/// The blake3 key-derivation context for a theory's identity. Its input is
/// [`canonical_theory_bytes`].
pub const THEORY_CONTEXT: &str = "panproto 2026-09 theory identity v1";

/// The blake3 key-derivation context for a theory composition's identity.
/// Its input is [`canonical_composition_bytes`].
pub const COMPOSITION_CONTEXT: &str = "panproto 2026-09 theory composition v1";

/// Encode a theory as a total, deterministic byte string.
///
/// Equal theories encode alike in any process and under any hash seed, and
/// the order in which a theory lists its sorts, operations, equations,
/// directed equations or policies does not matter, even when two of its
/// elements share a name; any other difference changes the bytes.
///
/// # Examples
///
/// ```
/// use panproto_gat::{Operation, Sort, Theory};
/// use panproto_schema::canonical_theory::canonical_theory_bytes;
///
/// let a = Theory::new(
///     "G",
///     vec![Sort::simple("V"), Sort::simple("E")],
///     vec![Operation::unary("src", "e", "E", "V")],
///     vec![],
/// );
/// let b = Theory::new(
///     "G",
///     vec![Sort::simple("E"), Sort::simple("V")],
///     vec![Operation::unary("src", "e", "E", "V")],
///     vec![],
/// );
/// assert_eq!(canonical_theory_bytes(&a), canonical_theory_bytes(&b));
/// ```
#[must_use]
pub fn canonical_theory_bytes(theory: &Theory) -> Vec<u8> {
    let mut out = CanonicalWriter::new();
    write_theory(&mut out, theory);
    out.into_bytes()
}

/// Encode a composition as a total, deterministic byte string: its theory,
/// its inclusions in node order with sorted maps, its provenance in key
/// order, and its report.
#[must_use]
pub fn canonical_composition_bytes(composition: &Composition) -> Vec<u8> {
    let mut out = CanonicalWriter::new();
    write_composition(&mut out, composition);
    out.into_bytes()
}

/// The blake3 key derivation of a theory's [`canonical_theory_bytes`] under
/// [`THEORY_CONTEXT`].
#[must_use]
pub fn theory_digest(theory: &Theory) -> [u8; 32] {
    blake3::derive_key(THEORY_CONTEXT, &canonical_theory_bytes(theory))
}

/// The blake3 key derivation of a composition's
/// [`canonical_composition_bytes`] under [`COMPOSITION_CONTEXT`].
#[must_use]
pub fn composition_digest(composition: &Composition) -> [u8; 32] {
    blake3::derive_key(
        COMPOSITION_CONTEXT,
        &canonical_composition_bytes(composition),
    )
}

// ─── theories ──────────────────────────────────────────────────────

/// Write `items` as a length-prefixed run, each element encoded by `write`,
/// in ascending order of the name `key` gives and, among elements that
/// share a name, of their encodings. The order is total, so the order in
/// which `items` lists its elements never reaches the bytes.
fn write_by_name<T>(
    out: &mut CanonicalWriter,
    items: &[T],
    key: impl Fn(&T) -> &str,
    write: impl Fn(&mut CanonicalWriter, &T),
) {
    let mut encoded: Vec<(&str, Vec<u8>)> = items
        .iter()
        .map(|item| {
            let mut element = CanonicalWriter::new();
            write(&mut element, item);
            (key(item), element.into_bytes())
        })
        .collect();
    encoded.sort();
    out.len(encoded.len());
    for (_, bytes) in encoded {
        out.raw(&bytes);
    }
}

fn write_theory(out: &mut CanonicalWriter, theory: &Theory) {
    out.str(&theory.name);
    out.len(theory.extends.len());
    for parent in &theory.extends {
        out.str(parent);
    }
    write_by_name(out, &theory.sorts, |s| &s.name, write_sort);
    write_by_name(out, &theory.ops, |o| &o.name, write_op);
    write_by_name(out, &theory.eqs, |e| &e.name, write_equation);
    write_by_name(
        out,
        &theory.directed_eqs,
        |d| &d.name,
        write_directed_equation,
    );
    write_by_name(out, &theory.policies, |p| &p.name, write_policy);
}

fn write_sort(out: &mut CanonicalWriter, sort: &Sort) {
    let Sort {
        name,
        params,
        kind,
        closure,
    } = sort;
    out.str(name);
    out.len(params.len());
    for SortParam { name, sort } in params {
        out.str(name);
        write_sort_expr(out, sort);
    }
    write_sort_kind(out, kind);
    match closure {
        SortClosure::Open => out.tag(0),
        SortClosure::Closed(constructors) => {
            out.tag(1);
            out.len(constructors.len());
            for constructor in constructors {
                out.str(constructor);
            }
        }
    }
}

fn write_sort_kind(out: &mut CanonicalWriter, kind: &SortKind) {
    match kind {
        SortKind::Structural => out.tag(0),
        SortKind::Val(value) => {
            out.tag(1);
            write_value_kind(out, *value);
        }
        SortKind::Coercion { from, to, class } => {
            out.tag(2);
            write_value_kind(out, *from);
            write_value_kind(out, *to);
            write_coercion_class(out, *class);
        }
        SortKind::Merger(value) => {
            out.tag(3);
            write_value_kind(out, *value);
        }
    }
}

fn write_value_kind(out: &mut CanonicalWriter, kind: ValueKind) {
    out.tag(match kind {
        ValueKind::Bool => 0,
        ValueKind::Int => 1,
        ValueKind::Float => 2,
        ValueKind::Str => 3,
        ValueKind::Bytes => 4,
        ValueKind::Token => 5,
        ValueKind::Null => 6,
        ValueKind::Any => 7,
        ValueKind::DateTime => 8,
        ValueKind::Date => 9,
        ValueKind::Time => 10,
        ValueKind::Decimal => 11,
        ValueKind::Uuid => 12,
    });
}

/// A coercion class is written as its position in [`CoercionClass::all`],
/// the order panproto-gat keeps in lockstep with the (non-exhaustive) enum.
fn write_coercion_class(out: &mut CanonicalWriter, class: CoercionClass) {
    let position = CoercionClass::all()
        .iter()
        .position(|known| *known == class)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(u8::MAX);
    out.tag(position);
}

fn write_sort_expr(out: &mut CanonicalWriter, expr: &SortExpr) {
    match expr {
        SortExpr::Name(name) => {
            out.tag(0);
            out.str(name);
        }
        SortExpr::App { name, args } => {
            out.tag(1);
            out.str(name);
            out.len(args.len());
            for arg in args {
                write_term(out, arg);
            }
        }
    }
}

fn write_op(out: &mut CanonicalWriter, op: &Operation) {
    let Operation {
        name,
        inputs,
        output,
    } = op;
    out.str(name);
    out.len(inputs.len());
    for (var, sort, implicit) in inputs {
        out.str(var);
        write_sort_expr(out, sort);
        out.tag(match implicit {
            Implicit::No => 0,
            Implicit::Yes => 1,
        });
    }
    write_sort_expr(out, output);
}

fn write_term(out: &mut CanonicalWriter, term: &Term) {
    match term {
        Term::Var(name) => {
            out.tag(0);
            out.str(name);
        }
        Term::App { op, args } => {
            out.tag(1);
            out.str(op);
            out.len(args.len());
            for arg in args {
                write_term(out, arg);
            }
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            out.tag(2);
            write_term(out, scrutinee);
            out.len(branches.len());
            for CaseBranch {
                constructor,
                binders,
                body,
            } in branches
            {
                out.str(constructor);
                out.len(binders.len());
                for binder in binders {
                    out.str(binder);
                }
                write_term(out, body);
            }
        }
        Term::Hole { name } => {
            out.tag(3);
            write_opt_str(out, name.as_ref());
        }
        Term::Let { name, bound, body } => {
            out.tag(4);
            out.str(name);
            write_term(out, bound);
            write_term(out, body);
        }
    }
}

fn write_opt_str(out: &mut CanonicalWriter, value: Option<&Arc<str>>) {
    match value {
        None => out.tag(0),
        Some(value) => {
            out.tag(1);
            out.str(value);
        }
    }
}

fn write_equation(out: &mut CanonicalWriter, eq: &Equation) {
    let Equation { name, lhs, rhs } = eq;
    out.str(name);
    write_term(out, lhs);
    write_term(out, rhs);
}

fn write_directed_equation(out: &mut CanonicalWriter, eq: &DirectedEquation) {
    let DirectedEquation {
        name,
        lhs,
        rhs,
        impl_term,
        inverse,
        source_kind,
        target_kind,
        coercion_class,
    } = eq;
    out.str(name);
    write_term(out, lhs);
    write_term(out, rhs);
    out.expr(impl_term);
    match inverse {
        None => out.tag(0),
        Some(expr) => {
            out.tag(1);
            out.expr(expr);
        }
    }
    for kind in [source_kind, target_kind] {
        match kind {
            None => out.tag(0),
            Some(kind) => {
                out.tag(1);
                write_value_kind(out, *kind);
            }
        }
    }
    write_coercion_class(out, *coercion_class);
}

fn write_policy(out: &mut CanonicalWriter, policy: &ConflictPolicy) {
    let ConflictPolicy {
        name,
        value_kind,
        strategy,
    } = policy;
    out.str(name);
    write_value_kind(out, *value_kind);
    match strategy {
        ConflictStrategy::KeepLeft => out.tag(0),
        ConflictStrategy::KeepRight => out.tag(1),
        ConflictStrategy::Fail => out.tag(2),
        ConflictStrategy::Custom(expr) => {
            out.tag(3);
            out.expr(expr);
        }
    }
}

// ─── compositions ──────────────────────────────────────────────────

fn write_composition(out: &mut CanonicalWriter, composition: &Composition) {
    let Composition {
        theory,
        inclusions,
        provenance,
        report,
    } = composition;
    write_theory(out, theory);
    out.len(inclusions.len());
    for inclusion in inclusions {
        write_morphism(out, inclusion);
    }
    write_provenance(out, provenance);
    write_report(out, report);
}

fn write_morphism(out: &mut CanonicalWriter, morphism: &TheoryMorphism) {
    let TheoryMorphism {
        name,
        domain,
        codomain,
        sort_map,
        op_map,
    } = morphism;
    out.str(name);
    out.str(domain);
    out.str(codomain);
    let mut sorts: Vec<(&Arc<str>, &Arc<str>)> = sort_map.iter().collect();
    sorts.sort();
    out.len(sorts.len());
    for (from, to) in sorts {
        out.str(from);
        out.str(to);
    }
    let mut ops: Vec<(&Arc<str>, &OpAssignment)> = op_map.iter().collect();
    ops.sort_by(|a, b| a.0.cmp(b.0));
    out.len(ops.len());
    for (from, image) in ops {
        out.str(from);
        match image {
            OpAssignment::Op(to) => {
                out.tag(0);
                out.str(to);
            }
            OpAssignment::Term(term) => {
                out.tag(1);
                write_term(out, term);
            }
        }
    }
}

fn write_sym_ref(out: &mut CanonicalWriter, sym: &SymRef) {
    let SymRef { node, name } = sym;
    out.str(node);
    out.str(name);
}

fn write_sym_kind(out: &mut CanonicalWriter, kind: SymKind) {
    out.tag(match kind {
        SymKind::Sort => 0,
        SymKind::Op => 1,
        SymKind::Eq => 2,
        SymKind::DirectedEq => 3,
        SymKind::Policy => 4,
    });
}

fn write_provenance(out: &mut CanonicalWriter, provenance: &Provenance) {
    let Provenance {
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
        added_eqs,
    } = provenance;
    for map in [sorts, ops, eqs, directed_eqs, policies] {
        out.len(map.len());
        for (source, image) in map {
            write_sym_ref(out, source);
            out.str(image);
        }
    }
    out.len(added_eqs.len());
    for (name, AddedEquation { edge, op }) in added_eqs {
        out.str(name);
        out.len(*edge);
        write_sym_ref(out, op);
    }
}

fn write_report(out: &mut CanonicalWriter, report: &CompositionReport) {
    let CompositionReport {
        mode,
        status,
        name_clash_free,
        checks,
        freshened,
    } = report;
    out.tag(match mode {
        CompositionMode::Amalgamate => 0,
        CompositionMode::ExplicitIdentity => 1,
    });
    write_status(out, *status);
    out.bool(*name_clash_free);
    out.len(checks.len());
    for record in checks {
        write_check_record(out, record);
    }
    out.len(freshened.len());
    for Freshening {
        kind,
        members,
        candidate,
        result,
    } in freshened
    {
        write_sym_kind(out, *kind);
        out.len(members.len());
        for member in members {
            write_sym_ref(out, member);
        }
        out.str(candidate);
        out.str(result);
    }
}

fn write_status(out: &mut CanonicalWriter, status: CheckStatus) {
    out.tag(match status {
        CheckStatus::Passed => 0,
        CheckStatus::Failed => 1,
        CheckStatus::Incomplete => 2,
    });
}

fn write_check_record(out: &mut CanonicalWriter, record: &CheckRecord) {
    let CheckRecord {
        check,
        subject,
        outcome,
    } = record;
    out.tag(match check {
        CompositionCheckKind::LegIsMorphism => 0,
        CompositionCheckKind::NodeWellFormed => 1,
        CompositionCheckKind::ClassSignaturesAgree => 2,
        CompositionCheckKind::CoconeCommutes => 3,
        CompositionCheckKind::InclusionIsMorphism => 4,
        CompositionCheckKind::EquationImage => 5,
        CompositionCheckKind::EquationEntailed => 6,
        CompositionCheckKind::UniversalIdentity => 7,
        CompositionCheckKind::UniversalAgainst => 8,
        CompositionCheckKind::ResultWellTyped => 9,
        CompositionCheckKind::NameClashFree => 10,
    });
    match subject {
        CheckSubject::Diagram => out.tag(0),
        CheckSubject::Node(label) => {
            out.tag(1);
            out.str(label);
        }
        CheckSubject::Edge(index) => {
            out.tag(2);
            out.len(*index);
        }
        CheckSubject::Cocone(index) => {
            out.tag(3);
            out.len(*index);
        }
    }
    match outcome {
        CheckOutcome::Passed => out.tag(0),
        CheckOutcome::PassedByEquation { equation } => {
            out.tag(1);
            out.str(equation);
        }
        CheckOutcome::Failed { detail } => {
            out.tag(2);
            out.str(detail);
        }
        CheckOutcome::Inapplicable { reason } => {
            out.tag(3);
            match reason {
                InapplicableReason::OpenTheory { missing_sorts } => {
                    out.tag(0);
                    out.len(missing_sorts.len());
                    for sort in missing_sorts {
                        out.str(sort);
                    }
                }
                InapplicableReason::NoEquations => out.tag(1),
            }
        }
        CheckOutcome::Incomplete { reason } => {
            out.tag(4);
            match reason {
                CompositionIncomplete::RewriteBudgetExhausted { equation, steps } => {
                    out.tag(0);
                    out.str(equation);
                    out.u32(*steps);
                }
                CompositionIncomplete::NoTerminatingRewriteSystem => out.tag(1),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use panproto_gat::{
        ClashPolicy, ComposeOptions, CompositionMode, DEFAULT_VERIFY_BUDGET, Equation, Operation,
        Sort, SortParam, Term, Theory, TheoryDiagram, TheoryMorphism, compose,
    };

    use super::*;

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(char::from(DIGITS[usize::from(b >> 4)]));
            out.push(char::from(DIGITS[usize::from(b & 0xf)]));
        }
        out
    }

    fn graph() -> Theory {
        Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![],
        )
    }

    fn constraint() -> Theory {
        Theory::new(
            "ThConstraint",
            vec![
                Sort::simple("Vertex"),
                Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
            ],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            vec![],
        )
    }

    fn looped() -> Theory {
        Theory::new(
            "ThLoop",
            vec![Sort::simple("Node"), Sort::simple("Arc")],
            vec![
                Operation::unary("from", "a", "Arc", "Node"),
                Operation::unary("to", "a", "Arc", "Node"),
            ],
            vec![Equation::new(
                "loop",
                Term::app("from", vec![Term::var("a")]),
                Term::app("to", vec![Term::var("a")]),
            )],
        )
    }

    fn apex() -> Theory {
        Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![])
    }

    /// A leg from the apex, its sort map built in the given insertion order
    /// so two builds hold differently ordered hash maps.
    fn leg(tgt: &str, sorts: &[(&str, &str)]) -> TheoryMorphism {
        let map: HashMap<Arc<str>, Arc<str>> = sorts
            .iter()
            .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
            .collect();
        TheoryMorphism::new(
            format!("into_{tgt}"),
            "ThVertex",
            tgt,
            map,
            HashMap::<Arc<str>, OpAssignment>::new(),
        )
    }

    fn opts() -> ComposeOptions {
        ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: "Composed".into(),
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        }
    }

    fn diagram(node_order: &[usize], edge_order: &[usize]) -> TheoryDiagram {
        let nodes = [
            ("ThVertex", apex()),
            ("ThGraph", graph()),
            ("ThConstraint", constraint()),
            ("ThLoop", looped()),
        ];
        let edges = [
            ("ThGraph", leg("ThGraph", &[("Vertex", "Vertex")])),
            ("ThConstraint", leg("ThConstraint", &[("Vertex", "Vertex")])),
            ("ThLoop", leg("ThLoop", &[("Vertex", "Node")])),
        ];
        TheoryDiagram {
            nodes: node_order
                .iter()
                .map(|&i| panproto_gat::DiagramNode {
                    label: nodes[i].0.into(),
                    theory: nodes[i].1.clone(),
                })
                .collect(),
            edges: edge_order
                .iter()
                .map(|&i| panproto_gat::DiagramEdge {
                    src: "ThVertex".into(),
                    tgt: edges[i].0.into(),
                    morphism: edges[i].1.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn theory_context_strings_are_the_contract_strings() {
        assert_eq!(THEORY_CONTEXT, "panproto 2026-09 theory identity v1");
        assert_eq!(
            COMPOSITION_CONTEXT,
            "panproto 2026-09 theory composition v1"
        );
        let theory = graph();
        assert_eq!(
            theory_digest(&theory),
            blake3::derive_key(THEORY_CONTEXT, &canonical_theory_bytes(&theory))
        );
        let composition = compose(&diagram(&[0, 1, 2, 3], &[0, 1, 2]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        assert_eq!(
            composition_digest(&composition),
            blake3::derive_key(
                COMPOSITION_CONTEXT,
                &canonical_composition_bytes(&composition)
            )
        );
    }

    /// Pins the encodings: a change to either is a change to every stored
    /// theory or composition identity and must bump its context string.
    #[test]
    fn theory_and_composition_digests_are_pinned() {
        let composition = compose(&diagram(&[0, 1, 2, 3], &[0, 1, 2]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        assert_eq!(
            hex(&theory_digest(&constraint())),
            "78b885aada86cdec2a622b25723885bfc4f2a8ccc14fce8fc297b1784ec190b1",
            "theory digest"
        );
        assert_eq!(
            hex(&composition_digest(&composition)),
            "f573b186b8dcd719d4571522fcdb01a313c354cfa93e96e450441581b73e3826",
            "composition digest"
        );
    }

    #[test]
    fn theory_bytes_ignore_declaration_order_and_nothing_else() {
        let theory = looped();
        let mut reordered = Theory::new(
            "ThLoop",
            theory.sorts.iter().rev().cloned().collect(),
            theory.ops.iter().rev().cloned().collect(),
            theory.eqs.clone(),
        );
        assert_eq!(
            canonical_theory_bytes(&theory),
            canonical_theory_bytes(&reordered)
        );
        reordered.name = "ThLoop2".into();
        assert_ne!(
            canonical_theory_bytes(&theory),
            canonical_theory_bytes(&reordered)
        );
        let flipped = Theory::new(
            "ThLoop",
            theory.sorts.clone(),
            theory.ops.clone(),
            vec![Equation::new(
                "loop",
                Term::app("to", vec![Term::var("a")]),
                Term::app("from", vec![Term::var("a")]),
            )],
        );
        assert_ne!(
            canonical_theory_bytes(&theory),
            canonical_theory_bytes(&flipped)
        );
        // Length prefixes keep adjacent names apart.
        let ab = Theory::new("ab", vec![Sort::simple("c")], vec![], vec![]);
        let a = Theory::new("a", vec![Sort::simple("bc")], vec![], vec![]);
        assert_ne!(canonical_theory_bytes(&ab), canonical_theory_bytes(&a));
    }

    /// Two elements of one kind that share a name, as `Theory::full`
    /// accepts them, listed in either order.
    fn same_name_pairs(reversed: bool) -> Theory {
        fn pair<T>(reversed: bool, a: T, b: T) -> Vec<T> {
            if reversed { vec![b, a] } else { vec![a, b] }
        }
        let directed = |lhs: &str, rhs: &str| DirectedEquation {
            name: "d".into(),
            lhs: Term::app("f", vec![Term::var(lhs)]),
            rhs: Term::var(rhs),
            impl_term: panproto_expr::Expr::Var("x".into()),
            inverse: None,
            source_kind: None,
            target_kind: None,
            coercion_class: CoercionClass::Iso,
        };
        let policy = |strategy| ConflictPolicy {
            name: "p".into(),
            value_kind: panproto_gat::ValueKind::Str,
            strategy,
        };
        Theory::full(
            "T",
            vec![],
            pair(
                reversed,
                Sort::simple("V"),
                Sort::dependent("V", vec![SortParam::new("v", "W")]),
            ),
            pair(
                reversed,
                Operation::unary("f", "x", "V", "W"),
                Operation::unary("f", "x", "W", "V"),
            ),
            pair(
                reversed,
                Equation::new("e", Term::var("x"), Term::var("y")),
                Equation::new("e", Term::var("y"), Term::var("x")),
            ),
            pair(reversed, directed("x", "x"), directed("y", "y")),
            pair(
                reversed,
                policy(ConflictStrategy::KeepLeft),
                policy(ConflictStrategy::KeepRight),
            ),
        )
    }

    #[test]
    fn theory_bytes_order_same_name_elements_by_their_content() {
        let forward = same_name_pairs(false);
        let backward = same_name_pairs(true);
        for (kind, len) in [
            ("sorts", forward.sorts.len()),
            ("ops", forward.ops.len()),
            ("eqs", forward.eqs.len()),
            ("directed_eqs", forward.directed_eqs.len()),
            ("policies", forward.policies.len()),
        ] {
            assert_eq!(len, 2, "the theory keeps both {kind}");
        }
        assert_eq!(
            hex(&canonical_theory_bytes(&forward)),
            hex(&canonical_theory_bytes(&backward))
        );
        // Each pair still differs by content: dropping either element of
        // any pair changes the bytes.
        let mut fewer = forward.clone();
        fewer.eqs.pop();
        assert_ne!(
            canonical_theory_bytes(&forward),
            canonical_theory_bytes(&fewer)
        );
    }

    #[test]
    fn composition_bytes_are_order_independent() {
        let reference = compose(&diagram(&[0, 1, 2, 3], &[0, 1, 2]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        let bytes = canonical_composition_bytes(&reference);
        // The same diagram rebuilt, so every morphism's hash maps are new.
        let again = compose(&diagram(&[0, 1, 2, 3], &[0, 1, 2]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        assert_eq!(canonical_composition_bytes(&again), bytes);
        // Edges in another order.
        let edges = compose(&diagram(&[0, 1, 2, 3], &[2, 0, 1]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        assert_eq!(canonical_composition_bytes(&edges), bytes);
        // Nodes in another order: only the inclusions follow the node
        // order, so putting them back in the reference order restores the
        // reference bytes.
        let order = [3, 1, 0, 2];
        let mut permuted = compose(&diagram(&order, &[1, 2, 0]), &opts())
            .unwrap_or_else(|e| panic!("the diagram composes: {e}"));
        assert_ne!(canonical_composition_bytes(&permuted), bytes);
        let mut inclusions = vec![None; 4];
        for (position, &node) in order.iter().enumerate() {
            inclusions[node] = Some(permuted.inclusions[position].clone());
        }
        permuted.inclusions = inclusions
            .into_iter()
            .map(|i| i.unwrap_or_else(|| panic!("every node has an inclusion")))
            .collect();
        assert_eq!(canonical_composition_bytes(&permuted), bytes);
        // A serde round trip keeps the bytes.
        let json = serde_json::to_string(&reference).unwrap_or_else(|e| panic!("encodes: {e}"));
        let decoded: Composition =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("decodes: {e}"));
        assert_eq!(canonical_composition_bytes(&decoded), bytes);
    }
}
