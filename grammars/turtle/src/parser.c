#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 118
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 89
#define ALIAS_COUNT 0
#define TOKEN_COUNT 46
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 3
#define MAX_ALIAS_SEQUENCE_LENGTH 5
#define PRODUCTION_ID_COUNT 4

enum ts_symbol_identifiers {
  sym_pn_prefix = 1,
  anon_sym_LBRACE = 2,
  anon_sym_RBRACE = 3,
  anon_sym_GRAPH = 4,
  sym_comment = 5,
  anon_sym_DOT = 6,
  anon_sym_ATprefix = 7,
  anon_sym_ATbase = 8,
  aux_sym_sparql_base_token1 = 9,
  aux_sym_sparql_prefix_token1 = 10,
  anon_sym_SEMI = 11,
  anon_sym_COMMA = 12,
  anon_sym_a = 13,
  anon_sym_LBRACK = 14,
  anon_sym_RBRACK = 15,
  anon_sym_LPAREN = 16,
  anon_sym_RPAREN = 17,
  anon_sym_LT = 18,
  anon_sym_POUND = 19,
  aux_sym_iri_reference_token1 = 20,
  anon_sym_GT = 21,
  sym_integer = 22,
  sym_decimal = 23,
  sym_double = 24,
  anon_sym_DQUOTE = 25,
  aux_sym__string_literal_quote_token1 = 26,
  aux_sym__string_literal_quote_token2 = 27,
  anon_sym_SQUOTE = 28,
  aux_sym__string_literal_single_quote_token1 = 29,
  anon_sym_SQUOTE_SQUOTE_SQUOTE = 30,
  anon_sym_SQUOTE_SQUOTE = 31,
  aux_sym__string_literal_long_single_quote_token1 = 32,
  anon_sym_DQUOTE_DQUOTE_DQUOTE = 33,
  anon_sym_DQUOTE_DQUOTE = 34,
  aux_sym__string_literal_long_quote_token1 = 35,
  anon_sym_CARET_CARET = 36,
  anon_sym_true = 37,
  anon_sym_false = 38,
  anon_sym_COLON = 39,
  anon_sym__COLON = 40,
  aux_sym_blank_node_label_token1 = 41,
  sym_lang_tag = 42,
  sym_echar = 43,
  sym_anon = 44,
  sym_pn_local = 45,
  sym_document = 46,
  sym_graph = 47,
  sym__label = 48,
  sym_triple = 49,
  sym_directive = 50,
  sym_prefix_id = 51,
  sym_base = 52,
  sym_sparql_base = 53,
  sym_sparql_prefix = 54,
  sym__triples = 55,
  sym_property_list = 56,
  sym_property = 57,
  sym_object_list = 58,
  sym_predicate = 59,
  sym_subject = 60,
  sym__object = 61,
  sym__literal = 62,
  sym_blank_node_property_list = 63,
  sym_collection = 64,
  sym_object_collection = 65,
  sym__numeric_literal = 66,
  sym_string = 67,
  sym_iri_reference = 68,
  sym__string_literal_quote = 69,
  sym__string_literal_single_quote = 70,
  sym__string_literal_long_single_quote = 71,
  sym__string_literal_long_quote = 72,
  sym_rdf_literal = 73,
  sym_boolean_literal = 74,
  sym__iri = 75,
  sym_prefixed_name = 76,
  sym__blank_node = 77,
  sym_namespace = 78,
  sym_blank_node_label = 79,
  aux_sym_document_repeat1 = 80,
  aux_sym_graph_repeat1 = 81,
  aux_sym_property_list_repeat1 = 82,
  aux_sym_object_list_repeat1 = 83,
  aux_sym_object_collection_repeat1 = 84,
  aux_sym__string_literal_quote_repeat1 = 85,
  aux_sym__string_literal_single_quote_repeat1 = 86,
  aux_sym__string_literal_long_single_quote_repeat1 = 87,
  aux_sym__string_literal_long_quote_repeat1 = 88,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym_pn_prefix] = "pn_prefix",
  [anon_sym_LBRACE] = "{",
  [anon_sym_RBRACE] = "}",
  [anon_sym_GRAPH] = "GRAPH",
  [sym_comment] = "comment",
  [anon_sym_DOT] = ".",
  [anon_sym_ATprefix] = "@prefix",
  [anon_sym_ATbase] = "@base",
  [aux_sym_sparql_base_token1] = "BASE",
  [aux_sym_sparql_prefix_token1] = "PREFIX",
  [anon_sym_SEMI] = ";",
  [anon_sym_COMMA] = ",",
  [anon_sym_a] = "a",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_LT] = "<",
  [anon_sym_POUND] = "#",
  [aux_sym_iri_reference_token1] = "iri_reference_token1",
  [anon_sym_GT] = ">",
  [sym_integer] = "integer",
  [sym_decimal] = "decimal",
  [sym_double] = "double",
  [anon_sym_DQUOTE] = "\"",
  [aux_sym__string_literal_quote_token1] = "_string_literal_quote_token1",
  [aux_sym__string_literal_quote_token2] = "_string_literal_quote_token2",
  [anon_sym_SQUOTE] = "'",
  [aux_sym__string_literal_single_quote_token1] = "_string_literal_single_quote_token1",
  [anon_sym_SQUOTE_SQUOTE_SQUOTE] = "'''",
  [anon_sym_SQUOTE_SQUOTE] = "''",
  [aux_sym__string_literal_long_single_quote_token1] = "_string_literal_long_single_quote_token1",
  [anon_sym_DQUOTE_DQUOTE_DQUOTE] = "\"\"\"",
  [anon_sym_DQUOTE_DQUOTE] = "\"\"",
  [aux_sym__string_literal_long_quote_token1] = "_string_literal_long_quote_token1",
  [anon_sym_CARET_CARET] = "^^",
  [anon_sym_true] = "true",
  [anon_sym_false] = "false",
  [anon_sym_COLON] = ":",
  [anon_sym__COLON] = "_:",
  [aux_sym_blank_node_label_token1] = "blank_node_label_token1",
  [sym_lang_tag] = "lang_tag",
  [sym_echar] = "echar",
  [sym_anon] = "anon",
  [sym_pn_local] = "pn_local",
  [sym_document] = "document",
  [sym_graph] = "graph",
  [sym__label] = "_label",
  [sym_triple] = "triple",
  [sym_directive] = "directive",
  [sym_prefix_id] = "prefix_id",
  [sym_base] = "base",
  [sym_sparql_base] = "sparql_base",
  [sym_sparql_prefix] = "sparql_prefix",
  [sym__triples] = "_triples",
  [sym_property_list] = "property_list",
  [sym_property] = "property",
  [sym_object_list] = "object_list",
  [sym_predicate] = "predicate",
  [sym_subject] = "subject",
  [sym__object] = "_object",
  [sym__literal] = "_literal",
  [sym_blank_node_property_list] = "blank_node_property_list",
  [sym_collection] = "collection",
  [sym_object_collection] = "object_collection",
  [sym__numeric_literal] = "_numeric_literal",
  [sym_string] = "string",
  [sym_iri_reference] = "iri_reference",
  [sym__string_literal_quote] = "_string_literal_quote",
  [sym__string_literal_single_quote] = "_string_literal_single_quote",
  [sym__string_literal_long_single_quote] = "_string_literal_long_single_quote",
  [sym__string_literal_long_quote] = "_string_literal_long_quote",
  [sym_rdf_literal] = "rdf_literal",
  [sym_boolean_literal] = "boolean_literal",
  [sym__iri] = "_iri",
  [sym_prefixed_name] = "prefixed_name",
  [sym__blank_node] = "_blank_node",
  [sym_namespace] = "namespace",
  [sym_blank_node_label] = "blank_node_label",
  [aux_sym_document_repeat1] = "document_repeat1",
  [aux_sym_graph_repeat1] = "graph_repeat1",
  [aux_sym_property_list_repeat1] = "property_list_repeat1",
  [aux_sym_object_list_repeat1] = "object_list_repeat1",
  [aux_sym_object_collection_repeat1] = "object_collection_repeat1",
  [aux_sym__string_literal_quote_repeat1] = "_string_literal_quote_repeat1",
  [aux_sym__string_literal_single_quote_repeat1] = "_string_literal_single_quote_repeat1",
  [aux_sym__string_literal_long_single_quote_repeat1] = "_string_literal_long_single_quote_repeat1",
  [aux_sym__string_literal_long_quote_repeat1] = "_string_literal_long_quote_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym_pn_prefix] = sym_pn_prefix,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_GRAPH] = anon_sym_GRAPH,
  [sym_comment] = sym_comment,
  [anon_sym_DOT] = anon_sym_DOT,
  [anon_sym_ATprefix] = anon_sym_ATprefix,
  [anon_sym_ATbase] = anon_sym_ATbase,
  [aux_sym_sparql_base_token1] = aux_sym_sparql_base_token1,
  [aux_sym_sparql_prefix_token1] = aux_sym_sparql_prefix_token1,
  [anon_sym_SEMI] = anon_sym_SEMI,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_a] = anon_sym_a,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_LT] = anon_sym_LT,
  [anon_sym_POUND] = anon_sym_POUND,
  [aux_sym_iri_reference_token1] = aux_sym_iri_reference_token1,
  [anon_sym_GT] = anon_sym_GT,
  [sym_integer] = sym_integer,
  [sym_decimal] = sym_decimal,
  [sym_double] = sym_double,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [aux_sym__string_literal_quote_token1] = aux_sym__string_literal_quote_token1,
  [aux_sym__string_literal_quote_token2] = aux_sym__string_literal_quote_token2,
  [anon_sym_SQUOTE] = anon_sym_SQUOTE,
  [aux_sym__string_literal_single_quote_token1] = aux_sym__string_literal_single_quote_token1,
  [anon_sym_SQUOTE_SQUOTE_SQUOTE] = anon_sym_SQUOTE_SQUOTE_SQUOTE,
  [anon_sym_SQUOTE_SQUOTE] = anon_sym_SQUOTE_SQUOTE,
  [aux_sym__string_literal_long_single_quote_token1] = aux_sym__string_literal_long_single_quote_token1,
  [anon_sym_DQUOTE_DQUOTE_DQUOTE] = anon_sym_DQUOTE_DQUOTE_DQUOTE,
  [anon_sym_DQUOTE_DQUOTE] = anon_sym_DQUOTE_DQUOTE,
  [aux_sym__string_literal_long_quote_token1] = aux_sym__string_literal_long_quote_token1,
  [anon_sym_CARET_CARET] = anon_sym_CARET_CARET,
  [anon_sym_true] = anon_sym_true,
  [anon_sym_false] = anon_sym_false,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym__COLON] = anon_sym__COLON,
  [aux_sym_blank_node_label_token1] = aux_sym_blank_node_label_token1,
  [sym_lang_tag] = sym_lang_tag,
  [sym_echar] = sym_echar,
  [sym_anon] = sym_anon,
  [sym_pn_local] = sym_pn_local,
  [sym_document] = sym_document,
  [sym_graph] = sym_graph,
  [sym__label] = sym__label,
  [sym_triple] = sym_triple,
  [sym_directive] = sym_directive,
  [sym_prefix_id] = sym_prefix_id,
  [sym_base] = sym_base,
  [sym_sparql_base] = sym_sparql_base,
  [sym_sparql_prefix] = sym_sparql_prefix,
  [sym__triples] = sym__triples,
  [sym_property_list] = sym_property_list,
  [sym_property] = sym_property,
  [sym_object_list] = sym_object_list,
  [sym_predicate] = sym_predicate,
  [sym_subject] = sym_subject,
  [sym__object] = sym__object,
  [sym__literal] = sym__literal,
  [sym_blank_node_property_list] = sym_blank_node_property_list,
  [sym_collection] = sym_collection,
  [sym_object_collection] = sym_object_collection,
  [sym__numeric_literal] = sym__numeric_literal,
  [sym_string] = sym_string,
  [sym_iri_reference] = sym_iri_reference,
  [sym__string_literal_quote] = sym__string_literal_quote,
  [sym__string_literal_single_quote] = sym__string_literal_single_quote,
  [sym__string_literal_long_single_quote] = sym__string_literal_long_single_quote,
  [sym__string_literal_long_quote] = sym__string_literal_long_quote,
  [sym_rdf_literal] = sym_rdf_literal,
  [sym_boolean_literal] = sym_boolean_literal,
  [sym__iri] = sym__iri,
  [sym_prefixed_name] = sym_prefixed_name,
  [sym__blank_node] = sym__blank_node,
  [sym_namespace] = sym_namespace,
  [sym_blank_node_label] = sym_blank_node_label,
  [aux_sym_document_repeat1] = aux_sym_document_repeat1,
  [aux_sym_graph_repeat1] = aux_sym_graph_repeat1,
  [aux_sym_property_list_repeat1] = aux_sym_property_list_repeat1,
  [aux_sym_object_list_repeat1] = aux_sym_object_list_repeat1,
  [aux_sym_object_collection_repeat1] = aux_sym_object_collection_repeat1,
  [aux_sym__string_literal_quote_repeat1] = aux_sym__string_literal_quote_repeat1,
  [aux_sym__string_literal_single_quote_repeat1] = aux_sym__string_literal_single_quote_repeat1,
  [aux_sym__string_literal_long_single_quote_repeat1] = aux_sym__string_literal_long_single_quote_repeat1,
  [aux_sym__string_literal_long_quote_repeat1] = aux_sym__string_literal_long_quote_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym_pn_prefix] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GRAPH] = {
    .visible = true,
    .named = false,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ATprefix] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ATbase] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_sparql_base_token1] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_sparql_prefix_token1] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SEMI] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_a] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_POUND] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_iri_reference_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_GT] = {
    .visible = true,
    .named = false,
  },
  [sym_integer] = {
    .visible = true,
    .named = true,
  },
  [sym_decimal] = {
    .visible = true,
    .named = true,
  },
  [sym_double] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__string_literal_quote_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__string_literal_quote_token2] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_SQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__string_literal_single_quote_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_SQUOTE_SQUOTE_SQUOTE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SQUOTE_SQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__string_literal_long_single_quote_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DQUOTE_DQUOTE_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DQUOTE_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__string_literal_long_quote_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_CARET_CARET] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_true] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_false] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym__COLON] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_blank_node_label_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_lang_tag] = {
    .visible = true,
    .named = true,
  },
  [sym_echar] = {
    .visible = true,
    .named = true,
  },
  [sym_anon] = {
    .visible = true,
    .named = true,
  },
  [sym_pn_local] = {
    .visible = true,
    .named = true,
  },
  [sym_document] = {
    .visible = true,
    .named = true,
  },
  [sym_graph] = {
    .visible = true,
    .named = true,
  },
  [sym__label] = {
    .visible = false,
    .named = true,
  },
  [sym_triple] = {
    .visible = true,
    .named = true,
  },
  [sym_directive] = {
    .visible = true,
    .named = true,
  },
  [sym_prefix_id] = {
    .visible = true,
    .named = true,
  },
  [sym_base] = {
    .visible = true,
    .named = true,
  },
  [sym_sparql_base] = {
    .visible = true,
    .named = true,
  },
  [sym_sparql_prefix] = {
    .visible = true,
    .named = true,
  },
  [sym__triples] = {
    .visible = false,
    .named = true,
  },
  [sym_property_list] = {
    .visible = true,
    .named = true,
  },
  [sym_property] = {
    .visible = true,
    .named = true,
  },
  [sym_object_list] = {
    .visible = true,
    .named = true,
  },
  [sym_predicate] = {
    .visible = true,
    .named = true,
  },
  [sym_subject] = {
    .visible = true,
    .named = true,
  },
  [sym__object] = {
    .visible = false,
    .named = true,
  },
  [sym__literal] = {
    .visible = false,
    .named = true,
  },
  [sym_blank_node_property_list] = {
    .visible = true,
    .named = true,
  },
  [sym_collection] = {
    .visible = true,
    .named = true,
  },
  [sym_object_collection] = {
    .visible = true,
    .named = true,
  },
  [sym__numeric_literal] = {
    .visible = false,
    .named = true,
  },
  [sym_string] = {
    .visible = true,
    .named = true,
  },
  [sym_iri_reference] = {
    .visible = true,
    .named = true,
  },
  [sym__string_literal_quote] = {
    .visible = false,
    .named = true,
  },
  [sym__string_literal_single_quote] = {
    .visible = false,
    .named = true,
  },
  [sym__string_literal_long_single_quote] = {
    .visible = false,
    .named = true,
  },
  [sym__string_literal_long_quote] = {
    .visible = false,
    .named = true,
  },
  [sym_rdf_literal] = {
    .visible = true,
    .named = true,
  },
  [sym_boolean_literal] = {
    .visible = true,
    .named = true,
  },
  [sym__iri] = {
    .visible = false,
    .named = true,
  },
  [sym_prefixed_name] = {
    .visible = true,
    .named = true,
  },
  [sym__blank_node] = {
    .visible = false,
    .named = true,
  },
  [sym_namespace] = {
    .visible = true,
    .named = true,
  },
  [sym_blank_node_label] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_document_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_graph_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_property_list_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_object_list_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_object_collection_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__string_literal_quote_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__string_literal_single_quote_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__string_literal_long_single_quote_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__string_literal_long_quote_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum ts_field_identifiers {
  field_datatype = 1,
  field_label = 2,
  field_value = 3,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_datatype] = "datatype",
  [field_label] = "label",
  [field_value] = "value",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
  [2] = {.index = 1, .length = 1},
  [3] = {.index = 2, .length = 3},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_value, 0},
  [1] =
    {field_label, 0},
  [2] =
    {field_datatype, 1},
    {field_datatype, 2},
    {field_value, 0},
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
};

