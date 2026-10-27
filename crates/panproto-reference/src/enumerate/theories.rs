//! Complete finite scopes for the theory page.
//!
//! The scopes are built over one fixed alphabet, the two-sort alphabet:
//!
//! - sorts named `A` and `B`; `A` is simple, open or closed (its
//!   constructors are then every operation whose output is `A`, in
//!   declaration order), and structural or, when it is the only sort, of
//!   kind `value`; `B` is simple or depends on one parameter `p : A`;
//! - at most two operations, `f` and then `g`, of arity at most two, whose
//!   inputs are named `x` and then `y`; an input's and the output's sort is
//!   `A`, a simple `B`, or the dependent `B` applied to an earlier input of
//!   sort `A`;
//! - at most one equation `e`, both of whose sides are terms of depth at
//!   most one: the variables `x` and `y` and the constants (the nullary
//!   operations applied to nothing). Depth is that of the term action's
//!   rewriting bound: a variable, a hole and a constant have depth one.
//!
//! Every enumerator lists labelled structures (names and declaration order
//! are part of the structure) with no pruning of isomorphic copies; the
//! count of each is computed by the page's tests from a closed formula,
//! independently of the enumeration.

use crate::schema::Schema;
use crate::theory::{
    CaseBranch, ClashPolicy, CoercionClass, ComposeOptions, CompositionMode, CompositionSpec,
    CompositionStep, ConflictStrategy, Equation, Expression, Implicit, Morphism, Op, OpImage,
    RecipeEdge, RecipeOpImage, Sort, SortExpr, SortKind, Term, Theory, TheoryDiagram, ValueKind,
};

/// Every two-object schema with one kind, one edge name and at most one edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every word of length at most two over a supplied labelled alphabet.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 2)
}

/// Every subset of size at most two of a supplied labelled set.
#[must_use]
pub fn subsets<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    super::subsets(items, 2)
}

/// The kind of a structural sort.
pub const STRUCTURAL: SortKind = SortKind::Structural;
/// The kind of a value sort.
pub const VALUE: SortKind = SortKind::Val(ValueKind::Str);
/// The name of every theory the scopes build.
pub const THEORY_NAME: &str = "Th";

/// The sort declarations of a signature of the two-sort alphabet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortShape {
    /// Whether `A` is declared.
    pub a: bool,
    /// Whether `A` is closed.
    pub a_closed: bool,
    /// Whether `A` has kind `value`.
    pub a_value: bool,
    /// `None` without `B`; `Some(false)` for a simple `B`; `Some(true)`
    /// for `B(p : A)`.
    pub b: Option<bool>,
}

impl SortShape {
    /// The eight sort shapes, in enumeration order: none; `A` open, closed
    /// and of kind `value`; `A` open and closed with a simple `B`; `A` open
    /// and closed with the dependent `B`.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let only = |a_closed, a_value| Self {
            a: true,
            a_closed,
            a_value,
            b: None,
        };
        let with = |a_closed, dependent| Self {
            a: true,
            a_closed,
            a_value: false,
            b: Some(dependent),
        };
        vec![
            Self {
                a: false,
                a_closed: false,
                a_value: false,
                b: None,
            },
            only(false, false),
            only(true, false),
            only(false, true),
            with(false, false),
            with(true, false),
            with(false, true),
            with(true, true),
        ]
    }

    /// The sort expressions an input or output may have, given the earlier
    /// inputs: `A`, a simple `B`, and the dependent `B` applied to each
    /// earlier input whose sort is `A`.
    fn expressions(self, earlier: &[(String, SortExpr, Implicit)]) -> Vec<SortExpr> {
        let mut out = Vec::new();
        if self.a {
            out.push(SortExpr::Name("A".to_owned()));
        }
        match self.b {
            Some(false) => out.push(SortExpr::Name("B".to_owned())),
            Some(true) => {
                for (name, sort, _) in earlier {
                    if *sort == SortExpr::Name("A".to_owned()) {
                        out.push(SortExpr::App("B".to_owned(), vec![Term::Var(name.clone())]));
                    }
                }
            }
            None => {}
        }
        out
    }

    /// Every operation signature of arity at most two, named `name`.
    #[must_use]
    pub fn operations(self, name: &str) -> Vec<Op> {
        let mut out = Vec::new();
        let mut frontier: Vec<Vec<(String, SortExpr, Implicit)>> = vec![Vec::new()];
        for arity in 0..=2 {
            let mut next = Vec::new();
            for inputs in &frontier {
                for output in self.expressions(inputs) {
                    out.push(Op {
                        name: name.to_owned(),
                        inputs: inputs.clone(),
                        output,
                    });
                }
                if arity < 2 {
                    let binder = ["x", "y"][arity];
                    for sort in self.expressions(inputs) {
                        let mut grown = inputs.clone();
                        grown.push((binder.to_owned(), sort, Implicit::No));
                        next.push(grown);
                    }
                }
            }
            frontier = next;
        }
        out
    }

    fn sorts(self, ops: &[Op]) -> Vec<Sort> {
        let mut sorts = Vec::new();
        if self.a {
            sorts.push(Sort {
                name: "A".to_owned(),
                params: Vec::new(),
                kind: if self.a_value { VALUE } else { STRUCTURAL },
                constructors: self.a_closed.then(|| {
                    ops.iter()
                        .filter(|op| op.output.head() == "A")
                        .map(|op| op.name.clone())
                        .collect()
                }),
            });
        }
        if let Some(dependent) = self.b {
            sorts.push(Sort {
                name: "B".to_owned(),
                params: if dependent {
                    vec![("p".to_owned(), SortExpr::Name("A".to_owned()))]
                } else {
                    Vec::new()
                },
                kind: STRUCTURAL,
                constructors: None,
            });
        }
        sorts
    }
}

/// Every operation list of a sort shape: none, `[f]`, and `[f, g]`.
#[must_use]
pub fn operation_lists(shape: SortShape) -> Vec<Vec<Op>> {
    let firsts = shape.operations("f");
    let seconds = shape.operations("g");
    let mut out = vec![Vec::new()];
    out.extend(firsts.iter().map(|f| vec![f.clone()]));
    for f in &firsts {
        for g in &seconds {
            out.push(vec![f.clone(), g.clone()]);
        }
    }
    out
}

/// The terms of depth at most one over the variables `x` and `y` and the
/// nullary operations of `ops`, in that order.
#[must_use]
pub fn shallow_terms(ops: &[Op]) -> Vec<Term> {
    let mut out = vec![Term::Var("x".to_owned()), Term::Var("y".to_owned())];
    out.extend(
        ops.iter()
            .filter(|op| op.inputs.is_empty())
            .map(|op| Term::App(op.name.clone(), Vec::new())),
    );
    out
}

fn theory(shape: SortShape, ops: Vec<Op>) -> Theory {
    Theory {
        name: THEORY_NAME.to_owned(),
        sorts: shape.sorts(&ops),
        ops,
        ..Theory::default()
    }
}

/// Every signature of the two-sort alphabet without an equation.
#[must_use]
pub fn bare_signatures() -> Vec<Theory> {
    let mut out = Vec::new();
    for shape in SortShape::all() {
        for ops in operation_lists(shape) {
            out.push(theory(shape, ops));
        }
    }
    out
}

/// Every signature of the two-sort alphabet: each bare signature with no
/// equation and with every equation `e : l = r` between terms of depth at
/// most one.
#[must_use]
pub fn signatures() -> Vec<Theory> {
    let mut out = Vec::new();
    for bare in bare_signatures() {
        let terms = shallow_terms(&bare.ops);
        out.push(bare.clone());
        for lhs in &terms {
            for rhs in &terms {
                let mut with = bare.clone();
                with.eqs.push(Equation {
                    name: "e".to_owned(),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                });
                out.push(with);
            }
        }
    }
    out
}

/// The sort expressions of the raw alphabet: the declared heads, the
/// undeclared `C`, `A` applied to an argument, and `B` applied to `x`,
/// `y` and the unbound `z`.
#[must_use]
pub fn raw_sort_expressions() -> Vec<SortExpr> {
    let app =
        |head: &str, var: &str| SortExpr::App(head.to_owned(), vec![Term::Var(var.to_owned())]);
    vec![
        SortExpr::Name("A".to_owned()),
        SortExpr::Name("B".to_owned()),
        SortExpr::Name("C".to_owned()),
        app("A", "x"),
        app("B", "x"),
        app("B", "y"),
        app("B", "z"),
    ]
}

/// The sort declarations of the raw family.
///
/// They are `A`; `A` and `B`; `A` and `B(p : A)`; `A` and `B(p : B)` (a self-dependency); `A(p : B)` and
/// `B(q : A)` (a cycle); `A` twice; and `A` with `B(p : C)` (an undeclared
/// parameter sort).
#[must_use]
pub fn raw_sort_lists() -> Vec<Vec<Sort>> {
    let sort = |name: &str, param: Option<(&str, &str)>| Sort {
        name: name.to_owned(),
        params: param
            .map(|(p, s)| vec![(p.to_owned(), SortExpr::Name(s.to_owned()))])
            .unwrap_or_default(),
        kind: STRUCTURAL,
        constructors: None,
    };
    vec![
        vec![sort("A", None)],
        vec![sort("A", None), sort("B", None)],
        vec![sort("A", None), sort("B", Some(("p", "A")))],
        vec![sort("A", None), sort("B", Some(("p", "B")))],
        vec![sort("A", Some(("p", "B"))), sort("B", Some(("q", "A")))],
        vec![sort("A", None), sort("A", None)],
        vec![sort("A", None), sort("B", Some(("p", "C")))],
    ]
}

/// Every operation `f` of arity at most two whose inputs are named from
/// `x` and `y` (repetition allowed) and whose input and output sorts are
/// drawn from [`raw_sort_expressions`].
#[must_use]
pub fn raw_operations() -> Vec<Op> {
    let exprs = raw_sort_expressions();
    let binders = ["x", "y"];
    let mut out = Vec::new();
    let mut frontier: Vec<Vec<(String, SortExpr, Implicit)>> = vec![Vec::new()];
    for _ in 0..=2 {
        let mut next = Vec::new();
        for inputs in &frontier {
            for output in &exprs {
                out.push(Op {
                    name: "f".to_owned(),
                    inputs: inputs.clone(),
                    output: output.clone(),
                });
            }
            for binder in binders {
                for sort in &exprs {
                    let mut grown = inputs.clone();
                    grown.push((binder.to_owned(), sort.clone(), Implicit::No));
                    next.push(grown);
                }
            }
        }
        frontier = next;
    }
    out
}

/// The raw family of signature-checking inputs.
///
/// It holds every sort list of [`raw_sort_lists`] with no operation and with each operation of
/// [`raw_operations`]; then the five duplicate-symbol theories (two sorts,
/// two operations, two equations, two directed equations, two policies
/// sharing one name).
#[must_use]
pub fn raw_signatures() -> Vec<Theory> {
    let mut out = Vec::new();
    let ops = raw_operations();
    for sorts in raw_sort_lists() {
        out.push(Theory {
            name: THEORY_NAME.to_owned(),
            sorts: sorts.clone(),
            ..Theory::default()
        });
        for op in &ops {
            out.push(Theory {
                name: THEORY_NAME.to_owned(),
                sorts: sorts.clone(),
                ops: vec![op.clone()],
                ..Theory::default()
            });
        }
    }
    out.extend(duplicate_symbol_theories());
    out
}

