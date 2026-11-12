//! Ansible protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! An Ansible playbook, task file or variables file (ansible-core 2.22) is
//! YAML, and this crate links no YAML grammar:
//! `panproto_io::config::ansible::read_ansible` reads it through the shared
//! YAML route of `panproto-io` (feature `tree-sitter`) into a schema of this
//! protocol, and both schema dispatches refuse the protocol with an error
//! naming that reader.
//!
//! The schema has a root vertex `/` of kind `playbook`, `task-file` or
//! `vars-file`; a playbook's plays and playbook imports below `entry`
//! edges; a play's roles, tasks and handlers below edges named by their
//! section (`roles`, `pre_tasks`, `tasks`, `post_tasks`, `handlers`); a
//! block's tasks below `block`, `rescue` and `always` edges; a `variable`
//! vertex per variable below a `var` edge (or, for a role parameter, a
//! `param` edge) named by the variable; and a `notify` edge from a task or
//! block to each handler of its play that a `notify` name reaches.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::theories;

/// The keywords the reader keeps as constraints.
///
/// Each holds the keyword's value as canonical JSON text, plus `module`
/// and `arguments` (a task's module and its arguments), `with` and
/// `with_terms` (a `with_<lookup>` loop's lookup and terms), `role` (a role
/// entry's role), `import_playbook` (the playbook an entry imports) and
/// `value` (a variable's value).
pub const CONSTRAINT_SORTS: &[&str] = &[
    "name",
    "connection",
    "port",
    "remote_user",
    "module_defaults",
    "environment",
    "no_log",
    "run_once",
    "ignore_errors",
    "ignore_unreachable",
    "check_mode",
    "diff",
    "any_errors_fatal",
    "throttle",
    "timeout",
    "debugger",
    "become",
    "become_method",
    "become_user",
    "become_flags",
    "become_exe",
    "tags",
    "collections",
    "hosts",
    "gather_facts",
    "gather_subset",
    "gather_timeout",
    "fact_path",
    "vars_files",
    "vars_prompt",
    "validate_argspec",
    "force_handlers",
    "max_fail_percentage",
    "serial",
    "strategy",
    "order",
    "when",
    "notify",
    "delegate_to",
    "delegate_facts",
    "args",
    "async",
    "changed_when",
    "delay",
    "failed_when",
    "loop",
    "loop_control",
    "poll",
    "register",
    "retries",
    "until",
    "listen",
    "module",
    "arguments",
    "with",
    "with_terms",
    "role",
    "import_playbook",
    "value",
];

/// The vertex kinds of an Ansible schema.
const KINDS: &[&str] = &[
    "playbook",
    "task-file",
    "vars-file",
    "play",
    "playbook-import",
    "role",
    "block",
    "task",
    "handler",
    "variable",
];

/// Returns the Ansible protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "ansible".into(),
        schema_theory: "ThAnsibleSchema".into(),
        instance_theory: "ThAnsibleInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: KINDS.iter().map(|kind| (*kind).into()).collect(),
        constraint_sorts: CONSTRAINT_SORTS
            .iter()
            .map(|sort| (*sort).to_owned())
            .collect(),
        has_order: true,
        has_recursion: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for Ansible.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThAnsibleSchema",
        "ThAnsibleInstance",
    );
}

fn rule(kind: &str, src: &[&str], tgt: &[&str]) -> EdgeRule {
    let names = |kinds: &[&str]| kinds.iter().map(|k| (*k).to_owned()).collect();
    EdgeRule {
        edge_kind: kind.into(),
        src_kinds: names(src),
        tgt_kinds: names(tgt),
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    let tasks = &["task", "block"][..];
    vec![
        rule("entry", &["playbook"], &["play", "playbook-import"]),
        rule("pre_tasks", &["play"], tasks),
        rule("tasks", &["play", "task-file"], tasks),
        rule("post_tasks", &["play"], tasks),
        rule("handlers", &["play"], &["handler", "block"]),
        rule("roles", &["play"], &["role"]),
        rule("block", &["block"], &["task", "handler", "block"]),
        rule("rescue", &["block"], &["task", "handler", "block"]),
        rule("always", &["block"], &["task", "handler", "block"]),
        rule(
            "var",
            &[
                "vars-file",
                "playbook-import",
                "play",
                "role",
                "block",
                "task",
                "handler",
            ],
            &["variable"],
        ),
        rule("param", &["role"], &["variable"]),
        rule("notify", &["block", "task", "handler"], &["handler"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_declares_every_kept_keyword_as_a_constraint_sort() {
        let p = protocol();
        assert_eq!(p.name, "ansible");
        for sort in CONSTRAINT_SORTS {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "{sort} is not declared"
            );
        }
    }

    #[test]
    fn register_theories_registers_the_schema_theory() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAnsibleSchema"));
    }
}
