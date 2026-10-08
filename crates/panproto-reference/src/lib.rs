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
//! | Discriminator and numeral-dialect coercion | [`coerce::coerce_tagged`], [`coerce::coerce_dialect`] | `values.md` |
//! | Exact decimals and integers, the GDA total order | [`exact::DecimalValue::cmp_numeric`], [`exact::Decimal::compare_total`] | `values.md` |
//! | Temporal parsing, order and epoch counts | [`temporal::parse_display`], [`temporal::display_parse`], [`temporal::cmp_chronological`], [`temporal::from_epoch_count`], [`temporal::to_epoch_count`] | `values.md` |
//! | Byte-text mappings | [`bytes::latin1_encode`], [`bytes::latin1_decode`], [`bytes::encode_base64`], [`bytes::decode_base64`] | `values.md` |
//! | Declared key equality and sort order | [`keys::key_equivalence`], [`keys::sort_order`], [`keys::key_order`] | `keys-and-registration.md` |
//! | Schema bytes, change classification and registration | [`schema::canonical_bytes`], [`schema::classify_change`], [`schema::register`] | `keys-and-registration.md` |
//! | URI, standoff and rebinding references | [`resolution::rfc3986`], [`resolution::standoff_form`], [`resolution::standoff_inverse`], [`resolution::nearest_preceding`] | `resolution.md` |
//! | Bounded XML expansion and tabular edge cases | [`xml::expand_entities`], [`tabular::missing_token_cst`], [`tabular::one_column_empty_cst`], [`tabular::tsv_quote_cst`] | `value-io.md` |
//! | Canonical and preserving value-I/O laws | [`value_io::round_trip`], [`value_io::preserve`], [`value_io::duplicate_member`] | `value-io.md` |
//! | Injectivity witnesses of the canonical byte encoders | [`canonical::decode_value`], [`canonical::decode_graph`], [`canonical::decode_table`], [`canonical::decode_tree`] | `representation.md` |
//! | Decimal bytes and the exact identity-envelope layout | [`canonical::decimal_bytes`], [`canonical::identity`] | `values.md`, `representation.md` |
//! | Binary16, nearest/exact readings and computed NaNs | [`float::binary16`], [`float::nearest_reading`], [`float::exact_reading`], [`float::computed_nan`] | `values.md` |
//! | Byte-text round-trip predicates | [`bytes::latin1_decode_encode`], [`bytes::latin1_encode_decode`], [`bytes::padded_bytes_decode_encode`], [`bytes::padded_text_encode_decode`], [`bytes::optional`] | `values.md` |
//! | Temporal canonical bytes and unwritten zero offsets | [`temporal::canonical_bytes`], [`temporal::unwritten_zulu`] | `values.md` |
//! | Representation equality and expression evaluation | [`value::representation`], [`value::eval`] | `values.md` |
//! | Reification and tree edit application | [`encode::reify`], [`edit::apply_tree`] | `representation.md`, `edits.md` |
//! | Protolens operations and observable squares | [`lens::protolens_schema`], [`lens::protolens_instance`], [`lens::get`], [`lens::put`], [`lens::graph_square`], [`lens::table_square`] | `lenses.md` |
//! | Migration observable squares | [`migration::graph_square`], [`migration::table_square`], [`migration::tree_square`] | `lenses.md` |
//! | Declared equality, refinement, target sets and lexical boundaries | [`keys::representation_refines`], [`keys::target_set`], [`keys::index_and_null_match`], [`keys::lexical_space`], [`keys::annotated_format`], [`keys::xml_noncdata`] | `keys-and-registration.md` |
//! | Theory morphisms, quotients and recomposition | [`theory::check_morphism`], [`theory::quotient`], [`theory::recompose`] | `theories.md` |
//! | Declared resolution observables | [`resolution::declared_key_equality`], [`resolution::guarded_partition`], [`resolution::index_target`], [`resolution::inherited_base`], [`resolution::preserving_registry`], [`resolution::text_identity`], [`resolution::uri_environment`] | `resolution.md` |
//! | Validation observables for new declarations | [`validation::annotated_format_advisory`], [`validation::annotated_format_parse`], [`validation::format_assertion_acceptance`], [`validation::index_and_null_match`], [`validation::key_equality`], [`validation::key_forms`], [`validation::lexical_space`], [`validation::variable_binding`] | `validation.md` |
//! | Generic parse, emit and route agreement | [`value_io::parse`], [`value_io::emit`], [`value_io::bytes`], [`value_io::blob_writer`], [`value_io::decimal`], [`value_io::temporal`], [`value_io::raw_numeric`] | `value-io.md` |
//! | CST preservation and duplicate handling | [`value_io::parse_cst`], [`value_io::round_trip_cst`], [`value_io::preserve_cst`], [`value_io::duplicate_member_cst`], [`value_io::encoding_bom_cst`], [`value_io::encoding`], [`value_io::xml_encoding`] | `value-io.md` |
//! | TOML, TSV, tabular and XML edge observables | [`toml::decimal_bigint_cst`], [`value_io::toml_scalar`], [`value_io::tsv_comment`], [`value_io::tsv_quote`], [`value_io::tabular_empty_row`], [`value_io::tabular_missing`], [`value_io::xml_entity`], [`value_io::xml_expanded_name`] | `value-io.md` |
//! | XML CST laws | [`xml::bounded_expansion_cst`], [`xml::expanded_name_cst`], [`xml::round_trip_cst`] | `value-io.md` |
//!
//! [`enumerate`] holds the exhaustive small-scope enumerators (labelled,
//! with no pruning of isomorphic copies, each with an independent count),
//! and [`harness`] the differential harness.

pub mod bytes;
pub mod canonical;
pub mod coerce;
pub mod edit;
pub mod encode;
pub mod enumerate;
pub mod exact;
pub mod fan;
pub mod float;
pub mod harness;
pub mod json;
pub mod keys;
pub mod lens;
pub mod manifest;
pub mod migration;
pub mod model;
pub mod num;
pub mod pages;
pub mod path;
pub mod pattern;
pub mod rel;
pub mod resolution;
pub mod restrict;
pub mod schema;
pub mod segment;
pub mod table;
pub mod tabular;
pub mod temporal;
pub mod theory;
pub mod toml;
pub mod validation;
pub mod value;
pub mod value_io;
pub mod xml;
