//! Scope-aware vertex ID generation for full-AST schemas.
//!
//! Generates stable, human-readable vertex IDs that encode the scope path
//! from file to the specific AST node. IDs follow the pattern:
//!
//! ```text
//! src/main.rs::parse_input::$3::$0.left
//! ```
//!
//! where `src/main.rs` is the file, `parse_input` is the function,
//! `$3` is the 4th statement in the function body, and `$0.left` is
//! the left operand of the first expression.
//!
//! Statement indices are positional within blocks (shift on insertion).
//! This is correct: the merge algorithm handles reindexing via ThOrder
//! pushouts (`has_order: true`).
//!
//! # Name disambiguation
//!
//! Within a single scope frame, the same name can appear multiple
//! times. The clearest case is Python's `@typing.overload`:
//!
//! ```python
//! @overload
//! def foo(x: int) -> int: ...
//! @overload
//! def foo(x: str) -> str: ...
//! def foo(x): return x
//! ```
//!
//! The same shape arises for C++ overloads, Rust trait methods of the
//! same name in different `impl` blocks, Erlang clauses, Lean / Idris
//! dependent overloads, and any tree-sitter grammar whose `tags.scm`
//! tags two siblings of the same kind with the same identifier. The
//! generator deduplicates by suffixing repeat occurrences with `#N`:
//!
//! * first occurrence: `…::foo`
//! * second:           `…::foo#1`
//! * third:            `…::foo#2`
//!
//! The disambiguated leaf is recorded on the parent frame's `seen`
//! table so that a subsequent [`IdGenerator::push_named_scope`] for
//! the same name returns and persists the matching disambiguated
//! string. Descendants of the second `foo` are prefixed `…::foo#1::…`,
//! not `…::foo::…`, so two collisions never merge into one schema
//! vertex transitively.
//!
//! Field IDs (the `.field` suffix used for expression-tree children)
//! share the same disambiguation: a second [`IdGenerator::field_id`]
//! call at the same base + field name yields `base.field#1`.

use std::collections::HashMap;

/// One scope frame.
#[derive(Debug)]
struct ScopeFrame {
    /// The already-disambiguated scope name. Used verbatim as a
    /// segment in [`IdGenerator::current_prefix`].
    name: String,
    /// Counter for anonymous (`$N`) children of this scope.
    child_counter: u32,
    /// Per-name occurrence counts. `seen[n]` is the number of prior
    /// [`IdGenerator::named_id`] / [`IdGenerator::push_named_scope`]
    /// calls at this frame for the bare name `n` (before suffixing).
    /// The next call at index `seen[n]` is suffixed `#seen[n]`
    /// (0 → no suffix; 1 → `#1`; 2 → `#2`; …).
    ///
    /// Field IDs use the same table, keyed by `field:<base>::<name>`,
    /// so a second `field_id(parent, "args")` on the same parent
    /// yields `parent.args#1`.
    seen: HashMap<String, usize>,
}

/// Generates scope-aware vertex IDs for AST nodes.
#[derive(Debug)]
pub struct IdGenerator {
    /// The scope stack: outermost (file path) first.
    frames: Vec<ScopeFrame>,
}

impl IdGenerator {
    /// Create a new generator rooted at the given file path.
    #[must_use]
    pub fn new(file_path: &str) -> Self {
        Self {
            frames: vec![ScopeFrame {
                name: file_path.to_owned(),
                child_counter: 0,
                seen: HashMap::new(),
            }],
        }
    }

    /// Record an occurrence of `name` in the current frame and return
    /// the disambiguated form: `name` for the first occurrence, then
    /// `name#1`, `name#2`, ….
    ///
    /// The current frame is the *parent* of any vertex about to be
    /// emitted under `name`, so disambiguation is one-per-parent.
    ///
    /// Callers that want a vertex ID and a matching scope push (the
    /// walker's case when a named-scope node is also scope-introducing)
    /// use this once and then [`Self::push_recorded_scope`] with the
    /// returned leaf, so the scope-stack frame and the vertex ID agree
    /// on the suffix.
    pub fn record_name(&mut self, name: &str) -> String {
        let Some(frame) = self.frames.last_mut() else {
            return name.to_owned();
        };
        let count = frame.seen.entry(name.to_owned()).or_insert(0);
        let n = *count;
        *count += 1;
        if n == 0 {
            name.to_owned()
        } else {
            format!("{name}#{n}")
        }
    }

