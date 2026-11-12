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
//! The inventory assigns each definition to a page of the book's correctness
//! part (`book/src/correctness/`). Table membership records that assignment;
//! it does not by itself claim that the page's proof and verification scope
//! are complete. Each function's documentation states the definition and
//! exact domain it implements. Where a function covers only part of an
//! operation's domain, its documentation states the part, and every input
//! outside it gives `None` rather than an answer.
//!
//! | Operation | Function | Page |
//! |---|---|---|
//! | Delta on finite attributed presheaves | [`rel::delta`] | `kan-extensions.md` |
//! | Sigma, by the quotient of the disjoint union | [`rel::sigma`] | `kan-extensions.md` |
//! | Pi, by enumerating compatible families | [`rel::pi`] | `kan-extensions.md` |
//! | The migration's entity functor | [`rel::entity_functor`] | `kan-extensions.md` |
//! | Relation-ified schemas (classification) | [`rel::classify`] | `kan-extensions.md` |
//! | Homomorphisms and structure-preserving bijections of instances | [`rel::homs`], [`rel::row_bijection`] | `kan-extensions.md` |
//! | Delta, Sigma and Pi on maps, with the `NotANaturalMap` row | [`rel::delta_on_hom`], [`rel::sigma_on_hom`], [`rel::pi_on_hom`] | `kan-extensions.md` |
//! | Sigma--Delta unit, counit and hom-set transposes | [`rel::sigma_unit`], [`rel::sigma_counit`], [`rel::sigma_transpose_forward`], [`rel::sigma_transpose_inverse`] | `kan-extensions.md` |
//! | Delta--Pi unit, counit and hom-set transposes | [`rel::pi_unit`], [`rel::pi_counit`], [`rel::pi_transpose_forward`], [`rel::pi_transpose_inverse`] | `kan-extensions.md` |
//! | Graph Delta, Sigma and Pi (encode, presheaf operation, decode) | [`encode::g_delta`], [`encode::g_sigma`], [`encode::g_pi`] | `kan-extensions.md` |
//! | Tree Delta, Sigma and Pi through the graph form | [`encode::t_delta`], [`encode::t_sigma`], [`encode::t_pi`] | `kan-extensions.md` |
//! | Graph encoding and record-free decoding | [`encode::encode_graph`], [`encode::decode_table`] | `kan-extensions.md` |
//! | Tree and table bridges to graphs | [`encode::tree_to_graph`], [`encode::graph_to_tree`], [`encode::table_to_graph`], [`encode::graph_to_table`] | `representation.md` |
//! | The edge image | [`restrict::edge_image`] | `restrict-and-contraction.md` |
//! | Pair resolution | [`restrict::resolve_pair`] | `restrict-and-contraction.md` |
//! | The contraction plan | [`restrict::contraction_plan`] | `restrict-and-contraction.md` |
//! | Restriction plan compilation | [`restrict::compile`] | `restrict-and-contraction.md` |
//! | Restrict (Delta, then contraction, then fans) | [`restrict::get`] | `restrict-and-contraction.md` |
//! | Restrict reachability by explicit walks | [`restrict::realized_pairs_by_walks`] | `restrict-and-contraction.md` |
//! | Relational-column and ordered-child restriction observables | [`restrict::column_fates`], [`restrict::scalar_child_bindings`] | `restrict-and-contraction.md` |
//! | Tree restriction domain and law basis | [`lens::TreeShape::check_put_domain`](lens::LawShape::check_put_domain), [`lens::TreeShape::single_step_basis`](lens::LawShape::single_step_basis), [`lens::check_tree_laws`] | `restrict-and-contraction.md` |
//! | Tree-edit restriction translation | [`pages::restrict_and_contraction::tree_edit_lens::lower`], [`pages::restrict_and_contraction::tree_edit_lens::Lens::get_edit`], [`pages::restrict_and_contraction::tree_edit_lens::Lens::put_edit`], [`pages::restrict_and_contraction::tree_edit_lens::Delta::compose`], [`pages::restrict_and_contraction::tree_edit_lens::Delta::invert`], [`pages::restrict_and_contraction::tree_edit_lens::Delta::apply`], [`pages::restrict_and_contraction::tree_edit_lens::is_minimal`] | `restrict-and-contraction.md` |
//! | The fan domain | [`fan::violations_at`] | `representation.md` |
//! | Get and put of the graph lens | [`restrict::get`], [`lens::GraphLens::put`] | `lenses.md` |
//! | The graph put domain | [`lens::GraphLens::check_put_domain`] | `lenses.md` |
//! | Graph lens law reports | [`lens::check_graph_laws`], [`lens::check_laws_report`] | `lenses.md` |
//! | Complement composition | [`lens::compose_complement`] | `lenses.md` |
//! | Get and put of the table lens | [`table::get`], [`table::TableLens::put`] | `lenses.md` |
//! | The table plan and cell writers | [`table::compile_for_tables`], [`table::TablePlan::writer`] | `lenses.md` |
//! | Get and put of the tree lens | [`lens::tree_get`], [`lens::tree_put`] | `lenses.md` |
//! | Shape-specific path folds | [`lens::TreeShape::get`](lens::LawShape::get), [`lens::TreeShape::put`](lens::LawShape::put), [`lens::TableShape::get`](lens::LawShape::get), [`lens::TableShape::put`](lens::LawShape::put) | `lenses.md` |
//! | Get and put of the resolution lens | [`resolution::get`], [`resolution::put`], [`resolution::deferred_form`] | `resolution.md` |
//! | Lens-path composition (the free category; no fusion) | [`path::LensPath::then`], [`path::get_path`], [`path::put_path`] | `lenses.md` |
//! | The edit algebra: invert, compose, independence and merge | [`edit::Delta::invert`], [`edit::Delta::compose`], [`edit::invert_table`], [`edit::compose_table`], [`edit::independence`], [`edit::merge`] | `edits.md` |
//! | Edit application and the differ | [`edit::apply`], [`edit::diff`], [`edit::apply_table`], [`edit::diff_table`] | `edits.md` |
//! | Table edit steps: the delta of a surface edit, its stale-checked application and the row-delete delta | [`edit::table_edit_delta`], [`edit::apply_table_step`], [`edit::delete_row_delta`] | `edits.md` |
//! | Canonical graph, table and tree edit bytes | [`edit::canonical_graph_delta_bytes`], [`edit::canonical_table_delta_bytes`], [`edit::canonical_tree_edit_bytes`] | `edits.md` |
//! | Edit translation (apply then get against translate then apply) | [`edit::get_edit`], [`edit::put_edit`], [`edit::graph_delta_from_tree_edit`], [`edit::diff_keeping_replacements`] | `edits.md` |
//! | The graph edit lens: the stable presentation of synthesized ids and the stateful translations of source and view edits | [`edit::stable_presentation`], [`edit::graph_edit_get`], [`edit::graph_edit_put`] | `edits.md` |
//! | The table edit lens: the closed table differ and the stateful translations of source and view edits | [`edit::diff_keeping_replacements_table`], [`edit::table_edit_get`], [`edit::table_edit_put`] | `edits.md` |
//! | Rebasing graph and table deltas over committed deltas, with table independence and merge | [`edit::rebase`], [`edit::rebase_table`], [`edit::table_independence`], [`edit::merge_table`] | `edits.md` |
//! | Validation as predicates over instances | [`validation::validate`], [`validation::validate_table`], [`validation::validate_tree`] | `validation.md` |
//! | Validation transition and boundary observations | [`validation::witness_with_trust`], [`validation::incremental_footprint`], [`validation::incremental_report`], [`validation::restriction_boundary`] | `validation.md` |
//! | Admission policies | [`validation::admits`] | `validation.md` |
//! | Key values, scopes and buckets | [`keys::key_value`], [`keys::buckets`] | `keys-and-registration.md` |
//! | Segment ownership, boundaries and splitting | [`segment::partition`] | `segments.md` |
//! | Segment assembly | [`segment::glue`] | `segments.md` |
//! | Canonical segment artifacts and manifests | [`segment::canonical_manifest_bytes`], [`segment::canonical_segment_bytes`], [`segment::seal_manifest`], [`segment::verify_manifest`] | `segments.md` |
//! | Segment task-input identity and journal reuse | [`segment::task_inputs_digest`], [`segment::journal_task_reusable`] | `segments.md` |
//! | Segment refusal and contract preflights | [`segment::decode_segment_bytes`], [`segment::validate_segment_contract`], [`segment::validation_view_preflight`] | `segments.md` |
//! | Segment execution as whole-state execution on the glued state | [`segment::segmented`] | `segments.md` |
//! | Allocation within a range | [`segment::fresh`] | `segments.md` |
//! | Segmented graph restriction, by whole-state restriction of the glued graph | [`segment::restrict_graph_segmented`] | `segments.md` |
//! | Segmented graph restriction numbering stably against held complements | [`segment::restrict_graph_segmented_held`] | `segments.md` |
//! | The segmented graph edit lens, by application to the glued source, stable segmented restriction and the differ of the glued views | [`segment::bind_segmented_edit`], [`segment::segmented_edit_get`], [`segment::segmented_edit_put`] | `segments.md` |
//! | Segmented table get, by whole-state table get of the glued table split by owner | [`segment::restrict_table_segmented`], [`segment::split_table_complement`] | `segments.md` |
//! | Segmented table put, by whole-state table put of the glued view split by owner | [`segment::put_table_segmented`] | `segments.md` |
//! | Segmented tree get, by tree get over the glued tree's recorded embedding split by owner | [`segment::restrict_tree_segmented`], [`segment::split_tree_complement`], [`lens::tree_get_embedded`] | `segments.md` |
//! | The segmented table edit lens, by segmented application, segmented table get and put, and the differ of the glued tables | [`segment::bind_segmented_table_edit`], [`segment::segmented_table_edit_get`], [`segment::segmented_table_edit_put`] | `segments.md` |
//! | Store collection, by reachability through the journals of the root runs | [`segment::collected`] | `segments.md` |
//! | The checkpointed run of segmented graph restriction, resumed or carrying tasks over from an earlier run | [`segment::segmented_restrict_with_progress`] | `segments.md` |
//! | Record streams of segments, by frames of the canonical instance records | [`segment::segment_record_stream`] | `segments.md` |
//! | Manifest record bytes | [`segment::canonical_record_bytes`] | `segments.md` |
//! | Manifest records of a family, without the seal floor | [`segment::manifest_records`] | `segments.md` |
//! | Segmented graph put, by whole-state put of the glued view | [`segment::put_graph_segmented`] | `segments.md` |
//! | The checkpointed run of segmented graph put | [`segment::segmented_put_with_progress`] | `segments.md` |
//! | Segmented graph extension, by whole-state extension of the glued graph | [`segment::extend_graph_segmented`], [`segment::extend_graph`] | `segments.md` |
//! | The checkpointed run of segmented graph extension | [`segment::segmented_extend_with_progress`] | `segments.md` |
//! | Segmented graph edits, by whole-state application of the delta to the glued graph, and the segments they read | [`segment::apply_graph_segmented`], [`segment::read_set`] | `segments.md` |
//! | The checkpointed run of segmented graph edits | [`segment::segmented_apply_with_progress`], [`segment::apply_facts_bytes`] | `segments.md` |
//! | Segmented table edits, by whole-state application of the delta to the glued table, and the segments they read | [`segment::apply_table_segmented`], [`segment::table_read_set`] | `segments.md` |
//! | The checkpointed run of segmented table edits | [`segment::segmented_table_apply_with_progress`], [`segment::table_apply_facts_bytes`] | `segments.md` |
//! | Segmented tree edits, by whole-state application of the edit to the glued tree, and the segments they read | [`segment::apply_tree_segmented`], [`segment::tree_read_set`] | `segments.md` |
//! | The checkpointed run of segmented tree edits | [`segment::segmented_tree_apply_with_progress`], [`segment::tree_apply_facts_bytes`] | `segments.md` |
//! | Graph complement segment bytes and their decoding | [`segment::canonical_complement_segment_bytes`], [`segment::decode_complement_segment_bytes`] | `segments.md` |
//! | Signature type checking | [`theory::typecheck_signature`] | `theories.md` |
//! | Theory colimits | [`theory::compose`] | `theories.md` |
//! | `L_b`: round to nearest, ties to even, at widths 16, 32 and 64 | [`float::round_to`] | `values.md` |
//! | `C_b`: closest-shortest decimal, ties to even | [`float::shortest`] | `values.md` |
//! | NaN narrowing and widening on the bits | [`float::narrow_bits`], [`float::widen_bits`] | `values.md` |
//! | Coercion of a lexeme under a declared kind | [`coerce::coerce`] | `values.md` |
//! | Coercion of a value | [`coerce::coerce_value`] | `values.md` |
//! | Discriminator and numeral-dialect coercion | [`coerce::coerce_tagged`], [`coerce::coerce_dialect`] | `values.md` |
//! | Exact decimals and integers, the GDA total order | [`exact::DecimalValue::cmp_numeric`], [`exact::Decimal::cmp_numeric`], [`exact::Decimal::compare_total`] | `values.md` |
//! | Temporal parsing, order and epoch counts | [`temporal::parse_display`], [`temporal::display_parse`], [`temporal::cmp_chronological`], [`temporal::from_epoch_count`], [`temporal::to_epoch_count`] | `values.md` |
//! | Byte-text mappings | [`bytes::latin1_encode`], [`bytes::latin1_decode`], [`bytes::encode_base64`], [`bytes::decode_base64`] | `values.md` |
//! | Declared key equality and sort order | [`keys::key_equivalence`], [`keys::sort_order`], [`keys::key_order`] | `keys-and-registration.md` |
//! | Schema bytes and change classification | [`schema::canonical_bytes`], [`schema::classify_change`] | `keys-and-registration.md` |
//! | URI, standoff and rebinding references | [`resolution::rfc3986`], [`resolution::standoff_form`], [`resolution::standoff_inverse`], [`resolution::standoff_byte_span`], [`resolution::standoff_byte_inverse`], [`resolution::nearest_preceding`] | `resolution.md` |
//! | The bounded S8 resolution state model | [`resolution::indexed_get`], [`resolution::indexed_put`], [`resolution::check_put_domain`], [`resolution::coherent_rekey`], [`resolution::coherent_rename`], [`resolution::equivalent`], [`resolution::gated_put_twice`] | `resolution.md` |
//! | Bounded XML expansion and tabular edge cases | [`xml::expand_entities`], [`tabular::missing_token_cst`], [`tabular::one_column_empty_cst`], [`tabular::tsv_quote_cst`] | `value-io.md` |
//! | Canonical value-I/O and presence laws | [`value_io::round_trip`], [`value_io::presence`], [`value_io::duplicate_member`] | `value-io.md` |
//! | Injectivity witnesses of the canonical byte encoders | [`canonical::decode_value`], [`canonical::value_bytes`], [`canonical::decode_graph`], [`canonical::decode_table`], [`canonical::decode_tree`] | `representation.md` |
//! | Decimal bytes and the exact identity-envelope layout | [`canonical::decimal_bytes`], [`canonical::identity`] | `values.md`, `representation.md` |
//! | Binary16, exact readings and computed NaNs | [`float::binary16`], [`float::exact_reading`], [`float::computed_nan`] | `values.md` |
//! | Byte-text round-trip predicates | [`bytes::latin1_decode_encode`], [`bytes::latin1_encode_decode`], [`bytes::padded_bytes_decode_encode`], [`bytes::padded_text_encode_decode`], [`bytes::optional`] | `values.md` |
//! | Temporal canonical bytes and unwritten zero offsets | [`temporal::canonical_bytes`], [`temporal::unwritten_zulu`] | `values.md` |
//! | Representation equality and expression evaluation | [`value::representation`], [`value::eval`] | `values.md` |
//! | Reification and tree edit application | [`encode::reify`], [`edit::apply_tree`] | `representation.md`, `edits.md` |
//! | Protolens operations | [`lens::protolens_schema`], [`lens::protolens_instance`], [`lens::get`], [`lens::put`] | `lenses.md` |
//! | Elementary protolens kinds | [`lens::ElementaryProtolens::target_and_migration`], [`lens::relabel`], [`lens::restrict`], [`lens::extend`] | `lenses.md` |
//! | Fused chains and scoped protolenses | [`lens::ProtolensTerm::at`], [`lens::ProtolensTerm::target`], [`lens::ProtolensTerm::transform`], [`lens::ProtolensTerm::surviving_vertices`], [`lens::ProtolensTerm::edge_images`], [`lens::structural_migration`] | `lenses.md` |
//! | Sequential protolens composites | [`lens::CompositeLens::instantiate`], [`lens::CompositeLens::get`], [`lens::CompositeLens::put`], [`lens::CompositeLens::check_put_domain`] | `lenses.md` |
//! | Tree view alignment | [`lens::align_tree_view`] | `lenses.md` |
//! | Graph and table observable squares | [`lens::graph_square`], [`lens::graph_put_square`], [`lens::table_square`], [`lens::table_put_square`], [`lens::relational_table_get`], [`lens::relational_table_put`] | `lenses.md` |
//! | Migration observable squares | [`migration::graph_square`], [`migration::table_square`], [`migration::tree_square`] | `lenses.md` |
//! | Declared equality, refinement, target sets and XML lexical boundaries | [`keys::representation_refines`], [`keys::target_set`], [`keys::xml_noncdata`] | `keys-and-registration.md`, `validation.md` |
//! | Lexical-space and annotated-format declarations | [`keys::lexical_space_outcome`], [`keys::annotated_format_outcome`], [`keys::uuid_format`] | `keys-and-registration.md` |
//! | Schema declaration validation | [`schema::validate_declarations`] | `validation.md`, `keys-and-registration.md` |
//! | Validation status fold | [`validation::status`] | `validation.md` |
//! | Tree and graph presentation equivalence | [`lens::GraphShape::equivalent`](lens::LensShape::equivalent), [`lens::TreeShape::equivalent`](lens::LensShape::equivalent) | `lenses.md` |
//! | Lens construction obstructions and span compatibility | [`lens::construction_obstruction`], [`lens::span_has_shared_middle`] | `lenses.md` |
//! | Table isomorphism and complement-free put | [`table::is_isomorphism`], [`table::obstruction_to_isomorphism`], [`table::put_without_complement`] | `lenses.md` |
//! | Theory morphisms, quotients and recomposition | [`theory::check_morphism`], [`theory::quotient`], [`theory::recompose`] | `theories.md` |
//! | Declared resolution observables | [`resolution::declared_key_equality`], [`resolution::guarded_partition`], [`resolution::inherited_base`], [`resolution::preserving_registry`], [`resolution::text_identity`], [`resolution::uri_environment`] | `resolution.md` |
//! | Validation observables for new declarations | [`validation::annotated_format_advisory`], [`validation::annotated_format_parse`], [`validation::classify_selector`], [`validation::format_assertion_acceptance`], [`validation::lexical_space`] | `validation.md` |
//! | Canonical theory and composition bytes | [`theory::canonical_theory_bytes`], [`theory::canonical_composition_bytes`] | `keys-and-registration.md` |
//! | Generic parse, emit and route agreement | [`value_io::parse`], [`value_io::emit`], [`value_io::bytes`], [`value_io::blob_writer`], [`value_io::decimal`], [`value_io::temporal`], [`value_io::raw_numeric`] | `value-io.md` |
//! | Canonical encoding normalization | [`value_io::encoding`], [`value_io::xml_encoding`] | `value-io.md` |
//! | CST preservation and duplicate handling | [`value_io::parse_cst`], [`value_io::round_trip_cst`], [`value_io::preserve`], [`value_io::preserve_cst`], [`value_io::duplicate_member_cst`], [`value_io::encoding_bom_cst`] | `value-io.md` |
//! | TOML, TSV, tabular and XML edge observables | [`toml::decimal_bigint_cst`], [`value_io::toml_scalar`], [`value_io::tsv_comment`], [`value_io::tsv_quote`], [`value_io::tabular_empty_row`], [`value_io::tabular_missing`], [`value_io::xml_entity`], [`value_io::xml_expanded_name`] | `value-io.md` |
//! | XML CST laws | [`xml::bounded_expansion_cst`], [`xml::expanded_name_cst`], [`xml::round_trip_cst`] | `value-io.md` |
//! | TOML key resolution and YAML directive acceptance | [`pages::text_formats::toml::resolve_keys`], [`pages::text_formats::yaml::read_directives`] | `text-formats.md` |
//! | Whole TOML and block YAML document reading | [`pages::text_formats::toml::read_document`], [`pages::text_formats::yaml::render_block`], [`pages::text_formats::yaml::read_block_document`] | `text-formats.md` |
//! | PostgreSQL data values, rows, canonical text and table directory fields | [`pages::postgresql_data::input_value`], [`pages::postgresql_data::render`], [`pages::postgresql_data::read_script`], [`pages::postgresql_data::canonical_text`], [`pages::postgresql_data::csv_field`], [`pages::postgresql_data::text_field`] | `postgresql-data.md` |
//! | PostgreSQL DDL column types, keys and references, and canonical text | [`pages::postgresql_ddl::column_type`], [`pages::postgresql_ddl::render`], [`pages::postgresql_ddl::read_script`], [`pages::postgresql_ddl::canonical_text`] | `postgresql-ddl.md` |
//! | N-Triples reading, the RDF graph encoding and canonical writing | [`pages::rdf_formats::ntriples::read_document`], [`pages::rdf_formats::ntriples::encode`], [`pages::rdf_formats::ntriples::canonical_text`], [`pages::rdf_formats::ntriples::write_graph`] | `rdf-formats.md` |
//! | Turtle reading, IRI resolution, canonical Turtle and format-preserving edits | [`pages::rdf_formats::turtle::read_document`], [`pages::rdf_formats::turtle::resolve`], [`pages::rdf_formats::turtle::canonical_text`], [`pages::rdf_formats::turtle::edit`], [`pages::rdf_formats::turtle::edited_triples`] | `rdf-formats.md` |
//! | JSON-LD reading, native numbers, canonical JSON-LD and format-preserving edits | [`pages::rdf_formats::jsonld::read_document`], [`pages::rdf_formats::jsonld::number_form`], [`pages::rdf_formats::jsonld::canonical_text`] | `rdf-formats.md` |
//! | Edge-list reading, the edge-list graph encoding and canonical writing | [`pages::edge_lists::read_document`], [`pages::edge_lists::encode`], [`pages::edge_lists::canonical_text`], [`pages::edge_lists::write_graph`] | `edge-lists.md` |
//! | Kubernetes CRD reading and canonical writing | [`pages::kubernetes_crds::read`], [`pages::kubernetes_crds::render_yaml`], [`pages::kubernetes_crds::canonical_text`] | `kubernetes-crds.md` |
//! | `CloudFormation` template reading, reference resolution and canonical writing | [`pages::cloudformation::read`], [`pages::cloudformation::render_yaml`], [`pages::cloudformation::json`], [`pages::cloudformation::canonical_text`] | `cloudformation.md` |
//! | Ansible playbook, task-file and variables-file reading and canonical writing | [`pages::ansible::read`], [`pages::ansible::render_yaml`], [`pages::ansible::canonical_text`] | `ansible.md` |
//! | JSON Schema 2020-12 reading, reference resolution and canonical writing | [`pages::json_schema::read`], [`pages::json_schema::render`], [`pages::json_schema::resolve`], [`pages::json_schema::canonical_text`] | `json-schema.md` |
//! | `OpenAPI` 3.1 reading, reference resolution and canonical writing | [`pages::openapi::read`], [`pages::json_schema::read_embedded`], [`pages::openapi::canonical_text`] | `openapi.md` |
//! | BSON 1.1 decoding, canonical binary writing, the BSON tree encoding and Extended JSON reading and writing | [`pages::bson::decode`], [`pages::bson::encode`], [`pages::bson::tree`], [`pages::bson::canonical_text`], [`pages::bson::relaxed_text`], [`pages::bson::read_text`] | `bson.md` |
//! | Optimal total morphisms and spans, the morphism checker, the search cost, the isomorphism edge pairing and the answer digest | [`pages::migration_search::optimal_total`], [`pages::migration_search::optimal_span`], [`pages::migration_search::check_total`], [`pages::migration_search::check_span`], [`pages::migration_search::cost`], [`pages::migration_search::paired_edge_images`], [`pages::migration_search::digest`] | `migration-search.md` |
//! | The per-element three-way schema merge, well-formedness, the merge rendering and conflict order, the selected migration's surviving identity and the repository format marker | [`pages::schema_merge::merge`], [`pages::schema_merge::Schema::well_formed`], [`pages::schema_merge::render`], [`pages::schema_merge::Conflict::order_key`], [`pages::schema_merge::Conflict::swapped`], [`pages::schema_merge::surviving_identity`], [`pages::schema_merge::may_rename`], [`pages::schema_merge::repository_format`] | `schema-merge.md` |
//! | Theory DSL document grammar, elaboration, morphism, protocol and composition documents and refusal location | [`pages::theory_dsl::check_document`], [`pages::theory_dsl::elaborate`], [`pages::theory_dsl::compile_morphism`], [`pages::theory_dsl::compile_protocol`], [`pages::theory_dsl::compile_composition`], [`pages::theory_dsl::compile_document`], [`pages::theory_dsl::locate`] | `theory-dsl.md` |
//! | Lens DSL document grammar, compilation of steps and rules, vertical and horizontal composition, and refusal location | [`pages::lens_dsl::check_document`], [`pages::lens_dsl::step_stages`], [`pages::lens_dsl::compile_document`], [`pages::lens_dsl::rules_stages`], [`pages::lens_dsl::vertical`], [`pages::lens_dsl::horizontal`], [`dsl_path::locate`] | `lens-dsl.md` |
//! | Payload decoding outcomes, the recorded payload encoding, the roots, references, reachable objects and garbage of a repository, the instances a commit tracks, the deltas between instances, the three-way merge of tracked instances, their rebase, the refusal order and stored objects of a data migration, the merge of versions migrated to the merged schema, and the migration path first-parent history records | [`pages::instance_version_control::payload_outcome`], [`pages::instance_version_control::recorded_encoding`], [`pages::instance_version_control::writes_encoding_field`], [`pages::instance_version_control::Repository::roots`], [`pages::instance_version_control::Object::links`], [`pages::instance_version_control::Repository::reachable`], [`pages::instance_version_control::Repository::garbage`], [`pages::instance_version_control::tracked`], [`pages::instance_version_control::diff_instances`], [`pages::instance_version_control::apply_delta`], [`pages::instance_version_control::diff_tracked`], [`pages::instance_version_control::normalized`], [`pages::instance_version_control::merge_tracked`], [`pages::instance_version_control::merge_instances`], [`pages::instance_version_control::rebase_instances`], [`pages::instance_version_control::migration_outcome`], [`pages::instance_version_control::forward_objects`], [`pages::instance_version_control::merge_tracked_to`], [`pages::instance_version_control::migrate_and_combine`], [`pages::instance_version_control::merge_path`], [`pages::instance_version_control::migration_path`] | `instance-version-control.md` |
//!
//! [`enumerate`] holds the exhaustive small-scope enumerators (labelled,
//! with no pruning of isomorphic copies, each with an independent count),
//! and [`harness`] the differential harness.

mod blake3_ref;

pub mod bytes;
pub mod canonical;
pub mod coerce;
pub mod dsl_path;
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
