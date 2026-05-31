//! The operational layout complement: the per-vertex fibres the emit
//! review (put-direction) consumes.
//!
//! This is the **shared currency** between the two emit worlds:
//!
//! - `panproto-parse` (programming languages): the de-novo emitter and
//!   the reconstruction-from-parse path.
//! - `panproto-io` (data representation formats): the format-preserving
//!   reconstruction path (`UnifiedCodec`).
//!
//! Both are the put-direction of one dependent optic; the only thing
//! that differs is *which fibres are present*. Where a fibre is present
//! the review **replays** it (byte-faithful); where it is absent the
//! review's **canonical section** synthesizes it from `grammar.json`.
//! A transpiled abstract schema carries no fibres at all (every field
//! is `None`/empty), so the canonical section drives the whole emit;
//! a freshly parsed schema carries all three, so the review is a pure
//! replay. That single object subsumes both historical mechanisms.
//!
//! ## The three fibres (cf. [`crate::optic::OpticKind`])
//!
//! The layout complement over a vertex decomposes structurally:
//!
//! 1. **Variant-tag fibre** (Prism / coproduct). Which `CHOICE`
//!    alternative the parser took, recorded as the ordered production
//!    [`trace`](VertexComplement::trace). Each slot is either a
//!    [`TraceSlot::Child`] (a named child → a schema edge, i.e. the
//!    variant tag the review consumes) or a [`TraceSlot::Token`] (an
//!    anonymous grammar literal → layout). [`pre_alias`] refines the
//!    tag when two source rules alias to one surface kind.
//! 2. **Layout fibre** (Lens / product, the dropped component). The
//!    interstitial whitespace between tokens, plus byte span, indent
//!    and blank-line counts. This is the spacing the canonical section
//!    must otherwise synthesize from a `FormatPolicy`.
//! 3. **External-text fibre** (Lens-dropped, *language-specific*).
//!    Scanner-managed token text that is *not* in the grammar
//!    (heredoc bodies, string contents, regex bodies) plus terminal
//!    [`literal`] text and anonymous [`field_tokens`]. This is the
//!    cassette's domain.
//!
//! ## Serialized form
//!
//! The persisted encoding is the per-vertex constraint list on a
//! [`Schema`] (sorts `start-byte`, `interstitial-N`, `ptrace-N`,
//! `chose-alt-*`, `literal-value`, `pre-alias-symbol`, `field:<name>`,
//! `indent`, `blank-lines-before`). [`LayoutComplement::from_schema`]
//! decodes that into typed fibres; [`LayoutComplement::write_into`]
//! re-encodes them. The constraint encoding is what the parse walker
//! writes and what crosses crate and wire boundaries; this type is the
//! typed in-memory view both emitters program against.

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_schema::{Constraint, Schema};
use serde::{Deserialize, Serialize};

/// One slot of the production trace (the variant-tag fibre).
///
/// tree-sitter has already tokenized the source: every `child(i)` of a
/// named node is either a named child (which becomes a schema edge) or
/// an anonymous token with exact text. The ordered interleaving of
/// those is exactly the variant tags (Prism) plus the layout literals
/// (Lens), and it aligns 1:1 with the emitter's cursor edges and the
/// grammar's string literals because hidden rules are inlined by
/// tree-sitter and never surface as their own child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum TraceSlot {
    /// A named child: the parser dispatched into a sub-rule of this
    /// kind. The review consumes one schema edge here.
    Child {
        /// The child's surface kind (a vertex kind on the schema side).
        kind: String,
    },
    /// An anonymous grammar token emitted verbatim: a `STRING`/literal
    /// the grammar fixes. Pure layout; consumes no schema edge.
    Token {
        /// The exact token text.
        text: String,
    },
}

impl TraceSlot {
    /// The serialized one-string form: `C<kind>` for a child,
    /// `T<text>` for a token. Matches the `ptrace-N` constraint value.
    #[must_use]
    pub fn encode(&self) -> String {
        match self {
            Self::Child { kind } => format!("C{kind}"),
            Self::Token { text } => format!("T{text}"),
        }
    }

    /// Decode the `ptrace-N` constraint value back into a slot. A
    /// leading `C` marks a child kind, `T` a token; any other prefix
    /// (or empty) is treated as a token for forward-compatibility.
    #[must_use]
    pub fn decode(value: &str) -> Self {
        match value.as_bytes().first() {
            Some(b'C') => Self::Child {
                kind: value[1..].to_owned(),
            },
            Some(b'T') => Self::Token {
                text: value[1..].to_owned(),
            },
            _ => Self::Token {
                text: value.to_owned(),
            },
        }
    }
}