    /// Push a named scope (function, class, method, module, etc.).
    ///
    /// Named scopes appear in the ID as their (disambiguated) name:
    /// `file.rs::function_name::…`. Returns the disambiguated leaf so
    /// the caller can use it as the vertex ID for the scope-introducing
    /// node itself.
    pub fn push_named_scope(&mut self, name: &str) -> String {
        let leaf = self.record_name(name);
        self.frames.push(ScopeFrame {
            name: leaf.clone(),
            child_counter: 0,
            seen: HashMap::new(),
        });
        leaf
    }

    /// Push a scope using a name that was already disambiguated by
    /// [`Self::record_name`]. The frame's `name` field becomes `leaf`
    /// verbatim — no further suffixing.
    ///
    /// Walker pattern:
    ///
    /// ```text
    /// let leaf = id_gen.record_name("foo");
    /// let vertex_id = format!("{}::{leaf}", id_gen.current_prefix());
    /// // … emit vertex, edges, constraints …
    /// id_gen.push_recorded_scope(leaf);   // frame = "foo" / "foo#1" / …
    /// ```
    pub fn push_recorded_scope(&mut self, leaf: String) {
        self.frames.push(ScopeFrame {
            name: leaf,
            child_counter: 0,
            seen: HashMap::new(),
        });
    }

    /// Push an anonymous scope (block, statement body, etc.) and
    /// return its positional index.
    pub fn push_anonymous_scope(&mut self) -> u32 {
        let Some(parent) = self.frames.last_mut() else {
            return 0;
        };
        let idx = parent.child_counter;
        parent.child_counter += 1;
        let name = format!("${idx}");
        self.frames.push(ScopeFrame {
            name,
            child_counter: 0,
            seen: HashMap::new(),
        });
        idx
    }