fn duplicate_symbol_theories() -> Vec<Theory> {
    use crate::theory::{DirectedEquation, Policy};
    let a = || Sort {
        name: "A".to_owned(),
        params: Vec::new(),
        kind: STRUCTURAL,
        constructors: None,
    };
    let constant = || Op {
        name: "f".to_owned(),
        inputs: Vec::new(),
        output: SortExpr::Name("A".to_owned()),
    };
    let base = || Theory {
        name: THEORY_NAME.to_owned(),
        sorts: vec![a()],
        ops: vec![constant()],
        ..Theory::default()
    };
    let c = || Term::App("f".to_owned(), Vec::new());
    let equation = || Equation {
        name: "e".to_owned(),
        lhs: c(),
        rhs: c(),
    };
    let directed = || DirectedEquation {
        name: "d".to_owned(),
        lhs: c(),
        rhs: c(),
        implementation: Expression::Var("x".to_owned()),
        inverse: None,
        source_kind: None,
        target_kind: None,
        coercion_class: CoercionClass::Iso,
    };
    let policy = || Policy {
        name: "q".to_owned(),
        value_kind: ValueKind::Str,
        strategy: ConflictStrategy::KeepLeft,
    };
    vec![
        Theory {
            sorts: vec![a(), a()],
            ..base()
        },
        Theory {
            ops: vec![constant(), constant()],
            ..base()
        },
        Theory {
            eqs: vec![equation(), equation()],
            ..base()
        },
        Theory {
            directed_eqs: vec![directed(), directed()],
            ..base()
        },
        Theory {
            policies: vec![policy(), policy()],
            ..base()
        },
    ]
}

/// The images an operation `op` of the domain may have in `codomain`.
///
/// They are each codomain operation by name, then each term of depth at most one
/// over `op`'s inputs, that is each input variable and each codomain
/// constant.
#[must_use]
pub fn operation_images(op: &Op, codomain: &Theory) -> Vec<OpImage> {
    let mut out: Vec<OpImage> = codomain
        .ops
        .iter()
        .map(|target| OpImage::Op(target.name.clone()))
        .collect();
    out.extend(
        op.inputs
            .iter()
            .map(|(name, _, _)| OpImage::Term(Term::Var(name.clone()))),
    );
    out.extend(
        codomain
            .ops
            .iter()
            .filter(|target| target.inputs.is_empty())
            .map(|target| OpImage::Term(Term::App(target.name.clone(), Vec::new()))),
    );
    out
}

/// Every total assignment from `domain` to `codomain`: each domain sort to
/// a codomain sort, and each domain operation to one of its
/// [`operation_images`]. The morphism is named `m`.
#[must_use]
pub fn morphisms(domain: &Theory, codomain: &Theory) -> Vec<Morphism> {
    let mut out = vec![Morphism {
        name: "m".to_owned(),
        domain: domain.name.clone(),
        codomain: codomain.name.clone(),
        ..Morphism::default()
    }];
    for sort in &domain.sorts {
        out = out
            .into_iter()
            .flat_map(|partial| {
                codomain.sorts.iter().map(move |target| {
                    let mut grown = partial.clone();
                    grown.sorts.insert(sort.name.clone(), target.name.clone());
                    grown
                })
            })
            .collect();
    }
    for op in &domain.ops {
        let images = operation_images(op, codomain);
        out = out
            .into_iter()
            .flat_map(|partial| {
                images.iter().map(move |image| {
                    let mut grown = partial.clone();
                    grown.ops.insert(op.name.clone(), image.clone());
                    grown
                })
            })
            .collect();
    }
    out
}

/// The assignments that are not total or name what the codomain does not
/// declare.
///
/// They reach the refusals a total assignment cannot: for each
/// domain sort, the identity-on-names assignment without it and with it
/// sent to the undeclared `Z`; for each domain operation, the same
/// assignment without it and with it sent to the undeclared `h`. The
/// identity-on-names assignment sends each symbol to the codomain symbol
/// of its name when there is one, and leaves it out otherwise.
#[must_use]
pub fn partial_morphisms(domain: &Theory, codomain: &Theory) -> Vec<Morphism> {
    let mut identity = Morphism {
        name: "m".to_owned(),
        domain: domain.name.clone(),
        codomain: codomain.name.clone(),
        ..Morphism::default()
    };
    for sort in &domain.sorts {
        if codomain.sorts.iter().any(|target| target.name == sort.name) {
            identity.sorts.insert(sort.name.clone(), sort.name.clone());
        }
    }
    for op in &domain.ops {
        if codomain.ops.iter().any(|target| target.name == op.name) {
            identity
                .ops
                .insert(op.name.clone(), OpImage::Op(op.name.clone()));
        }
    }
    let mut out = Vec::new();
    for sort in &domain.sorts {
        let mut without = identity.clone();
        without.sorts.remove(&sort.name);
        out.push(without);
        let mut unknown = identity.clone();
        unknown.sorts.insert(sort.name.clone(), "Z".to_owned());
        out.push(unknown);
    }
    for op in &domain.ops {
        let mut without = identity.clone();
        without.ops.remove(&op.name);
        out.push(without);
        let mut unknown = identity.clone();
        unknown
            .ops
            .insert(op.name.clone(), OpImage::Op("h".to_owned()));
        out.push(unknown);
    }
    out
}

/// The four option sets of a composition: each mode with each clash
/// policy, named `R` with the rewrite budget 512.
#[must_use]
pub fn option_sets() -> Vec<ComposeOptions> {
    let mut out = Vec::new();
    for mode in [
        CompositionMode::Amalgamate,
        CompositionMode::ExplicitIdentity,
    ] {
        for clash in [ClashPolicy::Freshen, ClashPolicy::Reject] {
            out.push(ComposeOptions {
                mode,
                result_name: "R".to_owned(),
                clash,
                rewrite_budget: 512,
            });
        }
    }
    out
}

/// Every identification list of length at most `max_len` over the pairs
/// of `0..=size` (the index `size` is out of range).
#[must_use]
pub fn identifications(size: usize, max_len: usize) -> Vec<Vec<(usize, usize)>> {
    let pairs: Vec<(usize, usize)> = (0..=size)
        .flat_map(|left| (0..=size).map(move |right| (left, right)))
        .collect();
    super::sequences(&pairs, max_len)
}

/// The recipes of the replay scope over a registry holding `L` and `R`.
///
/// They are the base step of `L`, of `R` and of the missing `M`; the by-name
/// colimit of `L` and `R` in each mode over every subset of their common
/// sort names and every subset of their common operation names, and over
/// the absent sort `Z`; and the explicit diagram step of the span
/// `L <- I -> R` for each of the four option sets, `I` an inline theory
/// declaring `A` and the identity-on-names legs, together with the same
/// step naming a node that is neither inline nor registered. Every recipe
/// is named `out` and states version 1.
#[must_use]
pub fn recipes(left: &Theory, right: &Theory) -> Vec<CompositionSpec> {
    let spec = |steps| CompositionSpec {
        result_name: "out".to_owned(),
        steps,
        version: 1,
    };
    let mut out = vec![
        spec(vec![CompositionStep::Base("L".to_owned())]),
        spec(vec![CompositionStep::Base("R".to_owned())]),
        spec(vec![CompositionStep::Base("M".to_owned())]),
    ];
    let common_sorts: Vec<String> = left
        .sorts
        .iter()
        .filter(|sort| right.sorts.iter().any(|other| other.name == sort.name))
        .map(|sort| sort.name.clone())
        .collect();
    let common_ops: Vec<String> = left
        .ops
        .iter()
        .filter(|op| right.ops.iter().any(|other| other.name == op.name))
        .map(|op| op.name.clone())
        .collect();
    for mode in [
        CompositionMode::Amalgamate,
        CompositionMode::ExplicitIdentity,
    ] {
        for shared_sorts in super::subsets(&common_sorts, common_sorts.len()) {
            for shared_ops in super::subsets(&common_ops, common_ops.len()) {
                out.push(spec(vec![CompositionStep::Colimit {
                    left: "L".to_owned(),
                    right: "R".to_owned(),
                    shared_sorts: shared_sorts.clone(),
                    shared_ops,
                    mode,
                }]));
            }
        }
        out.push(spec(vec![CompositionStep::Colimit {
            left: "L".to_owned(),
            right: "R".to_owned(),
            shared_sorts: vec!["Z".to_owned()],
            shared_ops: Vec::new(),
            mode,
        }]));
    }
    let interface = Theory {
        name: "I".to_owned(),
        sorts: vec![Sort {
            name: "A".to_owned(),
            params: Vec::new(),
            kind: STRUCTURAL,
            constructors: None,
        }],
        ..Theory::default()
    };
    let leg = |target: &str| RecipeEdge {
        src: "I".to_owned(),
        tgt: target.to_owned(),
        sorts: vec![("A".to_owned(), "A".to_owned())],
        ops: Vec::<(String, RecipeOpImage)>::new(),
    };
    for options in option_sets() {
        for nodes in [["I", "L", "R"], ["I", "L", "M"]] {
            out.push(spec(vec![CompositionStep::Diagram {
                nodes: nodes.iter().map(|node| (*node).to_owned()).collect(),
                edges: vec![leg(nodes[1]), leg(nodes[2])],
                mode: options.mode,
                clash: options.clash,
                rewrite_budget: options.rewrite_budget,
                inline: vec![interface.clone()],
            }]));
        }
    }
    out
}

/// A family of morphism checks: every leg against every domain and every
/// codomain, with probe terms on which the whole action is compared.
#[derive(Clone, Debug)]
pub struct ActionScope {
    /// The domains.
    pub domains: Vec<Theory>,
    /// The codomains.
    pub codomains: Vec<Theory>,
    /// The legs, each from every domain to every codomain.
    pub legs: Vec<Morphism>,
    /// Domain terms on which the whole action is compared.
    pub probes: Vec<Term>,
}

fn var(name: &str) -> Term {
    Term::Var(name.to_owned())
}

fn constant(name: &str) -> Term {
    Term::App(name.to_owned(), Vec::new())
}

fn app(op: &str, args: Vec<Term>) -> Term {
    Term::App(op.to_owned(), args)
}

fn simple(name: &str) -> SortExpr {
    SortExpr::Name(name.to_owned())
}

fn sort_app(head: &str, args: Vec<Term>) -> SortExpr {
    SortExpr::App(head.to_owned(), args)
}

fn declare(name: &str, params: Vec<(&str, SortExpr)>, constructors: Option<&[&str]>) -> Sort {
    Sort {
        name: name.to_owned(),
        params: params.into_iter().map(|(p, s)| (p.to_owned(), s)).collect(),
        kind: STRUCTURAL,
        constructors: constructors.map(|cs| cs.iter().map(|c| (*c).to_owned()).collect()),
    }
}

fn operation(name: &str, inputs: Vec<(&str, SortExpr)>, output: SortExpr) -> Op {
    Op {
        name: name.to_owned(),
        inputs: inputs
            .into_iter()
            .map(|(n, s)| (n.to_owned(), s, Implicit::No))
            .collect(),
        output,
    }
}

/// `X`, `Y(x : X)`, `H(x : X, y : X)`, the closed `B` over `closure`, and
/// `Z(w : Y(z_arg()))`.
fn action_sorts(closure: &[&str], z_arg: &str) -> Vec<Sort> {
    vec![
        declare("X", Vec::new(), None),
        declare("Y", vec![("x", simple("X"))], None),
        declare("H", vec![("x", simple("X")), ("y", simple("X"))], None),
        declare("B", Vec::new(), Some(closure)),
        declare("Z", vec![("w", sort_app("Y", vec![constant(z_arg)]))], None),
    ]
}