/// One interstitial gap (the layout fibre): the inter-token text the
/// parser dropped, in declared order, with its source byte offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interstitial {
    /// Position in the parent's gap sequence (`interstitial-N`).
    pub slot: usize,
    /// The exact gap text (whitespace, comments, etc.).
    pub text: String,
    /// Start byte of the gap in the original source, when recorded.
    pub start_byte: Option<usize>,
}

/// The per-vertex layout complement: the three fibres plus the
/// positional witnesses. Every field is optional/empty so an abstract
/// (transpiled) vertex is simply [`VertexComplement::default`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexComplement {
    // ── Variant-tag fibre (Prism) ────────────────────────────────────
    /// The ordered production trace. `None` when the vertex was not
    /// produced by a parse (a by-construction / transpiled vertex):
    /// the review must then synthesize the variant via grammar
    /// unification rather than replay.
    pub trace: Option<Vec<TraceSlot>>,
    /// Pre-alias source symbol, refining a same-kind-aliased child so
    /// the variant review is exact (`pre-alias-symbol`).
    pub pre_alias: Option<String>,
    /// Lossy legacy projections of the variant-tag fibre kept for the
    /// transition: the trimmed literal fingerprint and the named-child
    /// kind sequence. The canonical witness is [`trace`]; these are
    /// retired once the review reads the trace everywhere.
    pub chose_alt_fingerprint: Option<String>,
    /// Named-child kind sequence (`chose-alt-child-kinds`).
    pub chose_alt_child_kinds: Option<String>,

    // ── Layout fibre (Lens) ──────────────────────────────────────────
    /// Inter-token gaps in declared order.
    pub interstitials: Vec<Interstitial>,
    /// `(start_byte, end_byte)` span of the vertex in the original
    /// source, when parsed.
    pub byte_span: Option<(usize, usize)>,
    /// Leading indentation whitespace of the vertex's line.
    pub indent: Option<String>,
    /// Blank lines immediately preceding the vertex.
    pub blank_lines_before: Option<usize>,

    // ── External-text fibre (cassette / value) ───────────────────────
    /// Terminal text for a leaf vertex (`literal-value`): the literal
    /// bytes an Iso/const optic carries.
    pub literal: Option<String>,
    /// Anonymous field-token text keyed by field name (`field:<name>`).
    pub field_tokens: BTreeMap<String, String>,
}

