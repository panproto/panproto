//! Complete finite scopes for the ansible page.
//!
//! The document scope is a base playbook (one play on all hosts with a
//! handler `restart`) whose play holds no task, one of the [`tasks`], or an
//! ordered pair of them, together with the [`envelopes`]: files of the
//! three classes that change the base in one place outside its tasks.

use crate::pages::ansible::{Class, Doc, m, s};

const fn seq(items: Vec<Doc>) -> Doc {
    Doc::Seq(items)
}

fn ping() -> Doc {
    m(&[("ping", Doc::Null)])
}

/// The tasks a play of the scope may hold.
///
/// They are a named task, a task notifying the handler and one notifying
/// nothing the play declares, a loop, a lookup loop, two loops, two
/// modules, no module, the `action` keyword, a block with a rescue that
/// notifies, a block with an unknown key, task variables with a valid and
/// an invalid name, an invalid `register`, a task include, a name that is
/// not a string and a role import.
#[must_use]
pub fn tasks() -> Vec<Doc> {
    vec![
        m(&[("name", s("t")), ("ping", Doc::Null)]),
        m(&[
            ("copy", m(&[("src", s("a")), ("dest", s("b"))])),
            ("notify", s("restart")),
        ]),
        m(&[
            ("command", s("echo hi")),
            ("notify", seq(vec![s("missing")])),
        ]),
        m(&[
            ("shell", s("x")),
            ("loop", seq(vec![Doc::Int(1), Doc::Int(2)])),
        ]),
        m(&[
            ("debug", m(&[("var", s("x"))])),
            ("with_items", seq(vec![Doc::Int(1)])),
        ]),
        m(&[
            ("ping", Doc::Null),
            ("loop", seq(vec![Doc::Int(1)])),
            ("with_items", seq(vec![Doc::Int(2)])),
        ]),
        m(&[("ping", Doc::Null), ("copy", m(&[]))]),
        m(&[("name", s("n"))]),
        m(&[("action", s("ping"))]),
        m(&[
            ("block", seq(vec![ping()])),
            ("rescue", seq(vec![m(&[("debug", m(&[("msg", s("m"))]))])])),
            ("notify", s("restart")),
        ]),
        m(&[("block", seq(vec![ping()])), ("foo", Doc::Int(1))]),
        m(&[
            ("ping", Doc::Null),
            ("vars", m(&[("x", Doc::Int(1)), ("y", s("a"))])),
        ]),
        m(&[
            ("ping", Doc::Null),
            ("vars", m(&[("bad-name", Doc::Int(1))])),
        ]),
        m(&[("ping", Doc::Null), ("register", s("1x"))]),
        m(&[("include_tasks", s("other.yml"))]),
        m(&[("ping", Doc::Null), ("name", Doc::Int(5))]),
        m(&[("ansible.builtin.import_role", m(&[("name", s("r"))]))]),
    ]
}

fn handler() -> Doc {
    m(&[("name", s("restart")), ("service", m(&[("name", s("x"))]))])
}

/// A play with `extra` entries before its handlers and tasks.
fn play_with(extra: &[(&str, Doc)], handlers: Doc, tasks: Vec<Doc>) -> Doc {
    let mut entries: Vec<(&str, Doc)> = vec![("hosts", s("all"))];
    entries.extend(extra.iter().cloned());
    entries.push(("handlers", handlers));
    entries.push(("tasks", seq(tasks)));
    m(&entries)
}

/// The base playbook with `tasks` in its play.
#[must_use]
pub fn base(tasks: Vec<Doc>) -> Doc {
    seq(vec![play_with(&[], seq(vec![handler()]), tasks)])
}

fn one(extra: &[(&str, Doc)]) -> Doc {
    seq(vec![play_with(extra, seq(vec![handler()]), vec![ping()])])
}

