#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::elidable_lifetime_names,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::single_char_pattern,
    clippy::naive_bytecount,
    clippy::expect_used,
    clippy::redundant_pub_crate,
    clippy::used_underscore_binding,
    clippy::redundant_field_names,
    clippy::struct_field_names,
    clippy::redundant_else,
    clippy::similar_names
)]

//! `emit_pretty::cursor` (Phase A decomposition).

use super::Edge;

/// Linear cursor over a vertex's outgoing edges, used to thread
/// children through a production rule without double-consuming them.
pub(crate) struct ChildCursor<'a> {
    pub(crate) edges: &'a [&'a Edge],
    pub(crate) consumed: Vec<bool>,
}

impl<'a> ChildCursor<'a> {
    pub(crate) fn new(edges: &'a [&'a Edge]) -> Self {
        Self {
            edges,
            consumed: vec![false; edges.len()],
        }
    }

    /// Take the next unconsumed edge whose kind equals `field_name`.
    pub(crate) fn take_field(&mut self, field_name: &str) -> Option<&'a Edge> {
        for (i, edge) in self.edges.iter().enumerate() {
            if !self.consumed[i] && edge.kind.as_ref() == field_name {
                self.consumed[i] = true;
                return Some(edge);
            }
        }
        None
    }

    /// Whether any unconsumed edge carries the given field name.
    /// A read-only peek (does not consume): lets the SYMBOL handler
    /// decide whether to expand a hidden rule inline so a nested FIELD
    /// can claim a re-labelled edge.
    pub(crate) fn has_field(&self, field_name: &str) -> bool {
        self.peek_field(field_name).is_some()
    }

    /// The next unconsumed edge carrying the given field name, without
    /// consuming it. Companion to `has_field`: the nested-FIELD re-label
    /// check inspects the edge's target kind against node-types.
    pub(crate) fn peek_field(&self, field_name: &str) -> Option<&'a Edge> {
        self.edges
            .iter()
            .enumerate()
            .find(|(i, edge)| !self.consumed[*i] && edge.kind.as_ref() == field_name)
            .map(|(_, edge)| *edge)
    }

    /// Whether any unconsumed edge satisfies `predicate`. Used by the
    /// unit tests; the live emit path went through `has_matching` on
    /// each alternative until cursor-driven dispatch was rewritten to
    /// pick the first-unconsumed-edge's kind directly.
    #[cfg(test)]
    pub(crate) fn has_matching(&self, predicate: impl Fn(&Edge) -> bool) -> bool {
        self.edges
            .iter()
            .enumerate()
            .any(|(i, edge)| !self.consumed[i] && predicate(edge))
    }

    /// Take the next unconsumed edge whose target vertex satisfies
    /// `predicate`. Returns the edge and the underlying production
    /// resolution path is the caller's job.
    pub(crate) fn take_matching(&mut self, predicate: impl Fn(&Edge) -> bool) -> Option<&'a Edge> {
        for (i, edge) in self.edges.iter().enumerate() {
            if !self.consumed[i] && predicate(edge) {
                self.consumed[i] = true;
                return Some(edge);
            }
        }
        None
    }
}