impl VertexComplement {
    /// True when this vertex carries no parse-recorded fibre at all —
    /// i.e. it is a by-construction / transpiled vertex the review must
    /// emit through the canonical section, not replay.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Decode one vertex's constraint list into typed fibres.
    #[must_use]
    pub fn from_constraints(constraints: &[Constraint]) -> Self {
        let mut out = Self::default();
        let mut trace_slots: BTreeMap<usize, TraceSlot> = BTreeMap::new();
        let mut inter: BTreeMap<usize, (String, Option<usize>)> = BTreeMap::new();
        let mut start_byte: Option<usize> = None;
        let mut end_byte: Option<usize> = None;

        for c in constraints {
            let sort = c.sort.as_ref();
            if let Some(n) = sort.strip_prefix("ptrace-") {
                if let Ok(idx) = n.parse::<usize>() {
                    trace_slots.insert(idx, TraceSlot::decode(&c.value));
                }
            } else if let Some(rest) = sort.strip_prefix("interstitial-") {
                if let Some(n) = rest.strip_suffix("-start-byte") {
                    if let (Ok(idx), Ok(b)) = (n.parse::<usize>(), c.value.parse::<usize>()) {
                        inter.entry(idx).or_default().1 = Some(b);
                    }
                } else if let Ok(idx) = rest.parse::<usize>() {
                    inter.entry(idx).or_default().0.clone_from(&c.value);
                }
            } else if let Some(field) = sort.strip_prefix("field:") {
                out.field_tokens.insert(field.to_owned(), c.value.clone());
            } else {
                match sort {
                    "start-byte" => start_byte = c.value.parse().ok(),
                    "end-byte" => end_byte = c.value.parse().ok(),
                    "pre-alias-symbol" => out.pre_alias = Some(c.value.clone()),
                    "chose-alt-fingerprint" => out.chose_alt_fingerprint = Some(c.value.clone()),
                    "chose-alt-child-kinds" => out.chose_alt_child_kinds = Some(c.value.clone()),
                    "literal-value" => out.literal = Some(c.value.clone()),
                    "indent" => out.indent = Some(c.value.clone()),
                    "blank-lines-before" => out.blank_lines_before = c.value.parse().ok(),
                    _ => {}
                }
            }
        }

        if !trace_slots.is_empty() {
            out.trace = Some(trace_slots.into_values().collect());
        }
        out.interstitials = inter
            .into_iter()
            .map(|(slot, (text, start_byte))| Interstitial {
                slot,
                text,
                start_byte,
            })
            .collect();
        out.byte_span = match (start_byte, end_byte) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        };
        out
    }

    /// Re-encode the typed fibres into a constraint list. The inverse of
    /// [`from_constraints`](Self::from_constraints) up to constraint
    /// ordering (callers compare as a set/multiset, never by position).
    #[must_use]
    pub fn to_constraints(&self) -> Vec<Constraint> {
        let mut out = Vec::new();
        let push = |out: &mut Vec<Constraint>, sort: String, value: String| {
            out.push(Constraint {
                sort: Name::from(sort),
                value,
            });
        };
        if let Some((s, e)) = self.byte_span {
            push(&mut out, "start-byte".into(), s.to_string());
            push(&mut out, "end-byte".into(), e.to_string());
        }
        if let Some(p) = &self.pre_alias {
            push(&mut out, "pre-alias-symbol".into(), p.clone());
        }
        if let Some(l) = &self.literal {
            push(&mut out, "literal-value".into(), l.clone());
        }
        for (field, text) in &self.field_tokens {
            push(&mut out, format!("field:{field}"), text.clone());
        }
        for itl in &self.interstitials {
            push(&mut out, format!("interstitial-{}", itl.slot), itl.text.clone());
            if let Some(b) = itl.start_byte {
                push(
                    &mut out,
                    format!("interstitial-{}-start-byte", itl.slot),
                    b.to_string(),
                );
            }
        }
        if let Some(fp) = &self.chose_alt_fingerprint {
            push(&mut out, "chose-alt-fingerprint".into(), fp.clone());
        }
        if let Some(k) = &self.chose_alt_child_kinds {
            push(&mut out, "chose-alt-child-kinds".into(), k.clone());
        }
        if let Some(t) = &self.trace {
            for (i, slot) in t.iter().enumerate() {
                push(&mut out, format!("ptrace-{i}"), slot.encode());
            }
        }
        if let Some(ind) = &self.indent {
            push(&mut out, "indent".into(), ind.clone());
        }
        if let Some(n) = self.blank_lines_before {
            push(&mut out, "blank-lines-before".into(), n.to_string());
        }
        out
    }
}

/// The whole-schema layout complement.
///
/// A typed view of every vertex's parse-recorded fibres. Vertices
/// absent from the map (or mapping to an empty [`VertexComplement`])
/// are emitted through the canonical section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutComplement {
    /// Per-vertex fibres, keyed by vertex id.
    pub vertices: BTreeMap<Name, VertexComplement>,
}

impl LayoutComplement {
    /// Decode a schema's per-vertex constraints into the typed
    /// complement. Vertices whose constraints decode to an empty
    /// complement are omitted.
    #[must_use]
    pub fn from_schema(schema: &Schema) -> Self {
        let mut vertices = BTreeMap::new();
        for (id, constraints) in &schema.constraints {
            let vc = VertexComplement::from_constraints(constraints);
            if !vc.is_empty() {
                vertices.insert(id.clone(), vc);
            }
        }
        Self { vertices }
    }

    /// The typed fibres for one vertex, or `None` (canonical section).
    #[must_use]
    pub fn vertex(&self, id: &Name) -> Option<&VertexComplement> {
        self.vertices.get(id)
    }

