//! Composition of theories along a labeled diagram.
//!
//! A [`TheoryDiagram`] names its theories by node label and relates them
//! by theory morphisms. [`compose`] computes the diagram's colimit: every
//! node's sorts and operations form one disjoint union per kind, the
//! least equivalence the morphisms generate identifies them, and each
//! class becomes one symbol of the result, which carries every node's
//! equations translated along the cocone. Sharing is decided by which
//! symbols the morphisms relate, never by spelling, unless the caller asks
//! for [`CompositionMode::Amalgamate`], which additionally identifies
//! same-name symbols of the same kind.
//!
//! The construction follows the colimit of theories of Goguen and Burstall
//! (goguenburstall1992institutions, Thm 11, proved: the colimit is built on
//! signatures and carries the translated sentences of every component).
//! Carrying it to dependent sort contexts is ours (inferred): every class
//! must agree on its members' signatures modulo the identification before
//! anything is built, and the result must pass the signature typechecker.
//!
//! A composition returns a [`Composition`]: the theory, one inclusion
//! morphism per node, a [`Provenance`] that sends every source symbol,
//! equation, directed equation and policy to its image, and a
//! [`CompositionReport`] that records every check that ran.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// The kind of a symbol of a theory.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymKind {
    /// A sort.
    Sort,
    /// An operation.
    Op,
    /// An equation.
    Eq,
    /// A directed equation.
    DirectedEq,
    /// A conflict policy.
    Policy,
}

impl SymKind {
    /// The `snake_case` name of this kind, as it serializes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sort => "sort",
            Self::Op => "op",
            Self::Eq => "eq",
            Self::DirectedEq => "directed_eq",
            Self::Policy => "policy",
        }
    }
}

impl std::fmt::Display for SymKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A symbol of one node of a diagram: the node's label and the symbol's
/// name in that node's theory.
///
/// The derived order compares the label first, then the name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymRef {
    /// The label of the node that declares (or uses) the symbol.
    pub node: Arc<str>,
    /// The symbol's name in that node's theory.
    pub name: Arc<str>,
}

impl SymRef {
    /// Build a symbol reference.
    #[must_use]
    pub fn new(node: impl Into<Arc<str>>, name: impl Into<Arc<str>>) -> Self {
        Self {
            node: node.into(),
            name: name.into(),
        }
    }
}

impl std::fmt::Display for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.node, self.name)
    }
}

/// How the members of one sort class disagree.
///
/// Serialized externally tagged in `snake_case`; the unit variants are
/// bare strings (`"kind"`, `"closure"`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMismatch {
    /// The two sorts have different numbers of parameters.
    Arity {
        /// The first member's arity.
        first: usize,
        /// The second member's arity.
        second: usize,
    },
    /// Parameter `index` ranges over different sort classes.
    ParamSort {
        /// The parameter's position.
        index: usize,
        /// The first member's parameter sort, as declared.
        first: Arc<str>,
        /// The second member's parameter sort, as declared.
        second: Arc<str>,
    },
    /// The two sorts have different [`crate::SortKind`]s.
    Kind,
    /// The two sorts have different closures, after their constructors are
    /// sent to their classes.
    Closure,
}

impl std::fmt::Display for SortMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arity { first, second } => write!(f, "sort arities differ ({first} vs {second})"),
            Self::ParamSort {
                index,
                first,
                second,
            } => write!(
                f,
                "parameter {index} sorts differ ({first} vs {second})"
            ),
            Self::Kind => f.write_str("sort kinds differ"),
            Self::Closure => f.write_str("sort closures differ"),
        }
    }
}

/// Why an identification the diagram asks for cannot be expressed as a
/// theory.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotRepresentableReason {
    /// Two identified operations differ only in the term arguments of a
    /// dependent input or output sort (for instance `Hom(x, y)` against
    /// `Hom(y, x)`); identifying them would need an equation between
    /// sorts, which a [`crate::Theory`] cannot state.
    NeedsSortEquation,
}

/// How two members of one operation class disagree, once every sort and
/// operation they mention is sent to its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpMismatch {
    /// Only the term arguments of dependent sorts differ.
    NeedsSortEquation,
    /// The signatures differ in some other way.
    Signature,
}

