//! Complete finite scopes for the theory-dsl page.
//!
//! Every scope is a cartesian product of short axes, so its size is the
//! product of the axis lengths. Documents are JSON text; the library holds
//! the theories `L`, `K` and `C` the documents name, and `M` names no
//! theory.

use crate::json::Value;
use crate::pages::theories::theory::{
    Equation, Implicit, Op, Sort, SortExpr, SortKind, Term, Theory,
};

fn simple_sort(name: &str) -> Sort {
    Sort {
        name: name.to_owned(),
        params: Vec::new(),
        kind: SortKind::Structural,
        constructors: None,
    }
}

fn unary(name: &str, binder: &str, input: &str, output: &str) -> Op {
    Op {
        name: name.to_owned(),
        inputs: vec![(
            binder.to_owned(),
            SortExpr::Name(input.to_owned()),
            Implicit::No,
        )],
        output: SortExpr::Name(output.to_owned()),
    }
}

fn retraction(name: &str, outer: &str, inner: &str, var: &str) -> Equation {
    Equation {
        name: name.to_owned(),
        lhs: Term::App(
            outer.to_owned(),
            vec![Term::App(inner.to_owned(), vec![Term::Var(var.to_owned())])],
        ),
        rhs: Term::Var(var.to_owned()),
    }
}

/// The library theory `L`: sorts `V`, `E`; `s, t : E -> V`, `r : V -> E`;
/// `sr : s(r(v)) = v`.
#[must_use]
pub fn theory_l() -> Theory {
    Theory {
        name: "L".to_owned(),
        sorts: vec![simple_sort("V"), simple_sort("E")],
        ops: vec![
            unary("s", "e", "E", "V"),
            unary("t", "e", "E", "V"),
            unary("r", "v", "V", "E"),
        ],
        eqs: vec![retraction("sr", "s", "r", "v")],
        ..Theory::default()
    }
}

/// The library theory `K`: sorts `P`, `Q`; `a, b : Q -> P`, `c : P -> Q`;
/// `ac : a(c(p)) = p`.
#[must_use]
pub fn theory_k() -> Theory {
    Theory {
        name: "K".to_owned(),
        sorts: vec![simple_sort("P"), simple_sort("Q")],
        ops: vec![
            unary("a", "e", "Q", "P"),
            unary("b", "e", "Q", "P"),
            unary("c", "p", "P", "Q"),
        ],
        eqs: vec![retraction("ac", "a", "c", "p")],
        ..Theory::default()
    }
}

fn var(name: &str) -> Term {
    Term::Var(name.to_owned())
}

fn app(op: &str, args: Vec<Term>) -> Term {
    Term::App(op.to_owned(), args)
}

fn hom(a: &str, b: &str) -> SortExpr {
    SortExpr::App("Hom".to_owned(), vec![var(a), var(b)])
}

fn ob() -> SortExpr {
    SortExpr::Name("Ob".to_owned())
}

/// The library theory `C` of categories with implicit objects.
///
/// Sorts `Ob` and `Hom(a : Ob, b : Ob)`; `id(x : Ob) : Hom(x, x)` and
/// `comp({a b c : Ob}, g : Hom(b, c), f : Hom(a, b)) : Hom(a, c)` with `a`,
/// `b`, `c` implicit; `idl : comp(id(b), f) = f` and `idr : comp(f, id(a))
/// = f`.
#[must_use]
pub fn theory_c() -> Theory {
    let implicit = |n: &str| (n.to_owned(), ob(), Implicit::Yes);
    Theory {
        name: "C".to_owned(),
        sorts: vec![
            simple_sort("Ob"),
            Sort {
                name: "Hom".to_owned(),
                params: vec![("a".to_owned(), ob()), ("b".to_owned(), ob())],
                kind: SortKind::Structural,
                constructors: None,
            },
        ],
        ops: vec![
            Op {
                name: "id".to_owned(),
                inputs: vec![("x".to_owned(), ob(), Implicit::No)],
                output: hom("x", "x"),
            },
            Op {
                name: "comp".to_owned(),
                inputs: vec![
                    implicit("a"),
                    implicit("b"),
                    implicit("c"),
                    ("g".to_owned(), hom("b", "c"), Implicit::No),
                    ("f".to_owned(), hom("a", "b"), Implicit::No),
                ],
                output: hom("a", "c"),
            },
        ],
        eqs: vec![
            Equation {
                name: "idl".to_owned(),
                lhs: app("comp", vec![app("id", vec![var("b")]), var("f")]),
                rhs: var("f"),
            },
            Equation {
                name: "idr".to_owned(),
                lhs: app("comp", vec![var("f"), app("id", vec![var("a")])]),
                rhs: var("f"),
            },
        ],
        ..Theory::default()
    }
}