/// Files of the three classes, each the base changed in one place outside
/// its tasks (or, for a task file or a variables file, a small file of
/// that class).
#[must_use]
pub fn envelopes() -> Vec<(Doc, Class)> {
    let playbook = |doc: Doc| (doc, Class::Playbook);
    let notifying = m(&[("ping", Doc::Null), ("notify", s("restart"))]);
    let mut out = vec![
        playbook(seq(vec![m(&[("tasks", seq(vec![ping()]))])])),
        playbook(one(&[("foo", s("x"))])),
        playbook(one(&[
            ("vars", m(&[("a", Doc::Int(1))])),
            (
                "vars_files",
                seq(vec![s("v.yml"), seq(vec![s("a.yml"), s("b.yml")])]),
            ),
        ])),
        playbook(one(&[(
            "roles",
            seq(vec![
                s("common"),
                m(&[
                    ("role", s("web")),
                    ("http_port", Doc::Int(80)),
                    ("tags", seq(vec![s("t")])),
                ]),
            ]),
        )])),
        playbook(one(&[(
            "roles",
            seq(vec![m(&[("tags", seq(vec![s("x")]))])]),
        )])),
        playbook(one(&[("roles", seq(vec![seq(vec![s("a")])]))])),
        playbook(one(&[(
            "roles",
            seq(vec![m(&[("role", s("web")), ("bad-param", Doc::Int(1))])]),
        )])),
    ];
    let Doc::Seq(mut entries) = base(vec![ping()]) else {
        return out;
    };
    entries.push(m(&[
        ("import_playbook", s("other.yml")),
        ("vars", m(&[("x", Doc::Int(1))])),
    ]));
    out.push(playbook(seq(entries)));
    out.push(playbook(seq(vec![m(&[
        ("import_playbook", s("x")),
        ("hosts", s("all")),
    ])])));
    out.push(playbook(seq(Vec::new())));
    out.push(playbook(m(&[("hosts", s("all"))])));
    out.push(playbook(seq(vec![play_with(
        &[],
        seq(vec![m(&[
            ("name", s("h")),
            ("listen", s("restart")),
            ("service", m(&[])),
        ])]),
        vec![notifying.clone()],
    )])));
    out.push(playbook(seq(vec![play_with(
        &[],
        seq(vec![m(&[("block", seq(vec![handler()]))])]),
        vec![notifying.clone()],
    )])));
    out.push(playbook(seq(vec![m(&[
        ("hosts", s("all")),
        ("tasks", s("x")),
    ])])));
    out.extend(file_envelopes(notifying));
    out
}

fn file_envelopes(notifying: Doc) -> Vec<(Doc, Class)> {
    vec![
        (seq(vec![notifying]), Class::Tasks),
        (m(&[]), Class::Tasks),
        (
            m(&[("a", Doc::Int(1)), ("b", seq(vec![Doc::Int(1), s("x")]))]),
            Class::Vars,
        ),
        (m(&[("not", Doc::Int(1))]), Class::Vars),
        (Doc::Null, Class::Vars),
        (seq(vec![Doc::Int(1)]), Class::Vars),
        (one(&[("name", Doc::Int(5))]), Class::Playbook),
        (
            seq(vec![m(&[
                ("import_playbook", s("a")),
                ("ansible.builtin.import_playbook", s("b")),
            ])]),
            Class::Playbook,
        ),
        (
            base(vec![m(&[
                ("ping", Doc::Null),
                ("register", m(&[("ok", s("x"))])),
            ])]),
            Class::Playbook,
        ),
    ]
}

/// The documents of the scope with their classes: the base playbook with
/// no task, with each task, and with each ordered pair of tasks, then the
/// envelopes.
#[must_use]
pub fn documents() -> Vec<(Doc, Class)> {
    let tasks = tasks();
    let mut out = vec![(base(Vec::new()), Class::Playbook)];
    for t in &tasks {
        out.push((base(vec![t.clone()]), Class::Playbook));
    }
    for t in &tasks {
        for u in &tasks {
            out.push((base(vec![t.clone(), u.clone()]), Class::Playbook));
        }
    }
    out.extend(envelopes());
    out
}

/// The closed form of the scope's size for `t` tasks and `e` envelopes:
/// `1 + t + t² + e`.
#[must_use]
pub const fn document_count(t: usize, e: usize) -> usize {
    1 + t + t * t + e
}
