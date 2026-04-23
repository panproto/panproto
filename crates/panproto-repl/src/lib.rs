//! Interactive REPL driver for panproto theories, terms, and morphisms.
//!
//! The binary (`panproto-repl`) wires [`Repl::handle_line`] to
//! `rustyline`. Tests drive the same entry point with a scripted input
//! stream so no terminal is required.
//!
//! ## Commands
//!
//! Commands start with `:`. Bare input (no leading `:`) is treated as a
//! term to typecheck in the active theory.
//!
//! - `:load <path>`: load a theory document.
//! - `:theories`: list loaded theories.
//! - `:use <name>`: switch the active theory.
//! - `:sorts`, `:ops`: list the active theory's sorts and ops.
//! - `:type <term>`: infer the sort of a term.
//! - `:normalize <term>`: normalize a term under the active theory's
//!   directed equations.
//! - `:model [depth]`: print terms in each fiber from the active
//!   theory's free model.
//! - `:instance <class> in <target> { <bindings> }`: compile an ad-hoc
//!   instance morphism.
//! - `:quit` / `:q`: leave the REPL.

#![allow(clippy::too_many_lines)]

use std::collections::HashMap;
use std::path::Path;

use panproto_gat::{
    FreeModelConfig, Theory, TheoryMorphism, VarContext, free_model, normalize, typecheck_term,
};
use panproto_theory_dsl::compile_theory::parse_term;
use panproto_theory_dsl::{builtin_resolver, load_and_compile};

/// The REPL driver state. Holds loaded theories, the currently active
/// theory name, and any compiled morphisms.
#[derive(Default)]
pub struct Repl {
    /// Loaded theories, keyed by name.
    pub theories: HashMap<String, Theory>,
    /// Loaded morphisms, keyed by name.
    pub morphisms: HashMap<String, TheoryMorphism>,
    /// Currently active theory name, if any.
    pub active: Option<String>,
}

/// Outcome of processing a single line.
#[derive(Debug, Clone)]
pub enum ReplOutcome {
    /// Normal output to display.
    Output(String),
    /// An error message to display.
    Error(String),
    /// The user asked the REPL to quit.
    Quit,
}