/// The domain of the whole-action family: constants `c, d : X`,
/// constructors `t, f : B`, `p : Y(p_arg())`, `r(z : X) : r_out` and
/// `q(u : X, v : X) : H(u, v)`.
fn action_domain(p_arg: &str, r_out: SortExpr) -> Theory {
    Theory {
        name: "D".to_owned(),
        sorts: action_sorts(&["t", "f"], "c"),
        ops: vec![
            operation("c", Vec::new(), simple("X")),
            operation("d", Vec::new(), simple("X")),
            operation("t", Vec::new(), simple("B")),
            operation("f", Vec::new(), simple("B")),
            operation("p", Vec::new(), sort_app("Y", vec![constant(p_arg)])),
            operation("r", vec![("z", simple("X"))], r_out),
            operation(
                "q",
                vec![("u", simple("X")), ("v", simple("X"))],
                sort_app("H", vec![var("u"), var("v")]),
            ),
        ],
        ..Theory::default()
    }
}

/// The codomain of the whole-action family, with `B` closed over
/// `closure` and `Z(w : Y(z_arg()))`: `c, d : X`, `t, f : B`, `p : Y(c())`,
/// `p2 : Y(d())`, `r(z : X) : H(z, c())`, `r2(w : X) : H(w, d())`,
/// `q(z : X, z : X) : H(z, z)`, `q2(a : X, b : X) : H(a, b)` and
/// `neg(b : B) : B`.
fn action_codomain(closure: &[&str], z_arg: &str) -> Theory {
    let x = || simple("X");
    Theory {
        name: "C".to_owned(),
        sorts: action_sorts(closure, z_arg),
        ops: vec![
            operation("c", Vec::new(), x()),
            operation("d", Vec::new(), x()),
            operation("t", Vec::new(), simple("B")),
            operation("f", Vec::new(), simple("B")),
            operation("p", Vec::new(), sort_app("Y", vec![constant("c")])),
            operation("p2", Vec::new(), sort_app("Y", vec![constant("d")])),
            operation(
                "r",
                vec![("z", x())],
                sort_app("H", vec![var("z"), constant("c")]),
            ),
            operation(
                "r2",
                vec![("w", x())],
                sort_app("H", vec![var("w"), constant("d")]),
            ),
            operation(
                "q",
                vec![("z", x()), ("z", x())],
                sort_app("H", vec![var("z"), var("z")]),
            ),
            operation(
                "q2",
                vec![("a", x()), ("b", x())],
                sort_app("H", vec![var("a"), var("b")]),
            ),
            operation("neg", vec![("b", simple("B"))], simple("B")),
        ],
        ..Theory::default()
    }
}

fn product_legs(
    domain: &str,
    codomain: &str,
    sorts: &[&str],
    images: Vec<(&str, Vec<OpImage>)>,
) -> Vec<Morphism> {
    let base = Morphism {
        name: "m".to_owned(),
        domain: domain.to_owned(),
        codomain: codomain.to_owned(),
        sorts: sorts
            .iter()
            .map(|s| ((*s).to_owned(), (*s).to_owned()))
            .collect(),
        ops: std::collections::BTreeMap::new(),
    };
    let mut legs = vec![base];
    for (op, choices) in images {
        legs = legs
            .into_iter()
            .flat_map(|partial| {
                choices.iter().map(move |image| {
                    let mut grown = partial.clone();
                    grown.ops.insert(op.to_owned(), image.clone());
                    grown
                })
            })
            .collect();
    }
    legs
}

/// The whole-action family.
///
/// It has six domains (`p : Y(c())` or `Y(d())`, and
/// `r`'s output `H(z, c())`, `H(c(), z)` or `H(z, z)`), six codomains
/// (`B` closed over `{t, f}`, `{t}` or `{f}`, and `Z` over `Y(c())` or
/// `Y(d())`), and 7200 legs: the identity on sorts with one candidate
/// image per domain operation, renames and derived terms built from
/// constants, constructors and the operation's inputs; probes are a case
/// analysis over `B`, uses of `r` and `q`, and a local binding.
#[must_use]
pub fn whole_action_scope() -> ActionScope {
    let rename = |name: &str| OpImage::Op(name.to_owned());
    let term = OpImage::Term;
    let mut domains = Vec::new();
    for p_arg in ["c", "d"] {
        for r_out in [
            sort_app("H", vec![var("z"), constant("c")]),
            sort_app("H", vec![constant("c"), var("z")]),
            sort_app("H", vec![var("z"), var("z")]),
        ] {
            domains.push(action_domain(p_arg, r_out));
        }
    }
    let mut codomains = Vec::new();
    for closure in [&["t", "f"][..], &["t"][..], &["f"][..]] {
        for z_arg in ["c", "d"] {
            codomains.push(action_codomain(closure, z_arg));
        }
    }
    let images = vec![
        (
            "c",
            vec![
                rename("c"),
                rename("d"),
                term(constant("c")),
                term(constant("d")),
            ],
        ),
        ("d", vec![rename("d"), rename("c"), term(constant("d"))]),
        (
            "t",
            vec![
                rename("t"),
                rename("f"),
                term(constant("t")),
                term(constant("f")),
                term(app("neg", vec![constant("f")])),
            ],
        ),
        ("f", vec![rename("f"), rename("t"), term(constant("f"))]),
        ("p", vec![rename("p"), rename("p2")]),
        (
            "r",
            vec![
                rename("r"),
                rename("r2"),
                term(app("r", vec![var("z")])),
                term(app("r2", vec![var("z")])),
                term(app("r", vec![constant("c")])),
            ],
        ),
        (
            "q",
            vec![
                rename("q"),
                rename("q2"),
                term(app("q2", vec![var("u"), var("v")])),
                term(app("q2", vec![var("v"), var("u")])),
            ],
        ),
    ];
    let probes = vec![
        Term::Case(
            Box::new(var("b")),
            vec![
                crate::theory::CaseBranch {
                    constructor: "t".to_owned(),
                    binders: Vec::new(),
                    body: constant("f"),
                },
                crate::theory::CaseBranch {
                    constructor: "f".to_owned(),
                    binders: Vec::new(),
                    body: app("r", vec![constant("c")]),
                },
            ],
        ),
        app("q", vec![constant("d"), app("r", vec![constant("c")])]),
        Term::Let(
            "n".to_owned(),
            Box::new(constant("c")),
            Box::new(app("q", vec![var("n"), constant("d")])),
        ),
    ];
    ActionScope {
        domains,
        codomains,
        legs: product_legs("D", "C", &["X", "Y", "H", "B", "Z"], images),
        probes,
    }
}

/// The constructor family.
///
/// Its domains have `X`, `a : X`, `B` closed over
/// `c(first : X, second : X) : B` and `pick(b : B) : X`, the input names
/// of `c` being `(z, w)`, `(z, z)` or `(w, z)`; codomains with `X`,
/// `a : X`, `B` closed over `{k}`, `{k, k1}`, `{k1}` or nothing,
/// `k(u : X, v : X) : B`, `k1(u : X) : B` and `pick(b : B) : X`; and 16
/// legs sending `c` to `k`, to `k1`, to `k1` and `k` applied to every
/// choice of `z`, `w` and `a()`, to an over-applied `k`, or to a bare
/// variable, every other symbol to itself.
#[must_use]
pub fn constructor_scope() -> ActionScope {
    let x = || simple("X");
    let domains = [("z", "w"), ("z", "z"), ("w", "z")]
        .into_iter()
        .map(|(first, second)| Theory {
            name: "D".to_owned(),
            sorts: vec![
                declare("X", Vec::new(), None),
                declare("B", Vec::new(), Some(&["c"])),
            ],
            ops: vec![
                operation("a", Vec::new(), x()),
                operation("c", vec![(first, x()), (second, x())], simple("B")),
                operation("pick", vec![("b", simple("B"))], x()),
            ],
            ..Theory::default()
        })
        .collect();
    let codomains = [&["k"][..], &["k", "k1"][..], &["k1"][..], &[][..]]
        .into_iter()
        .map(|closure| Theory {
            name: "C".to_owned(),
            sorts: vec![
                declare("X", Vec::new(), None),
                declare("B", Vec::new(), Some(closure)),
            ],
            ops: vec![
                operation("a", Vec::new(), x()),
                operation("k", vec![("u", x()), ("v", x())], simple("B")),
                operation("k1", vec![("u", x())], simple("B")),
                operation("pick", vec![("b", simple("B"))], x()),
            ],
            ..Theory::default()
        })
        .collect();
    let leaves = [var("z"), var("w"), constant("a")];
    let mut images = vec![OpImage::Op("k".to_owned()), OpImage::Op("k1".to_owned())];
    for first in &leaves {
        images.push(OpImage::Term(app("k1", vec![first.clone()])));
        for second in &leaves {
            images.push(OpImage::Term(app("k", vec![first.clone(), second.clone()])));
        }
    }
    images.push(OpImage::Term(app("k", vec![var("z"), var("w"), var("z")])));
    images.push(OpImage::Term(var("z")));
    let legs = product_legs(
        "D",
        "C",
        &["X", "B"],
        vec![
            ("a", vec![OpImage::Op("a".to_owned())]),
            ("c", images),
            ("pick", vec![OpImage::Op("pick".to_owned())]),
        ],
    );
    let probes = vec![
        Term::Case(
            Box::new(var("b")),
            vec![crate::theory::CaseBranch {
                constructor: "c".to_owned(),
                binders: vec!["p".to_owned(), "q".to_owned()],
                body: var("p"),
            }],
        ),
        app("c", vec![constant("a"), var("y")]),
        app("pick", vec![app("c", vec![var("y"), constant("a")])]),
    ];
    ActionScope {
        domains,
        codomains,
        legs,
        probes,
    }
}

/// A theory with `X`, `Y(x : X)`, a constant `constant : X` and a constant
/// `fibre : Y(constant())`.
fn fibred(name: &str, constant_name: &str, fibre: &str) -> Theory {
    Theory {
        name: name.to_owned(),
        sorts: vec![
            declare("X", Vec::new(), None),
            declare("Y", vec![("x", simple("X"))], None),
        ],
        ops: vec![
            operation(constant_name, Vec::new(), simple("X")),
            operation(
                fibre,
                Vec::new(),
                sort_app("Y", vec![constant(constant_name)]),
            ),
        ],
        ..Theory::default()
    }
}

/// The fibre span over `constant` with fibres `fibres` in `S`, `L` and
/// `R`.
///
/// Each leg is the identity on `X` and `Y`, renames the constant or
/// sends it to the derived term that is the target's constant, as
/// `renames` says, and renames the apex's fibre to the target's.
#[must_use]
pub fn fibre_span(
    constant_name: &str,
    fibres: [&str; 3],
    renames: [bool; 2],
) -> crate::theory::TheoryDiagram {
    let [h, f, g] = fibres;
    let leg = |target: &str, fibre: &str, renamed: bool| {
        let mut ops = std::collections::BTreeMap::new();
        ops.insert(
            constant_name.to_owned(),
            if renamed {
                OpImage::Op(constant_name.to_owned())
            } else {
                OpImage::Term(constant(constant_name))
            },
        );
        ops.insert(h.to_owned(), OpImage::Op(fibre.to_owned()));
        Morphism {
            name: format!("S_to_{target}"),
            domain: "S".to_owned(),
            codomain: target.to_owned(),
            sorts: [("X", "X"), ("Y", "Y")]
                .into_iter()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .collect(),
            ops,
        }
    };
    crate::theory::TheoryDiagram::span(
        ("S".to_owned(), fibred("S", constant_name, h)),
        ("L".to_owned(), fibred("L", constant_name, f)),
        ("R".to_owned(), fibred("R", constant_name, g)),
        leg("L", f, renames[0]),
        leg("R", g, renames[1]),
    )
}