static const uint16_t ts_non_terminal_alias_map[] = {
  0,
};

static const TSStateId ts_primary_state_ids[STATE_COUNT] = {
  [0] = 0,
  [1] = 1,
  [2] = 2,
  [3] = 3,
  [4] = 4,
  [5] = 5,
  [6] = 6,
  [7] = 7,
  [8] = 8,
  [9] = 9,
  [10] = 10,
  [11] = 11,
  [12] = 12,
  [13] = 13,
  [14] = 14,
  [15] = 15,
  [16] = 16,
  [17] = 17,
  [18] = 18,
  [19] = 19,
  [20] = 20,
  [21] = 21,
  [22] = 22,
  [23] = 23,
  [24] = 24,
  [25] = 25,
  [26] = 26,
  [27] = 27,
  [28] = 28,
  [29] = 29,
  [30] = 30,
  [31] = 31,
  [32] = 32,
  [33] = 33,
  [34] = 34,
  [35] = 35,
  [36] = 36,
  [37] = 37,
  [38] = 38,
  [39] = 39,
  [40] = 40,
  [41] = 41,
  [42] = 42,
  [43] = 43,
  [44] = 44,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 50,
  [51] = 51,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 33,
  [58] = 58,
  [59] = 59,
  [60] = 56,
  [61] = 61,
  [62] = 34,
  [63] = 63,
  [64] = 36,
  [65] = 65,
  [66] = 35,
  [67] = 35,
  [68] = 68,
  [69] = 69,
  [70] = 34,
  [71] = 71,
  [72] = 36,
  [73] = 73,
  [74] = 74,
  [75] = 75,
  [76] = 76,
  [77] = 77,
  [78] = 78,
  [79] = 79,
  [80] = 80,
  [81] = 81,
  [82] = 82,
  [83] = 83,
  [84] = 84,
  [85] = 85,
  [86] = 86,
  [87] = 87,
  [88] = 88,
  [89] = 89,
  [90] = 90,
  [91] = 91,
  [92] = 92,
  [93] = 93,
  [94] = 94,
  [95] = 95,
  [96] = 96,
  [97] = 97,
  [98] = 98,
  [99] = 99,
  [100] = 100,
  [101] = 101,
  [102] = 102,
  [103] = 103,
  [104] = 104,
  [105] = 105,
  [106] = 106,
  [107] = 107,
  [108] = 108,
  [109] = 109,
  [110] = 110,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 110,
  [115] = 115,
  [116] = 110,
  [117] = 117,
};

static TSCharacterRange aux_sym_blank_node_label_token1_character_set_1[] = {
  {'0', '9'}, {'A', 'Z'}, {'_', '_'}, {'a', 'z'}, {0xc0, 0xd6}, {0xd8, 0xf6}, {0xf8, 0x2ff}, {0x370, 0x37d},
  {0x37f, 0x1fff}, {0x200c, 0x200d}, {0x2070, 0x218f}, {0x2c00, 0x2fef}, {0x3001, 0xd7ff}, {0xf900, 0xfdcf}, {0xfdf0, 0xfffd}, {0x10000, 0xeffff},
};

static TSCharacterRange aux_sym_blank_node_label_token1_character_set_2[] = {
  {'-', '.'}, {'0', '9'}, {'A', 'Z'}, {'_', '_'}, {'a', 'z'}, {0xb7, 0xb7}, {0xc0, 0xd6}, {0xd8, 0xf6},
  {0xf8, 0x37d}, {0x37f, 0x1fff}, {0x200c, 0x200d}, {0x203f, 0x2040}, {0x2070, 0x218f}, {0x2c00, 0x2fef}, {0x3001, 0xd7ff}, {0xf900, 0xfdcf},
  {0xfdf0, 0xfffd}, {0x10000, 0xeffff},
};

static TSCharacterRange sym_pn_prefix_character_set_1[] = {
  {'A', 'Z'}, {'a', 'z'}, {0xc0, 0xd6}, {0xd8, 0xf6}, {0xf8, 0x2ff}, {0x370, 0x37d}, {0x37f, 0x1fff}, {0x200c, 0x200d},
  {0x2070, 0x218f}, {0x2c00, 0x2fef}, {0x3001, 0xd7ff}, {0xf900, 0xfdcf}, {0xfdf0, 0xfffd}, {0x10000, 0xeffff},
};

static TSCharacterRange sym_pn_local_character_set_1[] = {
  {'%', '%'}, {'0', ':'}, {'A', 'Z'}, {'\\', '\\'}, {'_', '_'}, {'a', 'z'}, {0xc0, 0xd6}, {0xd8, 0xf6},
  {0xf8, 0x2ff}, {0x370, 0x37d}, {0x37f, 0x1fff}, {0x200c, 0x200d}, {0x2070, 0x218f}, {0x2c00, 0x2fef}, {0x3001, 0xd7ff}, {0xf900, 0xfdcf},
  {0xfdf0, 0xfffd}, {0x10000, 0xeffff},
};