thread_local! {
    pub(crate) static EMIT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Set of `(vertex_id, rule_name)` pairs that are currently being
    /// walked by the recursion. A SYMBOL that resolves to a rule
    /// already on this stack closes a μ-binder cycle: in the
    /// coinductive reading, the rule walk at any vertex is the least
    /// fixed point of `body[μ X . body / X]`, which unfolds at most
    /// once, with the second visit returning the empty sequence (the
    /// unit of the free token monoid). Examples that trigger this:
    /// YAML's `stream` ⊃ `_b_blk_*` mutually-recursive chain, Rust's
    /// `_expression` ⊃ `binary_expression` ⊃ `_expression`.
    /// The frame key carries the cursor's CONSUMED-edge count as a third
    /// component. A hidden rule that recurses to consume successive children
    /// of the SAME vertex (a right-recursive list tree-sitter flattens onto
    /// one vertex: http's `_section_content = SEQ[item, CHOICE[
    /// _section_content | BLANK]]`) re-enters this rule at the same vertex but
    /// with a strictly larger consumed count, because each level consumed one
    /// item before recursing. Keying on the count lets that PROGRESS-bearing
    /// re-entry through (each item is emitted) while a genuine zero-progress
    /// cycle (`_expression ⊃ binary_expression ⊃ _expression` re-entered with
    /// no item consumed) keeps the same key and still collapses to ε.
    /// Recursion stays bounded by the edge count: the consumed count strictly
    /// increases on every admitted re-entry.
    pub(crate) static EMIT_MU_FRAMES: std::cell::RefCell<std::collections::HashSet<(String, String, usize)>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// The name of the FIELD whose body the walker is currently inside,
    /// or `None` at top level. Lets a SYMBOL nested arbitrarily deep
    /// in the field's content (under SEQ, CHOICE, REPEAT, OPTIONAL)
    /// consume from the *outer* cursor by edge-kind rather than from
    /// the child's own cursor by symbol-match. Without this, shapes
    /// like `field('args', commaSep1($.X))` — which expands to
    /// `FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X))))` — emit only
    /// the first matched edge: the FIELD handler consumed one edge,
    /// the inner REPEAT searched the consumed child's cursor (which
    /// has no more sibling field edges), and the REPEAT broke after
    /// one iteration. Setting the context here so the inner SYMBOL
    /// pulls successive field-named edges from the outer cursor
    /// recovers every matched edge across arbitrary nesting.
    pub(crate) static EMIT_FIELD_CONTEXT: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    /// Per-vertex `ptrace`-literal budget: for each anonymous literal the
    /// parser recorded on the current vertex (`ptrace-<slot> = T<lit>`), how
    /// many emissions of that literal remain admissible on this vertex's
    /// role-table (by-construction) walk. The parser records EVERY anonymous
    /// token of a node as a `ptrace` slot, so the count is the literal's true
    /// source multiplicity; a `field:<name>` recording of a field-bound token
    /// is a redundant copy of the same `ptrace` slot. A single recorded
    /// separator (bash `case_item`'s one `;;`) must therefore satisfy at most
    /// one of the several separator CHOICEs its walk visits — the inner
    /// `_terminator` inside `_statements`' `REPEAT(SEQ[_statement,
    /// _terminator])`, `_statements`' trailing `CHOICE[_terminator|BLANK]`, and
    /// the `case_item`'s own terminator FIELD — rather than being matched at
    /// each. This mirrors the `sep_budget` cap the REPEAT walker applies to a
    /// trailing mandatory separator, extended across the whole vertex walk to
    /// the variant-tag literal tie-break in [`super::select_choice_with_trace`].
    pub(crate) static EMIT_PTRACE_BUDGET: std::cell::RefCell<std::collections::HashMap<String, usize>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// The vertex whose `ptrace` budget is currently installed. A nested walk
    /// of the SAME vertex (a hidden rule inlined onto the same vertex, e.g.
    /// bash `_statements`) must keep the decrements accumulated across its
    /// CHOICE sites, so re-entry for the same owner does not reinstall the
    /// budget; a walk that descends into a CHILD vertex installs (and later
    /// restores) that child's own budget.
    pub(crate) static EMIT_PTRACE_BUDGET_OWNER: std::cell::RefCell<Option<panproto_gat::Name>> =
        const { std::cell::RefCell::new(None) };
}

/// RAII guard that restores the prior [`EMIT_PTRACE_BUDGET`] and
/// [`EMIT_PTRACE_BUDGET_OWNER`] when the current vertex's walk unwinds.
pub(crate) struct PtraceBudgetGuard {
    prev_budget: std::collections::HashMap<String, usize>,
    prev_owner: Option<panproto_gat::Name>,
}

impl Drop for PtraceBudgetGuard {
    fn drop(&mut self) {
        EMIT_PTRACE_BUDGET.with(|b| *b.borrow_mut() = std::mem::take(&mut self.prev_budget));
        EMIT_PTRACE_BUDGET_OWNER.with(|o| *o.borrow_mut() = self.prev_owner.take());
    }
}