/// The library: `L`, `K` and `C`; every other name is unresolved.
#[must_use]
pub fn library(name: &str) -> Option<Theory> {
    match name {
        "L" => Some(theory_l()),
        "K" => Some(theory_k()),
        "C" => Some(theory_c()),
        _ => None,
    }
}

/// `extends` members of the theory scope: none, the library theory, a
/// missing theory.
pub const EXTENDS: &[&str] = &["", r#""extends":["L"],"#, r#""extends":["M"],"#];

/// `imports` members of the theory scope.
pub const IMPORTS: &[&str] = &[
    "",
    r#""imports":[{"from":"L"}],"#,
    r#""imports":[{"from":"L","alias":"A"}],"#,
    r#""imports":[{"from":"L","alias":"A","expose":["V","s"]}],"#,
    r#""imports":[{"from":"L","expose":["W"]}],"#,
    r#""imports":[{"from":"M"}],"#,
];

/// `sorts` lists of the theory scope.
pub const SORTS: &[&str] = &[
    r#""sorts":[]"#,
    r#""sorts":[{"name":"V"}]"#,
    r#""sorts":[{"name":"N"}]"#,
    r#""sorts":[{"name":"N"},{"name":"N"}]"#,
];

/// `ops` members of the theory scope: none, or one operation.
pub const OPS: &[&str] = &[
    "",
    r#","ops":[{"name":"f","input":"V","output":"V"}]"#,
    r#","ops":[{"name":"f","input":"N","output":"V"}]"#,
    r#","ops":[{"name":"f","input":"L.V","output":"N"}]"#,
    r#","ops":[{"name":"f","input":"A.E","output":"A.V"}]"#,
    r#","ops":[{"name":"f","input":"Q.V","output":"V"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"E"}],"output":"V"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"V"},{"name":"y","sort":"V()"}],"output":"E"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"V"},{"name":"x","sort":"V"}],"output":"V"}]"#,
    r#","ops":[{"name":"f","input":"V","inputs":[{"name":"x","sort":"V"}],"output":"V"}]"#,
    r#","ops":[{"name":"f","output":"N"}]"#,
    r#","ops":[{"name":"f","input":"V(","output":"V"}]"#,
    r#","ops":[{"name":"f","input":"V","output":"Z"}]"#,
    r#","ops":[{"name":"s","input":"E","output":"V"}]"#,
    r#","ops":[{"name":"f","input":"9V","output":"V"}]"#,
    r#","ops":[{"name":"f","inputs":[],"output":"V"}]"#,
];

/// `equations` members of the theory scope: none, or one equation.
pub const EQUATIONS: &[&str] = &[
    "",
    r#","equations":[{"name":"e","lhs":"f(x)","rhs":"f(x)"}]"#,
    r#","equations":[{"name":"e","lhs":"s(r(v))","rhs":"v"}]"#,
    r#","equations":[{"name":"e","lhs":"L.s(L.r( v ))","rhs":"v"}]"#,
    r#","equations":[{"name":"e","lhs":"f(x,)","rhs":"f(x)"}]"#,
    r#","equations":[{"name":"e","lhs":"r(v)","rhs":"v"}]"#,
];

/// Every theory document of the scope, `|EXTENDS|·|IMPORTS|·|SORTS|·|OPS|·
/// |EQUATIONS|` of them.
#[must_use]
pub fn theory_documents() -> Vec<String> {
    let mut out = Vec::new();
    for extends in EXTENDS {
        for imports in IMPORTS {
            for sorts in SORTS {
                for ops in OPS {
                    for equations in EQUATIONS {
                        out.push(format!(
                            r#"{{"id":"t","description":"","theory":"T",{extends}{imports}{sorts}{ops}{equations}}}"#
                        ));
                    }
                }
            }
        }
    }
    out
}

/// `extends` members of the dependent scope: none, or the category theory.
pub const DEPENDENT_EXTENDS: &[&str] = &["", r#""extends":["C"],"#];

/// `imports` members of the dependent scope: none, or `C` imported bare,
/// aliased, or aliased with `Hom` exposed.
pub const DEPENDENT_IMPORTS: &[&str] = &[
    "",
    r#""imports":[{"from":"C"}],"#,
    r#""imports":[{"from":"C","alias":"D"}],"#,
    r#""imports":[{"from":"C","alias":"D","expose":["Hom"]}],"#,
];

/// `sorts` lists of the dependent scope.
pub const DEPENDENT_SORTS: &[&str] = &[
    r#""sorts":[]"#,
    r#""sorts":[{"name":"Ob"},{"name":"Hom","params":[{"name":"a","sort":"Ob"},{"name":"b","sort":"Ob"}]}]"#,
    r#""sorts":[{"name":"Ob"},{"name":"Hom","params":[{"name":"a","sort":"Ob"},{"name":"b","sort":"Ob"}]},{"name":"Sq","params":[{"name":"x","sort":"Ob"},{"name":"f","sort":"Hom(x, x)"}]}]"#,
    r#""sorts":[{"name":"Cell","params":[{"name":"f","sort":"Hom(a, b)"}]}]"#,
    r#""sorts":[{"name":"Sq","params":[{"name":"x","sort":"Ob"},{"name":"f","sort":"D.Hom(x, x)"}]}]"#,
    r#""sorts":[{"name":"Ob"},{"name":"P","params":[{"name":"a","sort":"Ob"},{"name":"a","sort":"Ob"}]}]"#,
    r#""sorts":[{"name":"Ob"},{"name":"P","params":[{"name":"a","sort":"Ob("}]}]"#,
    r#""sorts":[{"name":"Ob"},{"name":"P","params":[{"name":"a","sort":"Ob(a)"}]}]"#,
];

/// `ops` members of the dependent scope: none, or one operation.
pub const DEPENDENT_OPS: &[&str] = &[
    "",
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"}],"output":"Hom(x, x)"}]"#,
    r#","ops":[{"name":"g","inputs":[{"name":"a","sort":"Ob","implicit":true},{"name":"b","sort":"Ob","implicit":true},{"name":"h","sort":"Hom(a, b)"}],"output":"Hom(b, a)"}]"#,
    r#","ops":[{"name":"g","inputs":[{"name":"a","sort":"Ob","implicit":true},{"name":"x","sort":"Ob"}],"output":"Ob"}]"#,
    r#","ops":[{"name":"u","inputs":[{"name":"a","sort":"Ob","implicit":true}],"output":"Hom(a, a)"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"}],"output":"Hom(x)"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"}],"output":"Hom(x, y)"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"},{"name":"h","sort":"Hom(x, x)"}],"output":"Hom(h, x)"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"C.Ob"}],"output":"C.Hom( x , x )"}]"#,
    r#","ops":[{"name":"f","input":"Hom(a, b)","output":"Ob"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"}],"output":"Hom(x,)"}]"#,
    r#","ops":[{"name":"f","inputs":[{"name":"x","sort":"Ob"}],"output":"Hom(x, id(x))"}]"#,
];

/// `equations` members of the dependent scope: none, or one equation.
pub const DEPENDENT_EQUATIONS: &[&str] = &[
    "",
    r#","equations":[{"name":"e","lhs":"comp(id(b), h)","rhs":"h"}]"#,
    r#","equations":[{"name":"e","lhs":"comp(id(a), id(b))","rhs":"id(a)"}]"#,
    r#","equations":[{"name":"e","lhs":"comp(h, comp(g, f))","rhs":"comp(comp(h, g), f)"}]"#,
    r#","equations":[{"name":"e","lhs":"f(x)","rhs":"id(x)"}]"#,
    r#","equations":[{"name":"e","lhs":"g(h)","rhs":"g(h)"}]"#,
    r#","equations":[{"name":"e","lhs":"comp(f(x), id(y))","rhs":"f(x)"}]"#,
];

/// Every theory document of the dependent scope, `|DEPENDENT_EXTENDS|·
/// |DEPENDENT_IMPORTS|·|DEPENDENT_SORTS|·|DEPENDENT_OPS|·
/// |DEPENDENT_EQUATIONS|` of them.
#[must_use]
pub fn dependent_documents() -> Vec<String> {
    let mut out = Vec::new();
    for extends in DEPENDENT_EXTENDS {
        for imports in DEPENDENT_IMPORTS {
            for sorts in DEPENDENT_SORTS {
                for ops in DEPENDENT_OPS {
                    for equations in DEPENDENT_EQUATIONS {
                        out.push(format!(
                            r#"{{"id":"t","description":"","theory":"T",{extends}{imports}{sorts}{ops}{equations}}}"#
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Domains and codomains of the morphism scope.
pub const MORPHISM_ENDS: &[(&str, &str)] = &[("L", "K"), ("L", "M"), ("M", "K"), ("M", "M")];

/// The images of a library sort in the morphism scope: unmapped, `P`, `Q`.
pub const SORT_IMAGES: &[Option<&str>] = &[None, Some("P"), Some("Q")];

/// The images of a library operation in the morphism scope.
pub const OP_IMAGES: &[Option<&str>] = &[None, Some("a"), Some("b"), Some("c")];

/// An extra sort-map entry: none, or `Z`, which `L` does not declare.
pub const EXTRA_SORT_ENTRY: &[Option<&str>] = &[None, Some("Z")];

fn map_text(entries: &[(&str, Option<&str>)]) -> String {
    let inner: Vec<String> = entries
        .iter()
        .filter_map(|(k, v)| v.map(|v| format!(r#""{k}":"{v}""#)))
        .collect();
    format!("{{{}}}", inner.join(","))
}

/// Every morphism document of the scope: `|MORPHISM_ENDS|·|SORT_IMAGES|²·
/// |EXTRA_SORT_ENTRY|·|OP_IMAGES|³` of them.
#[must_use]
pub fn morphism_documents() -> Vec<String> {
    let mut out = Vec::new();
    for (domain, codomain) in MORPHISM_ENDS {
        for v in SORT_IMAGES {
            for e in SORT_IMAGES {
                for extra in EXTRA_SORT_ENTRY {
                    for s in OP_IMAGES {
                        for t in OP_IMAGES {
                            for r in OP_IMAGES {
                                let sorts = map_text(&[
                                    ("E", *e),
                                    ("V", *v),
                                    (extra.unwrap_or("_"), extra.map(|_| "P")),
                                ]);
                                let ops = map_text(&[("r", *r), ("s", *s), ("t", *t)]);
                                out.push(format!(
                                    r#"{{"id":"m","description":"","morphism":"F","domain":"{domain}","codomain":"{codomain}","sort_map":{sorts},"op_map":{ops}}}"#
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// The theory specs a bundle of the scope lists.
pub const BUNDLE_THEORIES: &[&str] = &[
    r#"{"theory":"A","sorts":[{"name":"X"}]}"#,
    r#"{"theory":"B","extends":["A"],"ops":[{"name":"f","input":"X","output":"X"}]}"#,
    r#"{"theory":"A","imports":[{"from":"L","alias":"P"}]}"#,
    r#"{"theory":"C","imports":[{"from":"B"}],"equations":[{"name":"e","lhs":"f(x)","rhs":"x"}]}"#,
];

/// Every bundle document of the scope: each sequence of at most three of
/// [`BUNDLE_THEORIES`], `1 + 4 + 16 + 64` of them.
#[must_use]
pub fn bundle_documents() -> Vec<String> {
    crate::enumerate::sequences(&(0..BUNDLE_THEORIES.len()).collect::<Vec<_>>(), 3)
        .into_iter()
        .map(|sequence| {
            let theories: Vec<&str> = sequence.iter().map(|i| BUNDLE_THEORIES[*i]).collect();
            format!(
                r#"{{"id":"b","description":"","bundle":"B","theories":[{}]}}"#,
                theories.join(",")
            )
        })
        .collect()
}

/// Schema theory references of the protocol scope: library and missing
/// names, and inline theories (plain, extending `L`, ill formed, and named
/// as the library theory `L`).
pub const PROTOCOL_SCHEMAS: &[&str] = &[
    r#""L""#,
    r#""M""#,
    r#"{"theory":"S","sorts":[{"name":"X"}]}"#,
    r#"{"theory":"S","extends":["L"],"ops":[{"name":"f","input":"V","output":"E"}]}"#,
    r#"{"theory":"S","ops":[{"name":"f","output":"Q"}]}"#,
    r#"{"theory":"L","sorts":[{"name":"X"}]}"#,
];

/// Instance theory references of the protocol scope.
///
/// Library and missing names, an inline theory equal to the first inline
/// schema theory, an inline theory with an aliased import, and one
/// importing a missing theory.
pub const PROTOCOL_INSTANCES: &[&str] = &[
    r#""K""#,
    r#""M""#,
    r#"{"theory":"S","sorts":[{"name":"X"}]}"#,
    r#"{"theory":"I","imports":[{"from":"L","alias":"A"}]}"#,
    r#"{"theory":"I","imports":[{"from":"M"}]}"#,
];

/// Edge-rule members of the protocol scope: none, one rule, two rules.
pub const PROTOCOL_EDGE_RULES: &[&str] = &[
    "",
    r#","edge_rules":[{"edge_kind":"prop","src_kind":"object","tgt_kind":"string"}]"#,
    r#","edge_rules":[{"edge_kind":"prop","src_kind":"object","tgt_kind":"string"},{"edge_kind":"item","src_kind":"array","tgt_kind":"object"}]"#,
];

/// Every protocol document of the scope, `|PROTOCOL_SCHEMAS|·
/// |PROTOCOL_INSTANCES|·|PROTOCOL_EDGE_RULES|` of them.
#[must_use]
pub fn protocol_documents() -> Vec<String> {
    let mut out = Vec::new();
    for schema in PROTOCOL_SCHEMAS {
        for instance in PROTOCOL_INSTANCES {
            for rules in PROTOCOL_EDGE_RULES {
                out.push(format!(
                    r#"{{"id":"p","description":"","protocol":"P","schema_theory":{schema},"instance_theory":{instance}{rules}}}"#
                ));
            }
        }
    }
    out
}

/// The protocol specs a bundle of the protocol bundle scope lists.
///
/// After the bundle's theory `A` with sort `X`: one naming `A`, one whose
/// inline theory is `A` again, one whose inline theory is another `A`, and
/// one naming the theory its own schema reference builds inline.
pub const BUNDLE_PROTOCOLS: &[&str] = &[
    r#"{"protocol":"P","schema_theory":"A","instance_theory":"L"}"#,
    r#"{"protocol":"Q","schema_theory":{"theory":"A","sorts":[{"name":"X"}]},"instance_theory":"A"}"#,
    r#"{"protocol":"R","schema_theory":{"theory":"A","sorts":[{"name":"Y"}]},"instance_theory":"K"}"#,
    r#"{"protocol":"T","schema_theory":{"theory":"B","sorts":[{"name":"X"}]},"instance_theory":"B"}"#,
];

/// Every bundle document of the protocol bundle scope: the theory `A` and
/// each sequence of at most three of [`BUNDLE_PROTOCOLS`], `1 + 4 + 16 +
/// 64` of them.
#[must_use]
pub fn protocol_bundle_documents() -> Vec<String> {
    crate::enumerate::sequences(&(0..BUNDLE_PROTOCOLS.len()).collect::<Vec<_>>(), 3)
        .into_iter()
        .map(|sequence| {
            let protocols: Vec<&str> = sequence.iter().map(|i| BUNDLE_PROTOCOLS[*i]).collect();
            format!(
                r#"{{"id":"b","description":"","bundle":"B","theories":[{{"theory":"A","sorts":[{{"name":"X"}}]}}],"protocols":[{}]}}"#,
                protocols.join(",")
            )
        })
        .collect()
}

/// The base documents the grammar scope mutates: one per body, together
/// using every field of the grammar.
pub const SHAPE_BASES: &[&str] = &[
    r#"{"id":"t","description":"","theory":"T","extends":["L"],"imports":[{"from":"L","alias":"A","expose":["V"]}],"sorts":[{"name":"S","params":[{"name":"x","sort":"V","implicit":false}],"kind":{"type":"val","value_kind":"int"},"closed":null}],"ops":[{"name":"f","input":null,"inputs":[{"name":"x","sort":"V"}],"output":"V"}],"equations":[{"name":"e","lhs":"f(x)","rhs":"x"}],"directed_equations":[{"name":"d","lhs":"f(x)","rhs":"x","impl_expr":"x","inverse":null,"source_kind":null,"target_kind":null,"coercion_class":"iso"}],"policies":[{"name":"p","value_kind":"int","strategy":{"type":"custom","expr":"x"}}]}"#,
    r#"{"id":"m","description":"","morphism":"F","domain":"L","codomain":"K","sort_map":{"V":"P","E":"Q"},"op_map":{"s":"a"}}"#,
    r#"{"id":"b","description":"","bundle":"B","theories":[{"theory":"A"}],"morphisms":[],"compositions":[{"result":"R","bases":["A"],"steps":[{"left":"A","right":"A","shared_sorts":[],"shared_ops":[],"mode":"amalgamate","identify":[{"kind":"sort","left":"X","right":"Y"}]}]}],"protocols":[{"protocol":"p","schema_theory":"A","instance_theory":{"theory":"I"},"edge_rules":[{"edge_kind":"e","src_kind":"a","tgt_kind":"b"}]}]}"#,
    r#"{"id":"c","description":"","compose":{"result":"R","steps":[]}}"#,
    r#"{"id":"p","description":"","protocol":"p","schema_theory":{"result":"R","steps":[]},"instance_theory":"I"}"#,
    r#"{"id":"k","description":"","class":"Eq","params":["T"],"signatures":[{"name":"eq","inputs":[{"name":"a","sort":"T"}],"output":"T"}],"axioms":[]}"#,
    r#"{"id":"i","description":"","instance":"I","class":"Eq","target":"L","bindings":{"T":"V"}}"#,
    r#"{"id":"n","description":"","inductive":"Nat","params":[],"constructors":[{"name":"zero"},{"name":"succ","inputs":[{"name":"n","sort":"Nat"}]}]}"#,
];

/// The replacement values of the grammar scope, as JSON text; `@` stands
/// for the value replaced.
pub const REPLACEMENTS: &[&str] = &[
    "null",
    "true",
    "1",
    r#""x""#,
    "[]",
    "{}",
    "[@]",
    r#"{"zz":@}"#,
];

/// Serialize a value as compact JSON text.
#[must_use]
pub fn to_text(value: &Value) -> String {
    let string = |s: &str| {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
        out.push('"');
        out
    };
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.clone(),
        Value::String(s) => string(s),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(to_text).collect::<Vec<_>>().join(",")
        ),
        Value::Object(members) => format!(
            "{{{}}}",
            members
                .iter()
                .map(|(k, v)| format!("{}:{}", string(k), to_text(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// The value `root` with the value at `steps` replaced by `f` of it.
fn edit(root: &Value, steps: &[usize], f: &dyn Fn(&Value) -> Option<Value>) -> Option<Value> {
    let Some((first, rest)) = steps.split_first() else {
        return f(root);
    };
    match root {
        Value::Array(items) => {
            let mut items = items.clone();
            match f_nested(&items[*first], rest, f) {
                Some(Change::Replace(v)) => items[*first] = v,
                Some(Change::Remove) => {
                    items.remove(*first);
                }
                None => return None,
            }
            Some(Value::Array(items))
        }
        Value::Object(members) => {
            let mut members = members.clone();
            match f_nested(&members[*first].1, rest, f) {
                Some(Change::Replace(v)) => members[*first].1 = v,
                Some(Change::Remove) => {
                    members.remove(*first);
                }
                None => return None,
            }
            Some(Value::Object(members))
        }
        _ => None,
    }
}

/// What becomes of a child when an edit reaches it.
enum Change {
    /// The child is replaced by this value.
    Replace(Value),
    /// The child is removed.
    Remove,
}

/// The change an edit along `rest` makes to `child`: `f` of it when `rest`
/// is empty, the edited child otherwise; `None` when the edit fails.
fn f_nested(child: &Value, rest: &[usize], f: &dyn Fn(&Value) -> Option<Value>) -> Option<Change> {
    if rest.is_empty() {
        Some(f(child).map_or(Change::Remove, Change::Replace))
    } else {
        edit(child, rest, f).map(Change::Replace)
    }
}

/// Every position of a value, as child indices from the root.
fn positions(value: &Value, prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    out.push(prefix.clone());
    let children: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(members) => members.iter().map(|(_, v)| v).collect(),
        _ => Vec::new(),
    };
    for (index, child) in children.into_iter().enumerate() {
        prefix.push(index);
        positions(child, prefix, out);
        prefix.pop();
    }
}

/// The grammar scope: for each base document and each position in it,
/// every replacement of [`REPLACEMENTS`], then the removal of each object
/// member and the addition of an unknown member `zz` to each object.
///
/// Returns `(replacements, removals, additions)`, the three families in that
/// order.
#[must_use]
pub fn shape_documents() -> (Vec<String>, Vec<String>, Vec<String>) {
    mutations(SHAPE_BASES, REPLACEMENTS)
}

/// The grammar mutations of `bases`.
///
/// For each base document and each position in it, every replacement of
/// `replacements` (JSON text in which `@` stands for the value replaced),
/// then the removal of each object member and the addition of an unknown
/// member `zz` to each object.
///
/// Returns `(replacements, removals, additions)`, the three families in that
/// order. A base that is not JSON text contributes nothing.
#[must_use]
pub fn mutations(bases: &[&str], replacements: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut replaced = Vec::new();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for base in bases {
        let Some(root) = crate::json::parse(base) else {
            continue;
        };
        let mut all = Vec::new();
        positions(&root, &mut Vec::new(), &mut all);
        for steps in &all {
            for replacement in replacements {
                let swap = |v: &Value| crate::json::parse(&replacement.replace('@', &to_text(v)));
                if let Some(doc) = edit(&root, steps, &swap) {
                    replaced.push(to_text(&doc));
                }
            }
            if let Some((last, parent)) = steps.split_last()
                && let Some(Value::Object(_)) = value_at(&root, parent)
                && let Some(doc) = edit(&root, parent, &|v: &Value| match v {
                    Value::Object(members) => {
                        let mut members = members.clone();
                        members.remove(*last);
                        Some(Value::Object(members))
                    }
                    _ => None,
                })
            {
                removed.push(to_text(&doc));
            }
            if let Some(Value::Object(_)) = value_at(&root, steps)
                && let Some(doc) = edit(&root, steps, &|v: &Value| match v {
                    Value::Object(members) => {
                        let mut members = members.clone();
                        members.push(("zz".to_owned(), Value::Null));
                        Some(Value::Object(members))
                    }
                    _ => None,
                })
            {
                added.push(to_text(&doc));
            }
        }
    }
    (replaced, removed, added)
}

fn value_at<'a>(root: &'a Value, steps: &[usize]) -> Option<&'a Value> {
    steps.iter().try_fold(root, |v, i| match v {
        Value::Array(items) => items.get(*i),
        Value::Object(members) => members.get(*i).map(|(_, v)| v),
        _ => None,
    })
}