static TSCharacterRange sym_pn_local_character_set_2[] = {
  {'%', '%'}, {'-', '.'}, {'0', ':'}, {'A', 'Z'}, {'\\', '\\'}, {'_', '_'}, {'a', 'z'}, {0xb7, 0xb7},
  {0xc0, 0xd6}, {0xd8, 0xf6}, {0xf8, 0x37d}, {0x37f, 0x1fff}, {0x200c, 0x200d}, {0x203f, 0x2040}, {0x2070, 0x218f}, {0x2c00, 0x2fef},
  {0x3001, 0xd7ff}, {0xf900, 0xfdcf}, {0xfdf0, 0xfffd}, {0x10000, 0xeffff},
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(67);
      ADVANCE_MAP(
        '"', 101,
        '#', 91,
        '\'', 107,
        '(', 88,
        ')', 89,
        ',', 85,
        '.', 81,
        ':', 120,
        ';', 84,
        '<', 90,
        '>', 96,
        '@', 30,
        '[', 86,
        '\\', 25,
        ']', 87,
        '^', 28,
        '_', 24,
        '{', 68,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(63);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 1:
      if (lookahead == '"') ADVANCE(101);
      if (lookahead == '#') ADVANCE(117);
      if (lookahead == '\\') ADVANCE(25);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(118);
      if (lookahead != 0) ADVANCE(117);
      END_STATE();
    case 2:
      if (lookahead == '"') ADVANCE(115);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(100);
      if (lookahead == '#') ADVANCE(103);
      if (lookahead == '\\') ADVANCE(25);
      if (lookahead == '\n' ||
          lookahead == '\r') SKIP(3);
      if (('\t' <= lookahead && lookahead <= '\f') ||
          lookahead == ' ') ADVANCE(104);
      if (lookahead != 0) ADVANCE(103);
      END_STATE();
    case 4:
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '%', 47,
        '\'', 108,
        '(', 88,
        ')', 89,
        '.', 40,
        ':', 121,
        '<', 90,
        '[', 86,
        '\\', 42,
        '_', 131,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(135);
      if (set_contains(sym_pn_local_character_set_1, 18, lookahead)) ADVANCE(136);
      END_STATE();
    case 5:
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        ',', 85,
        '.', 81,
        ':', 120,
        ';', 84,
        '<', 90,
        '@', 61,
        '[', 86,
        ']', 87,
        '^', 28,
        '_', 24,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(5);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 6:
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        '.', 40,
        ':', 120,
        '<', 90,
        '@', 61,
        '[', 86,
        '^', 28,
        '_', 24,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(6);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 7:
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        '.', 40,
        ':', 120,
        '<', 90,
        '[', 86,
        '_', 24,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 8:
      ADVANCE_MAP(
        '#', 79,
        '%', 47,
        ',', 85,
        '.', 80,
        ';', 84,
        '\\', 42,
        ']', 87,
        '{', 68,
        '}', 69,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11);
      if (set_contains(sym_pn_local_character_set_1, 18, lookahead)) ADVANCE(134);
      END_STATE();
    case 9:
      if (lookahead == '#') ADVANCE(79);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == ':') ADVANCE(121);
      if (lookahead == '<') ADVANCE(90);
      if (lookahead == '\\') ADVANCE(42);
      if (lookahead == '{') ADVANCE(68);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(12);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(134);
      if (set_contains(sym_pn_local_character_set_1, 18, lookahead)) ADVANCE(136);
      END_STATE();
    case 10:
      ADVANCE_MAP(
        '#', 79,
        ',', 85,
        '.', 80,
        ':', 120,
        ';', 84,
        '<', 90,
        '@', 61,
        ']', 87,
        '^', 28,
        '}', 69,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(10);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 11:
      if (lookahead == '#') ADVANCE(79);
      if (lookahead == ',') ADVANCE(85);
      if (lookahead == '.') ADVANCE(80);
      if (lookahead == ';') ADVANCE(84);
      if (lookahead == ']') ADVANCE(87);
      if (lookahead == '{') ADVANCE(68);
      if (lookahead == '}') ADVANCE(69);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11);
      END_STATE();
    case 12:
      if (lookahead == '#') ADVANCE(79);
      if (lookahead == ':') ADVANCE(120);
      if (lookahead == '<') ADVANCE(90);
      if (lookahead == '{') ADVANCE(68);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(12);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 13:
      if (lookahead == '#') ADVANCE(79);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(13);
      END_STATE();
    case 14:
      if (lookahead == '#') ADVANCE(79);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(13);
      if (set_contains(aux_sym_blank_node_label_token1_character_set_1, 16, lookahead)) ADVANCE(124);
      END_STATE();
    case 15:
      if (lookahead == '#') ADVANCE(113);
      if (lookahead == '\'') ADVANCE(107);
      if (lookahead == '\\') ADVANCE(25);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(114);
      if (lookahead != 0) ADVANCE(113);
      END_STATE();
    case 16:
      if (lookahead == '#') ADVANCE(109);
      if (lookahead == '\'') ADVANCE(106);
      if (lookahead == '\\') ADVANCE(25);
      if (lookahead == '\n' ||
          lookahead == '\r') SKIP(16);
      if (('\t' <= lookahead && lookahead <= '\f') ||
          lookahead == ' ') ADVANCE(110);
      if (lookahead != 0) ADVANCE(109);
      END_STATE();
    case 17:
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(130);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(132);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 18:
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 19:
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(19);
      if (lookahead == ':') ADVANCE(134);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(136);
      END_STATE();
    case 20:
      if (lookahead == '\'') ADVANCE(111);
      END_STATE();
    case 21:
      if (lookahead == '.') ADVANCE(40);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      END_STATE();
    case 22:
      if (lookahead == '.') ADVANCE(22);
      if (set_contains(aux_sym_blank_node_label_token1_character_set_2, 18, lookahead)) ADVANCE(129);
      END_STATE();
    case 23:
      if (lookahead == '.') ADVANCE(23);
      if (set_contains(aux_sym_blank_node_label_token1_character_set_2, 18, lookahead)) ADVANCE(124);
      END_STATE();
    case 24:
      if (lookahead == ':') ADVANCE(122);
      END_STATE();
    case 25:
      ADVANCE_MAP(
        'U', 59,
        'u', 51,
        '"', 127,
        '\'', 127,
        '\\', 127,
        'b', 127,
        'f', 127,
        'n', 127,
        'r', 127,
        't', 127,
      );
      END_STATE();
    case 26:
      if (lookahead == 'U') ADVANCE(60);
      if (lookahead == 'u') ADVANCE(52);
      END_STATE();
    case 27:
      if (lookahead == ']') ADVANCE(128);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(27);
      END_STATE();
    case 28:
      if (lookahead == '^') ADVANCE(119);
      END_STATE();
    case 29:
      if (lookahead == 'a') ADVANCE(36);
      END_STATE();
    case 30:
      if (lookahead == 'b') ADVANCE(29);
      if (lookahead == 'p') ADVANCE(35);
      END_STATE();
    case 31:
      if (lookahead == 'e') ADVANCE(33);
      END_STATE();
    case 32:
      if (lookahead == 'e') ADVANCE(83);
      END_STATE();
    case 33:
      if (lookahead == 'f') ADVANCE(34);
      END_STATE();
    case 34:
      if (lookahead == 'i') ADVANCE(37);
      END_STATE();
    case 35:
      if (lookahead == 'r') ADVANCE(31);
      END_STATE();
    case 36:
      if (lookahead == 's') ADVANCE(32);
      END_STATE();
    case 37:
      if (lookahead == 'x') ADVANCE(82);
      END_STATE();
    case 38:
      if (lookahead == '+' ||
          lookahead == '-') ADVANCE(41);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(99);
      END_STATE();
    case 39:
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(38);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(98);
      END_STATE();
    case 40:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(98);
      END_STATE();
    case 41:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(99);
      END_STATE();
    case 42:
      if (lookahead == '!' ||
          ('#' <= lookahead && lookahead <= '/') ||
          lookahead == ';' ||
          lookahead == '=' ||
          lookahead == '?' ||
          lookahead == '@' ||
          lookahead == '_' ||
          lookahead == '~') ADVANCE(134);
      END_STATE();
    case 43:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(105);
      END_STATE();
    case 44:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(134);
      END_STATE();
    case 45:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(94);
      END_STATE();
    case 46:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(43);
      END_STATE();
    case 47:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(44);
      END_STATE();
    case 48:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(45);
      END_STATE();
    case 49:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(46);
      END_STATE();
    case 50:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(48);
      END_STATE();
    case 51:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(49);
      END_STATE();
    case 52:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(50);
      END_STATE();
    case 53:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(51);
      END_STATE();
    case 54:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(52);
      END_STATE();
    case 55:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(53);
      END_STATE();
    case 56:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(54);
      END_STATE();
    case 57:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(55);
      END_STATE();
    case 58:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(56);
      END_STATE();
    case 59:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(57);
      END_STATE();
    case 60:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(58);
      END_STATE();
    case 61:
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(125);
      END_STATE();
    case 62:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(126);
      END_STATE();
    case 63:
      if (eof) ADVANCE(67);
      ADVANCE_MAP(
        '"', 101,
        '#', 91,
        '\'', 107,
        '(', 88,
        ')', 89,
        ',', 85,
        '.', 81,
        ':', 120,
        ';', 84,
        '<', 90,
        '@', 30,
        '[', 86,
        '\\', 25,
        ']', 87,
        '^', 28,
        '_', 24,
        '{', 68,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(63);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 64:
      if (eof) ADVANCE(67);
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        ',', 85,
        '.', 81,
        ':', 120,
        ';', 84,
        '<', 90,
        '@', 30,
        '[', 86,
        ']', 87,
        '_', 24,
        '{', 68,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(64);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 65:
      if (eof) ADVANCE(67);
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        '.', 40,
        ':', 120,
        '<', 90,
        '>', 96,
        '@', 30,
        '[', 86,
        ']', 87,
        '_', 24,
        '{', 68,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(66);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 66:
      if (eof) ADVANCE(67);
      ADVANCE_MAP(
        '"', 102,
        '#', 79,
        '\'', 108,
        '(', 88,
        ')', 89,
        '.', 40,
        ':', 120,
        '<', 90,
        '@', 30,
        '[', 86,
        ']', 87,
        '_', 24,
        '{', 68,
        '}', 69,
        '+', 21,
        '-', 21,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(66);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      if (set_contains(sym_pn_prefix_character_set_1, 14, lookahead)) ADVANCE(129);
      END_STATE();
    case 67:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 68:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 69:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 70:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead == 'U') ADVANCE(78);
      if (lookahead == 'u') ADVANCE(74);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 71:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(95);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 72:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(71);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 73:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(72);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 74:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(73);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 75:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(74);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 76:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(75);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 77:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(76);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 78:
      ACCEPT_TOKEN(sym_comment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(77);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 79:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(79);
      END_STATE();
    case 80:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 81:
      ACCEPT_TOKEN(anon_sym_DOT);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(98);
      END_STATE();
    case 82:
      ACCEPT_TOKEN(anon_sym_ATprefix);
      END_STATE();
    case 83:
      ACCEPT_TOKEN(anon_sym_ATbase);
      END_STATE();
    case 84:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 86:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      if (lookahead == ']') ADVANCE(128);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(27);
      END_STATE();
    case 87:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 88:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 89:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 90:
      ACCEPT_TOKEN(anon_sym_LT);
      END_STATE();
    case 91:
      ACCEPT_TOKEN(anon_sym_POUND);
      END_STATE();
    case 92:
      ACCEPT_TOKEN(aux_sym_iri_reference_token1);
      if (lookahead == '#') ADVANCE(91);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead > ' ' &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '<' &&
          lookahead != '>' &&
          lookahead != '^' &&
          lookahead != '`' &&
          (lookahead < '{' || '}' < lookahead)) ADVANCE(94);
      END_STATE();
    case 93:
      ACCEPT_TOKEN(aux_sym_iri_reference_token1);
      if (lookahead == '#') ADVANCE(95);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead > ' ' &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '<' &&
          lookahead != '>' &&
          lookahead != '^' &&
          lookahead != '`' &&
          (lookahead < '{' || '}' < lookahead)) ADVANCE(94);
      END_STATE();
    case 94:
      ACCEPT_TOKEN(aux_sym_iri_reference_token1);
      if (lookahead == '\\') ADVANCE(26);
      if (lookahead > ' ' &&
          lookahead != '"' &&
          lookahead != '<' &&
          lookahead != '>' &&
          lookahead != '^' &&
          lookahead != '`' &&
          (lookahead < '{' || '}' < lookahead)) ADVANCE(94);
      END_STATE();
    case 95:
      ACCEPT_TOKEN(aux_sym_iri_reference_token1);
      if (lookahead == '\\') ADVANCE(70);
      if (lookahead > ' ' &&
          lookahead != '"' &&
          lookahead != '<' &&
          lookahead != '>' &&
          lookahead != '^' &&
          lookahead != '`' &&
          (lookahead < '{' || '}' < lookahead)) ADVANCE(95);
      END_STATE();
    case 96:
      ACCEPT_TOKEN(anon_sym_GT);
      END_STATE();
    case 97:
      ACCEPT_TOKEN(sym_integer);
      if (lookahead == '.') ADVANCE(39);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(38);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      END_STATE();
    case 98:
      ACCEPT_TOKEN(sym_decimal);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(38);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(98);
      END_STATE();
    case 99:
      ACCEPT_TOKEN(sym_double);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(99);
      END_STATE();
    case 100:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 101:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      if (lookahead == '"') ADVANCE(116);
      END_STATE();
    case 102:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      if (lookahead == '"') ADVANCE(2);
      END_STATE();
    case 103:
      ACCEPT_TOKEN(aux_sym__string_literal_quote_token1);
      END_STATE();
    case 104:
      ACCEPT_TOKEN(aux_sym__string_literal_quote_token1);
      if (lookahead == '#') ADVANCE(103);
      if (lookahead == '\t' ||
          lookahead == 0x0b ||
          lookahead == '\f' ||
          lookahead == ' ') ADVANCE(104);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead) &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '\\') ADVANCE(103);
      END_STATE();
    case 105:
      ACCEPT_TOKEN(aux_sym__string_literal_quote_token2);
      END_STATE();
    case 106:
      ACCEPT_TOKEN(anon_sym_SQUOTE);
      END_STATE();
    case 107:
      ACCEPT_TOKEN(anon_sym_SQUOTE);
      if (lookahead == '\'') ADVANCE(112);
      END_STATE();
    case 108:
      ACCEPT_TOKEN(anon_sym_SQUOTE);
      if (lookahead == '\'') ADVANCE(20);
      END_STATE();
    case 109:
      ACCEPT_TOKEN(aux_sym__string_literal_single_quote_token1);
      END_STATE();
    case 110:
      ACCEPT_TOKEN(aux_sym__string_literal_single_quote_token1);
      if (lookahead == '#') ADVANCE(109);
      if (lookahead == '\t' ||
          lookahead == 0x0b ||
          lookahead == '\f' ||
          lookahead == ' ') ADVANCE(110);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead) &&
          lookahead != '\'' &&
          lookahead != '\\') ADVANCE(109);
      END_STATE();
    case 111:
      ACCEPT_TOKEN(anon_sym_SQUOTE_SQUOTE_SQUOTE);
      END_STATE();
    case 112:
      ACCEPT_TOKEN(anon_sym_SQUOTE_SQUOTE);
      if (lookahead == '\'') ADVANCE(111);
      END_STATE();
    case 113:
      ACCEPT_TOKEN(aux_sym__string_literal_long_single_quote_token1);
      END_STATE();
    case 114:
      ACCEPT_TOKEN(aux_sym__string_literal_long_single_quote_token1);
      if (lookahead == '#') ADVANCE(113);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(114);
      if (lookahead != 0 &&
          lookahead != '\'' &&
          lookahead != '\\') ADVANCE(113);
      END_STATE();
    case 115:
      ACCEPT_TOKEN(anon_sym_DQUOTE_DQUOTE_DQUOTE);
      END_STATE();
    case 116:
      ACCEPT_TOKEN(anon_sym_DQUOTE_DQUOTE);
      if (lookahead == '"') ADVANCE(115);
      END_STATE();
    case 117:
      ACCEPT_TOKEN(aux_sym__string_literal_long_quote_token1);
      END_STATE();
    case 118:
      ACCEPT_TOKEN(aux_sym__string_literal_long_quote_token1);
      if (lookahead == '#') ADVANCE(117);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(118);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '\\') ADVANCE(117);
      END_STATE();
    case 119:
      ACCEPT_TOKEN(anon_sym_CARET_CARET);
      END_STATE();
    case 120:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 121:
      ACCEPT_TOKEN(anon_sym_COLON);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 122:
      ACCEPT_TOKEN(anon_sym__COLON);
      END_STATE();
    case 123:
      ACCEPT_TOKEN(anon_sym__COLON);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 124:
      ACCEPT_TOKEN(aux_sym_blank_node_label_token1);
      if (lookahead == '.') ADVANCE(23);
      if (set_contains(aux_sym_blank_node_label_token1_character_set_2, 18, lookahead)) ADVANCE(124);
      END_STATE();
    case 125:
      ACCEPT_TOKEN(sym_lang_tag);
      if (lookahead == '-') ADVANCE(62);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(125);
      END_STATE();
    case 126:
      ACCEPT_TOKEN(sym_lang_tag);
      if (lookahead == '-') ADVANCE(62);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(126);
      END_STATE();
    case 127:
      ACCEPT_TOKEN(sym_echar);
      END_STATE();
    case 128:
      ACCEPT_TOKEN(sym_anon);
      END_STATE();
    case 129:
      ACCEPT_TOKEN(sym_pn_prefix);
      if (lookahead == '.') ADVANCE(22);
      if (set_contains(aux_sym_blank_node_label_token1_character_set_2, 18, lookahead)) ADVANCE(129);
      END_STATE();
    case 130:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '+') ADVANCE(41);
      if (lookahead == '-') ADVANCE(133);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(133);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 131:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == ':') ADVANCE(123);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 132:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(130);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(132);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 133:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(133);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 134:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(18);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 135:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(17);
      if (lookahead == '\\') ADVANCE(42);
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(130);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(135);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(134);
      END_STATE();
    case 136:
      ACCEPT_TOKEN(sym_pn_local);
      if (lookahead == '%') ADVANCE(47);
      if (lookahead == '.') ADVANCE(19);
      if (lookahead == ':') ADVANCE(134);
      if (lookahead == '\\') ADVANCE(42);
      if (set_contains(sym_pn_local_character_set_2, 20, lookahead)) ADVANCE(136);
      END_STATE();
    default:
      return false;
  }
}