    /// Pop the current scope, returning to the parent.
    ///
    /// Debug builds assert that the root frame is not popped. Walker
    /// bugs that mis-pair push and pop surface here rather than as
    /// mis-shaped IDs down the road.
    pub fn pop_scope(&mut self) {
        debug_assert!(
            self.frames.len() > 1,
            "IdGenerator::pop_scope called at root; walker push/pop is unbalanced"
        );
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Generate an ID for a named node at the current scope level.
    ///
    /// Returns the full scope-qualified ID. The second call with the
    /// same `name` at the same scope returns `name#1`, the third
    /// `name#2`, and so on. Disambiguation is recorded so a matching
    /// [`Self::push_named_scope`] call returns the same suffix.
    pub fn named_id(&mut self, name: &str) -> String {
        let leaf = self.record_name(name);
        if self.frames.len() == 1 {
            format!("{}::{leaf}", self.frames[0].name)
        } else {
            format!("{}::{leaf}", self.current_prefix())
        }
    }

    /// Generate an ID for an anonymous (positional) node at the
    /// current scope level.
    ///
    /// The index is auto-incremented within the current scope and is
    /// independent of the named-id occurrence counters, so anonymous
    /// and named children can interleave at one scope without
    /// collision.
    pub fn anonymous_id(&mut self) -> String {
        let Some(frame) = self.frames.last_mut() else {
            return String::new();
        };
        let idx = frame.child_counter;
        frame.child_counter += 1;
        format!("{}::${idx}", self.current_prefix())
    }

    /// Generate an ID with a field path suffix for expression
    /// sub-nodes.
    ///
    /// Used for expression tree paths like `$3::$0.left` where
    /// `.left` is the field name within the parent expression.
    ///
    /// Repeated calls with the same `base_id` and `field_name` are
    /// disambiguated by a `#N` suffix, mirroring the named-id scheme.
    /// Tree-sitter `field('xs', repeat($.X))` shapes produce many
    /// `args` children under one parent; without disambiguation the
    /// resulting IDs would collide.
    pub fn field_id(&mut self, base_id: &str, field_name: &str) -> String {
        let key = format!("field:{base_id}::{field_name}");
        let Some(frame) = self.frames.last_mut() else {
            return format!("{base_id}.{field_name}");
        };
        let count = frame.seen.entry(key).or_insert(0);
        let n = *count;
        *count += 1;
        if n == 0 {
            format!("{base_id}.{field_name}")
        } else {
            format!("{base_id}.{field_name}#{n}")
        }
    }

    /// Get the current scope prefix (all scope components joined by `::`).
    #[must_use]
    pub fn current_prefix(&self) -> String {
        let mut out = String::new();
        for (i, frame) in self.frames.iter().enumerate() {
            if i > 0 {
                out.push_str("::");
            }
            out.push_str(&frame.name);
        }
        out
    }

    /// Get the current scope depth.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Reset the anonymous-child counter for the current scope.
    ///
    /// Useful when entering a new block within the same scope level.
    /// The `seen` table for named-id disambiguation is *not* reset;
    /// resetting it would re-introduce duplicate-vertex bugs when a
    /// caller intentionally interleaves blocks under one scope.
    pub fn reset_counter(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.child_counter = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_named_ids() {
        let mut id_gen = IdGenerator::new("src/main.rs");
        assert_eq!(id_gen.named_id("User"), "src/main.rs::User");
    }

    #[test]
    fn nested_scopes() {
        let mut id_gen = IdGenerator::new("src/lib.rs");
        id_gen.push_named_scope("Parser");
        id_gen.push_named_scope("parse");
        assert_eq!(
            id_gen.named_id("config"),
            "src/lib.rs::Parser::parse::config"
        );
        id_gen.pop_scope();
        assert_eq!(id_gen.named_id("new"), "src/lib.rs::Parser::new");
    }

    #[test]
    fn anonymous_ids_increment() {
        let mut id_gen = IdGenerator::new("test.ts");
        id_gen.push_named_scope("main");

        let id0 = id_gen.anonymous_id();
        let id1 = id_gen.anonymous_id();
        let id2 = id_gen.anonymous_id();

        assert_eq!(id0, "test.ts::main::$0");
        assert_eq!(id1, "test.ts::main::$1");
        assert_eq!(id2, "test.ts::main::$2");
    }

    #[test]
    fn anonymous_scopes() {
        let mut id_gen = IdGenerator::new("test.py");
        id_gen.push_named_scope("process");
        let _stmt_idx = id_gen.push_anonymous_scope(); // enters $0 scope
        let inner = id_gen.anonymous_id();
        assert_eq!(inner, "test.py::process::$0::$0");
        id_gen.pop_scope();
        let _stmt_idx2 = id_gen.push_anonymous_scope(); // enters $1 scope
        let inner2 = id_gen.anonymous_id();
        assert_eq!(inner2, "test.py::process::$1::$0");
    }

    #[test]
    fn field_ids() {
        let mut id_gen = IdGenerator::new("test.rs");
        let base = id_gen.named_id("expr");
        let left = id_gen.field_id(&base, "left");
        let right = id_gen.field_id(&base, "right");
        assert_eq!(left, "test.rs::expr.left");
        assert_eq!(right, "test.rs::expr.right");
    }

    #[test]
    fn depth_tracking() {
        let mut id_gen = IdGenerator::new("f.ts");
        assert_eq!(id_gen.depth(), 1);
        id_gen.push_named_scope("fn");
        assert_eq!(id_gen.depth(), 2);
        id_gen.push_anonymous_scope();
        assert_eq!(id_gen.depth(), 3);
        id_gen.pop_scope();
        assert_eq!(id_gen.depth(), 2);
    }

    /// The overload regression: three same-named declarations at the
    /// same scope produce three distinct IDs.
    #[test]
    fn duplicate_named_ids_at_same_scope_get_suffixed() {
        let mut id_gen = IdGenerator::new("repro.py");
        assert_eq!(id_gen.named_id("foo"), "repro.py::foo");
        assert_eq!(id_gen.named_id("foo"), "repro.py::foo#1");
        assert_eq!(id_gen.named_id("foo"), "repro.py::foo#2");
    }

    /// Disambiguation must propagate to the scope stack so children of
    /// the second `foo` are prefixed `foo#1`, not `foo`. Without this,
    /// `foo::body` from the first overload and `foo::body` from the
    /// second would still collide in the schema.
    #[test]
    fn push_named_scope_uses_disambiguated_leaf() {
        let mut id_gen = IdGenerator::new("repro.py");
        let first = id_gen.push_named_scope("foo");
        assert_eq!(first, "foo");
        assert_eq!(id_gen.named_id("body"), "repro.py::foo::body");
        id_gen.pop_scope();

        let second = id_gen.push_named_scope("foo");
        assert_eq!(second, "foo#1");
        assert_eq!(id_gen.named_id("body"), "repro.py::foo#1::body");
        id_gen.pop_scope();

        let third = id_gen.push_named_scope("foo");
        assert_eq!(third, "foo#2");
    }

    /// `named_id` and `push_named_scope` share the same `seen` table;
    /// using one then the other for the same bare name continues the
    /// suffix sequence.
    #[test]
    fn named_id_and_push_share_disambiguation() {
        let mut id_gen = IdGenerator::new("test.rs");
        assert_eq!(id_gen.named_id("Foo"), "test.rs::Foo");
        let leaf = id_gen.push_named_scope("Foo");
        assert_eq!(leaf, "Foo#1");
        id_gen.pop_scope();
        assert_eq!(id_gen.named_id("Foo"), "test.rs::Foo#2");
    }

    /// Anonymous and named children interleave at the same scope
    /// without collision.
    #[test]
    fn anonymous_and_named_interleave_cleanly() {
        let mut id_gen = IdGenerator::new("f.rs");
        let a = id_gen.anonymous_id();
        let x1 = id_gen.named_id("x");
        let b = id_gen.anonymous_id();
        let x2 = id_gen.named_id("x");
        assert_eq!(a, "f.rs::$0");
        assert_eq!(x1, "f.rs::x");
        assert_eq!(b, "f.rs::$1");
        assert_eq!(x2, "f.rs::x#1");
    }

    /// Repeated field names at the same parent disambiguate.
    #[test]
    fn field_ids_disambiguate_under_repeat() {
        let mut id_gen = IdGenerator::new("f.qvr");
        let base = id_gen.named_id("call");
        let a0 = id_gen.field_id(&base, "args");
        let a1 = id_gen.field_id(&base, "args");
        let a2 = id_gen.field_id(&base, "args");
        assert_eq!(a0, "f.qvr::call.args");
        assert_eq!(a1, "f.qvr::call.args#1");
        assert_eq!(a2, "f.qvr::call.args#2");
    }

    /// Sibling scopes share no disambiguation state. Two functions
    /// each containing a `foo` produce identical inner IDs.
    #[test]
    fn sibling_scopes_have_fresh_seen_tables() {
        let mut id_gen = IdGenerator::new("f.rs");
        id_gen.push_named_scope("a");
        let inner_a = id_gen.named_id("foo");
        id_gen.pop_scope();
        id_gen.push_named_scope("b");
        let inner_b = id_gen.named_id("foo");
        id_gen.pop_scope();
        assert_eq!(inner_a, "f.rs::a::foo");
        assert_eq!(inner_b, "f.rs::b::foo");
    }

    /// `pop_scope` at the root is a no-op in release; the debug
    /// assertion catches the bug.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "walker push/pop is unbalanced")
    )]
    fn pop_at_root_debug_panics_release_noops() {
        let mut id_gen = IdGenerator::new("f.rs");
        id_gen.pop_scope();
        // Release-mode reaches here; depth still 1.
        assert_eq!(id_gen.depth(), 1);
    }
}
