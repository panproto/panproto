//! Executable reference semantics of panproto.
//!
//! This crate holds deliberately naive definitions of panproto's
//! operations, written straight from their definitions with exact
//! arithmetic (naturals and integers as digit vectors in base `10^9`,
//! rationals as reduced pairs) and no dependency on any panproto crate or
//! any external crate. Production code is checked against it: a test in a
//! production crate converts its inputs and outputs to the models here and
//! runs [`harness::agree`] over an exhaustive scope from [`enumerate`].
//! The crate is test-only and never published.
//!
//! Each definition lives on a page of the book's correctness part
//! (`book/src/correctness/`); the page named beside each operation below
//! states it as a law block with its exact domain, and each function's
//! documentation quotes the definition it implements. Where a function
//! covers only part of an operation's domain, its documentation states the
//! part, and every input outside it gives `None` rather than an answer.
//!
//! | Operation | Function | Page |
//! |---|---|---|
//! | Delta on finite attributed presheaves | [`rel::delta`] | `kan-extensions.md` |
//! | Sigma, by the quotient of the disjoint union | [`rel::sigma`] | `kan-extensions.md` |
//! | Pi, by enumerating compatible families | [`rel::pi`] | `kan-extensions.md` |
//! | The migration's entity functor | [`rel::entity_functor`] | `kan-extensions.md` |
//! | Relation-ified schemas (classification) | [`rel::classify`] | `kan-extensions.md` |
//! | Homomorphisms and structure-preserving bijections of instances | [`rel::homs`], [`rel::row_bijection`] | `kan-extensions.md` |
//! | Graph Delta, Sigma and Pi (encode, presheaf operation, decode) | [`encode::g_delta`], [`encode::g_sigma`], [`encode::g_pi`] | `kan-extensions.md` |
//! | Tree Delta, Sigma and Pi through the graph form | [`encode::t_delta`], [`encode::t_sigma`], [`encode::t_pi`] | `kan-extensions.md` |
//! | Graph encoding and record-free decoding | [`encode::encode_graph`], [`encode::decode_table`] | `kan-extensions.md` |
//! | Tree and table bridges to graphs | [`encode::tree_to_graph`], [`encode::graph_to_tree`], [`encode::table_to_graph`], [`encode::graph_to_table`] | `representation.md` |
//! | The edge image | [`restrict::edge_image`] | `restrict-and-contraction.md` |
//! | Pair resolution | [`restrict::resolve_pair`] | `restrict-and-contraction.md` |
//! | The contraction plan | [`restrict::contraction_plan`] | `restrict-and-contraction.md` |
//! | Restrict (Delta, then contraction, then fans) | [`restrict::get`] | `restrict-and-contraction.md` |
//! | The fan domain | [`fan::violations_at`] | `representation.md` |
//! | Get and put of the graph lens | [`restrict::get`], [`lens::GraphLens::put`] | `lenses.md` |
//! | The graph put domain | [`lens::GraphLens::check_put_domain`] | `lenses.md` |
//! | Get and put of the table lens | [`table::get`], [`table::TableLens::put`] | `lenses.md` |
//! | The table plan and cell writers | [`table::compile_for_tables`], [`table::TablePlan::writer`] | `lenses.md` |
//! | Get and put of the tree lens | [`lens::tree_get`], [`lens::tree_put`] | `lenses.md` |
//! | Get and put of the resolution lens | [`resolution::get`], [`resolution::put`] | `resolution.md` |
//! | Lens-path composition (the free category; no fusion) | [`path::LensPath::then`], [`path::get_path`], [`path::put_path`] | `lenses.md` |
//! | The edit algebra: invert, compose | [`edit::Delta::invert`], [`edit::Delta::compose`] | `edits.md` |
//! | Edit application and the differ | [`edit::apply`], [`edit::diff`], [`edit::apply_table`] | `edits.md` |
//! | Independence and merge | [`edit::independence`], [`edit::is_benign`], [`edit::merge`] | `edits.md` |
//! | Edit translation (apply then get against translate then apply) | [`edit::get_edit`], [`edit::put_edit`] | `edits.md` |
//! | Validation as predicates over instances | [`validation::validate`] | `validation.md` |
//! | Status aggregation | [`validation::status`], [`validation::CheckStatus::all`] | `validation.md` |
//! | Admission policies | [`validation::admits`] | `validation.md` |
//! | Key values, scopes and buckets | [`keys::key_value`], [`keys::buckets`] | `keys-and-registration.md` |
//! | Segment execution as whole-state execution on the glued state | [`segment::glue`], [`segment::segmented`], [`segment::partition`] | `segments.md` |
//! | Allocation within a range | [`segment::fresh`] | `segments.md` |
//! | Signature type checking | [`theory::typecheck_signature`] | `theories.md` |
//! | Theory colimits | [`theory::compose`] | `theories.md` |
//! | `L_b`: round to nearest, ties to even, at widths 16, 32 and 64 | [`float::round_to`] | `values.md` |
//! | `C_b`: closest-shortest decimal, ties to even | [`float::shortest`] | `values.md` |
//! | NaN narrowing and widening on the bits | [`float::narrow_bits`], [`float::widen_bits`] | `values.md` |
//! | Coercion of a lexeme under a declared kind | [`coerce::coerce`] | `values.md` |
//! | Coercion of a value | [`coerce::coerce_value`] | `values.md` |
//! | Exact decimals and integers, the GDA total order | [`exact::DecimalValue::cmp_numeric`], [`exact::Decimal::compare_total`] | `values.md` |
//! | Temporal order | [`temporal::cmp_chronological`] | `values.md` |
//! | Injectivity witnesses of the canonical byte encoders | [`canonical::decode_value`], [`canonical::decode_graph`], [`canonical::decode_table`], [`canonical::decode_tree`] | `representation.md` |
//!
//! [`enumerate`] holds the exhaustive small-scope enumerators (labelled,
//! with no pruning of isomorphic copies, each with an independent count),
//! and [`harness`] the differential harness.

pub mod canonical;
pub mod coerce;
pub mod edit;
pub mod encode;
pub mod enumerate;
pub mod exact;
pub mod fan;
pub mod float;
pub mod harness;
pub mod keys;
pub mod lens;
pub mod migration;
pub mod model;
pub mod num;
pub mod path;
pub mod rel;
pub mod resolution;
pub mod restrict;
pub mod schema;
pub mod segment;
pub mod table;
pub mod temporal;
pub mod theory;
pub mod validation;
pub mod value;