/// The four fibre spans over `a` with fibres `h`, `f` and `g`, one per
/// choice of each leg's image of `a`.
#[must_use]
pub fn fibre_spans() -> Vec<crate::theory::TheoryDiagram> {
    [[true, true], [true, false], [false, true], [false, false]]
        .into_iter()
        .map(|renames| fibre_span("a", ["h", "f", "g"], renames))
        .collect()
}

/// Operations in the prefix-valid class-comparison scope.
pub const CLASS_OPERATION_SCOPE_LEN: usize = 2_815;
/// Sorts in the prefix-valid class-comparison scope.
pub const CLASS_SORT_SCOPE_LEN: usize = 260;

const BASE_CLASS_OPERATION_SCOPE_LEN: usize = 2_797;
const SCOPED_BINDER_OPERATION_LEN: usize = 18;
const BASE_CLASS_SORT_SCOPE_LEN: usize = 244;
const SCOPED_BINDER_SORT_LEN: usize = 16;

#[derive(Clone, Debug)]
enum ClassTermForm {
    Bound(usize),
    App(&'static str, Vec<Self>),
}

#[derive(Clone, Debug)]
struct ClassSortForm {
    head: &'static str,
    args: Option<Vec<ClassTermForm>>,
}

#[derive(Clone, Debug)]
struct ClassOpForm {
    inputs: Vec<(ClassSortForm, Implicit)>,
    output: ClassSortForm,
}

fn class_bound_positions(prefix: &[ClassSortForm]) -> Vec<usize> {
    prefix
        .iter()
        .enumerate()
        .filter(|(_, sort)| sort.head == "X")
        .map(|(index, _)| index)
        .collect()
}

const fn class_constant(name: &'static str) -> ClassTermForm {
    ClassTermForm::App(name, Vec::new())
}

fn class_sort_form_has_bare_x(sort: &ClassSortForm) -> bool {
    sort.head == "X" && sort.args.is_none()
}

fn class_operation_has_bare_x(operation: &ClassOpForm) -> bool {
    operation
        .inputs
        .iter()
        .any(|(sort, _)| class_sort_form_has_bare_x(sort))
        || class_sort_form_has_bare_x(&operation.output)
}

/// Prefix-valid sort forms. A dependent argument may mention only an
/// earlier binder whose sort is `X`; output forms additionally include `H`.
fn class_sort_forms(bound: &[usize], with_h: bool) -> Vec<ClassSortForm> {
    let mut atoms = vec![class_constant("a"), class_constant("b")];
    atoms.extend(bound.iter().copied().map(ClassTermForm::Bound));
    let mut out = vec![
        ClassSortForm {
            head: "X",
            args: None,
        },
        ClassSortForm {
            head: "Y",
            args: None,
        },
    ];
    for term in &atoms {
        out.push(ClassSortForm {
            head: "Y",
            args: Some(vec![term.clone()]),
        });
    }
    if with_h {
        for left in &atoms {
            for right in &atoms {
                out.push(ClassSortForm {
                    head: "H",
                    args: Some(vec![left.clone(), right.clone()]),
                });
            }
        }
    }
    out
}

fn class_names(arity: usize) -> Vec<Vec<&'static str>> {
    match arity {
        0 => vec![Vec::new()],
        1 => vec![vec!["x"], vec!["y"]],
        _ => vec![vec!["x", "y"], vec!["y", "x"]],
    }
}

fn render_class_term(term: &ClassTermForm, names: &[&str]) -> Term {
    match term {
        ClassTermForm::Bound(position) => var(names[*position]),
        ClassTermForm::App(operation, arguments) => app(
            operation,
            arguments
                .iter()
                .map(|argument| render_class_term(argument, names))
                .collect(),
        ),
    }
}

fn render_class_sort(sort: &ClassSortForm, names: &[&str], empty_app: bool) -> SortExpr {
    match &sort.args {
        Some(arguments) => sort_app(
            sort.head,
            arguments
                .iter()
                .map(|argument| render_class_term(argument, names))
                .collect(),
        ),
        None if empty_app && sort.head == "X" => sort_app("X", Vec::new()),
        None => simple(sort.head),
    }
}

fn render_class_operation(form: &ClassOpForm, names: &[&str], empty_app: bool) -> Op {
    Op {
        name: "member".to_owned(),
        inputs: form
            .inputs
            .iter()
            .enumerate()
            .map(|(index, (sort, implicit))| {
                (
                    names[index].to_owned(),
                    render_class_sort(sort, names, empty_app),
                    *implicit,
                )
            })
            .collect(),
        output: render_class_sort(&form.output, names, empty_app),
    }
}

fn extend_class_operations(
    prefix: &mut Vec<(ClassSortForm, Implicit)>,
    out: &mut Vec<ClassOpForm>,
) {
    let sorts: Vec<ClassSortForm> = prefix.iter().map(|(sort, _)| sort.clone()).collect();
    let bound = class_bound_positions(&sorts);
    for output in class_sort_forms(&bound, true) {
        out.push(ClassOpForm {
            inputs: prefix.clone(),
            output,
        });
    }
    if prefix.len() < 2 {
        for sort in class_sort_forms(&bound, false) {
            for implicit in [Implicit::No, Implicit::Yes] {
                prefix.push((sort.clone(), implicit));
                extend_class_operations(prefix, out);
                prefix.pop();
            }
        }
    }
}

/// An independent recurrence for the number of unique rendered operations.
/// `x_bound` is the number of earlier inputs whose sort is `X`; `has_bare_x`
/// records whether the prefix already contains an `X` spelling to vary.
fn class_operation_count(depth: usize, x_bound: usize, has_bare_x: bool) -> usize {
    let atoms = 2 + x_bound;
    let outputs = 2 + atoms + atoms.pow(2);
    let naming = if depth == 0 { 1 } else { 2 };
    let spellings = if has_bare_x { 2 * outputs } else { outputs + 1 };
    let here = naming * spellings;
    if depth == 2 {
        return here;
    }
    let non_x_inputs = 3 + x_bound;
    here + 2 * class_operation_count(depth + 1, x_bound + 1, true)
        + 2 * non_x_inputs * class_operation_count(depth + 1, x_bound, has_bare_x)
}

fn base_class_operation_scope() -> Vec<Op> {
    let mut forms = Vec::new();
    extend_class_operations(&mut Vec::new(), &mut forms);
    let mut out = Vec::new();
    for form in forms {
        let has_bare_x = class_operation_has_bare_x(&form);
        for names in class_names(form.inputs.len()) {
            out.push(render_class_operation(&form, &names, false));
            if has_bare_x {
                out.push(render_class_operation(&form, &names, true));
            }
        }
    }
    assert_eq!(
        class_operation_count(0, 0, false),
        BASE_CLASS_OPERATION_SCOPE_LEN
    );
    assert_eq!(out.len(), class_operation_count(0, 0, false));
    out
}

fn scoped_hole(name: Option<&str>) -> Term {
    Term::Let(
        "u".to_owned(),
        Box::new(Term::Hole(name.map(str::to_owned))),
        Box::new(constant("a")),
    )
}

fn scoped_shadow() -> Term {
    Term::Let("x".to_owned(), Box::new(var("x")), Box::new(var("x")))
}

fn scoped_nested_let(outer: &str, inner: &str, select_inner: bool) -> Term {
    Term::Let(
        outer.to_owned(),
        Box::new(var("x")),
        Box::new(Term::Let(
            inner.to_owned(),
            Box::new(app("s", vec![var(outer)])),
            Box::new(app(
                "s",
                vec![var(if select_inner { inner } else { outer })],
            )),
        )),
    )
}

fn scoped_case(first: &str, second: &str, select_last: bool) -> Term {
    Term::Case(
        Box::new(constant("z")),
        vec![
            CaseBranch {
                constructor: "z".to_owned(),
                binders: Vec::new(),
                body: app("s", vec![constant("a")]),
            },
            CaseBranch {
                constructor: "pair".to_owned(),
                binders: vec![first.to_owned(), second.to_owned()],
                body: app("s", vec![var(if select_last { second } else { first })]),
            },
        ],
    )
}

fn scoped_signature_let(local: &str, select_local: bool) -> Term {
    Term::Let(
        local.to_owned(),
        Box::new(constant("a")),
        Box::new(var(if select_local { local } else { "x" })),
    )
}

fn scoped_signature_case(local: &str, select_local: bool) -> Term {
    Term::Case(
        Box::new(constant("z")),
        vec![
            CaseBranch {
                constructor: "z".to_owned(),
                binders: Vec::new(),
                body: constant("a"),
            },
            CaseBranch {
                constructor: "pair".to_owned(),
                binders: vec![local.to_owned(), "v".to_owned()],
                body: var(if select_local { local } else { "x" }),
            },
        ],
    )
}

fn scoped_operation(inputs: Vec<(String, SortExpr, Implicit)>, output: Term) -> Op {
    Op {
        name: "member".to_owned(),
        inputs,
        output: sort_app("Y", vec![output]),
    }
}

/// The `Let` and `Case` capture witnesses for operation signatures.
///
/// The first two triples distinguish repeated local binders from alpha-renamed and
/// capture-changing forms. The last two distinguish a local binder that
/// shadows the signature input `x` from a renamed local and a use of the
/// signature input.
#[must_use]
pub fn class_operation_capture_witnesses() -> [[Op; 3]; 4] {
    let input = || vec![("x".to_owned(), simple("X"), Implicit::No)];
    [
        [
            scoped_operation(input(), scoped_nested_let("u", "u", true)),
            scoped_operation(input(), scoped_nested_let("p", "q", true)),
            scoped_operation(input(), scoped_nested_let("p", "q", false)),
        ],
        [
            scoped_operation(Vec::new(), scoped_case("u", "u", true)),
            scoped_operation(Vec::new(), scoped_case("p", "q", true)),
            scoped_operation(Vec::new(), scoped_case("p", "q", false)),
        ],
        [
            scoped_operation(input(), scoped_signature_let("x", true)),
            scoped_operation(input(), scoped_signature_let("u", true)),
            scoped_operation(input(), scoped_signature_let("u", false)),
        ],
        [
            scoped_operation(input(), scoped_signature_case("x", true)),
            scoped_operation(input(), scoped_signature_case("u", true)),
            scoped_operation(input(), scoped_signature_case("u", false)),
        ],
    ]
}

