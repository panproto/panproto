#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 214
#define LARGE_STATE_COUNT 6
#define SYMBOL_COUNT 242
#define ALIAS_COUNT 0
#define TOKEN_COUNT 180
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 7
#define MAX_ALIAS_SEQUENCE_LENGTH 8
#define PRODUCTION_ID_COUNT 17

enum {
  sym_identifier = 1,
  anon_sym_LPAREN = 2,
  anon_sym_define_DASHtrait = 3,
  anon_sym_RPAREN = 4,
  anon_sym_impl_DASHtrait = 5,
  anon_sym_DOT = 6,
  anon_sym_use_DASHtrait = 7,
  anon_sym_define_DASHfungible_DASHtoken = 8,
  anon_sym_define_DASHnon_DASHfungible_DASHtoken = 9,
  anon_sym_define_DASHconstant = 10,
  anon_sym_define_DASHdata_DASHvar = 11,
  anon_sym_define_DASHmap = 12,
  anon_sym_define_DASHprivate = 13,
  anon_sym_define_DASHread_DASHonly = 14,
  anon_sym_define_DASHpublic = 15,
  anon_sym_let = 16,
  anon_sym_LT = 17,
  anon_sym_GT = 18,
  anon_sym_int = 19,
  anon_sym_uint = 20,
  anon_sym_bool = 21,
  anon_sym_principal = 22,
  anon_sym_buff = 23,
  aux_sym_buffer_type_token1 = 24,
  anon_sym_string_DASHascii = 25,
  anon_sym_string_DASHutf8 = 26,
  anon_sym_list = 27,
  anon_sym_optional = 28,
  anon_sym_tuple = 29,
  anon_sym_LBRACE = 30,
  anon_sym_COMMA = 31,
  anon_sym_RBRACE = 32,
  anon_sym_COLON = 33,
  anon_sym_response = 34,
  anon_sym_DASH = 35,
  sym_uint_lit = 36,
  anon_sym_true = 37,
  anon_sym_false = 38,
  anon_sym_SQUOTE = 39,
  aux_sym_standard_principal_lit_token1 = 40,
  aux_sym_standard_principal_lit_token2 = 41,
  anon_sym_0x = 42,
  aux_sym_buffer_lit_token1 = 43,
  anon_sym_DQUOTE = 44,
  aux_sym_ascii_string_lit_token1 = 45,
  sym_utf8_string_lit = 46,
  anon_sym_some = 47,
  sym_none_lit = 48,
  anon_sym_ok = 49,
  anon_sym_err = 50,
  anon_sym_PLUS = 51,
  anon_sym_STAR = 52,
  anon_sym_SLASH = 53,
  anon_sym_mod = 54,
  anon_sym_pow = 55,
  anon_sym_LT_EQ = 56,
  anon_sym_GT_EQ = 57,
  anon_sym_and = 58,
  anon_sym_or = 59,
  anon_sym_xor = 60,
  anon_sym_append = 61,
  anon_sym_as_DASHcontract = 62,
  anon_sym_as_DASHmax_DASHlen_QMARK = 63,
  anon_sym_asserts_BANG = 64,
  anon_sym_at_DASHblock = 65,
  anon_sym_begin = 66,
  anon_sym_bit_DASHand = 67,
  anon_sym_bit_DASHnot = 68,
  anon_sym_bit_DASHor = 69,
  anon_sym_bit_DASHshift_DASHleft = 70,
  anon_sym_bit_DASHshift_DASHright = 71,
  anon_sym_bit_DASHxor = 72,
  anon_sym_buff_DASHto_DASHint_DASHbe = 73,
  anon_sym_buff_DASHto_DASHint_DASHle = 74,
  anon_sym_buff_DASHto_DASHuint_DASHbe = 75,
  anon_sym_buff_DASHto_DASHuint_DASHle = 76,
  anon_sym_concat = 77,
  anon_sym_contract_DASHcall_QMARK = 78,
  anon_sym_contract_DASHhash_QMARK = 79,
  anon_sym_contract_DASHof = 80,
  anon_sym_default_DASHto = 81,
  anon_sym_element_DASHat = 82,
  anon_sym_element_DASHat_QMARK = 83,
  anon_sym_filter = 84,
  anon_sym_fold = 85,
  anon_sym_from_DASHconsensus_DASHbuff_QMARK = 86,
  anon_sym_ft_DASHburn_QMARK = 87,
  anon_sym_ft_DASHmint_QMARK = 88,
  anon_sym_ft_DASHtransfer_QMARK = 89,
  anon_sym_ft_DASHget_DASHsupply = 90,
  anon_sym_ft_DASHget_DASHbalance = 91,
  anon_sym_get = 92,
  anon_sym_get_DASHblock_DASHinfo_QMARK = 93,
  anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK = 94,
  anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK = 95,
  anon_sym_get_DASHtenure_DASHinfo_QMARK = 96,
  anon_sym_hash160 = 97,
  anon_sym_if = 98,
  anon_sym_index_DASHof = 99,
  anon_sym_index_DASHof_QMARK = 100,
  anon_sym_int_DASHto_DASHascii = 101,
  anon_sym_int_DASHto_DASHutf8 = 102,
  anon_sym_is_DASHeq = 103,
  anon_sym_is_DASHerr = 104,
  anon_sym_is_DASHnone = 105,
  anon_sym_is_DASHok = 106,
  anon_sym_is_DASHsome = 107,
  anon_sym_is_DASHstandard = 108,
  anon_sym_keccak256 = 109,
  anon_sym_len = 110,
  anon_sym_log2 = 111,
  anon_sym_map = 112,
  anon_sym_map_DASHdelete = 113,
  anon_sym_map_DASHget_QMARK = 114,
  anon_sym_map_DASHinsert = 115,
  anon_sym_map_DASHset = 116,
  anon_sym_match = 117,
  anon_sym_merge = 118,
  anon_sym_nft_DASHburn_QMARK = 119,
  anon_sym_nft_DASHmint_QMARK = 120,
  anon_sym_nft_DASHget_DASHowner_QMARK = 121,
  anon_sym_nft_DASHtransfer_QMARK = 122,
  anon_sym_not = 123,
  anon_sym_principal_DASHconstruct_QMARK = 124,
  anon_sym_principal_DASHdestruct_QMARK = 125,
  anon_sym_principal_DASHof_QMARK = 126,
  anon_sym_print = 127,
  anon_sym_replace_DASHat_QMARK = 128,
  anon_sym_restrict_DASHassets_QMARK = 129,
  anon_sym_secp256k1_DASHrecover_QMARK = 130,
  anon_sym_secp256k1_DASHverify = 131,
  anon_sym_secp256r1_DASHverify = 132,
  anon_sym_sha256 = 133,
  anon_sym_sha512 = 134,
  anon_sym_sha512_SLASH256 = 135,
  anon_sym_slice_QMARK = 136,
  anon_sym_sqrti = 137,
  anon_sym_string_DASHto_DASHint_QMARK = 138,
  anon_sym_string_DASHto_DASHuint_QMARK = 139,
  anon_sym_stx_DASHaccount = 140,
  anon_sym_stx_DASHburn_QMARK = 141,
  anon_sym_stx_DASHget_DASHbalance = 142,
  anon_sym_stx_DASHtransfer_DASHmemo_QMARK = 143,
  anon_sym_stx_DASHtransfer_QMARK = 144,
  anon_sym_to_DASHascii_QMARK = 145,
  anon_sym_to_DASHconsensus_DASHbuff_QMARK = 146,
  anon_sym_to_DASHint = 147,
  anon_sym_to_DASHuint = 148,
  anon_sym_try_BANG = 149,
  anon_sym_unwrap_BANG = 150,
  anon_sym_unwrap_DASHerr_BANG = 151,
  anon_sym_unwrap_DASHerr_DASHpanic = 152,
  anon_sym_unwrap_DASHpanic = 153,
  anon_sym_var_DASHget = 154,
  anon_sym_var_DASHset = 155,
  anon_sym_block_DASHheight = 156,
  anon_sym_burn_DASHblock_DASHheight = 157,
  anon_sym_chain_DASHid = 158,
  anon_sym_contract_DASHcaller = 159,
  anon_sym_is_DASHin_DASHmainnet = 160,
  anon_sym_is_DASHin_DASHregtest = 161,
  anon_sym_stacks_DASHblock_DASHheight = 162,
  anon_sym_stacks_DASHblock_DASHtime = 163,
  anon_sym_stx_DASHliquid_DASHsupply = 164,
  anon_sym_current_DASHcontract = 165,
  anon_sym_tenure_DASHheight = 166,
  anon_sym_tx_DASHsender = 167,
  anon_sym_tx_DASHsponsor_QMARK = 168,
  anon_sym_block_DASHreward = 169,
  anon_sym_burnchain_DASHheader_DASHhash = 170,
  anon_sym_id_DASHheader_DASHhash = 171,
  anon_sym_header_DASHhash = 172,
  anon_sym_miner_DASHaddress = 173,
  anon_sym_miner_DASHspend_DASHtotal = 174,
  anon_sym_miner_DASHspend_DASHwinner = 175,
  anon_sym_time = 176,
  anon_sym_vrf_DASHseed = 177,
  anon_sym_pox_DASHaddrs = 178,
  sym_comment = 179,
  sym_source = 180,
  sym__expression = 181,
  sym__function_call = 182,
  sym_trait_definition = 183,
  sym_trait_implementation = 184,
  sym_trait_usage = 185,
  sym_token_definition = 186,
  sym_fungible_token_definition = 187,
  sym_non_fungible_token_definition = 188,
  sym_constant_definition = 189,
  sym_variable_definition = 190,
  sym_mapping_definition = 191,
  sym_function_definition = 192,
  sym_private_function = 193,
  sym_read_only_function = 194,
  sym_public_function = 195,
  sym__native_function_call = 196,
  sym_basic_native_form = 197,
  sym_contract_function_call = 198,
  sym_let_expression = 199,
  sym_local_binding = 200,
  sym_function_signature = 201,
  sym_function_parameter = 202,
  sym_function_signature_for_trait = 203,
  sym_parameter_type = 204,
  sym_trait_type = 205,
  sym_native_type = 206,
  sym_buffer_type = 207,
  sym_ascii_string_type = 208,
  sym_utf8_string_type = 209,
  sym_list_type = 210,
  sym_optional_type = 211,
  sym_tuple_type_for_trait = 212,
  sym_tuple_type = 213,
  sym__tuple_type_pair = 214,
  sym_response_type = 215,
  sym__parameter = 216,
  sym__literal = 217,
  sym_int_lit = 218,
  sym_bool_lit = 219,
  sym_standard_principal_lit = 220,
  sym_contract_principal_lit = 221,
  sym_buffer_lit = 222,
  sym_ascii_string_lit = 223,
  sym_list_lit = 224,
  sym_list_lit_token = 225,
  sym_some_lit = 226,
  sym_tuple_lit = 227,
  sym__tuple_lit_pair = 228,
  sym_response_lit = 229,
  sym_native_identifier = 230,
  sym_global = 231,
  aux_sym_source_repeat1 = 232,
  aux_sym_trait_definition_repeat1 = 233,
  aux_sym_basic_native_form_repeat1 = 234,
  aux_sym_let_expression_repeat1 = 235,
  aux_sym_let_expression_repeat2 = 236,
  aux_sym_function_signature_repeat1 = 237,
  aux_sym_function_signature_for_trait_repeat1 = 238,
  aux_sym_tuple_type_for_trait_repeat1 = 239,
  aux_sym_tuple_type_repeat1 = 240,
  aux_sym_tuple_lit_repeat1 = 241,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym_identifier] = "identifier",
  [anon_sym_LPAREN] = "(",
  [anon_sym_define_DASHtrait] = "define-trait",
  [anon_sym_RPAREN] = ")",
  [anon_sym_impl_DASHtrait] = "impl-trait",
  [anon_sym_DOT] = ".",
  [anon_sym_use_DASHtrait] = "use-trait",
  [anon_sym_define_DASHfungible_DASHtoken] = "define-fungible-token",
  [anon_sym_define_DASHnon_DASHfungible_DASHtoken] = "define-non-fungible-token",
  [anon_sym_define_DASHconstant] = "define-constant",
  [anon_sym_define_DASHdata_DASHvar] = "define-data-var",
  [anon_sym_define_DASHmap] = "define-map",
  [anon_sym_define_DASHprivate] = "define-private",
  [anon_sym_define_DASHread_DASHonly] = "define-read-only",
  [anon_sym_define_DASHpublic] = "define-public",
  [anon_sym_let] = "let",
  [anon_sym_LT] = "<",
  [anon_sym_GT] = ">",
  [anon_sym_int] = "int",
  [anon_sym_uint] = "uint",
  [anon_sym_bool] = "bool",
  [anon_sym_principal] = "principal",
  [anon_sym_buff] = "buff",
  [aux_sym_buffer_type_token1] = "buffer_type_token1",
  [anon_sym_string_DASHascii] = "string-ascii",
  [anon_sym_string_DASHutf8] = "string-utf8",
  [anon_sym_list] = "list",
  [anon_sym_optional] = "optional",
  [anon_sym_tuple] = "tuple",
  [anon_sym_LBRACE] = "{",
  [anon_sym_COMMA] = ",",
  [anon_sym_RBRACE] = "}",
  [anon_sym_COLON] = ":",
  [anon_sym_response] = "response",
  [anon_sym_DASH] = "-",
  [sym_uint_lit] = "uint_lit",
  [anon_sym_true] = "true",
  [anon_sym_false] = "false",
  [anon_sym_SQUOTE] = "'",
  [aux_sym_standard_principal_lit_token1] = "standard_principal_lit_token1",
  [aux_sym_standard_principal_lit_token2] = "standard_principal_lit_token2",
  [anon_sym_0x] = "0x",
  [aux_sym_buffer_lit_token1] = "buffer_lit_token1",
  [anon_sym_DQUOTE] = "\"",
  [aux_sym_ascii_string_lit_token1] = "ascii_string_lit_token1",
  [sym_utf8_string_lit] = "utf8_string_lit",
  [anon_sym_some] = "some",
  [sym_none_lit] = "none_lit",
  [anon_sym_ok] = "ok",
  [anon_sym_err] = "err",
  [anon_sym_PLUS] = "+",
  [anon_sym_STAR] = "*",
  [anon_sym_SLASH] = "/",
  [anon_sym_mod] = "mod",
  [anon_sym_pow] = "pow",
  [anon_sym_LT_EQ] = "<=",
  [anon_sym_GT_EQ] = ">=",
  [anon_sym_and] = "and",
  [anon_sym_or] = "or",
  [anon_sym_xor] = "xor",
  [anon_sym_append] = "append",
  [anon_sym_as_DASHcontract] = "as-contract",
  [anon_sym_as_DASHmax_DASHlen_QMARK] = "as-max-len\?",
  [anon_sym_asserts_BANG] = "asserts!",
  [anon_sym_at_DASHblock] = "at-block",
  [anon_sym_begin] = "begin",
  [anon_sym_bit_DASHand] = "bit-and",
  [anon_sym_bit_DASHnot] = "bit-not",
  [anon_sym_bit_DASHor] = "bit-or",
  [anon_sym_bit_DASHshift_DASHleft] = "bit-shift-left",
  [anon_sym_bit_DASHshift_DASHright] = "bit-shift-right",
  [anon_sym_bit_DASHxor] = "bit-xor",
  [anon_sym_buff_DASHto_DASHint_DASHbe] = "buff-to-int-be",
  [anon_sym_buff_DASHto_DASHint_DASHle] = "buff-to-int-le",
  [anon_sym_buff_DASHto_DASHuint_DASHbe] = "buff-to-uint-be",
  [anon_sym_buff_DASHto_DASHuint_DASHle] = "buff-to-uint-le",
  [anon_sym_concat] = "concat",
  [anon_sym_contract_DASHcall_QMARK] = "contract-call\?",
  [anon_sym_contract_DASHhash_QMARK] = "contract-hash\?",
  [anon_sym_contract_DASHof] = "contract-of",
  [anon_sym_default_DASHto] = "default-to",
  [anon_sym_element_DASHat] = "element-at",
  [anon_sym_element_DASHat_QMARK] = "element-at\?",
  [anon_sym_filter] = "filter",
  [anon_sym_fold] = "fold",
  [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = "from-consensus-buff\?",
  [anon_sym_ft_DASHburn_QMARK] = "ft-burn\?",
  [anon_sym_ft_DASHmint_QMARK] = "ft-mint\?",
  [anon_sym_ft_DASHtransfer_QMARK] = "ft-transfer\?",
  [anon_sym_ft_DASHget_DASHsupply] = "ft-get-supply",
  [anon_sym_ft_DASHget_DASHbalance] = "ft-get-balance",
  [anon_sym_get] = "get",
  [anon_sym_get_DASHblock_DASHinfo_QMARK] = "get-block-info\?",
  [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = "get-burn-block-info\?",
  [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = "get-stacks-block-info\?",
  [anon_sym_get_DASHtenure_DASHinfo_QMARK] = "get-tenure-info\?",
  [anon_sym_hash160] = "hash160",
  [anon_sym_if] = "if",
  [anon_sym_index_DASHof] = "index-of",
  [anon_sym_index_DASHof_QMARK] = "index-of\?",
  [anon_sym_int_DASHto_DASHascii] = "int-to-ascii",
  [anon_sym_int_DASHto_DASHutf8] = "int-to-utf8",
  [anon_sym_is_DASHeq] = "is-eq",
  [anon_sym_is_DASHerr] = "is-err",
  [anon_sym_is_DASHnone] = "is-none",
  [anon_sym_is_DASHok] = "is-ok",
  [anon_sym_is_DASHsome] = "is-some",
  [anon_sym_is_DASHstandard] = "is-standard",
  [anon_sym_keccak256] = "keccak256",
  [anon_sym_len] = "len",
  [anon_sym_log2] = "log2",
  [anon_sym_map] = "map",
  [anon_sym_map_DASHdelete] = "map-delete",
  [anon_sym_map_DASHget_QMARK] = "map-get\?",
  [anon_sym_map_DASHinsert] = "map-insert",
  [anon_sym_map_DASHset] = "map-set",
  [anon_sym_match] = "match",
  [anon_sym_merge] = "merge",
  [anon_sym_nft_DASHburn_QMARK] = "nft-burn\?",
  [anon_sym_nft_DASHmint_QMARK] = "nft-mint\?",
  [anon_sym_nft_DASHget_DASHowner_QMARK] = "nft-get-owner\?",
  [anon_sym_nft_DASHtransfer_QMARK] = "nft-transfer\?",
  [anon_sym_not] = "not",
  [anon_sym_principal_DASHconstruct_QMARK] = "principal-construct\?",
  [anon_sym_principal_DASHdestruct_QMARK] = "principal-destruct\?",
  [anon_sym_principal_DASHof_QMARK] = "principal-of\?",
  [anon_sym_print] = "print",
  [anon_sym_replace_DASHat_QMARK] = "replace-at\?",
  [anon_sym_restrict_DASHassets_QMARK] = "restrict-assets\?",
  [anon_sym_secp256k1_DASHrecover_QMARK] = "secp256k1-recover\?",
  [anon_sym_secp256k1_DASHverify] = "secp256k1-verify",
  [anon_sym_secp256r1_DASHverify] = "secp256r1-verify",
  [anon_sym_sha256] = "sha256",
  [anon_sym_sha512] = "sha512",
  [anon_sym_sha512_SLASH256] = "sha512/256",
  [anon_sym_slice_QMARK] = "slice\?",
  [anon_sym_sqrti] = "sqrti",
  [anon_sym_string_DASHto_DASHint_QMARK] = "string-to-int\?",
  [anon_sym_string_DASHto_DASHuint_QMARK] = "string-to-uint\?",
  [anon_sym_stx_DASHaccount] = "stx-account",
  [anon_sym_stx_DASHburn_QMARK] = "stx-burn\?",
  [anon_sym_stx_DASHget_DASHbalance] = "stx-get-balance",
  [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = "stx-transfer-memo\?",
  [anon_sym_stx_DASHtransfer_QMARK] = "stx-transfer\?",
  [anon_sym_to_DASHascii_QMARK] = "to-ascii\?",
  [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = "to-consensus-buff\?",
  [anon_sym_to_DASHint] = "to-int",
  [anon_sym_to_DASHuint] = "to-uint",
  [anon_sym_try_BANG] = "try!",
  [anon_sym_unwrap_BANG] = "unwrap!",
  [anon_sym_unwrap_DASHerr_BANG] = "unwrap-err!",
  [anon_sym_unwrap_DASHerr_DASHpanic] = "unwrap-err-panic",
  [anon_sym_unwrap_DASHpanic] = "unwrap-panic",
  [anon_sym_var_DASHget] = "var-get",
  [anon_sym_var_DASHset] = "var-set",
  [anon_sym_block_DASHheight] = "block-height",
  [anon_sym_burn_DASHblock_DASHheight] = "burn-block-height",
  [anon_sym_chain_DASHid] = "chain-id",
  [anon_sym_contract_DASHcaller] = "contract-caller",
  [anon_sym_is_DASHin_DASHmainnet] = "is-in-mainnet",
  [anon_sym_is_DASHin_DASHregtest] = "is-in-regtest",
  [anon_sym_stacks_DASHblock_DASHheight] = "stacks-block-height",
  [anon_sym_stacks_DASHblock_DASHtime] = "stacks-block-time",
  [anon_sym_stx_DASHliquid_DASHsupply] = "stx-liquid-supply",
  [anon_sym_current_DASHcontract] = "current-contract",
  [anon_sym_tenure_DASHheight] = "tenure-height",
  [anon_sym_tx_DASHsender] = "tx-sender",
  [anon_sym_tx_DASHsponsor_QMARK] = "tx-sponsor\?",
  [anon_sym_block_DASHreward] = "block-reward",
  [anon_sym_burnchain_DASHheader_DASHhash] = "burnchain-header-hash",
  [anon_sym_id_DASHheader_DASHhash] = "id-header-hash",
  [anon_sym_header_DASHhash] = "header-hash",
  [anon_sym_miner_DASHaddress] = "miner-address",
  [anon_sym_miner_DASHspend_DASHtotal] = "miner-spend-total",
  [anon_sym_miner_DASHspend_DASHwinner] = "miner-spend-winner",
  [anon_sym_time] = "time",
  [anon_sym_vrf_DASHseed] = "vrf-seed",
  [anon_sym_pox_DASHaddrs] = "pox-addrs",
  [sym_comment] = "comment",
  [sym_source] = "source",
  [sym__expression] = "_expression",
  [sym__function_call] = "_function_call",
  [sym_trait_definition] = "trait_definition",
  [sym_trait_implementation] = "trait_implementation",
  [sym_trait_usage] = "trait_usage",
  [sym_token_definition] = "token_definition",
  [sym_fungible_token_definition] = "fungible_token_definition",
  [sym_non_fungible_token_definition] = "non_fungible_token_definition",
  [sym_constant_definition] = "constant_definition",
  [sym_variable_definition] = "variable_definition",
  [sym_mapping_definition] = "mapping_definition",
  [sym_function_definition] = "function_definition",
  [sym_private_function] = "private_function",
  [sym_read_only_function] = "read_only_function",
  [sym_public_function] = "public_function",
  [sym__native_function_call] = "_native_function_call",
  [sym_basic_native_form] = "basic_native_form",
  [sym_contract_function_call] = "contract_function_call",
  [sym_let_expression] = "let_expression",
  [sym_local_binding] = "local_binding",
  [sym_function_signature] = "function_signature",
  [sym_function_parameter] = "function_parameter",
  [sym_function_signature_for_trait] = "function_signature_for_trait",
  [sym_parameter_type] = "parameter_type",
  [sym_trait_type] = "trait_type",
  [sym_native_type] = "native_type",
  [sym_buffer_type] = "buffer_type",
  [sym_ascii_string_type] = "ascii_string_type",
  [sym_utf8_string_type] = "utf8_string_type",
  [sym_list_type] = "list_type",
  [sym_optional_type] = "optional_type",
  [sym_tuple_type_for_trait] = "tuple_type_for_trait",
  [sym_tuple_type] = "tuple_type",
  [sym__tuple_type_pair] = "_tuple_type_pair",
  [sym_response_type] = "response_type",
  [sym__parameter] = "_parameter",
  [sym__literal] = "_literal",
  [sym_int_lit] = "int_lit",
  [sym_bool_lit] = "bool_lit",
  [sym_standard_principal_lit] = "standard_principal_lit",
  [sym_contract_principal_lit] = "contract_principal_lit",
  [sym_buffer_lit] = "buffer_lit",
  [sym_ascii_string_lit] = "ascii_string_lit",
  [sym_list_lit] = "list_lit",
  [sym_list_lit_token] = "list_lit_token",
  [sym_some_lit] = "some_lit",
  [sym_tuple_lit] = "tuple_lit",
  [sym__tuple_lit_pair] = "_tuple_lit_pair",
  [sym_response_lit] = "response_lit",
  [sym_native_identifier] = "native_identifier",
  [sym_global] = "global",
  [aux_sym_source_repeat1] = "source_repeat1",
  [aux_sym_trait_definition_repeat1] = "trait_definition_repeat1",
  [aux_sym_basic_native_form_repeat1] = "basic_native_form_repeat1",
  [aux_sym_let_expression_repeat1] = "let_expression_repeat1",
  [aux_sym_let_expression_repeat2] = "let_expression_repeat2",
  [aux_sym_function_signature_repeat1] = "function_signature_repeat1",
  [aux_sym_function_signature_for_trait_repeat1] = "function_signature_for_trait_repeat1",
  [aux_sym_tuple_type_for_trait_repeat1] = "tuple_type_for_trait_repeat1",
  [aux_sym_tuple_type_repeat1] = "tuple_type_repeat1",
  [aux_sym_tuple_lit_repeat1] = "tuple_lit_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym_identifier] = sym_identifier,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_define_DASHtrait] = anon_sym_define_DASHtrait,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_impl_DASHtrait] = anon_sym_impl_DASHtrait,
  [anon_sym_DOT] = anon_sym_DOT,
  [anon_sym_use_DASHtrait] = anon_sym_use_DASHtrait,
  [anon_sym_define_DASHfungible_DASHtoken] = anon_sym_define_DASHfungible_DASHtoken,
  [anon_sym_define_DASHnon_DASHfungible_DASHtoken] = anon_sym_define_DASHnon_DASHfungible_DASHtoken,
  [anon_sym_define_DASHconstant] = anon_sym_define_DASHconstant,
  [anon_sym_define_DASHdata_DASHvar] = anon_sym_define_DASHdata_DASHvar,
  [anon_sym_define_DASHmap] = anon_sym_define_DASHmap,
  [anon_sym_define_DASHprivate] = anon_sym_define_DASHprivate,
  [anon_sym_define_DASHread_DASHonly] = anon_sym_define_DASHread_DASHonly,
  [anon_sym_define_DASHpublic] = anon_sym_define_DASHpublic,
  [anon_sym_let] = anon_sym_let,
  [anon_sym_LT] = anon_sym_LT,
  [anon_sym_GT] = anon_sym_GT,
  [anon_sym_int] = anon_sym_int,
  [anon_sym_uint] = anon_sym_uint,
  [anon_sym_bool] = anon_sym_bool,
  [anon_sym_principal] = anon_sym_principal,
  [anon_sym_buff] = anon_sym_buff,
  [aux_sym_buffer_type_token1] = aux_sym_buffer_type_token1,
  [anon_sym_string_DASHascii] = anon_sym_string_DASHascii,
  [anon_sym_string_DASHutf8] = anon_sym_string_DASHutf8,
  [anon_sym_list] = anon_sym_list,
  [anon_sym_optional] = anon_sym_optional,
  [anon_sym_tuple] = anon_sym_tuple,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_response] = anon_sym_response,
  [anon_sym_DASH] = anon_sym_DASH,
  [sym_uint_lit] = sym_uint_lit,
  [anon_sym_true] = anon_sym_true,
  [anon_sym_false] = anon_sym_false,
  [anon_sym_SQUOTE] = anon_sym_SQUOTE,
  [aux_sym_standard_principal_lit_token1] = aux_sym_standard_principal_lit_token1,
  [aux_sym_standard_principal_lit_token2] = aux_sym_standard_principal_lit_token2,
  [anon_sym_0x] = anon_sym_0x,
  [aux_sym_buffer_lit_token1] = aux_sym_buffer_lit_token1,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [aux_sym_ascii_string_lit_token1] = aux_sym_ascii_string_lit_token1,
  [sym_utf8_string_lit] = sym_utf8_string_lit,
  [anon_sym_some] = anon_sym_some,
  [sym_none_lit] = sym_none_lit,
  [anon_sym_ok] = anon_sym_ok,
  [anon_sym_err] = anon_sym_err,
  [anon_sym_PLUS] = anon_sym_PLUS,
  [anon_sym_STAR] = anon_sym_STAR,
  [anon_sym_SLASH] = anon_sym_SLASH,
  [anon_sym_mod] = anon_sym_mod,
  [anon_sym_pow] = anon_sym_pow,
  [anon_sym_LT_EQ] = anon_sym_LT_EQ,
  [anon_sym_GT_EQ] = anon_sym_GT_EQ,
  [anon_sym_and] = anon_sym_and,
  [anon_sym_or] = anon_sym_or,
  [anon_sym_xor] = anon_sym_xor,
  [anon_sym_append] = anon_sym_append,
  [anon_sym_as_DASHcontract] = anon_sym_as_DASHcontract,
  [anon_sym_as_DASHmax_DASHlen_QMARK] = anon_sym_as_DASHmax_DASHlen_QMARK,
  [anon_sym_asserts_BANG] = anon_sym_asserts_BANG,
  [anon_sym_at_DASHblock] = anon_sym_at_DASHblock,
  [anon_sym_begin] = anon_sym_begin,
  [anon_sym_bit_DASHand] = anon_sym_bit_DASHand,
  [anon_sym_bit_DASHnot] = anon_sym_bit_DASHnot,
  [anon_sym_bit_DASHor] = anon_sym_bit_DASHor,
  [anon_sym_bit_DASHshift_DASHleft] = anon_sym_bit_DASHshift_DASHleft,
  [anon_sym_bit_DASHshift_DASHright] = anon_sym_bit_DASHshift_DASHright,
  [anon_sym_bit_DASHxor] = anon_sym_bit_DASHxor,
  [anon_sym_buff_DASHto_DASHint_DASHbe] = anon_sym_buff_DASHto_DASHint_DASHbe,
  [anon_sym_buff_DASHto_DASHint_DASHle] = anon_sym_buff_DASHto_DASHint_DASHle,
  [anon_sym_buff_DASHto_DASHuint_DASHbe] = anon_sym_buff_DASHto_DASHuint_DASHbe,
  [anon_sym_buff_DASHto_DASHuint_DASHle] = anon_sym_buff_DASHto_DASHuint_DASHle,
  [anon_sym_concat] = anon_sym_concat,
  [anon_sym_contract_DASHcall_QMARK] = anon_sym_contract_DASHcall_QMARK,
  [anon_sym_contract_DASHhash_QMARK] = anon_sym_contract_DASHhash_QMARK,
  [anon_sym_contract_DASHof] = anon_sym_contract_DASHof,
  [anon_sym_default_DASHto] = anon_sym_default_DASHto,
  [anon_sym_element_DASHat] = anon_sym_element_DASHat,
  [anon_sym_element_DASHat_QMARK] = anon_sym_element_DASHat_QMARK,
  [anon_sym_filter] = anon_sym_filter,
  [anon_sym_fold] = anon_sym_fold,
  [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = anon_sym_from_DASHconsensus_DASHbuff_QMARK,
  [anon_sym_ft_DASHburn_QMARK] = anon_sym_ft_DASHburn_QMARK,
  [anon_sym_ft_DASHmint_QMARK] = anon_sym_ft_DASHmint_QMARK,
  [anon_sym_ft_DASHtransfer_QMARK] = anon_sym_ft_DASHtransfer_QMARK,
  [anon_sym_ft_DASHget_DASHsupply] = anon_sym_ft_DASHget_DASHsupply,
  [anon_sym_ft_DASHget_DASHbalance] = anon_sym_ft_DASHget_DASHbalance,
  [anon_sym_get] = anon_sym_get,
  [anon_sym_get_DASHblock_DASHinfo_QMARK] = anon_sym_get_DASHblock_DASHinfo_QMARK,
  [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK,
  [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK,
  [anon_sym_get_DASHtenure_DASHinfo_QMARK] = anon_sym_get_DASHtenure_DASHinfo_QMARK,
  [anon_sym_hash160] = anon_sym_hash160,
  [anon_sym_if] = anon_sym_if,
  [anon_sym_index_DASHof] = anon_sym_index_DASHof,
  [anon_sym_index_DASHof_QMARK] = anon_sym_index_DASHof_QMARK,
  [anon_sym_int_DASHto_DASHascii] = anon_sym_int_DASHto_DASHascii,
  [anon_sym_int_DASHto_DASHutf8] = anon_sym_int_DASHto_DASHutf8,
  [anon_sym_is_DASHeq] = anon_sym_is_DASHeq,
  [anon_sym_is_DASHerr] = anon_sym_is_DASHerr,
  [anon_sym_is_DASHnone] = anon_sym_is_DASHnone,
  [anon_sym_is_DASHok] = anon_sym_is_DASHok,
  [anon_sym_is_DASHsome] = anon_sym_is_DASHsome,
  [anon_sym_is_DASHstandard] = anon_sym_is_DASHstandard,
  [anon_sym_keccak256] = anon_sym_keccak256,
  [anon_sym_len] = anon_sym_len,
  [anon_sym_log2] = anon_sym_log2,
  [anon_sym_map] = anon_sym_map,
  [anon_sym_map_DASHdelete] = anon_sym_map_DASHdelete,
  [anon_sym_map_DASHget_QMARK] = anon_sym_map_DASHget_QMARK,
  [anon_sym_map_DASHinsert] = anon_sym_map_DASHinsert,
  [anon_sym_map_DASHset] = anon_sym_map_DASHset,
  [anon_sym_match] = anon_sym_match,
  [anon_sym_merge] = anon_sym_merge,
  [anon_sym_nft_DASHburn_QMARK] = anon_sym_nft_DASHburn_QMARK,
  [anon_sym_nft_DASHmint_QMARK] = anon_sym_nft_DASHmint_QMARK,
  [anon_sym_nft_DASHget_DASHowner_QMARK] = anon_sym_nft_DASHget_DASHowner_QMARK,
  [anon_sym_nft_DASHtransfer_QMARK] = anon_sym_nft_DASHtransfer_QMARK,
  [anon_sym_not] = anon_sym_not,
  [anon_sym_principal_DASHconstruct_QMARK] = anon_sym_principal_DASHconstruct_QMARK,
  [anon_sym_principal_DASHdestruct_QMARK] = anon_sym_principal_DASHdestruct_QMARK,
  [anon_sym_principal_DASHof_QMARK] = anon_sym_principal_DASHof_QMARK,
  [anon_sym_print] = anon_sym_print,
  [anon_sym_replace_DASHat_QMARK] = anon_sym_replace_DASHat_QMARK,
  [anon_sym_restrict_DASHassets_QMARK] = anon_sym_restrict_DASHassets_QMARK,
  [anon_sym_secp256k1_DASHrecover_QMARK] = anon_sym_secp256k1_DASHrecover_QMARK,
  [anon_sym_secp256k1_DASHverify] = anon_sym_secp256k1_DASHverify,
  [anon_sym_secp256r1_DASHverify] = anon_sym_secp256r1_DASHverify,
  [anon_sym_sha256] = anon_sym_sha256,
  [anon_sym_sha512] = anon_sym_sha512,
  [anon_sym_sha512_SLASH256] = anon_sym_sha512_SLASH256,
  [anon_sym_slice_QMARK] = anon_sym_slice_QMARK,
  [anon_sym_sqrti] = anon_sym_sqrti,
  [anon_sym_string_DASHto_DASHint_QMARK] = anon_sym_string_DASHto_DASHint_QMARK,
  [anon_sym_string_DASHto_DASHuint_QMARK] = anon_sym_string_DASHto_DASHuint_QMARK,
  [anon_sym_stx_DASHaccount] = anon_sym_stx_DASHaccount,
  [anon_sym_stx_DASHburn_QMARK] = anon_sym_stx_DASHburn_QMARK,
  [anon_sym_stx_DASHget_DASHbalance] = anon_sym_stx_DASHget_DASHbalance,
  [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = anon_sym_stx_DASHtransfer_DASHmemo_QMARK,
  [anon_sym_stx_DASHtransfer_QMARK] = anon_sym_stx_DASHtransfer_QMARK,
  [anon_sym_to_DASHascii_QMARK] = anon_sym_to_DASHascii_QMARK,
  [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = anon_sym_to_DASHconsensus_DASHbuff_QMARK,
  [anon_sym_to_DASHint] = anon_sym_to_DASHint,
  [anon_sym_to_DASHuint] = anon_sym_to_DASHuint,
  [anon_sym_try_BANG] = anon_sym_try_BANG,
  [anon_sym_unwrap_BANG] = anon_sym_unwrap_BANG,
  [anon_sym_unwrap_DASHerr_BANG] = anon_sym_unwrap_DASHerr_BANG,
  [anon_sym_unwrap_DASHerr_DASHpanic] = anon_sym_unwrap_DASHerr_DASHpanic,
  [anon_sym_unwrap_DASHpanic] = anon_sym_unwrap_DASHpanic,
  [anon_sym_var_DASHget] = anon_sym_var_DASHget,
  [anon_sym_var_DASHset] = anon_sym_var_DASHset,
  [anon_sym_block_DASHheight] = anon_sym_block_DASHheight,
  [anon_sym_burn_DASHblock_DASHheight] = anon_sym_burn_DASHblock_DASHheight,
  [anon_sym_chain_DASHid] = anon_sym_chain_DASHid,
  [anon_sym_contract_DASHcaller] = anon_sym_contract_DASHcaller,
  [anon_sym_is_DASHin_DASHmainnet] = anon_sym_is_DASHin_DASHmainnet,
  [anon_sym_is_DASHin_DASHregtest] = anon_sym_is_DASHin_DASHregtest,
  [anon_sym_stacks_DASHblock_DASHheight] = anon_sym_stacks_DASHblock_DASHheight,
  [anon_sym_stacks_DASHblock_DASHtime] = anon_sym_stacks_DASHblock_DASHtime,
  [anon_sym_stx_DASHliquid_DASHsupply] = anon_sym_stx_DASHliquid_DASHsupply,
  [anon_sym_current_DASHcontract] = anon_sym_current_DASHcontract,
  [anon_sym_tenure_DASHheight] = anon_sym_tenure_DASHheight,
  [anon_sym_tx_DASHsender] = anon_sym_tx_DASHsender,
  [anon_sym_tx_DASHsponsor_QMARK] = anon_sym_tx_DASHsponsor_QMARK,
  [anon_sym_block_DASHreward] = anon_sym_block_DASHreward,
  [anon_sym_burnchain_DASHheader_DASHhash] = anon_sym_burnchain_DASHheader_DASHhash,
  [anon_sym_id_DASHheader_DASHhash] = anon_sym_id_DASHheader_DASHhash,
  [anon_sym_header_DASHhash] = anon_sym_header_DASHhash,
  [anon_sym_miner_DASHaddress] = anon_sym_miner_DASHaddress,
  [anon_sym_miner_DASHspend_DASHtotal] = anon_sym_miner_DASHspend_DASHtotal,
  [anon_sym_miner_DASHspend_DASHwinner] = anon_sym_miner_DASHspend_DASHwinner,
  [anon_sym_time] = anon_sym_time,
  [anon_sym_vrf_DASHseed] = anon_sym_vrf_DASHseed,
  [anon_sym_pox_DASHaddrs] = anon_sym_pox_DASHaddrs,
  [sym_comment] = sym_comment,
  [sym_source] = sym_source,
  [sym__expression] = sym__expression,
  [sym__function_call] = sym__function_call,
  [sym_trait_definition] = sym_trait_definition,
  [sym_trait_implementation] = sym_trait_implementation,
  [sym_trait_usage] = sym_trait_usage,
  [sym_token_definition] = sym_token_definition,
  [sym_fungible_token_definition] = sym_fungible_token_definition,
  [sym_non_fungible_token_definition] = sym_non_fungible_token_definition,
  [sym_constant_definition] = sym_constant_definition,
  [sym_variable_definition] = sym_variable_definition,
  [sym_mapping_definition] = sym_mapping_definition,
  [sym_function_definition] = sym_function_definition,
  [sym_private_function] = sym_private_function,
  [sym_read_only_function] = sym_read_only_function,
  [sym_public_function] = sym_public_function,
  [sym__native_function_call] = sym__native_function_call,
  [sym_basic_native_form] = sym_basic_native_form,
  [sym_contract_function_call] = sym_contract_function_call,
  [sym_let_expression] = sym_let_expression,
  [sym_local_binding] = sym_local_binding,
  [sym_function_signature] = sym_function_signature,
  [sym_function_parameter] = sym_function_parameter,
  [sym_function_signature_for_trait] = sym_function_signature_for_trait,
  [sym_parameter_type] = sym_parameter_type,
  [sym_trait_type] = sym_trait_type,
  [sym_native_type] = sym_native_type,
  [sym_buffer_type] = sym_buffer_type,
  [sym_ascii_string_type] = sym_ascii_string_type,
  [sym_utf8_string_type] = sym_utf8_string_type,
  [sym_list_type] = sym_list_type,
  [sym_optional_type] = sym_optional_type,
  [sym_tuple_type_for_trait] = sym_tuple_type_for_trait,
  [sym_tuple_type] = sym_tuple_type,
  [sym__tuple_type_pair] = sym__tuple_type_pair,
  [sym_response_type] = sym_response_type,
  [sym__parameter] = sym__parameter,
  [sym__literal] = sym__literal,
  [sym_int_lit] = sym_int_lit,
  [sym_bool_lit] = sym_bool_lit,
  [sym_standard_principal_lit] = sym_standard_principal_lit,
  [sym_contract_principal_lit] = sym_contract_principal_lit,
  [sym_buffer_lit] = sym_buffer_lit,
  [sym_ascii_string_lit] = sym_ascii_string_lit,
  [sym_list_lit] = sym_list_lit,
  [sym_list_lit_token] = sym_list_lit_token,
  [sym_some_lit] = sym_some_lit,
  [sym_tuple_lit] = sym_tuple_lit,
  [sym__tuple_lit_pair] = sym__tuple_lit_pair,
  [sym_response_lit] = sym_response_lit,
  [sym_native_identifier] = sym_native_identifier,
  [sym_global] = sym_global,
  [aux_sym_source_repeat1] = aux_sym_source_repeat1,
  [aux_sym_trait_definition_repeat1] = aux_sym_trait_definition_repeat1,
  [aux_sym_basic_native_form_repeat1] = aux_sym_basic_native_form_repeat1,
  [aux_sym_let_expression_repeat1] = aux_sym_let_expression_repeat1,
  [aux_sym_let_expression_repeat2] = aux_sym_let_expression_repeat2,
  [aux_sym_function_signature_repeat1] = aux_sym_function_signature_repeat1,
  [aux_sym_function_signature_for_trait_repeat1] = aux_sym_function_signature_for_trait_repeat1,
  [aux_sym_tuple_type_for_trait_repeat1] = aux_sym_tuple_type_for_trait_repeat1,
  [aux_sym_tuple_type_repeat1] = aux_sym_tuple_type_repeat1,
  [aux_sym_tuple_lit_repeat1] = aux_sym_tuple_lit_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHtrait] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_impl_DASHtrait] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_use_DASHtrait] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHfungible_DASHtoken] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHnon_DASHfungible_DASHtoken] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHconstant] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHdata_DASHvar] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHmap] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHprivate] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHread_DASHonly] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_define_DASHpublic] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_let] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_int] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_uint] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bool] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_principal] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_buff] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_buffer_type_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_string_DASHascii] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_string_DASHutf8] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_list] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_optional] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tuple] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_response] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [sym_uint_lit] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_true] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_false] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_standard_principal_lit_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_standard_principal_lit_token2] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_0x] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_buffer_lit_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_ascii_string_lit_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_utf8_string_lit] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_some] = {
    .visible = true,
    .named = false,
  },
  [sym_none_lit] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_ok] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_err] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PLUS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_STAR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_mod] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_pow] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_and] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_or] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_xor] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_append] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_as_DASHcontract] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_as_DASHmax_DASHlen_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_asserts_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_at_DASHblock] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_begin] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHand] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHnot] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHor] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHshift_DASHleft] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHshift_DASHright] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bit_DASHxor] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_buff_DASHto_DASHint_DASHbe] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_buff_DASHto_DASHint_DASHle] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_buff_DASHto_DASHuint_DASHbe] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_buff_DASHto_DASHuint_DASHle] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_concat] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_contract_DASHcall_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_contract_DASHhash_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_contract_DASHof] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_default_DASHto] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_element_DASHat] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_element_DASHat_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_filter] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_fold] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ft_DASHburn_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ft_DASHmint_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ft_DASHtransfer_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ft_DASHget_DASHsupply] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ft_DASHget_DASHbalance] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_get] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_get_DASHblock_DASHinfo_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_get_DASHtenure_DASHinfo_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_hash160] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_if] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_index_DASHof] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_index_DASHof_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_int_DASHto_DASHascii] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_int_DASHto_DASHutf8] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHeq] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHerr] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHnone] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHok] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHsome] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHstandard] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_keccak256] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_len] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_log2] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_map] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_map_DASHdelete] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_map_DASHget_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_map_DASHinsert] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_map_DASHset] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_match] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_merge] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_nft_DASHburn_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_nft_DASHmint_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_nft_DASHget_DASHowner_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_nft_DASHtransfer_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_not] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_principal_DASHconstruct_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_principal_DASHdestruct_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_principal_DASHof_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_print] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_replace_DASHat_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_restrict_DASHassets_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_secp256k1_DASHrecover_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_secp256k1_DASHverify] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_secp256r1_DASHverify] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sha256] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sha512] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sha512_SLASH256] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_slice_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sqrti] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_string_DASHto_DASHint_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_string_DASHto_DASHuint_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHaccount] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHburn_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHget_DASHbalance] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHtransfer_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_to_DASHascii_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_to_DASHint] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_to_DASHuint] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_try_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_unwrap_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_unwrap_DASHerr_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_unwrap_DASHerr_DASHpanic] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_unwrap_DASHpanic] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_var_DASHget] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_var_DASHset] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_block_DASHheight] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_burn_DASHblock_DASHheight] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_chain_DASHid] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_contract_DASHcaller] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHin_DASHmainnet] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_is_DASHin_DASHregtest] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stacks_DASHblock_DASHheight] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stacks_DASHblock_DASHtime] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_stx_DASHliquid_DASHsupply] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_current_DASHcontract] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tenure_DASHheight] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tx_DASHsender] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tx_DASHsponsor_QMARK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_block_DASHreward] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_burnchain_DASHheader_DASHhash] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_id_DASHheader_DASHhash] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_header_DASHhash] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_miner_DASHaddress] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_miner_DASHspend_DASHtotal] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_miner_DASHspend_DASHwinner] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_time] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_vrf_DASHseed] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_pox_DASHaddrs] = {
    .visible = true,
    .named = false,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_source] = {
    .visible = true,
    .named = true,
  },
  [sym__expression] = {
    .visible = false,
    .named = true,
  },
  [sym__function_call] = {
    .visible = false,
    .named = true,
  },
  [sym_trait_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_trait_implementation] = {
    .visible = true,
    .named = true,
  },
  [sym_trait_usage] = {
    .visible = true,
    .named = true,
  },
  [sym_token_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_fungible_token_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_non_fungible_token_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_constant_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_variable_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_mapping_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_function_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_private_function] = {
    .visible = true,
    .named = true,
  },
  [sym_read_only_function] = {
    .visible = true,
    .named = true,
  },
  [sym_public_function] = {
    .visible = true,
    .named = true,
  },
  [sym__native_function_call] = {
    .visible = false,
    .named = true,
  },
  [sym_basic_native_form] = {
    .visible = true,
    .named = true,
  },
  [sym_contract_function_call] = {
    .visible = true,
    .named = true,
  },
  [sym_let_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_local_binding] = {
    .visible = true,
    .named = true,
  },
  [sym_function_signature] = {
    .visible = true,
    .named = true,
  },
  [sym_function_parameter] = {
    .visible = true,
    .named = true,
  },
  [sym_function_signature_for_trait] = {
    .visible = true,
    .named = true,
  },
  [sym_parameter_type] = {
    .visible = true,
    .named = true,
  },
  [sym_trait_type] = {
    .visible = true,
    .named = true,
  },
  [sym_native_type] = {
    .visible = true,
    .named = true,
  },
  [sym_buffer_type] = {
    .visible = true,
    .named = true,
  },
  [sym_ascii_string_type] = {
    .visible = true,
    .named = true,
  },
  [sym_utf8_string_type] = {
    .visible = true,
    .named = true,
  },
  [sym_list_type] = {
    .visible = true,
    .named = true,
  },
  [sym_optional_type] = {
    .visible = true,
    .named = true,
  },
  [sym_tuple_type_for_trait] = {
    .visible = true,
    .named = true,
  },
  [sym_tuple_type] = {
    .visible = true,
    .named = true,
  },
  [sym__tuple_type_pair] = {
    .visible = false,
    .named = true,
  },
  [sym_response_type] = {
    .visible = true,
    .named = true,
  },
  [sym__parameter] = {
    .visible = false,
    .named = true,
  },
  [sym__literal] = {
    .visible = false,
    .named = true,
  },
  [sym_int_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_bool_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_standard_principal_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_contract_principal_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_buffer_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_ascii_string_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_list_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_list_lit_token] = {
    .visible = true,
    .named = true,
  },
  [sym_some_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_tuple_lit] = {
    .visible = true,
    .named = true,
  },
  [sym__tuple_lit_pair] = {
    .visible = false,
    .named = true,
  },
  [sym_response_lit] = {
    .visible = true,
    .named = true,
  },
  [sym_native_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_global] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_trait_definition_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_basic_native_form_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_let_expression_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_let_expression_repeat2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_function_signature_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_function_signature_for_trait_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tuple_type_for_trait_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tuple_type_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tuple_lit_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum {
  field_key = 1,
  field_key_type = 2,
  field_operator = 3,
  field_trait_alias = 4,
  field_trait_name = 5,
  field_value = 6,
  field_value_type = 7,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_key] = "key",
  [field_key_type] = "key_type",
  [field_operator] = "operator",
  [field_trait_alias] = "trait_alias",
  [field_trait_name] = "trait_name",
  [field_value] = "value",
  [field_value_type] = "value_type",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
  [2] = {.index = 1, .length = 2},
  [3] = {.index = 3, .length = 2},
  [4] = {.index = 5, .length = 4},
  [5] = {.index = 9, .length = 1},
  [6] = {.index = 10, .length = 2},
  [7] = {.index = 12, .length = 2},
  [8] = {.index = 14, .length = 4},
  [9] = {.index = 18, .length = 2},
  [10] = {.index = 20, .length = 2},
  [11] = {.index = 22, .length = 4},
  [12] = {.index = 26, .length = 2},
  [13] = {.index = 28, .length = 2},
  [14] = {.index = 30, .length = 2},
  [15] = {.index = 32, .length = 4},
  [16] = {.index = 36, .length = 2},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_operator, 1},
  [1] =
    {field_key, 0, .inherited = true},
    {field_value, 0, .inherited = true},
  [3] =
    {field_key, 1, .inherited = true},
    {field_value, 1, .inherited = true},
  [5] =
    {field_key, 0, .inherited = true},
    {field_key, 1, .inherited = true},
    {field_value, 0, .inherited = true},
    {field_value, 1, .inherited = true},
  [9] =
    {field_trait_name, 4},
  [10] =
    {field_key, 0, .inherited = true},
    {field_value_type, 0, .inherited = true},
  [12] =
    {field_key, 1, .inherited = true},
    {field_value_type, 1, .inherited = true},
  [14] =
    {field_key, 0, .inherited = true},
    {field_key, 1, .inherited = true},
    {field_value_type, 0, .inherited = true},
    {field_value_type, 1, .inherited = true},
  [18] =
    {field_key_type, 3},
    {field_value_type, 4},
  [20] =
    {field_key, 0},
    {field_value, 2},
  [22] =
    {field_key, 1, .inherited = true},
    {field_key, 2, .inherited = true},
    {field_value, 1, .inherited = true},
    {field_value, 2, .inherited = true},
  [26] =
    {field_trait_alias, 2},
    {field_trait_name, 5},
  [28] =
    {field_key, 2, .inherited = true},
    {field_value_type, 2, .inherited = true},
  [30] =
    {field_key, 0},
    {field_value_type, 2},
  [32] =
    {field_key, 1, .inherited = true},
    {field_key, 2, .inherited = true},
    {field_value_type, 1, .inherited = true},
    {field_value_type, 2, .inherited = true},
  [36] =
    {field_key, 1},
    {field_value_type, 2},
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
  [57] = 57,
  [58] = 58,
  [59] = 59,
  [60] = 60,
  [61] = 61,
  [62] = 62,
  [63] = 63,
  [64] = 64,
  [65] = 65,
  [66] = 66,
  [67] = 67,
  [68] = 68,
  [69] = 69,
  [70] = 70,
  [71] = 71,
  [72] = 72,
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
  [114] = 114,
  [115] = 115,
  [116] = 116,
  [117] = 117,
  [118] = 118,
  [119] = 119,
  [120] = 120,
  [121] = 121,
  [122] = 122,
  [123] = 123,
  [124] = 124,
  [125] = 125,
  [126] = 126,
  [127] = 127,
  [128] = 128,
  [129] = 129,
  [130] = 130,
  [131] = 131,
  [132] = 132,
  [133] = 133,
  [134] = 134,
  [135] = 135,
  [136] = 136,
  [137] = 137,
  [138] = 138,
  [139] = 139,
  [140] = 140,
  [141] = 141,
  [142] = 142,
  [143] = 143,
  [144] = 144,
  [145] = 145,
  [146] = 146,
  [147] = 147,
  [148] = 148,
  [149] = 149,
  [150] = 150,
  [151] = 151,
  [152] = 152,
  [153] = 153,
  [154] = 154,
  [155] = 155,
  [156] = 156,
  [157] = 157,
  [158] = 158,
  [159] = 159,
  [160] = 160,
  [161] = 161,
  [162] = 162,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 166,
  [167] = 167,
  [168] = 168,
  [169] = 169,
  [170] = 170,
  [171] = 171,
  [172] = 172,
  [173] = 173,
  [174] = 174,
  [175] = 175,
  [176] = 176,
  [177] = 177,
  [178] = 178,
  [179] = 179,
  [180] = 180,
  [181] = 181,
  [182] = 182,
  [183] = 183,
  [184] = 184,
  [185] = 185,
  [186] = 186,
  [187] = 187,
  [188] = 188,
  [189] = 189,
  [190] = 190,
  [191] = 191,
  [192] = 192,
  [193] = 193,
  [194] = 194,
  [195] = 195,
  [196] = 196,
  [197] = 197,
  [198] = 198,
  [199] = 199,
  [200] = 200,
  [201] = 201,
  [202] = 202,
  [203] = 203,
  [204] = 204,
  [205] = 205,
  [206] = 206,
  [207] = 207,
  [208] = 208,
  [209] = 209,
  [210] = 210,
  [211] = 211,
  [212] = 212,
  [213] = 213,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(56);
      if (lookahead == '"') ADVANCE(74);
      if (lookahead == '\'') ADVANCE(69);
      if (lookahead == '(') ADVANCE(57);
      if (lookahead == ')') ADVANCE(58);
      if (lookahead == '*') ADVANCE(82);
      if (lookahead == '+') ADVANCE(81);
      if (lookahead == ',') ADVANCE(65);
      if (lookahead == '-') ADVANCE(68);
      if (lookahead == '.') ADVANCE(59);
      if (lookahead == '/') ADVANCE(83);
      if (lookahead == '0') ADVANCE(62);
      if (lookahead == ':') ADVANCE(67);
      if (lookahead == ';') ADVANCE(15);
      if (lookahead == '<') ADVANCE(60);
      if (lookahead == '>') ADVANCE(61);
      if (lookahead == 's') ADVANCE(93);
      if (lookahead == 'u') ADVANCE(87);
      if (lookahead == '{') ADVANCE(64);
      if (lookahead == '}') ADVANCE(66);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) SKIP(0)
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(63);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 1:
      if (lookahead == '\n') ADVANCE(6);
      if (lookahead == '\r') ADVANCE(2);
      if (lookahead == '"') ADVANCE(80);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 2:
      if (lookahead == '\n') ADVANCE(6);
      if (lookahead == '"') ADVANCE(80);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 3:
      if (lookahead == '\n') SKIP(3)
      if (lookahead == '"') ADVANCE(74);
      if (lookahead == ';') ADVANCE(78);
      if (lookahead == '\\') ADVANCE(5);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) ADVANCE(77);
      if (lookahead != 0) ADVANCE(79);
      END_STATE();
    case 4:
      if (lookahead == '\n') ADVANCE(75);
      END_STATE();
    case 5:
      if (lookahead == '\n') ADVANCE(75);
      if (lookahead == '\r') ADVANCE(4);
      END_STATE();
    case 6:
      if (lookahead == '"') ADVANCE(80);
      END_STATE();
    case 7:
      if (lookahead == '"') ADVANCE(80);
      if (lookahead == '\\') ADVANCE(1);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(8);
      END_STATE();
    case 8:
      if (lookahead == '"') ADVANCE(80);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(8);
      END_STATE();
    case 9:
      if (lookahead == '(') ADVANCE(57);
      if (lookahead == ')') ADVANCE(58);
      if (lookahead == '*') ADVANCE(82);
      if (lookahead == '+') ADVANCE(81);
      if (lookahead == ',') ADVANCE(65);
      if (lookahead == '-') ADVANCE(68);
      if (lookahead == '/') ADVANCE(83);
      if (lookahead == ';') ADVANCE(15);
      if (lookahead == '<') ADVANCE(60);
      if (lookahead == '>') ADVANCE(61);
      if (lookahead == 's') ADVANCE(93);
      if (lookahead == '{') ADVANCE(64);
      if (lookahead == '}') ADVANCE(66);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) SKIP(9)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(63);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 10:
      if (lookahead == '2') ADVANCE(11);
      END_STATE();
    case 11:
      if (lookahead == '5') ADVANCE(12);
      END_STATE();
    case 12:
      if (lookahead == '6') ADVANCE(86);
      END_STATE();
    case 13:
      if (lookahead == ';') ADVANCE(15);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) SKIP(13)
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(54);
      END_STATE();
    case 14:
      if (lookahead == ';') ADVANCE(15);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) SKIP(14)
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(73);
      END_STATE();
    case 15:
      if (lookahead == ';') ADVANCE(95);
      END_STATE();
    case 16:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(71);
      END_STATE();
    case 17:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(16);
      END_STATE();
    case 18:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(17);
      END_STATE();
    case 19:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(18);
      END_STATE();
    case 20:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(19);
      END_STATE();
    case 21:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(20);
      END_STATE();
    case 22:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(21);
      END_STATE();
    case 23:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(22);
      END_STATE();
    case 24:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(23);
      END_STATE();
    case 25:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(24);
      END_STATE();
    case 26:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(25);
      END_STATE();
    case 27:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(26);
      END_STATE();
    case 28:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(27);
      END_STATE();
    case 29:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(28);
      END_STATE();
    case 30:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(29);
      END_STATE();
    case 31:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(30);
      END_STATE();
    case 32:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(31);
      END_STATE();
    case 33:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(32);
      END_STATE();
    case 34:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(33);
      END_STATE();
    case 35:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(34);
      END_STATE();
    case 36:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(35);
      END_STATE();
    case 37:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(36);
      END_STATE();
    case 38:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(37);
      END_STATE();
    case 39:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(38);
      END_STATE();
    case 40:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(39);
      END_STATE();
    case 41:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(40);
      END_STATE();
    case 42:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(41);
      END_STATE();
    case 43:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(42);
      END_STATE();
    case 44:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(43);
      END_STATE();
    case 45:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(44);
      END_STATE();
    case 46:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(45);
      END_STATE();
    case 47:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(46);
      END_STATE();
    case 48:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(47);
      END_STATE();
    case 49:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(48);
      END_STATE();
    case 50:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(49);
      END_STATE();
    case 51:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(50);
      END_STATE();
    case 52:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(51);
      END_STATE();
    case 53:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(52);
      END_STATE();
    case 54:
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(53);
      END_STATE();
    case 55:
      if (lookahead != 0 &&
          lookahead != '\r') ADVANCE(95);
      if (lookahead == '\r') ADVANCE(96);
      END_STATE();
    case 56:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 57:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 58:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 59:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 60:
      ACCEPT_TOKEN(anon_sym_LT);
      if (lookahead == '=') ADVANCE(84);
      END_STATE();
    case 61:
      ACCEPT_TOKEN(anon_sym_GT);
      if (lookahead == '=') ADVANCE(85);
      END_STATE();
    case 62:
      ACCEPT_TOKEN(aux_sym_buffer_type_token1);
      if (lookahead == 'x') ADVANCE(72);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(63);
      END_STATE();
    case 63:
      ACCEPT_TOKEN(aux_sym_buffer_type_token1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(63);
      END_STATE();
    case 64:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 65:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 66:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 67:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 68:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 69:
      ACCEPT_TOKEN(anon_sym_SQUOTE);
      END_STATE();
    case 70:
      ACCEPT_TOKEN(aux_sym_standard_principal_lit_token1);
      END_STATE();
    case 71:
      ACCEPT_TOKEN(aux_sym_standard_principal_lit_token2);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z')) ADVANCE(70);
      END_STATE();
    case 72:
      ACCEPT_TOKEN(anon_sym_0x);
      END_STATE();
    case 73:
      ACCEPT_TOKEN(aux_sym_buffer_lit_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'F') ||
          ('a' <= lookahead && lookahead <= 'f')) ADVANCE(73);
      END_STATE();
    case 74:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 75:
      ACCEPT_TOKEN(aux_sym_ascii_string_lit_token1);
      END_STATE();
    case 76:
      ACCEPT_TOKEN(aux_sym_ascii_string_lit_token1);
      if (lookahead == '"') ADVANCE(95);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(76);
      END_STATE();
    case 77:
      ACCEPT_TOKEN(aux_sym_ascii_string_lit_token1);
      if (lookahead == ';') ADVANCE(78);
      if (lookahead == '\\') ADVANCE(5);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) ADVANCE(77);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '"') ADVANCE(79);
      END_STATE();
    case 78:
      ACCEPT_TOKEN(aux_sym_ascii_string_lit_token1);
      if (lookahead == ';') ADVANCE(76);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '"' &&
          lookahead != '\\') ADVANCE(79);
      END_STATE();
    case 79:
      ACCEPT_TOKEN(aux_sym_ascii_string_lit_token1);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '"' &&
          lookahead != '\\') ADVANCE(79);
      END_STATE();
    case 80:
      ACCEPT_TOKEN(sym_utf8_string_lit);
      END_STATE();
    case 81:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 82:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 83:
      ACCEPT_TOKEN(anon_sym_SLASH);
      END_STATE();
    case 84:
      ACCEPT_TOKEN(anon_sym_LT_EQ);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(anon_sym_GT_EQ);
      END_STATE();
    case 86:
      ACCEPT_TOKEN(anon_sym_sha512_SLASH256);
      END_STATE();
    case 87:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '"') ADVANCE(7);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 88:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '/') ADVANCE(10);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 89:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '1') ADVANCE(90);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 90:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '2') ADVANCE(88);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 91:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '5') ADVANCE(89);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 92:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(91);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 93:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(92);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 94:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == '!' ||
          lookahead == '-' ||
          ('0' <= lookahead && lookahead <= '9') ||
          lookahead == '?' ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(94);
      END_STATE();
    case 95:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead == '\\') ADVANCE(55);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(95);
      END_STATE();
    case 96:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\\') ADVANCE(95);
      if (lookahead == '\\') ADVANCE(55);
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
      if (lookahead == 'a') ADVANCE(1);
      if (lookahead == 'b') ADVANCE(2);
      if (lookahead == 'c') ADVANCE(3);
      if (lookahead == 'd') ADVANCE(4);
      if (lookahead == 'e') ADVANCE(5);
      if (lookahead == 'f') ADVANCE(6);
      if (lookahead == 'g') ADVANCE(7);
      if (lookahead == 'h') ADVANCE(8);
      if (lookahead == 'i') ADVANCE(9);
      if (lookahead == 'k') ADVANCE(10);
      if (lookahead == 'l') ADVANCE(11);
      if (lookahead == 'm') ADVANCE(12);
      if (lookahead == 'n') ADVANCE(13);
      if (lookahead == 'o') ADVANCE(14);
      if (lookahead == 'p') ADVANCE(15);
      if (lookahead == 'r') ADVANCE(16);
      if (lookahead == 's') ADVANCE(17);
      if (lookahead == 't') ADVANCE(18);
      if (lookahead == 'u') ADVANCE(19);
      if (lookahead == 'v') ADVANCE(20);
      if (lookahead == 'x') ADVANCE(21);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ' ||
          lookahead == 160 ||
          lookahead == 8203 ||
          lookahead == 8288 ||
          lookahead == 65279) SKIP(0)
      END_STATE();
    case 1:
      if (lookahead == 'n') ADVANCE(22);
      if (lookahead == 'p') ADVANCE(23);
      if (lookahead == 's') ADVANCE(24);
      if (lookahead == 't') ADVANCE(25);
      END_STATE();
    case 2:
      if (lookahead == 'e') ADVANCE(26);
      if (lookahead == 'i') ADVANCE(27);
      if (lookahead == 'l') ADVANCE(28);
      if (lookahead == 'o') ADVANCE(29);
      if (lookahead == 'u') ADVANCE(30);
      END_STATE();
    case 3:
      if (lookahead == 'h') ADVANCE(31);
      if (lookahead == 'o') ADVANCE(32);
      if (lookahead == 'u') ADVANCE(33);
      END_STATE();
    case 4:
      if (lookahead == 'e') ADVANCE(34);
      END_STATE();
    case 5:
      if (lookahead == 'l') ADVANCE(35);
      if (lookahead == 'r') ADVANCE(36);
      END_STATE();
    case 6:
      if (lookahead == 'a') ADVANCE(37);
      if (lookahead == 'i') ADVANCE(38);
      if (lookahead == 'o') ADVANCE(39);
      if (lookahead == 'r') ADVANCE(40);
      if (lookahead == 't') ADVANCE(41);
      END_STATE();
    case 7:
      if (lookahead == 'e') ADVANCE(42);
      END_STATE();
    case 8:
      if (lookahead == 'a') ADVANCE(43);
      if (lookahead == 'e') ADVANCE(44);
      END_STATE();
    case 9:
      if (lookahead == 'd') ADVANCE(45);
      if (lookahead == 'f') ADVANCE(46);
      if (lookahead == 'm') ADVANCE(47);
      if (lookahead == 'n') ADVANCE(48);
      if (lookahead == 's') ADVANCE(49);
      END_STATE();
    case 10:
      if (lookahead == 'e') ADVANCE(50);
      END_STATE();
    case 11:
      if (lookahead == 'e') ADVANCE(51);
      if (lookahead == 'i') ADVANCE(52);
      if (lookahead == 'o') ADVANCE(53);
      END_STATE();
    case 12:
      if (lookahead == 'a') ADVANCE(54);
      if (lookahead == 'e') ADVANCE(55);
      if (lookahead == 'i') ADVANCE(56);
      if (lookahead == 'o') ADVANCE(57);
      END_STATE();
    case 13:
      if (lookahead == 'f') ADVANCE(58);
      if (lookahead == 'o') ADVANCE(59);
      END_STATE();
    case 14:
      if (lookahead == 'k') ADVANCE(60);
      if (lookahead == 'p') ADVANCE(61);
      if (lookahead == 'r') ADVANCE(62);
      END_STATE();
    case 15:
      if (lookahead == 'o') ADVANCE(63);
      if (lookahead == 'r') ADVANCE(64);
      END_STATE();
    case 16:
      if (lookahead == 'e') ADVANCE(65);
      END_STATE();
    case 17:
      if (lookahead == 'e') ADVANCE(66);
      if (lookahead == 'h') ADVANCE(67);
      if (lookahead == 'l') ADVANCE(68);
      if (lookahead == 'o') ADVANCE(69);
      if (lookahead == 'q') ADVANCE(70);
      if (lookahead == 't') ADVANCE(71);
      END_STATE();
    case 18:
      if (lookahead == 'e') ADVANCE(72);
      if (lookahead == 'i') ADVANCE(73);
      if (lookahead == 'o') ADVANCE(74);
      if (lookahead == 'r') ADVANCE(75);
      if (lookahead == 'u') ADVANCE(76);
      if (lookahead == 'x') ADVANCE(77);
      END_STATE();
    case 19:
      if (lookahead == 'i') ADVANCE(78);
      if (lookahead == 'n') ADVANCE(79);
      if (lookahead == 's') ADVANCE(80);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(81);
      END_STATE();
    case 20:
      if (lookahead == 'a') ADVANCE(82);
      if (lookahead == 'r') ADVANCE(83);
      END_STATE();
    case 21:
      if (lookahead == 'o') ADVANCE(84);
      END_STATE();
    case 22:
      if (lookahead == 'd') ADVANCE(85);
      END_STATE();
    case 23:
      if (lookahead == 'p') ADVANCE(86);
      END_STATE();
    case 24:
      if (lookahead == '-') ADVANCE(87);
      if (lookahead == 's') ADVANCE(88);
      END_STATE();
    case 25:
      if (lookahead == '-') ADVANCE(89);
      END_STATE();
    case 26:
      if (lookahead == 'g') ADVANCE(90);
      END_STATE();
    case 27:
      if (lookahead == 't') ADVANCE(91);
      END_STATE();
    case 28:
      if (lookahead == 'o') ADVANCE(92);
      END_STATE();
    case 29:
      if (lookahead == 'o') ADVANCE(93);
      END_STATE();
    case 30:
      if (lookahead == 'f') ADVANCE(94);
      if (lookahead == 'r') ADVANCE(95);
      END_STATE();
    case 31:
      if (lookahead == 'a') ADVANCE(96);
      END_STATE();
    case 32:
      if (lookahead == 'n') ADVANCE(97);
      END_STATE();
    case 33:
      if (lookahead == 'r') ADVANCE(98);
      END_STATE();
    case 34:
      if (lookahead == 'f') ADVANCE(99);
      END_STATE();
    case 35:
      if (lookahead == 'e') ADVANCE(100);
      END_STATE();
    case 36:
      if (lookahead == 'r') ADVANCE(101);
      END_STATE();
    case 37:
      if (lookahead == 'l') ADVANCE(102);
      END_STATE();
    case 38:
      if (lookahead == 'l') ADVANCE(103);
      END_STATE();
    case 39:
      if (lookahead == 'l') ADVANCE(104);
      END_STATE();
    case 40:
      if (lookahead == 'o') ADVANCE(105);
      END_STATE();
    case 41:
      if (lookahead == '-') ADVANCE(106);
      END_STATE();
    case 42:
      if (lookahead == 't') ADVANCE(107);
      END_STATE();
    case 43:
      if (lookahead == 's') ADVANCE(108);
      END_STATE();
    case 44:
      if (lookahead == 'a') ADVANCE(109);
      END_STATE();
    case 45:
      if (lookahead == '-') ADVANCE(110);
      END_STATE();
    case 46:
      ACCEPT_TOKEN(anon_sym_if);
      END_STATE();
    case 47:
      if (lookahead == 'p') ADVANCE(111);
      END_STATE();
    case 48:
      if (lookahead == 'd') ADVANCE(112);
      if (lookahead == 't') ADVANCE(113);
      END_STATE();
    case 49:
      if (lookahead == '-') ADVANCE(114);
      END_STATE();
    case 50:
      if (lookahead == 'c') ADVANCE(115);
      END_STATE();
    case 51:
      if (lookahead == 'n') ADVANCE(116);
      if (lookahead == 't') ADVANCE(117);
      END_STATE();
    case 52:
      if (lookahead == 's') ADVANCE(118);
      END_STATE();
    case 53:
      if (lookahead == 'g') ADVANCE(119);
      END_STATE();
    case 54:
      if (lookahead == 'p') ADVANCE(120);
      if (lookahead == 't') ADVANCE(121);
      END_STATE();
    case 55:
      if (lookahead == 'r') ADVANCE(122);
      END_STATE();
    case 56:
      if (lookahead == 'n') ADVANCE(123);
      END_STATE();
    case 57:
      if (lookahead == 'd') ADVANCE(124);
      END_STATE();
    case 58:
      if (lookahead == 't') ADVANCE(125);
      END_STATE();
    case 59:
      if (lookahead == 'n') ADVANCE(126);
      if (lookahead == 't') ADVANCE(127);
      END_STATE();
    case 60:
      ACCEPT_TOKEN(anon_sym_ok);
      END_STATE();
    case 61:
      if (lookahead == 't') ADVANCE(128);
      END_STATE();
    case 62:
      ACCEPT_TOKEN(anon_sym_or);
      END_STATE();
    case 63:
      if (lookahead == 'w') ADVANCE(129);
      if (lookahead == 'x') ADVANCE(130);
      END_STATE();
    case 64:
      if (lookahead == 'i') ADVANCE(131);
      END_STATE();
    case 65:
      if (lookahead == 'p') ADVANCE(132);
      if (lookahead == 's') ADVANCE(133);
      END_STATE();
    case 66:
      if (lookahead == 'c') ADVANCE(134);
      END_STATE();
    case 67:
      if (lookahead == 'a') ADVANCE(135);
      END_STATE();
    case 68:
      if (lookahead == 'i') ADVANCE(136);
      END_STATE();
    case 69:
      if (lookahead == 'm') ADVANCE(137);
      END_STATE();
    case 70:
      if (lookahead == 'r') ADVANCE(138);
      END_STATE();
    case 71:
      if (lookahead == 'a') ADVANCE(139);
      if (lookahead == 'r') ADVANCE(140);
      if (lookahead == 'x') ADVANCE(141);
      END_STATE();
    case 72:
      if (lookahead == 'n') ADVANCE(142);
      END_STATE();
    case 73:
      if (lookahead == 'm') ADVANCE(143);
      END_STATE();
    case 74:
      if (lookahead == '-') ADVANCE(144);
      END_STATE();
    case 75:
      if (lookahead == 'u') ADVANCE(145);
      if (lookahead == 'y') ADVANCE(146);
      END_STATE();
    case 76:
      if (lookahead == 'p') ADVANCE(147);
      END_STATE();
    case 77:
      if (lookahead == '-') ADVANCE(148);
      END_STATE();
    case 78:
      if (lookahead == 'n') ADVANCE(149);
      END_STATE();
    case 79:
      if (lookahead == 'w') ADVANCE(150);
      END_STATE();
    case 80:
      if (lookahead == 'e') ADVANCE(151);
      END_STATE();
    case 81:
      ACCEPT_TOKEN(sym_uint_lit);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(81);
      END_STATE();
    case 82:
      if (lookahead == 'r') ADVANCE(152);
      END_STATE();
    case 83:
      if (lookahead == 'f') ADVANCE(153);
      END_STATE();
    case 84:
      if (lookahead == 'r') ADVANCE(154);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(anon_sym_and);
      END_STATE();
    case 86:
      if (lookahead == 'e') ADVANCE(155);
      END_STATE();
    case 87:
      if (lookahead == 'c') ADVANCE(156);
      if (lookahead == 'm') ADVANCE(157);
      END_STATE();
    case 88:
      if (lookahead == 'e') ADVANCE(158);
      END_STATE();
    case 89:
      if (lookahead == 'b') ADVANCE(159);
      END_STATE();
    case 90:
      if (lookahead == 'i') ADVANCE(160);
      END_STATE();
    case 91:
      if (lookahead == '-') ADVANCE(161);
      END_STATE();
    case 92:
      if (lookahead == 'c') ADVANCE(162);
      END_STATE();
    case 93:
      if (lookahead == 'l') ADVANCE(163);
      END_STATE();
    case 94:
      if (lookahead == 'f') ADVANCE(164);
      END_STATE();
    case 95:
      if (lookahead == 'n') ADVANCE(165);
      END_STATE();
    case 96:
      if (lookahead == 'i') ADVANCE(166);
      END_STATE();
    case 97:
      if (lookahead == 'c') ADVANCE(167);
      if (lookahead == 't') ADVANCE(168);
      END_STATE();
    case 98:
      if (lookahead == 'r') ADVANCE(169);
      END_STATE();
    case 99:
      if (lookahead == 'a') ADVANCE(170);
      if (lookahead == 'i') ADVANCE(171);
      END_STATE();
    case 100:
      if (lookahead == 'm') ADVANCE(172);
      END_STATE();
    case 101:
      ACCEPT_TOKEN(anon_sym_err);
      END_STATE();
    case 102:
      if (lookahead == 's') ADVANCE(173);
      END_STATE();
    case 103:
      if (lookahead == 't') ADVANCE(174);
      END_STATE();
    case 104:
      if (lookahead == 'd') ADVANCE(175);
      END_STATE();
    case 105:
      if (lookahead == 'm') ADVANCE(176);
      END_STATE();
    case 106:
      if (lookahead == 'b') ADVANCE(177);
      if (lookahead == 'g') ADVANCE(178);
      if (lookahead == 'm') ADVANCE(179);
      if (lookahead == 't') ADVANCE(180);
      END_STATE();
    case 107:
      ACCEPT_TOKEN(anon_sym_get);
      if (lookahead == '-') ADVANCE(181);
      END_STATE();
    case 108:
      if (lookahead == 'h') ADVANCE(182);
      END_STATE();
    case 109:
      if (lookahead == 'd') ADVANCE(183);
      END_STATE();
    case 110:
      if (lookahead == 'h') ADVANCE(184);
      END_STATE();
    case 111:
      if (lookahead == 'l') ADVANCE(185);
      END_STATE();
    case 112:
      if (lookahead == 'e') ADVANCE(186);
      END_STATE();
    case 113:
      ACCEPT_TOKEN(anon_sym_int);
      if (lookahead == '-') ADVANCE(187);
      END_STATE();
    case 114:
      if (lookahead == 'e') ADVANCE(188);
      if (lookahead == 'i') ADVANCE(189);
      if (lookahead == 'n') ADVANCE(190);
      if (lookahead == 'o') ADVANCE(191);
      if (lookahead == 's') ADVANCE(192);
      END_STATE();
    case 115:
      if (lookahead == 'c') ADVANCE(193);
      END_STATE();
    case 116:
      ACCEPT_TOKEN(anon_sym_len);
      END_STATE();
    case 117:
      ACCEPT_TOKEN(anon_sym_let);
      END_STATE();
    case 118:
      if (lookahead == 't') ADVANCE(194);
      END_STATE();
    case 119:
      if (lookahead == '2') ADVANCE(195);
      END_STATE();
    case 120:
      ACCEPT_TOKEN(anon_sym_map);
      if (lookahead == '-') ADVANCE(196);
      END_STATE();
    case 121:
      if (lookahead == 'c') ADVANCE(197);
      END_STATE();
    case 122:
      if (lookahead == 'g') ADVANCE(198);
      END_STATE();
    case 123:
      if (lookahead == 'e') ADVANCE(199);
      END_STATE();
    case 124:
      ACCEPT_TOKEN(anon_sym_mod);
      END_STATE();
    case 125:
      if (lookahead == '-') ADVANCE(200);
      END_STATE();
    case 126:
      if (lookahead == 'e') ADVANCE(201);
      END_STATE();
    case 127:
      ACCEPT_TOKEN(anon_sym_not);
      END_STATE();
    case 128:
      if (lookahead == 'i') ADVANCE(202);
      END_STATE();
    case 129:
      ACCEPT_TOKEN(anon_sym_pow);
      END_STATE();
    case 130:
      if (lookahead == '-') ADVANCE(203);
      END_STATE();
    case 131:
      if (lookahead == 'n') ADVANCE(204);
      END_STATE();
    case 132:
      if (lookahead == 'l') ADVANCE(205);
      END_STATE();
    case 133:
      if (lookahead == 'p') ADVANCE(206);
      if (lookahead == 't') ADVANCE(207);
      END_STATE();
    case 134:
      if (lookahead == 'p') ADVANCE(208);
      END_STATE();
    case 135:
      if (lookahead == '2') ADVANCE(209);
      if (lookahead == '5') ADVANCE(210);
      END_STATE();
    case 136:
      if (lookahead == 'c') ADVANCE(211);
      END_STATE();
    case 137:
      if (lookahead == 'e') ADVANCE(212);
      END_STATE();
    case 138:
      if (lookahead == 't') ADVANCE(213);
      END_STATE();
    case 139:
      if (lookahead == 'c') ADVANCE(214);
      END_STATE();
    case 140:
      if (lookahead == 'i') ADVANCE(215);
      END_STATE();
    case 141:
      if (lookahead == '-') ADVANCE(216);
      END_STATE();
    case 142:
      if (lookahead == 'u') ADVANCE(217);
      END_STATE();
    case 143:
      if (lookahead == 'e') ADVANCE(218);
      END_STATE();
    case 144:
      if (lookahead == 'a') ADVANCE(219);
      if (lookahead == 'c') ADVANCE(220);
      if (lookahead == 'i') ADVANCE(221);
      if (lookahead == 'u') ADVANCE(222);
      END_STATE();
    case 145:
      if (lookahead == 'e') ADVANCE(223);
      END_STATE();
    case 146:
      if (lookahead == '!') ADVANCE(224);
      END_STATE();
    case 147:
      if (lookahead == 'l') ADVANCE(225);
      END_STATE();
    case 148:
      if (lookahead == 's') ADVANCE(226);
      END_STATE();
    case 149:
      if (lookahead == 't') ADVANCE(227);
      END_STATE();
    case 150:
      if (lookahead == 'r') ADVANCE(228);
      END_STATE();
    case 151:
      if (lookahead == '-') ADVANCE(229);
      END_STATE();
    case 152:
      if (lookahead == '-') ADVANCE(230);
      END_STATE();
    case 153:
      if (lookahead == '-') ADVANCE(231);
      END_STATE();
    case 154:
      ACCEPT_TOKEN(anon_sym_xor);
      END_STATE();
    case 155:
      if (lookahead == 'n') ADVANCE(232);
      END_STATE();
    case 156:
      if (lookahead == 'o') ADVANCE(233);
      END_STATE();
    case 157:
      if (lookahead == 'a') ADVANCE(234);
      END_STATE();
    case 158:
      if (lookahead == 'r') ADVANCE(235);
      END_STATE();
    case 159:
      if (lookahead == 'l') ADVANCE(236);
      END_STATE();
    case 160:
      if (lookahead == 'n') ADVANCE(237);
      END_STATE();
    case 161:
      if (lookahead == 'a') ADVANCE(238);
      if (lookahead == 'n') ADVANCE(239);
      if (lookahead == 'o') ADVANCE(240);
      if (lookahead == 's') ADVANCE(241);
      if (lookahead == 'x') ADVANCE(242);
      END_STATE();
    case 162:
      if (lookahead == 'k') ADVANCE(243);
      END_STATE();
    case 163:
      ACCEPT_TOKEN(anon_sym_bool);
      END_STATE();
    case 164:
      ACCEPT_TOKEN(anon_sym_buff);
      if (lookahead == '-') ADVANCE(244);
      END_STATE();
    case 165:
      if (lookahead == '-') ADVANCE(245);
      if (lookahead == 'c') ADVANCE(246);
      END_STATE();
    case 166:
      if (lookahead == 'n') ADVANCE(247);
      END_STATE();
    case 167:
      if (lookahead == 'a') ADVANCE(248);
      END_STATE();
    case 168:
      if (lookahead == 'r') ADVANCE(249);
      END_STATE();
    case 169:
      if (lookahead == 'e') ADVANCE(250);
      END_STATE();
    case 170:
      if (lookahead == 'u') ADVANCE(251);
      END_STATE();
    case 171:
      if (lookahead == 'n') ADVANCE(252);
      END_STATE();
    case 172:
      if (lookahead == 'e') ADVANCE(253);
      END_STATE();
    case 173:
      if (lookahead == 'e') ADVANCE(254);
      END_STATE();
    case 174:
      if (lookahead == 'e') ADVANCE(255);
      END_STATE();
    case 175:
      ACCEPT_TOKEN(anon_sym_fold);
      END_STATE();
    case 176:
      if (lookahead == '-') ADVANCE(256);
      END_STATE();
    case 177:
      if (lookahead == 'u') ADVANCE(257);
      END_STATE();
    case 178:
      if (lookahead == 'e') ADVANCE(258);
      END_STATE();
    case 179:
      if (lookahead == 'i') ADVANCE(259);
      END_STATE();
    case 180:
      if (lookahead == 'r') ADVANCE(260);
      END_STATE();
    case 181:
      if (lookahead == 'b') ADVANCE(261);
      if (lookahead == 's') ADVANCE(262);
      if (lookahead == 't') ADVANCE(263);
      END_STATE();
    case 182:
      if (lookahead == '1') ADVANCE(264);
      END_STATE();
    case 183:
      if (lookahead == 'e') ADVANCE(265);
      END_STATE();
    case 184:
      if (lookahead == 'e') ADVANCE(266);
      END_STATE();
    case 185:
      if (lookahead == '-') ADVANCE(267);
      END_STATE();
    case 186:
      if (lookahead == 'x') ADVANCE(268);
      END_STATE();
    case 187:
      if (lookahead == 't') ADVANCE(269);
      END_STATE();
    case 188:
      if (lookahead == 'q') ADVANCE(270);
      if (lookahead == 'r') ADVANCE(271);
      END_STATE();
    case 189:
      if (lookahead == 'n') ADVANCE(272);
      END_STATE();
    case 190:
      if (lookahead == 'o') ADVANCE(273);
      END_STATE();
    case 191:
      if (lookahead == 'k') ADVANCE(274);
      END_STATE();
    case 192:
      if (lookahead == 'o') ADVANCE(275);
      if (lookahead == 't') ADVANCE(276);
      END_STATE();
    case 193:
      if (lookahead == 'a') ADVANCE(277);
      END_STATE();
    case 194:
      ACCEPT_TOKEN(anon_sym_list);
      END_STATE();
    case 195:
      ACCEPT_TOKEN(anon_sym_log2);
      END_STATE();
    case 196:
      if (lookahead == 'd') ADVANCE(278);
      if (lookahead == 'g') ADVANCE(279);
      if (lookahead == 'i') ADVANCE(280);
      if (lookahead == 's') ADVANCE(281);
      END_STATE();
    case 197:
      if (lookahead == 'h') ADVANCE(282);
      END_STATE();
    case 198:
      if (lookahead == 'e') ADVANCE(283);
      END_STATE();
    case 199:
      if (lookahead == 'r') ADVANCE(284);
      END_STATE();
    case 200:
      if (lookahead == 'b') ADVANCE(285);
      if (lookahead == 'g') ADVANCE(286);
      if (lookahead == 'm') ADVANCE(287);
      if (lookahead == 't') ADVANCE(288);
      END_STATE();
    case 201:
      ACCEPT_TOKEN(sym_none_lit);
      END_STATE();
    case 202:
      if (lookahead == 'o') ADVANCE(289);
      END_STATE();
    case 203:
      if (lookahead == 'a') ADVANCE(290);
      END_STATE();
    case 204:
      if (lookahead == 'c') ADVANCE(291);
      if (lookahead == 't') ADVANCE(292);
      END_STATE();
    case 205:
      if (lookahead == 'a') ADVANCE(293);
      END_STATE();
    case 206:
      if (lookahead == 'o') ADVANCE(294);
      END_STATE();
    case 207:
      if (lookahead == 'r') ADVANCE(295);
      END_STATE();
    case 208:
      if (lookahead == '2') ADVANCE(296);
      END_STATE();
    case 209:
      if (lookahead == '5') ADVANCE(297);
      END_STATE();
    case 210:
      if (lookahead == '1') ADVANCE(298);
      END_STATE();
    case 211:
      if (lookahead == 'e') ADVANCE(299);
      END_STATE();
    case 212:
      ACCEPT_TOKEN(anon_sym_some);
      END_STATE();
    case 213:
      if (lookahead == 'i') ADVANCE(300);
      END_STATE();
    case 214:
      if (lookahead == 'k') ADVANCE(301);
      END_STATE();
    case 215:
      if (lookahead == 'n') ADVANCE(302);
      END_STATE();
    case 216:
      if (lookahead == 'a') ADVANCE(303);
      if (lookahead == 'b') ADVANCE(304);
      if (lookahead == 'g') ADVANCE(305);
      if (lookahead == 'l') ADVANCE(306);
      if (lookahead == 't') ADVANCE(307);
      END_STATE();
    case 217:
      if (lookahead == 'r') ADVANCE(308);
      END_STATE();
    case 218:
      ACCEPT_TOKEN(anon_sym_time);
      END_STATE();
    case 219:
      if (lookahead == 's') ADVANCE(309);
      END_STATE();
    case 220:
      if (lookahead == 'o') ADVANCE(310);
      END_STATE();
    case 221:
      if (lookahead == 'n') ADVANCE(311);
      END_STATE();
    case 222:
      if (lookahead == 'i') ADVANCE(312);
      END_STATE();
    case 223:
      ACCEPT_TOKEN(anon_sym_true);
      END_STATE();
    case 224:
      ACCEPT_TOKEN(anon_sym_try_BANG);
      END_STATE();
    case 225:
      if (lookahead == 'e') ADVANCE(313);
      END_STATE();
    case 226:
      if (lookahead == 'e') ADVANCE(314);
      if (lookahead == 'p') ADVANCE(315);
      END_STATE();
    case 227:
      ACCEPT_TOKEN(anon_sym_uint);
      END_STATE();
    case 228:
      if (lookahead == 'a') ADVANCE(316);
      END_STATE();
    case 229:
      if (lookahead == 't') ADVANCE(317);
      END_STATE();
    case 230:
      if (lookahead == 'g') ADVANCE(318);
      if (lookahead == 's') ADVANCE(319);
      END_STATE();
    case 231:
      if (lookahead == 's') ADVANCE(320);
      END_STATE();
    case 232:
      if (lookahead == 'd') ADVANCE(321);
      END_STATE();
    case 233:
      if (lookahead == 'n') ADVANCE(322);
      END_STATE();
    case 234:
      if (lookahead == 'x') ADVANCE(323);
      END_STATE();
    case 235:
      if (lookahead == 't') ADVANCE(324);
      END_STATE();
    case 236:
      if (lookahead == 'o') ADVANCE(325);
      END_STATE();
    case 237:
      ACCEPT_TOKEN(anon_sym_begin);
      END_STATE();
    case 238:
      if (lookahead == 'n') ADVANCE(326);
      END_STATE();
    case 239:
      if (lookahead == 'o') ADVANCE(327);
      END_STATE();
    case 240:
      if (lookahead == 'r') ADVANCE(328);
      END_STATE();
    case 241:
      if (lookahead == 'h') ADVANCE(329);
      END_STATE();
    case 242:
      if (lookahead == 'o') ADVANCE(330);
      END_STATE();
    case 243:
      if (lookahead == '-') ADVANCE(331);
      END_STATE();
    case 244:
      if (lookahead == 't') ADVANCE(332);
      END_STATE();
    case 245:
      if (lookahead == 'b') ADVANCE(333);
      END_STATE();
    case 246:
      if (lookahead == 'h') ADVANCE(334);
      END_STATE();
    case 247:
      if (lookahead == '-') ADVANCE(335);
      END_STATE();
    case 248:
      if (lookahead == 't') ADVANCE(336);
      END_STATE();
    case 249:
      if (lookahead == 'a') ADVANCE(337);
      END_STATE();
    case 250:
      if (lookahead == 'n') ADVANCE(338);
      END_STATE();
    case 251:
      if (lookahead == 'l') ADVANCE(339);
      END_STATE();
    case 252:
      if (lookahead == 'e') ADVANCE(340);
      END_STATE();
    case 253:
      if (lookahead == 'n') ADVANCE(341);
      END_STATE();
    case 254:
      ACCEPT_TOKEN(anon_sym_false);
      END_STATE();
    case 255:
      if (lookahead == 'r') ADVANCE(342);
      END_STATE();
    case 256:
      if (lookahead == 'c') ADVANCE(343);
      END_STATE();
    case 257:
      if (lookahead == 'r') ADVANCE(344);
      END_STATE();
    case 258:
      if (lookahead == 't') ADVANCE(345);
      END_STATE();
    case 259:
      if (lookahead == 'n') ADVANCE(346);
      END_STATE();
    case 260:
      if (lookahead == 'a') ADVANCE(347);
      END_STATE();
    case 261:
      if (lookahead == 'l') ADVANCE(348);
      if (lookahead == 'u') ADVANCE(349);
      END_STATE();
    case 262:
      if (lookahead == 't') ADVANCE(350);
      END_STATE();
    case 263:
      if (lookahead == 'e') ADVANCE(351);
      END_STATE();
    case 264:
      if (lookahead == '6') ADVANCE(352);
      END_STATE();
    case 265:
      if (lookahead == 'r') ADVANCE(353);
      END_STATE();
    case 266:
      if (lookahead == 'a') ADVANCE(354);
      END_STATE();
    case 267:
      if (lookahead == 't') ADVANCE(355);
      END_STATE();
    case 268:
      if (lookahead == '-') ADVANCE(356);
      END_STATE();
    case 269:
      if (lookahead == 'o') ADVANCE(357);
      END_STATE();
    case 270:
      ACCEPT_TOKEN(anon_sym_is_DASHeq);
      END_STATE();
    case 271:
      if (lookahead == 'r') ADVANCE(358);
      END_STATE();
    case 272:
      if (lookahead == '-') ADVANCE(359);
      END_STATE();
    case 273:
      if (lookahead == 'n') ADVANCE(360);
      END_STATE();
    case 274:
      ACCEPT_TOKEN(anon_sym_is_DASHok);
      END_STATE();
    case 275:
      if (lookahead == 'm') ADVANCE(361);
      END_STATE();
    case 276:
      if (lookahead == 'a') ADVANCE(362);
      END_STATE();
    case 277:
      if (lookahead == 'k') ADVANCE(363);
      END_STATE();
    case 278:
      if (lookahead == 'e') ADVANCE(364);
      END_STATE();
    case 279:
      if (lookahead == 'e') ADVANCE(365);
      END_STATE();
    case 280:
      if (lookahead == 'n') ADVANCE(366);
      END_STATE();
    case 281:
      if (lookahead == 'e') ADVANCE(367);
      END_STATE();
    case 282:
      ACCEPT_TOKEN(anon_sym_match);
      END_STATE();
    case 283:
      ACCEPT_TOKEN(anon_sym_merge);
      END_STATE();
    case 284:
      if (lookahead == '-') ADVANCE(368);
      END_STATE();
    case 285:
      if (lookahead == 'u') ADVANCE(369);
      END_STATE();
    case 286:
      if (lookahead == 'e') ADVANCE(370);
      END_STATE();
    case 287:
      if (lookahead == 'i') ADVANCE(371);
      END_STATE();
    case 288:
      if (lookahead == 'r') ADVANCE(372);
      END_STATE();
    case 289:
      if (lookahead == 'n') ADVANCE(373);
      END_STATE();
    case 290:
      if (lookahead == 'd') ADVANCE(374);
      END_STATE();
    case 291:
      if (lookahead == 'i') ADVANCE(375);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(anon_sym_print);
      END_STATE();
    case 293:
      if (lookahead == 'c') ADVANCE(376);
      END_STATE();
    case 294:
      if (lookahead == 'n') ADVANCE(377);
      END_STATE();
    case 295:
      if (lookahead == 'i') ADVANCE(378);
      END_STATE();
    case 296:
      if (lookahead == '5') ADVANCE(379);
      END_STATE();
    case 297:
      if (lookahead == '6') ADVANCE(380);
      END_STATE();
    case 298:
      if (lookahead == '2') ADVANCE(381);
      END_STATE();
    case 299:
      if (lookahead == '?') ADVANCE(382);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(anon_sym_sqrti);
      END_STATE();
    case 301:
      if (lookahead == 's') ADVANCE(383);
      END_STATE();
    case 302:
      if (lookahead == 'g') ADVANCE(384);
      END_STATE();
    case 303:
      if (lookahead == 'c') ADVANCE(385);
      END_STATE();
    case 304:
      if (lookahead == 'u') ADVANCE(386);
      END_STATE();
    case 305:
      if (lookahead == 'e') ADVANCE(387);
      END_STATE();
    case 306:
      if (lookahead == 'i') ADVANCE(388);
      END_STATE();
    case 307:
      if (lookahead == 'r') ADVANCE(389);
      END_STATE();
    case 308:
      if (lookahead == 'e') ADVANCE(390);
      END_STATE();
    case 309:
      if (lookahead == 'c') ADVANCE(391);
      END_STATE();
    case 310:
      if (lookahead == 'n') ADVANCE(392);
      END_STATE();
    case 311:
      if (lookahead == 't') ADVANCE(393);
      END_STATE();
    case 312:
      if (lookahead == 'n') ADVANCE(394);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(anon_sym_tuple);
      END_STATE();
    case 314:
      if (lookahead == 'n') ADVANCE(395);
      END_STATE();
    case 315:
      if (lookahead == 'o') ADVANCE(396);
      END_STATE();
    case 316:
      if (lookahead == 'p') ADVANCE(397);
      END_STATE();
    case 317:
      if (lookahead == 'r') ADVANCE(398);
      END_STATE();
    case 318:
      if (lookahead == 'e') ADVANCE(399);
      END_STATE();
    case 319:
      if (lookahead == 'e') ADVANCE(400);
      END_STATE();
    case 320:
      if (lookahead == 'e') ADVANCE(401);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(anon_sym_append);
      END_STATE();
    case 322:
      if (lookahead == 't') ADVANCE(402);
      END_STATE();
    case 323:
      if (lookahead == '-') ADVANCE(403);
      END_STATE();
    case 324:
      if (lookahead == 's') ADVANCE(404);
      END_STATE();
    case 325:
      if (lookahead == 'c') ADVANCE(405);
      END_STATE();
    case 326:
      if (lookahead == 'd') ADVANCE(406);
      END_STATE();
    case 327:
      if (lookahead == 't') ADVANCE(407);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(anon_sym_bit_DASHor);
      END_STATE();
    case 329:
      if (lookahead == 'i') ADVANCE(408);
      END_STATE();
    case 330:
      if (lookahead == 'r') ADVANCE(409);
      END_STATE();
    case 331:
      if (lookahead == 'h') ADVANCE(410);
      if (lookahead == 'r') ADVANCE(411);
      END_STATE();
    case 332:
      if (lookahead == 'o') ADVANCE(412);
      END_STATE();
    case 333:
      if (lookahead == 'l') ADVANCE(413);
      END_STATE();
    case 334:
      if (lookahead == 'a') ADVANCE(414);
      END_STATE();
    case 335:
      if (lookahead == 'i') ADVANCE(415);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(anon_sym_concat);
      END_STATE();
    case 337:
      if (lookahead == 'c') ADVANCE(416);
      END_STATE();
    case 338:
      if (lookahead == 't') ADVANCE(417);
      END_STATE();
    case 339:
      if (lookahead == 't') ADVANCE(418);
      END_STATE();
    case 340:
      if (lookahead == '-') ADVANCE(419);
      END_STATE();
    case 341:
      if (lookahead == 't') ADVANCE(420);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(anon_sym_filter);
      END_STATE();
    case 343:
      if (lookahead == 'o') ADVANCE(421);
      END_STATE();
    case 344:
      if (lookahead == 'n') ADVANCE(422);
      END_STATE();
    case 345:
      if (lookahead == '-') ADVANCE(423);
      END_STATE();
    case 346:
      if (lookahead == 't') ADVANCE(424);
      END_STATE();
    case 347:
      if (lookahead == 'n') ADVANCE(425);
      END_STATE();
    case 348:
      if (lookahead == 'o') ADVANCE(426);
      END_STATE();
    case 349:
      if (lookahead == 'r') ADVANCE(427);
      END_STATE();
    case 350:
      if (lookahead == 'a') ADVANCE(428);
      END_STATE();
    case 351:
      if (lookahead == 'n') ADVANCE(429);
      END_STATE();
    case 352:
      if (lookahead == '0') ADVANCE(430);
      END_STATE();
    case 353:
      if (lookahead == '-') ADVANCE(431);
      END_STATE();
    case 354:
      if (lookahead == 'd') ADVANCE(432);
      END_STATE();
    case 355:
      if (lookahead == 'r') ADVANCE(433);
      END_STATE();
    case 356:
      if (lookahead == 'o') ADVANCE(434);
      END_STATE();
    case 357:
      if (lookahead == '-') ADVANCE(435);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(anon_sym_is_DASHerr);
      END_STATE();
    case 359:
      if (lookahead == 'm') ADVANCE(436);
      if (lookahead == 'r') ADVANCE(437);
      END_STATE();
    case 360:
      if (lookahead == 'e') ADVANCE(438);
      END_STATE();
    case 361:
      if (lookahead == 'e') ADVANCE(439);
      END_STATE();
    case 362:
      if (lookahead == 'n') ADVANCE(440);
      END_STATE();
    case 363:
      if (lookahead == '2') ADVANCE(441);
      END_STATE();
    case 364:
      if (lookahead == 'l') ADVANCE(442);
      END_STATE();
    case 365:
      if (lookahead == 't') ADVANCE(443);
      END_STATE();
    case 366:
      if (lookahead == 's') ADVANCE(444);
      END_STATE();
    case 367:
      if (lookahead == 't') ADVANCE(445);
      END_STATE();
    case 368:
      if (lookahead == 'a') ADVANCE(446);
      if (lookahead == 's') ADVANCE(447);
      END_STATE();
    case 369:
      if (lookahead == 'r') ADVANCE(448);
      END_STATE();
    case 370:
      if (lookahead == 't') ADVANCE(449);
      END_STATE();
    case 371:
      if (lookahead == 'n') ADVANCE(450);
      END_STATE();
    case 372:
      if (lookahead == 'a') ADVANCE(451);
      END_STATE();
    case 373:
      if (lookahead == 'a') ADVANCE(452);
      END_STATE();
    case 374:
      if (lookahead == 'd') ADVANCE(453);
      END_STATE();
    case 375:
      if (lookahead == 'p') ADVANCE(454);
      END_STATE();
    case 376:
      if (lookahead == 'e') ADVANCE(455);
      END_STATE();
    case 377:
      if (lookahead == 's') ADVANCE(456);
      END_STATE();
    case 378:
      if (lookahead == 'c') ADVANCE(457);
      END_STATE();
    case 379:
      if (lookahead == '6') ADVANCE(458);
      END_STATE();
    case 380:
      ACCEPT_TOKEN(anon_sym_sha256);
      END_STATE();
    case 381:
      ACCEPT_TOKEN(anon_sym_sha512);
      END_STATE();
    case 382:
      ACCEPT_TOKEN(anon_sym_slice_QMARK);
      END_STATE();
    case 383:
      if (lookahead == '-') ADVANCE(459);
      END_STATE();
    case 384:
      if (lookahead == '-') ADVANCE(460);
      END_STATE();
    case 385:
      if (lookahead == 'c') ADVANCE(461);
      END_STATE();
    case 386:
      if (lookahead == 'r') ADVANCE(462);
      END_STATE();
    case 387:
      if (lookahead == 't') ADVANCE(463);
      END_STATE();
    case 388:
      if (lookahead == 'q') ADVANCE(464);
      END_STATE();
    case 389:
      if (lookahead == 'a') ADVANCE(465);
      END_STATE();
    case 390:
      if (lookahead == '-') ADVANCE(466);
      END_STATE();
    case 391:
      if (lookahead == 'i') ADVANCE(467);
      END_STATE();
    case 392:
      if (lookahead == 's') ADVANCE(468);
      END_STATE();
    case 393:
      ACCEPT_TOKEN(anon_sym_to_DASHint);
      END_STATE();
    case 394:
      if (lookahead == 't') ADVANCE(469);
      END_STATE();
    case 395:
      if (lookahead == 'd') ADVANCE(470);
      END_STATE();
    case 396:
      if (lookahead == 'n') ADVANCE(471);
      END_STATE();
    case 397:
      if (lookahead == '!') ADVANCE(472);
      if (lookahead == '-') ADVANCE(473);
      END_STATE();
    case 398:
      if (lookahead == 'a') ADVANCE(474);
      END_STATE();
    case 399:
      if (lookahead == 't') ADVANCE(475);
      END_STATE();
    case 400:
      if (lookahead == 't') ADVANCE(476);
      END_STATE();
    case 401:
      if (lookahead == 'e') ADVANCE(477);
      END_STATE();
    case 402:
      if (lookahead == 'r') ADVANCE(478);
      END_STATE();
    case 403:
      if (lookahead == 'l') ADVANCE(479);
      END_STATE();
    case 404:
      if (lookahead == '!') ADVANCE(480);
      END_STATE();
    case 405:
      if (lookahead == 'k') ADVANCE(481);
      END_STATE();
    case 406:
      ACCEPT_TOKEN(anon_sym_bit_DASHand);
      END_STATE();
    case 407:
      ACCEPT_TOKEN(anon_sym_bit_DASHnot);
      END_STATE();
    case 408:
      if (lookahead == 'f') ADVANCE(482);
      END_STATE();
    case 409:
      ACCEPT_TOKEN(anon_sym_bit_DASHxor);
      END_STATE();
    case 410:
      if (lookahead == 'e') ADVANCE(483);
      END_STATE();
    case 411:
      if (lookahead == 'e') ADVANCE(484);
      END_STATE();
    case 412:
      if (lookahead == '-') ADVANCE(485);
      END_STATE();
    case 413:
      if (lookahead == 'o') ADVANCE(486);
      END_STATE();
    case 414:
      if (lookahead == 'i') ADVANCE(487);
      END_STATE();
    case 415:
      if (lookahead == 'd') ADVANCE(488);
      END_STATE();
    case 416:
      if (lookahead == 't') ADVANCE(489);
      END_STATE();
    case 417:
      if (lookahead == '-') ADVANCE(490);
      END_STATE();
    case 418:
      if (lookahead == '-') ADVANCE(491);
      END_STATE();
    case 419:
      if (lookahead == 'c') ADVANCE(492);
      if (lookahead == 'd') ADVANCE(493);
      if (lookahead == 'f') ADVANCE(494);
      if (lookahead == 'm') ADVANCE(495);
      if (lookahead == 'n') ADVANCE(496);
      if (lookahead == 'p') ADVANCE(497);
      if (lookahead == 'r') ADVANCE(498);
      if (lookahead == 't') ADVANCE(499);
      END_STATE();
    case 420:
      if (lookahead == '-') ADVANCE(500);
      END_STATE();
    case 421:
      if (lookahead == 'n') ADVANCE(501);
      END_STATE();
    case 422:
      if (lookahead == '?') ADVANCE(502);
      END_STATE();
    case 423:
      if (lookahead == 'b') ADVANCE(503);
      if (lookahead == 's') ADVANCE(504);
      END_STATE();
    case 424:
      if (lookahead == '?') ADVANCE(505);
      END_STATE();
    case 425:
      if (lookahead == 's') ADVANCE(506);
      END_STATE();
    case 426:
      if (lookahead == 'c') ADVANCE(507);
      END_STATE();
    case 427:
      if (lookahead == 'n') ADVANCE(508);
      END_STATE();
    case 428:
      if (lookahead == 'c') ADVANCE(509);
      END_STATE();
    case 429:
      if (lookahead == 'u') ADVANCE(510);
      END_STATE();
    case 430:
      ACCEPT_TOKEN(anon_sym_hash160);
      END_STATE();
    case 431:
      if (lookahead == 'h') ADVANCE(511);
      END_STATE();
    case 432:
      if (lookahead == 'e') ADVANCE(512);
      END_STATE();
    case 433:
      if (lookahead == 'a') ADVANCE(513);
      END_STATE();
    case 434:
      if (lookahead == 'f') ADVANCE(514);
      END_STATE();
    case 435:
      if (lookahead == 'a') ADVANCE(515);
      if (lookahead == 'u') ADVANCE(516);
      END_STATE();
    case 436:
      if (lookahead == 'a') ADVANCE(517);
      END_STATE();
    case 437:
      if (lookahead == 'e') ADVANCE(518);
      END_STATE();
    case 438:
      ACCEPT_TOKEN(anon_sym_is_DASHnone);
      END_STATE();
    case 439:
      ACCEPT_TOKEN(anon_sym_is_DASHsome);
      END_STATE();
    case 440:
      if (lookahead == 'd') ADVANCE(519);
      END_STATE();
    case 441:
      if (lookahead == '5') ADVANCE(520);
      END_STATE();
    case 442:
      if (lookahead == 'e') ADVANCE(521);
      END_STATE();
    case 443:
      if (lookahead == '?') ADVANCE(522);
      END_STATE();
    case 444:
      if (lookahead == 'e') ADVANCE(523);
      END_STATE();
    case 445:
      ACCEPT_TOKEN(anon_sym_map_DASHset);
      END_STATE();
    case 446:
      if (lookahead == 'd') ADVANCE(524);
      END_STATE();
    case 447:
      if (lookahead == 'p') ADVANCE(525);
      END_STATE();
    case 448:
      if (lookahead == 'n') ADVANCE(526);
      END_STATE();
    case 449:
      if (lookahead == '-') ADVANCE(527);
      END_STATE();
    case 450:
      if (lookahead == 't') ADVANCE(528);
      END_STATE();
    case 451:
      if (lookahead == 'n') ADVANCE(529);
      END_STATE();
    case 452:
      if (lookahead == 'l') ADVANCE(530);
      END_STATE();
    case 453:
      if (lookahead == 'r') ADVANCE(531);
      END_STATE();
    case 454:
      if (lookahead == 'a') ADVANCE(532);
      END_STATE();
    case 455:
      if (lookahead == '-') ADVANCE(533);
      END_STATE();
    case 456:
      if (lookahead == 'e') ADVANCE(534);
      END_STATE();
    case 457:
      if (lookahead == 't') ADVANCE(535);
      END_STATE();
    case 458:
      if (lookahead == 'k') ADVANCE(536);
      if (lookahead == 'r') ADVANCE(537);
      END_STATE();
    case 459:
      if (lookahead == 'b') ADVANCE(538);
      END_STATE();
    case 460:
      if (lookahead == 'a') ADVANCE(539);
      if (lookahead == 't') ADVANCE(540);
      if (lookahead == 'u') ADVANCE(541);
      END_STATE();
    case 461:
      if (lookahead == 'o') ADVANCE(542);
      END_STATE();
    case 462:
      if (lookahead == 'n') ADVANCE(543);
      END_STATE();
    case 463:
      if (lookahead == '-') ADVANCE(544);
      END_STATE();
    case 464:
      if (lookahead == 'u') ADVANCE(545);
      END_STATE();
    case 465:
      if (lookahead == 'n') ADVANCE(546);
      END_STATE();
    case 466:
      if (lookahead == 'h') ADVANCE(547);
      END_STATE();
    case 467:
      if (lookahead == 'i') ADVANCE(548);
      END_STATE();
    case 468:
      if (lookahead == 'e') ADVANCE(549);
      END_STATE();
    case 469:
      ACCEPT_TOKEN(anon_sym_to_DASHuint);
      END_STATE();
    case 470:
      if (lookahead == 'e') ADVANCE(550);
      END_STATE();
    case 471:
      if (lookahead == 's') ADVANCE(551);
      END_STATE();
    case 472:
      ACCEPT_TOKEN(anon_sym_unwrap_BANG);
      END_STATE();
    case 473:
      if (lookahead == 'e') ADVANCE(552);
      if (lookahead == 'p') ADVANCE(553);
      END_STATE();
    case 474:
      if (lookahead == 'i') ADVANCE(554);
      END_STATE();
    case 475:
      ACCEPT_TOKEN(anon_sym_var_DASHget);
      END_STATE();
    case 476:
      ACCEPT_TOKEN(anon_sym_var_DASHset);
      END_STATE();
    case 477:
      if (lookahead == 'd') ADVANCE(555);
      END_STATE();
    case 478:
      if (lookahead == 'a') ADVANCE(556);
      END_STATE();
    case 479:
      if (lookahead == 'e') ADVANCE(557);
      END_STATE();
    case 480:
      ACCEPT_TOKEN(anon_sym_asserts_BANG);
      END_STATE();
    case 481:
      ACCEPT_TOKEN(anon_sym_at_DASHblock);
      END_STATE();
    case 482:
      if (lookahead == 't') ADVANCE(558);
      END_STATE();
    case 483:
      if (lookahead == 'i') ADVANCE(559);
      END_STATE();
    case 484:
      if (lookahead == 'w') ADVANCE(560);
      END_STATE();
    case 485:
      if (lookahead == 'i') ADVANCE(561);
      if (lookahead == 'u') ADVANCE(562);
      END_STATE();
    case 486:
      if (lookahead == 'c') ADVANCE(563);
      END_STATE();
    case 487:
      if (lookahead == 'n') ADVANCE(564);
      END_STATE();
    case 488:
      ACCEPT_TOKEN(anon_sym_chain_DASHid);
      END_STATE();
    case 489:
      if (lookahead == '-') ADVANCE(565);
      END_STATE();
    case 490:
      if (lookahead == 'c') ADVANCE(566);
      END_STATE();
    case 491:
      if (lookahead == 't') ADVANCE(567);
      END_STATE();
    case 492:
      if (lookahead == 'o') ADVANCE(568);
      END_STATE();
    case 493:
      if (lookahead == 'a') ADVANCE(569);
      END_STATE();
    case 494:
      if (lookahead == 'u') ADVANCE(570);
      END_STATE();
    case 495:
      if (lookahead == 'a') ADVANCE(571);
      END_STATE();
    case 496:
      if (lookahead == 'o') ADVANCE(572);
      END_STATE();
    case 497:
      if (lookahead == 'r') ADVANCE(573);
      if (lookahead == 'u') ADVANCE(574);
      END_STATE();
    case 498:
      if (lookahead == 'e') ADVANCE(575);
      END_STATE();
    case 499:
      if (lookahead == 'r') ADVANCE(576);
      END_STATE();
    case 500:
      if (lookahead == 'a') ADVANCE(577);
      END_STATE();
    case 501:
      if (lookahead == 's') ADVANCE(578);
      END_STATE();
    case 502:
      ACCEPT_TOKEN(anon_sym_ft_DASHburn_QMARK);
      END_STATE();
    case 503:
      if (lookahead == 'a') ADVANCE(579);
      END_STATE();
    case 504:
      if (lookahead == 'u') ADVANCE(580);
      END_STATE();
    case 505:
      ACCEPT_TOKEN(anon_sym_ft_DASHmint_QMARK);
      END_STATE();
    case 506:
      if (lookahead == 'f') ADVANCE(581);
      END_STATE();
    case 507:
      if (lookahead == 'k') ADVANCE(582);
      END_STATE();
    case 508:
      if (lookahead == '-') ADVANCE(583);
      END_STATE();
    case 509:
      if (lookahead == 'k') ADVANCE(584);
      END_STATE();
    case 510:
      if (lookahead == 'r') ADVANCE(585);
      END_STATE();
    case 511:
      if (lookahead == 'a') ADVANCE(586);
      END_STATE();
    case 512:
      if (lookahead == 'r') ADVANCE(587);
      END_STATE();
    case 513:
      if (lookahead == 'i') ADVANCE(588);
      END_STATE();
    case 514:
      ACCEPT_TOKEN(anon_sym_index_DASHof);
      if (lookahead == '?') ADVANCE(589);
      END_STATE();
    case 515:
      if (lookahead == 's') ADVANCE(590);
      END_STATE();
    case 516:
      if (lookahead == 't') ADVANCE(591);
      END_STATE();
    case 517:
      if (lookahead == 'i') ADVANCE(592);
      END_STATE();
    case 518:
      if (lookahead == 'g') ADVANCE(593);
      END_STATE();
    case 519:
      if (lookahead == 'a') ADVANCE(594);
      END_STATE();
    case 520:
      if (lookahead == '6') ADVANCE(595);
      END_STATE();
    case 521:
      if (lookahead == 't') ADVANCE(596);
      END_STATE();
    case 522:
      ACCEPT_TOKEN(anon_sym_map_DASHget_QMARK);
      END_STATE();
    case 523:
      if (lookahead == 'r') ADVANCE(597);
      END_STATE();
    case 524:
      if (lookahead == 'd') ADVANCE(598);
      END_STATE();
    case 525:
      if (lookahead == 'e') ADVANCE(599);
      END_STATE();
    case 526:
      if (lookahead == '?') ADVANCE(600);
      END_STATE();
    case 527:
      if (lookahead == 'o') ADVANCE(601);
      END_STATE();
    case 528:
      if (lookahead == '?') ADVANCE(602);
      END_STATE();
    case 529:
      if (lookahead == 's') ADVANCE(603);
      END_STATE();
    case 530:
      ACCEPT_TOKEN(anon_sym_optional);
      END_STATE();
    case 531:
      if (lookahead == 's') ADVANCE(604);
      END_STATE();
    case 532:
      if (lookahead == 'l') ADVANCE(605);
      END_STATE();
    case 533:
      if (lookahead == 'a') ADVANCE(606);
      END_STATE();
    case 534:
      ACCEPT_TOKEN(anon_sym_response);
      END_STATE();
    case 535:
      if (lookahead == '-') ADVANCE(607);
      END_STATE();
    case 536:
      if (lookahead == '1') ADVANCE(608);
      END_STATE();
    case 537:
      if (lookahead == '1') ADVANCE(609);
      END_STATE();
    case 538:
      if (lookahead == 'l') ADVANCE(610);
      END_STATE();
    case 539:
      if (lookahead == 's') ADVANCE(611);
      END_STATE();
    case 540:
      if (lookahead == 'o') ADVANCE(612);
      END_STATE();
    case 541:
      if (lookahead == 't') ADVANCE(613);
      END_STATE();
    case 542:
      if (lookahead == 'u') ADVANCE(614);
      END_STATE();
    case 543:
      if (lookahead == '?') ADVANCE(615);
      END_STATE();
    case 544:
      if (lookahead == 'b') ADVANCE(616);
      END_STATE();
    case 545:
      if (lookahead == 'i') ADVANCE(617);
      END_STATE();
    case 546:
      if (lookahead == 's') ADVANCE(618);
      END_STATE();
    case 547:
      if (lookahead == 'e') ADVANCE(619);
      END_STATE();
    case 548:
      if (lookahead == '?') ADVANCE(620);
      END_STATE();
    case 549:
      if (lookahead == 'n') ADVANCE(621);
      END_STATE();
    case 550:
      if (lookahead == 'r') ADVANCE(622);
      END_STATE();
    case 551:
      if (lookahead == 'o') ADVANCE(623);
      END_STATE();
    case 552:
      if (lookahead == 'r') ADVANCE(624);
      END_STATE();
    case 553:
      if (lookahead == 'a') ADVANCE(625);
      END_STATE();
    case 554:
      if (lookahead == 't') ADVANCE(626);
      END_STATE();
    case 555:
      ACCEPT_TOKEN(anon_sym_vrf_DASHseed);
      END_STATE();
    case 556:
      if (lookahead == 'c') ADVANCE(627);
      END_STATE();
    case 557:
      if (lookahead == 'n') ADVANCE(628);
      END_STATE();
    case 558:
      if (lookahead == '-') ADVANCE(629);
      END_STATE();
    case 559:
      if (lookahead == 'g') ADVANCE(630);
      END_STATE();
    case 560:
      if (lookahead == 'a') ADVANCE(631);
      END_STATE();
    case 561:
      if (lookahead == 'n') ADVANCE(632);
      END_STATE();
    case 562:
      if (lookahead == 'i') ADVANCE(633);
      END_STATE();
    case 563:
      if (lookahead == 'k') ADVANCE(634);
      END_STATE();
    case 564:
      if (lookahead == '-') ADVANCE(635);
      END_STATE();
    case 565:
      if (lookahead == 'c') ADVANCE(636);
      if (lookahead == 'h') ADVANCE(637);
      if (lookahead == 'o') ADVANCE(638);
      END_STATE();
    case 566:
      if (lookahead == 'o') ADVANCE(639);
      END_STATE();
    case 567:
      if (lookahead == 'o') ADVANCE(640);
      END_STATE();
    case 568:
      if (lookahead == 'n') ADVANCE(641);
      END_STATE();
    case 569:
      if (lookahead == 't') ADVANCE(642);
      END_STATE();
    case 570:
      if (lookahead == 'n') ADVANCE(643);
      END_STATE();
    case 571:
      if (lookahead == 'p') ADVANCE(644);
      END_STATE();
    case 572:
      if (lookahead == 'n') ADVANCE(645);
      END_STATE();
    case 573:
      if (lookahead == 'i') ADVANCE(646);
      END_STATE();
    case 574:
      if (lookahead == 'b') ADVANCE(647);
      END_STATE();
    case 575:
      if (lookahead == 'a') ADVANCE(648);
      END_STATE();
    case 576:
      if (lookahead == 'a') ADVANCE(649);
      END_STATE();
    case 577:
      if (lookahead == 't') ADVANCE(650);
      END_STATE();
    case 578:
      if (lookahead == 'e') ADVANCE(651);
      END_STATE();
    case 579:
      if (lookahead == 'l') ADVANCE(652);
      END_STATE();
    case 580:
      if (lookahead == 'p') ADVANCE(653);
      END_STATE();
    case 581:
      if (lookahead == 'e') ADVANCE(654);
      END_STATE();
    case 582:
      if (lookahead == '-') ADVANCE(655);
      END_STATE();
    case 583:
      if (lookahead == 'b') ADVANCE(656);
      END_STATE();
    case 584:
      if (lookahead == 's') ADVANCE(657);
      END_STATE();
    case 585:
      if (lookahead == 'e') ADVANCE(658);
      END_STATE();
    case 586:
      if (lookahead == 's') ADVANCE(659);
      END_STATE();
    case 587:
      if (lookahead == '-') ADVANCE(660);
      END_STATE();
    case 588:
      if (lookahead == 't') ADVANCE(661);
      END_STATE();
    case 589:
      ACCEPT_TOKEN(anon_sym_index_DASHof_QMARK);
      END_STATE();
    case 590:
      if (lookahead == 'c') ADVANCE(662);
      END_STATE();
    case 591:
      if (lookahead == 'f') ADVANCE(663);
      END_STATE();
    case 592:
      if (lookahead == 'n') ADVANCE(664);
      END_STATE();
    case 593:
      if (lookahead == 't') ADVANCE(665);
      END_STATE();
    case 594:
      if (lookahead == 'r') ADVANCE(666);
      END_STATE();
    case 595:
      ACCEPT_TOKEN(anon_sym_keccak256);
      END_STATE();
    case 596:
      if (lookahead == 'e') ADVANCE(667);
      END_STATE();
    case 597:
      if (lookahead == 't') ADVANCE(668);
      END_STATE();
    case 598:
      if (lookahead == 'r') ADVANCE(669);
      END_STATE();
    case 599:
      if (lookahead == 'n') ADVANCE(670);
      END_STATE();
    case 600:
      ACCEPT_TOKEN(anon_sym_nft_DASHburn_QMARK);
      END_STATE();
    case 601:
      if (lookahead == 'w') ADVANCE(671);
      END_STATE();
    case 602:
      ACCEPT_TOKEN(anon_sym_nft_DASHmint_QMARK);
      END_STATE();
    case 603:
      if (lookahead == 'f') ADVANCE(672);
      END_STATE();
    case 604:
      ACCEPT_TOKEN(anon_sym_pox_DASHaddrs);
      END_STATE();
    case 605:
      ACCEPT_TOKEN(anon_sym_principal);
      if (lookahead == '-') ADVANCE(673);
      END_STATE();
    case 606:
      if (lookahead == 't') ADVANCE(674);
      END_STATE();
    case 607:
      if (lookahead == 'a') ADVANCE(675);
      END_STATE();
    case 608:
      if (lookahead == '-') ADVANCE(676);
      END_STATE();
    case 609:
      if (lookahead == '-') ADVANCE(677);
      END_STATE();
    case 610:
      if (lookahead == 'o') ADVANCE(678);
      END_STATE();
    case 611:
      if (lookahead == 'c') ADVANCE(679);
      END_STATE();
    case 612:
      if (lookahead == '-') ADVANCE(680);
      END_STATE();
    case 613:
      if (lookahead == 'f') ADVANCE(681);
      END_STATE();
    case 614:
      if (lookahead == 'n') ADVANCE(682);
      END_STATE();
    case 615:
      ACCEPT_TOKEN(anon_sym_stx_DASHburn_QMARK);
      END_STATE();
    case 616:
      if (lookahead == 'a') ADVANCE(683);
      END_STATE();
    case 617:
      if (lookahead == 'd') ADVANCE(684);
      END_STATE();
    case 618:
      if (lookahead == 'f') ADVANCE(685);
      END_STATE();
    case 619:
      if (lookahead == 'i') ADVANCE(686);
      END_STATE();
    case 620:
      ACCEPT_TOKEN(anon_sym_to_DASHascii_QMARK);
      END_STATE();
    case 621:
      if (lookahead == 's') ADVANCE(687);
      END_STATE();
    case 622:
      ACCEPT_TOKEN(anon_sym_tx_DASHsender);
      END_STATE();
    case 623:
      if (lookahead == 'r') ADVANCE(688);
      END_STATE();
    case 624:
      if (lookahead == 'r') ADVANCE(689);
      END_STATE();
    case 625:
      if (lookahead == 'n') ADVANCE(690);
      END_STATE();
    case 626:
      ACCEPT_TOKEN(anon_sym_use_DASHtrait);
      END_STATE();
    case 627:
      if (lookahead == 't') ADVANCE(691);
      END_STATE();
    case 628:
      if (lookahead == '?') ADVANCE(692);
      END_STATE();
    case 629:
      if (lookahead == 'l') ADVANCE(693);
      if (lookahead == 'r') ADVANCE(694);
      END_STATE();
    case 630:
      if (lookahead == 'h') ADVANCE(695);
      END_STATE();
    case 631:
      if (lookahead == 'r') ADVANCE(696);
      END_STATE();
    case 632:
      if (lookahead == 't') ADVANCE(697);
      END_STATE();
    case 633:
      if (lookahead == 'n') ADVANCE(698);
      END_STATE();
    case 634:
      if (lookahead == '-') ADVANCE(699);
      END_STATE();
    case 635:
      if (lookahead == 'h') ADVANCE(700);
      END_STATE();
    case 636:
      if (lookahead == 'a') ADVANCE(701);
      END_STATE();
    case 637:
      if (lookahead == 'a') ADVANCE(702);
      END_STATE();
    case 638:
      if (lookahead == 'f') ADVANCE(703);
      END_STATE();
    case 639:
      if (lookahead == 'n') ADVANCE(704);
      END_STATE();
    case 640:
      ACCEPT_TOKEN(anon_sym_default_DASHto);
      END_STATE();
    case 641:
      if (lookahead == 's') ADVANCE(705);
      END_STATE();
    case 642:
      if (lookahead == 'a') ADVANCE(706);
      END_STATE();
    case 643:
      if (lookahead == 'g') ADVANCE(707);
      END_STATE();
    case 644:
      ACCEPT_TOKEN(anon_sym_define_DASHmap);
      END_STATE();
    case 645:
      if (lookahead == '-') ADVANCE(708);
      END_STATE();
    case 646:
      if (lookahead == 'v') ADVANCE(709);
      END_STATE();
    case 647:
      if (lookahead == 'l') ADVANCE(710);
      END_STATE();
    case 648:
      if (lookahead == 'd') ADVANCE(711);
      END_STATE();
    case 649:
      if (lookahead == 'i') ADVANCE(712);
      END_STATE();
    case 650:
      ACCEPT_TOKEN(anon_sym_element_DASHat);
      if (lookahead == '?') ADVANCE(713);
      END_STATE();
    case 651:
      if (lookahead == 'n') ADVANCE(714);
      END_STATE();
    case 652:
      if (lookahead == 'a') ADVANCE(715);
      END_STATE();
    case 653:
      if (lookahead == 'p') ADVANCE(716);
      END_STATE();
    case 654:
      if (lookahead == 'r') ADVANCE(717);
      END_STATE();
    case 655:
      if (lookahead == 'i') ADVANCE(718);
      END_STATE();
    case 656:
      if (lookahead == 'l') ADVANCE(719);
      END_STATE();
    case 657:
      if (lookahead == '-') ADVANCE(720);
      END_STATE();
    case 658:
      if (lookahead == '-') ADVANCE(721);
      END_STATE();
    case 659:
      if (lookahead == 'h') ADVANCE(722);
      END_STATE();
    case 660:
      if (lookahead == 'h') ADVANCE(723);
      END_STATE();
    case 661:
      ACCEPT_TOKEN(anon_sym_impl_DASHtrait);
      END_STATE();
    case 662:
      if (lookahead == 'i') ADVANCE(724);
      END_STATE();
    case 663:
      if (lookahead == '8') ADVANCE(725);
      END_STATE();
    case 664:
      if (lookahead == 'n') ADVANCE(726);
      END_STATE();
    case 665:
      if (lookahead == 'e') ADVANCE(727);
      END_STATE();
    case 666:
      if (lookahead == 'd') ADVANCE(728);
      END_STATE();
    case 667:
      ACCEPT_TOKEN(anon_sym_map_DASHdelete);
      END_STATE();
    case 668:
      ACCEPT_TOKEN(anon_sym_map_DASHinsert);
      END_STATE();
    case 669:
      if (lookahead == 'e') ADVANCE(729);
      END_STATE();
    case 670:
      if (lookahead == 'd') ADVANCE(730);
      END_STATE();
    case 671:
      if (lookahead == 'n') ADVANCE(731);
      END_STATE();
    case 672:
      if (lookahead == 'e') ADVANCE(732);
      END_STATE();
    case 673:
      if (lookahead == 'c') ADVANCE(733);
      if (lookahead == 'd') ADVANCE(734);
      if (lookahead == 'o') ADVANCE(735);
      END_STATE();
    case 674:
      if (lookahead == '?') ADVANCE(736);
      END_STATE();
    case 675:
      if (lookahead == 's') ADVANCE(737);
      END_STATE();
    case 676:
      if (lookahead == 'r') ADVANCE(738);
      if (lookahead == 'v') ADVANCE(739);
      END_STATE();
    case 677:
      if (lookahead == 'v') ADVANCE(740);
      END_STATE();
    case 678:
      if (lookahead == 'c') ADVANCE(741);
      END_STATE();
    case 679:
      if (lookahead == 'i') ADVANCE(742);
      END_STATE();
    case 680:
      if (lookahead == 'i') ADVANCE(743);
      if (lookahead == 'u') ADVANCE(744);
      END_STATE();
    case 681:
      if (lookahead == '8') ADVANCE(745);
      END_STATE();
    case 682:
      if (lookahead == 't') ADVANCE(746);
      END_STATE();
    case 683:
      if (lookahead == 'l') ADVANCE(747);
      END_STATE();
    case 684:
      if (lookahead == '-') ADVANCE(748);
      END_STATE();
    case 685:
      if (lookahead == 'e') ADVANCE(749);
      END_STATE();
    case 686:
      if (lookahead == 'g') ADVANCE(750);
      END_STATE();
    case 687:
      if (lookahead == 'u') ADVANCE(751);
      END_STATE();
    case 688:
      if (lookahead == '?') ADVANCE(752);
      END_STATE();
    case 689:
      if (lookahead == '!') ADVANCE(753);
      if (lookahead == '-') ADVANCE(754);
      END_STATE();
    case 690:
      if (lookahead == 'i') ADVANCE(755);
      END_STATE();
    case 691:
      ACCEPT_TOKEN(anon_sym_as_DASHcontract);
      END_STATE();
    case 692:
      ACCEPT_TOKEN(anon_sym_as_DASHmax_DASHlen_QMARK);
      END_STATE();
    case 693:
      if (lookahead == 'e') ADVANCE(756);
      END_STATE();
    case 694:
      if (lookahead == 'i') ADVANCE(757);
      END_STATE();
    case 695:
      if (lookahead == 't') ADVANCE(758);
      END_STATE();
    case 696:
      if (lookahead == 'd') ADVANCE(759);
      END_STATE();
    case 697:
      if (lookahead == '-') ADVANCE(760);
      END_STATE();
    case 698:
      if (lookahead == 't') ADVANCE(761);
      END_STATE();
    case 699:
      if (lookahead == 'h') ADVANCE(762);
      END_STATE();
    case 700:
      if (lookahead == 'e') ADVANCE(763);
      END_STATE();
    case 701:
      if (lookahead == 'l') ADVANCE(764);
      END_STATE();
    case 702:
      if (lookahead == 's') ADVANCE(765);
      END_STATE();
    case 703:
      ACCEPT_TOKEN(anon_sym_contract_DASHof);
      END_STATE();
    case 704:
      if (lookahead == 't') ADVANCE(766);
      END_STATE();
    case 705:
      if (lookahead == 't') ADVANCE(767);
      END_STATE();
    case 706:
      if (lookahead == '-') ADVANCE(768);
      END_STATE();
    case 707:
      if (lookahead == 'i') ADVANCE(769);
      END_STATE();
    case 708:
      if (lookahead == 'f') ADVANCE(770);
      END_STATE();
    case 709:
      if (lookahead == 'a') ADVANCE(771);
      END_STATE();
    case 710:
      if (lookahead == 'i') ADVANCE(772);
      END_STATE();
    case 711:
      if (lookahead == '-') ADVANCE(773);
      END_STATE();
    case 712:
      if (lookahead == 't') ADVANCE(774);
      END_STATE();
    case 713:
      ACCEPT_TOKEN(anon_sym_element_DASHat_QMARK);
      END_STATE();
    case 714:
      if (lookahead == 's') ADVANCE(775);
      END_STATE();
    case 715:
      if (lookahead == 'n') ADVANCE(776);
      END_STATE();
    case 716:
      if (lookahead == 'l') ADVANCE(777);
      END_STATE();
    case 717:
      if (lookahead == '?') ADVANCE(778);
      END_STATE();
    case 718:
      if (lookahead == 'n') ADVANCE(779);
      END_STATE();
    case 719:
      if (lookahead == 'o') ADVANCE(780);
      END_STATE();
    case 720:
      if (lookahead == 'b') ADVANCE(781);
      END_STATE();
    case 721:
      if (lookahead == 'i') ADVANCE(782);
      END_STATE();
    case 722:
      ACCEPT_TOKEN(anon_sym_header_DASHhash);
      END_STATE();
    case 723:
      if (lookahead == 'a') ADVANCE(783);
      END_STATE();
    case 724:
      if (lookahead == 'i') ADVANCE(784);
      END_STATE();
    case 725:
      ACCEPT_TOKEN(anon_sym_int_DASHto_DASHutf8);
      END_STATE();
    case 726:
      if (lookahead == 'e') ADVANCE(785);
      END_STATE();
    case 727:
      if (lookahead == 's') ADVANCE(786);
      END_STATE();
    case 728:
      ACCEPT_TOKEN(anon_sym_is_DASHstandard);
      END_STATE();
    case 729:
      if (lookahead == 's') ADVANCE(787);
      END_STATE();
    case 730:
      if (lookahead == '-') ADVANCE(788);
      END_STATE();
    case 731:
      if (lookahead == 'e') ADVANCE(789);
      END_STATE();
    case 732:
      if (lookahead == 'r') ADVANCE(790);
      END_STATE();
    case 733:
      if (lookahead == 'o') ADVANCE(791);
      END_STATE();
    case 734:
      if (lookahead == 'e') ADVANCE(792);
      END_STATE();
    case 735:
      if (lookahead == 'f') ADVANCE(793);
      END_STATE();
    case 736:
      ACCEPT_TOKEN(anon_sym_replace_DASHat_QMARK);
      END_STATE();
    case 737:
      if (lookahead == 's') ADVANCE(794);
      END_STATE();
    case 738:
      if (lookahead == 'e') ADVANCE(795);
      END_STATE();
    case 739:
      if (lookahead == 'e') ADVANCE(796);
      END_STATE();
    case 740:
      if (lookahead == 'e') ADVANCE(797);
      END_STATE();
    case 741:
      if (lookahead == 'k') ADVANCE(798);
      END_STATE();
    case 742:
      if (lookahead == 'i') ADVANCE(799);
      END_STATE();
    case 743:
      if (lookahead == 'n') ADVANCE(800);
      END_STATE();
    case 744:
      if (lookahead == 'i') ADVANCE(801);
      END_STATE();
    case 745:
      ACCEPT_TOKEN(anon_sym_string_DASHutf8);
      END_STATE();
    case 746:
      ACCEPT_TOKEN(anon_sym_stx_DASHaccount);
      END_STATE();
    case 747:
      if (lookahead == 'a') ADVANCE(802);
      END_STATE();
    case 748:
      if (lookahead == 's') ADVANCE(803);
      END_STATE();
    case 749:
      if (lookahead == 'r') ADVANCE(804);
      END_STATE();
    case 750:
      if (lookahead == 'h') ADVANCE(805);
      END_STATE();
    case 751:
      if (lookahead == 's') ADVANCE(806);
      END_STATE();
    case 752:
      ACCEPT_TOKEN(anon_sym_tx_DASHsponsor_QMARK);
      END_STATE();
    case 753:
      ACCEPT_TOKEN(anon_sym_unwrap_DASHerr_BANG);
      END_STATE();
    case 754:
      if (lookahead == 'p') ADVANCE(807);
      END_STATE();
    case 755:
      if (lookahead == 'c') ADVANCE(808);
      END_STATE();
    case 756:
      if (lookahead == 'f') ADVANCE(809);
      END_STATE();
    case 757:
      if (lookahead == 'g') ADVANCE(810);
      END_STATE();
    case 758:
      ACCEPT_TOKEN(anon_sym_block_DASHheight);
      END_STATE();
    case 759:
      ACCEPT_TOKEN(anon_sym_block_DASHreward);
      END_STATE();
    case 760:
      if (lookahead == 'b') ADVANCE(811);
      if (lookahead == 'l') ADVANCE(812);
      END_STATE();
    case 761:
      if (lookahead == '-') ADVANCE(813);
      END_STATE();
    case 762:
      if (lookahead == 'e') ADVANCE(814);
      END_STATE();
    case 763:
      if (lookahead == 'a') ADVANCE(815);
      END_STATE();
    case 764:
      if (lookahead == 'l') ADVANCE(816);
      END_STATE();
    case 765:
      if (lookahead == 'h') ADVANCE(817);
      END_STATE();
    case 766:
      if (lookahead == 'r') ADVANCE(818);
      END_STATE();
    case 767:
      if (lookahead == 'a') ADVANCE(819);
      END_STATE();
    case 768:
      if (lookahead == 'v') ADVANCE(820);
      END_STATE();
    case 769:
      if (lookahead == 'b') ADVANCE(821);
      END_STATE();
    case 770:
      if (lookahead == 'u') ADVANCE(822);
      END_STATE();
    case 771:
      if (lookahead == 't') ADVANCE(823);
      END_STATE();
    case 772:
      if (lookahead == 'c') ADVANCE(824);
      END_STATE();
    case 773:
      if (lookahead == 'o') ADVANCE(825);
      END_STATE();
    case 774:
      ACCEPT_TOKEN(anon_sym_define_DASHtrait);
      END_STATE();
    case 775:
      if (lookahead == 'u') ADVANCE(826);
      END_STATE();
    case 776:
      if (lookahead == 'c') ADVANCE(827);
      END_STATE();
    case 777:
      if (lookahead == 'y') ADVANCE(828);
      END_STATE();
    case 778:
      ACCEPT_TOKEN(anon_sym_ft_DASHtransfer_QMARK);
      END_STATE();
    case 779:
      if (lookahead == 'f') ADVANCE(829);
      END_STATE();
    case 780:
      if (lookahead == 'c') ADVANCE(830);
      END_STATE();
    case 781:
      if (lookahead == 'l') ADVANCE(831);
      END_STATE();
    case 782:
      if (lookahead == 'n') ADVANCE(832);
      END_STATE();
    case 783:
      if (lookahead == 's') ADVANCE(833);
      END_STATE();
    case 784:
      ACCEPT_TOKEN(anon_sym_int_DASHto_DASHascii);
      END_STATE();
    case 785:
      if (lookahead == 't') ADVANCE(834);
      END_STATE();
    case 786:
      if (lookahead == 't') ADVANCE(835);
      END_STATE();
    case 787:
      if (lookahead == 's') ADVANCE(836);
      END_STATE();
    case 788:
      if (lookahead == 't') ADVANCE(837);
      if (lookahead == 'w') ADVANCE(838);
      END_STATE();
    case 789:
      if (lookahead == 'r') ADVANCE(839);
      END_STATE();
    case 790:
      if (lookahead == '?') ADVANCE(840);
      END_STATE();
    case 791:
      if (lookahead == 'n') ADVANCE(841);
      END_STATE();
    case 792:
      if (lookahead == 's') ADVANCE(842);
      END_STATE();
    case 793:
      if (lookahead == '?') ADVANCE(843);
      END_STATE();
    case 794:
      if (lookahead == 'e') ADVANCE(844);
      END_STATE();
    case 795:
      if (lookahead == 'c') ADVANCE(845);
      END_STATE();
    case 796:
      if (lookahead == 'r') ADVANCE(846);
      END_STATE();
    case 797:
      if (lookahead == 'r') ADVANCE(847);
      END_STATE();
    case 798:
      if (lookahead == '-') ADVANCE(848);
      END_STATE();
    case 799:
      ACCEPT_TOKEN(anon_sym_string_DASHascii);
      END_STATE();
    case 800:
      if (lookahead == 't') ADVANCE(849);
      END_STATE();
    case 801:
      if (lookahead == 'n') ADVANCE(850);
      END_STATE();
    case 802:
      if (lookahead == 'n') ADVANCE(851);
      END_STATE();
    case 803:
      if (lookahead == 'u') ADVANCE(852);
      END_STATE();
    case 804:
      if (lookahead == '-') ADVANCE(853);
      if (lookahead == '?') ADVANCE(854);
      END_STATE();
    case 805:
      if (lookahead == 't') ADVANCE(855);
      END_STATE();
    case 806:
      if (lookahead == '-') ADVANCE(856);
      END_STATE();
    case 807:
      if (lookahead == 'a') ADVANCE(857);
      END_STATE();
    case 808:
      ACCEPT_TOKEN(anon_sym_unwrap_DASHpanic);
      END_STATE();
    case 809:
      if (lookahead == 't') ADVANCE(858);
      END_STATE();
    case 810:
      if (lookahead == 'h') ADVANCE(859);
      END_STATE();
    case 811:
      if (lookahead == 'e') ADVANCE(860);
      END_STATE();
    case 812:
      if (lookahead == 'e') ADVANCE(861);
      END_STATE();
    case 813:
      if (lookahead == 'b') ADVANCE(862);
      if (lookahead == 'l') ADVANCE(863);
      END_STATE();
    case 814:
      if (lookahead == 'i') ADVANCE(864);
      END_STATE();
    case 815:
      if (lookahead == 'd') ADVANCE(865);
      END_STATE();
    case 816:
      if (lookahead == '?') ADVANCE(866);
      if (lookahead == 'e') ADVANCE(867);
      END_STATE();
    case 817:
      if (lookahead == '?') ADVANCE(868);
      END_STATE();
    case 818:
      if (lookahead == 'a') ADVANCE(869);
      END_STATE();
    case 819:
      if (lookahead == 'n') ADVANCE(870);
      END_STATE();
    case 820:
      if (lookahead == 'a') ADVANCE(871);
      END_STATE();
    case 821:
      if (lookahead == 'l') ADVANCE(872);
      END_STATE();
    case 822:
      if (lookahead == 'n') ADVANCE(873);
      END_STATE();
    case 823:
      if (lookahead == 'e') ADVANCE(874);
      END_STATE();
    case 824:
      ACCEPT_TOKEN(anon_sym_define_DASHpublic);
      END_STATE();
    case 825:
      if (lookahead == 'n') ADVANCE(875);
      END_STATE();
    case 826:
      if (lookahead == 's') ADVANCE(876);
      END_STATE();
    case 827:
      if (lookahead == 'e') ADVANCE(877);
      END_STATE();
    case 828:
      ACCEPT_TOKEN(anon_sym_ft_DASHget_DASHsupply);
      END_STATE();
    case 829:
      if (lookahead == 'o') ADVANCE(878);
      END_STATE();
    case 830:
      if (lookahead == 'k') ADVANCE(879);
      END_STATE();
    case 831:
      if (lookahead == 'o') ADVANCE(880);
      END_STATE();
    case 832:
      if (lookahead == 'f') ADVANCE(881);
      END_STATE();
    case 833:
      if (lookahead == 'h') ADVANCE(882);
      END_STATE();
    case 834:
      ACCEPT_TOKEN(anon_sym_is_DASHin_DASHmainnet);
      END_STATE();
    case 835:
      ACCEPT_TOKEN(anon_sym_is_DASHin_DASHregtest);
      END_STATE();
    case 836:
      ACCEPT_TOKEN(anon_sym_miner_DASHaddress);
      END_STATE();
    case 837:
      if (lookahead == 'o') ADVANCE(883);
      END_STATE();
    case 838:
      if (lookahead == 'i') ADVANCE(884);
      END_STATE();
    case 839:
      if (lookahead == '?') ADVANCE(885);
      END_STATE();
    case 840:
      ACCEPT_TOKEN(anon_sym_nft_DASHtransfer_QMARK);
      END_STATE();
    case 841:
      if (lookahead == 's') ADVANCE(886);
      END_STATE();
    case 842:
      if (lookahead == 't') ADVANCE(887);
      END_STATE();
    case 843:
      ACCEPT_TOKEN(anon_sym_principal_DASHof_QMARK);
      END_STATE();
    case 844:
      if (lookahead == 't') ADVANCE(888);
      END_STATE();
    case 845:
      if (lookahead == 'o') ADVANCE(889);
      END_STATE();
    case 846:
      if (lookahead == 'i') ADVANCE(890);
      END_STATE();
    case 847:
      if (lookahead == 'i') ADVANCE(891);
      END_STATE();
    case 848:
      if (lookahead == 'h') ADVANCE(892);
      if (lookahead == 't') ADVANCE(893);
      END_STATE();
    case 849:
      if (lookahead == '?') ADVANCE(894);
      END_STATE();
    case 850:
      if (lookahead == 't') ADVANCE(895);
      END_STATE();
    case 851:
      if (lookahead == 'c') ADVANCE(896);
      END_STATE();
    case 852:
      if (lookahead == 'p') ADVANCE(897);
      END_STATE();
    case 853:
      if (lookahead == 'm') ADVANCE(898);
      END_STATE();
    case 854:
      ACCEPT_TOKEN(anon_sym_stx_DASHtransfer_QMARK);
      END_STATE();
    case 855:
      ACCEPT_TOKEN(anon_sym_tenure_DASHheight);
      END_STATE();
    case 856:
      if (lookahead == 'b') ADVANCE(899);
      END_STATE();
    case 857:
      if (lookahead == 'n') ADVANCE(900);
      END_STATE();
    case 858:
      ACCEPT_TOKEN(anon_sym_bit_DASHshift_DASHleft);
      END_STATE();
    case 859:
      if (lookahead == 't') ADVANCE(901);
      END_STATE();
    case 860:
      ACCEPT_TOKEN(anon_sym_buff_DASHto_DASHint_DASHbe);
      END_STATE();
    case 861:
      ACCEPT_TOKEN(anon_sym_buff_DASHto_DASHint_DASHle);
      END_STATE();
    case 862:
      if (lookahead == 'e') ADVANCE(902);
      END_STATE();
    case 863:
      if (lookahead == 'e') ADVANCE(903);
      END_STATE();
    case 864:
      if (lookahead == 'g') ADVANCE(904);
      END_STATE();
    case 865:
      if (lookahead == 'e') ADVANCE(905);
      END_STATE();
    case 866:
      ACCEPT_TOKEN(anon_sym_contract_DASHcall_QMARK);
      END_STATE();
    case 867:
      if (lookahead == 'r') ADVANCE(906);
      END_STATE();
    case 868:
      ACCEPT_TOKEN(anon_sym_contract_DASHhash_QMARK);
      END_STATE();
    case 869:
      if (lookahead == 'c') ADVANCE(907);
      END_STATE();
    case 870:
      if (lookahead == 't') ADVANCE(908);
      END_STATE();
    case 871:
      if (lookahead == 'r') ADVANCE(909);
      END_STATE();
    case 872:
      if (lookahead == 'e') ADVANCE(910);
      END_STATE();
    case 873:
      if (lookahead == 'g') ADVANCE(911);
      END_STATE();
    case 874:
      ACCEPT_TOKEN(anon_sym_define_DASHprivate);
      END_STATE();
    case 875:
      if (lookahead == 'l') ADVANCE(912);
      END_STATE();
    case 876:
      if (lookahead == '-') ADVANCE(913);
      END_STATE();
    case 877:
      ACCEPT_TOKEN(anon_sym_ft_DASHget_DASHbalance);
      END_STATE();
    case 878:
      if (lookahead == '?') ADVANCE(914);
      END_STATE();
    case 879:
      if (lookahead == '-') ADVANCE(915);
      END_STATE();
    case 880:
      if (lookahead == 'c') ADVANCE(916);
      END_STATE();
    case 881:
      if (lookahead == 'o') ADVANCE(917);
      END_STATE();
    case 882:
      ACCEPT_TOKEN(anon_sym_id_DASHheader_DASHhash);
      END_STATE();
    case 883:
      if (lookahead == 't') ADVANCE(918);
      END_STATE();
    case 884:
      if (lookahead == 'n') ADVANCE(919);
      END_STATE();
    case 885:
      ACCEPT_TOKEN(anon_sym_nft_DASHget_DASHowner_QMARK);
      END_STATE();
    case 886:
      if (lookahead == 't') ADVANCE(920);
      END_STATE();
    case 887:
      if (lookahead == 'r') ADVANCE(921);
      END_STATE();
    case 888:
      if (lookahead == 's') ADVANCE(922);
      END_STATE();
    case 889:
      if (lookahead == 'v') ADVANCE(923);
      END_STATE();
    case 890:
      if (lookahead == 'f') ADVANCE(924);
      END_STATE();
    case 891:
      if (lookahead == 'f') ADVANCE(925);
      END_STATE();
    case 892:
      if (lookahead == 'e') ADVANCE(926);
      END_STATE();
    case 893:
      if (lookahead == 'i') ADVANCE(927);
      END_STATE();
    case 894:
      ACCEPT_TOKEN(anon_sym_string_DASHto_DASHint_QMARK);
      END_STATE();
    case 895:
      if (lookahead == '?') ADVANCE(928);
      END_STATE();
    case 896:
      if (lookahead == 'e') ADVANCE(929);
      END_STATE();
    case 897:
      if (lookahead == 'p') ADVANCE(930);
      END_STATE();
    case 898:
      if (lookahead == 'e') ADVANCE(931);
      END_STATE();
    case 899:
      if (lookahead == 'u') ADVANCE(932);
      END_STATE();
    case 900:
      if (lookahead == 'i') ADVANCE(933);
      END_STATE();
    case 901:
      ACCEPT_TOKEN(anon_sym_bit_DASHshift_DASHright);
      END_STATE();
    case 902:
      ACCEPT_TOKEN(anon_sym_buff_DASHto_DASHuint_DASHbe);
      END_STATE();
    case 903:
      ACCEPT_TOKEN(anon_sym_buff_DASHto_DASHuint_DASHle);
      END_STATE();
    case 904:
      if (lookahead == 'h') ADVANCE(934);
      END_STATE();
    case 905:
      if (lookahead == 'r') ADVANCE(935);
      END_STATE();
    case 906:
      ACCEPT_TOKEN(anon_sym_contract_DASHcaller);
      END_STATE();
    case 907:
      if (lookahead == 't') ADVANCE(936);
      END_STATE();
    case 908:
      ACCEPT_TOKEN(anon_sym_define_DASHconstant);
      END_STATE();
    case 909:
      ACCEPT_TOKEN(anon_sym_define_DASHdata_DASHvar);
      END_STATE();
    case 910:
      if (lookahead == '-') ADVANCE(937);
      END_STATE();
    case 911:
      if (lookahead == 'i') ADVANCE(938);
      END_STATE();
    case 912:
      if (lookahead == 'y') ADVANCE(939);
      END_STATE();
    case 913:
      if (lookahead == 'b') ADVANCE(940);
      END_STATE();
    case 914:
      ACCEPT_TOKEN(anon_sym_get_DASHblock_DASHinfo_QMARK);
      END_STATE();
    case 915:
      if (lookahead == 'i') ADVANCE(941);
      END_STATE();
    case 916:
      if (lookahead == 'k') ADVANCE(942);
      END_STATE();
    case 917:
      if (lookahead == '?') ADVANCE(943);
      END_STATE();
    case 918:
      if (lookahead == 'a') ADVANCE(944);
      END_STATE();
    case 919:
      if (lookahead == 'n') ADVANCE(945);
      END_STATE();
    case 920:
      if (lookahead == 'r') ADVANCE(946);
      END_STATE();
    case 921:
      if (lookahead == 'u') ADVANCE(947);
      END_STATE();
    case 922:
      if (lookahead == '?') ADVANCE(948);
      END_STATE();
    case 923:
      if (lookahead == 'e') ADVANCE(949);
      END_STATE();
    case 924:
      if (lookahead == 'y') ADVANCE(950);
      END_STATE();
    case 925:
      if (lookahead == 'y') ADVANCE(951);
      END_STATE();
    case 926:
      if (lookahead == 'i') ADVANCE(952);
      END_STATE();
    case 927:
      if (lookahead == 'm') ADVANCE(953);
      END_STATE();
    case 928:
      ACCEPT_TOKEN(anon_sym_string_DASHto_DASHuint_QMARK);
      END_STATE();
    case 929:
      ACCEPT_TOKEN(anon_sym_stx_DASHget_DASHbalance);
      END_STATE();
    case 930:
      if (lookahead == 'l') ADVANCE(954);
      END_STATE();
    case 931:
      if (lookahead == 'm') ADVANCE(955);
      END_STATE();
    case 932:
      if (lookahead == 'f') ADVANCE(956);
      END_STATE();
    case 933:
      if (lookahead == 'c') ADVANCE(957);
      END_STATE();
    case 934:
      if (lookahead == 't') ADVANCE(958);
      END_STATE();
    case 935:
      if (lookahead == '-') ADVANCE(959);
      END_STATE();
    case 936:
      ACCEPT_TOKEN(anon_sym_current_DASHcontract);
      END_STATE();
    case 937:
      if (lookahead == 't') ADVANCE(960);
      END_STATE();
    case 938:
      if (lookahead == 'b') ADVANCE(961);
      END_STATE();
    case 939:
      ACCEPT_TOKEN(anon_sym_define_DASHread_DASHonly);
      END_STATE();
    case 940:
      if (lookahead == 'u') ADVANCE(962);
      END_STATE();
    case 941:
      if (lookahead == 'n') ADVANCE(963);
      END_STATE();
    case 942:
      if (lookahead == '-') ADVANCE(964);
      END_STATE();
    case 943:
      ACCEPT_TOKEN(anon_sym_get_DASHtenure_DASHinfo_QMARK);
      END_STATE();
    case 944:
      if (lookahead == 'l') ADVANCE(965);
      END_STATE();
    case 945:
      if (lookahead == 'e') ADVANCE(966);
      END_STATE();
    case 946:
      if (lookahead == 'u') ADVANCE(967);
      END_STATE();
    case 947:
      if (lookahead == 'c') ADVANCE(968);
      END_STATE();
    case 948:
      ACCEPT_TOKEN(anon_sym_restrict_DASHassets_QMARK);
      END_STATE();
    case 949:
      if (lookahead == 'r') ADVANCE(969);
      END_STATE();
    case 950:
      ACCEPT_TOKEN(anon_sym_secp256k1_DASHverify);
      END_STATE();
    case 951:
      ACCEPT_TOKEN(anon_sym_secp256r1_DASHverify);
      END_STATE();
    case 952:
      if (lookahead == 'g') ADVANCE(970);
      END_STATE();
    case 953:
      if (lookahead == 'e') ADVANCE(971);
      END_STATE();
    case 954:
      if (lookahead == 'y') ADVANCE(972);
      END_STATE();
    case 955:
      if (lookahead == 'o') ADVANCE(973);
      END_STATE();
    case 956:
      if (lookahead == 'f') ADVANCE(974);
      END_STATE();
    case 957:
      ACCEPT_TOKEN(anon_sym_unwrap_DASHerr_DASHpanic);
      END_STATE();
    case 958:
      ACCEPT_TOKEN(anon_sym_burn_DASHblock_DASHheight);
      END_STATE();
    case 959:
      if (lookahead == 'h') ADVANCE(975);
      END_STATE();
    case 960:
      if (lookahead == 'o') ADVANCE(976);
      END_STATE();
    case 961:
      if (lookahead == 'l') ADVANCE(977);
      END_STATE();
    case 962:
      if (lookahead == 'f') ADVANCE(978);
      END_STATE();
    case 963:
      if (lookahead == 'f') ADVANCE(979);
      END_STATE();
    case 964:
      if (lookahead == 'i') ADVANCE(980);
      END_STATE();
    case 965:
      ACCEPT_TOKEN(anon_sym_miner_DASHspend_DASHtotal);
      END_STATE();
    case 966:
      if (lookahead == 'r') ADVANCE(981);
      END_STATE();
    case 967:
      if (lookahead == 'c') ADVANCE(982);
      END_STATE();
    case 968:
      if (lookahead == 't') ADVANCE(983);
      END_STATE();
    case 969:
      if (lookahead == '?') ADVANCE(984);
      END_STATE();
    case 970:
      if (lookahead == 'h') ADVANCE(985);
      END_STATE();
    case 971:
      ACCEPT_TOKEN(anon_sym_stacks_DASHblock_DASHtime);
      END_STATE();
    case 972:
      ACCEPT_TOKEN(anon_sym_stx_DASHliquid_DASHsupply);
      END_STATE();
    case 973:
      if (lookahead == '?') ADVANCE(986);
      END_STATE();
    case 974:
      if (lookahead == '?') ADVANCE(987);
      END_STATE();
    case 975:
      if (lookahead == 'a') ADVANCE(988);
      END_STATE();
    case 976:
      if (lookahead == 'k') ADVANCE(989);
      END_STATE();
    case 977:
      if (lookahead == 'e') ADVANCE(990);
      END_STATE();
    case 978:
      if (lookahead == 'f') ADVANCE(991);
      END_STATE();
    case 979:
      if (lookahead == 'o') ADVANCE(992);
      END_STATE();
    case 980:
      if (lookahead == 'n') ADVANCE(993);
      END_STATE();
    case 981:
      ACCEPT_TOKEN(anon_sym_miner_DASHspend_DASHwinner);
      END_STATE();
    case 982:
      if (lookahead == 't') ADVANCE(994);
      END_STATE();
    case 983:
      if (lookahead == '?') ADVANCE(995);
      END_STATE();
    case 984:
      ACCEPT_TOKEN(anon_sym_secp256k1_DASHrecover_QMARK);
      END_STATE();
    case 985:
      if (lookahead == 't') ADVANCE(996);
      END_STATE();
    case 986:
      ACCEPT_TOKEN(anon_sym_stx_DASHtransfer_DASHmemo_QMARK);
      END_STATE();
    case 987:
      ACCEPT_TOKEN(anon_sym_to_DASHconsensus_DASHbuff_QMARK);
      END_STATE();
    case 988:
      if (lookahead == 's') ADVANCE(997);
      END_STATE();
    case 989:
      if (lookahead == 'e') ADVANCE(998);
      END_STATE();
    case 990:
      if (lookahead == '-') ADVANCE(999);
      END_STATE();
    case 991:
      if (lookahead == '?') ADVANCE(1000);
      END_STATE();
    case 992:
      if (lookahead == '?') ADVANCE(1001);
      END_STATE();
    case 993:
      if (lookahead == 'f') ADVANCE(1002);
      END_STATE();
    case 994:
      if (lookahead == '?') ADVANCE(1003);
      END_STATE();
    case 995:
      ACCEPT_TOKEN(anon_sym_principal_DASHdestruct_QMARK);
      END_STATE();
    case 996:
      ACCEPT_TOKEN(anon_sym_stacks_DASHblock_DASHheight);
      END_STATE();
    case 997:
      if (lookahead == 'h') ADVANCE(1004);
      END_STATE();
    case 998:
      if (lookahead == 'n') ADVANCE(1005);
      END_STATE();
    case 999:
      if (lookahead == 't') ADVANCE(1006);
      END_STATE();
    case 1000:
      ACCEPT_TOKEN(anon_sym_from_DASHconsensus_DASHbuff_QMARK);
      END_STATE();
    case 1001:
      ACCEPT_TOKEN(anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK);
      END_STATE();
    case 1002:
      if (lookahead == 'o') ADVANCE(1007);
      END_STATE();
    case 1003:
      ACCEPT_TOKEN(anon_sym_principal_DASHconstruct_QMARK);
      END_STATE();
    case 1004:
      ACCEPT_TOKEN(anon_sym_burnchain_DASHheader_DASHhash);
      END_STATE();
    case 1005:
      ACCEPT_TOKEN(anon_sym_define_DASHfungible_DASHtoken);
      END_STATE();
    case 1006:
      if (lookahead == 'o') ADVANCE(1008);
      END_STATE();
    case 1007:
      if (lookahead == '?') ADVANCE(1009);
      END_STATE();
    case 1008:
      if (lookahead == 'k') ADVANCE(1010);
      END_STATE();
    case 1009:
      ACCEPT_TOKEN(anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK);
      END_STATE();
    case 1010:
      if (lookahead == 'e') ADVANCE(1011);
      END_STATE();
    case 1011:
      if (lookahead == 'n') ADVANCE(1012);
      END_STATE();
    case 1012:
      ACCEPT_TOKEN(anon_sym_define_DASHnon_DASHfungible_DASHtoken);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 0},
  [2] = {.lex_state = 9},
  [3] = {.lex_state = 9},
  [4] = {.lex_state = 9},
  [5] = {.lex_state = 9},
  [6] = {.lex_state = 0},
  [7] = {.lex_state = 0},
  [8] = {.lex_state = 0},
  [9] = {.lex_state = 0},
  [10] = {.lex_state = 0},
  [11] = {.lex_state = 0},
  [12] = {.lex_state = 0},
  [13] = {.lex_state = 0},
  [14] = {.lex_state = 0},
  [15] = {.lex_state = 0},
  [16] = {.lex_state = 0},
  [17] = {.lex_state = 0},
  [18] = {.lex_state = 0},
  [19] = {.lex_state = 0},
  [20] = {.lex_state = 0},
  [21] = {.lex_state = 0},
  [22] = {.lex_state = 0},
  [23] = {.lex_state = 0},
  [24] = {.lex_state = 0},
  [25] = {.lex_state = 0},
  [26] = {.lex_state = 0},
  [27] = {.lex_state = 0},
  [28] = {.lex_state = 0},
  [29] = {.lex_state = 0},
  [30] = {.lex_state = 0},
  [31] = {.lex_state = 0},
  [32] = {.lex_state = 0},
  [33] = {.lex_state = 0},
  [34] = {.lex_state = 0},
  [35] = {.lex_state = 0},
  [36] = {.lex_state = 0},
  [37] = {.lex_state = 0},
  [38] = {.lex_state = 0},
  [39] = {.lex_state = 0},
  [40] = {.lex_state = 0},
  [41] = {.lex_state = 0},
  [42] = {.lex_state = 0},
  [43] = {.lex_state = 0},
  [44] = {.lex_state = 0},
  [45] = {.lex_state = 0},
  [46] = {.lex_state = 0},
  [47] = {.lex_state = 0},
  [48] = {.lex_state = 0},
  [49] = {.lex_state = 0},
  [50] = {.lex_state = 0},
  [51] = {.lex_state = 0},
  [52] = {.lex_state = 0},
  [53] = {.lex_state = 0},
  [54] = {.lex_state = 0},
  [55] = {.lex_state = 0},
  [56] = {.lex_state = 0},
  [57] = {.lex_state = 0},
  [58] = {.lex_state = 0},
  [59] = {.lex_state = 0},
  [60] = {.lex_state = 0},
  [61] = {.lex_state = 0},
  [62] = {.lex_state = 0},
  [63] = {.lex_state = 0},
  [64] = {.lex_state = 0},
  [65] = {.lex_state = 0},
  [66] = {.lex_state = 0},
  [67] = {.lex_state = 0},
  [68] = {.lex_state = 0},
  [69] = {.lex_state = 0},
  [70] = {.lex_state = 0},
  [71] = {.lex_state = 0},
  [72] = {.lex_state = 9},
  [73] = {.lex_state = 9},
  [74] = {.lex_state = 9},
  [75] = {.lex_state = 9},
  [76] = {.lex_state = 9},
  [77] = {.lex_state = 9},
  [78] = {.lex_state = 9},
  [79] = {.lex_state = 9},
  [80] = {.lex_state = 9},
  [81] = {.lex_state = 9},
  [82] = {.lex_state = 9},
  [83] = {.lex_state = 9},
  [84] = {.lex_state = 9},
  [85] = {.lex_state = 9},
  [86] = {.lex_state = 9},
  [87] = {.lex_state = 9},
  [88] = {.lex_state = 9},
  [89] = {.lex_state = 9},
  [90] = {.lex_state = 9},
  [91] = {.lex_state = 0},
  [92] = {.lex_state = 9},
  [93] = {.lex_state = 0},
  [94] = {.lex_state = 0},
  [95] = {.lex_state = 0},
  [96] = {.lex_state = 0},
  [97] = {.lex_state = 0},
  [98] = {.lex_state = 0},
  [99] = {.lex_state = 0},
  [100] = {.lex_state = 0},
  [101] = {.lex_state = 0},
  [102] = {.lex_state = 0},
  [103] = {.lex_state = 0},
  [104] = {.lex_state = 0},
  [105] = {.lex_state = 0},
  [106] = {.lex_state = 0},
  [107] = {.lex_state = 9},
  [108] = {.lex_state = 9},
  [109] = {.lex_state = 9},
  [110] = {.lex_state = 9},
  [111] = {.lex_state = 9},
  [112] = {.lex_state = 9},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 0},
  [118] = {.lex_state = 0},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 0},
  [122] = {.lex_state = 0},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 3},
  [125] = {.lex_state = 0},
  [126] = {.lex_state = 13},
  [127] = {.lex_state = 0},
  [128] = {.lex_state = 9},
  [129] = {.lex_state = 0},
  [130] = {.lex_state = 9},
  [131] = {.lex_state = 0},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 0},
  [134] = {.lex_state = 0},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 0},
  [137] = {.lex_state = 0},
  [138] = {.lex_state = 9},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 0},
  [141] = {.lex_state = 0},
  [142] = {.lex_state = 9},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 0},
  [145] = {.lex_state = 0},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 0},
  [149] = {.lex_state = 9},
  [150] = {.lex_state = 0},
  [151] = {.lex_state = 0},
  [152] = {.lex_state = 9},
  [153] = {.lex_state = 9},
  [154] = {.lex_state = 9},
  [155] = {.lex_state = 9},
  [156] = {.lex_state = 9},
  [157] = {.lex_state = 0},
  [158] = {.lex_state = 0},
  [159] = {.lex_state = 0},
  [160] = {.lex_state = 0},
  [161] = {.lex_state = 0},
  [162] = {.lex_state = 9},
  [163] = {.lex_state = 0},
  [164] = {.lex_state = 0},
  [165] = {.lex_state = 0},
  [166] = {.lex_state = 0},
  [167] = {.lex_state = 0},
  [168] = {.lex_state = 9},
  [169] = {.lex_state = 0},
  [170] = {.lex_state = 0},
  [171] = {.lex_state = 0},
  [172] = {.lex_state = 0},
  [173] = {.lex_state = 0},
  [174] = {.lex_state = 0},
  [175] = {.lex_state = 9},
  [176] = {.lex_state = 9},
  [177] = {.lex_state = 0},
  [178] = {.lex_state = 0},
  [179] = {.lex_state = 9},
  [180] = {.lex_state = 0},
  [181] = {.lex_state = 9},
  [182] = {.lex_state = 0},
  [183] = {.lex_state = 0},
  [184] = {.lex_state = 0},
  [185] = {.lex_state = 9},
  [186] = {.lex_state = 0},
  [187] = {.lex_state = 9},
  [188] = {.lex_state = 0},
  [189] = {.lex_state = 0},
  [190] = {.lex_state = 0},
  [191] = {.lex_state = 9},
  [192] = {.lex_state = 9},
  [193] = {.lex_state = 0},
  [194] = {.lex_state = 0},
  [195] = {.lex_state = 9},
  [196] = {.lex_state = 9},
  [197] = {.lex_state = 9},
  [198] = {.lex_state = 0},
  [199] = {.lex_state = 9},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 0},
  [202] = {.lex_state = 0},
  [203] = {.lex_state = 9},
  [204] = {.lex_state = 0},
  [205] = {.lex_state = 0},
  [206] = {.lex_state = 0},
  [207] = {.lex_state = 14},
  [208] = {.lex_state = 9},
  [209] = {.lex_state = 9},
  [210] = {.lex_state = 0},
  [211] = {.lex_state = 0},
  [212] = {.lex_state = 0},
  [213] = {.lex_state = 9},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym_identifier] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_define_DASHtrait] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_impl_DASHtrait] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [anon_sym_use_DASHtrait] = ACTIONS(1),
    [anon_sym_define_DASHfungible_DASHtoken] = ACTIONS(1),
    [anon_sym_define_DASHnon_DASHfungible_DASHtoken] = ACTIONS(1),
    [anon_sym_define_DASHconstant] = ACTIONS(1),
    [anon_sym_define_DASHdata_DASHvar] = ACTIONS(1),
    [anon_sym_define_DASHmap] = ACTIONS(1),
    [anon_sym_define_DASHprivate] = ACTIONS(1),
    [anon_sym_define_DASHread_DASHonly] = ACTIONS(1),
    [anon_sym_define_DASHpublic] = ACTIONS(1),
    [anon_sym_let] = ACTIONS(1),
    [anon_sym_LT] = ACTIONS(1),
    [anon_sym_GT] = ACTIONS(1),
    [anon_sym_int] = ACTIONS(1),
    [anon_sym_uint] = ACTIONS(1),
    [anon_sym_bool] = ACTIONS(1),
    [anon_sym_principal] = ACTIONS(1),
    [anon_sym_buff] = ACTIONS(1),
    [aux_sym_buffer_type_token1] = ACTIONS(1),
    [anon_sym_string_DASHascii] = ACTIONS(1),
    [anon_sym_string_DASHutf8] = ACTIONS(1),
    [anon_sym_list] = ACTIONS(1),
    [anon_sym_optional] = ACTIONS(1),
    [anon_sym_tuple] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_response] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [sym_uint_lit] = ACTIONS(1),
    [anon_sym_true] = ACTIONS(1),
    [anon_sym_false] = ACTIONS(1),
    [anon_sym_SQUOTE] = ACTIONS(1),
    [anon_sym_0x] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [sym_utf8_string_lit] = ACTIONS(1),
    [anon_sym_some] = ACTIONS(1),
    [sym_none_lit] = ACTIONS(1),
    [anon_sym_ok] = ACTIONS(1),
    [anon_sym_err] = ACTIONS(1),
    [anon_sym_PLUS] = ACTIONS(1),
    [anon_sym_STAR] = ACTIONS(1),
    [anon_sym_SLASH] = ACTIONS(1),
    [anon_sym_mod] = ACTIONS(1),
    [anon_sym_pow] = ACTIONS(1),
    [anon_sym_LT_EQ] = ACTIONS(1),
    [anon_sym_GT_EQ] = ACTIONS(1),
    [anon_sym_and] = ACTIONS(1),
    [anon_sym_or] = ACTIONS(1),
    [anon_sym_xor] = ACTIONS(1),
    [anon_sym_append] = ACTIONS(1),
    [anon_sym_as_DASHcontract] = ACTIONS(1),
    [anon_sym_as_DASHmax_DASHlen_QMARK] = ACTIONS(1),
    [anon_sym_asserts_BANG] = ACTIONS(1),
    [anon_sym_at_DASHblock] = ACTIONS(1),
    [anon_sym_begin] = ACTIONS(1),
    [anon_sym_bit_DASHand] = ACTIONS(1),
    [anon_sym_bit_DASHnot] = ACTIONS(1),
    [anon_sym_bit_DASHor] = ACTIONS(1),
    [anon_sym_bit_DASHshift_DASHleft] = ACTIONS(1),
    [anon_sym_bit_DASHshift_DASHright] = ACTIONS(1),
    [anon_sym_bit_DASHxor] = ACTIONS(1),
    [anon_sym_buff_DASHto_DASHint_DASHbe] = ACTIONS(1),
    [anon_sym_buff_DASHto_DASHint_DASHle] = ACTIONS(1),
    [anon_sym_buff_DASHto_DASHuint_DASHbe] = ACTIONS(1),
    [anon_sym_buff_DASHto_DASHuint_DASHle] = ACTIONS(1),
    [anon_sym_concat] = ACTIONS(1),
    [anon_sym_contract_DASHcall_QMARK] = ACTIONS(1),
    [anon_sym_contract_DASHhash_QMARK] = ACTIONS(1),
    [anon_sym_contract_DASHof] = ACTIONS(1),
    [anon_sym_default_DASHto] = ACTIONS(1),
    [anon_sym_element_DASHat] = ACTIONS(1),
    [anon_sym_element_DASHat_QMARK] = ACTIONS(1),
    [anon_sym_filter] = ACTIONS(1),
    [anon_sym_fold] = ACTIONS(1),
    [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = ACTIONS(1),
    [anon_sym_ft_DASHburn_QMARK] = ACTIONS(1),
    [anon_sym_ft_DASHmint_QMARK] = ACTIONS(1),
    [anon_sym_ft_DASHtransfer_QMARK] = ACTIONS(1),
    [anon_sym_ft_DASHget_DASHsupply] = ACTIONS(1),
    [anon_sym_ft_DASHget_DASHbalance] = ACTIONS(1),
    [anon_sym_get] = ACTIONS(1),
    [anon_sym_get_DASHblock_DASHinfo_QMARK] = ACTIONS(1),
    [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = ACTIONS(1),
    [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = ACTIONS(1),
    [anon_sym_get_DASHtenure_DASHinfo_QMARK] = ACTIONS(1),
    [anon_sym_hash160] = ACTIONS(1),
    [anon_sym_if] = ACTIONS(1),
    [anon_sym_index_DASHof] = ACTIONS(1),
    [anon_sym_index_DASHof_QMARK] = ACTIONS(1),
    [anon_sym_int_DASHto_DASHascii] = ACTIONS(1),
    [anon_sym_int_DASHto_DASHutf8] = ACTIONS(1),
    [anon_sym_is_DASHeq] = ACTIONS(1),
    [anon_sym_is_DASHerr] = ACTIONS(1),
    [anon_sym_is_DASHnone] = ACTIONS(1),
    [anon_sym_is_DASHok] = ACTIONS(1),
    [anon_sym_is_DASHsome] = ACTIONS(1),
    [anon_sym_is_DASHstandard] = ACTIONS(1),
    [anon_sym_keccak256] = ACTIONS(1),
    [anon_sym_len] = ACTIONS(1),
    [anon_sym_log2] = ACTIONS(1),
    [anon_sym_map] = ACTIONS(1),
    [anon_sym_map_DASHdelete] = ACTIONS(1),
    [anon_sym_map_DASHget_QMARK] = ACTIONS(1),
    [anon_sym_map_DASHinsert] = ACTIONS(1),
    [anon_sym_map_DASHset] = ACTIONS(1),
    [anon_sym_match] = ACTIONS(1),
    [anon_sym_merge] = ACTIONS(1),
    [anon_sym_nft_DASHburn_QMARK] = ACTIONS(1),
    [anon_sym_nft_DASHmint_QMARK] = ACTIONS(1),
    [anon_sym_nft_DASHget_DASHowner_QMARK] = ACTIONS(1),
    [anon_sym_nft_DASHtransfer_QMARK] = ACTIONS(1),
    [anon_sym_not] = ACTIONS(1),
    [anon_sym_principal_DASHconstruct_QMARK] = ACTIONS(1),
    [anon_sym_principal_DASHdestruct_QMARK] = ACTIONS(1),
    [anon_sym_principal_DASHof_QMARK] = ACTIONS(1),
    [anon_sym_print] = ACTIONS(1),
    [anon_sym_replace_DASHat_QMARK] = ACTIONS(1),
    [anon_sym_restrict_DASHassets_QMARK] = ACTIONS(1),
    [anon_sym_secp256k1_DASHrecover_QMARK] = ACTIONS(1),
    [anon_sym_secp256k1_DASHverify] = ACTIONS(1),
    [anon_sym_secp256r1_DASHverify] = ACTIONS(1),
    [anon_sym_sha256] = ACTIONS(1),
    [anon_sym_sha512] = ACTIONS(1),
    [anon_sym_sha512_SLASH256] = ACTIONS(1),
    [anon_sym_slice_QMARK] = ACTIONS(1),
    [anon_sym_sqrti] = ACTIONS(1),
    [anon_sym_string_DASHto_DASHint_QMARK] = ACTIONS(1),
    [anon_sym_string_DASHto_DASHuint_QMARK] = ACTIONS(1),
    [anon_sym_stx_DASHaccount] = ACTIONS(1),
    [anon_sym_stx_DASHburn_QMARK] = ACTIONS(1),
    [anon_sym_stx_DASHget_DASHbalance] = ACTIONS(1),
    [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = ACTIONS(1),
    [anon_sym_stx_DASHtransfer_QMARK] = ACTIONS(1),
    [anon_sym_to_DASHascii_QMARK] = ACTIONS(1),
    [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = ACTIONS(1),
    [anon_sym_to_DASHint] = ACTIONS(1),
    [anon_sym_to_DASHuint] = ACTIONS(1),
    [anon_sym_try_BANG] = ACTIONS(1),
    [anon_sym_unwrap_BANG] = ACTIONS(1),
    [anon_sym_unwrap_DASHerr_BANG] = ACTIONS(1),
    [anon_sym_unwrap_DASHerr_DASHpanic] = ACTIONS(1),
    [anon_sym_unwrap_DASHpanic] = ACTIONS(1),
    [anon_sym_var_DASHget] = ACTIONS(1),
    [anon_sym_var_DASHset] = ACTIONS(1),
    [anon_sym_block_DASHheight] = ACTIONS(1),
    [anon_sym_burn_DASHblock_DASHheight] = ACTIONS(1),
    [anon_sym_chain_DASHid] = ACTIONS(1),
    [anon_sym_contract_DASHcaller] = ACTIONS(1),
    [anon_sym_is_DASHin_DASHmainnet] = ACTIONS(1),
    [anon_sym_is_DASHin_DASHregtest] = ACTIONS(1),
    [anon_sym_stacks_DASHblock_DASHheight] = ACTIONS(1),
    [anon_sym_stacks_DASHblock_DASHtime] = ACTIONS(1),
    [anon_sym_stx_DASHliquid_DASHsupply] = ACTIONS(1),
    [anon_sym_current_DASHcontract] = ACTIONS(1),
    [anon_sym_tenure_DASHheight] = ACTIONS(1),
    [anon_sym_tx_DASHsender] = ACTIONS(1),
    [anon_sym_tx_DASHsponsor_QMARK] = ACTIONS(1),
    [anon_sym_block_DASHreward] = ACTIONS(1),
    [anon_sym_burnchain_DASHheader_DASHhash] = ACTIONS(1),
    [anon_sym_id_DASHheader_DASHhash] = ACTIONS(1),
    [anon_sym_header_DASHhash] = ACTIONS(1),
    [anon_sym_miner_DASHaddress] = ACTIONS(1),
    [anon_sym_miner_DASHspend_DASHtotal] = ACTIONS(1),
    [anon_sym_miner_DASHspend_DASHwinner] = ACTIONS(1),
    [anon_sym_time] = ACTIONS(1),
    [anon_sym_vrf_DASHseed] = ACTIONS(1),
    [anon_sym_pox_DASHaddrs] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source] = STATE(211),
    [sym__expression] = STATE(71),
    [sym__function_call] = STATE(71),
    [sym_trait_definition] = STATE(71),
    [sym_trait_implementation] = STATE(71),
    [sym_trait_usage] = STATE(71),
    [sym_token_definition] = STATE(71),
    [sym_fungible_token_definition] = STATE(121),
    [sym_non_fungible_token_definition] = STATE(121),
    [sym_constant_definition] = STATE(71),
    [sym_variable_definition] = STATE(71),
    [sym_mapping_definition] = STATE(71),
    [sym_function_definition] = STATE(71),
    [sym_private_function] = STATE(123),
    [sym_read_only_function] = STATE(123),
    [sym_public_function] = STATE(123),
    [sym__native_function_call] = STATE(71),
    [sym_basic_native_form] = STATE(71),
    [sym_contract_function_call] = STATE(71),
    [sym_let_expression] = STATE(71),
    [aux_sym_source_repeat1] = STATE(71),
    [ts_builtin_sym_end] = ACTIONS(5),
    [anon_sym_LPAREN] = ACTIONS(7),
    [sym_comment] = ACTIONS(3),
  },
  [2] = {
    [sym_native_identifier] = STATE(8),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_define_DASHtrait] = ACTIONS(11),
    [anon_sym_impl_DASHtrait] = ACTIONS(13),
    [anon_sym_use_DASHtrait] = ACTIONS(15),
    [anon_sym_define_DASHfungible_DASHtoken] = ACTIONS(17),
    [anon_sym_define_DASHnon_DASHfungible_DASHtoken] = ACTIONS(19),
    [anon_sym_define_DASHconstant] = ACTIONS(21),
    [anon_sym_define_DASHdata_DASHvar] = ACTIONS(23),
    [anon_sym_define_DASHmap] = ACTIONS(25),
    [anon_sym_define_DASHprivate] = ACTIONS(27),
    [anon_sym_define_DASHread_DASHonly] = ACTIONS(29),
    [anon_sym_define_DASHpublic] = ACTIONS(31),
    [anon_sym_let] = ACTIONS(33),
    [anon_sym_LT] = ACTIONS(35),
    [anon_sym_GT] = ACTIONS(35),
    [anon_sym_DASH] = ACTIONS(37),
    [anon_sym_PLUS] = ACTIONS(37),
    [anon_sym_STAR] = ACTIONS(37),
    [anon_sym_SLASH] = ACTIONS(37),
    [anon_sym_mod] = ACTIONS(35),
    [anon_sym_pow] = ACTIONS(35),
    [anon_sym_LT_EQ] = ACTIONS(37),
    [anon_sym_GT_EQ] = ACTIONS(37),
    [anon_sym_and] = ACTIONS(35),
    [anon_sym_or] = ACTIONS(35),
    [anon_sym_xor] = ACTIONS(35),
    [anon_sym_append] = ACTIONS(35),
    [anon_sym_as_DASHcontract] = ACTIONS(35),
    [anon_sym_as_DASHmax_DASHlen_QMARK] = ACTIONS(35),
    [anon_sym_asserts_BANG] = ACTIONS(35),
    [anon_sym_at_DASHblock] = ACTIONS(35),
    [anon_sym_begin] = ACTIONS(35),
    [anon_sym_bit_DASHand] = ACTIONS(35),
    [anon_sym_bit_DASHnot] = ACTIONS(35),
    [anon_sym_bit_DASHor] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHleft] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHright] = ACTIONS(35),
    [anon_sym_bit_DASHxor] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHle] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHle] = ACTIONS(35),
    [anon_sym_concat] = ACTIONS(35),
    [anon_sym_contract_DASHcall_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHhash_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHof] = ACTIONS(35),
    [anon_sym_default_DASHto] = ACTIONS(35),
    [anon_sym_element_DASHat] = ACTIONS(35),
    [anon_sym_element_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_filter] = ACTIONS(35),
    [anon_sym_fold] = ACTIONS(35),
    [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHsupply] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_get] = ACTIONS(35),
    [anon_sym_get_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHtenure_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_hash160] = ACTIONS(35),
    [anon_sym_if] = ACTIONS(35),
    [anon_sym_index_DASHof] = ACTIONS(35),
    [anon_sym_index_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHascii] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHutf8] = ACTIONS(35),
    [anon_sym_is_DASHeq] = ACTIONS(35),
    [anon_sym_is_DASHerr] = ACTIONS(35),
    [anon_sym_is_DASHnone] = ACTIONS(35),
    [anon_sym_is_DASHok] = ACTIONS(35),
    [anon_sym_is_DASHsome] = ACTIONS(35),
    [anon_sym_is_DASHstandard] = ACTIONS(35),
    [anon_sym_keccak256] = ACTIONS(35),
    [anon_sym_len] = ACTIONS(35),
    [anon_sym_log2] = ACTIONS(35),
    [anon_sym_map] = ACTIONS(35),
    [anon_sym_map_DASHdelete] = ACTIONS(35),
    [anon_sym_map_DASHget_QMARK] = ACTIONS(35),
    [anon_sym_map_DASHinsert] = ACTIONS(35),
    [anon_sym_map_DASHset] = ACTIONS(35),
    [anon_sym_match] = ACTIONS(35),
    [anon_sym_merge] = ACTIONS(35),
    [anon_sym_nft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHget_DASHowner_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_not] = ACTIONS(35),
    [anon_sym_principal_DASHconstruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHdestruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_print] = ACTIONS(35),
    [anon_sym_replace_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_restrict_DASHassets_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHrecover_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHverify] = ACTIONS(35),
    [anon_sym_secp256r1_DASHverify] = ACTIONS(35),
    [anon_sym_sha256] = ACTIONS(35),
    [anon_sym_sha512] = ACTIONS(35),
    [anon_sym_sha512_SLASH256] = ACTIONS(37),
    [anon_sym_slice_QMARK] = ACTIONS(35),
    [anon_sym_sqrti] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHint_QMARK] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHuint_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHaccount] = ACTIONS(35),
    [anon_sym_stx_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHascii_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHint] = ACTIONS(35),
    [anon_sym_to_DASHuint] = ACTIONS(35),
    [anon_sym_try_BANG] = ACTIONS(35),
    [anon_sym_unwrap_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_DASHpanic] = ACTIONS(35),
    [anon_sym_unwrap_DASHpanic] = ACTIONS(35),
    [anon_sym_var_DASHget] = ACTIONS(35),
    [anon_sym_var_DASHset] = ACTIONS(35),
    [sym_comment] = ACTIONS(3),
  },
  [3] = {
    [sym_list_lit_token] = STATE(11),
    [sym_native_identifier] = STATE(8),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_let] = ACTIONS(33),
    [anon_sym_LT] = ACTIONS(35),
    [anon_sym_GT] = ACTIONS(35),
    [anon_sym_list] = ACTIONS(39),
    [anon_sym_DASH] = ACTIONS(37),
    [anon_sym_some] = ACTIONS(41),
    [anon_sym_ok] = ACTIONS(43),
    [anon_sym_err] = ACTIONS(43),
    [anon_sym_PLUS] = ACTIONS(37),
    [anon_sym_STAR] = ACTIONS(37),
    [anon_sym_SLASH] = ACTIONS(37),
    [anon_sym_mod] = ACTIONS(35),
    [anon_sym_pow] = ACTIONS(35),
    [anon_sym_LT_EQ] = ACTIONS(37),
    [anon_sym_GT_EQ] = ACTIONS(37),
    [anon_sym_and] = ACTIONS(35),
    [anon_sym_or] = ACTIONS(35),
    [anon_sym_xor] = ACTIONS(35),
    [anon_sym_append] = ACTIONS(35),
    [anon_sym_as_DASHcontract] = ACTIONS(35),
    [anon_sym_as_DASHmax_DASHlen_QMARK] = ACTIONS(35),
    [anon_sym_asserts_BANG] = ACTIONS(35),
    [anon_sym_at_DASHblock] = ACTIONS(35),
    [anon_sym_begin] = ACTIONS(35),
    [anon_sym_bit_DASHand] = ACTIONS(35),
    [anon_sym_bit_DASHnot] = ACTIONS(35),
    [anon_sym_bit_DASHor] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHleft] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHright] = ACTIONS(35),
    [anon_sym_bit_DASHxor] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHle] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHle] = ACTIONS(35),
    [anon_sym_concat] = ACTIONS(35),
    [anon_sym_contract_DASHcall_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHhash_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHof] = ACTIONS(35),
    [anon_sym_default_DASHto] = ACTIONS(35),
    [anon_sym_element_DASHat] = ACTIONS(35),
    [anon_sym_element_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_filter] = ACTIONS(35),
    [anon_sym_fold] = ACTIONS(35),
    [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHsupply] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_get] = ACTIONS(35),
    [anon_sym_get_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHtenure_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_hash160] = ACTIONS(35),
    [anon_sym_if] = ACTIONS(35),
    [anon_sym_index_DASHof] = ACTIONS(35),
    [anon_sym_index_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHascii] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHutf8] = ACTIONS(35),
    [anon_sym_is_DASHeq] = ACTIONS(35),
    [anon_sym_is_DASHerr] = ACTIONS(35),
    [anon_sym_is_DASHnone] = ACTIONS(35),
    [anon_sym_is_DASHok] = ACTIONS(35),
    [anon_sym_is_DASHsome] = ACTIONS(35),
    [anon_sym_is_DASHstandard] = ACTIONS(35),
    [anon_sym_keccak256] = ACTIONS(35),
    [anon_sym_len] = ACTIONS(35),
    [anon_sym_log2] = ACTIONS(35),
    [anon_sym_map] = ACTIONS(35),
    [anon_sym_map_DASHdelete] = ACTIONS(35),
    [anon_sym_map_DASHget_QMARK] = ACTIONS(35),
    [anon_sym_map_DASHinsert] = ACTIONS(35),
    [anon_sym_map_DASHset] = ACTIONS(35),
    [anon_sym_match] = ACTIONS(35),
    [anon_sym_merge] = ACTIONS(35),
    [anon_sym_nft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHget_DASHowner_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_not] = ACTIONS(35),
    [anon_sym_principal_DASHconstruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHdestruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_print] = ACTIONS(35),
    [anon_sym_replace_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_restrict_DASHassets_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHrecover_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHverify] = ACTIONS(35),
    [anon_sym_secp256r1_DASHverify] = ACTIONS(35),
    [anon_sym_sha256] = ACTIONS(35),
    [anon_sym_sha512] = ACTIONS(35),
    [anon_sym_sha512_SLASH256] = ACTIONS(37),
    [anon_sym_slice_QMARK] = ACTIONS(35),
    [anon_sym_sqrti] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHint_QMARK] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHuint_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHaccount] = ACTIONS(35),
    [anon_sym_stx_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHascii_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHint] = ACTIONS(35),
    [anon_sym_to_DASHuint] = ACTIONS(35),
    [anon_sym_try_BANG] = ACTIONS(35),
    [anon_sym_unwrap_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_DASHpanic] = ACTIONS(35),
    [anon_sym_unwrap_DASHpanic] = ACTIONS(35),
    [anon_sym_var_DASHget] = ACTIONS(35),
    [anon_sym_var_DASHset] = ACTIONS(35),
    [sym_comment] = ACTIONS(3),
  },
  [4] = {
    [sym_native_identifier] = STATE(8),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_let] = ACTIONS(33),
    [anon_sym_LT] = ACTIONS(35),
    [anon_sym_GT] = ACTIONS(35),
    [anon_sym_DASH] = ACTIONS(37),
    [anon_sym_ok] = ACTIONS(43),
    [anon_sym_err] = ACTIONS(43),
    [anon_sym_PLUS] = ACTIONS(37),
    [anon_sym_STAR] = ACTIONS(37),
    [anon_sym_SLASH] = ACTIONS(37),
    [anon_sym_mod] = ACTIONS(35),
    [anon_sym_pow] = ACTIONS(35),
    [anon_sym_LT_EQ] = ACTIONS(37),
    [anon_sym_GT_EQ] = ACTIONS(37),
    [anon_sym_and] = ACTIONS(35),
    [anon_sym_or] = ACTIONS(35),
    [anon_sym_xor] = ACTIONS(35),
    [anon_sym_append] = ACTIONS(35),
    [anon_sym_as_DASHcontract] = ACTIONS(35),
    [anon_sym_as_DASHmax_DASHlen_QMARK] = ACTIONS(35),
    [anon_sym_asserts_BANG] = ACTIONS(35),
    [anon_sym_at_DASHblock] = ACTIONS(35),
    [anon_sym_begin] = ACTIONS(35),
    [anon_sym_bit_DASHand] = ACTIONS(35),
    [anon_sym_bit_DASHnot] = ACTIONS(35),
    [anon_sym_bit_DASHor] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHleft] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHright] = ACTIONS(35),
    [anon_sym_bit_DASHxor] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHle] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHle] = ACTIONS(35),
    [anon_sym_concat] = ACTIONS(35),
    [anon_sym_contract_DASHcall_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHhash_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHof] = ACTIONS(35),
    [anon_sym_default_DASHto] = ACTIONS(35),
    [anon_sym_element_DASHat] = ACTIONS(35),
    [anon_sym_element_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_filter] = ACTIONS(35),
    [anon_sym_fold] = ACTIONS(35),
    [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHsupply] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_get] = ACTIONS(35),
    [anon_sym_get_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHtenure_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_hash160] = ACTIONS(35),
    [anon_sym_if] = ACTIONS(35),
    [anon_sym_index_DASHof] = ACTIONS(35),
    [anon_sym_index_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHascii] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHutf8] = ACTIONS(35),
    [anon_sym_is_DASHeq] = ACTIONS(35),
    [anon_sym_is_DASHerr] = ACTIONS(35),
    [anon_sym_is_DASHnone] = ACTIONS(35),
    [anon_sym_is_DASHok] = ACTIONS(35),
    [anon_sym_is_DASHsome] = ACTIONS(35),
    [anon_sym_is_DASHstandard] = ACTIONS(35),
    [anon_sym_keccak256] = ACTIONS(35),
    [anon_sym_len] = ACTIONS(35),
    [anon_sym_log2] = ACTIONS(35),
    [anon_sym_map] = ACTIONS(35),
    [anon_sym_map_DASHdelete] = ACTIONS(35),
    [anon_sym_map_DASHget_QMARK] = ACTIONS(35),
    [anon_sym_map_DASHinsert] = ACTIONS(35),
    [anon_sym_map_DASHset] = ACTIONS(35),
    [anon_sym_match] = ACTIONS(35),
    [anon_sym_merge] = ACTIONS(35),
    [anon_sym_nft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHget_DASHowner_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_not] = ACTIONS(35),
    [anon_sym_principal_DASHconstruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHdestruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_print] = ACTIONS(35),
    [anon_sym_replace_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_restrict_DASHassets_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHrecover_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHverify] = ACTIONS(35),
    [anon_sym_secp256r1_DASHverify] = ACTIONS(35),
    [anon_sym_sha256] = ACTIONS(35),
    [anon_sym_sha512] = ACTIONS(35),
    [anon_sym_sha512_SLASH256] = ACTIONS(37),
    [anon_sym_slice_QMARK] = ACTIONS(35),
    [anon_sym_sqrti] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHint_QMARK] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHuint_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHaccount] = ACTIONS(35),
    [anon_sym_stx_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHascii_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHint] = ACTIONS(35),
    [anon_sym_to_DASHuint] = ACTIONS(35),
    [anon_sym_try_BANG] = ACTIONS(35),
    [anon_sym_unwrap_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_DASHpanic] = ACTIONS(35),
    [anon_sym_unwrap_DASHpanic] = ACTIONS(35),
    [anon_sym_var_DASHget] = ACTIONS(35),
    [anon_sym_var_DASHset] = ACTIONS(35),
    [sym_comment] = ACTIONS(3),
  },
  [5] = {
    [sym_native_identifier] = STATE(8),
    [sym_identifier] = ACTIONS(9),
    [anon_sym_let] = ACTIONS(33),
    [anon_sym_LT] = ACTIONS(35),
    [anon_sym_GT] = ACTIONS(35),
    [anon_sym_DASH] = ACTIONS(37),
    [anon_sym_PLUS] = ACTIONS(37),
    [anon_sym_STAR] = ACTIONS(37),
    [anon_sym_SLASH] = ACTIONS(37),
    [anon_sym_mod] = ACTIONS(35),
    [anon_sym_pow] = ACTIONS(35),
    [anon_sym_LT_EQ] = ACTIONS(37),
    [anon_sym_GT_EQ] = ACTIONS(37),
    [anon_sym_and] = ACTIONS(35),
    [anon_sym_or] = ACTIONS(35),
    [anon_sym_xor] = ACTIONS(35),
    [anon_sym_append] = ACTIONS(35),
    [anon_sym_as_DASHcontract] = ACTIONS(35),
    [anon_sym_as_DASHmax_DASHlen_QMARK] = ACTIONS(35),
    [anon_sym_asserts_BANG] = ACTIONS(35),
    [anon_sym_at_DASHblock] = ACTIONS(35),
    [anon_sym_begin] = ACTIONS(35),
    [anon_sym_bit_DASHand] = ACTIONS(35),
    [anon_sym_bit_DASHnot] = ACTIONS(35),
    [anon_sym_bit_DASHor] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHleft] = ACTIONS(35),
    [anon_sym_bit_DASHshift_DASHright] = ACTIONS(35),
    [anon_sym_bit_DASHxor] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHint_DASHle] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHbe] = ACTIONS(35),
    [anon_sym_buff_DASHto_DASHuint_DASHle] = ACTIONS(35),
    [anon_sym_concat] = ACTIONS(35),
    [anon_sym_contract_DASHcall_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHhash_QMARK] = ACTIONS(35),
    [anon_sym_contract_DASHof] = ACTIONS(35),
    [anon_sym_default_DASHto] = ACTIONS(35),
    [anon_sym_element_DASHat] = ACTIONS(35),
    [anon_sym_element_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_filter] = ACTIONS(35),
    [anon_sym_fold] = ACTIONS(35),
    [anon_sym_from_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHsupply] = ACTIONS(35),
    [anon_sym_ft_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_get] = ACTIONS(35),
    [anon_sym_get_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHburn_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHstacks_DASHblock_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_get_DASHtenure_DASHinfo_QMARK] = ACTIONS(35),
    [anon_sym_hash160] = ACTIONS(35),
    [anon_sym_if] = ACTIONS(35),
    [anon_sym_index_DASHof] = ACTIONS(35),
    [anon_sym_index_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHascii] = ACTIONS(35),
    [anon_sym_int_DASHto_DASHutf8] = ACTIONS(35),
    [anon_sym_is_DASHeq] = ACTIONS(35),
    [anon_sym_is_DASHerr] = ACTIONS(35),
    [anon_sym_is_DASHnone] = ACTIONS(35),
    [anon_sym_is_DASHok] = ACTIONS(35),
    [anon_sym_is_DASHsome] = ACTIONS(35),
    [anon_sym_is_DASHstandard] = ACTIONS(35),
    [anon_sym_keccak256] = ACTIONS(35),
    [anon_sym_len] = ACTIONS(35),
    [anon_sym_log2] = ACTIONS(35),
    [anon_sym_map] = ACTIONS(35),
    [anon_sym_map_DASHdelete] = ACTIONS(35),
    [anon_sym_map_DASHget_QMARK] = ACTIONS(35),
    [anon_sym_map_DASHinsert] = ACTIONS(35),
    [anon_sym_map_DASHset] = ACTIONS(35),
    [anon_sym_match] = ACTIONS(35),
    [anon_sym_merge] = ACTIONS(35),
    [anon_sym_nft_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHmint_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHget_DASHowner_QMARK] = ACTIONS(35),
    [anon_sym_nft_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_not] = ACTIONS(35),
    [anon_sym_principal_DASHconstruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHdestruct_QMARK] = ACTIONS(35),
    [anon_sym_principal_DASHof_QMARK] = ACTIONS(35),
    [anon_sym_print] = ACTIONS(35),
    [anon_sym_replace_DASHat_QMARK] = ACTIONS(35),
    [anon_sym_restrict_DASHassets_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHrecover_QMARK] = ACTIONS(35),
    [anon_sym_secp256k1_DASHverify] = ACTIONS(35),
    [anon_sym_secp256r1_DASHverify] = ACTIONS(35),
    [anon_sym_sha256] = ACTIONS(35),
    [anon_sym_sha512] = ACTIONS(35),
    [anon_sym_sha512_SLASH256] = ACTIONS(37),
    [anon_sym_slice_QMARK] = ACTIONS(35),
    [anon_sym_sqrti] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHint_QMARK] = ACTIONS(35),
    [anon_sym_string_DASHto_DASHuint_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHaccount] = ACTIONS(35),
    [anon_sym_stx_DASHburn_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHget_DASHbalance] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_DASHmemo_QMARK] = ACTIONS(35),
    [anon_sym_stx_DASHtransfer_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHascii_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHconsensus_DASHbuff_QMARK] = ACTIONS(35),
    [anon_sym_to_DASHint] = ACTIONS(35),
    [anon_sym_to_DASHuint] = ACTIONS(35),
    [anon_sym_try_BANG] = ACTIONS(35),
    [anon_sym_unwrap_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_BANG] = ACTIONS(35),
    [anon_sym_unwrap_DASHerr_DASHpanic] = ACTIONS(35),
    [anon_sym_unwrap_DASHpanic] = ACTIONS(35),
    [anon_sym_var_DASHget] = ACTIONS(35),
    [anon_sym_var_DASHset] = ACTIONS(35),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(49), 1,
      anon_sym_RPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(67), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(45), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(9), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [91] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(67), 1,
      sym_utf8_string_lit,
    ACTIONS(71), 1,
      anon_sym_RPAREN,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(45), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(9), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [182] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(75), 1,
      anon_sym_RPAREN,
    ACTIONS(77), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(73), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(6), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [273] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(82), 1,
      anon_sym_LPAREN,
    ACTIONS(85), 1,
      anon_sym_RPAREN,
    ACTIONS(87), 1,
      anon_sym_DOT,
    ACTIONS(90), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(93), 1,
      anon_sym_LBRACE,
    ACTIONS(96), 1,
      anon_sym_DASH,
    ACTIONS(102), 1,
      anon_sym_SQUOTE,
    ACTIONS(105), 1,
      anon_sym_0x,
    ACTIONS(108), 1,
      anon_sym_DQUOTE,
    ACTIONS(111), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(99), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(79), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(9), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(114), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [364] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(119), 1,
      anon_sym_RPAREN,
    ACTIONS(121), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(117), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(12), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [455] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(125), 1,
      anon_sym_RPAREN,
    ACTIONS(127), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(123), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(7), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [546] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(67), 1,
      sym_utf8_string_lit,
    ACTIONS(129), 1,
      anon_sym_RPAREN,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(45), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(9), 18,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
      aux_sym_basic_native_form_repeat1,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [637] = 17,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(133), 1,
      sym_utf8_string_lit,
    STATE(16), 1,
      aux_sym_let_expression_repeat2,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(131), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(67), 5,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
    STATE(210), 12,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [729] = 17,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(137), 1,
      sym_utf8_string_lit,
    STATE(38), 1,
      aux_sym_let_expression_repeat2,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(135), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(67), 5,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
    STATE(193), 12,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [821] = 17,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(141), 1,
      sym_utf8_string_lit,
    STATE(14), 1,
      aux_sym_let_expression_repeat2,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(139), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(67), 5,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
    STATE(160), 12,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [913] = 17,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(141), 1,
      sym_utf8_string_lit,
    STATE(38), 1,
      aux_sym_let_expression_repeat2,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(139), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(67), 5,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
    STATE(160), 12,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1005] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(145), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(143), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(148), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1092] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(149), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(147), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(201), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1179] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(153), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(151), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(178), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1266] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(157), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(155), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(171), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1353] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(161), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(159), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(158), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1440] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(165), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(163), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(186), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1527] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(169), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(167), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(139), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1614] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_LPAREN,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(53), 1,
      aux_sym_buffer_type_token1,
    ACTIONS(55), 1,
      anon_sym_LBRACE,
    ACTIONS(57), 1,
      anon_sym_DASH,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    ACTIONS(63), 1,
      anon_sym_0x,
    ACTIONS(65), 1,
      anon_sym_DQUOTE,
    ACTIONS(173), 1,
      sym_utf8_string_lit,
    STATE(62), 1,
      sym_standard_principal_lit,
    ACTIONS(59), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(171), 3,
      sym_uint_lit,
      sym_none_lit,
      sym_identifier,
    STATE(202), 17,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym__parameter,
      sym__literal,
      sym_int_lit,
      sym_bool_lit,
      sym_contract_principal_lit,
      sym_buffer_lit,
      sym_ascii_string_lit,
      sym_list_lit,
      sym_some_lit,
      sym_tuple_lit,
      sym_response_lit,
      sym_global,
    ACTIONS(69), 23,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
  [1701] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(177), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(175), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [1754] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(181), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(179), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [1807] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(185), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(183), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [1860] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(189), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(187), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [1913] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(193), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(191), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [1966] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(197), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(195), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2019] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(201), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(199), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2072] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(205), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(203), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2125] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(209), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(207), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2178] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(213), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(211), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2231] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(217), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(215), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2284] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(221), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(219), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2337] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(225), 12,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(223), 33,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2390] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(229), 1,
      anon_sym_LPAREN,
    STATE(38), 6,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      aux_sym_let_expression_repeat2,
    ACTIONS(232), 7,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(227), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2445] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(234), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(236), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2494] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(238), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(240), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2543] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(244), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2592] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(246), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(248), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2641] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(250), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(252), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2690] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(254), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(256), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2739] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(258), 12,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(260), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2788] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(264), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(262), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2836] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(268), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(266), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2884] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(272), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(270), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2932] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(276), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(274), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [2980] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(280), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(278), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3028] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(284), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(282), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3076] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(288), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(286), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3124] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(292), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(290), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3172] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(296), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(294), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3220] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(300), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(298), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3268] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(304), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(302), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3316] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(308), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(306), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3364] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(312), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(310), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3412] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(314), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3460] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(320), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(318), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3508] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(324), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(322), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3556] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_DOT,
    ACTIONS(328), 10,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(326), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3606] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(334), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(332), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3654] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(338), 11,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(336), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3702] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(342), 9,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(340), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3748] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(346), 9,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(344), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3794] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 1,
      anon_sym_RPAREN,
    ACTIONS(350), 8,
      anon_sym_LPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(348), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3842] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(356), 8,
      anon_sym_LPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(354), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3887] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(360), 8,
      anon_sym_LPAREN,
      anon_sym_DOT,
      anon_sym_LBRACE,
      anon_sym_DASH,
      anon_sym_SQUOTE,
      anon_sym_0x,
      anon_sym_DQUOTE,
      sym_utf8_string_lit,
    ACTIONS(358), 29,
      aux_sym_buffer_type_token1,
      sym_uint_lit,
      anon_sym_true,
      anon_sym_false,
      sym_none_lit,
      anon_sym_block_DASHheight,
      anon_sym_burn_DASHblock_DASHheight,
      anon_sym_chain_DASHid,
      anon_sym_contract_DASHcaller,
      anon_sym_is_DASHin_DASHmainnet,
      anon_sym_is_DASHin_DASHregtest,
      anon_sym_stacks_DASHblock_DASHheight,
      anon_sym_stacks_DASHblock_DASHtime,
      anon_sym_stx_DASHliquid_DASHsupply,
      anon_sym_current_DASHcontract,
      anon_sym_tenure_DASHheight,
      anon_sym_tx_DASHsender,
      anon_sym_tx_DASHsponsor_QMARK,
      anon_sym_block_DASHreward,
      anon_sym_burnchain_DASHheader_DASHhash,
      anon_sym_id_DASHheader_DASHhash,
      anon_sym_header_DASHhash,
      anon_sym_miner_DASHaddress,
      anon_sym_miner_DASHspend_DASHtotal,
      anon_sym_miner_DASHspend_DASHwinner,
      anon_sym_time,
      anon_sym_vrf_DASHseed,
      anon_sym_pox_DASHaddrs,
      sym_identifier,
  [3932] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(362), 1,
      ts_builtin_sym_end,
    ACTIONS(364), 1,
      anon_sym_LPAREN,
    STATE(121), 2,
      sym_fungible_token_definition,
      sym_non_fungible_token_definition,
    STATE(123), 3,
      sym_private_function,
      sym_read_only_function,
      sym_public_function,
    STATE(70), 15,
      sym__expression,
      sym__function_call,
      sym_trait_definition,
      sym_trait_implementation,
      sym_trait_usage,
      sym_token_definition,
      sym_constant_definition,
      sym_variable_definition,
      sym_mapping_definition,
      sym_function_definition,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      aux_sym_source_repeat1,
  [3968] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(7), 1,
      anon_sym_LPAREN,
    ACTIONS(367), 1,
      ts_builtin_sym_end,
    STATE(121), 2,
      sym_fungible_token_definition,
      sym_non_fungible_token_definition,
    STATE(123), 3,
      sym_private_function,
      sym_read_only_function,
      sym_public_function,
    STATE(70), 15,
      sym__expression,
      sym__function_call,
      sym_trait_definition,
      sym_trait_implementation,
      sym_trait_usage,
      sym_token_definition,
      sym_constant_definition,
      sym_variable_definition,
      sym_mapping_definition,
      sym_function_definition,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      aux_sym_source_repeat1,
  [4004] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(371), 1,
      anon_sym_RPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(73), 2,
      sym_parameter_type,
      aux_sym_function_signature_for_trait_repeat1,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4044] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    ACTIONS(379), 1,
      anon_sym_RPAREN,
    STATE(74), 2,
      sym_parameter_type,
      aux_sym_function_signature_for_trait_repeat1,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4084] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(381), 1,
      anon_sym_LPAREN,
    ACTIONS(384), 1,
      anon_sym_RPAREN,
    ACTIONS(386), 1,
      anon_sym_LT,
    ACTIONS(392), 1,
      anon_sym_LBRACE,
    STATE(74), 2,
      sym_parameter_type,
      aux_sym_function_signature_for_trait_repeat1,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(389), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4124] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(177), 1,
      sym_parameter_type,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4160] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(131), 1,
      sym_parameter_type,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4196] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(189), 1,
      sym_parameter_type,
    STATE(89), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4232] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(373), 1,
      anon_sym_LT,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(200), 2,
      sym_trait_type,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4265] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(212), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4294] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(183), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4323] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(22), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4352] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(206), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4381] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(164), 1,
      sym_tuple_type,
    STATE(167), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 7,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_response_type,
  [4412] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(80), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4441] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(180), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4470] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(164), 1,
      sym_tuple_type,
    STATE(167), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 7,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_response_type,
  [4501] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(182), 1,
      sym_native_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 8,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_tuple_type,
      sym_response_type,
  [4530] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 1,
      anon_sym_LPAREN,
    ACTIONS(377), 1,
      anon_sym_LBRACE,
    STATE(83), 1,
      sym_native_type,
    STATE(86), 1,
      sym_tuple_type,
    ACTIONS(375), 4,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
    STATE(32), 7,
      sym_buffer_type,
      sym_ascii_string_type,
      sym_utf8_string_type,
      sym_list_type,
      sym_optional_type,
      sym_tuple_type_for_trait,
      sym_response_type,
  [4561] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(395), 10,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [4577] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(397), 10,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
      anon_sym_LT,
      anon_sym_int,
      anon_sym_uint,
      anon_sym_bool,
      anon_sym_principal,
      anon_sym_LBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [4593] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(399), 1,
      anon_sym_LPAREN,
    STATE(170), 6,
      sym__function_call,
      sym__native_function_call,
      sym_basic_native_form,
      sym_contract_function_call,
      sym_let_expression,
      sym_response_lit,
  [4608] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(401), 1,
      anon_sym_buff,
    ACTIONS(403), 1,
      anon_sym_string_DASHascii,
    ACTIONS(405), 1,
      anon_sym_string_DASHutf8,
    ACTIONS(407), 1,
      anon_sym_list,
    ACTIONS(409), 1,
      anon_sym_optional,
    ACTIONS(411), 1,
      anon_sym_tuple,
    ACTIONS(413), 1,
      anon_sym_response,
  [4633] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(415), 1,
      anon_sym_LPAREN,
    ACTIONS(417), 1,
      anon_sym_RPAREN,
    STATE(95), 2,
      sym_function_signature_for_trait,
      aux_sym_trait_definition_repeat1,
  [4647] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(419), 1,
      anon_sym_LPAREN,
    ACTIONS(422), 1,
      anon_sym_RPAREN,
    STATE(94), 2,
      sym_function_parameter,
      aux_sym_function_signature_repeat1,
  [4661] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(415), 1,
      anon_sym_LPAREN,
    ACTIONS(424), 1,
      anon_sym_RPAREN,
    STATE(102), 2,
      sym_function_signature_for_trait,
      aux_sym_trait_definition_repeat1,
  [4675] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(426), 1,
      anon_sym_LPAREN,
    ACTIONS(428), 1,
      anon_sym_RPAREN,
    STATE(101), 2,
      sym_local_binding,
      aux_sym_let_expression_repeat1,
  [4689] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(430), 1,
      anon_sym_LPAREN,
    ACTIONS(432), 1,
      anon_sym_RPAREN,
    STATE(100), 2,
      sym_function_parameter,
      aux_sym_function_signature_repeat1,
  [4703] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(434), 1,
      anon_sym_LPAREN,
    ACTIONS(437), 1,
      anon_sym_RPAREN,
    STATE(98), 2,
      sym_local_binding,
      aux_sym_let_expression_repeat1,
  [4717] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    STATE(204), 1,
      sym_standard_principal_lit,
    STATE(205), 1,
      sym_contract_principal_lit,
  [4733] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(430), 1,
      anon_sym_LPAREN,
    ACTIONS(439), 1,
      anon_sym_RPAREN,
    STATE(94), 2,
      sym_function_parameter,
      aux_sym_function_signature_repeat1,
  [4747] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(426), 1,
      anon_sym_LPAREN,
    ACTIONS(441), 1,
      anon_sym_RPAREN,
    STATE(98), 2,
      sym_local_binding,
      aux_sym_let_expression_repeat1,
  [4761] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(443), 1,
      anon_sym_LPAREN,
    ACTIONS(446), 1,
      anon_sym_RPAREN,
    STATE(102), 2,
      sym_function_signature_for_trait,
      aux_sym_trait_definition_repeat1,
  [4775] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_DOT,
    ACTIONS(61), 1,
      anon_sym_SQUOTE,
    STATE(194), 1,
      sym_contract_principal_lit,
    STATE(204), 1,
      sym_standard_principal_lit,
  [4791] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(448), 1,
      anon_sym_LPAREN,
    ACTIONS(451), 1,
      anon_sym_RPAREN,
    STATE(104), 1,
      aux_sym_tuple_type_for_trait_repeat1,
  [4804] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(453), 1,
      anon_sym_LPAREN,
    ACTIONS(455), 1,
      anon_sym_RPAREN,
    STATE(104), 1,
      aux_sym_tuple_type_for_trait_repeat1,
  [4817] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(453), 1,
      anon_sym_LPAREN,
    ACTIONS(457), 1,
      anon_sym_RPAREN,
    STATE(105), 1,
      aux_sym_tuple_type_for_trait_repeat1,
  [4830] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(459), 1,
      sym_identifier,
    STATE(107), 1,
      aux_sym_tuple_lit_repeat1,
    STATE(169), 1,
      sym__tuple_lit_pair,
  [4843] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(462), 1,
      sym_identifier,
    STATE(110), 1,
      aux_sym_tuple_type_repeat1,
    STATE(144), 1,
      sym__tuple_type_pair,
  [4856] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(462), 1,
      sym_identifier,
    STATE(108), 1,
      aux_sym_tuple_type_repeat1,
    STATE(145), 1,
      sym__tuple_type_pair,
  [4869] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(464), 1,
      sym_identifier,
    STATE(110), 1,
      aux_sym_tuple_type_repeat1,
    STATE(188), 1,
      sym__tuple_type_pair,
  [4882] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(467), 1,
      sym_identifier,
    STATE(112), 1,
      aux_sym_tuple_lit_repeat1,
    STATE(140), 1,
      sym__tuple_lit_pair,
  [4895] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(467), 1,
      sym_identifier,
    STATE(107), 1,
      aux_sym_tuple_lit_repeat1,
    STATE(129), 1,
      sym__tuple_lit_pair,
  [4908] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(469), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4916] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(471), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4924] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(473), 2,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
  [4932] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(475), 2,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
  [4940] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(477), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4948] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(479), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4956] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(481), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4964] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(483), 2,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
  [4972] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(485), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4980] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(487), 2,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
  [4988] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(489), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [4996] = 3,
    ACTIONS(491), 1,
      anon_sym_DQUOTE,
    ACTIONS(493), 1,
      aux_sym_ascii_string_lit_token1,
    ACTIONS(495), 1,
      sym_comment,
  [5006] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(497), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5014] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(499), 1,
      aux_sym_standard_principal_lit_token1,
    ACTIONS(501), 1,
      aux_sym_standard_principal_lit_token2,
  [5024] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(503), 2,
      anon_sym_LPAREN,
      anon_sym_RPAREN,
  [5032] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(505), 1,
      sym_identifier,
    ACTIONS(507), 1,
      anon_sym_RBRACE,
  [5042] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(509), 1,
      anon_sym_COMMA,
    ACTIONS(511), 1,
      anon_sym_RBRACE,
  [5052] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(513), 1,
      sym_identifier,
    ACTIONS(515), 1,
      anon_sym_RBRACE,
  [5062] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(517), 2,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [5070] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(519), 1,
      anon_sym_LPAREN,
    STATE(19), 1,
      sym_function_signature,
  [5080] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(521), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5088] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(519), 1,
      anon_sym_LPAREN,
    STATE(20), 1,
      sym_function_signature,
  [5098] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(523), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5106] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(525), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5114] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(527), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5122] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(505), 1,
      sym_identifier,
    ACTIONS(529), 1,
      anon_sym_RBRACE,
  [5132] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(531), 2,
      anon_sym_COMMA,
      anon_sym_RBRACE,
  [5140] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(533), 1,
      anon_sym_COMMA,
    ACTIONS(535), 1,
      anon_sym_RBRACE,
  [5150] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(537), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5158] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(513), 1,
      sym_identifier,
    ACTIONS(539), 1,
      anon_sym_RBRACE,
  [5168] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(541), 2,
      ts_builtin_sym_end,
      anon_sym_LPAREN,
  [5176] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(543), 1,
      anon_sym_COMMA,
    ACTIONS(545), 1,
      anon_sym_RBRACE,
  [5186] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(547), 1,
      anon_sym_COMMA,
    ACTIONS(549), 1,
      anon_sym_RBRACE,
  [5196] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(519), 1,
      anon_sym_LPAREN,
    STATE(91), 1,
      sym_function_signature,
  [5206] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(551), 1,
      anon_sym_RPAREN,
  [5213] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(553), 1,
      anon_sym_RPAREN,
  [5220] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(555), 1,
      sym_identifier,
  [5227] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(557), 1,
      anon_sym_LPAREN,
  [5234] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(559), 1,
      anon_sym_COLON,
  [5241] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(561), 1,
      aux_sym_buffer_type_token1,
  [5248] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(563), 1,
      aux_sym_buffer_type_token1,
  [5255] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(565), 1,
      aux_sym_buffer_type_token1,
  [5262] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(567), 1,
      aux_sym_buffer_type_token1,
  [5269] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(569), 1,
      sym_identifier,
  [5276] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(571), 1,
      anon_sym_RPAREN,
  [5283] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(573), 1,
      anon_sym_RPAREN,
  [5290] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(575), 1,
      anon_sym_RPAREN,
  [5297] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(577), 1,
      anon_sym_RPAREN,
  [5304] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(579), 1,
      anon_sym_LPAREN,
  [5311] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(581), 1,
      sym_identifier,
  [5318] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(583), 1,
      anon_sym_DQUOTE,
  [5325] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(585), 1,
      anon_sym_RPAREN,
  [5332] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(587), 1,
      anon_sym_RPAREN,
  [5339] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(589), 1,
      anon_sym_COLON,
  [5346] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(585), 1,
      anon_sym_RPAREN,
  [5353] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(591), 1,
      sym_identifier,
  [5360] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(593), 1,
      anon_sym_COMMA,
  [5367] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(595), 1,
      anon_sym_RPAREN,
  [5374] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(597), 1,
      anon_sym_RPAREN,
  [5381] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(599), 1,
      anon_sym_RPAREN,
  [5388] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(601), 1,
      anon_sym_RPAREN,
  [5395] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(603), 1,
      anon_sym_RPAREN,
  [5402] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(605), 1,
      sym_identifier,
  [5409] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(607), 1,
      sym_identifier,
  [5416] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(609), 1,
      anon_sym_RPAREN,
  [5423] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(611), 1,
      anon_sym_RPAREN,
  [5430] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(613), 1,
      sym_identifier,
  [5437] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(615), 1,
      anon_sym_RPAREN,
  [5444] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(617), 1,
      sym_identifier,
  [5451] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(619), 1,
      anon_sym_RPAREN,
  [5458] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(621), 1,
      anon_sym_RPAREN,
  [5465] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(623), 1,
      anon_sym_LPAREN,
  [5472] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(625), 1,
      sym_identifier,
  [5479] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(627), 1,
      anon_sym_RPAREN,
  [5486] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(629), 1,
      sym_identifier,
  [5493] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(631), 1,
      anon_sym_COMMA,
  [5500] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(633), 1,
      anon_sym_RPAREN,
  [5507] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(635), 1,
      anon_sym_RPAREN,
  [5514] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(637), 1,
      sym_identifier,
  [5521] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(639), 1,
      sym_identifier,
  [5528] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(641), 1,
      anon_sym_RPAREN,
  [5535] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(643), 1,
      anon_sym_DOT,
  [5542] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(505), 1,
      sym_identifier,
  [5549] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(645), 1,
      sym_identifier,
  [5556] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(647), 1,
      sym_identifier,
  [5563] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(649), 1,
      anon_sym_GT,
  [5570] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(651), 1,
      sym_identifier,
  [5577] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(653), 1,
      anon_sym_RPAREN,
  [5584] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(655), 1,
      anon_sym_RPAREN,
  [5591] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(657), 1,
      anon_sym_RPAREN,
  [5598] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(513), 1,
      sym_identifier,
  [5605] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_DOT,
  [5612] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(659), 1,
      anon_sym_DOT,
  [5619] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(661), 1,
      anon_sym_RPAREN,
  [5626] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(663), 1,
      aux_sym_buffer_lit_token1,
  [5633] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(665), 1,
      aux_sym_buffer_type_token1,
  [5640] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(667), 1,
      sym_identifier,
  [5647] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(669), 1,
      anon_sym_RPAREN,
  [5654] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(671), 1,
      ts_builtin_sym_end,
  [5661] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(673), 1,
      anon_sym_RPAREN,
  [5668] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(675), 1,
      sym_identifier,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(6)] = 0,
  [SMALL_STATE(7)] = 91,
  [SMALL_STATE(8)] = 182,
  [SMALL_STATE(9)] = 273,
  [SMALL_STATE(10)] = 364,
  [SMALL_STATE(11)] = 455,
  [SMALL_STATE(12)] = 546,
  [SMALL_STATE(13)] = 637,
  [SMALL_STATE(14)] = 729,
  [SMALL_STATE(15)] = 821,
  [SMALL_STATE(16)] = 913,
  [SMALL_STATE(17)] = 1005,
  [SMALL_STATE(18)] = 1092,
  [SMALL_STATE(19)] = 1179,
  [SMALL_STATE(20)] = 1266,
  [SMALL_STATE(21)] = 1353,
  [SMALL_STATE(22)] = 1440,
  [SMALL_STATE(23)] = 1527,
  [SMALL_STATE(24)] = 1614,
  [SMALL_STATE(25)] = 1701,
  [SMALL_STATE(26)] = 1754,
  [SMALL_STATE(27)] = 1807,
  [SMALL_STATE(28)] = 1860,
  [SMALL_STATE(29)] = 1913,
  [SMALL_STATE(30)] = 1966,
  [SMALL_STATE(31)] = 2019,
  [SMALL_STATE(32)] = 2072,
  [SMALL_STATE(33)] = 2125,
  [SMALL_STATE(34)] = 2178,
  [SMALL_STATE(35)] = 2231,
  [SMALL_STATE(36)] = 2284,
  [SMALL_STATE(37)] = 2337,
  [SMALL_STATE(38)] = 2390,
  [SMALL_STATE(39)] = 2445,
  [SMALL_STATE(40)] = 2494,
  [SMALL_STATE(41)] = 2543,
  [SMALL_STATE(42)] = 2592,
  [SMALL_STATE(43)] = 2641,
  [SMALL_STATE(44)] = 2690,
  [SMALL_STATE(45)] = 2739,
  [SMALL_STATE(46)] = 2788,
  [SMALL_STATE(47)] = 2836,
  [SMALL_STATE(48)] = 2884,
  [SMALL_STATE(49)] = 2932,
  [SMALL_STATE(50)] = 2980,
  [SMALL_STATE(51)] = 3028,
  [SMALL_STATE(52)] = 3076,
  [SMALL_STATE(53)] = 3124,
  [SMALL_STATE(54)] = 3172,
  [SMALL_STATE(55)] = 3220,
  [SMALL_STATE(56)] = 3268,
  [SMALL_STATE(57)] = 3316,
  [SMALL_STATE(58)] = 3364,
  [SMALL_STATE(59)] = 3412,
  [SMALL_STATE(60)] = 3460,
  [SMALL_STATE(61)] = 3508,
  [SMALL_STATE(62)] = 3556,
  [SMALL_STATE(63)] = 3606,
  [SMALL_STATE(64)] = 3654,
  [SMALL_STATE(65)] = 3702,
  [SMALL_STATE(66)] = 3748,
  [SMALL_STATE(67)] = 3794,
  [SMALL_STATE(68)] = 3842,
  [SMALL_STATE(69)] = 3887,
  [SMALL_STATE(70)] = 3932,
  [SMALL_STATE(71)] = 3968,
  [SMALL_STATE(72)] = 4004,
  [SMALL_STATE(73)] = 4044,
  [SMALL_STATE(74)] = 4084,
  [SMALL_STATE(75)] = 4124,
  [SMALL_STATE(76)] = 4160,
  [SMALL_STATE(77)] = 4196,
  [SMALL_STATE(78)] = 4232,
  [SMALL_STATE(79)] = 4265,
  [SMALL_STATE(80)] = 4294,
  [SMALL_STATE(81)] = 4323,
  [SMALL_STATE(82)] = 4352,
  [SMALL_STATE(83)] = 4381,
  [SMALL_STATE(84)] = 4412,
  [SMALL_STATE(85)] = 4441,
  [SMALL_STATE(86)] = 4470,
  [SMALL_STATE(87)] = 4501,
  [SMALL_STATE(88)] = 4530,
  [SMALL_STATE(89)] = 4561,
  [SMALL_STATE(90)] = 4577,
  [SMALL_STATE(91)] = 4593,
  [SMALL_STATE(92)] = 4608,
  [SMALL_STATE(93)] = 4633,
  [SMALL_STATE(94)] = 4647,
  [SMALL_STATE(95)] = 4661,
  [SMALL_STATE(96)] = 4675,
  [SMALL_STATE(97)] = 4689,
  [SMALL_STATE(98)] = 4703,
  [SMALL_STATE(99)] = 4717,
  [SMALL_STATE(100)] = 4733,
  [SMALL_STATE(101)] = 4747,
  [SMALL_STATE(102)] = 4761,
  [SMALL_STATE(103)] = 4775,
  [SMALL_STATE(104)] = 4791,
  [SMALL_STATE(105)] = 4804,
  [SMALL_STATE(106)] = 4817,
  [SMALL_STATE(107)] = 4830,
  [SMALL_STATE(108)] = 4843,
  [SMALL_STATE(109)] = 4856,
  [SMALL_STATE(110)] = 4869,
  [SMALL_STATE(111)] = 4882,
  [SMALL_STATE(112)] = 4895,
  [SMALL_STATE(113)] = 4908,
  [SMALL_STATE(114)] = 4916,
  [SMALL_STATE(115)] = 4924,
  [SMALL_STATE(116)] = 4932,
  [SMALL_STATE(117)] = 4940,
  [SMALL_STATE(118)] = 4948,
  [SMALL_STATE(119)] = 4956,
  [SMALL_STATE(120)] = 4964,
  [SMALL_STATE(121)] = 4972,
  [SMALL_STATE(122)] = 4980,
  [SMALL_STATE(123)] = 4988,
  [SMALL_STATE(124)] = 4996,
  [SMALL_STATE(125)] = 5006,
  [SMALL_STATE(126)] = 5014,
  [SMALL_STATE(127)] = 5024,
  [SMALL_STATE(128)] = 5032,
  [SMALL_STATE(129)] = 5042,
  [SMALL_STATE(130)] = 5052,
  [SMALL_STATE(131)] = 5062,
  [SMALL_STATE(132)] = 5070,
  [SMALL_STATE(133)] = 5080,
  [SMALL_STATE(134)] = 5088,
  [SMALL_STATE(135)] = 5098,
  [SMALL_STATE(136)] = 5106,
  [SMALL_STATE(137)] = 5114,
  [SMALL_STATE(138)] = 5122,
  [SMALL_STATE(139)] = 5132,
  [SMALL_STATE(140)] = 5140,
  [SMALL_STATE(141)] = 5150,
  [SMALL_STATE(142)] = 5158,
  [SMALL_STATE(143)] = 5168,
  [SMALL_STATE(144)] = 5176,
  [SMALL_STATE(145)] = 5186,
  [SMALL_STATE(146)] = 5196,
  [SMALL_STATE(147)] = 5206,
  [SMALL_STATE(148)] = 5213,
  [SMALL_STATE(149)] = 5220,
  [SMALL_STATE(150)] = 5227,
  [SMALL_STATE(151)] = 5234,
  [SMALL_STATE(152)] = 5241,
  [SMALL_STATE(153)] = 5248,
  [SMALL_STATE(154)] = 5255,
  [SMALL_STATE(155)] = 5262,
  [SMALL_STATE(156)] = 5269,
  [SMALL_STATE(157)] = 5276,
  [SMALL_STATE(158)] = 5283,
  [SMALL_STATE(159)] = 5290,
  [SMALL_STATE(160)] = 5297,
  [SMALL_STATE(161)] = 5304,
  [SMALL_STATE(162)] = 5311,
  [SMALL_STATE(163)] = 5318,
  [SMALL_STATE(164)] = 5325,
  [SMALL_STATE(165)] = 5332,
  [SMALL_STATE(166)] = 5339,
  [SMALL_STATE(167)] = 5346,
  [SMALL_STATE(168)] = 5353,
  [SMALL_STATE(169)] = 5360,
  [SMALL_STATE(170)] = 5367,
  [SMALL_STATE(171)] = 5374,
  [SMALL_STATE(172)] = 5381,
  [SMALL_STATE(173)] = 5388,
  [SMALL_STATE(174)] = 5395,
  [SMALL_STATE(175)] = 5402,
  [SMALL_STATE(176)] = 5409,
  [SMALL_STATE(177)] = 5416,
  [SMALL_STATE(178)] = 5423,
  [SMALL_STATE(179)] = 5430,
  [SMALL_STATE(180)] = 5437,
  [SMALL_STATE(181)] = 5444,
  [SMALL_STATE(182)] = 5451,
  [SMALL_STATE(183)] = 5458,
  [SMALL_STATE(184)] = 5465,
  [SMALL_STATE(185)] = 5472,
  [SMALL_STATE(186)] = 5479,
  [SMALL_STATE(187)] = 5486,
  [SMALL_STATE(188)] = 5493,
  [SMALL_STATE(189)] = 5500,
  [SMALL_STATE(190)] = 5507,
  [SMALL_STATE(191)] = 5514,
  [SMALL_STATE(192)] = 5521,
  [SMALL_STATE(193)] = 5528,
  [SMALL_STATE(194)] = 5535,
  [SMALL_STATE(195)] = 5542,
  [SMALL_STATE(196)] = 5549,
  [SMALL_STATE(197)] = 5556,
  [SMALL_STATE(198)] = 5563,
  [SMALL_STATE(199)] = 5570,
  [SMALL_STATE(200)] = 5577,
  [SMALL_STATE(201)] = 5584,
  [SMALL_STATE(202)] = 5591,
  [SMALL_STATE(203)] = 5598,
  [SMALL_STATE(204)] = 5605,
  [SMALL_STATE(205)] = 5612,
  [SMALL_STATE(206)] = 5619,
  [SMALL_STATE(207)] = 5626,
  [SMALL_STATE(208)] = 5633,
  [SMALL_STATE(209)] = 5640,
  [SMALL_STATE(210)] = 5647,
  [SMALL_STATE(211)] = 5654,
  [SMALL_STATE(212)] = 5661,
  [SMALL_STATE(213)] = 5668,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [9] = {.entry = {.count = 1, .reusable = false}}, SHIFT(10),
  [11] = {.entry = {.count = 1, .reusable = false}}, SHIFT(199),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(99),
  [15] = {.entry = {.count = 1, .reusable = false}}, SHIFT(191),
  [17] = {.entry = {.count = 1, .reusable = false}}, SHIFT(187),
  [19] = {.entry = {.count = 1, .reusable = false}}, SHIFT(185),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(181),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(179),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(175),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT(132),
  [29] = {.entry = {.count = 1, .reusable = false}}, SHIFT(134),
  [31] = {.entry = {.count = 1, .reusable = false}}, SHIFT(146),
  [33] = {.entry = {.count = 1, .reusable = false}}, SHIFT(161),
  [35] = {.entry = {.count = 1, .reusable = false}}, SHIFT(66),
  [37] = {.entry = {.count = 1, .reusable = true}}, SHIFT(66),
  [39] = {.entry = {.count = 1, .reusable = false}}, SHIFT(65),
  [41] = {.entry = {.count = 1, .reusable = false}}, SHIFT(24),
  [43] = {.entry = {.count = 1, .reusable = false}}, SHIFT(18),
  [45] = {.entry = {.count = 1, .reusable = false}}, SHIFT(9),
  [47] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [49] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [51] = {.entry = {.count = 1, .reusable = true}}, SHIFT(192),
  [53] = {.entry = {.count = 1, .reusable = false}}, SHIFT(55),
  [55] = {.entry = {.count = 1, .reusable = true}}, SHIFT(111),
  [57] = {.entry = {.count = 1, .reusable = true}}, SHIFT(208),
  [59] = {.entry = {.count = 1, .reusable = false}}, SHIFT(58),
  [61] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [63] = {.entry = {.count = 1, .reusable = true}}, SHIFT(207),
  [65] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [67] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [69] = {.entry = {.count = 1, .reusable = false}}, SHIFT(61),
  [71] = {.entry = {.count = 1, .reusable = true}}, SHIFT(59),
  [73] = {.entry = {.count = 1, .reusable = false}}, SHIFT(6),
  [75] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [77] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [79] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(9),
  [82] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(3),
  [85] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2),
  [87] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(192),
  [90] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(55),
  [93] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(111),
  [96] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(208),
  [99] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(58),
  [102] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(126),
  [105] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(207),
  [108] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(124),
  [111] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(9),
  [114] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_basic_native_form_repeat1, 2), SHIFT_REPEAT(61),
  [117] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [119] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [121] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [123] = {.entry = {.count = 1, .reusable = false}}, SHIFT(7),
  [125] = {.entry = {.count = 1, .reusable = true}}, SHIFT(49),
  [127] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [129] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
  [131] = {.entry = {.count = 1, .reusable = false}}, SHIFT(210),
  [133] = {.entry = {.count = 1, .reusable = true}}, SHIFT(210),
  [135] = {.entry = {.count = 1, .reusable = false}}, SHIFT(193),
  [137] = {.entry = {.count = 1, .reusable = true}}, SHIFT(193),
  [139] = {.entry = {.count = 1, .reusable = false}}, SHIFT(160),
  [141] = {.entry = {.count = 1, .reusable = true}}, SHIFT(160),
  [143] = {.entry = {.count = 1, .reusable = false}}, SHIFT(148),
  [145] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [147] = {.entry = {.count = 1, .reusable = false}}, SHIFT(201),
  [149] = {.entry = {.count = 1, .reusable = true}}, SHIFT(201),
  [151] = {.entry = {.count = 1, .reusable = false}}, SHIFT(178),
  [153] = {.entry = {.count = 1, .reusable = true}}, SHIFT(178),
  [155] = {.entry = {.count = 1, .reusable = false}}, SHIFT(171),
  [157] = {.entry = {.count = 1, .reusable = true}}, SHIFT(171),
  [159] = {.entry = {.count = 1, .reusable = false}}, SHIFT(158),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(158),
  [163] = {.entry = {.count = 1, .reusable = false}}, SHIFT(186),
  [165] = {.entry = {.count = 1, .reusable = true}}, SHIFT(186),
  [167] = {.entry = {.count = 1, .reusable = false}}, SHIFT(139),
  [169] = {.entry = {.count = 1, .reusable = true}}, SHIFT(139),
  [171] = {.entry = {.count = 1, .reusable = false}}, SHIFT(202),
  [173] = {.entry = {.count = 1, .reusable = true}}, SHIFT(202),
  [175] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type_for_trait, 4, .production_id = 13),
  [177] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type_for_trait, 4, .production_id = 13),
  [179] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type_for_trait, 3),
  [181] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type_for_trait, 3),
  [183] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type, 3, .production_id = 7),
  [185] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type, 3, .production_id = 7),
  [187] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_buffer_type, 4),
  [189] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_buffer_type, 4),
  [191] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_ascii_string_type, 4),
  [193] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_ascii_string_type, 4),
  [195] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_utf8_string_type, 4),
  [197] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_utf8_string_type, 4),
  [199] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_optional_type, 4),
  [201] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_optional_type, 4),
  [203] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_native_type, 1),
  [205] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_native_type, 1),
  [207] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type, 4, .production_id = 7),
  [209] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type, 4, .production_id = 7),
  [211] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type, 4, .production_id = 15),
  [213] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type, 4, .production_id = 15),
  [215] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_type, 5),
  [217] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_type, 5),
  [219] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_response_type, 5),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_response_type, 5),
  [223] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_type, 5, .production_id = 15),
  [225] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_type, 5, .production_id = 15),
  [227] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_let_expression_repeat2, 2),
  [229] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_let_expression_repeat2, 2), SHIFT_REPEAT(5),
  [232] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_let_expression_repeat2, 2),
  [234] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_let_expression, 8, .production_id = 1),
  [236] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_let_expression, 8, .production_id = 1),
  [238] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_let_expression, 6, .production_id = 1),
  [240] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_let_expression, 6, .production_id = 1),
  [242] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_basic_native_form, 3, .production_id = 1),
  [244] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_basic_native_form, 3, .production_id = 1),
  [246] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_basic_native_form, 4, .production_id = 1),
  [248] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_basic_native_form, 4, .production_id = 1),
  [250] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_contract_function_call, 3, .production_id = 1),
  [252] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_contract_function_call, 3, .production_id = 1),
  [254] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_let_expression, 7, .production_id = 1),
  [256] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_let_expression, 7, .production_id = 1),
  [258] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_contract_function_call, 4, .production_id = 1),
  [260] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_contract_function_call, 4, .production_id = 1),
  [262] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_standard_principal_lit, 2),
  [264] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_standard_principal_lit, 2),
  [266] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_lit, 4, .production_id = 3),
  [268] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_lit, 4, .production_id = 3),
  [270] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_lit, 3, .production_id = 3),
  [272] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_lit, 3, .production_id = 3),
  [274] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_lit, 3),
  [276] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_lit, 3),
  [278] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_ascii_string_lit, 3),
  [280] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_ascii_string_lit, 3),
  [282] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_contract_principal_lit, 3),
  [284] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_contract_principal_lit, 3),
  [286] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_some_lit, 4),
  [288] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_some_lit, 4),
  [290] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_ascii_string_lit, 2),
  [292] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_ascii_string_lit, 2),
  [294] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_buffer_lit, 2),
  [296] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_buffer_lit, 2),
  [298] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_int_lit, 1),
  [300] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_int_lit, 1),
  [302] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_int_lit, 2),
  [304] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_int_lit, 2),
  [306] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_response_lit, 4),
  [308] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_response_lit, 4),
  [310] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_bool_lit, 1),
  [312] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_bool_lit, 1),
  [314] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_lit, 4),
  [316] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_lit, 4),
  [318] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_lit, 4, .production_id = 11),
  [320] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_lit, 4, .production_id = 11),
  [322] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_global, 1),
  [324] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_global, 1),
  [326] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__literal, 1),
  [328] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__literal, 1),
  [330] = {.entry = {.count = 1, .reusable = true}}, SHIFT(197),
  [332] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tuple_lit, 5, .production_id = 11),
  [334] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tuple_lit, 5, .production_id = 11),
  [336] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_contract_principal_lit, 2),
  [338] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_contract_principal_lit, 2),
  [340] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_list_lit_token, 1),
  [342] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_list_lit_token, 1),
  [344] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_native_identifier, 1),
  [346] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_native_identifier, 1),
  [348] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_let_expression_repeat2, 1),
  [350] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_let_expression_repeat2, 1),
  [352] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__parameter, 1),
  [354] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_signature, 3),
  [356] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_signature, 3),
  [358] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_signature, 4),
  [360] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_signature, 4),
  [362] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2),
  [364] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_repeat1, 2), SHIFT_REPEAT(2),
  [367] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source, 1),
  [369] = {.entry = {.count = 1, .reusable = true}}, SHIFT(92),
  [371] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [373] = {.entry = {.count = 1, .reusable = true}}, SHIFT(176),
  [375] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [377] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [379] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [381] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_signature_for_trait_repeat1, 2), SHIFT_REPEAT(92),
  [384] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_signature_for_trait_repeat1, 2),
  [386] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_signature_for_trait_repeat1, 2), SHIFT_REPEAT(176),
  [389] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_signature_for_trait_repeat1, 2), SHIFT_REPEAT(32),
  [392] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_signature_for_trait_repeat1, 2), SHIFT_REPEAT(109),
  [395] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_type, 1),
  [397] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_trait_type, 3),
  [399] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [401] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [403] = {.entry = {.count = 1, .reusable = true}}, SHIFT(154),
  [405] = {.entry = {.count = 1, .reusable = true}}, SHIFT(153),
  [407] = {.entry = {.count = 1, .reusable = true}}, SHIFT(152),
  [409] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [411] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [413] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [415] = {.entry = {.count = 1, .reusable = true}}, SHIFT(162),
  [417] = {.entry = {.count = 1, .reusable = true}}, SHIFT(159),
  [419] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_signature_repeat1, 2), SHIFT_REPEAT(213),
  [422] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_signature_repeat1, 2),
  [424] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [426] = {.entry = {.count = 1, .reusable = true}}, SHIFT(168),
  [428] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [430] = {.entry = {.count = 1, .reusable = true}}, SHIFT(213),
  [432] = {.entry = {.count = 1, .reusable = true}}, SHIFT(68),
  [434] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_let_expression_repeat1, 2), SHIFT_REPEAT(168),
  [437] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_let_expression_repeat1, 2),
  [439] = {.entry = {.count = 1, .reusable = true}}, SHIFT(69),
  [441] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [443] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_trait_definition_repeat1, 2), SHIFT_REPEAT(162),
  [446] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_trait_definition_repeat1, 2),
  [448] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tuple_type_for_trait_repeat1, 2, .production_id = 8), SHIFT_REPEAT(149),
  [451] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tuple_type_for_trait_repeat1, 2, .production_id = 8),
  [453] = {.entry = {.count = 1, .reusable = true}}, SHIFT(149),
  [455] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [457] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [459] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tuple_lit_repeat1, 2, .production_id = 4), SHIFT_REPEAT(166),
  [462] = {.entry = {.count = 1, .reusable = true}}, SHIFT(151),
  [464] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tuple_type_repeat1, 2, .production_id = 8), SHIFT_REPEAT(151),
  [467] = {.entry = {.count = 1, .reusable = true}}, SHIFT(166),
  [469] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_trait_definition, 7),
  [471] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_variable_definition, 6),
  [473] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_signature_for_trait, 7),
  [475] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_signature_for_trait, 6),
  [477] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_private_function, 5),
  [479] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_read_only_function, 5),
  [481] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_public_function, 5),
  [483] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tuple_type_for_trait_repeat1, 4, .production_id = 16),
  [485] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_token_definition, 1),
  [487] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_parameter, 4),
  [489] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_definition, 1),
  [491] = {.entry = {.count = 1, .reusable = false}}, SHIFT(53),
  [493] = {.entry = {.count = 1, .reusable = false}}, SHIFT(163),
  [495] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [497] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_fungible_token_definition, 4),
  [499] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [501] = {.entry = {.count = 1, .reusable = false}}, SHIFT(46),
  [503] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_local_binding, 4),
  [505] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tuple_lit_repeat1, 2, .production_id = 2),
  [507] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [509] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [511] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [513] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tuple_type_repeat1, 2, .production_id = 6),
  [515] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [517] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__tuple_type_pair, 3, .production_id = 14),
  [519] = {.entry = {.count = 1, .reusable = true}}, SHIFT(209),
  [521] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_trait_definition, 6),
  [523] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_constant_definition, 5),
  [525] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_trait_implementation, 6, .production_id = 5),
  [527] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_trait_usage, 7, .production_id = 12),
  [529] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [531] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__tuple_lit_pair, 3, .production_id = 10),
  [533] = {.entry = {.count = 1, .reusable = true}}, SHIFT(128),
  [535] = {.entry = {.count = 1, .reusable = true}}, SHIFT(48),
  [537] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_non_fungible_token_definition, 5),
  [539] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [541] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_mapping_definition, 6, .production_id = 9),
  [543] = {.entry = {.count = 1, .reusable = true}}, SHIFT(130),
  [545] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [547] = {.entry = {.count = 1, .reusable = true}}, SHIFT(142),
  [549] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [551] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [553] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [555] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [557] = {.entry = {.count = 1, .reusable = true}}, SHIFT(93),
  [559] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [561] = {.entry = {.count = 1, .reusable = true}}, SHIFT(75),
  [563] = {.entry = {.count = 1, .reusable = true}}, SHIFT(165),
  [565] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [567] = {.entry = {.count = 1, .reusable = true}}, SHIFT(172),
  [569] = {.entry = {.count = 1, .reusable = true}}, SHIFT(173),
  [571] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [573] = {.entry = {.count = 1, .reusable = true}}, SHIFT(127),
  [575] = {.entry = {.count = 1, .reusable = true}}, SHIFT(133),
  [577] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [579] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [581] = {.entry = {.count = 1, .reusable = true}}, SHIFT(184),
  [583] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [585] = {.entry = {.count = 1, .reusable = true}}, SHIFT(143),
  [587] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [589] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [591] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [593] = {.entry = {.count = 1, .reusable = true}}, SHIFT(195),
  [595] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [597] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [599] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [601] = {.entry = {.count = 1, .reusable = true}}, SHIFT(137),
  [603] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [605] = {.entry = {.count = 1, .reusable = true}}, SHIFT(88),
  [607] = {.entry = {.count = 1, .reusable = true}}, SHIFT(198),
  [609] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [611] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [613] = {.entry = {.count = 1, .reusable = true}}, SHIFT(81),
  [615] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [617] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [619] = {.entry = {.count = 1, .reusable = true}}, SHIFT(141),
  [621] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [623] = {.entry = {.count = 1, .reusable = true}}, SHIFT(72),
  [625] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [627] = {.entry = {.count = 1, .reusable = true}}, SHIFT(114),
  [629] = {.entry = {.count = 1, .reusable = true}}, SHIFT(190),
  [631] = {.entry = {.count = 1, .reusable = true}}, SHIFT(203),
  [633] = {.entry = {.count = 1, .reusable = true}}, SHIFT(122),
  [635] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [637] = {.entry = {.count = 1, .reusable = true}}, SHIFT(103),
  [639] = {.entry = {.count = 1, .reusable = true}}, SHIFT(64),
  [641] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [643] = {.entry = {.count = 1, .reusable = true}}, SHIFT(156),
  [645] = {.entry = {.count = 1, .reusable = true}}, SHIFT(157),
  [647] = {.entry = {.count = 1, .reusable = true}}, SHIFT(51),
  [649] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [651] = {.entry = {.count = 1, .reusable = true}}, SHIFT(150),
  [653] = {.entry = {.count = 1, .reusable = true}}, SHIFT(120),
  [655] = {.entry = {.count = 1, .reusable = true}}, SHIFT(57),
  [657] = {.entry = {.count = 1, .reusable = true}}, SHIFT(52),
  [659] = {.entry = {.count = 1, .reusable = true}}, SHIFT(196),
  [661] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [663] = {.entry = {.count = 1, .reusable = true}}, SHIFT(54),
  [665] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [667] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [669] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [671] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [673] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [675] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_clarity(void) {
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
    .keyword_capture_token = sym_identifier,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