/// Install `budget` as the active per-vertex `ptrace` budget owned by
/// `owner`, returning a guard that restores the previous budget on drop.
/// Returns `None` when `owner` already owns the active budget — a nested
/// walk of the same vertex must accumulate decrements across its CHOICE
/// sites rather than reset them.
pub(crate) fn enter_ptrace_budget(
    owner: &panproto_gat::Name,
    budget: std::collections::HashMap<String, usize>,
) -> Option<PtraceBudgetGuard> {
    if is_ptrace_budget_owner(owner) {
        return None;
    }
    let prev_budget = EMIT_PTRACE_BUDGET.with(|b| std::mem::replace(&mut *b.borrow_mut(), budget));
    let prev_owner = EMIT_PTRACE_BUDGET_OWNER.with(|o| o.borrow_mut().replace(owner.clone()));
    Some(PtraceBudgetGuard {
        prev_budget,
        prev_owner,
    })
}

/// True iff `vertex_id` already owns the active `ptrace` budget.
pub(crate) fn is_ptrace_budget_owner(vertex_id: &panproto_gat::Name) -> bool {
    EMIT_PTRACE_BUDGET_OWNER
        .with(|o| {
            o.borrow()
                .as_ref()
                .map(|n| n.as_str() == vertex_id.as_str())
        })
        .unwrap_or(false)
}

/// True iff the active budget still admits an emission of `literal` (no
/// recorded budget for it ⇒ unbounded, the canonical default for literals
/// the parser never traced).
pub(crate) fn ptrace_budget_allows(literal: &str) -> bool {
    EMIT_PTRACE_BUDGET.with(|b| b.borrow().get(literal).is_none_or(|&rem| rem > 0))
}

/// Spend one unit of the active budget for `literal`, if any remains.
/// A no-op when `literal` carries no recorded budget.
pub(crate) fn ptrace_budget_consume(literal: &str) {
    EMIT_PTRACE_BUDGET.with(|b| {
        if let Some(rem) = b.borrow_mut().get_mut(literal) {
            *rem = rem.saturating_sub(1);
        }
    });
}

/// Clone the active budget for an `OutputSnapshot`. Restored by
/// [`restore_ptrace_budget`] so a REPEAT/OPTIONAL iteration that rolls back
/// its emitted tokens also rolls back any budget it spent.
pub(crate) fn snapshot_ptrace_budget() -> std::collections::HashMap<String, usize> {
    EMIT_PTRACE_BUDGET.with(|b| b.borrow().clone())
}

/// Restore a budget captured by [`snapshot_ptrace_budget`].
pub(crate) fn restore_ptrace_budget(snap: std::collections::HashMap<String, usize>) {
    EMIT_PTRACE_BUDGET.with(|b| *b.borrow_mut() = snap);
}

/// RAII guard that restores the prior `EMIT_FIELD_CONTEXT` value on drop.
pub(crate) struct FieldContextGuard(Option<String>);

impl Drop for FieldContextGuard {
    fn drop(&mut self) {
        EMIT_FIELD_CONTEXT.with(|f| *f.borrow_mut() = self.0.take());
    }
}

pub(crate) fn push_field_context(name: &str) -> FieldContextGuard {
    let prev = EMIT_FIELD_CONTEXT.with(|f| f.borrow_mut().replace(name.to_owned()));
    FieldContextGuard(prev)
}

/// Clear the field context for the duration of a child-context walk.
/// The child's own production has its own FIELDs that set their own
/// context; the outer field hint must not leak into them.
pub(crate) fn clear_field_context() -> FieldContextGuard {
    let prev = EMIT_FIELD_CONTEXT.with(|f| f.borrow_mut().take());
    FieldContextGuard(prev)
}

pub(crate) fn current_field_context() -> Option<String> {
    EMIT_FIELD_CONTEXT.with(|f| f.borrow().clone())
}