fn scoped_binder_operations() -> Vec<Op> {
    let y = |term| sort_app("Y", vec![term]);
    let input = |name: &str, sort: SortExpr, implicit| (name.to_owned(), sort, implicit);
    let mut out = vec![
        Op {
            name: "member".to_owned(),
            inputs: Vec::new(),
            output: y(scoped_hole(None)),
        },
        Op {
            name: "member".to_owned(),
            inputs: Vec::new(),
            output: y(scoped_hole(Some("h"))),
        },
        Op {
            name: "member".to_owned(),
            inputs: vec![input("x", simple("X"), Implicit::No)],
            output: y(scoped_shadow()),
        },
    ];
    out.extend(class_operation_capture_witnesses().into_iter().flatten());
    out.extend([
        Op {
            name: "member".to_owned(),
            inputs: vec![input("x", simple("X"), Implicit::No)],
            output: sort_app(
                "H",
                vec![
                    app("m", vec![app("s", vec![var("x")]), constant("a")]),
                    var("x"),
                ],
            ),
        },
        Op {
            name: "member".to_owned(),
            inputs: vec![
                input("x", simple("X"), Implicit::No),
                input("y", sort_app("Y", vec![var("x")]), Implicit::Yes),
                input("z", sort_app("H", vec![var("x"), var("x")]), Implicit::No),
            ],
            output: y(scoped_shadow()),
        },
        Op {
            name: "member".to_owned(),
            inputs: vec![
                input("x", simple("X"), Implicit::No),
                input("y", simple("X"), Implicit::No),
            ],
            output: y(scoped_case("u", "u", true)),
        },
    ]);
    out
}

/// Every well-formed operation signature of the class-comparison scope.
///
/// The base product has arity at most two, prefix-scoped dependencies, both
/// implicit flags and both `X` spellings without duplicate renderings. Eighteen
/// additional signatures cover arity three, applications, holes, shadowing,
/// alpha-equivalent and capture-distinguishing local binders, capture between
/// signature and local binders, and exhaustive case analysis without
/// introducing a duplicate telescope binder.
///
/// # Panics
///
/// Panics when the scope's length differs from its independently computed
/// count, which is a defect of this enumerator.
#[must_use]
pub fn class_operation_scope() -> Vec<Op> {
    let mut out = base_class_operation_scope();
    let scoped = scoped_binder_operations();
    assert_eq!(scoped.len(), SCOPED_BINDER_OPERATION_LEN);
    out.extend(scoped);
    assert_eq!(out.len(), CLASS_OPERATION_SCOPE_LEN);
    out
}

fn render_class_sort_declaration(
    params: &[ClassSortForm],
    names: &[&str],
    empty_app: bool,
    value_kind: bool,
    closed: bool,
) -> Sort {
    Sort {
        name: "W".to_owned(),
        params: params
            .iter()
            .enumerate()
            .map(|(index, sort)| {
                (
                    names[index].to_owned(),
                    render_class_sort(sort, names, empty_app),
                )
            })
            .collect(),
        kind: if value_kind { VALUE } else { STRUCTURAL },
        constructors: closed.then(|| vec!["a".to_owned()]),
    }
}

fn extend_class_sorts(prefix: &mut Vec<ClassSortForm>, out: &mut Vec<Vec<ClassSortForm>>) {
    out.push(prefix.clone());
    if prefix.len() < 2 {
        let bound = class_bound_positions(prefix);
        for sort in class_sort_forms(&bound, false) {
            prefix.push(sort);
            extend_class_sorts(prefix, out);
            prefix.pop();
        }
    }
}

fn class_sort_count(depth: usize, x_bound: usize, has_bare_x: bool) -> usize {
    let naming = if depth == 0 { 1 } else { 2 };
    let here = 4 * naming * if has_bare_x { 2 } else { 1 };
    if depth == 2 {
        return here;
    }
    here + class_sort_count(depth + 1, x_bound + 1, true)
        + (3 + x_bound) * class_sort_count(depth + 1, x_bound, has_bare_x)
}

fn base_class_sort_scope() -> Vec<Sort> {
    let mut forms = Vec::new();
    extend_class_sorts(&mut Vec::new(), &mut forms);
    let mut out = Vec::new();
    for params in forms {
        let has_bare_x = params.iter().any(class_sort_form_has_bare_x);
        for names in class_names(params.len()) {
            for value_kind in [false, true] {
                for closed in [false, true] {
                    out.push(render_class_sort_declaration(
                        &params, &names, false, value_kind, closed,
                    ));
                    if has_bare_x {
                        out.push(render_class_sort_declaration(
                            &params, &names, true, value_kind, closed,
                        ));
                    }
                }
            }
        }
    }
    assert_eq!(class_sort_count(0, 0, false), BASE_CLASS_SORT_SCOPE_LEN);
    assert_eq!(out.len(), class_sort_count(0, 0, false));
    out
}

fn scoped_binder_sorts() -> Vec<Sort> {
    let declaration = |params| Sort {
        name: "W".to_owned(),
        params,
        kind: STRUCTURAL,
        constructors: None,
    };
    let param = |name: &str, sort| (name.to_owned(), sort);
    let mut out = vec![
        declaration(vec![param("p", sort_app("Y", vec![scoped_hole(None)]))]),
        declaration(vec![param(
            "p",
            sort_app("Y", vec![scoped_hole(Some("h"))]),
        )]),
        declaration(vec![
            param("x", simple("X")),
            param("y", sort_app("Y", vec![scoped_shadow()])),
        ]),
    ];
    out.extend(class_sort_capture_witnesses().into_iter().flatten());
    out.push(declaration(vec![
        param("x", simple("X")),
        param("y", sort_app("Y", vec![var("x")])),
        param(
            "z",
            sort_app(
                "H",
                vec![
                    var("x"),
                    app("m", vec![app("s", vec![var("x")]), constant("a")]),
                ],
            ),
        ),
    ]));
    out
}

/// The local-local and telescope-local capture witnesses for sort declarations.
/// The entry order and meaning of each triple are the same as in
/// [`class_operation_capture_witnesses`].
#[must_use]
pub fn class_sort_capture_witnesses() -> [[Sort; 3]; 4] {
    let declaration = |params| Sort {
        name: "W".to_owned(),
        params,
        kind: STRUCTURAL,
        constructors: None,
    };
    let dependent_declaration = |term| {
        declaration(vec![
            ("x".to_owned(), simple("X")),
            ("y".to_owned(), sort_app("Y", vec![term])),
        ])
    };
    let case_declaration = |term| declaration(vec![("p".to_owned(), sort_app("Y", vec![term]))]);
    [
        [
            dependent_declaration(scoped_nested_let("u", "u", true)),
            dependent_declaration(scoped_nested_let("p", "q", true)),
            dependent_declaration(scoped_nested_let("p", "q", false)),
        ],
        [
            case_declaration(scoped_case("u", "u", true)),
            case_declaration(scoped_case("x", "y", true)),
            case_declaration(scoped_case("x", "y", false)),
        ],
        [
            dependent_declaration(scoped_signature_let("x", true)),
            dependent_declaration(scoped_signature_let("u", true)),
            dependent_declaration(scoped_signature_let("u", false)),
        ],
        [
            dependent_declaration(scoped_signature_case("x", true)),
            dependent_declaration(scoped_signature_case("u", true)),
            dependent_declaration(scoped_signature_case("u", false)),
        ],
    ]
}

/// Every well-formed sort declaration of the class-comparison scope.
///
/// The base product crosses prefix-scoped parameters with structural/value kind,
/// open/closed closure and bare/empty `X` spellings without duplicate
/// renderings. Sixteen additional declarations cover arity three, the
/// alpha-equivalent and capture-distinguishing local term forms, and capture
/// between telescope and local binders.
///
/// # Panics
///
/// Panics when the scope's length differs from its independently computed
/// count, which is a defect of this enumerator.
#[must_use]
pub fn class_sort_scope() -> Vec<Sort> {
    let mut out = base_class_sort_scope();
    let scoped = scoped_binder_sorts();
    assert_eq!(scoped.len(), SCOPED_BINDER_SORT_LEN);
    out.extend(scoped);
    assert_eq!(out.len(), CLASS_SORT_SCOPE_LEN);
    out
}

fn class_ambient(name: &str, extra_op: Option<Op>, extra_sort: Option<Sort>) -> Theory {
    let mut sorts = vec![
        declare("X", Vec::new(), None),
        declare("Y", vec![("x", simple("X"))], None),
        declare("H", vec![("p", simple("X")), ("q", simple("X"))], None),
        declare("B", Vec::new(), Some(&["z", "pair"])),
    ];
    if let Some(sort) = extra_sort {
        sorts.push(sort);
    }
    let mut ops = vec![
        operation("a", Vec::new(), simple("X")),
        operation("b", Vec::new(), simple("X")),
        operation("c", Vec::new(), simple("X")),
        operation("s", vec![("u", simple("X"))], simple("X")),
        operation(
            "m",
            vec![("u", simple("X")), ("v", simple("X"))],
            simple("X"),
        ),
        operation("z", Vec::new(), simple("B")),
        operation(
            "pair",
            vec![("left", simple("X")), ("right", simple("X"))],
            simple("B"),
        ),
    ];
    if let Some(op) = extra_op {
        ops.push(op);
    }
    Theory {
        name: name.to_owned(),
        sorts,
        ops,
        ..Theory::default()
    }
}

/// The three-node connector that compares two class members through public
/// composition in amalgamation mode.
///
/// Node `l` declares the left member under
/// the name `lead`, node `m` declares the left member under its own name and
/// node `r` the right member under its own name, each beside the same
/// ambient sorts and operations. The edge `m -> l` is the identity on the
/// ambient and sends `m`'s member to `lead`, so it is a theory morphism, and
/// the name rule joins `m`'s member to `r`'s. The class is thus
/// `{l.lead, m.<member>, r.<member>}`; its least member is `l.lead`, `m`'s
/// member agrees with it, and the pair `l.lead`, `r.<member>` carries two
/// names, so the tier-one verdict of the two members, not a name conflict,
/// is what composition reports.
fn class_connector(
    left: Theory,
    middle: Theory,
    right: Theory,
    renamed: (&str, &str),
    sort: bool,
) -> TheoryDiagram {
    let (from, to) = renamed;
    let image =
        |name: &str, of_kind: bool| if of_kind && name == from { to } else { name }.to_owned();
    let morphism = Morphism {
        name: "m_to_l".to_owned(),
        domain: middle.name.clone(),
        codomain: left.name.clone(),
        sorts: middle
            .sorts
            .iter()
            .map(|s| (s.name.clone(), image(&s.name, sort)))
            .collect(),
        ops: middle
            .ops
            .iter()
            .map(|o| (o.name.clone(), OpImage::Op(image(&o.name, !sort))))
            .collect(),
    };
    let mut diagram = TheoryDiagram::discrete(vec![
        ("l".to_owned(), left),
        ("m".to_owned(), middle),
        ("r".to_owned(), right),
    ]);
    diagram.edges.push(crate::theory::DiagramEdge {
        src: "m".to_owned(),
        tgt: "l".to_owned(),
        morphism,
    });
    diagram
}

/// The three-node connector diagram for two operation signatures.
///
/// Its operation class holds `left` twice (as `l.lead` and `m`'s member)
/// and `right` once (as `r`'s member), so that public composition in
/// amalgamation mode reports the tier-one verdict of `left` against
/// `right`.
#[must_use]
pub fn operation_class_pair(left: &Op, right: &Op) -> TheoryDiagram {
    let lead = Op {
        name: "lead".to_owned(),
        ..left.clone()
    };
    class_connector(
        class_ambient("L", Some(lead), None),
        class_ambient("M", Some(left.clone()), None),
        class_ambient("R", Some(right.clone()), None),
        (left.name.as_str(), "lead"),
        false,
    )
}

