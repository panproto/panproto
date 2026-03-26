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

/// Generates scope-aware vertex IDs for AST nodes.
#[derive(Debug)]
pub struct IdGenerator {
    /// The scope stack: each entry is a scope name (file, function, block, etc.).
    scope_stack: Vec<String>,
    /// Counter for unnamed children within the current scope.
    child_counter: Vec<u32>,
}

impl IdGenerator {
    /// Create a new generator rooted at the given file path.
    #[must_use]
    pub fn new(file_path: &str) -> Self {
        Self {
            scope_stack: vec![file_path.to_owned()],
            child_counter: vec![0],
        }
    }

    /// Push a named scope (function, class, method, module, etc.).
    ///
    /// Named scopes appear in the ID as their name:
    /// `file.rs::function_name::...`
    pub fn push_named_scope(&mut self, name: &str) {
        self.scope_stack.push(name.to_owned());
        self.child_counter.push(0);
    }

    /// Push an anonymous scope (block, statement body, etc.).
    ///
    /// Anonymous scopes appear in the ID with a positional index:
    /// `file.rs::function_name::$3::...`
    pub fn push_anonymous_scope(&mut self) -> u32 {
        let idx = self.child_counter.last().copied().unwrap_or(0);
        if let Some(counter) = self.child_counter.last_mut() {
            *counter += 1;
        }

        self.scope_stack.push(format!("${idx}"));
        self.child_counter.push(0);
        idx
    }

    /// Pop the current scope, returning to the parent.
    pub fn pop_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
            self.child_counter.pop();
        }
    }

    /// Generate an ID for a named node at the current scope level.
    ///
    /// Returns the full scope-qualified ID (e.g. `"src/main.rs::parse_input"`).
    #[must_use]
    pub fn named_id(&self, name: &str) -> String {
        if self.scope_stack.len() == 1 {
            format!("{}::{name}", self.scope_stack[0])
        } else {
            format!("{}::{name}", self.current_prefix())
        }
    }

    /// Generate an ID for an anonymous (positional) node at the current scope level.
    ///
    /// The index is auto-incremented within the current scope.
    /// Returns the full scope-qualified ID (e.g. `"src/main.rs::parse_input::$3"`).
    pub fn anonymous_id(&mut self) -> String {
        let idx = self.child_counter.last().copied().unwrap_or(0);
        if let Some(counter) = self.child_counter.last_mut() {
            *counter += 1;
        }

        format!("{}::${idx}", self.current_prefix())
    }

    /// Generate an ID with a field path suffix for expression sub-nodes.
    ///
    /// Used for expression tree paths like `$3::$0.left` where `.left`
    /// is the field name within the parent expression.
    #[must_use]
    pub fn field_id(&self, base_id: &str, field_name: &str) -> String {
        format!("{base_id}.{field_name}")
    }

    /// Get the current scope prefix (all scope components joined by `::`).
    #[must_use]
    pub fn current_prefix(&self) -> String {
        self.scope_stack.join("::")
    }

    /// Get the current scope depth.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.scope_stack.len()
    }

    /// Reset the child counter for the current scope.
    ///
    /// Useful when entering a new block within the same scope level.
    pub fn reset_counter(&mut self) {
        if let Some(counter) = self.child_counter.last_mut() {
            *counter = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_named_ids() {
        let id_gen = IdGenerator::new("src/main.rs");
        assert_eq!(id_gen.named_id("User"), "src/main.rs::User");
    }

    #[test]
    fn nested_scopes() {
        let mut id_gen = IdGenerator::new("src/lib.rs");
        id_gen.push_named_scope("Parser");
        id_gen.push_named_scope("parse");
        assert_eq!(id_gen.named_id("config"), "src/lib.rs::Parser::parse::config");
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
        let id_gen = IdGenerator::new("test.rs");
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
}