impl std::fmt::Display for OpMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsSortEquation => f.write_str(
                "operation signatures differ only in dependent sort arguments, \
                 which would need a sort equation",
            ),
            Self::Signature => f.write_str("operation signatures differ in the quotient"),
        }
    }
}

/// Send a sort's parameter sorts and closure constructors through the
/// given class maps. The name is left as it is.
pub(crate) fn translate_sort(
    sort: &crate::sort::Sort,
    sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
    op_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
) -> crate::sort::Sort {
    crate::sort::Sort {
        name: Arc::clone(&sort.name),
        params: sort
            .params
            .iter()
            .map(|p| crate::sort::SortParam {
                name: Arc::clone(&p.name),
                sort: p.sort.apply_maps(sort_map, op_map),
            })
            .collect(),
        kind: sort.kind.clone(),
        closure: match &sort.closure {
            crate::sort::SortClosure::Open => crate::sort::SortClosure::Open,
            crate::sort::SortClosure::Closed(ctors) => {
                let mut mapped: Vec<Arc<str>> = ctors
                    .iter()
                    .map(|c| op_map.get(c).cloned().unwrap_or_else(|| Arc::clone(c)))
                    .collect();
                mapped.sort();
                mapped.dedup();
                crate::sort::SortClosure::Closed(mapped)
            }
        },
    }
}

/// Send an operation's input and output sorts through the given class
/// maps. The name is left as it is.
pub(crate) fn translate_op(
    op: &crate::op::Operation,
    sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
    op_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
) -> crate::op::Operation {
    crate::op::Operation::with_implicit(
        Arc::clone(&op.name),
        op.inputs
            .iter()
            .map(|(n, s, imp)| (Arc::clone(n), s.apply_maps(sort_map, op_map), *imp))
            .collect(),
        op.output.apply_maps(sort_map, op_map),
    )
}

/// Compare two members of one sort class, both already translated into
/// class names. `first_decl` and `second_decl` are the members as
/// declared, used to render a parameter-sort mismatch.
pub(crate) fn sort_class_mismatch(
    first: &crate::sort::Sort,
    second: &crate::sort::Sort,
    first_decl: &crate::sort::Sort,
    second_decl: &crate::sort::Sort,
) -> Option<SortMismatch> {
    if first.arity() != second.arity() {
        return Some(SortMismatch::Arity {
            first: first.arity(),
            second: second.arity(),
        });
    }
    let rename = crate::sort::positional_param_rename(
        first.params.iter().map(|p| Arc::clone(&p.name)),
        second.params.iter().map(|p| Arc::clone(&p.name)),
    );
    for (index, (a, b)) in first.params.iter().zip(&second.params).enumerate() {
        if !a.sort.subst(&rename).alpha_eq(&b.sort) {
            return Some(SortMismatch::ParamSort {
                index,
                first: Arc::from(first_decl.params[index].sort.to_string()),
                second: Arc::from(second_decl.params[index].sort.to_string()),
            });
        }
    }
    if first.kind != second.kind {
        return Some(SortMismatch::Kind);
    }
    if first.closure != second.closure {
        return Some(SortMismatch::Closure);
    }
    None
}

/// Compare two members of one operation class, both already translated
/// into class names.
pub(crate) fn op_class_mismatch(
    first: &crate::op::Operation,
    second: &crate::op::Operation,
) -> Option<OpMismatch> {
    if crate::sort::signatures_equivalent_modulo_param_rename(
        &first.inputs,
        &first.output,
        &second.inputs,
        &second.output,
    ) {
        return None;
    }
    let same_shape = first.inputs.len() == second.inputs.len()
        && first
            .inputs
            .iter()
            .zip(&second.inputs)
            .all(|((_, a, ia), (_, b, ib))| {
                ia == ib && a.head() == b.head() && a.args().len() == b.args().len()
            })
        && first.output.head() == second.output.head()
        && first.output.args().len() == second.output.args().len();
    if same_shape {
        Some(OpMismatch::NeedsSortEquation)
    } else {
        Some(OpMismatch::Signature)
    }
}