/// The three-node connector diagram for two sort declarations.
///
/// Its sort class holds `left` twice (as `l.lead` and `m`'s member) and
/// `right` once (as `r`'s member), so that public composition in
/// amalgamation mode reports the tier-one verdict of `left` against
/// `right`.
#[must_use]
pub fn sort_class_pair(left: &Sort, right: &Sort) -> TheoryDiagram {
    let lead = Sort {
        name: "lead".to_owned(),
        ..left.clone()
    };
    class_connector(
        class_ambient("L", None, Some(lead)),
        class_ambient("M", None, Some(left.clone())),
        class_ambient("R", None, Some(right.clone())),
        (left.name.as_str(), "lead"),
        true,
    )
}

fn sort_mismatch_target(name: &str, sort: Sort) -> Theory {
    let mut theory = class_ambient(name, None, Some(sort));
    theory
        .ops
        .push(operation("witness", Vec::new(), simple("W")));
    theory.ops.push(operation("k", Vec::new(), simple("W")));
    theory
}

fn sort_mismatch_span(left: Sort, right: Sort) -> TheoryDiagram {
    let source = Theory {
        name: "S".to_owned(),
        sorts: vec![declare("X", Vec::new(), None)],
        ops: vec![operation("witness", Vec::new(), simple("W"))],
        ..Theory::default()
    };
    let leg = |target: &str| Morphism {
        name: format!("S_to_{target}"),
        domain: "S".to_owned(),
        codomain: target.to_owned(),
        sorts: [("X", "X"), ("W", "W")]
            .into_iter()
            .map(|(from, to)| (from.to_owned(), to.to_owned()))
            .collect(),
        ops: std::collections::BTreeMap::from([(
            "witness".to_owned(),
            OpImage::Op("witness".to_owned()),
        )]),
    };
    TheoryDiagram::span(
        ("S".to_owned(), source),
        ("L".to_owned(), sort_mismatch_target("L", left)),
        ("R".to_owned(), sort_mismatch_target("R", right)),
        leg("L"),
        leg("R"),
    )
}

