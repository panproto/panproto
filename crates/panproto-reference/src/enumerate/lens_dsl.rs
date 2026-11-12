//! Complete finite scopes for the lens-dsl page.
//!
//! Every scope is a cartesian product of short axes, so its size is the
//! product of the axis lengths (a sequence axis of length at most `n` over
//! an alphabet of `k` letters has `1 + k + ... + k^n` points). Documents are
//! JSON text whose identifier is `top`; [`library`] holds the documents a
//! composition's references name, and the body vertex is [`BODY`].

use crate::json::Value;

/// The body vertex every scope compiles at.
pub const BODY: &str = "body";

/// The identifier of every enumerated document.
pub const TOP: &str = "top";

/// Wrap a body (JSON members after the header) into a document.
#[must_use]
pub fn document(body: &str) -> String {
    format!(r#"{{"id":"{TOP}","source":"s","target":"t",{body}}}"#)
}

/// Every sequence of at most `max` letters of `alphabet`, shortest first,
/// each joined with commas.
#[must_use]
pub fn sequences(alphabet: &[&str], max: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut layer = vec![Vec::<&str>::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for prefix in &layer {
            for letter in alphabet {
                let mut longer = prefix.clone();
                longer.push(letter);
                all.push(longer.join(","));
                next.push(longer);
            }
        }
        layer = next;
    }
    all
}

/// The number of sequences of at most `max` letters over `k` letters.
#[must_use]
pub fn sequence_count(k: usize, max: u32) -> usize {
    (0..=max).map(|n| k.pow(n)).sum()
}

/// The step alphabet: every step kind, with documents that compile and
/// documents refused at each place a step can be refused.
pub const STEPS: &[&str] = &[
    r#"{"remove_field":"a"}"#,
    r#"{"rename_field":{"old":"a","new":"b"}}"#,
    r#"{"add_field":{"name":"c","kind":"integer","default":0}}"#,
    r#"{"add_field":{"name":"c","kind":"string","default":"x","expr":"a"}}"#,
    r#"{"add_field":{"name":"c","kind":"integer","default":"x"}}"#,
    r#"{"add_field":{"name":"c","kind":"array","default":[1,{"k":true,"k":-0},2.5,-9223372036854775808]}}"#,
    r#"{"add_field":{"name":"c","kind":"integer","default":18446744073709551615}}"#,
    r#"{"add_field":{"name":"c","kind":"integer","expr":"(("}}"#,
    r#"{"apply_expr":{"field":"a","expr":"add a 1","inverse":"sub a 1","coercion":"iso"}}"#,
    r#"{"apply_expr":{"field":"a","expr":"(("}}"#,
    r#"{"compute_field":{"target":"d","expr":"a"}}"#,
    r#"{"compute_field":{"target":"d","expr":"a","inverse":"(("}}"#,
    r#"{"hoist_field":{"parent":"body","intermediate":"m","child":"c"}}"#,
    r#"{"nest_field":{"parent":"body","child":"body.a","intermediate":"m","intermediate_kind":"object","edge_kind":"prop"}}"#,
    r#"{"nest_field":{"parent":"body","child":"body.a","intermediate":"m","intermediate_kind":"object","edge_kind":"prop","old_edge_name":"a","parent_to_intermediate":"mm","intermediate_to_child":"aa"}}"#,
    r#"{"scoped":{"focus":"item","inner":[{"rename_field":{"old":"a","new":"b"}},{"add_field":{"name":"c","kind":"integer","default":1}}]}}"#,
    r#"{"scoped":{"focus":"item","inner":[{"apply_expr":{"field":"a","expr":"(("}}]}}"#,
    r#"{"scoped":{"focus":"item","inner":[]}}"#,
    r#"{"scoped":{"focus":"item","inner":[{"rename_op":{"old":"f","new":"g"}}]}}"#,
    r#"{"scoped":{"focus":"item","inner":[{"compute_field":{"target":"d","expr":"a"}},{"scoped":{"focus":"leaf","inner":[{"rename_sort":{"old":"s","new":"t"}}]}}]}}"#,
    r#"{"pullback":{"name":"F","domain":"A","codomain":"B","sort_map":{"S":"T"},"op_map":{"f":"g"}}}"#,
    r#"{"coerce_sort":{"sort":"flag","source_kind":"boolean","target_kind":"boolean","expr":"not v","inverse":"not v","coercion":"iso"}}"#,
    r#"{"coerce_sort":{"sort":"name","source_kind":"string","target_kind":"string","expr":"upper v","inverse":"v","coercion":"iso"}}"#,
    r#"{"coerce_sort":{"sort":"n","target_kind":"string","expr":"((","coercion":"projection"}}"#,
    r#"{"merge_sorts":{"sort_a":"a","sort_b":"b","merged":"ab","expr":"a"}}"#,
    r#"{"add_sort":{"name":"s","kind":"boolean","default":false}}"#,
    r#"{"add_sort":{"name":"s","kind":"boolean","default":1}}"#,
    r#"{"drop_sort":"s"}"#,
    r#"{"rename_sort":{"old":"s","new":"t"}}"#,
    r#"{"add_op":{"name":"f","src":"s","tgt":"t","kind":"prop"}}"#,
    r#"{"drop_op":"f"}"#,
    r#"{"rename_op":{"old":"f","new":"g"}}"#,
    r#"{"add_equation":{"name":"e","lhs":"f(x, g( y ))","rhs":"x"}}"#,
    r#"{"add_equation":{"name":"e","lhs":"f(x,)","rhs":"x"}}"#,
    r#"{"add_equation":{"name":"e","lhs":"x","rhs":"a b"}}"#,
    r#"{"drop_equation":"e"}"#,
];

/// The `invertible` axis: absent, or `true`.
pub const INVERTIBLE: &[&str] = &["", r#","invertible":true"#];

/// The `directed_equations` axis: absent, an honest lossless equation, an
/// honest lossy one, one whose left side is not a term, and a dishonest one.
pub const DIRECTED: &[&str] = &[
    "",
    r#","directed_equations":[{"name":"neg","lhs":"x","rhs":"negate(x)","impl":"not x","inverse":"not x","source_kind":"boolean","target_kind":"boolean","coercion":"iso"}]"#,
    r#","directed_equations":[{"name":"str","lhs":"n","rhs":"s","impl":"int_to_str n","source_kind":"integer","coercion":"projection"}]"#,
    r#","directed_equations":[{"name":"bad","lhs":"f(","rhs":"x","impl":"x"}]"#,
    r#","directed_equations":[{"name":"up","lhs":"x","rhs":"y","impl":"upper x","inverse":"x","source_kind":"string","coercion":"iso"}]"#,
];

/// The steps scope: every step list of at most two steps of [`STEPS`], with
/// each `invertible` and each `directed_equations` value.
#[must_use]
pub fn step_documents() -> Vec<String> {
    let mut out = Vec::new();
    for steps in sequences(STEPS, 2) {
        for invertible in INVERTIBLE {
            for directed in DIRECTED {
                out.push(document(&format!(
                    r#""steps":[{steps}]{invertible}{directed}"#
                )));
            }
        }
    }
    out
}

/// The rule alphabet.
pub const RULES: &[&str] = &[
    r#"{"pattern":{"name":"h"}}"#,
    r#"{"pattern":{"name":"h"},"replace":null}"#,
    r#"{"pattern":{"name":"h"},"replace":{"name":"header"}}"#,
    r#"{"pattern":{"name":"h"},"replace":{"name":"h"}}"#,
    r#"{"pattern":{"name":"h"},"replace":{"name":{"template":"h{level}x{ a{b }"}}}"#,
    r#"{"pattern":{"name":"h"},"replace":{"name":{"template":"h{level"}}}"#,
    r#"{"pattern":{"name":"p"},"replace":{"rename_attrs":{"b":"bb","a":"aa"},"drop_attrs":["c","d"],"keep_attrs":["k"]}}"#,
    r#"{"pattern":{"name":"p"},"replace":{"add_attrs":{"z":1,"y":1.5,"x":true,"w":null,"v":"s","u":[1],"t":{"k":2},"s":9223372036854775808e0}}}"#,
    r#"{"pattern":{"name":"p"},"replace":{"add_attrs":{"big":18446744073709551615}}}"#,
    r#"{"pattern":{"name":"p"},"replace":{"map_attr_value":{"a":{"op":"add","value":2},"b":{"op":"multiply","value":1.5},"c":{"op":"prefix","value":"x\"y"},"d":{"op":"suffix","value":"y"},"e":{"op":"negate"},"f":{"op":"to-string"},"g":{"op":"to-number"},"h":{"op":"to-boolean"},"i":{"op":"subtract","value":9007199254740993}}}}"#,
    r#"{"pattern":{"name":"q"},"replace":{"name":{"template":""},"keep_attrs":[]}}"#,
    r#"{"pattern":{"name":"q"},"replace":{"name":{"template":"{ }"}}}"#,
];

/// The `passthrough` axis: absent, `keep`, `drop`.
pub const PASSTHROUGH: &[&str] = &["", r#","passthrough":"keep""#, r#","passthrough":"drop""#];

/// The rules scope: every rule list of at most two rules of [`RULES`], with
/// each `passthrough` and each `invertible` value.
#[must_use]
pub fn rule_documents() -> Vec<String> {
    let mut out = Vec::new();
    for rules in sequences(RULES, 2) {
        for passthrough in PASSTHROUGH {
            for invertible in INVERTIBLE {
                out.push(document(&format!(
                    r#""rules":[{rules}]{passthrough}{invertible}"#
                )));
            }
        }
    }
    out
}

/// The documents a composition's references name, as `(id, JSON text)`.
pub const LIBRARY: &[(&str, &str)] = &[
    (
        "steps.doc",
        r#"{"id":"steps.doc","source":"s","target":"t","steps":[{"remove_field":"b"},{"compute_field":{"target":"d","expr":"a"}}]}"#,
    ),
    (
        "horizontal.doc",
        r#"{"id":"horizontal.doc","source":"s","target":"t","compose":{"mode":"horizontal","lenses":[{"inline":{"steps":[{"rename_sort":{"old":"s","new":"t"}}]}},{"inline":{"steps":[{"add_field":{"name":"c","kind":"integer","default":1}}]}}]}}"#,
    ),
    (
        "after.doc",
        r#"{"id":"after.doc","source":"s","target":"t","compose":{"mode":"vertical","lenses":[{"inline":{"steps":[{"remove_field":"a"}]}},{"ref":"horizontal.doc"}]}}"#,
    ),
    (
        "single.doc",
        r#"{"id":"single.doc","source":"s","target":"t","compose":{"mode":"horizontal","lenses":[{"inline":{"steps":[{"rename_sort":{"old":"s","new":"t"}},{"rename_op":{"old":"f","new":"g"}}]}}]}}"#,
    ),
    (
        "symmetric.doc",
        r#"{"id":"symmetric.doc","source":"s","target":"t","symmetric":{"left":[{"rename_field":{"old":"a","new":"b"}}],"right":[{"remove_field":"a"}]}}"#,
    ),
    (
        "refused.doc",
        r#"{"id":"refused.doc","source":"s","target":"t","steps":[{"apply_expr":{"field":"a","expr":"(("}}]}"#,
    ),
    (
        "cycle.a",
        r#"{"id":"cycle.a","source":"s","target":"t","compose":{"mode":"vertical","lenses":[{"ref":"cycle.b"}]}}"#,
    ),
    (
        "cycle.b",
        r#"{"id":"cycle.b","source":"s","target":"t","compose":{"mode":"vertical","lenses":[{"ref":"cycle.a"}]}}"#,
    ),
    (
        "auto.doc",
        r#"{"id":"auto.doc","source":"s","target":"t","auto":{}}"#,
    ),
    (
        "directed.doc",
        r#"{"id":"directed.doc","source":"s","target":"t","steps":[{"rename_op":{"old":"f","new":"g"}}],"directed_equations":[{"name":"str","lhs":"n","rhs":"s","impl":"int_to_str n","source_kind":"integer","coercion":"projection"}]}"#,
    ),
    (
        "ungrammatical.doc",
        r#"{"id":"ungrammatical.doc","source":"s","target":"t","steps":[{"nope":1}]}"#,
    ),
];

/// The library document with identifier `id`, read as a value.
#[must_use]
pub fn library(id: &str) -> Option<Value> {
    LIBRARY
        .iter()
        .find(|(name, _)| *name == id)
        .and_then(|(_, text)| crate::json::parse(text))
}

/// The part alphabet of the composition scope: inline step lists and
/// references to every library document but the ungrammatical one, to a
/// missing document and to the composing document itself.
pub const PARTS: &[&str] = &[
    r#"{"inline":{"steps":[]}}"#,
    r#"{"inline":{"steps":[{"remove_field":"a"}]}}"#,
    r#"{"inline":{"steps":[{"rename_sort":{"old":"s","new":"t"}},{"apply_expr":{"field":"a","expr":"add a 1","inverse":"sub a 1","coercion":"iso"}}]}}"#,
    r#"{"inline":{"steps":[{"apply_expr":{"field":"a","expr":"(("}}]}}"#,
    r#"{"inline":{"steps":[{"compute_field":{"target":"d","expr":"a"}}]}}"#,
    r#"{"ref":"steps.doc"}"#,
    r#"{"ref":"horizontal.doc"}"#,
    r#"{"ref":"after.doc"}"#,
    r#"{"ref":"single.doc"}"#,
    r#"{"ref":"symmetric.doc"}"#,
    r#"{"ref":"refused.doc"}"#,
    r#"{"ref":"cycle.a"}"#,
    r#"{"ref":"auto.doc"}"#,
    r#"{"ref":"directed.doc"}"#,
    r#"{"ref":"missing.doc"}"#,
    r#"{"ref":"top"}"#,
];

/// The composition modes.
pub const MODES: &[&str] = &["vertical", "horizontal"];

/// The composition scope: every part list of at most three parts of
/// [`PARTS`], in each mode, with each `invertible` value.
#[must_use]
pub fn compose_documents() -> Vec<String> {
    let mut out = Vec::new();
    for parts in sequences(PARTS, 3) {
        for mode in MODES {
            for invertible in INVERTIBLE {
                out.push(document(&format!(
                    r#""compose":{{"mode":"{mode}","lenses":[{parts}]}}{invertible}"#
                )));
            }
        }
    }
    out
}

/// The leg alphabet of the symmetric scope, each a whole step list.
pub const LEGS: &[&str] = &[
    "[]",
    r#"[{"rename_field":{"old":"a","new":"b"}}]"#,
    r#"[{"remove_field":"a"},{"rename_sort":{"old":"s","new":"t"}}]"#,
    r#"[{"apply_expr":{"field":"a","expr":"add a 1"}}]"#,
    r#"[{"scoped":{"focus":"item","inner":[{"compute_field":{"target":"d","expr":"a"}}]}}]"#,
    r#"[{"scoped":{"focus":"item","inner":[{"rename_op":{"old":"f","new":"g"}}]}}]"#,
    r#"[{"add_field":{"name":"c","kind":"string","default":"x","expr":"a"}}]"#,
    r#"[{"apply_expr":{"field":"a","expr":"(("}}]"#,
];

/// The `focus` axis of the symmetric scope: absent, empty, a vertex.
pub const FOCUS: &[&str] = &["", r#","focus":"""#, r#","focus":"item""#];

/// The symmetric scope: every pair of legs of [`LEGS`], with each focus
/// and each `invertible` value.
#[must_use]
pub fn symmetric_documents() -> Vec<String> {
    let mut out = Vec::new();
    for left in LEGS {
        for right in LEGS {
            for focus in FOCUS {
                for invertible in INVERTIBLE {
                    out.push(document(&format!(
                        r#""symmetric":{{"left":{left},"right":{right}{focus}}}{invertible}"#
                    )));
                }
            }
        }
    }
    out
}

/// The base documents the grammar scope mutates: one per body, together
/// using every member of the grammar.
pub const SHAPE_BASES: &[&str] = &[
    r#"{"id":"g","description":"","source":"s","target":"t","steps":[{"remove_field":"a"},{"rename_field":{"old":"a","new":"b"}},{"add_field":{"name":"c","kind":"integer","default":0,"expr":"a"}},{"apply_expr":{"field":"a","expr":"a","inverse":"a","coercion":"iso"}},{"compute_field":{"target":"d","expr":"a","inverse":"a","coercion":"projection"}},{"hoist_field":{"parent":"p","intermediate":"m","child":"c"}},{"nest_field":{"parent":"p","child":"c","intermediate":"m","intermediate_kind":"object","edge_kind":"prop","old_edge_name":"c","parent_to_intermediate":"m","intermediate_to_child":"c"}},{"scoped":{"focus":"f","inner":[{"drop_sort":"x"}]}},{"pullback":{"name":"F","domain":"A","codomain":"B","sort_map":{"S":"T"},"op_map":{"f":"g"}}},{"coerce_sort":{"sort":"s","source_kind":"int","target_kind":"string","expr":"v","inverse":"v","coercion":"retraction"}},{"merge_sorts":{"sort_a":"a","sort_b":"b","merged":"c","expr":"a"}},{"add_sort":{"name":"s","kind":"boolean","default":false}},{"drop_sort":"s"},{"rename_sort":{"old":"s","new":"t"}},{"add_op":{"name":"f","src":"s","tgt":"t","kind":"prop"}},{"drop_op":"f"},{"rename_op":{"old":"f","new":"g"}},{"add_equation":{"name":"e","lhs":"x","rhs":"x"}},{"drop_equation":"e"}],"directed_equations":[{"name":"d","lhs":"x","rhs":"x","impl":"x","inverse":"x","source_kind":"int","target_kind":"int","coercion":"iso"}],"invertible":false,"extensions":{"k":1}}"#,
    r#"{"id":"g","description":"","source":"s","target":"t","rules":[{"pattern":{"name":"h"},"replace":{"name":{"template":"h{l}"},"rename_attrs":{"a":"b"},"add_attrs":{"c":1},"drop_attrs":["d"],"keep_attrs":["e"],"map_attr_value":{"f":{"op":"add","value":1},"g":{"op":"negate"}}}},{"pattern":{"name":"x"},"replace":null}],"passthrough":"keep","directed_equations":[],"invertible":false,"extensions":{}}"#,
    r#"{"id":"g","description":"","source":"s","target":"t","compose":{"mode":"vertical","lenses":[{"ref":"r"},{"inline":{"steps":[{"drop_op":"f"}]}}]},"directed_equations":[],"invertible":true,"extensions":{}}"#,
    r#"{"id":"g","description":"","source":"s","target":"t","auto":{"quality_threshold":0.5,"enable_overlap":true,"max_results":3,"hints":{"anchors":{"a":"b"},"constraints":[{"type":"scope","under":"u","targets":"t"},{"type":"exclude_targets","vertices":["v"]},{"type":"exclude_sources","vertices":["w"]},{"type":"prefer","predicate":{"kind":"similar_name","threshold":0.5},"weight":1}],"stringency":"strict","alias_clusters":[["a","b"]]}},"directed_equations":[],"invertible":false,"extensions":{}}"#,
    r#"{"id":"g","description":"","source":"s","target":"t","from_diff":{},"invertible":false}"#,
    r#"{"id":"g","description":"","source":"s","target":"t","symmetric":{"left":[{"drop_sort":"a"}],"right":[{"rename_sort":{"old":"a","new":"b"}}],"focus":"f"},"invertible":false,"extensions":{}}"#,
];

/// The replacement values of the grammar scope, as JSON text; `@` stands
/// for the value replaced.
pub const REPLACEMENTS: &[&str] = &[
    "null",
    "true",
    "1",
    "1.5",
    r#""x""#,
    "[]",
    "{}",
    "[@]",
    r#"{"zz":@}"#,
];

/// The grammar scope: [`super::theory_dsl::mutations`] of [`SHAPE_BASES`]
/// by [`REPLACEMENTS`].
#[must_use]
pub fn shape_documents() -> (Vec<String>, Vec<String>, Vec<String>) {
    super::theory_dsl::mutations(SHAPE_BASES, REPLACEMENTS)
}