static bool ts_lex_keywords(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      ADVANCE_MAP(
        'G', 1,
        'a', 2,
        'f', 3,
        't', 4,
        'B', 5,
        'b', 5,
        'P', 6,
        'p', 6,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0);
      END_STATE();
    case 1:
      if (lookahead == 'R') ADVANCE(7);
      END_STATE();
    case 2:
      ACCEPT_TOKEN(anon_sym_a);
      END_STATE();
    case 3:
      if (lookahead == 'a') ADVANCE(8);
      END_STATE();
    case 4:
      if (lookahead == 'r') ADVANCE(9);
      END_STATE();
    case 5:
      if (lookahead == 'A' ||
          lookahead == 'a') ADVANCE(10);
      END_STATE();
    case 6:
      if (lookahead == 'R' ||
          lookahead == 'r') ADVANCE(11);
      END_STATE();
    case 7:
      if (lookahead == 'A') ADVANCE(12);
      END_STATE();
    case 8:
      if (lookahead == 'l') ADVANCE(13);
      END_STATE();
    case 9:
      if (lookahead == 'u') ADVANCE(14);
      END_STATE();
    case 10:
      if (lookahead == 'S' ||
          lookahead == 's') ADVANCE(15);
      END_STATE();
    case 11:
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(16);
      END_STATE();
    case 12:
      if (lookahead == 'P') ADVANCE(17);
      END_STATE();
    case 13:
      if (lookahead == 's') ADVANCE(18);
      END_STATE();
    case 14:
      if (lookahead == 'e') ADVANCE(19);
      END_STATE();
    case 15:
      if (lookahead == 'E' ||
          lookahead == 'e') ADVANCE(20);
      END_STATE();
    case 16:
      if (lookahead == 'F' ||
          lookahead == 'f') ADVANCE(21);
      END_STATE();
    case 17:
      if (lookahead == 'H') ADVANCE(22);
      END_STATE();
    case 18:
      if (lookahead == 'e') ADVANCE(23);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(anon_sym_true);
      END_STATE();
    case 20:
      ACCEPT_TOKEN(aux_sym_sparql_base_token1);
      END_STATE();
    case 21:
      if (lookahead == 'I' ||
          lookahead == 'i') ADVANCE(24);
      END_STATE();
    case 22:
      ACCEPT_TOKEN(anon_sym_GRAPH);
      END_STATE();
    case 23:
      ACCEPT_TOKEN(anon_sym_false);
      END_STATE();
    case 24:
      if (lookahead == 'X' ||
          lookahead == 'x') ADVANCE(25);
      END_STATE();
    case 25:
      ACCEPT_TOKEN(aux_sym_sparql_prefix_token1);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 65},
  [2] = {.lex_state = 65},
  [3] = {.lex_state = 65},
  [4] = {.lex_state = 65},
  [5] = {.lex_state = 65},
  [6] = {.lex_state = 65},
  [7] = {.lex_state = 65},
  [8] = {.lex_state = 65},
  [9] = {.lex_state = 64},
  [10] = {.lex_state = 64},
  [11] = {.lex_state = 64},
  [12] = {.lex_state = 5},
  [13] = {.lex_state = 5},
  [14] = {.lex_state = 5},
  [15] = {.lex_state = 5},
  [16] = {.lex_state = 5},
  [17] = {.lex_state = 64},
  [18] = {.lex_state = 5},
  [19] = {.lex_state = 5},
  [20] = {.lex_state = 5},
  [21] = {.lex_state = 5},
  [22] = {.lex_state = 64},
  [23] = {.lex_state = 64},
  [24] = {.lex_state = 64},
  [25] = {.lex_state = 64},
  [26] = {.lex_state = 64},
  [27] = {.lex_state = 64},
  [28] = {.lex_state = 65},
  [29] = {.lex_state = 65},
  [30] = {.lex_state = 65},
  [31] = {.lex_state = 65},
  [32] = {.lex_state = 65},
  [33] = {.lex_state = 6},
  [34] = {.lex_state = 4},
  [35] = {.lex_state = 4},
  [36] = {.lex_state = 4},
  [37] = {.lex_state = 65},
  [38] = {.lex_state = 65},
  [39] = {.lex_state = 10},
  [40] = {.lex_state = 65},
  [41] = {.lex_state = 65},
  [42] = {.lex_state = 65},
  [43] = {.lex_state = 65},
  [44] = {.lex_state = 65},
  [45] = {.lex_state = 65},
  [46] = {.lex_state = 65},
  [47] = {.lex_state = 65},
  [48] = {.lex_state = 65},
  [49] = {.lex_state = 65},
  [50] = {.lex_state = 65},
  [51] = {.lex_state = 10},
  [52] = {.lex_state = 65},
  [53] = {.lex_state = 65},
  [54] = {.lex_state = 65},
  [55] = {.lex_state = 1},
  [56] = {.lex_state = 65},
  [57] = {.lex_state = 10},
  [58] = {.lex_state = 15},
  [59] = {.lex_state = 1},
  [60] = {.lex_state = 65},
  [61] = {.lex_state = 1},
  [62] = {.lex_state = 8},
  [63] = {.lex_state = 15},
  [64] = {.lex_state = 8},
  [65] = {.lex_state = 15},
  [66] = {.lex_state = 8},
  [67] = {.lex_state = 9},
  [68] = {.lex_state = 15},
  [69] = {.lex_state = 10},
  [70] = {.lex_state = 9},
  [71] = {.lex_state = 10},
  [72] = {.lex_state = 9},
  [73] = {.lex_state = 10},
  [74] = {.lex_state = 1},
  [75] = {.lex_state = 10},
  [76] = {.lex_state = 3},
  [77] = {.lex_state = 10},
  [78] = {.lex_state = 3},
  [79] = {.lex_state = 65},
  [80] = {.lex_state = 10},
  [81] = {.lex_state = 10},
  [82] = {.lex_state = 16},
  [83] = {.lex_state = 3},
  [84] = {.lex_state = 16},
  [85] = {.lex_state = 16},
  [86] = {.lex_state = 65},
  [87] = {.lex_state = 10},
  [88] = {.lex_state = 10},
  [89] = {.lex_state = 65},
  [90] = {.lex_state = 65},
  [91] = {.lex_state = 15},
  [92] = {.lex_state = 1},
  [93] = {.lex_state = 10},
  [94] = {.lex_state = 10},
  [95] = {.lex_state = 65},
  [96] = {.lex_state = 65},
  [97] = {.lex_state = 10},
  [98] = {.lex_state = 92},
  [99] = {.lex_state = 10},
  [100] = {.lex_state = 65},
  [101] = {.lex_state = 65},
  [102] = {.lex_state = 10},
  [103] = {.lex_state = 65},
  [104] = {.lex_state = 65},
  [105] = {.lex_state = 65},
  [106] = {.lex_state = 10},
  [107] = {.lex_state = 10},
  [108] = {.lex_state = 65},
  [109] = {.lex_state = 65},
  [110] = {.lex_state = 65},
  [111] = {.lex_state = 14},
  [112] = {.lex_state = 65},
  [113] = {.lex_state = 10},
  [114] = {.lex_state = 65},
  [115] = {.lex_state = 65},
  [116] = {.lex_state = 65},
  [117] = {.lex_state = 93},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym_pn_prefix] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_GRAPH] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
    [anon_sym_DOT] = ACTIONS(1),
    [anon_sym_ATprefix] = ACTIONS(1),
    [anon_sym_ATbase] = ACTIONS(1),
    [aux_sym_sparql_base_token1] = ACTIONS(1),
    [aux_sym_sparql_prefix_token1] = ACTIONS(1),
    [anon_sym_SEMI] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_a] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_LT] = ACTIONS(1),
    [anon_sym_POUND] = ACTIONS(1),
    [anon_sym_GT] = ACTIONS(1),
    [sym_integer] = ACTIONS(1),
    [sym_decimal] = ACTIONS(1),
    [sym_double] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [aux_sym__string_literal_quote_token2] = ACTIONS(1),
    [anon_sym_SQUOTE] = ACTIONS(1),
    [anon_sym_SQUOTE_SQUOTE_SQUOTE] = ACTIONS(1),
    [anon_sym_SQUOTE_SQUOTE] = ACTIONS(1),
    [anon_sym_DQUOTE_DQUOTE_DQUOTE] = ACTIONS(1),
    [anon_sym_DQUOTE_DQUOTE] = ACTIONS(1),
    [anon_sym_CARET_CARET] = ACTIONS(1),
    [anon_sym_true] = ACTIONS(1),
    [anon_sym_false] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym__COLON] = ACTIONS(1),
    [sym_echar] = ACTIONS(1),
    [sym_anon] = ACTIONS(1),
  },
  [1] = {
    [sym_document] = STATE(109),
    [sym_graph] = STATE(7),
    [sym__label] = STATE(108),
    [sym_triple] = STATE(7),
    [sym_directive] = STATE(7),
    [sym_prefix_id] = STATE(40),
    [sym_base] = STATE(40),
    [sym_sparql_base] = STATE(40),
    [sym_sparql_prefix] = STATE(40),
    [sym__triples] = STATE(107),
    [sym_subject] = STATE(53),
    [sym_blank_node_property_list] = STATE(51),
    [sym_collection] = STATE(86),
    [sym_iri_reference] = STATE(79),
    [sym__iri] = STATE(79),
    [sym_prefixed_name] = STATE(79),
    [sym__blank_node] = STATE(79),
    [sym_namespace] = STATE(67),
    [sym_blank_node_label] = STATE(79),
    [aux_sym_document_repeat1] = STATE(7),
    [ts_builtin_sym_end] = ACTIONS(5),
    [sym_pn_prefix] = ACTIONS(7),
    [anon_sym_LBRACE] = ACTIONS(9),
    [anon_sym_GRAPH] = ACTIONS(11),
    [sym_comment] = ACTIONS(13),
    [anon_sym_ATprefix] = ACTIONS(15),
    [anon_sym_ATbase] = ACTIONS(17),
    [aux_sym_sparql_base_token1] = ACTIONS(19),
    [aux_sym_sparql_prefix_token1] = ACTIONS(21),
    [anon_sym_LBRACK] = ACTIONS(23),
    [anon_sym_LPAREN] = ACTIONS(25),
    [anon_sym_LT] = ACTIONS(27),
    [anon_sym_COLON] = ACTIONS(29),
    [anon_sym__COLON] = ACTIONS(31),
    [sym_anon] = ACTIONS(33),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 20,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(37), 1,
      anon_sym_RPAREN,
    ACTIONS(43), 1,
      anon_sym_DQUOTE,
    ACTIONS(45), 1,
      anon_sym_SQUOTE,
    ACTIONS(47), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(49), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(53), 1,
      anon_sym_COLON,
    STATE(33), 1,
      sym_string,
    STATE(35), 1,
      sym_namespace,
    STATE(115), 1,
      sym_object_collection,
    ACTIONS(39), 2,
      sym_integer,
      sym_decimal,
    ACTIONS(41), 2,
      sym_double,
      sym_anon,
    ACTIONS(51), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(18), 4,
      sym__string_literal_quote,
      sym__string_literal_single_quote,
      sym__string_literal_long_single_quote,
      sym__string_literal_long_quote,
    STATE(4), 13,
      sym__object,
      sym__literal,
      sym_blank_node_property_list,
      sym_collection,
      sym__numeric_literal,
      sym_iri_reference,
      sym_rdf_literal,
      sym_boolean_literal,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
      aux_sym_object_collection_repeat1,
  [79] = 19,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(55), 1,
      sym_pn_prefix,
    ACTIONS(58), 1,
      anon_sym_LBRACK,
    ACTIONS(61), 1,
      anon_sym_LPAREN,
    ACTIONS(64), 1,
      anon_sym_RPAREN,
    ACTIONS(66), 1,
      anon_sym_LT,
    ACTIONS(75), 1,
      anon_sym_DQUOTE,
    ACTIONS(78), 1,
      anon_sym_SQUOTE,
    ACTIONS(81), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(84), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(90), 1,
      anon_sym_COLON,
    ACTIONS(93), 1,
      anon_sym__COLON,
    STATE(33), 1,
      sym_string,
    STATE(35), 1,
      sym_namespace,
    ACTIONS(69), 2,
      sym_integer,
      sym_decimal,
    ACTIONS(72), 2,
      sym_double,
      sym_anon,
    ACTIONS(87), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(18), 4,
      sym__string_literal_quote,
      sym__string_literal_single_quote,
      sym__string_literal_long_single_quote,
      sym__string_literal_long_quote,
    STATE(3), 13,
      sym__object,
      sym__literal,
      sym_blank_node_property_list,
      sym_collection,
      sym__numeric_literal,
      sym_iri_reference,
      sym_rdf_literal,
      sym_boolean_literal,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
      aux_sym_object_collection_repeat1,
  [155] = 19,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(43), 1,
      anon_sym_DQUOTE,
    ACTIONS(45), 1,
      anon_sym_SQUOTE,
    ACTIONS(47), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(49), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(96), 1,
      anon_sym_RPAREN,
    STATE(33), 1,
      sym_string,
    STATE(35), 1,
      sym_namespace,
    ACTIONS(51), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(98), 2,
      sym_integer,
      sym_decimal,
    ACTIONS(100), 2,
      sym_double,
      sym_anon,
    STATE(18), 4,
      sym__string_literal_quote,
      sym__string_literal_single_quote,
      sym__string_literal_long_single_quote,
      sym__string_literal_long_quote,
    STATE(3), 13,
      sym__object,
      sym__literal,
      sym_blank_node_property_list,
      sym_collection,
      sym__numeric_literal,
      sym_iri_reference,
      sym_rdf_literal,
      sym_boolean_literal,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
      aux_sym_object_collection_repeat1,
  [231] = 19,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(43), 1,
      anon_sym_DQUOTE,
    ACTIONS(45), 1,
      anon_sym_SQUOTE,
    ACTIONS(47), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(49), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(102), 1,
      sym_pn_prefix,
    ACTIONS(108), 1,
      anon_sym_COLON,
    STATE(57), 1,
      sym_string,
    STATE(66), 1,
      sym_namespace,
    STATE(87), 1,
      sym_object_list,
    ACTIONS(51), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(104), 2,
      sym_integer,
      sym_decimal,
    ACTIONS(106), 2,
      sym_double,
      sym_anon,
    STATE(18), 4,
      sym__string_literal_quote,
      sym__string_literal_single_quote,
      sym__string_literal_long_single_quote,
      sym__string_literal_long_quote,
    STATE(69), 12,
      sym__object,
      sym__literal,
      sym_blank_node_property_list,
      sym_collection,
      sym__numeric_literal,
      sym_iri_reference,
      sym_rdf_literal,
      sym_boolean_literal,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [306] = 18,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(43), 1,
      anon_sym_DQUOTE,
    ACTIONS(45), 1,
      anon_sym_SQUOTE,
    ACTIONS(47), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(49), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(102), 1,
      sym_pn_prefix,
    ACTIONS(108), 1,
      anon_sym_COLON,
    STATE(57), 1,
      sym_string,
    STATE(66), 1,
      sym_namespace,
    ACTIONS(51), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(110), 2,
      sym_integer,
      sym_decimal,
    ACTIONS(112), 2,
      sym_double,
      sym_anon,
    STATE(18), 4,
      sym__string_literal_quote,
      sym__string_literal_single_quote,
      sym__string_literal_long_single_quote,
      sym__string_literal_long_quote,
    STATE(77), 12,
      sym__object,
      sym__literal,
      sym_blank_node_property_list,
      sym_collection,
      sym__numeric_literal,
      sym_iri_reference,
      sym_rdf_literal,
      sym_boolean_literal,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [378] = 24,
    ACTIONS(7), 1,
      sym_pn_prefix,
    ACTIONS(9), 1,
      anon_sym_LBRACE,
    ACTIONS(11), 1,
      anon_sym_GRAPH,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_ATprefix,
    ACTIONS(17), 1,
      anon_sym_ATbase,
    ACTIONS(19), 1,
      aux_sym_sparql_base_token1,
    ACTIONS(21), 1,
      aux_sym_sparql_prefix_token1,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(33), 1,
      sym_anon,
    ACTIONS(114), 1,
      ts_builtin_sym_end,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(86), 1,
      sym_collection,
    STATE(107), 1,
      sym__triples,
    STATE(108), 1,
      sym__label,
    STATE(8), 4,
      sym_graph,
      sym_triple,
      sym_directive,
      aux_sym_document_repeat1,
    STATE(40), 4,
      sym_prefix_id,
      sym_base,
      sym_sparql_base,
      sym_sparql_prefix,
    STATE(79), 5,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [461] = 24,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(116), 1,
      ts_builtin_sym_end,
    ACTIONS(118), 1,
      sym_pn_prefix,
    ACTIONS(121), 1,
      anon_sym_LBRACE,
    ACTIONS(124), 1,
      anon_sym_GRAPH,
    ACTIONS(127), 1,
      anon_sym_ATprefix,
    ACTIONS(130), 1,
      anon_sym_ATbase,
    ACTIONS(133), 1,
      aux_sym_sparql_base_token1,
    ACTIONS(136), 1,
      aux_sym_sparql_prefix_token1,
    ACTIONS(139), 1,
      anon_sym_LBRACK,
    ACTIONS(142), 1,
      anon_sym_LPAREN,
    ACTIONS(145), 1,
      anon_sym_LT,
    ACTIONS(148), 1,
      anon_sym_COLON,
    ACTIONS(151), 1,
      anon_sym__COLON,
    ACTIONS(154), 1,
      sym_anon,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(86), 1,
      sym_collection,
    STATE(107), 1,
      sym__triples,
    STATE(108), 1,
      sym__label,
    STATE(8), 4,
      sym_graph,
      sym_triple,
      sym_directive,
      aux_sym_document_repeat1,
    STATE(40), 4,
      sym_prefix_id,
      sym_base,
      sym_sparql_base,
      sym_sparql_prefix,
    STATE(79), 5,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [544] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(159), 13,
      anon_sym_GRAPH,
      anon_sym_DOT,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(157), 17,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [582] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(163), 13,
      anon_sym_GRAPH,
      anon_sym_DOT,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(161), 17,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [620] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(165), 10,
      anon_sym_DOT,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(167), 14,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [652] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(169), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(171), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [684] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(173), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(175), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [716] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(177), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(179), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [748] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(181), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(183), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [780] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(185), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(187), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [812] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(189), 10,
      anon_sym_DOT,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(191), 14,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [844] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(193), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(195), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [876] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(197), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(199), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [908] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(201), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(203), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [940] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(205), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(207), 15,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_CARET_CARET,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_lang_tag,
      sym_anon,
  [972] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(209), 10,
      anon_sym_DOT,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(211), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1003] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(213), 10,
      anon_sym_DOT,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(215), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1034] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(217), 10,
      anon_sym_DOT,
      anon_sym_a,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(219), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1065] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(221), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(223), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1095] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(225), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(227), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1125] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(229), 9,
      anon_sym_DOT,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(231), 13,
      anon_sym_RBRACE,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1155] = 15,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    ACTIONS(235), 1,
      anon_sym_RBRACE,
    ACTIONS(237), 1,
      sym_anon,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(93), 1,
      sym__triples,
    STATE(29), 2,
      sym_triple,
      aux_sym_graph_repeat1,
    STATE(86), 6,
      sym_collection,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1207] = 15,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(239), 1,
      sym_pn_prefix,
    ACTIONS(242), 1,
      anon_sym_RBRACE,
    ACTIONS(244), 1,
      anon_sym_LBRACK,
    ACTIONS(247), 1,
      anon_sym_LPAREN,
    ACTIONS(250), 1,
      anon_sym_LT,
    ACTIONS(253), 1,
      anon_sym_COLON,
    ACTIONS(256), 1,
      anon_sym__COLON,
    ACTIONS(259), 1,
      sym_anon,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(107), 1,
      sym__triples,
    STATE(29), 2,
      sym_triple,
      aux_sym_graph_repeat1,
    STATE(86), 6,
      sym_collection,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1259] = 15,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    ACTIONS(237), 1,
      sym_anon,
    ACTIONS(262), 1,
      anon_sym_RBRACE,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(99), 1,
      sym__triples,
    STATE(28), 2,
      sym_triple,
      aux_sym_graph_repeat1,
    STATE(86), 6,
      sym_collection,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1311] = 15,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    ACTIONS(237), 1,
      sym_anon,
    ACTIONS(264), 1,
      anon_sym_RBRACE,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(97), 1,
      sym__triples,
    STATE(29), 2,
      sym_triple,
      aux_sym_graph_repeat1,
    STATE(86), 6,
      sym_collection,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1363] = 15,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(23), 1,
      anon_sym_LBRACK,
    ACTIONS(25), 1,
      anon_sym_LPAREN,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    ACTIONS(237), 1,
      sym_anon,
    ACTIONS(266), 1,
      anon_sym_RBRACE,
    STATE(51), 1,
      sym_blank_node_property_list,
    STATE(53), 1,
      sym_subject,
    STATE(67), 1,
      sym_namespace,
    STATE(102), 1,
      sym__triples,
    STATE(31), 2,
      sym_triple,
      aux_sym_graph_repeat1,
    STATE(86), 6,
      sym_collection,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1415] = 5,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(272), 1,
      anon_sym_CARET_CARET,
    ACTIONS(274), 1,
      sym_lang_tag,
    ACTIONS(268), 8,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(270), 9,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1446] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(278), 6,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      sym_anon,
    ACTIONS(276), 12,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      sym_double,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_pn_prefix,
      sym_pn_local,
  [1472] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(284), 1,
      sym_pn_local,
    ACTIONS(282), 6,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      sym_anon,
    ACTIONS(280), 11,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      sym_double,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_pn_prefix,
  [1500] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(288), 6,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      sym_anon,
    ACTIONS(286), 12,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      sym_double,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_pn_prefix,
      sym_pn_local,
  [1526] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(290), 8,
      anon_sym_LBRACK,
      sym_integer,
      sym_decimal,
      anon_sym_DQUOTE,
      anon_sym_SQUOTE,
      anon_sym_true,
      anon_sym_false,
      sym_pn_prefix,
    ACTIONS(292), 8,
      anon_sym_LPAREN,
      anon_sym_LT,
      sym_double,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1550] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(296), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(294), 10,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1573] = 10,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(300), 1,
      anon_sym_a,
    STATE(5), 1,
      sym_predicate,
    STATE(35), 1,
      sym_namespace,
    STATE(88), 1,
      sym_property,
    STATE(37), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
    ACTIONS(298), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [1609] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(304), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(302), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1631] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(308), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(306), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1653] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(312), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(310), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1675] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(316), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(314), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1697] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(320), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(318), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1719] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(324), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(322), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1741] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(328), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(326), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1763] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(332), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(330), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1785] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(336), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(334), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1807] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(340), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(338), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1829] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(344), 5,
      anon_sym_GRAPH,
      aux_sym_sparql_base_token1,
      aux_sym_sparql_prefix_token1,
      anon_sym_LBRACK,
      sym_pn_prefix,
    ACTIONS(342), 9,
      ts_builtin_sym_end,
      anon_sym_LBRACE,
      anon_sym_ATprefix,
      anon_sym_ATbase,
      anon_sym_LPAREN,
      anon_sym_LT,
      anon_sym_COLON,
      anon_sym__COLON,
      sym_anon,
  [1851] = 11,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(300), 1,
      anon_sym_a,
    STATE(5), 1,
      sym_predicate,
    STATE(35), 1,
      sym_namespace,
    STATE(80), 1,
      sym_property,
    STATE(94), 1,
      sym_property_list,
    ACTIONS(346), 2,
      anon_sym_RBRACE,
      anon_sym_DOT,
    STATE(37), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
  [1888] = 8,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(31), 1,
      anon_sym__COLON,
    ACTIONS(108), 1,
      anon_sym_COLON,
    ACTIONS(348), 1,
      sym_pn_prefix,
    ACTIONS(350), 1,
      sym_anon,
    STATE(66), 1,
      sym_namespace,
    STATE(103), 5,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
      sym__blank_node,
      sym_blank_node_label,
  [1917] = 10,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(300), 1,
      anon_sym_a,
    STATE(5), 1,
      sym_predicate,
    STATE(35), 1,
      sym_namespace,
    STATE(80), 1,
      sym_property,
    STATE(94), 1,
      sym_property_list,
    STATE(37), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
  [1950] = 10,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(35), 1,
      sym_pn_prefix,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(300), 1,
      anon_sym_a,
    STATE(5), 1,
      sym_predicate,
    STATE(35), 1,
      sym_namespace,
    STATE(80), 1,
      sym_property,
    STATE(112), 1,
      sym_property_list,
    STATE(37), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
  [1983] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(356), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(358), 1,
      aux_sym__string_literal_long_quote_token1,
    STATE(59), 1,
      aux_sym__string_literal_long_quote_repeat1,
    ACTIONS(352), 2,
      anon_sym_DQUOTE,
      anon_sym_DQUOTE_DQUOTE,
    ACTIONS(354), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2004] = 6,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(108), 1,
      anon_sym_COLON,
    ACTIONS(348), 1,
      sym_pn_prefix,
    STATE(66), 1,
      sym_namespace,
    STATE(27), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
  [2025] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(274), 1,
      sym_lang_tag,
    ACTIONS(360), 1,
      anon_sym_CARET_CARET,
    ACTIONS(270), 5,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2042] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(370), 1,
      aux_sym__string_literal_long_single_quote_token1,
    STATE(58), 1,
      aux_sym__string_literal_long_single_quote_repeat1,
    ACTIONS(362), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
    ACTIONS(365), 2,
      anon_sym_SQUOTE,
      anon_sym_SQUOTE_SQUOTE,
  [2063] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(379), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(381), 1,
      aux_sym__string_literal_long_quote_token1,
    STATE(59), 1,
      aux_sym__string_literal_long_quote_repeat1,
    ACTIONS(373), 2,
      anon_sym_DQUOTE,
      anon_sym_DQUOTE_DQUOTE,
    ACTIONS(376), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2084] = 6,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    ACTIONS(53), 1,
      anon_sym_COLON,
    ACTIONS(384), 1,
      sym_pn_prefix,
    STATE(35), 1,
      sym_namespace,
    STATE(27), 3,
      sym_iri_reference,
      sym__iri,
      sym_prefixed_name,
  [2105] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(388), 1,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
    ACTIONS(390), 1,
      aux_sym__string_literal_long_quote_token1,
    STATE(55), 1,
      aux_sym__string_literal_long_quote_repeat1,
    ACTIONS(352), 2,
      anon_sym_DQUOTE,
      anon_sym_DQUOTE_DQUOTE,
    ACTIONS(386), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2126] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(278), 7,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      sym_pn_local,
  [2139] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(396), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(398), 1,
      aux_sym__string_literal_long_single_quote_token1,
    STATE(65), 1,
      aux_sym__string_literal_long_single_quote_repeat1,
    ACTIONS(392), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
    ACTIONS(394), 2,
      anon_sym_SQUOTE,
      anon_sym_SQUOTE_SQUOTE,
  [2160] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(288), 7,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      sym_pn_local,
  [2173] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 1,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
    ACTIONS(404), 1,
      aux_sym__string_literal_long_single_quote_token1,
    STATE(58), 1,
      aux_sym__string_literal_long_single_quote_repeat1,
    ACTIONS(394), 2,
      anon_sym_SQUOTE,
      anon_sym_SQUOTE_SQUOTE,
    ACTIONS(400), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2194] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(406), 1,
      sym_pn_local,
    ACTIONS(282), 6,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2209] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(284), 1,
      sym_pn_local,
    ACTIONS(282), 2,
      anon_sym_LBRACE,
      anon_sym_LT,
    ACTIONS(280), 3,
      anon_sym_a,
      anon_sym_COLON,
      sym_pn_prefix,
  [2225] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(408), 1,
      aux_sym__string_literal_long_single_quote_token1,
    ACTIONS(368), 5,
      aux_sym__string_literal_quote_token2,
      anon_sym_SQUOTE,
      anon_sym_SQUOTE_SQUOTE_SQUOTE,
      anon_sym_SQUOTE_SQUOTE,
      sym_echar,
  [2239] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(412), 1,
      anon_sym_COMMA,
    STATE(73), 1,
      aux_sym_object_list_repeat1,
    ACTIONS(410), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [2255] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(278), 2,
      anon_sym_LBRACE,
      anon_sym_LT,
    ACTIONS(276), 4,
      anon_sym_a,
      anon_sym_COLON,
      sym_pn_prefix,
      sym_pn_local,
  [2269] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(416), 1,
      anon_sym_COMMA,
    STATE(71), 1,
      aux_sym_object_list_repeat1,
    ACTIONS(414), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [2285] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(288), 2,
      anon_sym_LBRACE,
      anon_sym_LT,
    ACTIONS(286), 4,
      anon_sym_a,
      anon_sym_COLON,
      sym_pn_prefix,
      sym_pn_local,
  [2299] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(412), 1,
      anon_sym_COMMA,
    STATE(71), 1,
      aux_sym_object_list_repeat1,
    ACTIONS(419), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [2315] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(421), 1,
      aux_sym__string_literal_long_quote_token1,
    ACTIONS(379), 5,
      anon_sym_DQUOTE,
      aux_sym__string_literal_quote_token2,
      anon_sym_DQUOTE_DQUOTE_DQUOTE,
      anon_sym_DQUOTE_DQUOTE,
      sym_echar,
  [2329] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(425), 1,
      anon_sym_SEMI,
    STATE(75), 1,
      aux_sym_property_list_repeat1,
    ACTIONS(423), 3,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_RBRACK,
  [2344] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(428), 1,
      anon_sym_DQUOTE,
    ACTIONS(430), 1,
      aux_sym__string_literal_quote_token1,
    STATE(76), 1,
      aux_sym__string_literal_quote_repeat1,
    ACTIONS(433), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2361] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(414), 5,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2372] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(436), 1,
      anon_sym_DQUOTE,
    ACTIONS(438), 1,
      aux_sym__string_literal_quote_token1,
    STATE(76), 1,
      aux_sym__string_literal_quote_repeat1,
    ACTIONS(440), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2389] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(444), 1,
      anon_sym_LBRACE,
    ACTIONS(442), 2,
      anon_sym_a,
      sym_pn_prefix,
    ACTIONS(446), 2,
      anon_sym_LT,
      anon_sym_COLON,
  [2404] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(450), 1,
      anon_sym_SEMI,
    STATE(81), 1,
      aux_sym_property_list_repeat1,
    ACTIONS(448), 3,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_RBRACK,
  [2419] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(450), 1,
      anon_sym_SEMI,
    STATE(75), 1,
      aux_sym_property_list_repeat1,
    ACTIONS(452), 3,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_RBRACK,
  [2434] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(456), 1,
      anon_sym_SQUOTE,
    ACTIONS(458), 1,
      aux_sym__string_literal_single_quote_token1,
    STATE(84), 1,
      aux_sym__string_literal_single_quote_repeat1,
    ACTIONS(454), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2451] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(460), 1,
      anon_sym_DQUOTE,
    ACTIONS(462), 1,
      aux_sym__string_literal_quote_token1,
    STATE(78), 1,
      aux_sym__string_literal_quote_repeat1,
    ACTIONS(464), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2468] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(469), 1,
      anon_sym_SQUOTE,
    ACTIONS(471), 1,
      aux_sym__string_literal_single_quote_token1,
    STATE(84), 1,
      aux_sym__string_literal_single_quote_repeat1,
    ACTIONS(466), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2485] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(476), 1,
      anon_sym_SQUOTE,
    ACTIONS(478), 1,
      aux_sym__string_literal_single_quote_token1,
    STATE(82), 1,
      aux_sym__string_literal_single_quote_repeat1,
    ACTIONS(474), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2502] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(442), 2,
      anon_sym_a,
      sym_pn_prefix,
    ACTIONS(446), 2,
      anon_sym_LT,
      anon_sym_COLON,
  [2514] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(480), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [2524] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(423), 4,
      anon_sym_RBRACE,
      anon_sym_DOT,
      anon_sym_SEMI,
      anon_sym_RBRACK,
  [2534] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    STATE(100), 1,
      sym_namespace,
  [2547] = 4,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(29), 1,
      anon_sym_COLON,
    ACTIONS(233), 1,
      sym_pn_prefix,
    STATE(101), 1,
      sym_namespace,
  [2560] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(484), 1,
      aux_sym__string_literal_long_single_quote_token1,
    ACTIONS(482), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2571] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(488), 1,
      aux_sym__string_literal_long_quote_token1,
    ACTIONS(486), 2,
      aux_sym__string_literal_quote_token2,
      sym_echar,
  [2582] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(490), 1,
      anon_sym_RBRACE,
    ACTIONS(492), 1,
      anon_sym_DOT,
  [2592] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(494), 2,
      anon_sym_RBRACE,
      anon_sym_DOT,
  [2600] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    STATE(106), 1,
      sym_iri_reference,
  [2610] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    STATE(46), 1,
      sym_iri_reference,
  [2620] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(492), 1,
      anon_sym_DOT,
    ACTIONS(496), 1,
      anon_sym_RBRACE,
  [2630] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(498), 1,
      anon_sym_POUND,
    ACTIONS(500), 1,
      aux_sym_iri_reference_token1,
  [2640] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(235), 1,
      anon_sym_RBRACE,
    ACTIONS(492), 1,
      anon_sym_DOT,
  [2650] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    STATE(42), 1,
      sym_iri_reference,
  [2660] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LT,
    STATE(113), 1,
      sym_iri_reference,
  [2670] = 3,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(264), 1,
      anon_sym_RBRACE,
    ACTIONS(492), 1,
      anon_sym_DOT,
  [2680] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(502), 1,
      anon_sym_LBRACE,
  [2687] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(504), 1,
      anon_sym_GT,
  [2694] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(506), 1,
      anon_sym_GT,
  [2701] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(508), 1,
      anon_sym_DOT,
  [2708] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(492), 1,
      anon_sym_DOT,
  [2715] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(510), 1,
      anon_sym_LBRACE,
  [2722] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(512), 1,
      ts_builtin_sym_end,
  [2729] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(514), 1,
      anon_sym_COLON,
  [2736] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(516), 1,
      aux_sym_blank_node_label_token1,
  [2743] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(518), 1,
      anon_sym_RBRACK,
  [2750] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(520), 1,
      anon_sym_DOT,
  [2757] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(522), 1,
      anon_sym_COLON,
  [2764] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(524), 1,
      anon_sym_RPAREN,
  [2771] = 2,
    ACTIONS(13), 1,
      sym_comment,
    ACTIONS(526), 1,
      anon_sym_COLON,
  [2778] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(528), 1,
      aux_sym_iri_reference_token1,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 79,
  [SMALL_STATE(4)] = 155,
  [SMALL_STATE(5)] = 231,
  [SMALL_STATE(6)] = 306,
  [SMALL_STATE(7)] = 378,
  [SMALL_STATE(8)] = 461,
  [SMALL_STATE(9)] = 544,
  [SMALL_STATE(10)] = 582,
  [SMALL_STATE(11)] = 620,
  [SMALL_STATE(12)] = 652,
  [SMALL_STATE(13)] = 684,
  [SMALL_STATE(14)] = 716,
  [SMALL_STATE(15)] = 748,
  [SMALL_STATE(16)] = 780,
  [SMALL_STATE(17)] = 812,
  [SMALL_STATE(18)] = 844,
  [SMALL_STATE(19)] = 876,
  [SMALL_STATE(20)] = 908,
  [SMALL_STATE(21)] = 940,
  [SMALL_STATE(22)] = 972,
  [SMALL_STATE(23)] = 1003,
  [SMALL_STATE(24)] = 1034,
  [SMALL_STATE(25)] = 1065,
  [SMALL_STATE(26)] = 1095,
  [SMALL_STATE(27)] = 1125,
  [SMALL_STATE(28)] = 1155,
  [SMALL_STATE(29)] = 1207,
  [SMALL_STATE(30)] = 1259,
  [SMALL_STATE(31)] = 1311,
  [SMALL_STATE(32)] = 1363,
  [SMALL_STATE(33)] = 1415,
  [SMALL_STATE(34)] = 1446,
  [SMALL_STATE(35)] = 1472,
  [SMALL_STATE(36)] = 1500,
  [SMALL_STATE(37)] = 1526,
  [SMALL_STATE(38)] = 1550,
  [SMALL_STATE(39)] = 1573,
  [SMALL_STATE(40)] = 1609,
  [SMALL_STATE(41)] = 1631,
  [SMALL_STATE(42)] = 1653,
  [SMALL_STATE(43)] = 1675,
  [SMALL_STATE(44)] = 1697,
  [SMALL_STATE(45)] = 1719,
  [SMALL_STATE(46)] = 1741,
  [SMALL_STATE(47)] = 1763,
  [SMALL_STATE(48)] = 1785,
  [SMALL_STATE(49)] = 1807,
  [SMALL_STATE(50)] = 1829,
  [SMALL_STATE(51)] = 1851,
  [SMALL_STATE(52)] = 1888,
  [SMALL_STATE(53)] = 1917,
  [SMALL_STATE(54)] = 1950,
  [SMALL_STATE(55)] = 1983,
  [SMALL_STATE(56)] = 2004,
  [SMALL_STATE(57)] = 2025,
  [SMALL_STATE(58)] = 2042,
  [SMALL_STATE(59)] = 2063,
  [SMALL_STATE(60)] = 2084,
  [SMALL_STATE(61)] = 2105,
  [SMALL_STATE(62)] = 2126,
  [SMALL_STATE(63)] = 2139,
  [SMALL_STATE(64)] = 2160,
  [SMALL_STATE(65)] = 2173,
  [SMALL_STATE(66)] = 2194,
  [SMALL_STATE(67)] = 2209,
  [SMALL_STATE(68)] = 2225,
  [SMALL_STATE(69)] = 2239,
  [SMALL_STATE(70)] = 2255,
  [SMALL_STATE(71)] = 2269,
  [SMALL_STATE(72)] = 2285,
  [SMALL_STATE(73)] = 2299,
  [SMALL_STATE(74)] = 2315,
  [SMALL_STATE(75)] = 2329,
  [SMALL_STATE(76)] = 2344,
  [SMALL_STATE(77)] = 2361,
  [SMALL_STATE(78)] = 2372,
  [SMALL_STATE(79)] = 2389,
  [SMALL_STATE(80)] = 2404,
  [SMALL_STATE(81)] = 2419,
  [SMALL_STATE(82)] = 2434,
  [SMALL_STATE(83)] = 2451,
  [SMALL_STATE(84)] = 2468,
  [SMALL_STATE(85)] = 2485,
  [SMALL_STATE(86)] = 2502,
  [SMALL_STATE(87)] = 2514,
  [SMALL_STATE(88)] = 2524,
  [SMALL_STATE(89)] = 2534,
  [SMALL_STATE(90)] = 2547,
  [SMALL_STATE(91)] = 2560,
  [SMALL_STATE(92)] = 2571,
  [SMALL_STATE(93)] = 2582,
  [SMALL_STATE(94)] = 2592,
  [SMALL_STATE(95)] = 2600,
  [SMALL_STATE(96)] = 2610,
  [SMALL_STATE(97)] = 2620,
  [SMALL_STATE(98)] = 2630,
  [SMALL_STATE(99)] = 2640,
  [SMALL_STATE(100)] = 2650,
  [SMALL_STATE(101)] = 2660,
  [SMALL_STATE(102)] = 2670,
  [SMALL_STATE(103)] = 2680,
  [SMALL_STATE(104)] = 2687,
  [SMALL_STATE(105)] = 2694,
  [SMALL_STATE(106)] = 2701,
  [SMALL_STATE(107)] = 2708,
  [SMALL_STATE(108)] = 2715,
  [SMALL_STATE(109)] = 2722,
  [SMALL_STATE(110)] = 2729,
  [SMALL_STATE(111)] = 2736,
  [SMALL_STATE(112)] = 2743,
  [SMALL_STATE(113)] = 2750,
  [SMALL_STATE(114)] = 2757,
  [SMALL_STATE(115)] = 2764,
  [SMALL_STATE(116)] = 2771,
  [SMALL_STATE(117)] = 2778,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 0, 0, 0),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(110),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [11] = {.entry = {.count = 1, .reusable = false}}, SHIFT(52),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [19] = {.entry = {.count = 1, .reusable = false}}, SHIFT(96),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(89),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(54),
  [25] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [27] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [29] = {.entry = {.count = 1, .reusable = true}}, SHIFT(72),
  [31] = {.entry = {.count = 1, .reusable = true}}, SHIFT(111),
  [33] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [35] = {.entry = {.count = 1, .reusable = false}}, SHIFT(116),
  [37] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [39] = {.entry = {.count = 1, .reusable = false}}, SHIFT(4),
  [41] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [43] = {.entry = {.count = 1, .reusable = false}}, SHIFT(83),
  [45] = {.entry = {.count = 1, .reusable = false}}, SHIFT(85),
  [47] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [49] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [51] = {.entry = {.count = 1, .reusable = false}}, SHIFT(26),
  [53] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [55] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(116),
  [58] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(54),
  [61] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(2),
  [64] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0),
  [66] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(98),
  [69] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(3),
  [72] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(3),
  [75] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(83),
  [78] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(85),
  [81] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(63),
  [84] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(61),
  [87] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(26),
  [90] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(36),
  [93] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_collection_repeat1, 2, 0, 0), SHIFT_REPEAT(111),
  [96] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_collection, 1, 0, 0),
  [98] = {.entry = {.count = 1, .reusable = false}}, SHIFT(3),
  [100] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [102] = {.entry = {.count = 1, .reusable = false}}, SHIFT(114),
  [104] = {.entry = {.count = 1, .reusable = false}}, SHIFT(69),
  [106] = {.entry = {.count = 1, .reusable = true}}, SHIFT(69),
  [108] = {.entry = {.count = 1, .reusable = true}}, SHIFT(64),
  [110] = {.entry = {.count = 1, .reusable = false}}, SHIFT(77),
  [112] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [114] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_document, 1, 0, 0),
  [116] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0),
  [118] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(110),
  [121] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(32),
  [124] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(52),
  [127] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(90),
  [130] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(95),
  [133] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(96),
  [136] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(89),
  [139] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(54),
  [142] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(2),
  [145] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(98),
  [148] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(72),
  [151] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(111),
  [154] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_document_repeat1, 2, 0, 0), SHIFT_REPEAT(79),
  [157] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_iri_reference, 3, 0, 0),
  [159] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_iri_reference, 3, 0, 0),
  [161] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_iri_reference, 4, 0, 0),
  [163] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_iri_reference, 4, 0, 0),
  [165] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_prefixed_name, 2, 0, 0),
  [167] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_prefixed_name, 2, 0, 0),
  [169] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_long_single_quote, 3, 0, 0),
  [171] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_long_single_quote, 3, 0, 0),
  [173] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_long_quote, 2, 0, 0),
  [175] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_long_quote, 2, 0, 0),
  [177] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_long_single_quote, 2, 0, 0),
  [179] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_long_single_quote, 2, 0, 0),
  [181] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_single_quote, 2, 0, 0),
  [183] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_single_quote, 2, 0, 0),
  [185] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_quote, 2, 0, 0),
  [187] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_quote, 2, 0, 0),
  [189] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_blank_node_label, 2, 0, 0),
  [191] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_blank_node_label, 2, 0, 0),
  [193] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string, 1, 0, 0),
  [195] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 1, 0, 0),
  [197] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_long_quote, 3, 0, 0),
  [199] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_long_quote, 3, 0, 0),
  [201] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_single_quote, 3, 0, 0),
  [203] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_single_quote, 3, 0, 0),
  [205] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__string_literal_quote, 3, 0, 0),
  [207] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__string_literal_quote, 3, 0, 0),
  [209] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_collection, 3, 0, 0),
  [211] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_collection, 3, 0, 0),
  [213] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_blank_node_property_list, 3, 0, 0),
  [215] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_blank_node_property_list, 3, 0, 0),
  [217] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_collection, 2, 0, 0),
  [219] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_collection, 2, 0, 0),
  [221] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_rdf_literal, 2, 0, 1),
  [223] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rdf_literal, 2, 0, 1),
  [225] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_boolean_literal, 1, 0, 0),
  [227] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_boolean_literal, 1, 0, 0),
  [229] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_rdf_literal, 3, 0, 3),
  [231] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rdf_literal, 3, 0, 3),
  [233] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [235] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [237] = {.entry = {.count = 1, .reusable = true}}, SHIFT(86),
  [239] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(110),
  [242] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0),
  [244] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(54),
  [247] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(2),
  [250] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(98),
  [253] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(72),
  [256] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(111),
  [259] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_graph_repeat1, 2, 0, 0), SHIFT_REPEAT(86),
  [262] = {.entry = {.count = 1, .reusable = true}}, SHIFT(48),
  [264] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [266] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [268] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_rdf_literal, 1, 0, 1),
  [270] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rdf_literal, 1, 0, 1),
  [272] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [274] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [276] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_namespace, 2, 0, 0),
  [278] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_namespace, 2, 0, 0),
  [280] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_prefixed_name, 1, 0, 0),
  [282] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_prefixed_name, 1, 0, 0),
  [284] = {.entry = {.count = 1, .reusable = false}}, SHIFT(11),
  [286] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_namespace, 1, 0, 0),
  [288] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_namespace, 1, 0, 0),
  [290] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_predicate, 1, 0, 0),
  [292] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_predicate, 1, 0, 0),
  [294] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_triple, 2, 0, 0),
  [296] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_triple, 2, 0, 0),
  [298] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_property_list_repeat1, 1, 0, 0),
  [300] = {.entry = {.count = 1, .reusable = false}}, SHIFT(37),
  [302] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_directive, 1, 0, 0),
  [304] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_directive, 1, 0, 0),
  [306] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 5, 0, 2),
  [308] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 5, 0, 2),
  [310] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_sparql_prefix, 3, 0, 0),
  [312] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_sparql_prefix, 3, 0, 0),
  [314] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_base, 3, 0, 0),
  [316] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_base, 3, 0, 0),
  [318] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 4, 0, 2),
  [320] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 4, 0, 2),
  [322] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 4, 0, 0),
  [324] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 4, 0, 0),
  [326] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_sparql_base, 2, 0, 0),
  [328] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_sparql_base, 2, 0, 0),
  [330] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 3, 0, 0),
  [332] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 3, 0, 0),
  [334] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 3, 0, 2),
  [336] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 3, 0, 2),
  [338] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_prefix_id, 4, 0, 0),
  [340] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_prefix_id, 4, 0, 0),
  [342] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_graph, 2, 0, 0),
  [344] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_graph, 2, 0, 0),
  [346] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__triples, 1, 0, 0),
  [348] = {.entry = {.count = 1, .reusable = true}}, SHIFT(114),
  [350] = {.entry = {.count = 1, .reusable = true}}, SHIFT(103),
  [352] = {.entry = {.count = 1, .reusable = false}}, SHIFT(92),
  [354] = {.entry = {.count = 1, .reusable = false}}, SHIFT(59),
  [356] = {.entry = {.count = 1, .reusable = false}}, SHIFT(19),
  [358] = {.entry = {.count = 1, .reusable = true}}, SHIFT(59),
  [360] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [362] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_long_single_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(58),
  [365] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_long_single_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(91),
  [368] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__string_literal_long_single_quote_repeat1, 2, 0, 0),
  [370] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__string_literal_long_single_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(58),
  [373] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_long_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(92),
  [376] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_long_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(59),
  [379] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__string_literal_long_quote_repeat1, 2, 0, 0),
  [381] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__string_literal_long_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(59),
  [384] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [386] = {.entry = {.count = 1, .reusable = false}}, SHIFT(55),
  [388] = {.entry = {.count = 1, .reusable = false}}, SHIFT(13),
  [390] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [392] = {.entry = {.count = 1, .reusable = false}}, SHIFT(65),
  [394] = {.entry = {.count = 1, .reusable = false}}, SHIFT(91),
  [396] = {.entry = {.count = 1, .reusable = false}}, SHIFT(14),
  [398] = {.entry = {.count = 1, .reusable = true}}, SHIFT(65),
  [400] = {.entry = {.count = 1, .reusable = false}}, SHIFT(58),
  [402] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [404] = {.entry = {.count = 1, .reusable = true}}, SHIFT(58),
  [406] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [408] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__string_literal_long_single_quote_repeat1, 2, 0, 0),
  [410] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_list, 1, 0, 0),
  [412] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [414] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_object_list_repeat1, 2, 0, 0),
  [416] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_list_repeat1, 2, 0, 0), SHIFT_REPEAT(6),
  [419] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_list, 2, 0, 0),
  [421] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__string_literal_long_quote_repeat1, 2, 0, 0),
  [423] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_property_list_repeat1, 2, 0, 0),
  [425] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_property_list_repeat1, 2, 0, 0), SHIFT_REPEAT(39),
  [428] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__string_literal_quote_repeat1, 2, 0, 0),
  [430] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__string_literal_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(76),
  [433] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(76),
  [436] = {.entry = {.count = 1, .reusable = false}}, SHIFT(21),
  [438] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [440] = {.entry = {.count = 1, .reusable = false}}, SHIFT(76),
  [442] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_subject, 1, 0, 0),
  [444] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__label, 1, 0, 0),
  [446] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_subject, 1, 0, 0),
  [448] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property_list, 1, 0, 0),
  [450] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [452] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property_list, 2, 0, 0),
  [454] = {.entry = {.count = 1, .reusable = false}}, SHIFT(84),
  [456] = {.entry = {.count = 1, .reusable = false}}, SHIFT(20),
  [458] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [460] = {.entry = {.count = 1, .reusable = false}}, SHIFT(16),
  [462] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [464] = {.entry = {.count = 1, .reusable = false}}, SHIFT(78),
  [466] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__string_literal_single_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(84),
  [469] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__string_literal_single_quote_repeat1, 2, 0, 0),
  [471] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__string_literal_single_quote_repeat1, 2, 0, 0), SHIFT_REPEAT(84),
  [474] = {.entry = {.count = 1, .reusable = false}}, SHIFT(82),
  [476] = {.entry = {.count = 1, .reusable = false}}, SHIFT(15),
  [478] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [480] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property, 2, 0, 0),
  [482] = {.entry = {.count = 1, .reusable = false}}, SHIFT(68),
  [484] = {.entry = {.count = 1, .reusable = true}}, SHIFT(68),
  [486] = {.entry = {.count = 1, .reusable = false}}, SHIFT(74),
  [488] = {.entry = {.count = 1, .reusable = true}}, SHIFT(74),
  [490] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [492] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [494] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__triples, 2, 0, 0),
  [496] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
  [498] = {.entry = {.count = 1, .reusable = false}}, SHIFT(117),
  [500] = {.entry = {.count = 1, .reusable = false}}, SHIFT(105),
  [502] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__label, 2, 0, 0),
  [504] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [506] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [508] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [510] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [512] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [514] = {.entry = {.count = 1, .reusable = true}}, SHIFT(70),
  [516] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [518] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [520] = {.entry = {.count = 1, .reusable = true}}, SHIFT(49),
  [522] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [524] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [526] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [528] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef TREE_SITTER_HIDE_SYMBOLS