/// Four connector spans that reach, through public [`crate::theory::compose`],
/// each sort-class refusal detail: arity, parameter sort, kind and closure.
#[must_use]
pub fn sort_mismatch_spans() -> Vec<TheoryDiagram> {
    let open = |params, kind: SortKind| Sort {
        name: "W".to_owned(),
        params,
        kind,
        constructors: None,
    };
    vec![
        sort_mismatch_span(
            open(Vec::new(), STRUCTURAL),
            open(vec![("x".to_owned(), simple("X"))], STRUCTURAL),
        ),
        sort_mismatch_span(
            open(vec![("x".to_owned(), simple("X"))], STRUCTURAL),
            open(
                vec![("x".to_owned(), sort_app("Y", vec![constant("a")]))],
                STRUCTURAL,
            ),
        ),
        sort_mismatch_span(open(Vec::new(), STRUCTURAL), open(Vec::new(), VALUE)),
        sort_mismatch_span(
            open(Vec::new(), STRUCTURAL),
            Sort {
                name: "W".to_owned(),
                params: Vec::new(),
                kind: STRUCTURAL,
                constructors: Some(vec!["k".to_owned()]),
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    //! Every scope's size against a closed form derived from its definition,
    //! and its whole content against a recorded digest.

    use std::collections::BTreeSet;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    use super::*;
    use crate::theory::{
        Defect, MorphismError, Site, check_morphism, compose, typecheck_signature,
    };

    /// The BLAKE3 digest, as lowercase hex, of the debug rendering of every
    /// item, one per line.
    fn digest<T: std::fmt::Debug>(items: impl IntoIterator<Item = T>) -> String {
        let mut hasher =
            crate::blake3_ref::Hasher::new_derive_key("panproto-reference theory scope digest");
        for item in items {
            hasher.update(format!("{item:?}\n").as_bytes());
        }
        hasher
            .finalize()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"));
                hex
            })
    }

    /// Run `work` on a worker thread and return its result, failing when it
    /// has not returned after `seconds`.
    fn within<T: Send + 'static>(seconds: u64, work: impl FnOnce() -> T + Send + 'static) -> T {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("bounded".to_owned())
            .stack_size(64 << 20)
            .spawn(move || {
                let _ = sender.send(work());
            })
            .unwrap_or_else(|error| panic!("spawn the bounded worker: {error}"));
        match receiver.recv_timeout(Duration::from_secs(seconds)) {
            Ok(value) => value,
            Err(RecvTimeoutError::Timeout) => {
                panic!("the bounded work did not end within {seconds} s")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("the bounded work panicked"),
        }
    }

    fn assert_digest<T: std::fmt::Debug>(
        what: &str,
        items: impl IntoIterator<Item = T>,
        expected: &str,
    ) {
        assert_eq!(digest(items), expected, "content digest of {what}");
    }

    #[test]
    fn a_sortless_or_cyclic_signature_is_checked_in_bounded_time() {
        let cyclic = Theory {
            name: THEORY_NAME.to_owned(),
            sorts: raw_sort_lists()[4].clone(),
            ..Theory::default()
        };
        let (empty, cycle) = within(10, move || {
            (
                typecheck_signature(&Theory::default()),
                typecheck_signature(&cyclic),
            )
        });
        assert_eq!(empty, Ok(()), "a theory without sorts is well formed");
        assert_eq!(
            cycle,
            Err(vec![
                Defect::UndeclaredSort(Site::SortParam("A".to_owned(), 0), "B".to_owned()),
                Defect::UndeclaredSort(Site::SortParam("B".to_owned(), 0), "A".to_owned()),
            ]),
            "a dependency cycle leaves both parameter sorts undeclared"
        );
    }

    #[test]
    fn a_looping_rewrite_system_exhausts_the_morphism_budget_in_bounded_time() {
        let unary = |name: &str| operation(name, vec![("x", simple("A"))], simple("A"));
        let applied = |name: &str| app(name, vec![var("x")]);
        let equation = |name: &str, lhs: &str, rhs: &str| Equation {
            name: name.to_owned(),
            lhs: applied(lhs),
            rhs: applied(rhs),
        };
        let with = |name: &str, eqs: Vec<Equation>| Theory {
            name: name.to_owned(),
            sorts: vec![declare("A", Vec::new(), None)],
            ops: vec![unary("f"), unary("g"), unary("h")],
            eqs,
            ..Theory::default()
        };
        let domain = with("D", vec![equation("e", "f", "g")]);
        let codomain = with(
            "C",
            vec![equation("e1", "f", "h"), equation("e2", "h", "g")],
        );
        let identity = Morphism {
            name: "m".to_owned(),
            domain: "D".to_owned(),
            codomain: "C".to_owned(),
            sorts: [("A".to_owned(), "A".to_owned())].into(),
            ops: ["f", "g", "h"]
                .into_iter()
                .map(|name| (name.to_owned(), OpImage::Op(name.to_owned())))
                .collect(),
        };
        let outcome = within(10, move || check_morphism(&domain, &codomain, &identity));
        assert_eq!(
            outcome,
            Err(MorphismError::EquationPreservationUnknown {
                equation: "e".to_owned(),
                steps: 512,
            }),
            "rewriting f(x) and h(x) into each other spends the whole budget of 512 steps"
        );
    }

    #[test]
    fn composition_names_its_classes_in_bounded_time() {
        let node = |label: &str| {
            (
                label.to_owned(),
                Theory {
                    name: format!("T{label}"),
                    sorts: vec![declare("A", Vec::new(), None)],
                    ..Theory::default()
                },
            )
        };
        let diagram = TheoryDiagram::discrete(vec![node("L"), node("R")]);
        let names = within(10, move || {
            option_sets()
                .iter()
                .map(|options| {
                    compose(&diagram, options).map(|composition| {
                        composition
                            .theory
                            .sorts
                            .iter()
                            .map(|sort| sort.name.clone())
                            .collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>()
        });
        let rejected = Err(crate::theory::ComposeError::NameClash(
            crate::theory::SymKind::Sort,
            "A".to_owned(),
            Box::new(vec![
                vec![crate::theory::SymRef {
                    node: "L".to_owned(),
                    name: "A".to_owned(),
                }],
                vec![crate::theory::SymRef {
                    node: "R".to_owned(),
                    name: "A".to_owned(),
                }],
            ]),
        ));
        assert_eq!(
            names,
            vec![
                Ok(vec!["A".to_owned()]),
                Ok(vec!["A".to_owned()]),
                Ok(vec!["L.A".to_owned(), "R.A".to_owned()]),
                rejected,
            ],
            "amalgamation joins the two `A`s; explicit identity freshens or refuses them"
        );
    }

    #[test]
    fn small_alphabets_match_their_closed_form_counts() {
        assert_eq!(
            schemas().len(),
            5,
            "one kind on two objects and at most one of the 2^2 * 1 candidate edges: 1 * (1 + 4)"
        );
        assert_digest(
            "schemas",
            schemas(),
            "beb38302a6fc30b91bc6ba44935ce23e7e0cbf93eb1450c8468b2e8443ae9265",
        );
        let alphabet = ['p', 'q', 'r'];
        assert_eq!(
            sequences(&alphabet).len(),
            13,
            "words of length at most two over three letters: 1 + 3 + 9"
        );
        assert_digest(
            "sequences",
            sequences(&alphabet),
            "65408ae82d92a8150600d778a1a8c08bd2793334570928ba81bf93d442215987",
        );
        assert_eq!(
            subsets(&alphabet).len(),
            7,
            "subsets of size at most two of three items: 1 + 3 + 3"
        );
        assert_digest(
            "subsets",
            subsets(&alphabet),
            "b225a9d3519f07a363e1f1488b20facb12500b13adc7510e63f1aedf2fb788ac",
        );
    }

    #[test]
    fn sort_shapes_and_their_operations_match_their_closed_form_counts() {
        let shapes = SortShape::all();
        assert_eq!(
            shapes.len(),
            8,
            "none, three `A`-only shapes, four with `B`"
        );
        assert_digest(
            "sort shapes",
            &shapes,
            "663373c0d7b28e1b36ea867cd37b485bc83c67d11eb37a1bef8f89db5d999f38",
        );
        // `A` only: one expression at every arity, 1 + 1 + 1 operations;
        // simple `B`: two expressions everywhere, 2 + 2 * 2 + 4 * 2; the
        // dependent `B`: 1 + 2 + (3 + 2), the second input's choices adding
        // `B(x)` and `B(y)` as `x` and `y` have sort `A`.
        let operation_counts: Vec<usize> = shapes
            .iter()
            .map(|shape| shape.operations("f").len())
            .collect();
        assert_eq!(operation_counts, [0, 3, 3, 3, 14, 14, 8, 8]);
        for (shape, n) in shapes.iter().zip(&operation_counts) {
            assert_eq!(
                operation_lists(*shape).len(),
                1 + n + n * n,
                "no operation, [f] and [f, g] for {shape:?}"
            );
        }
        assert_digest(
            "operations",
            shapes.iter().map(|shape| shape.operations("g")),
            "59f4ec69588c58e8f11062b601dbfd445722465a91b838569c7e9d8ec6bc9c5d",
        );
        assert_digest(
            "operation lists",
            shapes.iter().map(|shape| operation_lists(*shape)),
            "0e149ae6d4f0b5b5230ced606961fb39682238451b3bec5a31a838c6137adc40",
        );
    }

    #[test]
    fn signatures_match_their_closed_form_counts() {
        let bare = bare_signatures();
        assert_eq!(
            bare.len(),
            608,
            "1 + 3 * (1 + 3 + 3^2) + 2 * (1 + 14 + 14^2) + 2 * (1 + 8 + 8^2)"
        );
        assert_digest(
            "bare signatures",
            &bare,
            "bbda54d42974da95158181d3d7c550056d5879b4f9e8c453adf4369e3e5584f2",
        );
        // A list with c constants has (2 + c)^2 equations. Per shape with n
        // operations of which k are constants, the lists contribute
        // 5 + (10k + 5(n - k)) + (17k^2 + 20k(n - k) + 5(n - k)^2): 5 for no
        // sort, 102 for each `A`-only shape (n = 3, k = 1), 1353 for each
        // simple `B` (n = 14, k = 2) and 452 for each dependent `B` (n = 8,
        // k = 1).
        let all = signatures();
        assert_eq!(all.len(), 5 + 3 * 102 + 2 * 1353 + 2 * 452);
        assert_digest(
            "signatures",
            &all,
            "0424effcc1f186a1ca2605a94d5732e9fa6b0aa961b00d2b3e4268e077ba89ac",
        );
        let terms: Vec<Vec<Term>> = bare
            .iter()
            .map(|theory| shallow_terms(&theory.ops))
            .collect();
        assert_eq!(
            terms.iter().map(Vec::len).sum::<usize>(),
            bare.iter()
                .map(|theory| { 2 + theory.ops.iter().filter(|op| op.inputs.is_empty()).count() })
                .sum::<usize>(),
            "the two variables and one term per constant"
        );
        assert_digest(
            "shallow terms",
            &terms,
            "becd9f03e5d53bbc97a559f7d093d4ccdc026074281f8e83437343e52c807124",
        );
    }

    #[test]
    fn the_raw_family_matches_its_closed_form_counts() {
        assert_eq!(raw_sort_expressions().len(), 7);
        assert_digest(
            "raw sort expressions",
            raw_sort_expressions(),
            "511304d91d1c789a019fb86dce321838d9db5f33ff559cf5772d2fb5b7cbb64c",
        );
        assert_eq!(raw_sort_lists().len(), 7);
        assert_digest(
            "raw sort lists",
            raw_sort_lists(),
            "371f77139a3ca187a827efcb00325258453b20a75ee1e0c65ee67075e8e17f35",
        );
        // Seven outputs per input list; 2 * 7 = 14 choices per input.
        assert_eq!(raw_operations().len(), 7 * (1 + 14 + 14 * 14));
        assert_digest(
            "raw operations",
            raw_operations(),
            "f05900b25f88a872ffe555a71e0adbc5657842161fc9c7fab92214ae7f695e3c",
        );
        assert_eq!(
            raw_signatures().len(),
            7 * (1 + 1477) + 5,
            "each sort list bare and with each raw operation, then the five duplicates"
        );
        assert_digest(
            "raw signatures",
            raw_signatures(),
            "2db971c098107b0b782e8578166a5867fb27e4b274e642da9b44b278f2c9180f",
        );
    }

    /// The bare signatures with at most one operation.
    fn small_signatures() -> Vec<Theory> {
        bare_signatures()
            .into_iter()
            .filter(|theory| theory.ops.len() <= 1)
            .collect()
    }

    #[test]
    fn morphism_families_match_their_closed_form_counts() {
        let small = small_signatures();
        assert_eq!(small.len(), 8 + 3 * 3 + 2 * 14 + 2 * 8);
        let mut images = Vec::new();
        let mut total = Vec::new();
        let mut partial = Vec::new();
        for domain in &small {
            for codomain in &small {
                let constants = codomain
                    .ops
                    .iter()
                    .filter(|op| op.inputs.is_empty())
                    .count();
                let mut expected = codomain.sorts.len().pow(
                    u32::try_from(domain.sorts.len())
                        .unwrap_or_else(|error| panic!("two sorts at most: {error}")),
                );
                for op in &domain.ops {
                    let these = operation_images(op, codomain);
                    assert_eq!(
                        these.len(),
                        codomain.ops.len() + op.inputs.len() + constants,
                        "renames, inputs and constants"
                    );
                    expected *= these.len();
                    images.push(these);
                }
                let morphisms = morphisms(domain, codomain);
                assert_eq!(
                    morphisms.len(),
                    expected,
                    "|sorts|^|sorts| times the images of each operation"
                );
                total.push(morphisms);
                let partials = partial_morphisms(domain, codomain);
                assert_eq!(
                    partials.len(),
                    2 * (domain.sorts.len() + domain.ops.len()),
                    "each symbol left out and sent to an undeclared name"
                );
                partial.push(partials);
            }
        }
        assert_digest(
            "operation images",
            &images,
            "8f4e348ad1fc1cba1c49b3bd82abe8115d4c6b9a3d118341c7cc0c838f803dba",
        );
        assert_digest(
            "morphisms",
            &total,
            "180867d2283e464458c1d32e32c3c156bdf6c5cd6d81569a147616afaef6b1db",
        );
        assert_digest(
            "partial morphisms",
            &partial,
            "20a9cc17e4ded3baf83db065d1cb2a3ffa2345ce5dcf3af2e0f645cdf31b0b28",
        );
    }

    #[test]
    fn options_identifications_and_recipes_match_their_closed_form_counts() {
        assert_eq!(option_sets().len(), 4, "two modes times two clash policies");
        assert_digest(
            "option sets",
            option_sets(),
            "a1910bec13f2debfc60a3ffc7f693e4ba58d352be58cae6acbd88a7e7df2b297",
        );
        for (size, max_len) in [(0, 2), (1, 2), (2, 1), (3, 2)] {
            let pairs: usize = (size + 1) * (size + 1);
            let expected: usize = (0..=max_len)
                .scan(1, |power, _| {
                    let this = *power;
                    *power *= pairs;
                    Some(this)
                })
                .sum();
            assert_eq!(
                identifications(size, max_len).len(),
                expected,
                "words of length at most {max_len} over the {pairs} pairs of 0..={size}"
            );
        }
        assert_digest(
            "identifications",
            [identifications(1, 2), identifications(3, 2)],
            "39a462cefcf985daf982fa384c947664f0198aa08fd20cf185475702283891ff",
        );
        let small = small_signatures();
        let mut all = Vec::new();
        for left in &small {
            for right in &small {
                let names = |theory: &Theory, sorts: bool| -> BTreeSet<String> {
                    if sorts {
                        theory.sorts.iter().map(|sort| sort.name.clone()).collect()
                    } else {
                        theory.ops.iter().map(|op| op.name.clone()).collect()
                    }
                };
                let common = names(left, true).intersection(&names(right, true)).count()
                    + names(left, false)
                        .intersection(&names(right, false))
                        .count();
                let these = recipes(left, right);
                assert_eq!(
                    these.len(),
                    13 + (2 << common),
                    "three bases, 2 * (2^common + 1) colimits, eight diagram steps"
                );
                all.push(these);
            }
        }
        assert_digest(
            "recipes",
            &all,
            "af8ed5c411f51701834d0ff4ac8c26d637d977fc1812d39af9f9d87f503a81f3",
        );
    }

    #[test]
    fn the_action_families_have_their_stated_sizes() {
        let whole = whole_action_scope();
        assert_eq!(
            whole.domains.len(),
            6,
            "two `p` arguments times three `r` outputs"
        );
        assert_eq!(
            whole.codomains.len(),
            6,
            "three closures times two `Z` arguments"
        );
        assert_eq!(whole.legs.len(), 4 * 3 * 5 * 3 * 2 * 5 * 4);
        assert_eq!(whole.probes.len(), 3);
        assert_digest(
            "whole action domains",
            &whole.domains,
            "a26a3be7b5498cd3f0ed5b10dcef6033de951d69bf2d1aed0d97694171d807fa",
        );
        assert_digest(
            "whole action codomains",
            &whole.codomains,
            "495080fd94457b0966beae4591848e7d9723845a73eeda8cd58fc058f27ee669",
        );
        assert_digest(
            "whole action legs",
            &whole.legs,
            "dfc02aa6b87f78a5ed6430cc6e1354cc0d2bef0290f8d8f573f9bd2628c4f784",
        );
        assert_digest(
            "whole action probes",
            &whole.probes,
            "feb7c1b9f844a5637f3fd79e60a27bd9b7489dec5f40160883a231f6834c4a28",
        );
        let constructors = constructor_scope();
        assert_eq!(constructors.domains.len(), 3);
        assert_eq!(constructors.codomains.len(), 4);
        assert_eq!(
            constructors.legs.len(),
            2 + 3 * (1 + 3) + 2,
            "two renames, k1 and k over the three leaves, two ill-formed images"
        );
        assert_eq!(constructors.probes.len(), 3);
        assert_digest(
            "constructor scope",
            [constructors],
            "ae2ca591c8ae8881c1a5c7c589675aab5c185ba616ede73b9d7fbe7058df0b82",
        );
    }

    #[test]
    fn fibre_spans_are_pinned() {
        assert_eq!(fibre_spans().len(), 4, "two image choices per leg");
        assert_digest(
            "fibre spans",
            fibre_spans(),
            "dccfa53a20e8742b67a28dd51a5a56bab7c77d7801cf4ae0f24db9d516adec17",
        );
        assert_digest(
            "a renamed fibre span",
            [fibre_span("k", ["u", "v", "w"], [false, true])],
            "f57fa22e342ab62ea53e8ed8bd9e06b54f3d9e4e42ff147a7ec0b906a05d1dde",
        );
    }

    fn operation_prefixes() -> Vec<Vec<(ClassSortForm, Implicit)>> {
        let mut out = vec![Vec::new()];
        let mut frontier: Vec<Vec<(ClassSortForm, Implicit)>> = vec![Vec::new()];
        for _ in 0..2 {
            let mut next = Vec::new();
            for prefix in &frontier {
                let sorts: Vec<ClassSortForm> =
                    prefix.iter().map(|(sort, _)| sort.clone()).collect();
                for sort in class_sort_forms(&class_bound_positions(&sorts), false) {
                    for implicit in [Implicit::No, Implicit::Yes] {
                        let mut grown = prefix.clone();
                        grown.push((sort.clone(), implicit));
                        next.push(grown);
                    }
                }
            }
            out.extend(next.iter().cloned());
            frontier = next;
        }
        out
    }

    fn sort_prefixes() -> Vec<Vec<ClassSortForm>> {
        let mut out = vec![Vec::new()];
        let mut frontier: Vec<Vec<ClassSortForm>> = vec![Vec::new()];
        for _ in 0..2 {
            let mut next = Vec::new();
            for prefix in &frontier {
                for sort in class_sort_forms(&class_bound_positions(prefix), false) {
                    let mut grown = prefix.clone();
                    grown.push(sort);
                    next.push(grown);
                }
            }
            out.extend(next.iter().cloned());
            frontier = next;
        }
        out
    }

    fn x_profile<'a>(sorts: impl Iterator<Item = &'a ClassSortForm>) -> (usize, bool) {
        let xs: Vec<&ClassSortForm> = sorts.filter(|sort| sort.head == "X").collect();
        let bare = xs.iter().any(|sort| sort.args.is_none());
        (xs.len(), bare)
    }

    /// The distinct operations rendered, under both `X` spellings, from
    /// every operation form extending `prefix`.
    fn rendered_operations(prefix: &[(ClassSortForm, Implicit)]) -> usize {
        let mut forms = Vec::new();
        extend_class_operations(&mut prefix.to_vec(), &mut forms);
        let mut rendered = BTreeSet::new();
        for form in &forms {
            for names in class_names(form.inputs.len()) {
                for empty_app in [false, true] {
                    rendered.insert(format!(
                        "{:?}",
                        render_class_operation(form, &names, empty_app)
                    ));
                }
            }
        }
        rendered.len()
    }

    /// The distinct sort declarations rendered from every parameter list
    /// extending `prefix`.
    fn rendered_sorts(prefix: &[ClassSortForm]) -> usize {
        let mut forms = Vec::new();
        extend_class_sorts(&mut prefix.to_vec(), &mut forms);
        let mut rendered = BTreeSet::new();
        for params in &forms {
            for names in class_names(params.len()) {
                for empty_app in [false, true] {
                    for value_kind in [false, true] {
                        for closed in [false, true] {
                            rendered.insert(format!(
                                "{:?}",
                                render_class_sort_declaration(
                                    params, &names, empty_app, value_kind, closed
                                )
                            ));
                        }
                    }
                }
            }
        }
        rendered.len()
    }

    #[test]
    fn class_recurrences_agree_with_the_materialized_prefix_scopes() {
        let operation_prefixes = operation_prefixes();
        assert_eq!(
            operation_prefixes.len(),
            1 + 8 + (2 * 5 * 2 + 6 * 4 * 2),
            "the empty prefix, four forms by two flags, and their prefix-valid extensions"
        );
        for prefix in &operation_prefixes {
            let (x_bound, has_bare_x) = x_profile(prefix.iter().map(|(sort, _)| sort));
            assert_eq!(
                class_operation_count(prefix.len(), x_bound, has_bare_x),
                rendered_operations(prefix),
                "operations extending {prefix:?}"
            );
        }
        let sort_prefixes = sort_prefixes();
        assert_eq!(sort_prefixes.len(), 1 + 4 + (5 + 3 * 4));
        for prefix in &sort_prefixes {
            let (x_bound, has_bare_x) = x_profile(prefix.iter());
            assert_eq!(
                class_sort_count(prefix.len(), x_bound, has_bare_x),
                rendered_sorts(prefix),
                "sort declarations extending {prefix:?}"
            );
        }
    }

    /// Output forms over `x` bound `X` inputs: `X`, `Y`, `Y` of each of the
    /// `2 + x` atoms and `H` of each pair of atoms.
    const fn class_outputs(x: usize) -> usize {
        2 + (2 + x) + (2 + x) * (2 + x)
    }

    /// Renderings per output form list: both spellings of every output when
    /// an input already spells `X`, else one more for the bare `X` output.
    const fn class_spellings(x: usize, bare: bool) -> usize {
        if bare {
            2 * class_outputs(x)
        } else {
            class_outputs(x) + 1
        }
    }

    /// The operation count unrolled over the three depths: a depth-two
    /// prefix has two namings; a shallower one adds two implicit flags times
    /// the `X` input and the `3 + x` other input forms.
    fn operation_closed_form(depth: usize, x: usize, bare: bool) -> usize {
        let two = |x: usize, bare: bool| 2 * class_spellings(x, bare);
        let one = |x: usize, bare: bool| {
            2 * class_spellings(x, bare) + 2 * two(x + 1, true) + 2 * (3 + x) * two(x, bare)
        };
        match depth {
            0 => class_spellings(x, bare) + 2 * one(x + 1, true) + 2 * (3 + x) * one(x, bare),
            1 => one(x, bare),
            _ => two(x, bare),
        }
    }

    /// The sort count unrolled likewise: four kind and closure choices,
    /// doubled for a bare `X` parameter, times the namings.
    fn sort_closed_form(depth: usize, x: usize, bare: bool) -> usize {
        let spellings = |bare: bool| if bare { 2 } else { 1 };
        let two = |bare: bool| 8 * spellings(bare);
        let one = |x: usize, bare: bool| 8 * spellings(bare) + two(true) + (3 + x) * two(bare);
        match depth {
            0 => 4 * spellings(bare) + one(x + 1, true) + (3 + x) * one(x, bare),
            1 => one(x, bare),
            _ => two(bare),
        }
    }

    #[test]
    fn class_recurrences_match_their_closed_forms() {
        for depth in 0..=2 {
            for x in 0..=3 {
                for bare in [false, true] {
                    assert_eq!(
                        class_operation_count(depth, x, bare),
                        operation_closed_form(depth, x, bare),
                        "operations at depth {depth}, {x} bound, bare {bare}"
                    );
                    assert_eq!(
                        class_sort_count(depth, x, bare),
                        sort_closed_form(depth, x, bare),
                        "sorts at depth {depth}, {x} bound, bare {bare}"
                    );
                }
            }
        }
        assert_eq!(class_operation_count(2, 0, false), 2 * (8 + 1));
        assert_eq!(class_operation_count(2, 1, true), 2 * 2 * 14);
        assert_eq!(class_operation_count(2, 2, true), 2 * 2 * 22);
        assert_eq!(class_sort_count(2, 0, false), 8);
        assert_eq!(class_sort_count(2, 1, true), 16);
    }

    #[test]
    fn class_scopes_match_their_closed_form_counts_and_are_syntactically_unique() {
        let operations = class_operation_scope();
        assert_eq!(
            operations.len(),
            (8 + 1) + (2 * 2 * 2 * 14 + 3 * 2 * 2 * 9) + (352 + 896 + 672 + 648) + 18,
            "arity 0: 8 outputs and the second X spelling; arity 1: an X input \
             (2 flags, 2 names, both spellings of 14 outputs) and three others \
             (2 flags, 2 names, 9); arity 2: X then X (2 * 2 * 2 * 2 * 22), X then \
             one of four others (2 * 4 * 2 * 2 * 2 * 14), one of three then X \
             (3 * 2 * 2 * 2 * 2 * 14), one of three then one of three \
             (3 * 2 * 3 * 2 * 2 * 9); then the eighteen scoped signatures"
        );
        assert_digest(
            "class operation scope",
            &operations,
            "71fbc8dbec52266a56fda703fca8abc357cdfab8ecabdcdc2a507e3e7ef1b8b4",
        );
        let operation_syntax: BTreeSet<String> =
            operations.iter().map(|op| format!("{op:?}")).collect();
        assert_eq!(
            operation_syntax.len(),
            operations.len(),
            "no two operations share a rendering"
        );
        let sorts = class_sort_scope();
        assert_eq!(
            sorts.len(),
            4 + (2 * 4 * 2 + 3 * 2 * 4) + (5 * 2 * 4 * 2 + 3 * (2 * 4 * 2 + 3 * 2 * 4)) + 16,
            "four kind and closure choices per rendering: no parameter; an X \
             parameter (two names, both spellings) or three others; X then any \
             of five, or one of three then X or one of three; then the sixteen \
             scoped declarations"
        );
        assert_digest(
            "class sort scope",
            &sorts,
            "6bffcc1739aafbffe69d14780f6135ba048095fd88b35b5d2d68afbfff098217",
        );
        let sort_syntax: BTreeSet<String> = sorts.iter().map(|sort| format!("{sort:?}")).collect();
        assert_eq!(
            sort_syntax.len(),
            sorts.len(),
            "no two sorts share a rendering"
        );
        assert_digest(
            "capture witnesses",
            [format!(
                "{:?}{:?}",
                class_operation_capture_witnesses(),
                class_sort_capture_witnesses()
            )],
            "8840ef67155ddc9dadc5a6c71b40d29a08f0ad6626b2f94f93ed8eb2e4362a9e",
        );
    }

    fn form(head: &'static str, args: Option<Vec<ClassTermForm>>) -> ClassSortForm {
        ClassSortForm { head, args }
    }

    #[test]
    fn class_form_helpers_are_pinned() {
        let x = || form("X", None);
        let y = || form("Y", None);
        assert_eq!(class_bound_positions(&[x(), y(), x()]), [0, 2]);
        assert_eq!(class_bound_positions(&[y(), x()]), [1]);
        assert_eq!(class_bound_positions(&[y()]), Vec::<usize>::new());
        assert!(class_sort_form_has_bare_x(&x()));
        assert!(!class_sort_form_has_bare_x(&form("X", Some(Vec::new()))));
        assert!(!class_sort_form_has_bare_x(&y()));
        let op = |inputs: Vec<ClassSortForm>, output| ClassOpForm {
            inputs: inputs
                .into_iter()
                .map(|sort| (sort, Implicit::No))
                .collect(),
            output,
        };
        assert!(class_operation_has_bare_x(&op(vec![y()], x())));
        assert!(class_operation_has_bare_x(&op(vec![x()], y())));
        assert!(!class_operation_has_bare_x(&op(vec![y()], y())));
        assert_eq!(class_names(0), [Vec::<&str>::new()]);
        assert_eq!(class_names(1), [["x"], ["y"]]);
        assert_eq!(class_names(2), [["x", "y"], ["y", "x"]]);
        assert_eq!(
            format!("{:?}", render_class_sort(&x(), &[], true)),
            "App(\"X\", [])"
        );
        assert_eq!(
            format!("{:?}", render_class_sort(&x(), &[], false)),
            "Name(\"X\")"
        );
        assert_eq!(
            format!("{:?}", render_class_sort(&y(), &[], true)),
            "Name(\"Y\")"
        );
        assert_eq!(
            format!(
                "{:?}",
                render_class_sort(
                    &form("Y", Some(vec![ClassTermForm::Bound(0)])),
                    &["u"],
                    true
                )
            ),
            "App(\"Y\", [Var(\"u\")])"
        );
        assert_eq!(class_sort_forms(&[], false).len(), 4);
        assert_eq!(class_sort_forms(&[0], true).len(), 2 + 3 + 3 * 3);
    }

    #[test]
    fn connector_diagrams_are_pinned() {
        let operations = class_operation_scope();
        let sorts = class_sort_scope();
        let operation_pairs: Vec<TheoryDiagram> = operations
            .iter()
            .step_by(151)
            .flat_map(|left| {
                operations
                    .iter()
                    .step_by(157)
                    .map(move |right| operation_class_pair(left, right))
            })
            .collect();
        assert_eq!(
            operation_pairs.len(),
            19 * 18,
            "every 151st against every 157th of the 2815 operations"
        );
        assert_digest(
            "operation class pairs",
            &operation_pairs,
            "38771f3c80bcffabf9d2eedba9af7f0293a83adcd0b056dbd14bc975c34ae411",
        );
        let sort_pairs: Vec<TheoryDiagram> = sorts
            .iter()
            .step_by(7)
            .flat_map(|left| {
                sorts
                    .iter()
                    .step_by(11)
                    .map(move |right| sort_class_pair(left, right))
            })
            .collect();
        assert_eq!(
            sort_pairs.len(),
            38 * 24,
            "every 7th against every 11th of the 260 sorts"
        );
        assert_digest(
            "sort class pairs",
            &sort_pairs,
            "6472b63af000fa17d6836dd653e4488fa27363da75fef9717f5863aaf4bff429",
        );
        assert_eq!(
            sort_mismatch_spans().len(),
            4,
            "arity, parameter, kind, closure"
        );
        assert_digest(
            "sort mismatch spans",
            sort_mismatch_spans(),
            "a04e8523218469755ec4a5afa8b9c17a00527f225c66e5c8a1dc9bde04ca1b61",
        );
    }
}