    /// Re-encode every vertex's fibres into the schema's constraint map,
    /// replacing any existing constraints for those vertices.
    pub fn write_into(&self, schema: &mut Schema) {
        for (id, vc) in &self.vertices {
            schema.constraints.insert(id.clone(), vc.to_constraints());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn trace_slot_encode_decode_round_trip() {
        let child = TraceSlot::Child {
            kind: "binary_expression".into(),
        };
        let token = TraceSlot::Token { text: "+".into() };
        assert_eq!(child.encode(), "Cbinary_expression");
        assert_eq!(token.encode(), "T+");
        assert_eq!(TraceSlot::decode(&child.encode()), child);
        assert_eq!(TraceSlot::decode(&token.encode()), token);
        // a token whose text begins with C/T survives the prefix tag
        let tricky = TraceSlot::Token { text: "Cat".into() };
        assert_eq!(TraceSlot::decode(&tricky.encode()), tricky);
    }

    fn c(sort: &str, value: &str) -> Constraint {
        Constraint {
            sort: Name::from(sort),
            value: value.to_owned(),
        }
    }

    #[test]
    fn from_constraints_decodes_all_fibres() {
        let cs = vec![
            c("start-byte", "0"),
            c("end-byte", "7"),
            c("ptrace-0", "Cnumber"),
            c("ptrace-1", "T+"),
            c("ptrace-2", "Cnumber"),
            c("interstitial-0", " "),
            c("interstitial-0-start-byte", "1"),
            c("pre-alias-symbol", "command_binary"),
            c("literal-value", "42"),
            c("chose-alt-child-kinds", "number number"),
            c("field:operator", "+"),
            c("indent", "    "),
            c("blank-lines-before", "2"),
        ];
        let vc = VertexComplement::from_constraints(&cs);
        assert_eq!(
            vc.trace,
            Some(vec![
                TraceSlot::Child { kind: "number".into() },
                TraceSlot::Token { text: "+".into() },
                TraceSlot::Child { kind: "number".into() },
            ])
        );
        assert_eq!(vc.byte_span, Some((0, 7)));
        assert_eq!(vc.pre_alias.as_deref(), Some("command_binary"));
        assert_eq!(vc.literal.as_deref(), Some("42"));
        assert_eq!(vc.interstitials.len(), 1);
        assert_eq!(vc.interstitials[0].text, " ");
        assert_eq!(vc.interstitials[0].start_byte, Some(1));
        assert_eq!(vc.field_tokens.get("operator").map(String::as_str), Some("+"));
        assert_eq!(vc.indent.as_deref(), Some("    "));
        assert_eq!(vc.blank_lines_before, Some(2));
        assert!(!vc.is_empty());
    }

    #[test]
    fn constraints_round_trip_as_a_set() {
        let cs = vec![
            c("start-byte", "10"),
            c("end-byte", "20"),
            c("ptrace-0", "Cidentifier"),
            c("ptrace-1", "T="),
            c("interstitial-0", "\n  "),
            c("interstitial-0-start-byte", "11"),
            c("literal-value", "x"),
            c("field:op", "="),
            c("indent", "  "),
            c("blank-lines-before", "1"),
            c("chose-alt-fingerprint", "="),
        ];
        let vc = VertexComplement::from_constraints(&cs);
        let back = vc.to_constraints();
        // re-decoding the re-encoded constraints is a fixed point
        assert_eq!(VertexComplement::from_constraints(&back), vc);
        // and every original constraint survives (set equality)
        let orig: std::collections::HashSet<(String, String)> = cs
            .iter()
            .map(|c| (c.sort.as_ref().to_owned(), c.value.clone()))
            .collect();
        let round: std::collections::HashSet<(String, String)> = back
            .iter()
            .map(|c| (c.sort.as_ref().to_owned(), c.value.clone()))
            .collect();
        assert_eq!(orig, round);
    }

    #[test]
    fn empty_constraints_decode_to_empty() {
        assert!(VertexComplement::from_constraints(&[]).is_empty());
        assert!(VertexComplement::default().is_empty());
        // a vertex carrying only an unrelated constraint is still empty
        // w.r.t. the layout fibres
        assert!(VertexComplement::from_constraints(&[c("maxLength", "3")]).is_empty());
    }

    fn schema_with(constraints: &[(&str, &[Constraint])]) -> Schema {
        use panproto_schema::{Protocol, SchemaBuilder};
        let mut b = SchemaBuilder::new(&Protocol::default());
        for (vid, cs) in constraints {
            b = b.vertex(vid, "object", None).unwrap();
            for cn in *cs {
                b = b.constraint(vid, cn.sort.as_ref(), &cn.value);
            }
        }
        b.build().unwrap()
    }

    #[test]
    fn schema_level_from_and_write() {
        let schema = schema_with(&[
            ("v1", &[c("ptrace-0", "Cfoo"), c("literal-value", "bar")]),
            ("v2", &[c("maxLength", "9")]),
        ]);
        let lc = LayoutComplement::from_schema(&schema);
        // v1 has layout fibres; v2 has none → omitted
        assert!(lc.vertex(&Name::from("v1")).is_some());
        assert!(lc.vertex(&Name::from("v2")).is_none());

        let mut fresh = schema_with(&[("v1", &[])]);
        lc.write_into(&mut fresh);
        let lc2 = LayoutComplement::from_schema(&fresh);
        assert_eq!(lc, lc2);
    }
}