impl Repl {
    /// Construct a fresh, empty REPL.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a single input line. Returns the outcome that the host
    /// should display.
    #[must_use]
    pub fn handle_line(&mut self, line: &str) -> ReplOutcome {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return ReplOutcome::Output(String::new());
        }
        if let Some(rest) = trimmed.strip_prefix(':') {
            self.handle_command(rest.trim())
        } else {
            self.handle_term_typecheck(trimmed)
        }
    }

    fn handle_command(&mut self, rest: &str) -> ReplOutcome {
        let (head, tail) = split_first_word(rest);
        match head {
            "quit" | "q" => ReplOutcome::Quit,
            "load" => self.cmd_load(tail),
            "theories" => self.cmd_theories(),
            "use" => self.cmd_use(tail),
            "sorts" => self.cmd_sorts(),
            "ops" => self.cmd_ops(),
            "type" => self.cmd_type(tail),
            "normalize" => self.cmd_normalize(tail),
            "model" => self.cmd_model(tail),
            "instance" => self.cmd_instance(tail),
            other => ReplOutcome::Error(format!("unknown command :{other}")),
        }
    }

    fn cmd_load(&mut self, path: &str) -> ReplOutcome {
        if path.is_empty() {
            return ReplOutcome::Error("usage: :load <path>".to_string());
        }
        let p = Path::new(path);
        let resolver = builtin_resolver();
        match load_and_compile(p, &resolver) {
            Ok(set) => {
                let mut loaded: Vec<String> = Vec::new();
                for (name, theory) in set.theories {
                    loaded.push(name.clone());
                    self.theories.insert(name, theory);
                }
                for (name, morph) in set.morphisms {
                    self.morphisms.insert(name, morph);
                }
                if self.active.is_none() {
                    self.active = loaded.first().cloned();
                }
                let msg = if loaded.is_empty() {
                    "loaded no theories".to_string()
                } else {
                    format!("loaded: {}", loaded.join(", "))
                };
                ReplOutcome::Output(msg)
            }
            Err(e) => ReplOutcome::Error(format!("load failed: {e}")),
        }
    }

    fn cmd_theories(&self) -> ReplOutcome {
        if self.theories.is_empty() {
            return ReplOutcome::Output("(no theories loaded)".to_string());
        }
        let mut names: Vec<&String> = self.theories.keys().collect();
        names.sort();
        let marked: Vec<String> = names
            .into_iter()
            .map(|n| {
                if self.active.as_deref() == Some(n.as_str()) {
                    format!("* {n}")
                } else {
                    format!("  {n}")
                }
            })
            .collect();
        ReplOutcome::Output(marked.join("\n"))
    }

    fn cmd_use(&mut self, name: &str) -> ReplOutcome {
        if name.is_empty() {
            return ReplOutcome::Error("usage: :use <theory>".to_string());
        }
        if !self.theories.contains_key(name) {
            return ReplOutcome::Error(format!("theory '{name}' not loaded"));
        }
        self.active = Some(name.to_string());
        ReplOutcome::Output(format!("active theory: {name}"))
    }

    fn active_theory(&self) -> Result<&Theory, ReplOutcome> {
        let name = self.active.as_ref().ok_or_else(|| {
            ReplOutcome::Error("no active theory; use :load and :use".to_string())
        })?;
        self.theories
            .get(name)
            .ok_or_else(|| ReplOutcome::Error(format!("active theory '{name}' missing")))
    }

    fn cmd_sorts(&self) -> ReplOutcome {
        let theory = match self.active_theory() {
            Ok(t) => t,
            Err(e) => return e,
        };
        if theory.sorts.is_empty() {
            return ReplOutcome::Output("(no sorts)".to_string());
        }
        let lines: Vec<String> = theory
            .sorts
            .iter()
            .map(|s| {
                if s.params.is_empty() {
                    s.name.to_string()
                } else {
                    let params: Vec<String> = s
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.sort))
                        .collect();
                    format!("{}({})", s.name, params.join(", "))
                }
            })
            .collect();
        ReplOutcome::Output(lines.join("\n"))
    }

    fn cmd_ops(&self) -> ReplOutcome {
        let theory = match self.active_theory() {
            Ok(t) => t,
            Err(e) => return e,
        };
        if theory.ops.is_empty() {
            return ReplOutcome::Output("(no ops)".to_string());
        }
        let lines: Vec<String> = theory
            .ops
            .iter()
            .map(|op| {
                let inputs: Vec<String> = op
                    .inputs
                    .iter()
                    .map(|(n, s, _)| format!("{n}: {s}"))
                    .collect();
                format!("{}({}) -> {}", op.name, inputs.join(", "), op.output)
            })
            .collect();
        ReplOutcome::Output(lines.join("\n"))
    }

    fn cmd_type(&self, term_src: &str) -> ReplOutcome {
        let theory = match self.active_theory() {
            Ok(t) => t,
            Err(e) => return e,
        };
        let term = match parse_term(term_src) {
            Ok(t) => t,
            Err(e) => return ReplOutcome::Error(format!("parse: {e}")),
        };
        let ctx = VarContext::default();
        match typecheck_term(&term, &ctx, theory) {
            Ok(sort) => ReplOutcome::Output(format!("{term} : {sort}")),
            Err(e) => ReplOutcome::Error(format!("typecheck: {e}")),
        }
    }

    fn cmd_normalize(&self, term_src: &str) -> ReplOutcome {
        let theory = match self.active_theory() {
            Ok(t) => t,
            Err(e) => return e,
        };
        let term = match parse_term(term_src) {
            Ok(t) => t,
            Err(e) => return ReplOutcome::Error(format!("parse: {e}")),
        };
        let nf = normalize(&term, &theory.directed_eqs, 1000);
        ReplOutcome::Output(format!("{nf}"))
    }

    fn cmd_model(&self, tail: &str) -> ReplOutcome {
        let theory = match self.active_theory() {
            Ok(t) => t,
            Err(e) => return e,
        };
        let depth = if tail.is_empty() {
            3
        } else {
            match tail.parse::<usize>() {
                Ok(d) => d,
                Err(_) => return ReplOutcome::Error(format!("invalid depth: {tail}")),
            }
        };
        let cfg = FreeModelConfig {
            max_depth: depth,
            ..FreeModelConfig::default()
        };
        match free_model(theory, &cfg) {
            Ok(result) => {
                let mut lines: Vec<String> = Vec::new();
                for (sort_name, carrier) in &result.model.sort_interp {
                    let preview: Vec<String> =
                        carrier.iter().take(5).map(|v| format!("{v:?}")).collect();
                    lines.push(format!(
                        "{sort_name}: [{}{suffix}]",
                        preview.join(", "),
                        suffix = if carrier.len() > 5 { ", ..." } else { "" },
                    ));
                }
                ReplOutcome::Output(lines.join("\n"))
            }
            Err(e) => ReplOutcome::Error(format!("free model failed: {e}")),
        }
    }

    fn cmd_instance(&mut self, rest: &str) -> ReplOutcome {
        // Minimal surface: `:instance <class> in <target> { sort = target_sort; op = target_op; ... }`
        let Some(brace_pos) = rest.find('{') else {
            return ReplOutcome::Error(
                "usage: :instance <class> in <target> { binding = target; ... }".to_string(),
            );
        };
        let header = rest[..brace_pos].trim();
        let body = rest[brace_pos + 1..].trim().trim_end_matches('}');
        let Some(in_pos) = header.find(" in ") else {
            return ReplOutcome::Error("expected `<class> in <target>` before `{`".to_string());
        };
        let class_name = header[..in_pos].trim();
        let target_name = header[in_pos + 4..].trim();
        let class_theory = match self.theories.get(class_name) {
            Some(t) => t.clone(),
            None => return ReplOutcome::Error(format!("class theory '{class_name}' not loaded")),
        };
        let target_theory = match self.theories.get(target_name) {
            Some(t) => t.clone(),
            None => return ReplOutcome::Error(format!("target theory '{target_name}' not loaded")),
        };
        let mut sort_map: HashMap<std::sync::Arc<str>, std::sync::Arc<str>> = HashMap::new();
        let mut op_map: HashMap<std::sync::Arc<str>, std::sync::Arc<str>> = HashMap::new();
        for entry in body.split(';') {
            let e = entry.trim();
            if e.is_empty() {
                continue;
            }
            let Some(eq_pos) = e.find('=') else {
                return ReplOutcome::Error(format!("binding missing `=`: {e}"));
            };
            let from = e[..eq_pos].trim();
            let to = e[eq_pos + 1..].trim();
            if class_theory.find_sort(from).is_some() {
                sort_map.insert(std::sync::Arc::from(from), std::sync::Arc::from(to));
            } else if class_theory.find_op(from).is_some() {
                op_map.insert(std::sync::Arc::from(from), std::sync::Arc::from(to));
            } else {
                return ReplOutcome::Error(format!(
                    "binding '{from}' is neither a sort nor an op of class '{class_name}'"
                ));
            }
        }
        let morphism = TheoryMorphism::new(
            format!("{class_name}_to_{target_name}"),
            class_name,
            target_name,
            sort_map,
            op_map,
        );
        match panproto_gat::check_morphism(&morphism, &class_theory, &target_theory) {
            Ok(()) => {
                let name = morphism.name.to_string();
                self.morphisms.insert(name.clone(), morphism);
                ReplOutcome::Output(format!("instance {name} ok"))
            }
            Err(e) => ReplOutcome::Error(format!("instance check failed: {e}")),
        }
    }

    fn handle_term_typecheck(&self, src: &str) -> ReplOutcome {
        self.cmd_type(src)
    }
}

fn split_first_word(s: &str) -> (&str, &str) {
    s.find(char::is_whitespace)
        .map_or((s, ""), |i| (s[..i].trim(), s[i..].trim()))
}