#define TS_PUBLIC
#elif defined(_WIN32)
#define TS_PUBLIC __declspec(dllexport)
#else
#define TS_PUBLIC __attribute__((visibility("default")))
#endif

TS_PUBLIC const TSLanguage *tree_sitter_turtle(void) {
  static const TSLanguage language = {
    .version = LANGUAGE_VERSION,
    .symbol_count = SYMBOL_COUNT,
    .alias_count = ALIAS_COUNT,
    .token_count = TOKEN_COUNT,
    .external_token_count = EXTERNAL_TOKEN_COUNT,
    .state_count = STATE_COUNT,
    .large_state_count = LARGE_STATE_COUNT,
    .production_id_count = PRODUCTION_ID_COUNT,
    .field_count = FIELD_COUNT,
    .max_alias_sequence_length = MAX_ALIAS_SEQUENCE_LENGTH,
    .parse_table = &ts_parse_table[0][0],
    .small_parse_table = ts_small_parse_table,
    .small_parse_table_map = ts_small_parse_table_map,
    .parse_actions = ts_parse_actions,
    .symbol_names = ts_symbol_names,
    .field_names = ts_field_names,
    .field_map_slices = ts_field_map_slices,
    .field_map_entries = ts_field_map_entries,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
    .keyword_lex_fn = ts_lex_keywords,
    .keyword_capture_token = sym_pn_prefix,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
