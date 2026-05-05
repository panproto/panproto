#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#ifdef _MSC_VER
#pragma optimize("", off)
#elif defined(__clang__)
#pragma clang optimize off
#elif defined(__GNUC__)
#pragma GCC optimize ("O0")
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 303
#define LARGE_STATE_COUNT 109
#define SYMBOL_COUNT 194
#define ALIAS_COUNT 0
#define TOKEN_COUNT 141
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 1
#define MAX_ALIAS_SEQUENCE_LENGTH 8
#define PRODUCTION_ID_COUNT 2

enum {
  sym__space = 1,
  sym__NL = 2,
  sym_noCommentText = 3,
  anon_sym_BSLASH = 4,
  anon_sym_PLUS = 5,
  anon_sym_COLON = 6,
  sym_note_skip = 7,
  anon_sym_DASH = 8,
  sym_UNDERSCORE = 9,
  sym_TILDE = 10,
  sym_BEAM_SEPARATOR = 11,
  anon_sym_LBRACK = 12,
  anon_sym_RBRACK = 13,
  sym_COMMENT = 14,
  anon_sym_PERCENT_PERCENT = 15,
  aux_sym__MUSIC_CODE_token1 = 16,
  sym_slur_open = 17,
  sym_slur_close = 18,
  aux_sym_note_construct_token1 = 19,
  sym_alteration = 20,
  sym_note_letter = 21,
  sym_octave = 22,
  sym_rest = 23,
  anon_sym_Z = 24,
  anon_sym_X = 25,
  aux_sym_multimeasure_rest_token1 = 26,
  sym_rhythm = 27,
  aux_sym_grace_note_token1 = 28,
  aux_sym_grace_note_token2 = 29,
  aux_sym_chord_symbol_token1 = 30,
  aux_sym_chord_symbol_token2 = 31,
  sym_annotation = 32,
  sym_tuplet_marker = 33,
  sym_chord_type = 34,
  sym_decoration_shorthand = 35,
  aux_sym_nth_ending_number_token1 = 36,
  anon_sym_COMMA = 37,
  sym_bar_line = 38,
  sym_close_thin_thick_double_bar_line = 39,
  sym_thin_double_bar_line = 40,
  sym_open_thick_thin_double_bar_line = 41,
  sym_start_of_repeated_section = 42,
  sym_end_of_repeated_section = 43,
  sym_start_end_of_two_repeated_sections = 44,
  aux_sym_first_repeat_bar_token1 = 45,
  aux_sym_first_repeat_bar_token2 = 46,
  aux_sym_lyric_line_token1 = 47,
  aux_sym_verse_number_token1 = 48,
  anon_sym_DOT = 49,
  anon_sym_BANGtrill_BANG = 50,
  anon_sym_BANGtrill_LPAREN_BANG = 51,
  anon_sym_BANGtrill_RPAREN_BANG = 52,
  anon_sym_BANGlowermordent_BANG = 53,
  anon_sym_BANGuppermordent_BANG = 54,
  anon_sym_BANGmordent_BANG = 55,
  anon_sym_BANGpralltriller_BANG = 56,
  anon_sym_BANGroll_BANG = 57,
  anon_sym_BANGturn_BANG = 58,
  anon_sym_BANGturnx_BANG = 59,
  anon_sym_BANGinvertedturn_BANG = 60,
  anon_sym_BANGinvertedturnx_BANG = 61,
  anon_sym_BANGarpeggio_BANG = 62,
  anon_sym_BANG_GT_BANG = 63,
  anon_sym_BANGaccent_BANG = 64,
  anon_sym_BANGemphasis_BANG = 65,
  anon_sym_BANGfermata_BANG = 66,
  anon_sym_BANGinvertedfermata_BANG = 67,
  anon_sym_BANGtenuto_BANG = 68,
  anon_sym_BANG0_BANG_DASH_BANG5_BANG = 69,
  anon_sym_BANG_PLUS_BANG = 70,
  anon_sym_BANGplus_BANG = 71,
  anon_sym_BANGsnap_BANG = 72,
  anon_sym_BANGslide_BANG = 73,
  anon_sym_BANGwedge_BANG = 74,
  anon_sym_BANGupbow_BANG = 75,
  anon_sym_BANGdownbow_BANG = 76,
  anon_sym_BANGopen_BANG = 77,
  anon_sym_BANGthumb_BANG = 78,
  anon_sym_BANGbreath_BANG = 79,
  anon_sym_BANGpppp_BANG = 80,
  anon_sym_BANGppp_BANG = 81,
  anon_sym_BANGpp_BANG = 82,
  anon_sym_BANGp_BANG = 83,
  anon_sym_BANGmp_BANG = 84,
  anon_sym_BANGmf_BANG = 85,
  anon_sym_BANGf_BANG = 86,
  anon_sym_BANGff_BANG = 87,
  anon_sym_BANGfff_BANG = 88,
  anon_sym_BANGffff_BANG = 89,
  anon_sym_BANGsfz_BANG = 90,
  anon_sym_BANGcrescendo_LPAREN_BANG = 91,
  anon_sym_BANG_LT_LPAREN_BANG = 92,
  anon_sym_BANGcrescendo_RPAREN_BANG = 93,
  anon_sym_BANG_LT_RPAREN_BANG = 94,
  anon_sym_BANGdiminuendo_LPAREN_BANG = 95,
  anon_sym_BANG_GT_LPAREN_BANG = 96,
  anon_sym_BANGdiminuendo_RPAREN_BANG = 97,
  anon_sym_BANG_GT_RPAREN_BANG = 98,
  anon_sym_BANGsegno_BANG = 99,
  anon_sym_BANGcoda_BANG = 100,
  anon_sym_BANGD_DOTS_DOT_BANG = 101,
  anon_sym_BANGD_DOTC_DOT_BANG = 102,
  anon_sym_BANGdacoda_BANG = 103,
  anon_sym_BANGdacapo_BANG = 104,
  anon_sym_BANGfine_BANG = 105,
  anon_sym_BANGshortphrase_BANG = 106,
  anon_sym_BANGmediumphrase_BANG = 107,
  anon_sym_BANGlongphrase_BANG = 108,
  sym_parts_line = 109,
  aux_sym_body_inline_info_token1 = 110,
  anon_sym_PERCENTabc_DASH = 111,
  sym_area = 112,
  sym_book = 113,
  sym_composer = 114,
  sym_discography = 115,
  sym_file = 116,
  sym_group = 117,
  sym_history = 118,
  sym_instruction = 119,
  sym_key = 120,
  sym_macros = 121,
  sym_meter = 122,
  sym_notes = 123,
  sym_origin = 124,
  anon_sym_X_COLON = 125,
  sym_remark = 126,
  sym_rhythm_line = 127,
  sym_source = 128,
  anon_sym_s_COLON = 129,
  sym_tempo = 130,
  sym_transcription = 131,
  sym_tune_title = 132,
  sym_unit_note_length = 133,
  anon_sym_U_COLON = 134,
  aux_sym_user_defined_token1 = 135,
  aux_sym_user_defined_token2 = 136,
  anon_sym_EQ = 137,
  sym_voice = 138,
  sym_words_tag = 139,
  sym_words_postbody = 140,
  sym_source_file = 141,
  sym_open_chord = 142,
  sym_close_chord = 143,
  sym_stylesheet_directives = 144,
  sym__MUSIC_CODE = 145,
  sym_file_structure = 146,
  sym_file_header = 147,
  sym_tune = 148,
  sym_tune_header = 149,
  sym_tune_body = 150,
  sym__music_content = 151,
  sym__nte_or_chrd = 152,
  sym_beam = 153,
  sym_note_construct = 154,
  sym__chord_cstrct = 155,
  sym_note = 156,
  sym_chord_note = 157,
  sym__pitch = 158,
  sym_multimeasure_rest = 159,
  sym_grace_note = 160,
  sym_chord_symbol_note = 161,
  sym_chord_symbol = 162,
  sym_decoration = 163,
  sym_nth_ending_barline = 164,
  sym_nth_ending_number = 165,
  sym_generic_bar_line = 166,
  sym_first_repeat_bar = 167,
  sym_second_repeat_bar = 168,
  sym_lyric_line = 169,
  sym_lyric_section = 170,
  sym_symbol = 171,
  sym_body_inline_info = 172,
  sym_abc_version = 173,
  sym_reference_number_line = 174,
  sym_symbol_line = 175,
  sym_user_defined = 176,
  sym_body_info_line = 177,
  sym_tune_header_info_line = 178,
  sym_file_header_info_line = 179,
  aux_sym_file_structure_repeat1 = 180,
  aux_sym_file_structure_repeat2 = 181,
  aux_sym_file_header_repeat1 = 182,
  aux_sym_tune_header_repeat1 = 183,
  aux_sym_tune_body_repeat1 = 184,
  aux_sym_beam_repeat1 = 185,
  aux_sym_note_construct_repeat1 = 186,
  aux_sym__chord_cstrct_repeat1 = 187,
  aux_sym__pitch_repeat1 = 188,
  aux_sym_grace_note_repeat1 = 189,
  aux_sym_nth_ending_number_repeat1 = 190,
  aux_sym_lyric_section_repeat1 = 191,
  aux_sym_abc_version_repeat1 = 192,
  aux_sym_symbol_line_repeat1 = 193,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [sym__space] = "_space",
  [sym__NL] = "_NL",
  [sym_noCommentText] = "noCommentText",
  [anon_sym_BSLASH] = "\\",
  [anon_sym_PLUS] = "+",
  [anon_sym_COLON] = ":",
  [sym_note_skip] = "note_skip",
  [anon_sym_DASH] = "-",
  [sym_UNDERSCORE] = "UNDERSCORE",
  [sym_TILDE] = "TILDE",
  [sym_BEAM_SEPARATOR] = "BEAM_SEPARATOR",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [sym_COMMENT] = "COMMENT",
  [anon_sym_PERCENT_PERCENT] = "%%",
  [aux_sym__MUSIC_CODE_token1] = "_MUSIC_CODE_token1",
  [sym_slur_open] = "slur_open",
  [sym_slur_close] = "slur_close",
  [aux_sym_note_construct_token1] = "note_construct_token1",
  [sym_alteration] = "alteration",
  [sym_note_letter] = "note_letter",
  [sym_octave] = "octave",
  [sym_rest] = "rest",
  [anon_sym_Z] = "Z",
  [anon_sym_X] = "X",
  [aux_sym_multimeasure_rest_token1] = "multimeasure_rest_token1",
  [sym_rhythm] = "rhythm",
  [aux_sym_grace_note_token1] = "grace_note_token1",
  [aux_sym_grace_note_token2] = "grace_note_token2",
  [aux_sym_chord_symbol_token1] = "chord_symbol_token1",
  [aux_sym_chord_symbol_token2] = "chord_symbol_token2",
  [sym_annotation] = "annotation",
  [sym_tuplet_marker] = "tuplet_marker",
  [sym_chord_type] = "chord_type",
  [sym_decoration_shorthand] = "decoration_shorthand",
  [aux_sym_nth_ending_number_token1] = "nth_ending_number_token1",
  [anon_sym_COMMA] = ",",
  [sym_bar_line] = "bar_line",
  [sym_close_thin_thick_double_bar_line] = "close_thin_thick_double_bar_line",
  [sym_thin_double_bar_line] = "thin_double_bar_line",
  [sym_open_thick_thin_double_bar_line] = "open_thick_thin_double_bar_line",
  [sym_start_of_repeated_section] = "start_of_repeated_section",
  [sym_end_of_repeated_section] = "end_of_repeated_section",
  [sym_start_end_of_two_repeated_sections] = "start_end_of_two_repeated_sections",
  [aux_sym_first_repeat_bar_token1] = "first_repeat_bar_token1",
  [aux_sym_first_repeat_bar_token2] = "first_repeat_bar_token2",
  [aux_sym_lyric_line_token1] = "lyric_line_token1",
  [aux_sym_verse_number_token1] = "verse_number_token1",
  [anon_sym_DOT] = ".",
  [anon_sym_BANGtrill_BANG] = "!trill!",
  [anon_sym_BANGtrill_LPAREN_BANG] = "!trill(!",
  [anon_sym_BANGtrill_RPAREN_BANG] = "!trill)!",
  [anon_sym_BANGlowermordent_BANG] = "!lowermordent!",
  [anon_sym_BANGuppermordent_BANG] = "!uppermordent!",
  [anon_sym_BANGmordent_BANG] = "!mordent!",
  [anon_sym_BANGpralltriller_BANG] = "!pralltriller!",
  [anon_sym_BANGroll_BANG] = "!roll!",
  [anon_sym_BANGturn_BANG] = "!turn!",
  [anon_sym_BANGturnx_BANG] = "!turnx!",
  [anon_sym_BANGinvertedturn_BANG] = "!invertedturn!",
  [anon_sym_BANGinvertedturnx_BANG] = "!invertedturnx!",
  [anon_sym_BANGarpeggio_BANG] = "!arpeggio!",
  [anon_sym_BANG_GT_BANG] = "!>!",
  [anon_sym_BANGaccent_BANG] = "!accent!",
  [anon_sym_BANGemphasis_BANG] = "!emphasis!",
  [anon_sym_BANGfermata_BANG] = "!fermata!",
  [anon_sym_BANGinvertedfermata_BANG] = "!invertedfermata!",
  [anon_sym_BANGtenuto_BANG] = "!tenuto!",
  [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = "!0! - !5!",
  [anon_sym_BANG_PLUS_BANG] = "!+!",
  [anon_sym_BANGplus_BANG] = "!plus!",
  [anon_sym_BANGsnap_BANG] = "!snap!",
  [anon_sym_BANGslide_BANG] = "!slide!",
  [anon_sym_BANGwedge_BANG] = "!wedge!",
  [anon_sym_BANGupbow_BANG] = "!upbow!",
  [anon_sym_BANGdownbow_BANG] = "!downbow!",
  [anon_sym_BANGopen_BANG] = "!open!",
  [anon_sym_BANGthumb_BANG] = "!thumb!",
  [anon_sym_BANGbreath_BANG] = "!breath!",
  [anon_sym_BANGpppp_BANG] = "!pppp!",
  [anon_sym_BANGppp_BANG] = "!ppp!",
  [anon_sym_BANGpp_BANG] = "!pp!",
  [anon_sym_BANGp_BANG] = "!p!",
  [anon_sym_BANGmp_BANG] = "!mp!",
  [anon_sym_BANGmf_BANG] = "!mf!",
  [anon_sym_BANGf_BANG] = "!f!",
  [anon_sym_BANGff_BANG] = "!ff!",
  [anon_sym_BANGfff_BANG] = "!fff!",
  [anon_sym_BANGffff_BANG] = "!ffff!",
  [anon_sym_BANGsfz_BANG] = "!sfz!",
  [anon_sym_BANGcrescendo_LPAREN_BANG] = "!crescendo(!",
  [anon_sym_BANG_LT_LPAREN_BANG] = "!<(!",
  [anon_sym_BANGcrescendo_RPAREN_BANG] = "!crescendo)!",
  [anon_sym_BANG_LT_RPAREN_BANG] = "!<)!",
  [anon_sym_BANGdiminuendo_LPAREN_BANG] = "!diminuendo(!",
  [anon_sym_BANG_GT_LPAREN_BANG] = "!>(!",
  [anon_sym_BANGdiminuendo_RPAREN_BANG] = "!diminuendo)!",
  [anon_sym_BANG_GT_RPAREN_BANG] = "!>)!",
  [anon_sym_BANGsegno_BANG] = "!segno!",
  [anon_sym_BANGcoda_BANG] = "!coda!",
  [anon_sym_BANGD_DOTS_DOT_BANG] = "!D.S.!",
  [anon_sym_BANGD_DOTC_DOT_BANG] = "!D.C.!",
  [anon_sym_BANGdacoda_BANG] = "!dacoda!",
  [anon_sym_BANGdacapo_BANG] = "!dacapo!",
  [anon_sym_BANGfine_BANG] = "!fine!",
  [anon_sym_BANGshortphrase_BANG] = "!shortphrase!",
  [anon_sym_BANGmediumphrase_BANG] = "!mediumphrase!",
  [anon_sym_BANGlongphrase_BANG] = "!longphrase!",
  [sym_parts_line] = "parts_line",
  [aux_sym_body_inline_info_token1] = "body_inline_info_token1",
  [anon_sym_PERCENTabc_DASH] = "%abc-",
  [sym_area] = "area",
  [sym_book] = "book",
  [sym_composer] = "composer",
  [sym_discography] = "discography",
  [sym_file] = "file",
  [sym_group] = "group",
  [sym_history] = "history",
  [sym_instruction] = "instruction",
  [sym_key] = "key",
  [sym_macros] = "macros",
  [sym_meter] = "meter",
  [sym_notes] = "notes",
  [sym_origin] = "origin",
  [anon_sym_X_COLON] = "X:",
  [sym_remark] = "remark",
  [sym_rhythm_line] = "rhythm_line",
  [sym_source] = "source",
  [anon_sym_s_COLON] = "s:",
  [sym_tempo] = "tempo",
  [sym_transcription] = "transcription",
  [sym_tune_title] = "tune_title",
  [sym_unit_note_length] = "unit_note_length",
  [anon_sym_U_COLON] = "U:",
  [aux_sym_user_defined_token1] = "user_defined_token1",
  [aux_sym_user_defined_token2] = "user_defined_token2",
  [anon_sym_EQ] = "=",
  [sym_voice] = "voice",
  [sym_words_tag] = "words_tag",
  [sym_words_postbody] = "words_postbody",
  [sym_source_file] = "source_file",
  [sym_open_chord] = "open_chord",
  [sym_close_chord] = "close_chord",
  [sym_stylesheet_directives] = "stylesheet_directives",
  [sym__MUSIC_CODE] = "_MUSIC_CODE",
  [sym_file_structure] = "file_structure",
  [sym_file_header] = "file_header",
  [sym_tune] = "tune",
  [sym_tune_header] = "tune_header",
  [sym_tune_body] = "tune_body",
  [sym__music_content] = "_music_content",
  [sym__nte_or_chrd] = "_nte_or_chrd",
  [sym_beam] = "beam",
  [sym_note_construct] = "note_construct",
  [sym__chord_cstrct] = "_chord_cstrct",
  [sym_note] = "note",
  [sym_chord_note] = "chord_note",
  [sym__pitch] = "_pitch",
  [sym_multimeasure_rest] = "multimeasure_rest",
  [sym_grace_note] = "grace_note",
  [sym_chord_symbol_note] = "chord_symbol_note",
  [sym_chord_symbol] = "chord_symbol",
  [sym_decoration] = "decoration",
  [sym_nth_ending_barline] = "nth_ending_barline",
  [sym_nth_ending_number] = "nth_ending_number",
  [sym_generic_bar_line] = "generic_bar_line",
  [sym_first_repeat_bar] = "first_repeat_bar",
  [sym_second_repeat_bar] = "second_repeat_bar",
  [sym_lyric_line] = "lyric_line",
  [sym_lyric_section] = "lyric_section",
  [sym_symbol] = "symbol",
  [sym_body_inline_info] = "body_inline_info",
  [sym_abc_version] = "abc_version",
  [sym_reference_number_line] = "reference_number_line",
  [sym_symbol_line] = "symbol_line",
  [sym_user_defined] = "user_defined",
  [sym_body_info_line] = "body_info_line",
  [sym_tune_header_info_line] = "tune_header_info_line",
  [sym_file_header_info_line] = "file_header_info_line",
  [aux_sym_file_structure_repeat1] = "file_structure_repeat1",
  [aux_sym_file_structure_repeat2] = "file_structure_repeat2",
  [aux_sym_file_header_repeat1] = "file_header_repeat1",
  [aux_sym_tune_header_repeat1] = "tune_header_repeat1",
  [aux_sym_tune_body_repeat1] = "tune_body_repeat1",
  [aux_sym_beam_repeat1] = "beam_repeat1",
  [aux_sym_note_construct_repeat1] = "note_construct_repeat1",
  [aux_sym__chord_cstrct_repeat1] = "_chord_cstrct_repeat1",
  [aux_sym__pitch_repeat1] = "_pitch_repeat1",
  [aux_sym_grace_note_repeat1] = "grace_note_repeat1",
  [aux_sym_nth_ending_number_repeat1] = "nth_ending_number_repeat1",
  [aux_sym_lyric_section_repeat1] = "lyric_section_repeat1",
  [aux_sym_abc_version_repeat1] = "abc_version_repeat1",
  [aux_sym_symbol_line_repeat1] = "symbol_line_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [sym__space] = sym__space,
  [sym__NL] = sym__NL,
  [sym_noCommentText] = sym_noCommentText,
  [anon_sym_BSLASH] = anon_sym_BSLASH,
  [anon_sym_PLUS] = anon_sym_PLUS,
  [anon_sym_COLON] = anon_sym_COLON,
  [sym_note_skip] = sym_note_skip,
  [anon_sym_DASH] = anon_sym_DASH,
  [sym_UNDERSCORE] = sym_UNDERSCORE,
  [sym_TILDE] = sym_TILDE,
  [sym_BEAM_SEPARATOR] = sym_BEAM_SEPARATOR,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [sym_COMMENT] = sym_COMMENT,
  [anon_sym_PERCENT_PERCENT] = anon_sym_PERCENT_PERCENT,
  [aux_sym__MUSIC_CODE_token1] = aux_sym__MUSIC_CODE_token1,
  [sym_slur_open] = sym_slur_open,
  [sym_slur_close] = sym_slur_close,
  [aux_sym_note_construct_token1] = aux_sym_note_construct_token1,
  [sym_alteration] = sym_alteration,
  [sym_note_letter] = sym_note_letter,
  [sym_octave] = sym_octave,
  [sym_rest] = sym_rest,
  [anon_sym_Z] = anon_sym_Z,
  [anon_sym_X] = anon_sym_X,
  [aux_sym_multimeasure_rest_token1] = aux_sym_multimeasure_rest_token1,
  [sym_rhythm] = sym_rhythm,
  [aux_sym_grace_note_token1] = aux_sym_grace_note_token1,
  [aux_sym_grace_note_token2] = aux_sym_grace_note_token2,
  [aux_sym_chord_symbol_token1] = aux_sym_chord_symbol_token1,
  [aux_sym_chord_symbol_token2] = aux_sym_chord_symbol_token2,
  [sym_annotation] = sym_annotation,
  [sym_tuplet_marker] = sym_tuplet_marker,
  [sym_chord_type] = sym_chord_type,
  [sym_decoration_shorthand] = sym_decoration_shorthand,
  [aux_sym_nth_ending_number_token1] = aux_sym_nth_ending_number_token1,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [sym_bar_line] = sym_bar_line,
  [sym_close_thin_thick_double_bar_line] = sym_close_thin_thick_double_bar_line,
  [sym_thin_double_bar_line] = sym_thin_double_bar_line,
  [sym_open_thick_thin_double_bar_line] = sym_open_thick_thin_double_bar_line,
  [sym_start_of_repeated_section] = sym_start_of_repeated_section,
  [sym_end_of_repeated_section] = sym_end_of_repeated_section,
  [sym_start_end_of_two_repeated_sections] = sym_start_end_of_two_repeated_sections,
  [aux_sym_first_repeat_bar_token1] = aux_sym_first_repeat_bar_token1,
  [aux_sym_first_repeat_bar_token2] = aux_sym_first_repeat_bar_token2,
  [aux_sym_lyric_line_token1] = aux_sym_lyric_line_token1,
  [aux_sym_verse_number_token1] = aux_sym_verse_number_token1,
  [anon_sym_DOT] = anon_sym_DOT,
  [anon_sym_BANGtrill_BANG] = anon_sym_BANGtrill_BANG,
  [anon_sym_BANGtrill_LPAREN_BANG] = anon_sym_BANGtrill_LPAREN_BANG,
  [anon_sym_BANGtrill_RPAREN_BANG] = anon_sym_BANGtrill_RPAREN_BANG,
  [anon_sym_BANGlowermordent_BANG] = anon_sym_BANGlowermordent_BANG,
  [anon_sym_BANGuppermordent_BANG] = anon_sym_BANGuppermordent_BANG,
  [anon_sym_BANGmordent_BANG] = anon_sym_BANGmordent_BANG,
  [anon_sym_BANGpralltriller_BANG] = anon_sym_BANGpralltriller_BANG,
  [anon_sym_BANGroll_BANG] = anon_sym_BANGroll_BANG,
  [anon_sym_BANGturn_BANG] = anon_sym_BANGturn_BANG,
  [anon_sym_BANGturnx_BANG] = anon_sym_BANGturnx_BANG,
  [anon_sym_BANGinvertedturn_BANG] = anon_sym_BANGinvertedturn_BANG,
  [anon_sym_BANGinvertedturnx_BANG] = anon_sym_BANGinvertedturnx_BANG,
  [anon_sym_BANGarpeggio_BANG] = anon_sym_BANGarpeggio_BANG,
  [anon_sym_BANG_GT_BANG] = anon_sym_BANG_GT_BANG,
  [anon_sym_BANGaccent_BANG] = anon_sym_BANGaccent_BANG,
  [anon_sym_BANGemphasis_BANG] = anon_sym_BANGemphasis_BANG,
  [anon_sym_BANGfermata_BANG] = anon_sym_BANGfermata_BANG,
  [anon_sym_BANGinvertedfermata_BANG] = anon_sym_BANGinvertedfermata_BANG,
  [anon_sym_BANGtenuto_BANG] = anon_sym_BANGtenuto_BANG,
  [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = anon_sym_BANG0_BANG_DASH_BANG5_BANG,
  [anon_sym_BANG_PLUS_BANG] = anon_sym_BANG_PLUS_BANG,
  [anon_sym_BANGplus_BANG] = anon_sym_BANGplus_BANG,
  [anon_sym_BANGsnap_BANG] = anon_sym_BANGsnap_BANG,
  [anon_sym_BANGslide_BANG] = anon_sym_BANGslide_BANG,
  [anon_sym_BANGwedge_BANG] = anon_sym_BANGwedge_BANG,
  [anon_sym_BANGupbow_BANG] = anon_sym_BANGupbow_BANG,
  [anon_sym_BANGdownbow_BANG] = anon_sym_BANGdownbow_BANG,
  [anon_sym_BANGopen_BANG] = anon_sym_BANGopen_BANG,
  [anon_sym_BANGthumb_BANG] = anon_sym_BANGthumb_BANG,
  [anon_sym_BANGbreath_BANG] = anon_sym_BANGbreath_BANG,
  [anon_sym_BANGpppp_BANG] = anon_sym_BANGpppp_BANG,
  [anon_sym_BANGppp_BANG] = anon_sym_BANGppp_BANG,
  [anon_sym_BANGpp_BANG] = anon_sym_BANGpp_BANG,
  [anon_sym_BANGp_BANG] = anon_sym_BANGp_BANG,
  [anon_sym_BANGmp_BANG] = anon_sym_BANGmp_BANG,
  [anon_sym_BANGmf_BANG] = anon_sym_BANGmf_BANG,
  [anon_sym_BANGf_BANG] = anon_sym_BANGf_BANG,
  [anon_sym_BANGff_BANG] = anon_sym_BANGff_BANG,
  [anon_sym_BANGfff_BANG] = anon_sym_BANGfff_BANG,
  [anon_sym_BANGffff_BANG] = anon_sym_BANGffff_BANG,
  [anon_sym_BANGsfz_BANG] = anon_sym_BANGsfz_BANG,
  [anon_sym_BANGcrescendo_LPAREN_BANG] = anon_sym_BANGcrescendo_LPAREN_BANG,
  [anon_sym_BANG_LT_LPAREN_BANG] = anon_sym_BANG_LT_LPAREN_BANG,
  [anon_sym_BANGcrescendo_RPAREN_BANG] = anon_sym_BANGcrescendo_RPAREN_BANG,
  [anon_sym_BANG_LT_RPAREN_BANG] = anon_sym_BANG_LT_RPAREN_BANG,
  [anon_sym_BANGdiminuendo_LPAREN_BANG] = anon_sym_BANGdiminuendo_LPAREN_BANG,
  [anon_sym_BANG_GT_LPAREN_BANG] = anon_sym_BANG_GT_LPAREN_BANG,
  [anon_sym_BANGdiminuendo_RPAREN_BANG] = anon_sym_BANGdiminuendo_RPAREN_BANG,
  [anon_sym_BANG_GT_RPAREN_BANG] = anon_sym_BANG_GT_RPAREN_BANG,
  [anon_sym_BANGsegno_BANG] = anon_sym_BANGsegno_BANG,
  [anon_sym_BANGcoda_BANG] = anon_sym_BANGcoda_BANG,
  [anon_sym_BANGD_DOTS_DOT_BANG] = anon_sym_BANGD_DOTS_DOT_BANG,
  [anon_sym_BANGD_DOTC_DOT_BANG] = anon_sym_BANGD_DOTC_DOT_BANG,
  [anon_sym_BANGdacoda_BANG] = anon_sym_BANGdacoda_BANG,
  [anon_sym_BANGdacapo_BANG] = anon_sym_BANGdacapo_BANG,
  [anon_sym_BANGfine_BANG] = anon_sym_BANGfine_BANG,
  [anon_sym_BANGshortphrase_BANG] = anon_sym_BANGshortphrase_BANG,
  [anon_sym_BANGmediumphrase_BANG] = anon_sym_BANGmediumphrase_BANG,
  [anon_sym_BANGlongphrase_BANG] = anon_sym_BANGlongphrase_BANG,
  [sym_parts_line] = sym_parts_line,
  [aux_sym_body_inline_info_token1] = aux_sym_body_inline_info_token1,
  [anon_sym_PERCENTabc_DASH] = anon_sym_PERCENTabc_DASH,
  [sym_area] = sym_area,
  [sym_book] = sym_book,
  [sym_composer] = sym_composer,
  [sym_discography] = sym_discography,
  [sym_file] = sym_file,
  [sym_group] = sym_group,
  [sym_history] = sym_history,
  [sym_instruction] = sym_instruction,
  [sym_key] = sym_key,
  [sym_macros] = sym_macros,
  [sym_meter] = sym_meter,
  [sym_notes] = sym_notes,
  [sym_origin] = sym_origin,
  [anon_sym_X_COLON] = anon_sym_X_COLON,
  [sym_remark] = sym_remark,
  [sym_rhythm_line] = sym_rhythm_line,
  [sym_source] = sym_source,
  [anon_sym_s_COLON] = anon_sym_s_COLON,
  [sym_tempo] = sym_tempo,
  [sym_transcription] = sym_transcription,
  [sym_tune_title] = sym_tune_title,
  [sym_unit_note_length] = sym_unit_note_length,
  [anon_sym_U_COLON] = anon_sym_U_COLON,
  [aux_sym_user_defined_token1] = aux_sym_user_defined_token1,
  [aux_sym_user_defined_token2] = aux_sym_user_defined_token2,
  [anon_sym_EQ] = anon_sym_EQ,
  [sym_voice] = sym_voice,
  [sym_words_tag] = sym_words_tag,
  [sym_words_postbody] = sym_words_postbody,
  [sym_source_file] = sym_source_file,
  [sym_open_chord] = sym_open_chord,
  [sym_close_chord] = sym_close_chord,
  [sym_stylesheet_directives] = sym_stylesheet_directives,
  [sym__MUSIC_CODE] = sym__MUSIC_CODE,
  [sym_file_structure] = sym_file_structure,
  [sym_file_header] = sym_file_header,
  [sym_tune] = sym_tune,
  [sym_tune_header] = sym_tune_header,
  [sym_tune_body] = sym_tune_body,
  [sym__music_content] = sym__music_content,
  [sym__nte_or_chrd] = sym__nte_or_chrd,
  [sym_beam] = sym_beam,
  [sym_note_construct] = sym_note_construct,
  [sym__chord_cstrct] = sym__chord_cstrct,
  [sym_note] = sym_note,
  [sym_chord_note] = sym_chord_note,
  [sym__pitch] = sym__pitch,
  [sym_multimeasure_rest] = sym_multimeasure_rest,
  [sym_grace_note] = sym_grace_note,
  [sym_chord_symbol_note] = sym_chord_symbol_note,
  [sym_chord_symbol] = sym_chord_symbol,
  [sym_decoration] = sym_decoration,
  [sym_nth_ending_barline] = sym_nth_ending_barline,
  [sym_nth_ending_number] = sym_nth_ending_number,
  [sym_generic_bar_line] = sym_generic_bar_line,
  [sym_first_repeat_bar] = sym_first_repeat_bar,
  [sym_second_repeat_bar] = sym_second_repeat_bar,
  [sym_lyric_line] = sym_lyric_line,
  [sym_lyric_section] = sym_lyric_section,
  [sym_symbol] = sym_symbol,
  [sym_body_inline_info] = sym_body_inline_info,
  [sym_abc_version] = sym_abc_version,
  [sym_reference_number_line] = sym_reference_number_line,
  [sym_symbol_line] = sym_symbol_line,
  [sym_user_defined] = sym_user_defined,
  [sym_body_info_line] = sym_body_info_line,
  [sym_tune_header_info_line] = sym_tune_header_info_line,
  [sym_file_header_info_line] = sym_file_header_info_line,
  [aux_sym_file_structure_repeat1] = aux_sym_file_structure_repeat1,
  [aux_sym_file_structure_repeat2] = aux_sym_file_structure_repeat2,
  [aux_sym_file_header_repeat1] = aux_sym_file_header_repeat1,
  [aux_sym_tune_header_repeat1] = aux_sym_tune_header_repeat1,
  [aux_sym_tune_body_repeat1] = aux_sym_tune_body_repeat1,
  [aux_sym_beam_repeat1] = aux_sym_beam_repeat1,
  [aux_sym_note_construct_repeat1] = aux_sym_note_construct_repeat1,
  [aux_sym__chord_cstrct_repeat1] = aux_sym__chord_cstrct_repeat1,
  [aux_sym__pitch_repeat1] = aux_sym__pitch_repeat1,
  [aux_sym_grace_note_repeat1] = aux_sym_grace_note_repeat1,
  [aux_sym_nth_ending_number_repeat1] = aux_sym_nth_ending_number_repeat1,
  [aux_sym_lyric_section_repeat1] = aux_sym_lyric_section_repeat1,
  [aux_sym_abc_version_repeat1] = aux_sym_abc_version_repeat1,
  [aux_sym_symbol_line_repeat1] = aux_sym_symbol_line_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [sym__space] = {
    .visible = false,
    .named = true,
  },
  [sym__NL] = {
    .visible = false,
    .named = true,
  },
  [sym_noCommentText] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_BSLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PLUS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym_note_skip] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [sym_UNDERSCORE] = {
    .visible = true,
    .named = true,
  },
  [sym_TILDE] = {
    .visible = true,
    .named = true,
  },
  [sym_BEAM_SEPARATOR] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [sym_COMMENT] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_PERCENT_PERCENT] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__MUSIC_CODE_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_slur_open] = {
    .visible = true,
    .named = true,
  },
  [sym_slur_close] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_note_construct_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_alteration] = {
    .visible = true,
    .named = true,
  },
  [sym_note_letter] = {
    .visible = true,
    .named = true,
  },
  [sym_octave] = {
    .visible = true,
    .named = true,
  },
  [sym_rest] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_Z] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_X] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_multimeasure_rest_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_rhythm] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_grace_note_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_grace_note_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_chord_symbol_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_chord_symbol_token2] = {
    .visible = false,
    .named = false,
  },
  [sym_annotation] = {
    .visible = true,
    .named = true,
  },
  [sym_tuplet_marker] = {
    .visible = true,
    .named = true,
  },
  [sym_chord_type] = {
    .visible = true,
    .named = true,
  },
  [sym_decoration_shorthand] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_nth_ending_number_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [sym_bar_line] = {
    .visible = true,
    .named = true,
  },
  [sym_close_thin_thick_double_bar_line] = {
    .visible = true,
    .named = true,
  },
  [sym_thin_double_bar_line] = {
    .visible = true,
    .named = true,
  },
  [sym_open_thick_thin_double_bar_line] = {
    .visible = true,
    .named = true,
  },
  [sym_start_of_repeated_section] = {
    .visible = true,
    .named = true,
  },
  [sym_end_of_repeated_section] = {
    .visible = true,
    .named = true,
  },
  [sym_start_end_of_two_repeated_sections] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_first_repeat_bar_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_first_repeat_bar_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_lyric_line_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_verse_number_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGtrill_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGtrill_LPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGtrill_RPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGlowermordent_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGuppermordent_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGmordent_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGpralltriller_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGroll_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGturn_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGturnx_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGinvertedturn_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGinvertedturnx_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGarpeggio_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_GT_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGaccent_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGemphasis_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGfermata_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGinvertedfermata_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGtenuto_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_PLUS_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGplus_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGsnap_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGslide_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGwedge_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGupbow_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGdownbow_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGopen_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGthumb_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGbreath_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGpppp_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGppp_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGpp_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGp_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGmp_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGmf_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGf_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGff_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGfff_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGffff_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGsfz_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGcrescendo_LPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_LT_LPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGcrescendo_RPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_LT_RPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGdiminuendo_LPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_GT_LPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGdiminuendo_RPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_GT_RPAREN_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGsegno_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGcoda_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGD_DOTS_DOT_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGD_DOTC_DOT_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGdacoda_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGdacapo_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGfine_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGshortphrase_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGmediumphrase_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANGlongphrase_BANG] = {
    .visible = true,
    .named = false,
  },
  [sym_parts_line] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_body_inline_info_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_PERCENTabc_DASH] = {
    .visible = true,
    .named = false,
  },
  [sym_area] = {
    .visible = true,
    .named = true,
  },
  [sym_book] = {
    .visible = true,
    .named = true,
  },
  [sym_composer] = {
    .visible = true,
    .named = true,
  },
  [sym_discography] = {
    .visible = true,
    .named = true,
  },
  [sym_file] = {
    .visible = true,
    .named = true,
  },
  [sym_group] = {
    .visible = true,
    .named = true,
  },
  [sym_history] = {
    .visible = true,
    .named = true,
  },
  [sym_instruction] = {
    .visible = true,
    .named = true,
  },
  [sym_key] = {
    .visible = true,
    .named = true,
  },
  [sym_macros] = {
    .visible = true,
    .named = true,
  },
  [sym_meter] = {
    .visible = true,
    .named = true,
  },
  [sym_notes] = {
    .visible = true,
    .named = true,
  },
  [sym_origin] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_X_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym_remark] = {
    .visible = true,
    .named = true,
  },
  [sym_rhythm_line] = {
    .visible = true,
    .named = true,
  },
  [sym_source] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_s_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym_tempo] = {
    .visible = true,
    .named = true,
  },
  [sym_transcription] = {
    .visible = true,
    .named = true,
  },
  [sym_tune_title] = {
    .visible = true,
    .named = true,
  },
  [sym_unit_note_length] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_U_COLON] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_user_defined_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_user_defined_token2] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_EQ] = {
    .visible = true,
    .named = false,
  },
  [sym_voice] = {
    .visible = true,
    .named = true,
  },
  [sym_words_tag] = {
    .visible = true,
    .named = true,
  },
  [sym_words_postbody] = {
    .visible = true,
    .named = true,
  },
  [sym_source_file] = {
    .visible = true,
    .named = true,
  },
  [sym_open_chord] = {
    .visible = true,
    .named = true,
  },
  [sym_close_chord] = {
    .visible = true,
    .named = true,
  },
  [sym_stylesheet_directives] = {
    .visible = true,
    .named = true,
  },
  [sym__MUSIC_CODE] = {
    .visible = false,
    .named = true,
  },
  [sym_file_structure] = {
    .visible = true,
    .named = true,
  },
  [sym_file_header] = {
    .visible = true,
    .named = true,
  },
  [sym_tune] = {
    .visible = true,
    .named = true,
  },
  [sym_tune_header] = {
    .visible = true,
    .named = true,
  },
  [sym_tune_body] = {
    .visible = true,
    .named = true,
  },
  [sym__music_content] = {
    .visible = false,
    .named = true,
  },
  [sym__nte_or_chrd] = {
    .visible = false,
    .named = true,
  },
  [sym_beam] = {
    .visible = true,
    .named = true,
  },
  [sym_note_construct] = {
    .visible = true,
    .named = true,
  },
  [sym__chord_cstrct] = {
    .visible = false,
    .named = true,
  },
  [sym_note] = {
    .visible = true,
    .named = true,
  },
  [sym_chord_note] = {
    .visible = true,
    .named = true,
  },
  [sym__pitch] = {
    .visible = false,
    .named = true,
  },
  [sym_multimeasure_rest] = {
    .visible = true,
    .named = true,
  },
  [sym_grace_note] = {
    .visible = true,
    .named = true,
  },
  [sym_chord_symbol_note] = {
    .visible = true,
    .named = true,
  },
  [sym_chord_symbol] = {
    .visible = true,
    .named = true,
  },
  [sym_decoration] = {
    .visible = true,
    .named = true,
  },
  [sym_nth_ending_barline] = {
    .visible = true,
    .named = true,
  },
  [sym_nth_ending_number] = {
    .visible = true,
    .named = true,
  },
  [sym_generic_bar_line] = {
    .visible = true,
    .named = true,
  },
  [sym_first_repeat_bar] = {
    .visible = true,
    .named = true,
  },
  [sym_second_repeat_bar] = {
    .visible = true,
    .named = true,
  },
  [sym_lyric_line] = {
    .visible = true,
    .named = true,
  },
  [sym_lyric_section] = {
    .visible = true,
    .named = true,
  },
  [sym_symbol] = {
    .visible = true,
    .named = true,
  },
  [sym_body_inline_info] = {
    .visible = true,
    .named = true,
  },
  [sym_abc_version] = {
    .visible = true,
    .named = true,
  },
  [sym_reference_number_line] = {
    .visible = true,
    .named = true,
  },
  [sym_symbol_line] = {
    .visible = true,
    .named = true,
  },
  [sym_user_defined] = {
    .visible = true,
    .named = true,
  },
  [sym_body_info_line] = {
    .visible = true,
    .named = true,
  },
  [sym_tune_header_info_line] = {
    .visible = true,
    .named = true,
  },
  [sym_file_header_info_line] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_file_structure_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_file_structure_repeat2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_file_header_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tune_header_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_tune_body_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_beam_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_note_construct_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__chord_cstrct_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__pitch_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_grace_note_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_nth_ending_number_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_lyric_section_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_abc_version_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_symbol_line_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum {
  field_type = 1,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_type] = "type",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_type, 0},
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
  [66] = 65,
  [67] = 67,
  [68] = 67,
  [69] = 69,
  [70] = 69,
  [71] = 22,
  [72] = 23,
  [73] = 22,
  [74] = 23,
  [75] = 20,
  [76] = 21,
  [77] = 48,
  [78] = 24,
  [79] = 48,
  [80] = 20,
  [81] = 21,
  [82] = 24,
  [83] = 36,
  [84] = 36,
  [85] = 85,
  [86] = 59,
  [87] = 87,
  [88] = 53,
  [89] = 59,
  [90] = 52,
  [91] = 55,
  [92] = 87,
  [93] = 50,
  [94] = 58,
  [95] = 95,
  [96] = 58,
  [97] = 57,
  [98] = 85,
  [99] = 51,
  [100] = 51,
  [101] = 53,
  [102] = 57,
  [103] = 95,
  [104] = 50,
  [105] = 52,
  [106] = 106,
  [107] = 55,
  [108] = 106,
  [109] = 109,
  [110] = 109,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 114,
  [115] = 115,
  [116] = 116,
  [117] = 117,
  [118] = 15,
  [119] = 16,
  [120] = 14,
  [121] = 17,
  [122] = 18,
  [123] = 123,
  [124] = 123,
  [125] = 125,
  [126] = 126,
  [127] = 127,
  [128] = 128,
  [129] = 129,
  [130] = 125,
  [131] = 128,
  [132] = 127,
  [133] = 129,
  [134] = 126,
  [135] = 19,
  [136] = 136,
  [137] = 137,
  [138] = 138,
  [139] = 139,
  [140] = 140,
  [141] = 141,
  [142] = 139,
  [143] = 137,
  [144] = 30,
  [145] = 138,
  [146] = 141,
  [147] = 140,
  [148] = 28,
  [149] = 26,
  [150] = 25,
  [151] = 37,
  [152] = 33,
  [153] = 32,
  [154] = 27,
  [155] = 35,
  [156] = 34,
  [157] = 29,
  [158] = 31,
  [159] = 46,
  [160] = 40,
  [161] = 161,
  [162] = 161,
  [163] = 163,
  [164] = 45,
  [165] = 41,
  [166] = 43,
  [167] = 44,
  [168] = 168,
  [169] = 169,
  [170] = 49,
  [171] = 47,
  [172] = 169,
  [173] = 38,
  [174] = 42,
  [175] = 175,
  [176] = 176,
  [177] = 177,
  [178] = 177,
  [179] = 176,
  [180] = 175,
  [181] = 181,
  [182] = 182,
  [183] = 183,
  [184] = 184,
  [185] = 185,
  [186] = 186,
  [187] = 187,
  [188] = 184,
  [189] = 185,
  [190] = 186,
  [191] = 187,
  [192] = 192,
  [193] = 193,
  [194] = 194,
  [195] = 195,
  [196] = 85,
  [197] = 106,
  [198] = 95,
  [199] = 199,
  [200] = 200,
  [201] = 87,
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
  [214] = 214,
  [215] = 215,
  [216] = 216,
  [217] = 217,
  [218] = 218,
  [219] = 219,
  [220] = 220,
  [221] = 221,
  [222] = 222,
  [223] = 223,
  [224] = 220,
  [225] = 220,
  [226] = 214,
  [227] = 227,
  [228] = 228,
  [229] = 229,
  [230] = 230,
  [231] = 229,
  [232] = 230,
  [233] = 228,
  [234] = 234,
  [235] = 235,
  [236] = 228,
  [237] = 230,
  [238] = 238,
  [239] = 238,
  [240] = 240,
  [241] = 227,
  [242] = 242,
  [243] = 234,
  [244] = 244,
  [245] = 245,
  [246] = 246,
  [247] = 234,
  [248] = 227,
  [249] = 238,
  [250] = 250,
  [251] = 251,
  [252] = 252,
  [253] = 253,
  [254] = 254,
  [255] = 255,
  [256] = 256,
  [257] = 257,
  [258] = 258,
  [259] = 53,
  [260] = 260,
  [261] = 261,
  [262] = 53,
  [263] = 254,
  [264] = 264,
  [265] = 265,
  [266] = 266,
  [267] = 267,
  [268] = 268,
  [269] = 269,
  [270] = 270,
  [271] = 271,
  [272] = 267,
  [273] = 273,
  [274] = 274,
  [275] = 275,
  [276] = 256,
  [277] = 275,
  [278] = 278,
  [279] = 279,
  [280] = 280,
  [281] = 267,
  [282] = 274,
  [283] = 283,
  [284] = 273,
  [285] = 274,
  [286] = 275,
  [287] = 256,
  [288] = 278,
  [289] = 257,
  [290] = 290,
  [291] = 291,
  [292] = 292,
  [293] = 280,
  [294] = 283,
  [295] = 273,
  [296] = 296,
  [297] = 297,
  [298] = 280,
  [299] = 283,
  [300] = 278,
  [301] = 301,
  [302] = 255,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(288);
      if (lookahead == '\n') ADVANCE(291);
      if (lookahead == ' ') ADVANCE(289);
      if (lookahead == '!') ADVANCE(73);
      if (lookahead == '"') ADVANCE(604);
      if (lookahead == '%') ADVANCE(566);
      if (lookahead == '\'') ADVANCE(588);
      if (lookahead == '(') ADVANCE(575);
      if (lookahead == ')') ADVANCE(576);
      if (lookahead == '*') ADVANCE(552);
      if (lookahead == '+') ADVANCE(550);
      if (lookahead == ',') ADVANCE(650);
      if (lookahead == '-') ADVANCE(555);
      if (lookahead == '.') ADVANCE(676);
      if (lookahead == '/') ADVANCE(596);
      if (lookahead == '0') ADVANCE(594);
      if (lookahead == ':') ADVANCE(551);
      if (lookahead == '=') ADVANCE(1145);
      if (lookahead == 'A') ADVANCE(582);
      if (lookahead == 'B') ADVANCE(583);
      if (lookahead == 'C') ADVANCE(584);
      if (lookahead == 'D') ADVANCE(585);
      if (lookahead == 'F') ADVANCE(586);
      if (lookahead == 'G') ADVANCE(587);
      if (lookahead == 'H') ADVANCE(641);
      if (lookahead == 'I') ADVANCE(82);
      if (lookahead == 'K') ADVANCE(83);
      if (lookahead == 'L') ADVANCE(643);
      if (lookahead == 'M') ADVANCE(644);
      if (lookahead == 'N') ADVANCE(84);
      if (lookahead == 'O') ADVANCE(645);
      if (lookahead == 'P') ADVANCE(646);
      if (lookahead == 'Q') ADVANCE(85);
      if (lookahead == 'R') ADVANCE(86);
      if (lookahead == 'S') ADVANCE(647);
      if (lookahead == 'T') ADVANCE(648);
      if (lookahead == 'U') ADVANCE(87);
      if (lookahead == 'V') ADVANCE(88);
      if (lookahead == 'W') ADVANCE(89);
      if (lookahead == 'X') ADVANCE(592);
      if (lookahead == 'Z') ADVANCE(591);
      if (lookahead == '[') ADVANCE(562);
      if (lookahead == '\\') ADVANCE(549);
      if (lookahead == ']') ADVANCE(565);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(558);
      if (lookahead == '`') ADVANCE(560);
      if (lookahead == 'm') ADVANCE(1140);
      if (lookahead == 'r') ADVANCE(1141);
      if (lookahead == 's') ADVANCE(1142);
      if (lookahead == 'w') ADVANCE(1143);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '|') ADVANCE(653);
      if (lookahead == '}') ADVANCE(602);
      if (lookahead == '~') ADVANCE(559);
      if ((9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == '\t' ||
          lookahead == '\r') ADVANCE(574);
      if (('<' <= lookahead && lookahead <= '>')) ADVANCE(598);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == 'u' ||
          lookahead == 'v') ADVANCE(640);
      if (lookahead == 'E' ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(593);
      if (('h' <= lookahead && lookahead <= 't')) ADVANCE(1139);
      END_STATE();
    case 1:
      if (lookahead == '\n') ADVANCE(574);
      if (lookahead == '!') ADVANCE(360);
      if (lookahead == '"') ADVANCE(356);
      if (lookahead == '*') ADVANCE(554);
      if (lookahead == ':') ADVANCE(366);
      if (lookahead == '[') ADVANCE(563);
      if (lookahead == '|') ADVANCE(654);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(292);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(547);
      if (lookahead != 0 &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 2:
      if (lookahead == ' ') ADVANCE(75);
      END_STATE();
    case 3:
      if (lookahead == ' ') ADVANCE(31);
      END_STATE();
    case 4:
      if (lookahead == '!') ADVANCE(738);
      END_STATE();
    case 5:
      if (lookahead == '!') ADVANCE(2);
      END_STATE();
    case 6:
      if (lookahead == '!') ADVANCE(717);
      if (lookahead == '(') ADVANCE(11);
      if (lookahead == ')') ADVANCE(12);
      END_STATE();
    case 7:
      if (lookahead == '!') ADVANCE(786);
      if (lookahead == 'e') ADVANCE(235);
      if (lookahead == 'f') ADVANCE(13);
      if (lookahead == 'i') ADVANCE(203);
      END_STATE();
    case 8:
      if (lookahead == '!') ADVANCE(777);
      if (lookahead == 'l') ADVANCE(268);
      if (lookahead == 'p') ADVANCE(16);
      if (lookahead == 'r') ADVANCE(112);
      END_STATE();
    case 9:
      if (lookahead == '!') ADVANCE(804);
      END_STATE();
    case 10:
      if (lookahead == '!') ADVANCE(810);
      END_STATE();
    case 11:
      if (lookahead == '!') ADVANCE(816);
      END_STATE();
    case 12:
      if (lookahead == '!') ADVANCE(822);
      END_STATE();
    case 13:
      if (lookahead == '!') ADVANCE(789);
      if (lookahead == 'f') ADVANCE(17);
      END_STATE();
    case 14:
      if (lookahead == '!') ADVANCE(783);
      END_STATE();
    case 15:
      if (lookahead == '!') ADVANCE(780);
      END_STATE();
    case 16:
      if (lookahead == '!') ADVANCE(774);
      if (lookahead == 'p') ADVANCE(18);
      END_STATE();
    case 17:
      if (lookahead == '!') ADVANCE(792);
      if (lookahead == 'f') ADVANCE(23);
      END_STATE();
    case 18:
      if (lookahead == '!') ADVANCE(771);
      if (lookahead == 'p') ADVANCE(27);
      END_STATE();
    case 19:
      if (lookahead == '!') ADVANCE(798);
      END_STATE();
    case 20:
      if (lookahead == '!') ADVANCE(834);
      END_STATE();
    case 21:
      if (lookahead == '!') ADVANCE(831);
      END_STATE();
    case 22:
      if (lookahead == '!') ADVANCE(828);
      END_STATE();
    case 23:
      if (lookahead == '!') ADVANCE(795);
      END_STATE();
    case 24:
      if (lookahead == '!') ADVANCE(843);
      END_STATE();
    case 25:
      if (lookahead == '!') ADVANCE(759);
      END_STATE();
    case 26:
      if (lookahead == '!') ADVANCE(741);
      END_STATE();
    case 27:
      if (lookahead == '!') ADVANCE(768);
      END_STATE();
    case 28:
      if (lookahead == '!') ADVANCE(699);
      END_STATE();
    case 29:
      if (lookahead == '!') ADVANCE(744);
      END_STATE();
    case 30:
      if (lookahead == '!') ADVANCE(702);
      if (lookahead == 'x') ADVANCE(36);
      END_STATE();
    case 31:
      if (lookahead == '!') ADVANCE(80);
      END_STATE();
    case 32:
      if (lookahead == '!') ADVANCE(825);
      END_STATE();
    case 33:
      if (lookahead == '!') ADVANCE(747);
      END_STATE();
    case 34:
      if (lookahead == '!') ADVANCE(762);
      END_STATE();
    case 35:
      if (lookahead == '!') ADVANCE(678);
      if (lookahead == '(') ADVANCE(44);
      if (lookahead == ')') ADVANCE(45);
      END_STATE();
    case 36:
      if (lookahead == '!') ADVANCE(705);
      END_STATE();
    case 37:
      if (lookahead == '!') ADVANCE(753);
      END_STATE();
    case 38:
      if (lookahead == '!') ADVANCE(750);
      END_STATE();
    case 39:
      if (lookahead == '!') ADVANCE(720);
      END_STATE();
    case 40:
      if (lookahead == '!') ADVANCE(765);
      END_STATE();
    case 41:
      if (lookahead == '!') ADVANCE(840);
      END_STATE();
    case 42:
      if (lookahead == '!') ADVANCE(837);
      END_STATE();
    case 43:
      if (lookahead == '!') ADVANCE(732);
      END_STATE();
    case 44:
      if (lookahead == '!') ADVANCE(681);
      END_STATE();
    case 45:
      if (lookahead == '!') ADVANCE(684);
      END_STATE();
    case 46:
      if (lookahead == '!') ADVANCE(735);
      END_STATE();
    case 47:
      if (lookahead == '!') ADVANCE(756);
      END_STATE();
    case 48:
      if (lookahead == '!') ADVANCE(726);
      END_STATE();
    case 49:
      if (lookahead == '!') ADVANCE(693);
      END_STATE();
    case 50:
      if (lookahead == '!') ADVANCE(714);
      END_STATE();
    case 51:
      if (lookahead == '!') ADVANCE(723);
      END_STATE();
    case 52:
      if (lookahead == '!') ADVANCE(801);
      END_STATE();
    case 53:
      if (lookahead == '!') ADVANCE(807);
      END_STATE();
    case 54:
      if (lookahead == '!') ADVANCE(852);
      END_STATE();
    case 55:
      if (lookahead == '!') ADVANCE(813);
      END_STATE();
    case 56:
      if (lookahead == '!') ADVANCE(819);
      END_STATE();
    case 57:
      if (lookahead == '!') ADVANCE(846);
      END_STATE();
    case 58:
      if (lookahead == '!') ADVANCE(708);
      if (lookahead == 'x') ADVANCE(63);
      END_STATE();
    case 59:
      if (lookahead == '!') ADVANCE(687);
      END_STATE();
    case 60:
      if (lookahead == '!') ADVANCE(849);
      END_STATE();
    case 61:
      if (lookahead == '!') ADVANCE(696);
      END_STATE();
    case 62:
      if (lookahead == '!') ADVANCE(690);
      END_STATE();
    case 63:
      if (lookahead == '!') ADVANCE(711);
      END_STATE();
    case 64:
      if (lookahead == '!') ADVANCE(729);
      END_STATE();
    case 65:
      if (lookahead == '!') ADVANCE(360);
      if (lookahead == '"') ADVANCE(356);
      if (lookahead == '*') ADVANCE(554);
      if (lookahead == ',') ADVANCE(652);
      if (lookahead == '-') ADVANCE(557);
      if (lookahead == ':') ADVANCE(366);
      if (lookahead == '[') ADVANCE(546);
      if (lookahead == '|') ADVANCE(654);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 66:
      if (lookahead == '!') ADVANCE(360);
      if (lookahead == '"') ADVANCE(356);
      if (lookahead == '*') ADVANCE(554);
      if (lookahead == ':') ADVANCE(366);
      if (lookahead == '[') ADVANCE(546);
      if (lookahead == '|') ADVANCE(654);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 67:
      if (lookahead == '"') ADVANCE(607);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(67);
      END_STATE();
    case 68:
      if (lookahead == '"') ADVANCE(603);
      if (lookahead == '(') ADVANCE(280);
      if (lookahead == '-') ADVANCE(577);
      if (lookahead == '/') ADVANCE(596);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == '[') ADVANCE(561);
      if (lookahead == ']') ADVANCE(565);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(580);
      if (lookahead == '`') ADVANCE(560);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '}') ADVANCE(602);
      if (lookahead == '\'' ||
          lookahead == ',') ADVANCE(588);
      if (('<' <= lookahead && lookahead <= '>')) ADVANCE(598);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(597);
      if (lookahead == '.' ||
          lookahead == 'H' ||
          lookahead == 'L' ||
          lookahead == 'M' ||
          lookahead == 'O' ||
          lookahead == 'P' ||
          lookahead == 'S' ||
          lookahead == 'T' ||
          lookahead == 'u' ||
          lookahead == 'v' ||
          lookahead == '~') ADVANCE(640);
      if (('A' <= lookahead && lookahead <= 'G') ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      END_STATE();
    case 69:
      if (lookahead == '"') ADVANCE(603);
      if (lookahead == '/') ADVANCE(606);
      if (lookahead == '7' ||
          lookahead == '9') ADVANCE(639);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(612);
      if (lookahead == 'a') ADVANCE(637);
      if (lookahead == 'd') ADVANCE(621);
      if (lookahead == 'm') ADVANCE(613);
      if (lookahead == 's') ADVANCE(638);
      if (('0' <= lookahead && lookahead <= '8') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 70:
      if (lookahead == '(') ADVANCE(9);
      if (lookahead == ')') ADVANCE(10);
      END_STATE();
    case 71:
      if (lookahead == '(') ADVANCE(52);
      if (lookahead == ')') ADVANCE(53);
      END_STATE();
    case 72:
      if (lookahead == '(') ADVANCE(55);
      if (lookahead == ')') ADVANCE(56);
      END_STATE();
    case 73:
      if (lookahead == '+') ADVANCE(4);
      if (lookahead == '0') ADVANCE(5);
      if (lookahead == '<') ADVANCE(70);
      if (lookahead == '>') ADVANCE(6);
      if (lookahead == 'D') ADVANCE(76);
      if (lookahead == 'a') ADVANCE(119);
      if (lookahead == 'b') ADVANCE(233);
      if (lookahead == 'c') ADVANCE(210);
      if (lookahead == 'd') ADVANCE(102);
      if (lookahead == 'e') ADVANCE(187);
      if (lookahead == 'f') ADVANCE(7);
      if (lookahead == 'i') ADVANCE(194);
      if (lookahead == 'l') ADVANCE(211);
      if (lookahead == 'm') ADVANCE(136);
      if (lookahead == 'o') ADVANCE(227);
      if (lookahead == 'p') ADVANCE(8);
      if (lookahead == 'r') ADVANCE(212);
      if (lookahead == 's') ADVANCE(134);
      if (lookahead == 't') ADVANCE(137);
      if (lookahead == 'u') ADVANCE(224);
      if (lookahead == 'w') ADVANCE(138);
      END_STATE();
    case 74:
      if (lookahead == '-') ADVANCE(100);
      END_STATE();
    case 75:
      if (lookahead == '-') ADVANCE(3);
      END_STATE();
    case 76:
      if (lookahead == '.') ADVANCE(99);
      END_STATE();
    case 77:
      if (lookahead == '.') ADVANCE(677);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 78:
      if (lookahead == '.') ADVANCE(20);
      END_STATE();
    case 79:
      if (lookahead == '.') ADVANCE(21);
      END_STATE();
    case 80:
      if (lookahead == '5') ADVANCE(46);
      END_STATE();
    case 81:
      if (lookahead == ':') ADVANCE(670);
      if (lookahead == '|') ADVANCE(667);
      END_STATE();
    case 82:
      if (lookahead == ':') ADVANCE(1123);
      END_STATE();
    case 83:
      if (lookahead == ':') ADVANCE(1124);
      END_STATE();
    case 84:
      if (lookahead == ':') ADVANCE(1127);
      END_STATE();
    case 85:
      if (lookahead == ':') ADVANCE(1134);
      END_STATE();
    case 86:
      if (lookahead == ':') ADVANCE(1131);
      END_STATE();
    case 87:
      if (lookahead == ':') ADVANCE(1138);
      END_STATE();
    case 88:
      if (lookahead == ':') ADVANCE(1146);
      END_STATE();
    case 89:
      if (lookahead == ':') ADVANCE(1148);
      END_STATE();
    case 90:
      if (lookahead == ':') ADVANCE(1125);
      END_STATE();
    case 91:
      if (lookahead == ':') ADVANCE(1130);
      END_STATE();
    case 92:
      if (lookahead == ':') ADVANCE(1133);
      END_STATE();
    case 93:
      if (lookahead == ':') ADVANCE(1147);
      END_STATE();
    case 94:
      if (lookahead == ':') ADVANCE(282);
      END_STATE();
    case 95:
      if (lookahead == ':') ADVANCE(282);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      END_STATE();
    case 96:
      if (lookahead == ':') ADVANCE(95);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(96);
      END_STATE();
    case 97:
      if (lookahead == ':') ADVANCE(94);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(97);
      END_STATE();
    case 98:
      if (lookahead == '=') ADVANCE(1145);
      if (lookahead == 'H') ADVANCE(74);
      if (('h' <= lookahead && lookahead <= 'w')) ADVANCE(1139);
      END_STATE();
    case 99:
      if (lookahead == 'C') ADVANCE(78);
      if (lookahead == 'S') ADVANCE(79);
      END_STATE();
    case 100:
      if (lookahead == 'W') ADVANCE(1144);
      END_STATE();
    case 101:
      if (lookahead == 'a') ADVANCE(257);
      END_STATE();
    case 102:
      if (lookahead == 'a') ADVANCE(120);
      if (lookahead == 'i') ADVANCE(188);
      if (lookahead == 'o') ADVANCE(275);
      END_STATE();
    case 103:
      if (lookahead == 'a') ADVANCE(250);
      END_STATE();
    case 104:
      if (lookahead == 'a') ADVANCE(229);
      END_STATE();
    case 105:
      if (lookahead == 'a') ADVANCE(253);
      END_STATE();
    case 106:
      if (lookahead == 'a') ADVANCE(230);
      if (lookahead == 'o') ADVANCE(128);
      END_STATE();
    case 107:
      if (lookahead == 'a') ADVANCE(264);
      END_STATE();
    case 108:
      if (lookahead == 'a') ADVANCE(22);
      END_STATE();
    case 109:
      if (lookahead == 'a') ADVANCE(42);
      END_STATE();
    case 110:
      if (lookahead == 'a') ADVANCE(48);
      END_STATE();
    case 111:
      if (lookahead == 'a') ADVANCE(64);
      END_STATE();
    case 112:
      if (lookahead == 'a') ADVANCE(180);
      END_STATE();
    case 113:
      if (lookahead == 'a') ADVANCE(254);
      END_STATE();
    case 114:
      if (lookahead == 'a') ADVANCE(266);
      END_STATE();
    case 115:
      if (lookahead == 'a') ADVANCE(255);
      END_STATE();
    case 116:
      if (lookahead == 'b') ADVANCE(213);
      if (lookahead == 'p') ADVANCE(159);
      END_STATE();
    case 117:
      if (lookahead == 'b') ADVANCE(34);
      END_STATE();
    case 118:
      if (lookahead == 'b') ADVANCE(220);
      END_STATE();
    case 119:
      if (lookahead == 'c') ADVANCE(121);
      if (lookahead == 'r') ADVANCE(228);
      END_STATE();
    case 120:
      if (lookahead == 'c') ADVANCE(106);
      END_STATE();
    case 121:
      if (lookahead == 'c') ADVANCE(144);
      END_STATE();
    case 122:
      if (lookahead == 'c') ADVANCE(146);
      END_STATE();
    case 123:
      if (lookahead == 'd') ADVANCE(160);
      END_STATE();
    case 124:
      if (lookahead == 'd') ADVANCE(165);
      END_STATE();
    case 125:
      if (lookahead == 'd') ADVANCE(108);
      END_STATE();
    case 126:
      if (lookahead == 'd') ADVANCE(171);
      END_STATE();
    case 127:
      if (lookahead == 'd') ADVANCE(219);
      END_STATE();
    case 128:
      if (lookahead == 'd') ADVANCE(109);
      END_STATE();
    case 129:
      if (lookahead == 'd') ADVANCE(148);
      END_STATE();
    case 130:
      if (lookahead == 'd') ADVANCE(154);
      END_STATE();
    case 131:
      if (lookahead == 'd') ADVANCE(155);
      END_STATE();
    case 132:
      if (lookahead == 'd') ADVANCE(156);
      END_STATE();
    case 133:
      if (lookahead == 'd') ADVANCE(221);
      END_STATE();
    case 134:
      if (lookahead == 'e') ADVANCE(163);
      if (lookahead == 'f') ADVANCE(278);
      if (lookahead == 'h') ADVANCE(214);
      if (lookahead == 'l') ADVANCE(172);
      if (lookahead == 'n') ADVANCE(104);
      END_STATE();
    case 135:
      if (lookahead == 'e') ADVANCE(256);
      END_STATE();
    case 136:
      if (lookahead == 'e') ADVANCE(126);
      if (lookahead == 'f') ADVANCE(14);
      if (lookahead == 'o') ADVANCE(236);
      if (lookahead == 'p') ADVANCE(15);
      END_STATE();
    case 137:
      if (lookahead == 'e') ADVANCE(199);
      if (lookahead == 'h') ADVANCE(270);
      if (lookahead == 'r') ADVANCE(175);
      if (lookahead == 'u') ADVANCE(239);
      END_STATE();
    case 138:
      if (lookahead == 'e') ADVANCE(124);
      END_STATE();
    case 139:
      if (lookahead == 'e') ADVANCE(101);
      END_STATE();
    case 140:
      if (lookahead == 'e') ADVANCE(161);
      END_STATE();
    case 141:
      if (lookahead == 'e') ADVANCE(205);
      END_STATE();
    case 142:
      if (lookahead == 'e') ADVANCE(238);
      END_STATE();
    case 143:
      if (lookahead == 'e') ADVANCE(123);
      END_STATE();
    case 144:
      if (lookahead == 'e') ADVANCE(200);
      END_STATE();
    case 145:
      if (lookahead == 'e') ADVANCE(242);
      END_STATE();
    case 146:
      if (lookahead == 'e') ADVANCE(204);
      END_STATE();
    case 147:
      if (lookahead == 'e') ADVANCE(24);
      END_STATE();
    case 148:
      if (lookahead == 'e') ADVANCE(33);
      END_STATE();
    case 149:
      if (lookahead == 'e') ADVANCE(38);
      END_STATE();
    case 150:
      if (lookahead == 'e') ADVANCE(54);
      END_STATE();
    case 151:
      if (lookahead == 'e') ADVANCE(57);
      END_STATE();
    case 152:
      if (lookahead == 'e') ADVANCE(60);
      END_STATE();
    case 153:
      if (lookahead == 'e') ADVANCE(237);
      END_STATE();
    case 154:
      if (lookahead == 'e') ADVANCE(206);
      END_STATE();
    case 155:
      if (lookahead == 'e') ADVANCE(207);
      END_STATE();
    case 156:
      if (lookahead == 'e') ADVANCE(208);
      END_STATE();
    case 157:
      if (lookahead == 'e') ADVANCE(209);
      END_STATE();
    case 158:
      if (lookahead == 'e') ADVANCE(248);
      END_STATE();
    case 159:
      if (lookahead == 'e') ADVANCE(249);
      END_STATE();
    case 160:
      if (lookahead == 'f') ADVANCE(158);
      if (lookahead == 't') ADVANCE(272);
      END_STATE();
    case 161:
      if (lookahead == 'g') ADVANCE(162);
      END_STATE();
    case 162:
      if (lookahead == 'g') ADVANCE(177);
      END_STATE();
    case 163:
      if (lookahead == 'g') ADVANCE(201);
      END_STATE();
    case 164:
      if (lookahead == 'g') ADVANCE(226);
      END_STATE();
    case 165:
      if (lookahead == 'g') ADVANCE(149);
      END_STATE();
    case 166:
      if (lookahead == 'h') ADVANCE(103);
      END_STATE();
    case 167:
      if (lookahead == 'h') ADVANCE(240);
      END_STATE();
    case 168:
      if (lookahead == 'h') ADVANCE(40);
      END_STATE();
    case 169:
      if (lookahead == 'h') ADVANCE(245);
      END_STATE();
    case 170:
      if (lookahead == 'h') ADVANCE(247);
      END_STATE();
    case 171:
      if (lookahead == 'i') ADVANCE(271);
      END_STATE();
    case 172:
      if (lookahead == 'i') ADVANCE(129);
      END_STATE();
    case 173:
      if (lookahead == 'i') ADVANCE(252);
      END_STATE();
    case 174:
      if (lookahead == 'i') ADVANCE(202);
      END_STATE();
    case 175:
      if (lookahead == 'i') ADVANCE(181);
      END_STATE();
    case 176:
      if (lookahead == 'i') ADVANCE(182);
      END_STATE();
    case 177:
      if (lookahead == 'i') ADVANCE(218);
      END_STATE();
    case 178:
      if (lookahead == 'l') ADVANCE(35);
      END_STATE();
    case 179:
      if (lookahead == 'l') ADVANCE(185);
      END_STATE();
    case 180:
      if (lookahead == 'l') ADVANCE(183);
      END_STATE();
    case 181:
      if (lookahead == 'l') ADVANCE(178);
      END_STATE();
    case 182:
      if (lookahead == 'l') ADVANCE(184);
      END_STATE();
    case 183:
      if (lookahead == 'l') ADVANCE(258);
      END_STATE();
    case 184:
      if (lookahead == 'l') ADVANCE(145);
      END_STATE();
    case 185:
      if (lookahead == 'l') ADVANCE(28);
      END_STATE();
    case 186:
      if (lookahead == 'm') ADVANCE(117);
      END_STATE();
    case 187:
      if (lookahead == 'm') ADVANCE(225);
      END_STATE();
    case 188:
      if (lookahead == 'm') ADVANCE(174);
      END_STATE();
    case 189:
      if (lookahead == 'm') ADVANCE(222);
      END_STATE();
    case 190:
      if (lookahead == 'm') ADVANCE(107);
      END_STATE();
    case 191:
      if (lookahead == 'm') ADVANCE(114);
      END_STATE();
    case 192:
      if (lookahead == 'm') ADVANCE(223);
      END_STATE();
    case 193:
      if (lookahead == 'm') ADVANCE(232);
      END_STATE();
    case 194:
      if (lookahead == 'n') ADVANCE(274);
      END_STATE();
    case 195:
      if (lookahead == 'n') ADVANCE(118);
      END_STATE();
    case 196:
      if (lookahead == 'n') ADVANCE(30);
      END_STATE();
    case 197:
      if (lookahead == 'n') ADVANCE(164);
      if (lookahead == 'w') ADVANCE(153);
      END_STATE();
    case 198:
      if (lookahead == 'n') ADVANCE(58);
      END_STATE();
    case 199:
      if (lookahead == 'n') ADVANCE(269);
      END_STATE();
    case 200:
      if (lookahead == 'n') ADVANCE(259);
      END_STATE();
    case 201:
      if (lookahead == 'n') ADVANCE(215);
      END_STATE();
    case 202:
      if (lookahead == 'n') ADVANCE(273);
      END_STATE();
    case 203:
      if (lookahead == 'n') ADVANCE(147);
      END_STATE();
    case 204:
      if (lookahead == 'n') ADVANCE(127);
      END_STATE();
    case 205:
      if (lookahead == 'n') ADVANCE(25);
      END_STATE();
    case 206:
      if (lookahead == 'n') ADVANCE(260);
      END_STATE();
    case 207:
      if (lookahead == 'n') ADVANCE(261);
      END_STATE();
    case 208:
      if (lookahead == 'n') ADVANCE(262);
      END_STATE();
    case 209:
      if (lookahead == 'n') ADVANCE(133);
      END_STATE();
    case 210:
      if (lookahead == 'o') ADVANCE(125);
      if (lookahead == 'r') ADVANCE(135);
      END_STATE();
    case 211:
      if (lookahead == 'o') ADVANCE(197);
      END_STATE();
    case 212:
      if (lookahead == 'o') ADVANCE(179);
      END_STATE();
    case 213:
      if (lookahead == 'o') ADVANCE(276);
      END_STATE();
    case 214:
      if (lookahead == 'o') ADVANCE(234);
      END_STATE();
    case 215:
      if (lookahead == 'o') ADVANCE(32);
      END_STATE();
    case 216:
      if (lookahead == 'o') ADVANCE(41);
      END_STATE();
    case 217:
      if (lookahead == 'o') ADVANCE(43);
      END_STATE();
    case 218:
      if (lookahead == 'o') ADVANCE(50);
      END_STATE();
    case 219:
      if (lookahead == 'o') ADVANCE(71);
      END_STATE();
    case 220:
      if (lookahead == 'o') ADVANCE(277);
      END_STATE();
    case 221:
      if (lookahead == 'o') ADVANCE(72);
      END_STATE();
    case 222:
      if (lookahead == 'o') ADVANCE(244);
      END_STATE();
    case 223:
      if (lookahead == 'o') ADVANCE(246);
      END_STATE();
    case 224:
      if (lookahead == 'p') ADVANCE(116);
      END_STATE();
    case 225:
      if (lookahead == 'p') ADVANCE(166);
      END_STATE();
    case 226:
      if (lookahead == 'p') ADVANCE(167);
      END_STATE();
    case 227:
      if (lookahead == 'p') ADVANCE(141);
      END_STATE();
    case 228:
      if (lookahead == 'p') ADVANCE(140);
      END_STATE();
    case 229:
      if (lookahead == 'p') ADVANCE(29);
      END_STATE();
    case 230:
      if (lookahead == 'p') ADVANCE(216);
      END_STATE();
    case 231:
      if (lookahead == 'p') ADVANCE(169);
      END_STATE();
    case 232:
      if (lookahead == 'p') ADVANCE(170);
      END_STATE();
    case 233:
      if (lookahead == 'r') ADVANCE(139);
      END_STATE();
    case 234:
      if (lookahead == 'r') ADVANCE(267);
      END_STATE();
    case 235:
      if (lookahead == 'r') ADVANCE(190);
      END_STATE();
    case 236:
      if (lookahead == 'r') ADVANCE(130);
      END_STATE();
    case 237:
      if (lookahead == 'r') ADVANCE(189);
      END_STATE();
    case 238:
      if (lookahead == 'r') ADVANCE(263);
      END_STATE();
    case 239:
      if (lookahead == 'r') ADVANCE(196);
      END_STATE();
    case 240:
      if (lookahead == 'r') ADVANCE(105);
      END_STATE();
    case 241:
      if (lookahead == 'r') ADVANCE(198);
      END_STATE();
    case 242:
      if (lookahead == 'r') ADVANCE(61);
      END_STATE();
    case 243:
      if (lookahead == 'r') ADVANCE(176);
      END_STATE();
    case 244:
      if (lookahead == 'r') ADVANCE(131);
      END_STATE();
    case 245:
      if (lookahead == 'r') ADVANCE(113);
      END_STATE();
    case 246:
      if (lookahead == 'r') ADVANCE(132);
      END_STATE();
    case 247:
      if (lookahead == 'r') ADVANCE(115);
      END_STATE();
    case 248:
      if (lookahead == 'r') ADVANCE(191);
      END_STATE();
    case 249:
      if (lookahead == 'r') ADVANCE(192);
      END_STATE();
    case 250:
      if (lookahead == 's') ADVANCE(173);
      END_STATE();
    case 251:
      if (lookahead == 's') ADVANCE(26);
      END_STATE();
    case 252:
      if (lookahead == 's') ADVANCE(51);
      END_STATE();
    case 253:
      if (lookahead == 's') ADVANCE(150);
      END_STATE();
    case 254:
      if (lookahead == 's') ADVANCE(151);
      END_STATE();
    case 255:
      if (lookahead == 's') ADVANCE(152);
      END_STATE();
    case 256:
      if (lookahead == 's') ADVANCE(122);
      END_STATE();
    case 257:
      if (lookahead == 't') ADVANCE(168);
      END_STATE();
    case 258:
      if (lookahead == 't') ADVANCE(243);
      END_STATE();
    case 259:
      if (lookahead == 't') ADVANCE(39);
      END_STATE();
    case 260:
      if (lookahead == 't') ADVANCE(49);
      END_STATE();
    case 261:
      if (lookahead == 't') ADVANCE(59);
      END_STATE();
    case 262:
      if (lookahead == 't') ADVANCE(62);
      END_STATE();
    case 263:
      if (lookahead == 't') ADVANCE(143);
      END_STATE();
    case 264:
      if (lookahead == 't') ADVANCE(110);
      END_STATE();
    case 265:
      if (lookahead == 't') ADVANCE(217);
      END_STATE();
    case 266:
      if (lookahead == 't') ADVANCE(111);
      END_STATE();
    case 267:
      if (lookahead == 't') ADVANCE(231);
      END_STATE();
    case 268:
      if (lookahead == 'u') ADVANCE(251);
      END_STATE();
    case 269:
      if (lookahead == 'u') ADVANCE(265);
      END_STATE();
    case 270:
      if (lookahead == 'u') ADVANCE(186);
      END_STATE();
    case 271:
      if (lookahead == 'u') ADVANCE(193);
      END_STATE();
    case 272:
      if (lookahead == 'u') ADVANCE(241);
      END_STATE();
    case 273:
      if (lookahead == 'u') ADVANCE(157);
      END_STATE();
    case 274:
      if (lookahead == 'v') ADVANCE(142);
      END_STATE();
    case 275:
      if (lookahead == 'w') ADVANCE(195);
      END_STATE();
    case 276:
      if (lookahead == 'w') ADVANCE(37);
      END_STATE();
    case 277:
      if (lookahead == 'w') ADVANCE(47);
      END_STATE();
    case 278:
      if (lookahead == 'z') ADVANCE(19);
      END_STATE();
    case 279:
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(599);
      END_STATE();
    case 280:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(609);
      END_STATE();
    case 281:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(96);
      END_STATE();
    case 282:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(611);
      END_STATE();
    case 283:
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 284:
      if (eof) ADVANCE(288);
      if (lookahead == '\n') ADVANCE(291);
      if (lookahead == '!') ADVANCE(73);
      if (lookahead == '"') ADVANCE(604);
      if (lookahead == '%') ADVANCE(567);
      if (lookahead == '(') ADVANCE(575);
      if (lookahead == ')') ADVANCE(576);
      if (lookahead == ',') ADVANCE(650);
      if (lookahead == '-') ADVANCE(555);
      if (lookahead == ':') ADVANCE(81);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == 'I') ADVANCE(82);
      if (lookahead == 'K') ADVANCE(83);
      if (lookahead == 'L') ADVANCE(643);
      if (lookahead == 'M') ADVANCE(644);
      if (lookahead == 'N') ADVANCE(84);
      if (lookahead == 'P') ADVANCE(646);
      if (lookahead == 'Q') ADVANCE(85);
      if (lookahead == 'R') ADVANCE(86);
      if (lookahead == 'T') ADVANCE(648);
      if (lookahead == 'U') ADVANCE(87);
      if (lookahead == 'V') ADVANCE(88);
      if (lookahead == 'W') ADVANCE(89);
      if (lookahead == 'X') ADVANCE(592);
      if (lookahead == 'Z') ADVANCE(590);
      if (lookahead == '[') ADVANCE(562);
      if (lookahead == '\\') ADVANCE(549);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(580);
      if (lookahead == 'm') ADVANCE(90);
      if (lookahead == 'r') ADVANCE(91);
      if (lookahead == 's') ADVANCE(92);
      if (lookahead == 'w') ADVANCE(93);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '|') ADVANCE(653);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      if (lookahead == '.' ||
          lookahead == 'H' ||
          ('O' <= lookahead && lookahead <= 'S') ||
          lookahead == 'u' ||
          lookahead == 'v' ||
          lookahead == '~') ADVANCE(640);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(673);
      if (('A' <= lookahead && lookahead <= 'G') ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      END_STATE();
    case 285:
      if (eof) ADVANCE(288);
      if (lookahead == '\n') ADVANCE(291);
      if (lookahead == '!') ADVANCE(73);
      if (lookahead == '"') ADVANCE(604);
      if (lookahead == '%') ADVANCE(567);
      if (lookahead == '(') ADVANCE(575);
      if (lookahead == ')') ADVANCE(576);
      if (lookahead == '-') ADVANCE(577);
      if (lookahead == '/') ADVANCE(596);
      if (lookahead == ':') ADVANCE(81);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == 'I') ADVANCE(82);
      if (lookahead == 'K') ADVANCE(83);
      if (lookahead == 'L') ADVANCE(643);
      if (lookahead == 'M') ADVANCE(644);
      if (lookahead == 'N') ADVANCE(84);
      if (lookahead == 'P') ADVANCE(646);
      if (lookahead == 'Q') ADVANCE(85);
      if (lookahead == 'R') ADVANCE(86);
      if (lookahead == 'T') ADVANCE(648);
      if (lookahead == 'U') ADVANCE(87);
      if (lookahead == 'V') ADVANCE(88);
      if (lookahead == 'W') ADVANCE(89);
      if (lookahead == 'X') ADVANCE(592);
      if (lookahead == 'Z') ADVANCE(590);
      if (lookahead == '[') ADVANCE(562);
      if (lookahead == '\\') ADVANCE(549);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(580);
      if (lookahead == '`') ADVANCE(560);
      if (lookahead == 'm') ADVANCE(90);
      if (lookahead == 'r') ADVANCE(91);
      if (lookahead == 's') ADVANCE(92);
      if (lookahead == 'w') ADVANCE(93);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '|') ADVANCE(653);
      if (lookahead == '\'' ||
          lookahead == ',') ADVANCE(588);
      if (('<' <= lookahead && lookahead <= '>')) ADVANCE(598);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      if (lookahead == '.' ||
          lookahead == 'H' ||
          ('O' <= lookahead && lookahead <= 'S') ||
          lookahead == 'u' ||
          lookahead == 'v' ||
          lookahead == '~') ADVANCE(640);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(597);
      if (('A' <= lookahead && lookahead <= 'G') ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      END_STATE();
    case 286:
      if (eof) ADVANCE(288);
      if (lookahead == '\n') ADVANCE(290);
      if (lookahead == '!') ADVANCE(73);
      if (lookahead == '"') ADVANCE(604);
      if (lookahead == '%') ADVANCE(566);
      if (lookahead == '(') ADVANCE(575);
      if (lookahead == ')') ADVANCE(576);
      if (lookahead == '*') ADVANCE(552);
      if (lookahead == ':') ADVANCE(81);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == 'A') ADVANCE(582);
      if (lookahead == 'B') ADVANCE(583);
      if (lookahead == 'C') ADVANCE(584);
      if (lookahead == 'D') ADVANCE(585);
      if (lookahead == 'F') ADVANCE(586);
      if (lookahead == 'G') ADVANCE(587);
      if (lookahead == 'H') ADVANCE(642);
      if (lookahead == 'I') ADVANCE(82);
      if (lookahead == 'K') ADVANCE(83);
      if (lookahead == 'L') ADVANCE(643);
      if (lookahead == 'M') ADVANCE(644);
      if (lookahead == 'N') ADVANCE(84);
      if (lookahead == 'O') ADVANCE(645);
      if (lookahead == 'P') ADVANCE(646);
      if (lookahead == 'Q') ADVANCE(85);
      if (lookahead == 'R') ADVANCE(86);
      if (lookahead == 'S') ADVANCE(647);
      if (lookahead == 'T') ADVANCE(648);
      if (lookahead == 'U') ADVANCE(87);
      if (lookahead == 'V') ADVANCE(88);
      if (lookahead == 'W') ADVANCE(89);
      if (lookahead == 'X') ADVANCE(592);
      if (lookahead == 'Z') ADVANCE(591);
      if (lookahead == '[') ADVANCE(562);
      if (lookahead == '\\') ADVANCE(549);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(580);
      if (lookahead == 'm') ADVANCE(90);
      if (lookahead == 'r') ADVANCE(91);
      if (lookahead == 's') ADVANCE(92);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '|') ADVANCE(653);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == '.' ||
          lookahead == 'u' ||
          lookahead == 'v' ||
          lookahead == '~') ADVANCE(640);
      if (lookahead == 'E' ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(594);
      END_STATE();
    case 287:
      if (eof) ADVANCE(288);
      if (lookahead == '\n') ADVANCE(290);
      if (lookahead == '!') ADVANCE(73);
      if (lookahead == '"') ADVANCE(604);
      if (lookahead == '%') ADVANCE(567);
      if (lookahead == '(') ADVANCE(575);
      if (lookahead == ')') ADVANCE(576);
      if (lookahead == ':') ADVANCE(81);
      if (lookahead == '=' ||
          (9837 <= lookahead && lookahead <= 9839) ||
          lookahead == 119082 ||
          lookahead == 119083) ADVANCE(578);
      if (lookahead == 'I') ADVANCE(82);
      if (lookahead == 'K') ADVANCE(83);
      if (lookahead == 'L') ADVANCE(643);
      if (lookahead == 'M') ADVANCE(644);
      if (lookahead == 'N') ADVANCE(84);
      if (lookahead == 'P') ADVANCE(646);
      if (lookahead == 'Q') ADVANCE(85);
      if (lookahead == 'R') ADVANCE(86);
      if (lookahead == 'T') ADVANCE(648);
      if (lookahead == 'U') ADVANCE(87);
      if (lookahead == 'V') ADVANCE(88);
      if (lookahead == 'W') ADVANCE(89);
      if (lookahead == 'X') ADVANCE(592);
      if (lookahead == 'Z') ADVANCE(590);
      if (lookahead == '[') ADVANCE(562);
      if (lookahead == '\\') ADVANCE(549);
      if (lookahead == ']') ADVANCE(565);
      if (lookahead == '^') ADVANCE(579);
      if (lookahead == '_') ADVANCE(580);
      if (lookahead == 'm') ADVANCE(90);
      if (lookahead == 'r') ADVANCE(91);
      if (lookahead == 's') ADVANCE(92);
      if (lookahead == 'w') ADVANCE(93);
      if (lookahead == '{') ADVANCE(601);
      if (lookahead == '|') ADVANCE(653);
      if (lookahead == '}') ADVANCE(602);
      if (lookahead == 'x' ||
          lookahead == 'z') ADVANCE(589);
      if (lookahead == '.' ||
          lookahead == 'H' ||
          ('O' <= lookahead && lookahead <= 'S') ||
          lookahead == 'u' ||
          lookahead == 'v' ||
          lookahead == '~') ADVANCE(640);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(649);
      if (('A' <= lookahead && lookahead <= 'G') ||
          ('a' <= lookahead && lookahead <= 'g')) ADVANCE(581);
      END_STATE();
    case 288:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 289:
      ACCEPT_TOKEN(sym__space);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      END_STATE();
    case 290:
      ACCEPT_TOKEN(sym__NL);
      END_STATE();
    case 291:
      ACCEPT_TOKEN(sym__NL);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '\n') ADVANCE(574);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(292);
      if (lookahead != 0 &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 293:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == ' ') ADVANCE(361);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 294:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == ' ') ADVANCE(322);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 295:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(740);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 296:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(293);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 297:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(719);
      if (lookahead == '(') ADVANCE(302);
      if (lookahead == ')') ADVANCE(303);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 298:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(788);
      if (lookahead == 'e') ADVANCE(502);
      if (lookahead == 'f') ADVANCE(304);
      if (lookahead == 'i') ADVANCE(470);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 299:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(779);
      if (lookahead == 'l') ADVANCE(535);
      if (lookahead == 'p') ADVANCE(307);
      if (lookahead == 'r') ADVANCE(379);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(806);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 301:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(812);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 302:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(818);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 303:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(824);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 304:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(791);
      if (lookahead == 'f') ADVANCE(308);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 305:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(785);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 306:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(782);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 307:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(776);
      if (lookahead == 'p') ADVANCE(309);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 308:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(794);
      if (lookahead == 'f') ADVANCE(314);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 309:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(773);
      if (lookahead == 'p') ADVANCE(318);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 310:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(800);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 311:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(836);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 312:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(833);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(830);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 314:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(797);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 315:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(845);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 316:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(761);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 317:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(743);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 318:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(770);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 319:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(701);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 320:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(746);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(704);
      if (lookahead == 'x') ADVANCE(327);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 322:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(365);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 323:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(827);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 324:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(749);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 325:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(764);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 326:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(680);
      if (lookahead == '(') ADVANCE(335);
      if (lookahead == ')') ADVANCE(336);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 327:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(707);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(755);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 329:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(752);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 330:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(722);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 331:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(767);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 332:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(842);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 333:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(839);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 334:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(734);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 335:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(683);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(686);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 337:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(737);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 338:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(758);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 339:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(728);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 340:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(695);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 341:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(716);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(725);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 343:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(803);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 344:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(809);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 345:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(854);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 346:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(815);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 347:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(821);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 348:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(848);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 349:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(710);
      if (lookahead == 'x') ADVANCE(354);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 350:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(689);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 351:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(851);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 352:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(698);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 353:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(692);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 354:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(713);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 355:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '!') ADVANCE(731);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 356:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '"') ADVANCE(548);
      if (lookahead == '%') ADVANCE(67);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(356);
      END_STATE();
    case 357:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '(') ADVANCE(300);
      if (lookahead == ')') ADVANCE(301);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '(') ADVANCE(343);
      if (lookahead == ')') ADVANCE(344);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 359:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '(') ADVANCE(346);
      if (lookahead == ')') ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 360:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '+') ADVANCE(295);
      if (lookahead == '0') ADVANCE(296);
      if (lookahead == '<') ADVANCE(357);
      if (lookahead == '>') ADVANCE(297);
      if (lookahead == 'D') ADVANCE(362);
      if (lookahead == 'a') ADVANCE(386);
      if (lookahead == 'b') ADVANCE(500);
      if (lookahead == 'c') ADVANCE(477);
      if (lookahead == 'd') ADVANCE(369);
      if (lookahead == 'e') ADVANCE(454);
      if (lookahead == 'f') ADVANCE(298);
      if (lookahead == 'i') ADVANCE(461);
      if (lookahead == 'l') ADVANCE(478);
      if (lookahead == 'm') ADVANCE(403);
      if (lookahead == 'o') ADVANCE(494);
      if (lookahead == 'p') ADVANCE(299);
      if (lookahead == 'r') ADVANCE(479);
      if (lookahead == 's') ADVANCE(401);
      if (lookahead == 't') ADVANCE(404);
      if (lookahead == 'u') ADVANCE(491);
      if (lookahead == 'w') ADVANCE(405);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 361:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '-') ADVANCE(294);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 362:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '.') ADVANCE(367);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 363:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '.') ADVANCE(311);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 364:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '.') ADVANCE(312);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 365:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '5') ADVANCE(337);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 366:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == ':') ADVANCE(672);
      if (lookahead == '|') ADVANCE(669);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 367:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'C') ADVANCE(363);
      if (lookahead == 'S') ADVANCE(364);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 368:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(524);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 369:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(387);
      if (lookahead == 'i') ADVANCE(455);
      if (lookahead == 'o') ADVANCE(542);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 370:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(517);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 371:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(496);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 372:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(520);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 373:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(497);
      if (lookahead == 'o') ADVANCE(395);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 374:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(531);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 375:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(313);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 376:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(333);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 377:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(339);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 378:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(355);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 379:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(447);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 380:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(521);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 381:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(533);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 382:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'a') ADVANCE(522);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 383:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'b') ADVANCE(480);
      if (lookahead == 'p') ADVANCE(426);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 384:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'b') ADVANCE(325);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 385:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'b') ADVANCE(487);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 386:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'c') ADVANCE(388);
      if (lookahead == 'r') ADVANCE(495);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 387:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'c') ADVANCE(373);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 388:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'c') ADVANCE(411);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 389:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'c') ADVANCE(413);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 390:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(427);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 391:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(432);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 392:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(375);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 393:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(438);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 394:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(486);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 395:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(376);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 396:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(415);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 397:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(421);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 398:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(422);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 399:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(423);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 400:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'd') ADVANCE(488);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 401:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(430);
      if (lookahead == 'f') ADVANCE(545);
      if (lookahead == 'h') ADVANCE(481);
      if (lookahead == 'l') ADVANCE(439);
      if (lookahead == 'n') ADVANCE(371);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 402:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(523);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 403:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(393);
      if (lookahead == 'f') ADVANCE(305);
      if (lookahead == 'o') ADVANCE(503);
      if (lookahead == 'p') ADVANCE(306);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 404:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(466);
      if (lookahead == 'h') ADVANCE(537);
      if (lookahead == 'r') ADVANCE(442);
      if (lookahead == 'u') ADVANCE(506);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 405:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(391);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 406:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(368);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 407:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(428);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 408:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(472);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 409:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(505);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 410:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(390);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 411:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(467);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 412:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(509);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 413:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(471);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 414:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(315);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 415:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(324);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 416:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(329);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 417:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(345);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 418:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 419:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(351);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 420:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(504);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 421:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(473);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 422:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(474);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 423:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(475);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 424:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(476);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 425:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(515);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 426:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'e') ADVANCE(516);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 427:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'f') ADVANCE(425);
      if (lookahead == 't') ADVANCE(539);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 428:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'g') ADVANCE(429);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 429:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'g') ADVANCE(444);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 430:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'g') ADVANCE(468);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 431:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'g') ADVANCE(493);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 432:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'g') ADVANCE(416);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 433:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'h') ADVANCE(370);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 434:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'h') ADVANCE(507);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 435:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'h') ADVANCE(331);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 436:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'h') ADVANCE(512);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 437:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'h') ADVANCE(514);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 438:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(538);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 439:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(396);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 440:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(519);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 441:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(469);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 442:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(448);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 443:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(449);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 444:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'i') ADVANCE(485);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 445:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(326);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 446:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(452);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 447:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(450);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 448:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(445);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 449:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(451);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 450:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(525);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 451:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(412);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 452:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'l') ADVANCE(319);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 453:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(384);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 454:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(492);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 455:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(441);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 456:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(489);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 457:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(374);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 458:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(381);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 459:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(490);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 460:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'm') ADVANCE(499);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 461:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(541);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 462:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(385);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 463:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(321);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 464:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(431);
      if (lookahead == 'w') ADVANCE(420);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 465:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(349);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 466:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(536);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 467:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(526);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 468:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(482);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 469:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(540);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 470:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(414);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 471:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(394);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 472:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(316);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 473:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(527);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 474:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(528);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 475:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(529);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 476:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'n') ADVANCE(400);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 477:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(392);
      if (lookahead == 'r') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 478:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(464);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 479:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(446);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 480:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(543);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 481:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(501);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 482:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(323);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 483:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(332);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 484:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(334);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 485:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(341);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 486:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(358);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 487:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(544);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 488:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(359);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 489:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(511);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 490:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'o') ADVANCE(513);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 491:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(383);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 492:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(433);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 493:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(434);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 494:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(408);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 495:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(407);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 496:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(320);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 497:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(483);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 498:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(436);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 499:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'p') ADVANCE(437);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 500:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(406);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 501:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(534);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 502:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(457);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 503:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(397);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 504:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(456);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 505:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(530);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 506:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(463);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 507:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(372);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 508:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(465);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 509:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(352);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 510:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(443);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 511:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(398);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 512:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(380);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 513:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(399);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 514:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(382);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 515:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(458);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 516:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'r') ADVANCE(459);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 517:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(440);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 518:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(317);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 519:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(342);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 520:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(417);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 521:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(418);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 522:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(419);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 523:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 's') ADVANCE(389);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 524:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(435);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 525:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(510);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 526:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(330);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 527:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(340);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 528:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(350);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 529:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(353);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 530:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(410);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 531:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(377);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 532:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(484);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 533:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(378);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 534:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 't') ADVANCE(498);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 535:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(518);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 536:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(532);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 537:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(453);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 538:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(460);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 539:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(508);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 540:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'u') ADVANCE(424);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 541:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'v') ADVANCE(409);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 542:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'w') ADVANCE(462);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 543:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'w') ADVANCE(328);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 544:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'w') ADVANCE(338);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 545:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == 'z') ADVANCE(310);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 546:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead == '|') ADVANCE(663);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 547:
      ACCEPT_TOKEN(sym_noCommentText);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(547);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 548:
      ACCEPT_TOKEN(sym_noCommentText);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 549:
      ACCEPT_TOKEN(anon_sym_BSLASH);
      END_STATE();
    case 550:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 551:
      ACCEPT_TOKEN(anon_sym_COLON);
      if (lookahead == ':') ADVANCE(670);
      if (lookahead == '|') ADVANCE(667);
      END_STATE();
    case 552:
      ACCEPT_TOKEN(sym_note_skip);
      END_STATE();
    case 553:
      ACCEPT_TOKEN(sym_note_skip);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 554:
      ACCEPT_TOKEN(sym_note_skip);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 555:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 556:
      ACCEPT_TOKEN(anon_sym_DASH);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 557:
      ACCEPT_TOKEN(anon_sym_DASH);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 558:
      ACCEPT_TOKEN(sym_UNDERSCORE);
      if (lookahead == '_') ADVANCE(578);
      END_STATE();
    case 559:
      ACCEPT_TOKEN(sym_TILDE);
      END_STATE();
    case 560:
      ACCEPT_TOKEN(sym_BEAM_SEPARATOR);
      END_STATE();
    case 561:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 562:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      if (lookahead == '|') ADVANCE(661);
      END_STATE();
    case 563:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      if (lookahead == '|') ADVANCE(663);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 564:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      if (lookahead == '|') ADVANCE(662);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 565:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 566:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead == '%') ADVANCE(572);
      if (lookahead == 'a') ADVANCE(569);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(571);
      END_STATE();
    case 567:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead == '%') ADVANCE(572);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(571);
      END_STATE();
    case 568:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead == '-') ADVANCE(1115);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(571);
      END_STATE();
    case 569:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead == 'b') ADVANCE(570);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(571);
      END_STATE();
    case 570:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead == 'c') ADVANCE(568);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(571);
      END_STATE();
    case 571:
      ACCEPT_TOKEN(sym_COMMENT);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(571);
      END_STATE();
    case 572:
      ACCEPT_TOKEN(anon_sym_PERCENT_PERCENT);
      END_STATE();
    case 573:
      ACCEPT_TOKEN(aux_sym__MUSIC_CODE_token1);
      if (lookahead == '\n') ADVANCE(574);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(573);
      if (lookahead != 0 &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 574:
      ACCEPT_TOKEN(aux_sym__MUSIC_CODE_token1);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(574);
      END_STATE();
    case 575:
      ACCEPT_TOKEN(sym_slur_open);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(609);
      END_STATE();
    case 576:
      ACCEPT_TOKEN(sym_slur_close);
      END_STATE();
    case 577:
      ACCEPT_TOKEN(aux_sym_note_construct_token1);
      END_STATE();
    case 578:
      ACCEPT_TOKEN(sym_alteration);
      END_STATE();
    case 579:
      ACCEPT_TOKEN(sym_alteration);
      if (lookahead == '^') ADVANCE(578);
      END_STATE();
    case 580:
      ACCEPT_TOKEN(sym_alteration);
      if (lookahead == '_') ADVANCE(578);
      END_STATE();
    case 581:
      ACCEPT_TOKEN(sym_note_letter);
      END_STATE();
    case 582:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1116);
      END_STATE();
    case 583:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1117);
      END_STATE();
    case 584:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1118);
      END_STATE();
    case 585:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1119);
      END_STATE();
    case 586:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1120);
      END_STATE();
    case 587:
      ACCEPT_TOKEN(sym_note_letter);
      if (lookahead == ':') ADVANCE(1121);
      END_STATE();
    case 588:
      ACCEPT_TOKEN(sym_octave);
      END_STATE();
    case 589:
      ACCEPT_TOKEN(sym_rest);
      END_STATE();
    case 590:
      ACCEPT_TOKEN(anon_sym_Z);
      END_STATE();
    case 591:
      ACCEPT_TOKEN(anon_sym_Z);
      if (lookahead == ':') ADVANCE(1135);
      END_STATE();
    case 592:
      ACCEPT_TOKEN(anon_sym_X);
      if (lookahead == ':') ADVANCE(1129);
      END_STATE();
    case 593:
      ACCEPT_TOKEN(aux_sym_multimeasure_rest_token1);
      if (lookahead == '/') ADVANCE(279);
      if (lookahead == '0') ADVANCE(594);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(593);
      END_STATE();
    case 594:
      ACCEPT_TOKEN(aux_sym_multimeasure_rest_token1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(594);
      END_STATE();
    case 595:
      ACCEPT_TOKEN(sym_rhythm);
      if (lookahead == '/') ADVANCE(595);
      END_STATE();
    case 596:
      ACCEPT_TOKEN(sym_rhythm);
      if (lookahead == '/') ADVANCE(595);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(599);
      END_STATE();
    case 597:
      ACCEPT_TOKEN(sym_rhythm);
      if (lookahead == '/') ADVANCE(279);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(597);
      END_STATE();
    case 598:
      ACCEPT_TOKEN(sym_rhythm);
      if (lookahead == '<' ||
          lookahead == '>') ADVANCE(598);
      END_STATE();
    case 599:
      ACCEPT_TOKEN(sym_rhythm);
      if (('1' <= lookahead && lookahead <= '9')) ADVANCE(599);
      END_STATE();
    case 600:
      ACCEPT_TOKEN(aux_sym_grace_note_token1);
      END_STATE();
    case 601:
      ACCEPT_TOKEN(aux_sym_grace_note_token1);
      if (lookahead == '/') ADVANCE(600);
      END_STATE();
    case 602:
      ACCEPT_TOKEN(aux_sym_grace_note_token2);
      END_STATE();
    case 603:
      ACCEPT_TOKEN(aux_sym_chord_symbol_token1);
      END_STATE();
    case 604:
      ACCEPT_TOKEN(aux_sym_chord_symbol_token1);
      if (lookahead == '"') ADVANCE(607);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(67);
      END_STATE();
    case 605:
      ACCEPT_TOKEN(aux_sym_chord_symbol_token1);
      if (lookahead == '"') ADVANCE(608);
      if (lookahead == '%' ||
          lookahead == ']') ADVANCE(67);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(923);
      END_STATE();
    case 606:
      ACCEPT_TOKEN(aux_sym_chord_symbol_token2);
      END_STATE();
    case 607:
      ACCEPT_TOKEN(sym_annotation);
      END_STATE();
    case 608:
      ACCEPT_TOKEN(sym_annotation);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 609:
      ACCEPT_TOKEN(sym_tuplet_marker);
      if (lookahead == ':') ADVANCE(281);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(610);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(609);
      END_STATE();
    case 610:
      ACCEPT_TOKEN(sym_tuplet_marker);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(610);
      END_STATE();
    case 611:
      ACCEPT_TOKEN(sym_tuplet_marker);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(610);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(611);
      END_STATE();
    case 612:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == '_') ADVANCE(639);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 613:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'a') ADVANCE(624);
      if (lookahead == 'i') ADVANCE(627);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 614:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'd') ADVANCE(639);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 615:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'd') ADVANCE(616);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 616:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'e') ADVANCE(614);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 617:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'e') ADVANCE(628);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 618:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'e') ADVANCE(629);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 619:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'g') ADVANCE(625);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 620:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'h') ADVANCE(616);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 621:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'i') ADVANCE(626);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 622:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'i') ADVANCE(630);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 623:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'i') ADVANCE(635);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 624:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'j') ADVANCE(631);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 625:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'm') ADVANCE(617);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 626:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'm') ADVANCE(622);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 627:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'n') ADVANCE(631);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 628:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'n') ADVANCE(636);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 629:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'n') ADVANCE(615);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 630:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'n') ADVANCE(623);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 631:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'o') ADVANCE(633);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 632:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'p') ADVANCE(618);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 633:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'r') ADVANCE(639);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 634:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 's') ADVANCE(632);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 635:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 's') ADVANCE(620);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 636:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 't') ADVANCE(616);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 637:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'u') ADVANCE(619);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 638:
      ACCEPT_TOKEN(sym_chord_type);
      if (lookahead == 'u') ADVANCE(634);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 639:
      ACCEPT_TOKEN(sym_chord_type);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(639);
      END_STATE();
    case 640:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      END_STATE();
    case 641:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == '-') ADVANCE(100);
      if (lookahead == ':') ADVANCE(1122);
      END_STATE();
    case 642:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1122);
      END_STATE();
    case 643:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1137);
      END_STATE();
    case 644:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1126);
      END_STATE();
    case 645:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1128);
      END_STATE();
    case 646:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(855);
      END_STATE();
    case 647:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1132);
      END_STATE();
    case 648:
      ACCEPT_TOKEN(sym_decoration_shorthand);
      if (lookahead == ':') ADVANCE(1136);
      END_STATE();
    case 649:
      ACCEPT_TOKEN(aux_sym_nth_ending_number_token1);
      END_STATE();
    case 650:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 651:
      ACCEPT_TOKEN(anon_sym_COMMA);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 652:
      ACCEPT_TOKEN(anon_sym_COMMA);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 653:
      ACCEPT_TOKEN(sym_bar_line);
      if (lookahead == ':') ADVANCE(664);
      if (lookahead == ']') ADVANCE(656);
      if (lookahead == '|') ADVANCE(658);
      END_STATE();
    case 654:
      ACCEPT_TOKEN(sym_bar_line);
      if (lookahead == ':') ADVANCE(666);
      if (lookahead == ']') ADVANCE(657);
      if (lookahead == '|') ADVANCE(660);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 655:
      ACCEPT_TOKEN(sym_bar_line);
      if (lookahead == ':') ADVANCE(665);
      if (lookahead == ']') ADVANCE(656);
      if (lookahead == '|') ADVANCE(659);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(1114);
      END_STATE();
    case 656:
      ACCEPT_TOKEN(sym_close_thin_thick_double_bar_line);
      END_STATE();
    case 657:
      ACCEPT_TOKEN(sym_close_thin_thick_double_bar_line);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 658:
      ACCEPT_TOKEN(sym_thin_double_bar_line);
      END_STATE();
    case 659:
      ACCEPT_TOKEN(sym_thin_double_bar_line);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 660:
      ACCEPT_TOKEN(sym_thin_double_bar_line);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 661:
      ACCEPT_TOKEN(sym_open_thick_thin_double_bar_line);
      END_STATE();
    case 662:
      ACCEPT_TOKEN(sym_open_thick_thin_double_bar_line);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 663:
      ACCEPT_TOKEN(sym_open_thick_thin_double_bar_line);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 664:
      ACCEPT_TOKEN(sym_start_of_repeated_section);
      END_STATE();
    case 665:
      ACCEPT_TOKEN(sym_start_of_repeated_section);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 666:
      ACCEPT_TOKEN(sym_start_of_repeated_section);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 667:
      ACCEPT_TOKEN(sym_end_of_repeated_section);
      END_STATE();
    case 668:
      ACCEPT_TOKEN(sym_end_of_repeated_section);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 669:
      ACCEPT_TOKEN(sym_end_of_repeated_section);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 670:
      ACCEPT_TOKEN(sym_start_end_of_two_repeated_sections);
      END_STATE();
    case 671:
      ACCEPT_TOKEN(sym_start_end_of_two_repeated_sections);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 672:
      ACCEPT_TOKEN(sym_start_end_of_two_repeated_sections);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 673:
      ACCEPT_TOKEN(aux_sym_first_repeat_bar_token2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(673);
      END_STATE();
    case 674:
      ACCEPT_TOKEN(aux_sym_first_repeat_bar_token2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(674);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 675:
      ACCEPT_TOKEN(aux_sym_lyric_line_token1);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(675);
      END_STATE();
    case 676:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 677:
      ACCEPT_TOKEN(anon_sym_DOT);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 678:
      ACCEPT_TOKEN(anon_sym_BANGtrill_BANG);
      END_STATE();
    case 679:
      ACCEPT_TOKEN(anon_sym_BANGtrill_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 680:
      ACCEPT_TOKEN(anon_sym_BANGtrill_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 681:
      ACCEPT_TOKEN(anon_sym_BANGtrill_LPAREN_BANG);
      END_STATE();
    case 682:
      ACCEPT_TOKEN(anon_sym_BANGtrill_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 683:
      ACCEPT_TOKEN(anon_sym_BANGtrill_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 684:
      ACCEPT_TOKEN(anon_sym_BANGtrill_RPAREN_BANG);
      END_STATE();
    case 685:
      ACCEPT_TOKEN(anon_sym_BANGtrill_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 686:
      ACCEPT_TOKEN(anon_sym_BANGtrill_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 687:
      ACCEPT_TOKEN(anon_sym_BANGlowermordent_BANG);
      END_STATE();
    case 688:
      ACCEPT_TOKEN(anon_sym_BANGlowermordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 689:
      ACCEPT_TOKEN(anon_sym_BANGlowermordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 690:
      ACCEPT_TOKEN(anon_sym_BANGuppermordent_BANG);
      END_STATE();
    case 691:
      ACCEPT_TOKEN(anon_sym_BANGuppermordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 692:
      ACCEPT_TOKEN(anon_sym_BANGuppermordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 693:
      ACCEPT_TOKEN(anon_sym_BANGmordent_BANG);
      END_STATE();
    case 694:
      ACCEPT_TOKEN(anon_sym_BANGmordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 695:
      ACCEPT_TOKEN(anon_sym_BANGmordent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 696:
      ACCEPT_TOKEN(anon_sym_BANGpralltriller_BANG);
      END_STATE();
    case 697:
      ACCEPT_TOKEN(anon_sym_BANGpralltriller_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 698:
      ACCEPT_TOKEN(anon_sym_BANGpralltriller_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 699:
      ACCEPT_TOKEN(anon_sym_BANGroll_BANG);
      END_STATE();
    case 700:
      ACCEPT_TOKEN(anon_sym_BANGroll_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 701:
      ACCEPT_TOKEN(anon_sym_BANGroll_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 702:
      ACCEPT_TOKEN(anon_sym_BANGturn_BANG);
      END_STATE();
    case 703:
      ACCEPT_TOKEN(anon_sym_BANGturn_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 704:
      ACCEPT_TOKEN(anon_sym_BANGturn_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 705:
      ACCEPT_TOKEN(anon_sym_BANGturnx_BANG);
      END_STATE();
    case 706:
      ACCEPT_TOKEN(anon_sym_BANGturnx_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 707:
      ACCEPT_TOKEN(anon_sym_BANGturnx_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 708:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturn_BANG);
      END_STATE();
    case 709:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturn_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 710:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturn_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 711:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturnx_BANG);
      END_STATE();
    case 712:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturnx_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 713:
      ACCEPT_TOKEN(anon_sym_BANGinvertedturnx_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 714:
      ACCEPT_TOKEN(anon_sym_BANGarpeggio_BANG);
      END_STATE();
    case 715:
      ACCEPT_TOKEN(anon_sym_BANGarpeggio_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 716:
      ACCEPT_TOKEN(anon_sym_BANGarpeggio_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 717:
      ACCEPT_TOKEN(anon_sym_BANG_GT_BANG);
      END_STATE();
    case 718:
      ACCEPT_TOKEN(anon_sym_BANG_GT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 719:
      ACCEPT_TOKEN(anon_sym_BANG_GT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 720:
      ACCEPT_TOKEN(anon_sym_BANGaccent_BANG);
      END_STATE();
    case 721:
      ACCEPT_TOKEN(anon_sym_BANGaccent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 722:
      ACCEPT_TOKEN(anon_sym_BANGaccent_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 723:
      ACCEPT_TOKEN(anon_sym_BANGemphasis_BANG);
      END_STATE();
    case 724:
      ACCEPT_TOKEN(anon_sym_BANGemphasis_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 725:
      ACCEPT_TOKEN(anon_sym_BANGemphasis_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 726:
      ACCEPT_TOKEN(anon_sym_BANGfermata_BANG);
      END_STATE();
    case 727:
      ACCEPT_TOKEN(anon_sym_BANGfermata_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 728:
      ACCEPT_TOKEN(anon_sym_BANGfermata_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 729:
      ACCEPT_TOKEN(anon_sym_BANGinvertedfermata_BANG);
      END_STATE();
    case 730:
      ACCEPT_TOKEN(anon_sym_BANGinvertedfermata_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 731:
      ACCEPT_TOKEN(anon_sym_BANGinvertedfermata_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 732:
      ACCEPT_TOKEN(anon_sym_BANGtenuto_BANG);
      END_STATE();
    case 733:
      ACCEPT_TOKEN(anon_sym_BANGtenuto_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 734:
      ACCEPT_TOKEN(anon_sym_BANGtenuto_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 735:
      ACCEPT_TOKEN(anon_sym_BANG0_BANG_DASH_BANG5_BANG);
      END_STATE();
    case 736:
      ACCEPT_TOKEN(anon_sym_BANG0_BANG_DASH_BANG5_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 737:
      ACCEPT_TOKEN(anon_sym_BANG0_BANG_DASH_BANG5_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 738:
      ACCEPT_TOKEN(anon_sym_BANG_PLUS_BANG);
      END_STATE();
    case 739:
      ACCEPT_TOKEN(anon_sym_BANG_PLUS_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 740:
      ACCEPT_TOKEN(anon_sym_BANG_PLUS_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 741:
      ACCEPT_TOKEN(anon_sym_BANGplus_BANG);
      END_STATE();
    case 742:
      ACCEPT_TOKEN(anon_sym_BANGplus_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 743:
      ACCEPT_TOKEN(anon_sym_BANGplus_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 744:
      ACCEPT_TOKEN(anon_sym_BANGsnap_BANG);
      END_STATE();
    case 745:
      ACCEPT_TOKEN(anon_sym_BANGsnap_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 746:
      ACCEPT_TOKEN(anon_sym_BANGsnap_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 747:
      ACCEPT_TOKEN(anon_sym_BANGslide_BANG);
      END_STATE();
    case 748:
      ACCEPT_TOKEN(anon_sym_BANGslide_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 749:
      ACCEPT_TOKEN(anon_sym_BANGslide_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 750:
      ACCEPT_TOKEN(anon_sym_BANGwedge_BANG);
      END_STATE();
    case 751:
      ACCEPT_TOKEN(anon_sym_BANGwedge_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 752:
      ACCEPT_TOKEN(anon_sym_BANGwedge_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 753:
      ACCEPT_TOKEN(anon_sym_BANGupbow_BANG);
      END_STATE();
    case 754:
      ACCEPT_TOKEN(anon_sym_BANGupbow_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 755:
      ACCEPT_TOKEN(anon_sym_BANGupbow_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 756:
      ACCEPT_TOKEN(anon_sym_BANGdownbow_BANG);
      END_STATE();
    case 757:
      ACCEPT_TOKEN(anon_sym_BANGdownbow_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 758:
      ACCEPT_TOKEN(anon_sym_BANGdownbow_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 759:
      ACCEPT_TOKEN(anon_sym_BANGopen_BANG);
      END_STATE();
    case 760:
      ACCEPT_TOKEN(anon_sym_BANGopen_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 761:
      ACCEPT_TOKEN(anon_sym_BANGopen_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 762:
      ACCEPT_TOKEN(anon_sym_BANGthumb_BANG);
      END_STATE();
    case 763:
      ACCEPT_TOKEN(anon_sym_BANGthumb_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 764:
      ACCEPT_TOKEN(anon_sym_BANGthumb_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 765:
      ACCEPT_TOKEN(anon_sym_BANGbreath_BANG);
      END_STATE();
    case 766:
      ACCEPT_TOKEN(anon_sym_BANGbreath_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 767:
      ACCEPT_TOKEN(anon_sym_BANGbreath_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 768:
      ACCEPT_TOKEN(anon_sym_BANGpppp_BANG);
      END_STATE();
    case 769:
      ACCEPT_TOKEN(anon_sym_BANGpppp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 770:
      ACCEPT_TOKEN(anon_sym_BANGpppp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 771:
      ACCEPT_TOKEN(anon_sym_BANGppp_BANG);
      END_STATE();
    case 772:
      ACCEPT_TOKEN(anon_sym_BANGppp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 773:
      ACCEPT_TOKEN(anon_sym_BANGppp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 774:
      ACCEPT_TOKEN(anon_sym_BANGpp_BANG);
      END_STATE();
    case 775:
      ACCEPT_TOKEN(anon_sym_BANGpp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 776:
      ACCEPT_TOKEN(anon_sym_BANGpp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 777:
      ACCEPT_TOKEN(anon_sym_BANGp_BANG);
      END_STATE();
    case 778:
      ACCEPT_TOKEN(anon_sym_BANGp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 779:
      ACCEPT_TOKEN(anon_sym_BANGp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 780:
      ACCEPT_TOKEN(anon_sym_BANGmp_BANG);
      END_STATE();
    case 781:
      ACCEPT_TOKEN(anon_sym_BANGmp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 782:
      ACCEPT_TOKEN(anon_sym_BANGmp_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 783:
      ACCEPT_TOKEN(anon_sym_BANGmf_BANG);
      END_STATE();
    case 784:
      ACCEPT_TOKEN(anon_sym_BANGmf_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 785:
      ACCEPT_TOKEN(anon_sym_BANGmf_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 786:
      ACCEPT_TOKEN(anon_sym_BANGf_BANG);
      END_STATE();
    case 787:
      ACCEPT_TOKEN(anon_sym_BANGf_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 788:
      ACCEPT_TOKEN(anon_sym_BANGf_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 789:
      ACCEPT_TOKEN(anon_sym_BANGff_BANG);
      END_STATE();
    case 790:
      ACCEPT_TOKEN(anon_sym_BANGff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 791:
      ACCEPT_TOKEN(anon_sym_BANGff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 792:
      ACCEPT_TOKEN(anon_sym_BANGfff_BANG);
      END_STATE();
    case 793:
      ACCEPT_TOKEN(anon_sym_BANGfff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 794:
      ACCEPT_TOKEN(anon_sym_BANGfff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 795:
      ACCEPT_TOKEN(anon_sym_BANGffff_BANG);
      END_STATE();
    case 796:
      ACCEPT_TOKEN(anon_sym_BANGffff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 797:
      ACCEPT_TOKEN(anon_sym_BANGffff_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 798:
      ACCEPT_TOKEN(anon_sym_BANGsfz_BANG);
      END_STATE();
    case 799:
      ACCEPT_TOKEN(anon_sym_BANGsfz_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 800:
      ACCEPT_TOKEN(anon_sym_BANGsfz_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 801:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_LPAREN_BANG);
      END_STATE();
    case 802:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 803:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 804:
      ACCEPT_TOKEN(anon_sym_BANG_LT_LPAREN_BANG);
      END_STATE();
    case 805:
      ACCEPT_TOKEN(anon_sym_BANG_LT_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 806:
      ACCEPT_TOKEN(anon_sym_BANG_LT_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 807:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_RPAREN_BANG);
      END_STATE();
    case 808:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 809:
      ACCEPT_TOKEN(anon_sym_BANGcrescendo_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 810:
      ACCEPT_TOKEN(anon_sym_BANG_LT_RPAREN_BANG);
      END_STATE();
    case 811:
      ACCEPT_TOKEN(anon_sym_BANG_LT_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 812:
      ACCEPT_TOKEN(anon_sym_BANG_LT_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 813:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_LPAREN_BANG);
      END_STATE();
    case 814:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 815:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 816:
      ACCEPT_TOKEN(anon_sym_BANG_GT_LPAREN_BANG);
      END_STATE();
    case 817:
      ACCEPT_TOKEN(anon_sym_BANG_GT_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 818:
      ACCEPT_TOKEN(anon_sym_BANG_GT_LPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 819:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_RPAREN_BANG);
      END_STATE();
    case 820:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 821:
      ACCEPT_TOKEN(anon_sym_BANGdiminuendo_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 822:
      ACCEPT_TOKEN(anon_sym_BANG_GT_RPAREN_BANG);
      END_STATE();
    case 823:
      ACCEPT_TOKEN(anon_sym_BANG_GT_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 824:
      ACCEPT_TOKEN(anon_sym_BANG_GT_RPAREN_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 825:
      ACCEPT_TOKEN(anon_sym_BANGsegno_BANG);
      END_STATE();
    case 826:
      ACCEPT_TOKEN(anon_sym_BANGsegno_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 827:
      ACCEPT_TOKEN(anon_sym_BANGsegno_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 828:
      ACCEPT_TOKEN(anon_sym_BANGcoda_BANG);
      END_STATE();
    case 829:
      ACCEPT_TOKEN(anon_sym_BANGcoda_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 830:
      ACCEPT_TOKEN(anon_sym_BANGcoda_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 831:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTS_DOT_BANG);
      END_STATE();
    case 832:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTS_DOT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 833:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTS_DOT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 834:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTC_DOT_BANG);
      END_STATE();
    case 835:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTC_DOT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 836:
      ACCEPT_TOKEN(anon_sym_BANGD_DOTC_DOT_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 837:
      ACCEPT_TOKEN(anon_sym_BANGdacoda_BANG);
      END_STATE();
    case 838:
      ACCEPT_TOKEN(anon_sym_BANGdacoda_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 839:
      ACCEPT_TOKEN(anon_sym_BANGdacoda_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 840:
      ACCEPT_TOKEN(anon_sym_BANGdacapo_BANG);
      END_STATE();
    case 841:
      ACCEPT_TOKEN(anon_sym_BANGdacapo_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 842:
      ACCEPT_TOKEN(anon_sym_BANGdacapo_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 843:
      ACCEPT_TOKEN(anon_sym_BANGfine_BANG);
      END_STATE();
    case 844:
      ACCEPT_TOKEN(anon_sym_BANGfine_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 845:
      ACCEPT_TOKEN(anon_sym_BANGfine_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 846:
      ACCEPT_TOKEN(anon_sym_BANGshortphrase_BANG);
      END_STATE();
    case 847:
      ACCEPT_TOKEN(anon_sym_BANGshortphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 848:
      ACCEPT_TOKEN(anon_sym_BANGshortphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 849:
      ACCEPT_TOKEN(anon_sym_BANGmediumphrase_BANG);
      END_STATE();
    case 850:
      ACCEPT_TOKEN(anon_sym_BANGmediumphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 851:
      ACCEPT_TOKEN(anon_sym_BANGmediumphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 852:
      ACCEPT_TOKEN(anon_sym_BANGlongphrase_BANG);
      END_STATE();
    case 853:
      ACCEPT_TOKEN(anon_sym_BANGlongphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 854:
      ACCEPT_TOKEN(anon_sym_BANGlongphrase_BANG);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(548);
      END_STATE();
    case 855:
      ACCEPT_TOKEN(sym_parts_line);
      END_STATE();
    case 856:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '\n') ADVANCE(574);
      if (lookahead == '!') ADVANCE(927);
      if (lookahead == '"') ADVANCE(605);
      if (lookahead == '*') ADVANCE(553);
      if (lookahead == ':') ADVANCE(933);
      if (lookahead == '[') ADVANCE(564);
      if (lookahead == '|') ADVANCE(655);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(573);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(674);
      if (lookahead != 0 &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 857:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '\n') ADVANCE(574);
      if (lookahead == '!') ADVANCE(927);
      if (lookahead == '"') ADVANCE(605);
      if (lookahead == '*') ADVANCE(553);
      if (lookahead == ':') ADVANCE(933);
      if (lookahead == '[') ADVANCE(564);
      if (lookahead == '|') ADVANCE(655);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(573);
      if (lookahead != 0 &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 858:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == ' ') ADVANCE(928);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 859:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == ' ') ADVANCE(889);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 860:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(927);
      if (lookahead == '"') ADVANCE(605);
      if (lookahead == '*') ADVANCE(553);
      if (lookahead == ',') ADVANCE(651);
      if (lookahead == '-') ADVANCE(556);
      if (lookahead == ':') ADVANCE(933);
      if (lookahead == '[') ADVANCE(1113);
      if (lookahead == '|') ADVANCE(655);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 861:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(927);
      if (lookahead == '"') ADVANCE(605);
      if (lookahead == '*') ADVANCE(553);
      if (lookahead == ':') ADVANCE(933);
      if (lookahead == '[') ADVANCE(1113);
      if (lookahead == '|') ADVANCE(655);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 862:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(739);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 863:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(858);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 864:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(718);
      if (lookahead == '(') ADVANCE(869);
      if (lookahead == ')') ADVANCE(870);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 865:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(787);
      if (lookahead == 'e') ADVANCE(1069);
      if (lookahead == 'f') ADVANCE(871);
      if (lookahead == 'i') ADVANCE(1037);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 866:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(778);
      if (lookahead == 'l') ADVANCE(1102);
      if (lookahead == 'p') ADVANCE(874);
      if (lookahead == 'r') ADVANCE(946);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 867:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(805);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 868:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(811);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 869:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(817);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 870:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(823);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 871:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(790);
      if (lookahead == 'f') ADVANCE(875);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 872:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(784);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 873:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(781);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 874:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(775);
      if (lookahead == 'p') ADVANCE(876);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 875:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(793);
      if (lookahead == 'f') ADVANCE(881);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 876:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(772);
      if (lookahead == 'p') ADVANCE(885);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 877:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(799);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 878:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(835);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 879:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(832);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 880:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(829);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 881:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(796);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 882:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(844);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 883:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(760);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 884:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(742);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 885:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(769);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 886:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(700);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 887:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(745);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 888:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(703);
      if (lookahead == 'x') ADVANCE(894);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 889:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(932);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 890:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(826);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 891:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(748);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 892:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(763);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 893:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(679);
      if (lookahead == '(') ADVANCE(902);
      if (lookahead == ')') ADVANCE(903);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 894:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(706);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 895:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(754);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 896:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(751);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 897:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(721);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 898:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(766);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 899:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(841);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 900:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(838);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 901:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(733);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 902:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(682);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 903:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(685);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 904:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(736);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 905:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(757);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 906:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(727);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 907:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(694);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 908:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(715);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 909:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(724);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 910:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(802);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 911:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(808);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 912:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(853);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 913:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(814);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 914:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(820);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 915:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(847);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 916:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(709);
      if (lookahead == 'x') ADVANCE(921);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 917:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(688);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 918:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(850);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 919:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(697);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 920:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(691);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 921:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(712);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 922:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '!') ADVANCE(730);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 923:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '"') ADVANCE(608);
      if (lookahead == '%' ||
          lookahead == ']') ADVANCE(67);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(923);
      END_STATE();
    case 924:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '(') ADVANCE(867);
      if (lookahead == ')') ADVANCE(868);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 925:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '(') ADVANCE(910);
      if (lookahead == ')') ADVANCE(911);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 926:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '(') ADVANCE(913);
      if (lookahead == ')') ADVANCE(914);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 927:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '+') ADVANCE(862);
      if (lookahead == '0') ADVANCE(863);
      if (lookahead == '<') ADVANCE(924);
      if (lookahead == '>') ADVANCE(864);
      if (lookahead == 'D') ADVANCE(929);
      if (lookahead == 'a') ADVANCE(953);
      if (lookahead == 'b') ADVANCE(1067);
      if (lookahead == 'c') ADVANCE(1044);
      if (lookahead == 'd') ADVANCE(936);
      if (lookahead == 'e') ADVANCE(1021);
      if (lookahead == 'f') ADVANCE(865);
      if (lookahead == 'i') ADVANCE(1028);
      if (lookahead == 'l') ADVANCE(1045);
      if (lookahead == 'm') ADVANCE(970);
      if (lookahead == 'o') ADVANCE(1061);
      if (lookahead == 'p') ADVANCE(866);
      if (lookahead == 'r') ADVANCE(1046);
      if (lookahead == 's') ADVANCE(968);
      if (lookahead == 't') ADVANCE(971);
      if (lookahead == 'u') ADVANCE(1058);
      if (lookahead == 'w') ADVANCE(972);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 928:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '-') ADVANCE(859);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 929:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '.') ADVANCE(934);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 930:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '.') ADVANCE(878);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 931:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '.') ADVANCE(879);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 932:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '5') ADVANCE(904);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 933:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == ':') ADVANCE(671);
      if (lookahead == '|') ADVANCE(668);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 934:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'C') ADVANCE(930);
      if (lookahead == 'S') ADVANCE(931);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 935:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1091);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 936:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(954);
      if (lookahead == 'i') ADVANCE(1022);
      if (lookahead == 'o') ADVANCE(1109);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 937:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1084);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 938:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1063);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 939:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1087);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 940:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1064);
      if (lookahead == 'o') ADVANCE(962);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 941:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1098);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 942:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(880);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 943:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(900);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 944:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(906);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 945:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(922);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 946:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1014);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 947:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1088);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 948:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1100);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 949:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'a') ADVANCE(1089);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 950:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'b') ADVANCE(1047);
      if (lookahead == 'p') ADVANCE(993);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 951:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'b') ADVANCE(892);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 952:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'b') ADVANCE(1054);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 953:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'c') ADVANCE(955);
      if (lookahead == 'r') ADVANCE(1062);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 954:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'c') ADVANCE(940);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 955:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'c') ADVANCE(978);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 956:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'c') ADVANCE(980);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 957:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(994);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 958:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(999);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 959:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(942);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 960:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(1005);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 961:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(1053);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 962:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(943);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 963:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(982);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 964:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(988);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 965:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(989);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 966:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(990);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 967:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'd') ADVANCE(1055);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 968:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(997);
      if (lookahead == 'f') ADVANCE(1112);
      if (lookahead == 'h') ADVANCE(1048);
      if (lookahead == 'l') ADVANCE(1006);
      if (lookahead == 'n') ADVANCE(938);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 969:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1090);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 970:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(960);
      if (lookahead == 'f') ADVANCE(872);
      if (lookahead == 'o') ADVANCE(1070);
      if (lookahead == 'p') ADVANCE(873);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 971:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1033);
      if (lookahead == 'h') ADVANCE(1104);
      if (lookahead == 'r') ADVANCE(1009);
      if (lookahead == 'u') ADVANCE(1073);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 972:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(958);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 973:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(935);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 974:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(995);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 975:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1039);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 976:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1072);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 977:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(957);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 978:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1034);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 979:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1076);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 980:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1038);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 981:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(882);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 982:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(891);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 983:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(896);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 984:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(912);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 985:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(915);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 986:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(918);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 987:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1071);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 988:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1040);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 989:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1041);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 990:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1042);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 991:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1043);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 992:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1082);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 993:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'e') ADVANCE(1083);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 994:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'f') ADVANCE(992);
      if (lookahead == 't') ADVANCE(1106);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 995:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'g') ADVANCE(996);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 996:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'g') ADVANCE(1011);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 997:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'g') ADVANCE(1035);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 998:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'g') ADVANCE(1060);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 999:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'g') ADVANCE(983);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1000:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'h') ADVANCE(937);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1001:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'h') ADVANCE(1074);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1002:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'h') ADVANCE(898);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1003:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'h') ADVANCE(1079);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1004:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'h') ADVANCE(1081);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1005:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1105);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1006:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(963);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1007:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1086);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1008:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1036);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1009:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1015);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1010:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1016);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1011:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'i') ADVANCE(1052);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1012:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(893);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1013:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(1019);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1014:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(1017);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1015:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(1012);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1016:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(1018);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1017:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(1092);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1018:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(979);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1019:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'l') ADVANCE(886);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1020:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(951);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1021:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(1059);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1022:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(1008);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1023:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(1056);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1024:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(941);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1025:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(948);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1026:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(1057);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1027:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'm') ADVANCE(1066);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1028:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1108);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1029:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(952);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1030:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(888);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1031:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(998);
      if (lookahead == 'w') ADVANCE(987);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1032:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(916);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1033:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1103);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1034:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1093);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1035:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1049);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1036:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1107);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1037:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(981);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1038:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(961);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1039:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(883);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1040:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1094);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1041:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1095);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1042:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(1096);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1043:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'n') ADVANCE(967);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1044:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(959);
      if (lookahead == 'r') ADVANCE(969);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1045:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1031);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1046:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1013);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1047:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1110);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1048:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1068);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1049:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(890);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1050:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(899);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1051:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(901);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1052:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(908);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1053:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(925);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1054:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1111);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1055:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(926);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1056:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1078);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1057:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'o') ADVANCE(1080);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1058:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(950);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1059:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(1000);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1060:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(1001);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1061:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(975);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1062:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(974);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1063:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(887);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1064:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(1050);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1065:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(1003);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1066:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'p') ADVANCE(1004);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1067:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(973);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1068:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1101);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1069:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1024);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1070:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(964);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1071:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1023);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1072:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1097);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1073:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1030);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1074:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(939);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1075:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1032);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1076:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(919);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1077:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1010);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1078:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(965);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1079:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(947);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1080:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(966);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1081:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(949);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1082:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1025);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1083:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'r') ADVANCE(1026);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1084:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(1007);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1085:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(884);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1086:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(909);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1087:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(984);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1088:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(985);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1089:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(986);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1090:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 's') ADVANCE(956);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1091:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(1002);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1092:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(1077);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1093:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(897);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1094:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(907);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1095:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(917);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1096:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(920);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1097:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(977);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1098:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(944);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1099:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(1051);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1100:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(945);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1101:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 't') ADVANCE(1065);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1102:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(1085);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1103:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(1099);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1104:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(1020);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1105:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(1027);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1106:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(1075);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1107:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'u') ADVANCE(991);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1108:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'v') ADVANCE(976);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1109:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'w') ADVANCE(1029);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1110:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'w') ADVANCE(895);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1111:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'w') ADVANCE(905);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1112:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == 'z') ADVANCE(877);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1113:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead == '|') ADVANCE(662);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1114:
      ACCEPT_TOKEN(aux_sym_body_inline_info_token1);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%' &&
          lookahead != ']') ADVANCE(1114);
      END_STATE();
    case 1115:
      ACCEPT_TOKEN(anon_sym_PERCENTabc_DASH);
      if (lookahead != 0 &&
          lookahead != '\n' &&
          lookahead != '%') ADVANCE(571);
      END_STATE();
    case 1116:
      ACCEPT_TOKEN(sym_area);
      END_STATE();
    case 1117:
      ACCEPT_TOKEN(sym_book);
      END_STATE();
    case 1118:
      ACCEPT_TOKEN(sym_composer);
      END_STATE();
    case 1119:
      ACCEPT_TOKEN(sym_discography);
      END_STATE();
    case 1120:
      ACCEPT_TOKEN(sym_file);
      END_STATE();
    case 1121:
      ACCEPT_TOKEN(sym_group);
      END_STATE();
    case 1122:
      ACCEPT_TOKEN(sym_history);
      END_STATE();
    case 1123:
      ACCEPT_TOKEN(sym_instruction);
      END_STATE();
    case 1124:
      ACCEPT_TOKEN(sym_key);
      END_STATE();
    case 1125:
      ACCEPT_TOKEN(sym_macros);
      END_STATE();
    case 1126:
      ACCEPT_TOKEN(sym_meter);
      END_STATE();
    case 1127:
      ACCEPT_TOKEN(sym_notes);
      END_STATE();
    case 1128:
      ACCEPT_TOKEN(sym_origin);
      END_STATE();
    case 1129:
      ACCEPT_TOKEN(anon_sym_X_COLON);
      END_STATE();
    case 1130:
      ACCEPT_TOKEN(sym_remark);
      END_STATE();
    case 1131:
      ACCEPT_TOKEN(sym_rhythm_line);
      END_STATE();
    case 1132:
      ACCEPT_TOKEN(sym_source);
      END_STATE();
    case 1133:
      ACCEPT_TOKEN(anon_sym_s_COLON);
      END_STATE();
    case 1134:
      ACCEPT_TOKEN(sym_tempo);
      END_STATE();
    case 1135:
      ACCEPT_TOKEN(sym_transcription);
      END_STATE();
    case 1136:
      ACCEPT_TOKEN(sym_tune_title);
      END_STATE();
    case 1137:
      ACCEPT_TOKEN(sym_unit_note_length);
      END_STATE();
    case 1138:
      ACCEPT_TOKEN(anon_sym_U_COLON);
      END_STATE();
    case 1139:
      ACCEPT_TOKEN(aux_sym_user_defined_token1);
      END_STATE();
    case 1140:
      ACCEPT_TOKEN(aux_sym_user_defined_token1);
      if (lookahead == ':') ADVANCE(1125);
      END_STATE();
    case 1141:
      ACCEPT_TOKEN(aux_sym_user_defined_token1);
      if (lookahead == ':') ADVANCE(1130);
      END_STATE();
    case 1142:
      ACCEPT_TOKEN(aux_sym_user_defined_token1);
      if (lookahead == ':') ADVANCE(1133);
      END_STATE();
    case 1143:
      ACCEPT_TOKEN(aux_sym_user_defined_token1);
      if (lookahead == ':') ADVANCE(1147);
      END_STATE();
    case 1144:
      ACCEPT_TOKEN(aux_sym_user_defined_token2);
      END_STATE();
    case 1145:
      ACCEPT_TOKEN(anon_sym_EQ);
      END_STATE();
    case 1146:
      ACCEPT_TOKEN(sym_voice);
      END_STATE();
    case 1147:
      ACCEPT_TOKEN(sym_words_tag);
      END_STATE();
    case 1148:
      ACCEPT_TOKEN(sym_words_postbody);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 286},
  [2] = {.lex_state = 287},
  [3] = {.lex_state = 287},
  [4] = {.lex_state = 287},
  [5] = {.lex_state = 286},
  [6] = {.lex_state = 286},
  [7] = {.lex_state = 286},
  [8] = {.lex_state = 285},
  [9] = {.lex_state = 285},
  [10] = {.lex_state = 286},
  [11] = {.lex_state = 286},
  [12] = {.lex_state = 286},
  [13] = {.lex_state = 287},
  [14] = {.lex_state = 285},
  [15] = {.lex_state = 285},
  [16] = {.lex_state = 285},
  [17] = {.lex_state = 285},
  [18] = {.lex_state = 285},
  [19] = {.lex_state = 285},
  [20] = {.lex_state = 284},
  [21] = {.lex_state = 284},
  [22] = {.lex_state = 284},
  [23] = {.lex_state = 284},
  [24] = {.lex_state = 284},
  [25] = {.lex_state = 285},
  [26] = {.lex_state = 285},
  [27] = {.lex_state = 285},
  [28] = {.lex_state = 285},
  [29] = {.lex_state = 285},
  [30] = {.lex_state = 285},
  [31] = {.lex_state = 285},
  [32] = {.lex_state = 285},
  [33] = {.lex_state = 285},
  [34] = {.lex_state = 285},
  [35] = {.lex_state = 285},
  [36] = {.lex_state = 284},
  [37] = {.lex_state = 285},
  [38] = {.lex_state = 285},
  [39] = {.lex_state = 285},
  [40] = {.lex_state = 285},
  [41] = {.lex_state = 285},
  [42] = {.lex_state = 285},
  [43] = {.lex_state = 285},
  [44] = {.lex_state = 285},
  [45] = {.lex_state = 285},
  [46] = {.lex_state = 285},
  [47] = {.lex_state = 285},
  [48] = {.lex_state = 285},
  [49] = {.lex_state = 285},
  [50] = {.lex_state = 285},
  [51] = {.lex_state = 285},
  [52] = {.lex_state = 285},
  [53] = {.lex_state = 285},
  [54] = {.lex_state = 285},
  [55] = {.lex_state = 285},
  [56] = {.lex_state = 285},
  [57] = {.lex_state = 285},
  [58] = {.lex_state = 285},
  [59] = {.lex_state = 285},
  [60] = {.lex_state = 285},
  [61] = {.lex_state = 285},
  [62] = {.lex_state = 287},
  [63] = {.lex_state = 287},
  [64] = {.lex_state = 287},
  [65] = {.lex_state = 66},
  [66] = {.lex_state = 861},
  [67] = {.lex_state = 861},
  [68] = {.lex_state = 66},
  [69] = {.lex_state = 286},
  [70] = {.lex_state = 286},
  [71] = {.lex_state = 1},
  [72] = {.lex_state = 1},
  [73] = {.lex_state = 856},
  [74] = {.lex_state = 856},
  [75] = {.lex_state = 860},
  [76] = {.lex_state = 860},
  [77] = {.lex_state = 1},
  [78] = {.lex_state = 65},
  [79] = {.lex_state = 857},
  [80] = {.lex_state = 65},
  [81] = {.lex_state = 65},
  [82] = {.lex_state = 860},
  [83] = {.lex_state = 65},
  [84] = {.lex_state = 860},
  [85] = {.lex_state = 66},
  [86] = {.lex_state = 66},
  [87] = {.lex_state = 66},
  [88] = {.lex_state = 66},
  [89] = {.lex_state = 861},
  [90] = {.lex_state = 66},
  [91] = {.lex_state = 66},
  [92] = {.lex_state = 861},
  [93] = {.lex_state = 66},
  [94] = {.lex_state = 861},
  [95] = {.lex_state = 66},
  [96] = {.lex_state = 66},
  [97] = {.lex_state = 861},
  [98] = {.lex_state = 861},
  [99] = {.lex_state = 66},
  [100] = {.lex_state = 861},
  [101] = {.lex_state = 861},
  [102] = {.lex_state = 66},
  [103] = {.lex_state = 861},
  [104] = {.lex_state = 861},
  [105] = {.lex_state = 861},
  [106] = {.lex_state = 861},
  [107] = {.lex_state = 861},
  [108] = {.lex_state = 66},
  [109] = {.lex_state = 0},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 286},
  [112] = {.lex_state = 286},
  [113] = {.lex_state = 286},
  [114] = {.lex_state = 286},
  [115] = {.lex_state = 68},
  [116] = {.lex_state = 287},
  [117] = {.lex_state = 68},
  [118] = {.lex_state = 68},
  [119] = {.lex_state = 68},
  [120] = {.lex_state = 68},
  [121] = {.lex_state = 68},
  [122] = {.lex_state = 68},
  [123] = {.lex_state = 68},
  [124] = {.lex_state = 68},
  [125] = {.lex_state = 287},
  [126] = {.lex_state = 287},
  [127] = {.lex_state = 287},
  [128] = {.lex_state = 287},
  [129] = {.lex_state = 287},
  [130] = {.lex_state = 287},
  [131] = {.lex_state = 287},
  [132] = {.lex_state = 287},
  [133] = {.lex_state = 287},
  [134] = {.lex_state = 287},
  [135] = {.lex_state = 68},
  [136] = {.lex_state = 287},
  [137] = {.lex_state = 287},
  [138] = {.lex_state = 287},
  [139] = {.lex_state = 287},
  [140] = {.lex_state = 287},
  [141] = {.lex_state = 287},
  [142] = {.lex_state = 287},
  [143] = {.lex_state = 287},
  [144] = {.lex_state = 68},
  [145] = {.lex_state = 287},
  [146] = {.lex_state = 287},
  [147] = {.lex_state = 287},
  [148] = {.lex_state = 68},
  [149] = {.lex_state = 68},
  [150] = {.lex_state = 68},
  [151] = {.lex_state = 68},
  [152] = {.lex_state = 68},
  [153] = {.lex_state = 68},
  [154] = {.lex_state = 68},
  [155] = {.lex_state = 68},
  [156] = {.lex_state = 68},
  [157] = {.lex_state = 68},
  [158] = {.lex_state = 68},
  [159] = {.lex_state = 68},
  [160] = {.lex_state = 68},
  [161] = {.lex_state = 287},
  [162] = {.lex_state = 287},
  [163] = {.lex_state = 68},
  [164] = {.lex_state = 68},
  [165] = {.lex_state = 68},
  [166] = {.lex_state = 68},
  [167] = {.lex_state = 68},
  [168] = {.lex_state = 287},
  [169] = {.lex_state = 287},
  [170] = {.lex_state = 68},
  [171] = {.lex_state = 68},
  [172] = {.lex_state = 287},
  [173] = {.lex_state = 68},
  [174] = {.lex_state = 68},
  [175] = {.lex_state = 287},
  [176] = {.lex_state = 287},
  [177] = {.lex_state = 287},
  [178] = {.lex_state = 287},
  [179] = {.lex_state = 287},
  [180] = {.lex_state = 287},
  [181] = {.lex_state = 286},
  [182] = {.lex_state = 68},
  [183] = {.lex_state = 286},
  [184] = {.lex_state = 287},
  [185] = {.lex_state = 287},
  [186] = {.lex_state = 287},
  [187] = {.lex_state = 287},
  [188] = {.lex_state = 287},
  [189] = {.lex_state = 287},
  [190] = {.lex_state = 287},
  [191] = {.lex_state = 287},
  [192] = {.lex_state = 287},
  [193] = {.lex_state = 0},
  [194] = {.lex_state = 0},
  [195] = {.lex_state = 287},
  [196] = {.lex_state = 287},
  [197] = {.lex_state = 287},
  [198] = {.lex_state = 287},
  [199] = {.lex_state = 287},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 287},
  [202] = {.lex_state = 286},
  [203] = {.lex_state = 286},
  [204] = {.lex_state = 287},
  [205] = {.lex_state = 287},
  [206] = {.lex_state = 287},
  [207] = {.lex_state = 287},
  [208] = {.lex_state = 0},
  [209] = {.lex_state = 287},
  [210] = {.lex_state = 286},
  [211] = {.lex_state = 287},
  [212] = {.lex_state = 287},
  [213] = {.lex_state = 286},
  [214] = {.lex_state = 69},
  [215] = {.lex_state = 286},
  [216] = {.lex_state = 286},
  [217] = {.lex_state = 286},
  [218] = {.lex_state = 77},
  [219] = {.lex_state = 77},
  [220] = {.lex_state = 69},
  [221] = {.lex_state = 69},
  [222] = {.lex_state = 77},
  [223] = {.lex_state = 286},
  [224] = {.lex_state = 69},
  [225] = {.lex_state = 69},
  [226] = {.lex_state = 68},
  [227] = {.lex_state = 287},
  [228] = {.lex_state = 69},
  [229] = {.lex_state = 98},
  [230] = {.lex_state = 0},
  [231] = {.lex_state = 98},
  [232] = {.lex_state = 0},
  [233] = {.lex_state = 69},
  [234] = {.lex_state = 287},
  [235] = {.lex_state = 287},
  [236] = {.lex_state = 69},
  [237] = {.lex_state = 0},
  [238] = {.lex_state = 287},
  [239] = {.lex_state = 287},
  [240] = {.lex_state = 287},
  [241] = {.lex_state = 287},
  [242] = {.lex_state = 286},
  [243] = {.lex_state = 287},
  [244] = {.lex_state = 287},
  [245] = {.lex_state = 287},
  [246] = {.lex_state = 77},
  [247] = {.lex_state = 287},
  [248] = {.lex_state = 287},
  [249] = {.lex_state = 287},
  [250] = {.lex_state = 286},
  [251] = {.lex_state = 287},
  [252] = {.lex_state = 675},
  [253] = {.lex_state = 0},
  [254] = {.lex_state = 287},
  [255] = {.lex_state = 1114},
  [256] = {.lex_state = 287},
  [257] = {.lex_state = 98},
  [258] = {.lex_state = 675},
  [259] = {.lex_state = 283},
  [260] = {.lex_state = 286},
  [261] = {.lex_state = 286},
  [262] = {.lex_state = 1114},
  [263] = {.lex_state = 287},
  [264] = {.lex_state = 283},
  [265] = {.lex_state = 287},
  [266] = {.lex_state = 286},
  [267] = {.lex_state = 287},
  [268] = {.lex_state = 1114},
  [269] = {.lex_state = 0},
  [270] = {.lex_state = 286},
  [271] = {.lex_state = 0},
  [272] = {.lex_state = 287},
  [273] = {.lex_state = 284},
  [274] = {.lex_state = 284},
  [275] = {.lex_state = 68},
  [276] = {.lex_state = 287},
  [277] = {.lex_state = 68},
  [278] = {.lex_state = 68},
  [279] = {.lex_state = 283},
  [280] = {.lex_state = 0},
  [281] = {.lex_state = 287},
  [282] = {.lex_state = 284},
  [283] = {.lex_state = 0},
  [284] = {.lex_state = 284},
  [285] = {.lex_state = 284},
  [286] = {.lex_state = 68},
  [287] = {.lex_state = 287},
  [288] = {.lex_state = 68},
  [289] = {.lex_state = 98},
  [290] = {.lex_state = 286},
  [291] = {.lex_state = 287},
  [292] = {.lex_state = 283},
  [293] = {.lex_state = 0},
  [294] = {.lex_state = 0},
  [295] = {.lex_state = 284},
  [296] = {.lex_state = 286},
  [297] = {.lex_state = 283},
  [298] = {.lex_state = 0},
  [299] = {.lex_state = 0},
  [300] = {.lex_state = 68},
  [301] = {.lex_state = 286},
  [302] = {.lex_state = 283},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [sym__space] = ACTIONS(1),
    [sym__NL] = ACTIONS(1),
    [anon_sym_BSLASH] = ACTIONS(1),
    [anon_sym_PLUS] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [sym_note_skip] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [sym_UNDERSCORE] = ACTIONS(1),
    [sym_TILDE] = ACTIONS(1),
    [sym_BEAM_SEPARATOR] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [sym_COMMENT] = ACTIONS(1),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(1),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(1),
    [sym_slur_open] = ACTIONS(1),
    [sym_slur_close] = ACTIONS(1),
    [aux_sym_note_construct_token1] = ACTIONS(1),
    [sym_alteration] = ACTIONS(1),
    [sym_note_letter] = ACTIONS(1),
    [sym_octave] = ACTIONS(1),
    [sym_rest] = ACTIONS(1),
    [anon_sym_Z] = ACTIONS(1),
    [anon_sym_X] = ACTIONS(1),
    [aux_sym_multimeasure_rest_token1] = ACTIONS(1),
    [sym_rhythm] = ACTIONS(1),
    [aux_sym_grace_note_token1] = ACTIONS(1),
    [aux_sym_grace_note_token2] = ACTIONS(1),
    [aux_sym_chord_symbol_token1] = ACTIONS(1),
    [aux_sym_chord_symbol_token2] = ACTIONS(1),
    [sym_annotation] = ACTIONS(1),
    [sym_tuplet_marker] = ACTIONS(1),
    [sym_decoration_shorthand] = ACTIONS(1),
    [aux_sym_nth_ending_number_token1] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [sym_bar_line] = ACTIONS(1),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(1),
    [sym_thin_double_bar_line] = ACTIONS(1),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(1),
    [sym_start_of_repeated_section] = ACTIONS(1),
    [sym_end_of_repeated_section] = ACTIONS(1),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(1),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(1),
    [aux_sym_verse_number_token1] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [anon_sym_BANGtrill_BANG] = ACTIONS(1),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(1),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(1),
    [anon_sym_BANGmordent_BANG] = ACTIONS(1),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(1),
    [anon_sym_BANGroll_BANG] = ACTIONS(1),
    [anon_sym_BANGturn_BANG] = ACTIONS(1),
    [anon_sym_BANGturnx_BANG] = ACTIONS(1),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(1),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(1),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(1),
    [anon_sym_BANG_GT_BANG] = ACTIONS(1),
    [anon_sym_BANGaccent_BANG] = ACTIONS(1),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(1),
    [anon_sym_BANGfermata_BANG] = ACTIONS(1),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(1),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(1),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(1),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(1),
    [anon_sym_BANGplus_BANG] = ACTIONS(1),
    [anon_sym_BANGsnap_BANG] = ACTIONS(1),
    [anon_sym_BANGslide_BANG] = ACTIONS(1),
    [anon_sym_BANGwedge_BANG] = ACTIONS(1),
    [anon_sym_BANGupbow_BANG] = ACTIONS(1),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(1),
    [anon_sym_BANGopen_BANG] = ACTIONS(1),
    [anon_sym_BANGthumb_BANG] = ACTIONS(1),
    [anon_sym_BANGbreath_BANG] = ACTIONS(1),
    [anon_sym_BANGpppp_BANG] = ACTIONS(1),
    [anon_sym_BANGppp_BANG] = ACTIONS(1),
    [anon_sym_BANGpp_BANG] = ACTIONS(1),
    [anon_sym_BANGp_BANG] = ACTIONS(1),
    [anon_sym_BANGmp_BANG] = ACTIONS(1),
    [anon_sym_BANGmf_BANG] = ACTIONS(1),
    [anon_sym_BANGf_BANG] = ACTIONS(1),
    [anon_sym_BANGff_BANG] = ACTIONS(1),
    [anon_sym_BANGfff_BANG] = ACTIONS(1),
    [anon_sym_BANGffff_BANG] = ACTIONS(1),
    [anon_sym_BANGsfz_BANG] = ACTIONS(1),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(1),
    [anon_sym_BANGsegno_BANG] = ACTIONS(1),
    [anon_sym_BANGcoda_BANG] = ACTIONS(1),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(1),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(1),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(1),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(1),
    [anon_sym_BANGfine_BANG] = ACTIONS(1),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(1),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(1),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(1),
    [sym_parts_line] = ACTIONS(1),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(1),
    [sym_area] = ACTIONS(1),
    [sym_book] = ACTIONS(1),
    [sym_composer] = ACTIONS(1),
    [sym_discography] = ACTIONS(1),
    [sym_file] = ACTIONS(1),
    [sym_group] = ACTIONS(1),
    [sym_history] = ACTIONS(1),
    [sym_instruction] = ACTIONS(1),
    [sym_key] = ACTIONS(1),
    [sym_macros] = ACTIONS(1),
    [sym_meter] = ACTIONS(1),
    [sym_notes] = ACTIONS(1),
    [sym_origin] = ACTIONS(1),
    [anon_sym_X_COLON] = ACTIONS(1),
    [sym_remark] = ACTIONS(1),
    [sym_rhythm_line] = ACTIONS(1),
    [sym_source] = ACTIONS(1),
    [anon_sym_s_COLON] = ACTIONS(1),
    [sym_tempo] = ACTIONS(1),
    [sym_transcription] = ACTIONS(1),
    [sym_tune_title] = ACTIONS(1),
    [sym_unit_note_length] = ACTIONS(1),
    [anon_sym_U_COLON] = ACTIONS(1),
    [aux_sym_user_defined_token1] = ACTIONS(1),
    [aux_sym_user_defined_token2] = ACTIONS(1),
    [anon_sym_EQ] = ACTIONS(1),
    [sym_voice] = ACTIONS(1),
    [sym_words_tag] = ACTIONS(1),
    [sym_words_postbody] = ACTIONS(1),
  },
  [1] = {
    [sym_source_file] = STATE(271),
    [sym_stylesheet_directives] = STATE(242),
    [sym_file_structure] = STATE(269),
    [sym_file_header] = STATE(208),
    [sym_tune] = STATE(210),
    [sym_tune_header] = STATE(13),
    [sym_abc_version] = STATE(279),
    [sym_reference_number_line] = STATE(7),
    [sym_user_defined] = STATE(279),
    [sym_file_header_info_line] = STATE(242),
    [aux_sym_file_structure_repeat2] = STATE(193),
    [aux_sym_file_header_repeat1] = STATE(112),
    [sym_COMMENT] = ACTIONS(3),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(5),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(7),
    [sym_area] = ACTIONS(9),
    [sym_book] = ACTIONS(9),
    [sym_composer] = ACTIONS(9),
    [sym_discography] = ACTIONS(9),
    [sym_file] = ACTIONS(9),
    [sym_group] = ACTIONS(9),
    [sym_history] = ACTIONS(9),
    [sym_instruction] = ACTIONS(9),
    [sym_macros] = ACTIONS(9),
    [sym_meter] = ACTIONS(9),
    [sym_notes] = ACTIONS(9),
    [sym_origin] = ACTIONS(9),
    [anon_sym_X_COLON] = ACTIONS(11),
    [sym_remark] = ACTIONS(9),
    [sym_rhythm_line] = ACTIONS(9),
    [sym_source] = ACTIONS(9),
    [sym_transcription] = ACTIONS(9),
    [sym_unit_note_length] = ACTIONS(9),
    [anon_sym_U_COLON] = ACTIONS(13),
  },
  [2] = {
    [sym_open_chord] = STATE(139),
    [sym_stylesheet_directives] = STATE(3),
    [sym__MUSIC_CODE] = STATE(3),
    [sym__music_content] = STATE(54),
    [sym__nte_or_chrd] = STATE(115),
    [sym_beam] = STATE(54),
    [sym_note_construct] = STATE(115),
    [sym__chord_cstrct] = STATE(115),
    [sym_note] = STATE(152),
    [sym__pitch] = STATE(135),
    [sym_multimeasure_rest] = STATE(54),
    [sym_grace_note] = STATE(124),
    [sym_chord_symbol] = STATE(172),
    [sym_decoration] = STATE(186),
    [sym_nth_ending_barline] = STATE(59),
    [sym_generic_bar_line] = STATE(54),
    [sym_first_repeat_bar] = STATE(59),
    [sym_second_repeat_bar] = STATE(59),
    [sym_symbol] = STATE(54),
    [sym_body_inline_info] = STATE(54),
    [sym_symbol_line] = STATE(297),
    [sym_user_defined] = STATE(297),
    [sym_body_info_line] = STATE(296),
    [aux_sym_tune_body_repeat1] = STATE(3),
    [aux_sym_note_construct_repeat1] = STATE(124),
    [ts_builtin_sym_end] = ACTIONS(15),
    [sym__NL] = ACTIONS(15),
    [anon_sym_BSLASH] = ACTIONS(17),
    [anon_sym_LBRACK] = ACTIONS(19),
    [sym_COMMENT] = ACTIONS(21),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(5),
    [sym_slur_open] = ACTIONS(23),
    [sym_slur_close] = ACTIONS(17),
    [sym_alteration] = ACTIONS(25),
    [sym_note_letter] = ACTIONS(27),
    [sym_rest] = ACTIONS(29),
    [anon_sym_Z] = ACTIONS(31),
    [anon_sym_X] = ACTIONS(33),
    [aux_sym_grace_note_token1] = ACTIONS(35),
    [aux_sym_chord_symbol_token1] = ACTIONS(37),
    [sym_annotation] = ACTIONS(17),
    [sym_tuplet_marker] = ACTIONS(39),
    [sym_decoration_shorthand] = ACTIONS(41),
    [sym_bar_line] = ACTIONS(43),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(45),
    [sym_thin_double_bar_line] = ACTIONS(45),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(45),
    [sym_start_of_repeated_section] = ACTIONS(47),
    [sym_end_of_repeated_section] = ACTIONS(49),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(47),
    [anon_sym_BANGtrill_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGmordent_BANG] = ACTIONS(51),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(51),
    [anon_sym_BANGroll_BANG] = ACTIONS(51),
    [anon_sym_BANGturn_BANG] = ACTIONS(51),
    [anon_sym_BANGturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_BANG] = ACTIONS(51),
    [anon_sym_BANGaccent_BANG] = ACTIONS(51),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(51),
    [anon_sym_BANGfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(51),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(51),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(51),
    [anon_sym_BANGplus_BANG] = ACTIONS(51),
    [anon_sym_BANGsnap_BANG] = ACTIONS(51),
    [anon_sym_BANGslide_BANG] = ACTIONS(51),
    [anon_sym_BANGwedge_BANG] = ACTIONS(51),
    [anon_sym_BANGupbow_BANG] = ACTIONS(51),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(51),
    [anon_sym_BANGopen_BANG] = ACTIONS(51),
    [anon_sym_BANGthumb_BANG] = ACTIONS(51),
    [anon_sym_BANGbreath_BANG] = ACTIONS(51),
    [anon_sym_BANGpppp_BANG] = ACTIONS(51),
    [anon_sym_BANGppp_BANG] = ACTIONS(51),
    [anon_sym_BANGpp_BANG] = ACTIONS(51),
    [anon_sym_BANGp_BANG] = ACTIONS(51),
    [anon_sym_BANGmp_BANG] = ACTIONS(51),
    [anon_sym_BANGmf_BANG] = ACTIONS(51),
    [anon_sym_BANGf_BANG] = ACTIONS(51),
    [anon_sym_BANGff_BANG] = ACTIONS(51),
    [anon_sym_BANGfff_BANG] = ACTIONS(51),
    [anon_sym_BANGffff_BANG] = ACTIONS(51),
    [anon_sym_BANGsfz_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGsegno_BANG] = ACTIONS(51),
    [anon_sym_BANGcoda_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(51),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(51),
    [anon_sym_BANGfine_BANG] = ACTIONS(51),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(51),
    [sym_parts_line] = ACTIONS(53),
    [sym_instruction] = ACTIONS(53),
    [sym_key] = ACTIONS(53),
    [sym_macros] = ACTIONS(53),
    [sym_meter] = ACTIONS(53),
    [sym_notes] = ACTIONS(53),
    [anon_sym_X_COLON] = ACTIONS(15),
    [sym_remark] = ACTIONS(53),
    [sym_rhythm_line] = ACTIONS(53),
    [anon_sym_s_COLON] = ACTIONS(55),
    [sym_tempo] = ACTIONS(53),
    [sym_tune_title] = ACTIONS(53),
    [sym_unit_note_length] = ACTIONS(53),
    [anon_sym_U_COLON] = ACTIONS(13),
    [sym_voice] = ACTIONS(53),
    [sym_words_tag] = ACTIONS(53),
    [sym_words_postbody] = ACTIONS(15),
  },
  [3] = {
    [sym_open_chord] = STATE(139),
    [sym_stylesheet_directives] = STATE(4),
    [sym__MUSIC_CODE] = STATE(4),
    [sym__music_content] = STATE(54),
    [sym__nte_or_chrd] = STATE(115),
    [sym_beam] = STATE(54),
    [sym_note_construct] = STATE(115),
    [sym__chord_cstrct] = STATE(115),
    [sym_note] = STATE(152),
    [sym__pitch] = STATE(135),
    [sym_multimeasure_rest] = STATE(54),
    [sym_grace_note] = STATE(124),
    [sym_chord_symbol] = STATE(172),
    [sym_decoration] = STATE(186),
    [sym_nth_ending_barline] = STATE(59),
    [sym_generic_bar_line] = STATE(54),
    [sym_first_repeat_bar] = STATE(59),
    [sym_second_repeat_bar] = STATE(59),
    [sym_symbol] = STATE(54),
    [sym_body_inline_info] = STATE(54),
    [sym_symbol_line] = STATE(297),
    [sym_user_defined] = STATE(297),
    [sym_body_info_line] = STATE(296),
    [aux_sym_tune_body_repeat1] = STATE(4),
    [aux_sym_note_construct_repeat1] = STATE(124),
    [ts_builtin_sym_end] = ACTIONS(57),
    [sym__NL] = ACTIONS(57),
    [anon_sym_BSLASH] = ACTIONS(17),
    [anon_sym_LBRACK] = ACTIONS(19),
    [sym_COMMENT] = ACTIONS(21),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(5),
    [sym_slur_open] = ACTIONS(23),
    [sym_slur_close] = ACTIONS(17),
    [sym_alteration] = ACTIONS(25),
    [sym_note_letter] = ACTIONS(27),
    [sym_rest] = ACTIONS(29),
    [anon_sym_Z] = ACTIONS(31),
    [anon_sym_X] = ACTIONS(33),
    [aux_sym_grace_note_token1] = ACTIONS(35),
    [aux_sym_chord_symbol_token1] = ACTIONS(37),
    [sym_annotation] = ACTIONS(17),
    [sym_tuplet_marker] = ACTIONS(39),
    [sym_decoration_shorthand] = ACTIONS(41),
    [sym_bar_line] = ACTIONS(43),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(45),
    [sym_thin_double_bar_line] = ACTIONS(45),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(45),
    [sym_start_of_repeated_section] = ACTIONS(47),
    [sym_end_of_repeated_section] = ACTIONS(49),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(47),
    [anon_sym_BANGtrill_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGmordent_BANG] = ACTIONS(51),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(51),
    [anon_sym_BANGroll_BANG] = ACTIONS(51),
    [anon_sym_BANGturn_BANG] = ACTIONS(51),
    [anon_sym_BANGturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_BANG] = ACTIONS(51),
    [anon_sym_BANGaccent_BANG] = ACTIONS(51),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(51),
    [anon_sym_BANGfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(51),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(51),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(51),
    [anon_sym_BANGplus_BANG] = ACTIONS(51),
    [anon_sym_BANGsnap_BANG] = ACTIONS(51),
    [anon_sym_BANGslide_BANG] = ACTIONS(51),
    [anon_sym_BANGwedge_BANG] = ACTIONS(51),
    [anon_sym_BANGupbow_BANG] = ACTIONS(51),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(51),
    [anon_sym_BANGopen_BANG] = ACTIONS(51),
    [anon_sym_BANGthumb_BANG] = ACTIONS(51),
    [anon_sym_BANGbreath_BANG] = ACTIONS(51),
    [anon_sym_BANGpppp_BANG] = ACTIONS(51),
    [anon_sym_BANGppp_BANG] = ACTIONS(51),
    [anon_sym_BANGpp_BANG] = ACTIONS(51),
    [anon_sym_BANGp_BANG] = ACTIONS(51),
    [anon_sym_BANGmp_BANG] = ACTIONS(51),
    [anon_sym_BANGmf_BANG] = ACTIONS(51),
    [anon_sym_BANGf_BANG] = ACTIONS(51),
    [anon_sym_BANGff_BANG] = ACTIONS(51),
    [anon_sym_BANGfff_BANG] = ACTIONS(51),
    [anon_sym_BANGffff_BANG] = ACTIONS(51),
    [anon_sym_BANGsfz_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGsegno_BANG] = ACTIONS(51),
    [anon_sym_BANGcoda_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(51),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(51),
    [anon_sym_BANGfine_BANG] = ACTIONS(51),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(51),
    [sym_parts_line] = ACTIONS(53),
    [sym_instruction] = ACTIONS(53),
    [sym_key] = ACTIONS(53),
    [sym_macros] = ACTIONS(53),
    [sym_meter] = ACTIONS(53),
    [sym_notes] = ACTIONS(53),
    [anon_sym_X_COLON] = ACTIONS(57),
    [sym_remark] = ACTIONS(53),
    [sym_rhythm_line] = ACTIONS(53),
    [anon_sym_s_COLON] = ACTIONS(55),
    [sym_tempo] = ACTIONS(53),
    [sym_tune_title] = ACTIONS(53),
    [sym_unit_note_length] = ACTIONS(53),
    [anon_sym_U_COLON] = ACTIONS(13),
    [sym_voice] = ACTIONS(53),
    [sym_words_tag] = ACTIONS(53),
    [sym_words_postbody] = ACTIONS(57),
  },
  [4] = {
    [sym_open_chord] = STATE(139),
    [sym_stylesheet_directives] = STATE(4),
    [sym__MUSIC_CODE] = STATE(4),
    [sym__music_content] = STATE(54),
    [sym__nte_or_chrd] = STATE(115),
    [sym_beam] = STATE(54),
    [sym_note_construct] = STATE(115),
    [sym__chord_cstrct] = STATE(115),
    [sym_note] = STATE(152),
    [sym__pitch] = STATE(135),
    [sym_multimeasure_rest] = STATE(54),
    [sym_grace_note] = STATE(124),
    [sym_chord_symbol] = STATE(172),
    [sym_decoration] = STATE(186),
    [sym_nth_ending_barline] = STATE(59),
    [sym_generic_bar_line] = STATE(54),
    [sym_first_repeat_bar] = STATE(59),
    [sym_second_repeat_bar] = STATE(59),
    [sym_symbol] = STATE(54),
    [sym_body_inline_info] = STATE(54),
    [sym_symbol_line] = STATE(297),
    [sym_user_defined] = STATE(297),
    [sym_body_info_line] = STATE(296),
    [aux_sym_tune_body_repeat1] = STATE(4),
    [aux_sym_note_construct_repeat1] = STATE(124),
    [ts_builtin_sym_end] = ACTIONS(59),
    [sym__NL] = ACTIONS(59),
    [anon_sym_BSLASH] = ACTIONS(61),
    [anon_sym_LBRACK] = ACTIONS(64),
    [sym_COMMENT] = ACTIONS(67),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(70),
    [sym_slur_open] = ACTIONS(73),
    [sym_slur_close] = ACTIONS(61),
    [sym_alteration] = ACTIONS(76),
    [sym_note_letter] = ACTIONS(79),
    [sym_rest] = ACTIONS(82),
    [anon_sym_Z] = ACTIONS(85),
    [anon_sym_X] = ACTIONS(88),
    [aux_sym_grace_note_token1] = ACTIONS(91),
    [aux_sym_chord_symbol_token1] = ACTIONS(94),
    [sym_annotation] = ACTIONS(61),
    [sym_tuplet_marker] = ACTIONS(97),
    [sym_decoration_shorthand] = ACTIONS(100),
    [sym_bar_line] = ACTIONS(103),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(106),
    [sym_thin_double_bar_line] = ACTIONS(106),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(106),
    [sym_start_of_repeated_section] = ACTIONS(109),
    [sym_end_of_repeated_section] = ACTIONS(112),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(109),
    [anon_sym_BANGtrill_BANG] = ACTIONS(115),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(115),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(115),
    [anon_sym_BANGmordent_BANG] = ACTIONS(115),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(115),
    [anon_sym_BANGroll_BANG] = ACTIONS(115),
    [anon_sym_BANGturn_BANG] = ACTIONS(115),
    [anon_sym_BANGturnx_BANG] = ACTIONS(115),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(115),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(115),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(115),
    [anon_sym_BANG_GT_BANG] = ACTIONS(115),
    [anon_sym_BANGaccent_BANG] = ACTIONS(115),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(115),
    [anon_sym_BANGfermata_BANG] = ACTIONS(115),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(115),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(115),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(115),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(115),
    [anon_sym_BANGplus_BANG] = ACTIONS(115),
    [anon_sym_BANGsnap_BANG] = ACTIONS(115),
    [anon_sym_BANGslide_BANG] = ACTIONS(115),
    [anon_sym_BANGwedge_BANG] = ACTIONS(115),
    [anon_sym_BANGupbow_BANG] = ACTIONS(115),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(115),
    [anon_sym_BANGopen_BANG] = ACTIONS(115),
    [anon_sym_BANGthumb_BANG] = ACTIONS(115),
    [anon_sym_BANGbreath_BANG] = ACTIONS(115),
    [anon_sym_BANGpppp_BANG] = ACTIONS(115),
    [anon_sym_BANGppp_BANG] = ACTIONS(115),
    [anon_sym_BANGpp_BANG] = ACTIONS(115),
    [anon_sym_BANGp_BANG] = ACTIONS(115),
    [anon_sym_BANGmp_BANG] = ACTIONS(115),
    [anon_sym_BANGmf_BANG] = ACTIONS(115),
    [anon_sym_BANGf_BANG] = ACTIONS(115),
    [anon_sym_BANGff_BANG] = ACTIONS(115),
    [anon_sym_BANGfff_BANG] = ACTIONS(115),
    [anon_sym_BANGffff_BANG] = ACTIONS(115),
    [anon_sym_BANGsfz_BANG] = ACTIONS(115),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(115),
    [anon_sym_BANGsegno_BANG] = ACTIONS(115),
    [anon_sym_BANGcoda_BANG] = ACTIONS(115),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(115),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(115),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(115),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(115),
    [anon_sym_BANGfine_BANG] = ACTIONS(115),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(115),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(115),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(115),
    [sym_parts_line] = ACTIONS(118),
    [sym_instruction] = ACTIONS(118),
    [sym_key] = ACTIONS(118),
    [sym_macros] = ACTIONS(118),
    [sym_meter] = ACTIONS(118),
    [sym_notes] = ACTIONS(118),
    [anon_sym_X_COLON] = ACTIONS(59),
    [sym_remark] = ACTIONS(118),
    [sym_rhythm_line] = ACTIONS(118),
    [anon_sym_s_COLON] = ACTIONS(121),
    [sym_tempo] = ACTIONS(118),
    [sym_tune_title] = ACTIONS(118),
    [sym_unit_note_length] = ACTIONS(118),
    [anon_sym_U_COLON] = ACTIONS(124),
    [sym_voice] = ACTIONS(118),
    [sym_words_tag] = ACTIONS(118),
    [sym_words_postbody] = ACTIONS(59),
  },
  [5] = {
    [sym_abc_version] = STATE(264),
    [sym_symbol_line] = STATE(264),
    [sym_user_defined] = STATE(264),
    [sym_tune_header_info_line] = STATE(261),
    [aux_sym_tune_header_repeat1] = STATE(6),
    [ts_builtin_sym_end] = ACTIONS(127),
    [sym__NL] = ACTIONS(127),
    [anon_sym_BSLASH] = ACTIONS(127),
    [anon_sym_LBRACK] = ACTIONS(129),
    [sym_COMMENT] = ACTIONS(131),
    [sym_slur_open] = ACTIONS(129),
    [sym_slur_close] = ACTIONS(127),
    [sym_alteration] = ACTIONS(127),
    [sym_note_letter] = ACTIONS(129),
    [sym_rest] = ACTIONS(127),
    [anon_sym_Z] = ACTIONS(129),
    [anon_sym_X] = ACTIONS(129),
    [aux_sym_grace_note_token1] = ACTIONS(127),
    [aux_sym_chord_symbol_token1] = ACTIONS(129),
    [sym_annotation] = ACTIONS(127),
    [sym_tuplet_marker] = ACTIONS(127),
    [sym_decoration_shorthand] = ACTIONS(129),
    [sym_bar_line] = ACTIONS(129),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(127),
    [sym_thin_double_bar_line] = ACTIONS(127),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(127),
    [sym_start_of_repeated_section] = ACTIONS(127),
    [sym_end_of_repeated_section] = ACTIONS(127),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(127),
    [anon_sym_BANGtrill_BANG] = ACTIONS(127),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(127),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(127),
    [anon_sym_BANGmordent_BANG] = ACTIONS(127),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(127),
    [anon_sym_BANGroll_BANG] = ACTIONS(127),
    [anon_sym_BANGturn_BANG] = ACTIONS(127),
    [anon_sym_BANGturnx_BANG] = ACTIONS(127),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(127),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(127),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(127),
    [anon_sym_BANG_GT_BANG] = ACTIONS(127),
    [anon_sym_BANGaccent_BANG] = ACTIONS(127),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(127),
    [anon_sym_BANGfermata_BANG] = ACTIONS(127),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(127),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(127),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(127),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(127),
    [anon_sym_BANGplus_BANG] = ACTIONS(127),
    [anon_sym_BANGsnap_BANG] = ACTIONS(127),
    [anon_sym_BANGslide_BANG] = ACTIONS(127),
    [anon_sym_BANGwedge_BANG] = ACTIONS(127),
    [anon_sym_BANGupbow_BANG] = ACTIONS(127),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(127),
    [anon_sym_BANGopen_BANG] = ACTIONS(127),
    [anon_sym_BANGthumb_BANG] = ACTIONS(127),
    [anon_sym_BANGbreath_BANG] = ACTIONS(127),
    [anon_sym_BANGpppp_BANG] = ACTIONS(127),
    [anon_sym_BANGppp_BANG] = ACTIONS(127),
    [anon_sym_BANGpp_BANG] = ACTIONS(127),
    [anon_sym_BANGp_BANG] = ACTIONS(127),
    [anon_sym_BANGmp_BANG] = ACTIONS(127),
    [anon_sym_BANGmf_BANG] = ACTIONS(127),
    [anon_sym_BANGf_BANG] = ACTIONS(127),
    [anon_sym_BANGff_BANG] = ACTIONS(127),
    [anon_sym_BANGfff_BANG] = ACTIONS(127),
    [anon_sym_BANGffff_BANG] = ACTIONS(127),
    [anon_sym_BANGsfz_BANG] = ACTIONS(127),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(127),
    [anon_sym_BANGsegno_BANG] = ACTIONS(127),
    [anon_sym_BANGcoda_BANG] = ACTIONS(127),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(127),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(127),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(127),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(127),
    [anon_sym_BANGfine_BANG] = ACTIONS(127),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(127),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(127),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(127),
    [sym_parts_line] = ACTIONS(133),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(7),
    [sym_area] = ACTIONS(133),
    [sym_book] = ACTIONS(133),
    [sym_composer] = ACTIONS(133),
    [sym_discography] = ACTIONS(133),
    [sym_file] = ACTIONS(133),
    [sym_group] = ACTIONS(133),
    [sym_history] = ACTIONS(133),
    [sym_instruction] = ACTIONS(133),
    [sym_key] = ACTIONS(133),
    [sym_macros] = ACTIONS(133),
    [sym_meter] = ACTIONS(133),
    [sym_notes] = ACTIONS(133),
    [sym_origin] = ACTIONS(133),
    [anon_sym_X_COLON] = ACTIONS(127),
    [sym_remark] = ACTIONS(133),
    [sym_rhythm_line] = ACTIONS(133),
    [sym_source] = ACTIONS(133),
    [anon_sym_s_COLON] = ACTIONS(55),
    [sym_tempo] = ACTIONS(133),
    [sym_transcription] = ACTIONS(133),
    [sym_tune_title] = ACTIONS(133),
    [sym_unit_note_length] = ACTIONS(133),
    [anon_sym_U_COLON] = ACTIONS(13),
    [sym_voice] = ACTIONS(133),
    [sym_words_postbody] = ACTIONS(127),
  },
  [6] = {
    [sym_abc_version] = STATE(264),
    [sym_symbol_line] = STATE(264),
    [sym_user_defined] = STATE(264),
    [sym_tune_header_info_line] = STATE(261),
    [aux_sym_tune_header_repeat1] = STATE(6),
    [ts_builtin_sym_end] = ACTIONS(135),
    [sym__NL] = ACTIONS(135),
    [anon_sym_BSLASH] = ACTIONS(135),
    [anon_sym_LBRACK] = ACTIONS(137),
    [sym_COMMENT] = ACTIONS(139),
    [sym_slur_open] = ACTIONS(137),
    [sym_slur_close] = ACTIONS(135),
    [sym_alteration] = ACTIONS(135),
    [sym_note_letter] = ACTIONS(137),
    [sym_rest] = ACTIONS(135),
    [anon_sym_Z] = ACTIONS(137),
    [anon_sym_X] = ACTIONS(137),
    [aux_sym_grace_note_token1] = ACTIONS(135),
    [aux_sym_chord_symbol_token1] = ACTIONS(137),
    [sym_annotation] = ACTIONS(135),
    [sym_tuplet_marker] = ACTIONS(135),
    [sym_decoration_shorthand] = ACTIONS(137),
    [sym_bar_line] = ACTIONS(137),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(135),
    [sym_thin_double_bar_line] = ACTIONS(135),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(135),
    [sym_start_of_repeated_section] = ACTIONS(135),
    [sym_end_of_repeated_section] = ACTIONS(135),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(135),
    [anon_sym_BANGtrill_BANG] = ACTIONS(135),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(135),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(135),
    [anon_sym_BANGmordent_BANG] = ACTIONS(135),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(135),
    [anon_sym_BANGroll_BANG] = ACTIONS(135),
    [anon_sym_BANGturn_BANG] = ACTIONS(135),
    [anon_sym_BANGturnx_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(135),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_BANG] = ACTIONS(135),
    [anon_sym_BANGaccent_BANG] = ACTIONS(135),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(135),
    [anon_sym_BANGfermata_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(135),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(135),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(135),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(135),
    [anon_sym_BANGplus_BANG] = ACTIONS(135),
    [anon_sym_BANGsnap_BANG] = ACTIONS(135),
    [anon_sym_BANGslide_BANG] = ACTIONS(135),
    [anon_sym_BANGwedge_BANG] = ACTIONS(135),
    [anon_sym_BANGupbow_BANG] = ACTIONS(135),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(135),
    [anon_sym_BANGopen_BANG] = ACTIONS(135),
    [anon_sym_BANGthumb_BANG] = ACTIONS(135),
    [anon_sym_BANGbreath_BANG] = ACTIONS(135),
    [anon_sym_BANGpppp_BANG] = ACTIONS(135),
    [anon_sym_BANGppp_BANG] = ACTIONS(135),
    [anon_sym_BANGpp_BANG] = ACTIONS(135),
    [anon_sym_BANGp_BANG] = ACTIONS(135),
    [anon_sym_BANGmp_BANG] = ACTIONS(135),
    [anon_sym_BANGmf_BANG] = ACTIONS(135),
    [anon_sym_BANGf_BANG] = ACTIONS(135),
    [anon_sym_BANGff_BANG] = ACTIONS(135),
    [anon_sym_BANGfff_BANG] = ACTIONS(135),
    [anon_sym_BANGffff_BANG] = ACTIONS(135),
    [anon_sym_BANGsfz_BANG] = ACTIONS(135),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGsegno_BANG] = ACTIONS(135),
    [anon_sym_BANGcoda_BANG] = ACTIONS(135),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(135),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(135),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(135),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(135),
    [anon_sym_BANGfine_BANG] = ACTIONS(135),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(135),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(135),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(135),
    [sym_parts_line] = ACTIONS(142),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(145),
    [sym_area] = ACTIONS(142),
    [sym_book] = ACTIONS(142),
    [sym_composer] = ACTIONS(142),
    [sym_discography] = ACTIONS(142),
    [sym_file] = ACTIONS(142),
    [sym_group] = ACTIONS(142),
    [sym_history] = ACTIONS(142),
    [sym_instruction] = ACTIONS(142),
    [sym_key] = ACTIONS(142),
    [sym_macros] = ACTIONS(142),
    [sym_meter] = ACTIONS(142),
    [sym_notes] = ACTIONS(142),
    [sym_origin] = ACTIONS(142),
    [anon_sym_X_COLON] = ACTIONS(135),
    [sym_remark] = ACTIONS(142),
    [sym_rhythm_line] = ACTIONS(142),
    [sym_source] = ACTIONS(142),
    [anon_sym_s_COLON] = ACTIONS(148),
    [sym_tempo] = ACTIONS(142),
    [sym_transcription] = ACTIONS(142),
    [sym_tune_title] = ACTIONS(142),
    [sym_unit_note_length] = ACTIONS(142),
    [anon_sym_U_COLON] = ACTIONS(151),
    [sym_voice] = ACTIONS(142),
    [sym_words_postbody] = ACTIONS(142),
  },
  [7] = {
    [sym_abc_version] = STATE(264),
    [sym_symbol_line] = STATE(264),
    [sym_user_defined] = STATE(264),
    [sym_tune_header_info_line] = STATE(261),
    [aux_sym_tune_header_repeat1] = STATE(5),
    [ts_builtin_sym_end] = ACTIONS(154),
    [sym__NL] = ACTIONS(154),
    [anon_sym_BSLASH] = ACTIONS(154),
    [anon_sym_LBRACK] = ACTIONS(156),
    [sym_COMMENT] = ACTIONS(131),
    [sym_slur_open] = ACTIONS(156),
    [sym_slur_close] = ACTIONS(154),
    [sym_alteration] = ACTIONS(154),
    [sym_note_letter] = ACTIONS(156),
    [sym_rest] = ACTIONS(154),
    [anon_sym_Z] = ACTIONS(156),
    [anon_sym_X] = ACTIONS(156),
    [aux_sym_grace_note_token1] = ACTIONS(154),
    [aux_sym_chord_symbol_token1] = ACTIONS(156),
    [sym_annotation] = ACTIONS(154),
    [sym_tuplet_marker] = ACTIONS(154),
    [sym_decoration_shorthand] = ACTIONS(156),
    [sym_bar_line] = ACTIONS(156),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(154),
    [sym_thin_double_bar_line] = ACTIONS(154),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(154),
    [sym_start_of_repeated_section] = ACTIONS(154),
    [sym_end_of_repeated_section] = ACTIONS(154),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(154),
    [anon_sym_BANGtrill_BANG] = ACTIONS(154),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(154),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(154),
    [anon_sym_BANGmordent_BANG] = ACTIONS(154),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(154),
    [anon_sym_BANGroll_BANG] = ACTIONS(154),
    [anon_sym_BANGturn_BANG] = ACTIONS(154),
    [anon_sym_BANGturnx_BANG] = ACTIONS(154),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(154),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(154),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(154),
    [anon_sym_BANG_GT_BANG] = ACTIONS(154),
    [anon_sym_BANGaccent_BANG] = ACTIONS(154),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(154),
    [anon_sym_BANGfermata_BANG] = ACTIONS(154),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(154),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(154),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(154),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(154),
    [anon_sym_BANGplus_BANG] = ACTIONS(154),
    [anon_sym_BANGsnap_BANG] = ACTIONS(154),
    [anon_sym_BANGslide_BANG] = ACTIONS(154),
    [anon_sym_BANGwedge_BANG] = ACTIONS(154),
    [anon_sym_BANGupbow_BANG] = ACTIONS(154),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(154),
    [anon_sym_BANGopen_BANG] = ACTIONS(154),
    [anon_sym_BANGthumb_BANG] = ACTIONS(154),
    [anon_sym_BANGbreath_BANG] = ACTIONS(154),
    [anon_sym_BANGpppp_BANG] = ACTIONS(154),
    [anon_sym_BANGppp_BANG] = ACTIONS(154),
    [anon_sym_BANGpp_BANG] = ACTIONS(154),
    [anon_sym_BANGp_BANG] = ACTIONS(154),
    [anon_sym_BANGmp_BANG] = ACTIONS(154),
    [anon_sym_BANGmf_BANG] = ACTIONS(154),
    [anon_sym_BANGf_BANG] = ACTIONS(154),
    [anon_sym_BANGff_BANG] = ACTIONS(154),
    [anon_sym_BANGfff_BANG] = ACTIONS(154),
    [anon_sym_BANGffff_BANG] = ACTIONS(154),
    [anon_sym_BANGsfz_BANG] = ACTIONS(154),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(154),
    [anon_sym_BANGsegno_BANG] = ACTIONS(154),
    [anon_sym_BANGcoda_BANG] = ACTIONS(154),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(154),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(154),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(154),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(154),
    [anon_sym_BANGfine_BANG] = ACTIONS(154),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(154),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(154),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(154),
    [sym_parts_line] = ACTIONS(133),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(7),
    [sym_area] = ACTIONS(133),
    [sym_book] = ACTIONS(133),
    [sym_composer] = ACTIONS(133),
    [sym_discography] = ACTIONS(133),
    [sym_file] = ACTIONS(133),
    [sym_group] = ACTIONS(133),
    [sym_history] = ACTIONS(133),
    [sym_instruction] = ACTIONS(133),
    [sym_key] = ACTIONS(133),
    [sym_macros] = ACTIONS(133),
    [sym_meter] = ACTIONS(133),
    [sym_notes] = ACTIONS(133),
    [sym_origin] = ACTIONS(133),
    [anon_sym_X_COLON] = ACTIONS(154),
    [sym_remark] = ACTIONS(133),
    [sym_rhythm_line] = ACTIONS(133),
    [sym_source] = ACTIONS(133),
    [anon_sym_s_COLON] = ACTIONS(55),
    [sym_tempo] = ACTIONS(133),
    [sym_transcription] = ACTIONS(133),
    [sym_tune_title] = ACTIONS(133),
    [sym_unit_note_length] = ACTIONS(133),
    [anon_sym_U_COLON] = ACTIONS(13),
    [sym_voice] = ACTIONS(133),
    [sym_words_postbody] = ACTIONS(154),
  },
  [8] = {
    [sym_open_chord] = STATE(142),
    [sym__nte_or_chrd] = STATE(9),
    [sym_note_construct] = STATE(9),
    [sym__chord_cstrct] = STATE(9),
    [sym_note] = STATE(33),
    [sym__pitch] = STATE(19),
    [sym_grace_note] = STATE(123),
    [sym_chord_symbol] = STATE(169),
    [sym_decoration] = STATE(190),
    [aux_sym_beam_repeat1] = STATE(9),
    [aux_sym_note_construct_repeat1] = STATE(123),
    [ts_builtin_sym_end] = ACTIONS(158),
    [sym__NL] = ACTIONS(160),
    [anon_sym_BSLASH] = ACTIONS(158),
    [sym_BEAM_SEPARATOR] = ACTIONS(162),
    [anon_sym_LBRACK] = ACTIONS(164),
    [sym_COMMENT] = ACTIONS(160),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(158),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(160),
    [sym_slur_open] = ACTIONS(160),
    [sym_slur_close] = ACTIONS(158),
    [sym_alteration] = ACTIONS(167),
    [sym_note_letter] = ACTIONS(170),
    [sym_rest] = ACTIONS(173),
    [anon_sym_Z] = ACTIONS(158),
    [anon_sym_X] = ACTIONS(160),
    [aux_sym_grace_note_token1] = ACTIONS(176),
    [aux_sym_chord_symbol_token1] = ACTIONS(179),
    [sym_annotation] = ACTIONS(158),
    [sym_tuplet_marker] = ACTIONS(182),
    [sym_decoration_shorthand] = ACTIONS(185),
    [sym_bar_line] = ACTIONS(160),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(158),
    [sym_thin_double_bar_line] = ACTIONS(158),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(158),
    [sym_start_of_repeated_section] = ACTIONS(158),
    [sym_end_of_repeated_section] = ACTIONS(158),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(158),
    [anon_sym_BANGtrill_BANG] = ACTIONS(158),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(158),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(158),
    [anon_sym_BANGmordent_BANG] = ACTIONS(158),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(158),
    [anon_sym_BANGroll_BANG] = ACTIONS(158),
    [anon_sym_BANGturn_BANG] = ACTIONS(158),
    [anon_sym_BANGturnx_BANG] = ACTIONS(158),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(158),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(158),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(158),
    [anon_sym_BANG_GT_BANG] = ACTIONS(158),
    [anon_sym_BANGaccent_BANG] = ACTIONS(158),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(158),
    [anon_sym_BANGfermata_BANG] = ACTIONS(158),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(158),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(158),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(158),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(158),
    [anon_sym_BANGplus_BANG] = ACTIONS(158),
    [anon_sym_BANGsnap_BANG] = ACTIONS(158),
    [anon_sym_BANGslide_BANG] = ACTIONS(158),
    [anon_sym_BANGwedge_BANG] = ACTIONS(158),
    [anon_sym_BANGupbow_BANG] = ACTIONS(158),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(158),
    [anon_sym_BANGopen_BANG] = ACTIONS(158),
    [anon_sym_BANGthumb_BANG] = ACTIONS(158),
    [anon_sym_BANGbreath_BANG] = ACTIONS(158),
    [anon_sym_BANGpppp_BANG] = ACTIONS(158),
    [anon_sym_BANGppp_BANG] = ACTIONS(158),
    [anon_sym_BANGpp_BANG] = ACTIONS(158),
    [anon_sym_BANGp_BANG] = ACTIONS(158),
    [anon_sym_BANGmp_BANG] = ACTIONS(158),
    [anon_sym_BANGmf_BANG] = ACTIONS(158),
    [anon_sym_BANGf_BANG] = ACTIONS(158),
    [anon_sym_BANGff_BANG] = ACTIONS(158),
    [anon_sym_BANGfff_BANG] = ACTIONS(158),
    [anon_sym_BANGffff_BANG] = ACTIONS(158),
    [anon_sym_BANGsfz_BANG] = ACTIONS(158),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(158),
    [anon_sym_BANGsegno_BANG] = ACTIONS(158),
    [anon_sym_BANGcoda_BANG] = ACTIONS(158),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(158),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(158),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(158),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(158),
    [anon_sym_BANGfine_BANG] = ACTIONS(158),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(158),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(158),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(158),
    [sym_parts_line] = ACTIONS(158),
    [sym_instruction] = ACTIONS(158),
    [sym_key] = ACTIONS(158),
    [sym_macros] = ACTIONS(158),
    [sym_meter] = ACTIONS(158),
    [sym_notes] = ACTIONS(158),
    [anon_sym_X_COLON] = ACTIONS(158),
    [sym_remark] = ACTIONS(158),
    [sym_rhythm_line] = ACTIONS(158),
    [anon_sym_s_COLON] = ACTIONS(158),
    [sym_tempo] = ACTIONS(158),
    [sym_tune_title] = ACTIONS(158),
    [sym_unit_note_length] = ACTIONS(158),
    [anon_sym_U_COLON] = ACTIONS(158),
    [sym_voice] = ACTIONS(158),
    [sym_words_tag] = ACTIONS(158),
    [sym_words_postbody] = ACTIONS(158),
  },
  [9] = {
    [sym_open_chord] = STATE(142),
    [sym__nte_or_chrd] = STATE(9),
    [sym_note_construct] = STATE(9),
    [sym__chord_cstrct] = STATE(9),
    [sym_note] = STATE(33),
    [sym__pitch] = STATE(19),
    [sym_grace_note] = STATE(123),
    [sym_chord_symbol] = STATE(169),
    [sym_decoration] = STATE(190),
    [aux_sym_beam_repeat1] = STATE(9),
    [aux_sym_note_construct_repeat1] = STATE(123),
    [ts_builtin_sym_end] = ACTIONS(188),
    [sym__NL] = ACTIONS(190),
    [anon_sym_BSLASH] = ACTIONS(188),
    [sym_BEAM_SEPARATOR] = ACTIONS(192),
    [anon_sym_LBRACK] = ACTIONS(195),
    [sym_COMMENT] = ACTIONS(190),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(188),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(190),
    [sym_slur_open] = ACTIONS(190),
    [sym_slur_close] = ACTIONS(188),
    [sym_alteration] = ACTIONS(198),
    [sym_note_letter] = ACTIONS(201),
    [sym_rest] = ACTIONS(204),
    [anon_sym_Z] = ACTIONS(188),
    [anon_sym_X] = ACTIONS(190),
    [aux_sym_grace_note_token1] = ACTIONS(207),
    [aux_sym_chord_symbol_token1] = ACTIONS(210),
    [sym_annotation] = ACTIONS(188),
    [sym_tuplet_marker] = ACTIONS(213),
    [sym_decoration_shorthand] = ACTIONS(216),
    [sym_bar_line] = ACTIONS(190),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(188),
    [sym_thin_double_bar_line] = ACTIONS(188),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(188),
    [sym_start_of_repeated_section] = ACTIONS(188),
    [sym_end_of_repeated_section] = ACTIONS(188),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(188),
    [anon_sym_BANGtrill_BANG] = ACTIONS(188),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(188),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(188),
    [anon_sym_BANGmordent_BANG] = ACTIONS(188),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(188),
    [anon_sym_BANGroll_BANG] = ACTIONS(188),
    [anon_sym_BANGturn_BANG] = ACTIONS(188),
    [anon_sym_BANGturnx_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(188),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_BANG] = ACTIONS(188),
    [anon_sym_BANGaccent_BANG] = ACTIONS(188),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(188),
    [anon_sym_BANGfermata_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(188),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(188),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(188),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(188),
    [anon_sym_BANGplus_BANG] = ACTIONS(188),
    [anon_sym_BANGsnap_BANG] = ACTIONS(188),
    [anon_sym_BANGslide_BANG] = ACTIONS(188),
    [anon_sym_BANGwedge_BANG] = ACTIONS(188),
    [anon_sym_BANGupbow_BANG] = ACTIONS(188),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(188),
    [anon_sym_BANGopen_BANG] = ACTIONS(188),
    [anon_sym_BANGthumb_BANG] = ACTIONS(188),
    [anon_sym_BANGbreath_BANG] = ACTIONS(188),
    [anon_sym_BANGpppp_BANG] = ACTIONS(188),
    [anon_sym_BANGppp_BANG] = ACTIONS(188),
    [anon_sym_BANGpp_BANG] = ACTIONS(188),
    [anon_sym_BANGp_BANG] = ACTIONS(188),
    [anon_sym_BANGmp_BANG] = ACTIONS(188),
    [anon_sym_BANGmf_BANG] = ACTIONS(188),
    [anon_sym_BANGf_BANG] = ACTIONS(188),
    [anon_sym_BANGff_BANG] = ACTIONS(188),
    [anon_sym_BANGfff_BANG] = ACTIONS(188),
    [anon_sym_BANGffff_BANG] = ACTIONS(188),
    [anon_sym_BANGsfz_BANG] = ACTIONS(188),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGsegno_BANG] = ACTIONS(188),
    [anon_sym_BANGcoda_BANG] = ACTIONS(188),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(188),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(188),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(188),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(188),
    [anon_sym_BANGfine_BANG] = ACTIONS(188),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(188),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(188),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(188),
    [sym_parts_line] = ACTIONS(188),
    [sym_instruction] = ACTIONS(188),
    [sym_key] = ACTIONS(188),
    [sym_macros] = ACTIONS(188),
    [sym_meter] = ACTIONS(188),
    [sym_notes] = ACTIONS(188),
    [anon_sym_X_COLON] = ACTIONS(188),
    [sym_remark] = ACTIONS(188),
    [sym_rhythm_line] = ACTIONS(188),
    [anon_sym_s_COLON] = ACTIONS(188),
    [sym_tempo] = ACTIONS(188),
    [sym_tune_title] = ACTIONS(188),
    [sym_unit_note_length] = ACTIONS(188),
    [anon_sym_U_COLON] = ACTIONS(188),
    [sym_voice] = ACTIONS(188),
    [sym_words_tag] = ACTIONS(188),
    [sym_words_postbody] = ACTIONS(188),
  },
  [10] = {
    [ts_builtin_sym_end] = ACTIONS(219),
    [sym__NL] = ACTIONS(219),
    [anon_sym_BSLASH] = ACTIONS(219),
    [anon_sym_LBRACK] = ACTIONS(221),
    [sym_COMMENT] = ACTIONS(221),
    [sym_slur_open] = ACTIONS(221),
    [sym_slur_close] = ACTIONS(219),
    [sym_alteration] = ACTIONS(219),
    [sym_note_letter] = ACTIONS(221),
    [sym_rest] = ACTIONS(219),
    [anon_sym_Z] = ACTIONS(221),
    [anon_sym_X] = ACTIONS(221),
    [aux_sym_grace_note_token1] = ACTIONS(219),
    [aux_sym_chord_symbol_token1] = ACTIONS(221),
    [sym_annotation] = ACTIONS(219),
    [sym_tuplet_marker] = ACTIONS(219),
    [sym_decoration_shorthand] = ACTIONS(221),
    [sym_bar_line] = ACTIONS(221),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(219),
    [sym_thin_double_bar_line] = ACTIONS(219),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(219),
    [sym_start_of_repeated_section] = ACTIONS(219),
    [sym_end_of_repeated_section] = ACTIONS(219),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(219),
    [anon_sym_BANGtrill_BANG] = ACTIONS(219),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(219),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(219),
    [anon_sym_BANGmordent_BANG] = ACTIONS(219),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(219),
    [anon_sym_BANGroll_BANG] = ACTIONS(219),
    [anon_sym_BANGturn_BANG] = ACTIONS(219),
    [anon_sym_BANGturnx_BANG] = ACTIONS(219),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(219),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(219),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(219),
    [anon_sym_BANG_GT_BANG] = ACTIONS(219),
    [anon_sym_BANGaccent_BANG] = ACTIONS(219),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(219),
    [anon_sym_BANGfermata_BANG] = ACTIONS(219),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(219),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(219),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(219),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(219),
    [anon_sym_BANGplus_BANG] = ACTIONS(219),
    [anon_sym_BANGsnap_BANG] = ACTIONS(219),
    [anon_sym_BANGslide_BANG] = ACTIONS(219),
    [anon_sym_BANGwedge_BANG] = ACTIONS(219),
    [anon_sym_BANGupbow_BANG] = ACTIONS(219),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(219),
    [anon_sym_BANGopen_BANG] = ACTIONS(219),
    [anon_sym_BANGthumb_BANG] = ACTIONS(219),
    [anon_sym_BANGbreath_BANG] = ACTIONS(219),
    [anon_sym_BANGpppp_BANG] = ACTIONS(219),
    [anon_sym_BANGppp_BANG] = ACTIONS(219),
    [anon_sym_BANGpp_BANG] = ACTIONS(219),
    [anon_sym_BANGp_BANG] = ACTIONS(219),
    [anon_sym_BANGmp_BANG] = ACTIONS(219),
    [anon_sym_BANGmf_BANG] = ACTIONS(219),
    [anon_sym_BANGf_BANG] = ACTIONS(219),
    [anon_sym_BANGff_BANG] = ACTIONS(219),
    [anon_sym_BANGfff_BANG] = ACTIONS(219),
    [anon_sym_BANGffff_BANG] = ACTIONS(219),
    [anon_sym_BANGsfz_BANG] = ACTIONS(219),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(219),
    [anon_sym_BANGsegno_BANG] = ACTIONS(219),
    [anon_sym_BANGcoda_BANG] = ACTIONS(219),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(219),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(219),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(219),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(219),
    [anon_sym_BANGfine_BANG] = ACTIONS(219),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(219),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(219),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(219),
    [sym_parts_line] = ACTIONS(219),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(221),
    [sym_area] = ACTIONS(219),
    [sym_book] = ACTIONS(219),
    [sym_composer] = ACTIONS(219),
    [sym_discography] = ACTIONS(219),
    [sym_file] = ACTIONS(219),
    [sym_group] = ACTIONS(219),
    [sym_history] = ACTIONS(219),
    [sym_instruction] = ACTIONS(219),
    [sym_key] = ACTIONS(219),
    [sym_macros] = ACTIONS(219),
    [sym_meter] = ACTIONS(219),
    [sym_notes] = ACTIONS(219),
    [sym_origin] = ACTIONS(219),
    [anon_sym_X_COLON] = ACTIONS(219),
    [sym_remark] = ACTIONS(219),
    [sym_rhythm_line] = ACTIONS(219),
    [sym_source] = ACTIONS(219),
    [anon_sym_s_COLON] = ACTIONS(219),
    [sym_tempo] = ACTIONS(219),
    [sym_transcription] = ACTIONS(219),
    [sym_tune_title] = ACTIONS(219),
    [sym_unit_note_length] = ACTIONS(219),
    [anon_sym_U_COLON] = ACTIONS(219),
    [sym_voice] = ACTIONS(219),
    [sym_words_postbody] = ACTIONS(219),
  },
  [11] = {
    [ts_builtin_sym_end] = ACTIONS(223),
    [sym__NL] = ACTIONS(223),
    [anon_sym_BSLASH] = ACTIONS(223),
    [anon_sym_LBRACK] = ACTIONS(225),
    [sym_COMMENT] = ACTIONS(225),
    [sym_slur_open] = ACTIONS(225),
    [sym_slur_close] = ACTIONS(223),
    [sym_alteration] = ACTIONS(223),
    [sym_note_letter] = ACTIONS(225),
    [sym_rest] = ACTIONS(223),
    [anon_sym_Z] = ACTIONS(225),
    [anon_sym_X] = ACTIONS(225),
    [aux_sym_grace_note_token1] = ACTIONS(223),
    [aux_sym_chord_symbol_token1] = ACTIONS(225),
    [sym_annotation] = ACTIONS(223),
    [sym_tuplet_marker] = ACTIONS(223),
    [sym_decoration_shorthand] = ACTIONS(225),
    [sym_bar_line] = ACTIONS(225),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(223),
    [sym_thin_double_bar_line] = ACTIONS(223),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(223),
    [sym_start_of_repeated_section] = ACTIONS(223),
    [sym_end_of_repeated_section] = ACTIONS(223),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(223),
    [anon_sym_BANGtrill_BANG] = ACTIONS(223),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(223),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(223),
    [anon_sym_BANGmordent_BANG] = ACTIONS(223),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(223),
    [anon_sym_BANGroll_BANG] = ACTIONS(223),
    [anon_sym_BANGturn_BANG] = ACTIONS(223),
    [anon_sym_BANGturnx_BANG] = ACTIONS(223),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(223),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(223),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(223),
    [anon_sym_BANG_GT_BANG] = ACTIONS(223),
    [anon_sym_BANGaccent_BANG] = ACTIONS(223),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(223),
    [anon_sym_BANGfermata_BANG] = ACTIONS(223),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(223),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(223),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(223),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(223),
    [anon_sym_BANGplus_BANG] = ACTIONS(223),
    [anon_sym_BANGsnap_BANG] = ACTIONS(223),
    [anon_sym_BANGslide_BANG] = ACTIONS(223),
    [anon_sym_BANGwedge_BANG] = ACTIONS(223),
    [anon_sym_BANGupbow_BANG] = ACTIONS(223),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(223),
    [anon_sym_BANGopen_BANG] = ACTIONS(223),
    [anon_sym_BANGthumb_BANG] = ACTIONS(223),
    [anon_sym_BANGbreath_BANG] = ACTIONS(223),
    [anon_sym_BANGpppp_BANG] = ACTIONS(223),
    [anon_sym_BANGppp_BANG] = ACTIONS(223),
    [anon_sym_BANGpp_BANG] = ACTIONS(223),
    [anon_sym_BANGp_BANG] = ACTIONS(223),
    [anon_sym_BANGmp_BANG] = ACTIONS(223),
    [anon_sym_BANGmf_BANG] = ACTIONS(223),
    [anon_sym_BANGf_BANG] = ACTIONS(223),
    [anon_sym_BANGff_BANG] = ACTIONS(223),
    [anon_sym_BANGfff_BANG] = ACTIONS(223),
    [anon_sym_BANGffff_BANG] = ACTIONS(223),
    [anon_sym_BANGsfz_BANG] = ACTIONS(223),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(223),
    [anon_sym_BANGsegno_BANG] = ACTIONS(223),
    [anon_sym_BANGcoda_BANG] = ACTIONS(223),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(223),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(223),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(223),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(223),
    [anon_sym_BANGfine_BANG] = ACTIONS(223),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(223),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(223),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(223),
    [sym_parts_line] = ACTIONS(223),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(225),
    [sym_area] = ACTIONS(223),
    [sym_book] = ACTIONS(223),
    [sym_composer] = ACTIONS(223),
    [sym_discography] = ACTIONS(223),
    [sym_file] = ACTIONS(223),
    [sym_group] = ACTIONS(223),
    [sym_history] = ACTIONS(223),
    [sym_instruction] = ACTIONS(223),
    [sym_key] = ACTIONS(223),
    [sym_macros] = ACTIONS(223),
    [sym_meter] = ACTIONS(223),
    [sym_notes] = ACTIONS(223),
    [sym_origin] = ACTIONS(223),
    [anon_sym_X_COLON] = ACTIONS(223),
    [sym_remark] = ACTIONS(223),
    [sym_rhythm_line] = ACTIONS(223),
    [sym_source] = ACTIONS(223),
    [anon_sym_s_COLON] = ACTIONS(223),
    [sym_tempo] = ACTIONS(223),
    [sym_transcription] = ACTIONS(223),
    [sym_tune_title] = ACTIONS(223),
    [sym_unit_note_length] = ACTIONS(223),
    [anon_sym_U_COLON] = ACTIONS(223),
    [sym_voice] = ACTIONS(223),
    [sym_words_postbody] = ACTIONS(223),
  },
  [12] = {
    [ts_builtin_sym_end] = ACTIONS(135),
    [sym__NL] = ACTIONS(135),
    [anon_sym_BSLASH] = ACTIONS(135),
    [anon_sym_LBRACK] = ACTIONS(137),
    [sym_COMMENT] = ACTIONS(137),
    [sym_slur_open] = ACTIONS(137),
    [sym_slur_close] = ACTIONS(135),
    [sym_alteration] = ACTIONS(135),
    [sym_note_letter] = ACTIONS(137),
    [sym_rest] = ACTIONS(135),
    [anon_sym_Z] = ACTIONS(137),
    [anon_sym_X] = ACTIONS(137),
    [aux_sym_grace_note_token1] = ACTIONS(135),
    [aux_sym_chord_symbol_token1] = ACTIONS(137),
    [sym_annotation] = ACTIONS(135),
    [sym_tuplet_marker] = ACTIONS(135),
    [sym_decoration_shorthand] = ACTIONS(137),
    [sym_bar_line] = ACTIONS(137),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(135),
    [sym_thin_double_bar_line] = ACTIONS(135),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(135),
    [sym_start_of_repeated_section] = ACTIONS(135),
    [sym_end_of_repeated_section] = ACTIONS(135),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(135),
    [anon_sym_BANGtrill_BANG] = ACTIONS(135),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(135),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(135),
    [anon_sym_BANGmordent_BANG] = ACTIONS(135),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(135),
    [anon_sym_BANGroll_BANG] = ACTIONS(135),
    [anon_sym_BANGturn_BANG] = ACTIONS(135),
    [anon_sym_BANGturnx_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(135),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_BANG] = ACTIONS(135),
    [anon_sym_BANGaccent_BANG] = ACTIONS(135),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(135),
    [anon_sym_BANGfermata_BANG] = ACTIONS(135),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(135),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(135),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(135),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(135),
    [anon_sym_BANGplus_BANG] = ACTIONS(135),
    [anon_sym_BANGsnap_BANG] = ACTIONS(135),
    [anon_sym_BANGslide_BANG] = ACTIONS(135),
    [anon_sym_BANGwedge_BANG] = ACTIONS(135),
    [anon_sym_BANGupbow_BANG] = ACTIONS(135),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(135),
    [anon_sym_BANGopen_BANG] = ACTIONS(135),
    [anon_sym_BANGthumb_BANG] = ACTIONS(135),
    [anon_sym_BANGbreath_BANG] = ACTIONS(135),
    [anon_sym_BANGpppp_BANG] = ACTIONS(135),
    [anon_sym_BANGppp_BANG] = ACTIONS(135),
    [anon_sym_BANGpp_BANG] = ACTIONS(135),
    [anon_sym_BANGp_BANG] = ACTIONS(135),
    [anon_sym_BANGmp_BANG] = ACTIONS(135),
    [anon_sym_BANGmf_BANG] = ACTIONS(135),
    [anon_sym_BANGf_BANG] = ACTIONS(135),
    [anon_sym_BANGff_BANG] = ACTIONS(135),
    [anon_sym_BANGfff_BANG] = ACTIONS(135),
    [anon_sym_BANGffff_BANG] = ACTIONS(135),
    [anon_sym_BANGsfz_BANG] = ACTIONS(135),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(135),
    [anon_sym_BANGsegno_BANG] = ACTIONS(135),
    [anon_sym_BANGcoda_BANG] = ACTIONS(135),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(135),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(135),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(135),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(135),
    [anon_sym_BANGfine_BANG] = ACTIONS(135),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(135),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(135),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(135),
    [sym_parts_line] = ACTIONS(135),
    [anon_sym_PERCENTabc_DASH] = ACTIONS(137),
    [sym_area] = ACTIONS(135),
    [sym_book] = ACTIONS(135),
    [sym_composer] = ACTIONS(135),
    [sym_discography] = ACTIONS(135),
    [sym_file] = ACTIONS(135),
    [sym_group] = ACTIONS(135),
    [sym_history] = ACTIONS(135),
    [sym_instruction] = ACTIONS(135),
    [sym_key] = ACTIONS(135),
    [sym_macros] = ACTIONS(135),
    [sym_meter] = ACTIONS(135),
    [sym_notes] = ACTIONS(135),
    [sym_origin] = ACTIONS(135),
    [anon_sym_X_COLON] = ACTIONS(135),
    [sym_remark] = ACTIONS(135),
    [sym_rhythm_line] = ACTIONS(135),
    [sym_source] = ACTIONS(135),
    [anon_sym_s_COLON] = ACTIONS(135),
    [sym_tempo] = ACTIONS(135),
    [sym_transcription] = ACTIONS(135),
    [sym_tune_title] = ACTIONS(135),
    [sym_unit_note_length] = ACTIONS(135),
    [anon_sym_U_COLON] = ACTIONS(135),
    [sym_voice] = ACTIONS(135),
    [sym_words_postbody] = ACTIONS(135),
  },
  [13] = {
    [sym_open_chord] = STATE(139),
    [sym__MUSIC_CODE] = STATE(2),
    [sym_tune_body] = STATE(181),
    [sym__music_content] = STATE(54),
    [sym__nte_or_chrd] = STATE(115),
    [sym_beam] = STATE(54),
    [sym_note_construct] = STATE(115),
    [sym__chord_cstrct] = STATE(115),
    [sym_note] = STATE(152),
    [sym__pitch] = STATE(135),
    [sym_multimeasure_rest] = STATE(54),
    [sym_grace_note] = STATE(124),
    [sym_chord_symbol] = STATE(172),
    [sym_decoration] = STATE(186),
    [sym_nth_ending_barline] = STATE(59),
    [sym_generic_bar_line] = STATE(54),
    [sym_first_repeat_bar] = STATE(59),
    [sym_second_repeat_bar] = STATE(59),
    [sym_lyric_line] = STATE(202),
    [sym_lyric_section] = STATE(223),
    [sym_symbol] = STATE(54),
    [sym_body_inline_info] = STATE(54),
    [aux_sym_note_construct_repeat1] = STATE(124),
    [aux_sym_lyric_section_repeat1] = STATE(202),
    [ts_builtin_sym_end] = ACTIONS(227),
    [sym__NL] = ACTIONS(227),
    [anon_sym_BSLASH] = ACTIONS(17),
    [anon_sym_LBRACK] = ACTIONS(19),
    [sym_slur_open] = ACTIONS(23),
    [sym_slur_close] = ACTIONS(17),
    [sym_alteration] = ACTIONS(25),
    [sym_note_letter] = ACTIONS(27),
    [sym_rest] = ACTIONS(29),
    [anon_sym_Z] = ACTIONS(31),
    [anon_sym_X] = ACTIONS(33),
    [aux_sym_grace_note_token1] = ACTIONS(35),
    [aux_sym_chord_symbol_token1] = ACTIONS(37),
    [sym_annotation] = ACTIONS(17),
    [sym_tuplet_marker] = ACTIONS(39),
    [sym_decoration_shorthand] = ACTIONS(229),
    [sym_bar_line] = ACTIONS(43),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(45),
    [sym_thin_double_bar_line] = ACTIONS(45),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(45),
    [sym_start_of_repeated_section] = ACTIONS(47),
    [sym_end_of_repeated_section] = ACTIONS(49),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(47),
    [anon_sym_BANGtrill_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(51),
    [anon_sym_BANGmordent_BANG] = ACTIONS(51),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(51),
    [anon_sym_BANGroll_BANG] = ACTIONS(51),
    [anon_sym_BANGturn_BANG] = ACTIONS(51),
    [anon_sym_BANGturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(51),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_BANG] = ACTIONS(51),
    [anon_sym_BANGaccent_BANG] = ACTIONS(51),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(51),
    [anon_sym_BANGfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(51),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(51),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(51),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(51),
    [anon_sym_BANGplus_BANG] = ACTIONS(51),
    [anon_sym_BANGsnap_BANG] = ACTIONS(51),
    [anon_sym_BANGslide_BANG] = ACTIONS(51),
    [anon_sym_BANGwedge_BANG] = ACTIONS(51),
    [anon_sym_BANGupbow_BANG] = ACTIONS(51),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(51),
    [anon_sym_BANGopen_BANG] = ACTIONS(51),
    [anon_sym_BANGthumb_BANG] = ACTIONS(51),
    [anon_sym_BANGbreath_BANG] = ACTIONS(51),
    [anon_sym_BANGpppp_BANG] = ACTIONS(51),
    [anon_sym_BANGppp_BANG] = ACTIONS(51),
    [anon_sym_BANGpp_BANG] = ACTIONS(51),
    [anon_sym_BANGp_BANG] = ACTIONS(51),
    [anon_sym_BANGmp_BANG] = ACTIONS(51),
    [anon_sym_BANGmf_BANG] = ACTIONS(51),
    [anon_sym_BANGf_BANG] = ACTIONS(51),
    [anon_sym_BANGff_BANG] = ACTIONS(51),
    [anon_sym_BANGfff_BANG] = ACTIONS(51),
    [anon_sym_BANGffff_BANG] = ACTIONS(51),
    [anon_sym_BANGsfz_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(51),
    [anon_sym_BANGsegno_BANG] = ACTIONS(51),
    [anon_sym_BANGcoda_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(51),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(51),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(51),
    [anon_sym_BANGfine_BANG] = ACTIONS(51),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(51),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(51),
    [anon_sym_X_COLON] = ACTIONS(227),
    [sym_words_postbody] = ACTIONS(231),
  },
  [14] = {
    [aux_sym__pitch_repeat1] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(233),
    [sym__NL] = ACTIONS(235),
    [anon_sym_BSLASH] = ACTIONS(233),
    [sym_BEAM_SEPARATOR] = ACTIONS(233),
    [anon_sym_LBRACK] = ACTIONS(235),
    [sym_COMMENT] = ACTIONS(235),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(233),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(235),
    [sym_slur_open] = ACTIONS(235),
    [sym_slur_close] = ACTIONS(233),
    [aux_sym_note_construct_token1] = ACTIONS(233),
    [sym_alteration] = ACTIONS(233),
    [sym_note_letter] = ACTIONS(233),
    [sym_octave] = ACTIONS(237),
    [sym_rest] = ACTIONS(233),
    [anon_sym_Z] = ACTIONS(233),
    [anon_sym_X] = ACTIONS(235),
    [sym_rhythm] = ACTIONS(233),
    [aux_sym_grace_note_token1] = ACTIONS(233),
    [aux_sym_chord_symbol_token1] = ACTIONS(235),
    [sym_annotation] = ACTIONS(233),
    [sym_tuplet_marker] = ACTIONS(233),
    [sym_decoration_shorthand] = ACTIONS(235),
    [sym_bar_line] = ACTIONS(235),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(233),
    [sym_thin_double_bar_line] = ACTIONS(233),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(233),
    [sym_start_of_repeated_section] = ACTIONS(233),
    [sym_end_of_repeated_section] = ACTIONS(233),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(233),
    [anon_sym_BANGtrill_BANG] = ACTIONS(233),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(233),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(233),
    [anon_sym_BANGmordent_BANG] = ACTIONS(233),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(233),
    [anon_sym_BANGroll_BANG] = ACTIONS(233),
    [anon_sym_BANGturn_BANG] = ACTIONS(233),
    [anon_sym_BANGturnx_BANG] = ACTIONS(233),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(233),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(233),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(233),
    [anon_sym_BANG_GT_BANG] = ACTIONS(233),
    [anon_sym_BANGaccent_BANG] = ACTIONS(233),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(233),
    [anon_sym_BANGfermata_BANG] = ACTIONS(233),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(233),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(233),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(233),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(233),
    [anon_sym_BANGplus_BANG] = ACTIONS(233),
    [anon_sym_BANGsnap_BANG] = ACTIONS(233),
    [anon_sym_BANGslide_BANG] = ACTIONS(233),
    [anon_sym_BANGwedge_BANG] = ACTIONS(233),
    [anon_sym_BANGupbow_BANG] = ACTIONS(233),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(233),
    [anon_sym_BANGopen_BANG] = ACTIONS(233),
    [anon_sym_BANGthumb_BANG] = ACTIONS(233),
    [anon_sym_BANGbreath_BANG] = ACTIONS(233),
    [anon_sym_BANGpppp_BANG] = ACTIONS(233),
    [anon_sym_BANGppp_BANG] = ACTIONS(233),
    [anon_sym_BANGpp_BANG] = ACTIONS(233),
    [anon_sym_BANGp_BANG] = ACTIONS(233),
    [anon_sym_BANGmp_BANG] = ACTIONS(233),
    [anon_sym_BANGmf_BANG] = ACTIONS(233),
    [anon_sym_BANGf_BANG] = ACTIONS(233),
    [anon_sym_BANGff_BANG] = ACTIONS(233),
    [anon_sym_BANGfff_BANG] = ACTIONS(233),
    [anon_sym_BANGffff_BANG] = ACTIONS(233),
    [anon_sym_BANGsfz_BANG] = ACTIONS(233),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(233),
    [anon_sym_BANGsegno_BANG] = ACTIONS(233),
    [anon_sym_BANGcoda_BANG] = ACTIONS(233),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(233),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(233),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(233),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(233),
    [anon_sym_BANGfine_BANG] = ACTIONS(233),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(233),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(233),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(233),
    [sym_parts_line] = ACTIONS(233),
    [sym_instruction] = ACTIONS(233),
    [sym_key] = ACTIONS(233),
    [sym_macros] = ACTIONS(233),
    [sym_meter] = ACTIONS(233),
    [sym_notes] = ACTIONS(233),
    [anon_sym_X_COLON] = ACTIONS(233),
    [sym_remark] = ACTIONS(233),
    [sym_rhythm_line] = ACTIONS(233),
    [anon_sym_s_COLON] = ACTIONS(233),
    [sym_tempo] = ACTIONS(233),
    [sym_tune_title] = ACTIONS(233),
    [sym_unit_note_length] = ACTIONS(233),
    [anon_sym_U_COLON] = ACTIONS(233),
    [sym_voice] = ACTIONS(233),
    [sym_words_tag] = ACTIONS(233),
    [sym_words_postbody] = ACTIONS(233),
  },
  [15] = {
    [aux_sym__pitch_repeat1] = STATE(17),
    [ts_builtin_sym_end] = ACTIONS(240),
    [sym__NL] = ACTIONS(242),
    [anon_sym_BSLASH] = ACTIONS(240),
    [sym_BEAM_SEPARATOR] = ACTIONS(240),
    [anon_sym_LBRACK] = ACTIONS(242),
    [sym_COMMENT] = ACTIONS(242),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(240),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(242),
    [sym_slur_open] = ACTIONS(242),
    [sym_slur_close] = ACTIONS(240),
    [aux_sym_note_construct_token1] = ACTIONS(240),
    [sym_alteration] = ACTIONS(240),
    [sym_note_letter] = ACTIONS(240),
    [sym_octave] = ACTIONS(244),
    [sym_rest] = ACTIONS(240),
    [anon_sym_Z] = ACTIONS(240),
    [anon_sym_X] = ACTIONS(242),
    [sym_rhythm] = ACTIONS(240),
    [aux_sym_grace_note_token1] = ACTIONS(240),
    [aux_sym_chord_symbol_token1] = ACTIONS(242),
    [sym_annotation] = ACTIONS(240),
    [sym_tuplet_marker] = ACTIONS(240),
    [sym_decoration_shorthand] = ACTIONS(242),
    [sym_bar_line] = ACTIONS(242),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(240),
    [sym_thin_double_bar_line] = ACTIONS(240),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(240),
    [sym_start_of_repeated_section] = ACTIONS(240),
    [sym_end_of_repeated_section] = ACTIONS(240),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(240),
    [anon_sym_BANGtrill_BANG] = ACTIONS(240),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(240),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(240),
    [anon_sym_BANGmordent_BANG] = ACTIONS(240),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(240),
    [anon_sym_BANGroll_BANG] = ACTIONS(240),
    [anon_sym_BANGturn_BANG] = ACTIONS(240),
    [anon_sym_BANGturnx_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(240),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_BANG] = ACTIONS(240),
    [anon_sym_BANGaccent_BANG] = ACTIONS(240),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(240),
    [anon_sym_BANGfermata_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(240),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(240),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(240),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(240),
    [anon_sym_BANGplus_BANG] = ACTIONS(240),
    [anon_sym_BANGsnap_BANG] = ACTIONS(240),
    [anon_sym_BANGslide_BANG] = ACTIONS(240),
    [anon_sym_BANGwedge_BANG] = ACTIONS(240),
    [anon_sym_BANGupbow_BANG] = ACTIONS(240),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(240),
    [anon_sym_BANGopen_BANG] = ACTIONS(240),
    [anon_sym_BANGthumb_BANG] = ACTIONS(240),
    [anon_sym_BANGbreath_BANG] = ACTIONS(240),
    [anon_sym_BANGpppp_BANG] = ACTIONS(240),
    [anon_sym_BANGppp_BANG] = ACTIONS(240),
    [anon_sym_BANGpp_BANG] = ACTIONS(240),
    [anon_sym_BANGp_BANG] = ACTIONS(240),
    [anon_sym_BANGmp_BANG] = ACTIONS(240),
    [anon_sym_BANGmf_BANG] = ACTIONS(240),
    [anon_sym_BANGf_BANG] = ACTIONS(240),
    [anon_sym_BANGff_BANG] = ACTIONS(240),
    [anon_sym_BANGfff_BANG] = ACTIONS(240),
    [anon_sym_BANGffff_BANG] = ACTIONS(240),
    [anon_sym_BANGsfz_BANG] = ACTIONS(240),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGsegno_BANG] = ACTIONS(240),
    [anon_sym_BANGcoda_BANG] = ACTIONS(240),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(240),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(240),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(240),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(240),
    [anon_sym_BANGfine_BANG] = ACTIONS(240),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(240),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(240),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(240),
    [sym_parts_line] = ACTIONS(240),
    [sym_instruction] = ACTIONS(240),
    [sym_key] = ACTIONS(240),
    [sym_macros] = ACTIONS(240),
    [sym_meter] = ACTIONS(240),
    [sym_notes] = ACTIONS(240),
    [anon_sym_X_COLON] = ACTIONS(240),
    [sym_remark] = ACTIONS(240),
    [sym_rhythm_line] = ACTIONS(240),
    [anon_sym_s_COLON] = ACTIONS(240),
    [sym_tempo] = ACTIONS(240),
    [sym_tune_title] = ACTIONS(240),
    [sym_unit_note_length] = ACTIONS(240),
    [anon_sym_U_COLON] = ACTIONS(240),
    [sym_voice] = ACTIONS(240),
    [sym_words_tag] = ACTIONS(240),
    [sym_words_postbody] = ACTIONS(240),
  },
  [16] = {
    [aux_sym__pitch_repeat1] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(240),
    [sym__NL] = ACTIONS(242),
    [anon_sym_BSLASH] = ACTIONS(240),
    [sym_BEAM_SEPARATOR] = ACTIONS(240),
    [anon_sym_LBRACK] = ACTIONS(242),
    [sym_COMMENT] = ACTIONS(242),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(240),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(242),
    [sym_slur_open] = ACTIONS(242),
    [sym_slur_close] = ACTIONS(240),
    [aux_sym_note_construct_token1] = ACTIONS(240),
    [sym_alteration] = ACTIONS(240),
    [sym_note_letter] = ACTIONS(240),
    [sym_octave] = ACTIONS(246),
    [sym_rest] = ACTIONS(240),
    [anon_sym_Z] = ACTIONS(240),
    [anon_sym_X] = ACTIONS(242),
    [sym_rhythm] = ACTIONS(240),
    [aux_sym_grace_note_token1] = ACTIONS(240),
    [aux_sym_chord_symbol_token1] = ACTIONS(242),
    [sym_annotation] = ACTIONS(240),
    [sym_tuplet_marker] = ACTIONS(240),
    [sym_decoration_shorthand] = ACTIONS(242),
    [sym_bar_line] = ACTIONS(242),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(240),
    [sym_thin_double_bar_line] = ACTIONS(240),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(240),
    [sym_start_of_repeated_section] = ACTIONS(240),
    [sym_end_of_repeated_section] = ACTIONS(240),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(240),
    [anon_sym_BANGtrill_BANG] = ACTIONS(240),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(240),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(240),
    [anon_sym_BANGmordent_BANG] = ACTIONS(240),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(240),
    [anon_sym_BANGroll_BANG] = ACTIONS(240),
    [anon_sym_BANGturn_BANG] = ACTIONS(240),
    [anon_sym_BANGturnx_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(240),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_BANG] = ACTIONS(240),
    [anon_sym_BANGaccent_BANG] = ACTIONS(240),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(240),
    [anon_sym_BANGfermata_BANG] = ACTIONS(240),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(240),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(240),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(240),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(240),
    [anon_sym_BANGplus_BANG] = ACTIONS(240),
    [anon_sym_BANGsnap_BANG] = ACTIONS(240),
    [anon_sym_BANGslide_BANG] = ACTIONS(240),
    [anon_sym_BANGwedge_BANG] = ACTIONS(240),
    [anon_sym_BANGupbow_BANG] = ACTIONS(240),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(240),
    [anon_sym_BANGopen_BANG] = ACTIONS(240),
    [anon_sym_BANGthumb_BANG] = ACTIONS(240),
    [anon_sym_BANGbreath_BANG] = ACTIONS(240),
    [anon_sym_BANGpppp_BANG] = ACTIONS(240),
    [anon_sym_BANGppp_BANG] = ACTIONS(240),
    [anon_sym_BANGpp_BANG] = ACTIONS(240),
    [anon_sym_BANGp_BANG] = ACTIONS(240),
    [anon_sym_BANGmp_BANG] = ACTIONS(240),
    [anon_sym_BANGmf_BANG] = ACTIONS(240),
    [anon_sym_BANGf_BANG] = ACTIONS(240),
    [anon_sym_BANGff_BANG] = ACTIONS(240),
    [anon_sym_BANGfff_BANG] = ACTIONS(240),
    [anon_sym_BANGffff_BANG] = ACTIONS(240),
    [anon_sym_BANGsfz_BANG] = ACTIONS(240),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(240),
    [anon_sym_BANGsegno_BANG] = ACTIONS(240),
    [anon_sym_BANGcoda_BANG] = ACTIONS(240),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(240),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(240),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(240),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(240),
    [anon_sym_BANGfine_BANG] = ACTIONS(240),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(240),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(240),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(240),
    [sym_parts_line] = ACTIONS(240),
    [sym_instruction] = ACTIONS(240),
    [sym_key] = ACTIONS(240),
    [sym_macros] = ACTIONS(240),
    [sym_meter] = ACTIONS(240),
    [sym_notes] = ACTIONS(240),
    [anon_sym_X_COLON] = ACTIONS(240),
    [sym_remark] = ACTIONS(240),
    [sym_rhythm_line] = ACTIONS(240),
    [anon_sym_s_COLON] = ACTIONS(240),
    [sym_tempo] = ACTIONS(240),
    [sym_tune_title] = ACTIONS(240),
    [sym_unit_note_length] = ACTIONS(240),
    [anon_sym_U_COLON] = ACTIONS(240),
    [sym_voice] = ACTIONS(240),
    [sym_words_tag] = ACTIONS(240),
    [sym_words_postbody] = ACTIONS(240),
  },
  [17] = {
    [aux_sym__pitch_repeat1] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(248),
    [sym__NL] = ACTIONS(250),
    [anon_sym_BSLASH] = ACTIONS(248),
    [sym_BEAM_SEPARATOR] = ACTIONS(248),
    [anon_sym_LBRACK] = ACTIONS(250),
    [sym_COMMENT] = ACTIONS(250),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(248),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(250),
    [sym_slur_open] = ACTIONS(250),
    [sym_slur_close] = ACTIONS(248),
    [aux_sym_note_construct_token1] = ACTIONS(248),
    [sym_alteration] = ACTIONS(248),
    [sym_note_letter] = ACTIONS(248),
    [sym_octave] = ACTIONS(246),
    [sym_rest] = ACTIONS(248),
    [anon_sym_Z] = ACTIONS(248),
    [anon_sym_X] = ACTIONS(250),
    [sym_rhythm] = ACTIONS(248),
    [aux_sym_grace_note_token1] = ACTIONS(248),
    [aux_sym_chord_symbol_token1] = ACTIONS(250),
    [sym_annotation] = ACTIONS(248),
    [sym_tuplet_marker] = ACTIONS(248),
    [sym_decoration_shorthand] = ACTIONS(250),
    [sym_bar_line] = ACTIONS(250),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(248),
    [sym_thin_double_bar_line] = ACTIONS(248),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(248),
    [sym_start_of_repeated_section] = ACTIONS(248),
    [sym_end_of_repeated_section] = ACTIONS(248),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(248),
    [anon_sym_BANGtrill_BANG] = ACTIONS(248),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(248),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(248),
    [anon_sym_BANGmordent_BANG] = ACTIONS(248),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(248),
    [anon_sym_BANGroll_BANG] = ACTIONS(248),
    [anon_sym_BANGturn_BANG] = ACTIONS(248),
    [anon_sym_BANGturnx_BANG] = ACTIONS(248),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(248),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(248),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(248),
    [anon_sym_BANG_GT_BANG] = ACTIONS(248),
    [anon_sym_BANGaccent_BANG] = ACTIONS(248),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(248),
    [anon_sym_BANGfermata_BANG] = ACTIONS(248),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(248),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(248),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(248),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(248),
    [anon_sym_BANGplus_BANG] = ACTIONS(248),
    [anon_sym_BANGsnap_BANG] = ACTIONS(248),
    [anon_sym_BANGslide_BANG] = ACTIONS(248),
    [anon_sym_BANGwedge_BANG] = ACTIONS(248),
    [anon_sym_BANGupbow_BANG] = ACTIONS(248),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(248),
    [anon_sym_BANGopen_BANG] = ACTIONS(248),
    [anon_sym_BANGthumb_BANG] = ACTIONS(248),
    [anon_sym_BANGbreath_BANG] = ACTIONS(248),
    [anon_sym_BANGpppp_BANG] = ACTIONS(248),
    [anon_sym_BANGppp_BANG] = ACTIONS(248),
    [anon_sym_BANGpp_BANG] = ACTIONS(248),
    [anon_sym_BANGp_BANG] = ACTIONS(248),
    [anon_sym_BANGmp_BANG] = ACTIONS(248),
    [anon_sym_BANGmf_BANG] = ACTIONS(248),
    [anon_sym_BANGf_BANG] = ACTIONS(248),
    [anon_sym_BANGff_BANG] = ACTIONS(248),
    [anon_sym_BANGfff_BANG] = ACTIONS(248),
    [anon_sym_BANGffff_BANG] = ACTIONS(248),
    [anon_sym_BANGsfz_BANG] = ACTIONS(248),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(248),
    [anon_sym_BANGsegno_BANG] = ACTIONS(248),
    [anon_sym_BANGcoda_BANG] = ACTIONS(248),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(248),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(248),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(248),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(248),
    [anon_sym_BANGfine_BANG] = ACTIONS(248),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(248),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(248),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(248),
    [sym_parts_line] = ACTIONS(248),
    [sym_instruction] = ACTIONS(248),
    [sym_key] = ACTIONS(248),
    [sym_macros] = ACTIONS(248),
    [sym_meter] = ACTIONS(248),
    [sym_notes] = ACTIONS(248),
    [anon_sym_X_COLON] = ACTIONS(248),
    [sym_remark] = ACTIONS(248),
    [sym_rhythm_line] = ACTIONS(248),
    [anon_sym_s_COLON] = ACTIONS(248),
    [sym_tempo] = ACTIONS(248),
    [sym_tune_title] = ACTIONS(248),
    [sym_unit_note_length] = ACTIONS(248),
    [anon_sym_U_COLON] = ACTIONS(248),
    [sym_voice] = ACTIONS(248),
    [sym_words_tag] = ACTIONS(248),
    [sym_words_postbody] = ACTIONS(248),
  },
  [18] = {
    [aux_sym__pitch_repeat1] = STATE(16),
    [ts_builtin_sym_end] = ACTIONS(252),
    [sym__NL] = ACTIONS(254),
    [anon_sym_BSLASH] = ACTIONS(252),
    [sym_BEAM_SEPARATOR] = ACTIONS(252),
    [anon_sym_LBRACK] = ACTIONS(254),
    [sym_COMMENT] = ACTIONS(254),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(252),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(254),
    [sym_slur_open] = ACTIONS(254),
    [sym_slur_close] = ACTIONS(252),
    [aux_sym_note_construct_token1] = ACTIONS(252),
    [sym_alteration] = ACTIONS(252),
    [sym_note_letter] = ACTIONS(252),
    [sym_octave] = ACTIONS(256),
    [sym_rest] = ACTIONS(252),
    [anon_sym_Z] = ACTIONS(252),
    [anon_sym_X] = ACTIONS(254),
    [sym_rhythm] = ACTIONS(252),
    [aux_sym_grace_note_token1] = ACTIONS(252),
    [aux_sym_chord_symbol_token1] = ACTIONS(254),
    [sym_annotation] = ACTIONS(252),
    [sym_tuplet_marker] = ACTIONS(252),
    [sym_decoration_shorthand] = ACTIONS(254),
    [sym_bar_line] = ACTIONS(254),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(252),
    [sym_thin_double_bar_line] = ACTIONS(252),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(252),
    [sym_start_of_repeated_section] = ACTIONS(252),
    [sym_end_of_repeated_section] = ACTIONS(252),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(252),
    [anon_sym_BANGtrill_BANG] = ACTIONS(252),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(252),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(252),
    [anon_sym_BANGmordent_BANG] = ACTIONS(252),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(252),
    [anon_sym_BANGroll_BANG] = ACTIONS(252),
    [anon_sym_BANGturn_BANG] = ACTIONS(252),
    [anon_sym_BANGturnx_BANG] = ACTIONS(252),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(252),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(252),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(252),
    [anon_sym_BANG_GT_BANG] = ACTIONS(252),
    [anon_sym_BANGaccent_BANG] = ACTIONS(252),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(252),
    [anon_sym_BANGfermata_BANG] = ACTIONS(252),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(252),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(252),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(252),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(252),
    [anon_sym_BANGplus_BANG] = ACTIONS(252),
    [anon_sym_BANGsnap_BANG] = ACTIONS(252),
    [anon_sym_BANGslide_BANG] = ACTIONS(252),
    [anon_sym_BANGwedge_BANG] = ACTIONS(252),
    [anon_sym_BANGupbow_BANG] = ACTIONS(252),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(252),
    [anon_sym_BANGopen_BANG] = ACTIONS(252),
    [anon_sym_BANGthumb_BANG] = ACTIONS(252),
    [anon_sym_BANGbreath_BANG] = ACTIONS(252),
    [anon_sym_BANGpppp_BANG] = ACTIONS(252),
    [anon_sym_BANGppp_BANG] = ACTIONS(252),
    [anon_sym_BANGpp_BANG] = ACTIONS(252),
    [anon_sym_BANGp_BANG] = ACTIONS(252),
    [anon_sym_BANGmp_BANG] = ACTIONS(252),
    [anon_sym_BANGmf_BANG] = ACTIONS(252),
    [anon_sym_BANGf_BANG] = ACTIONS(252),
    [anon_sym_BANGff_BANG] = ACTIONS(252),
    [anon_sym_BANGfff_BANG] = ACTIONS(252),
    [anon_sym_BANGffff_BANG] = ACTIONS(252),
    [anon_sym_BANGsfz_BANG] = ACTIONS(252),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(252),
    [anon_sym_BANGsegno_BANG] = ACTIONS(252),
    [anon_sym_BANGcoda_BANG] = ACTIONS(252),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(252),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(252),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(252),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(252),
    [anon_sym_BANGfine_BANG] = ACTIONS(252),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(252),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(252),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(252),
    [sym_parts_line] = ACTIONS(252),
    [sym_instruction] = ACTIONS(252),
    [sym_key] = ACTIONS(252),
    [sym_macros] = ACTIONS(252),
    [sym_meter] = ACTIONS(252),
    [sym_notes] = ACTIONS(252),
    [anon_sym_X_COLON] = ACTIONS(252),
    [sym_remark] = ACTIONS(252),
    [sym_rhythm_line] = ACTIONS(252),
    [anon_sym_s_COLON] = ACTIONS(252),
    [sym_tempo] = ACTIONS(252),
    [sym_tune_title] = ACTIONS(252),
    [sym_unit_note_length] = ACTIONS(252),
    [anon_sym_U_COLON] = ACTIONS(252),
    [sym_voice] = ACTIONS(252),
    [sym_words_tag] = ACTIONS(252),
    [sym_words_postbody] = ACTIONS(252),
  },
  [19] = {
    [ts_builtin_sym_end] = ACTIONS(258),
    [sym__NL] = ACTIONS(260),
    [anon_sym_BSLASH] = ACTIONS(258),
    [sym_BEAM_SEPARATOR] = ACTIONS(258),
    [anon_sym_LBRACK] = ACTIONS(260),
    [sym_COMMENT] = ACTIONS(260),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(258),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(260),
    [sym_slur_open] = ACTIONS(260),
    [sym_slur_close] = ACTIONS(258),
    [aux_sym_note_construct_token1] = ACTIONS(258),
    [sym_alteration] = ACTIONS(258),
    [sym_note_letter] = ACTIONS(258),
    [sym_rest] = ACTIONS(258),
    [anon_sym_Z] = ACTIONS(258),
    [anon_sym_X] = ACTIONS(260),
    [sym_rhythm] = ACTIONS(262),
    [aux_sym_grace_note_token1] = ACTIONS(258),
    [aux_sym_chord_symbol_token1] = ACTIONS(260),
    [sym_annotation] = ACTIONS(258),
    [sym_tuplet_marker] = ACTIONS(258),
    [sym_decoration_shorthand] = ACTIONS(260),
    [sym_bar_line] = ACTIONS(260),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(258),
    [sym_thin_double_bar_line] = ACTIONS(258),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(258),
    [sym_start_of_repeated_section] = ACTIONS(258),
    [sym_end_of_repeated_section] = ACTIONS(258),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(258),
    [anon_sym_BANGtrill_BANG] = ACTIONS(258),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(258),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(258),
    [anon_sym_BANGmordent_BANG] = ACTIONS(258),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(258),
    [anon_sym_BANGroll_BANG] = ACTIONS(258),
    [anon_sym_BANGturn_BANG] = ACTIONS(258),
    [anon_sym_BANGturnx_BANG] = ACTIONS(258),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(258),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(258),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(258),
    [anon_sym_BANG_GT_BANG] = ACTIONS(258),
    [anon_sym_BANGaccent_BANG] = ACTIONS(258),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(258),
    [anon_sym_BANGfermata_BANG] = ACTIONS(258),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(258),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(258),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(258),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(258),
    [anon_sym_BANGplus_BANG] = ACTIONS(258),
    [anon_sym_BANGsnap_BANG] = ACTIONS(258),
    [anon_sym_BANGslide_BANG] = ACTIONS(258),
    [anon_sym_BANGwedge_BANG] = ACTIONS(258),
    [anon_sym_BANGupbow_BANG] = ACTIONS(258),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(258),
    [anon_sym_BANGopen_BANG] = ACTIONS(258),
    [anon_sym_BANGthumb_BANG] = ACTIONS(258),
    [anon_sym_BANGbreath_BANG] = ACTIONS(258),
    [anon_sym_BANGpppp_BANG] = ACTIONS(258),
    [anon_sym_BANGppp_BANG] = ACTIONS(258),
    [anon_sym_BANGpp_BANG] = ACTIONS(258),
    [anon_sym_BANGp_BANG] = ACTIONS(258),
    [anon_sym_BANGmp_BANG] = ACTIONS(258),
    [anon_sym_BANGmf_BANG] = ACTIONS(258),
    [anon_sym_BANGf_BANG] = ACTIONS(258),
    [anon_sym_BANGff_BANG] = ACTIONS(258),
    [anon_sym_BANGfff_BANG] = ACTIONS(258),
    [anon_sym_BANGffff_BANG] = ACTIONS(258),
    [anon_sym_BANGsfz_BANG] = ACTIONS(258),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(258),
    [anon_sym_BANGsegno_BANG] = ACTIONS(258),
    [anon_sym_BANGcoda_BANG] = ACTIONS(258),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(258),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(258),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(258),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(258),
    [anon_sym_BANGfine_BANG] = ACTIONS(258),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(258),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(258),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(258),
    [sym_parts_line] = ACTIONS(258),
    [sym_instruction] = ACTIONS(258),
    [sym_key] = ACTIONS(258),
    [sym_macros] = ACTIONS(258),
    [sym_meter] = ACTIONS(258),
    [sym_notes] = ACTIONS(258),
    [anon_sym_X_COLON] = ACTIONS(258),
    [sym_remark] = ACTIONS(258),
    [sym_rhythm_line] = ACTIONS(258),
    [anon_sym_s_COLON] = ACTIONS(258),
    [sym_tempo] = ACTIONS(258),
    [sym_tune_title] = ACTIONS(258),
    [sym_unit_note_length] = ACTIONS(258),
    [anon_sym_U_COLON] = ACTIONS(258),
    [sym_voice] = ACTIONS(258),
    [sym_words_tag] = ACTIONS(258),
    [sym_words_postbody] = ACTIONS(258),
  },
  [20] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(20),
    [ts_builtin_sym_end] = ACTIONS(264),
    [sym__NL] = ACTIONS(266),
    [anon_sym_BSLASH] = ACTIONS(264),
    [anon_sym_DASH] = ACTIONS(268),
    [anon_sym_LBRACK] = ACTIONS(266),
    [sym_COMMENT] = ACTIONS(266),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(264),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(266),
    [sym_slur_open] = ACTIONS(266),
    [sym_slur_close] = ACTIONS(264),
    [sym_alteration] = ACTIONS(264),
    [sym_note_letter] = ACTIONS(264),
    [sym_rest] = ACTIONS(264),
    [anon_sym_Z] = ACTIONS(264),
    [anon_sym_X] = ACTIONS(266),
    [aux_sym_grace_note_token1] = ACTIONS(264),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(264),
    [sym_tuplet_marker] = ACTIONS(264),
    [sym_decoration_shorthand] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(268),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(264),
    [sym_thin_double_bar_line] = ACTIONS(264),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(264),
    [sym_start_of_repeated_section] = ACTIONS(264),
    [sym_end_of_repeated_section] = ACTIONS(264),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(264),
    [anon_sym_BANGtrill_BANG] = ACTIONS(264),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(264),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(264),
    [anon_sym_BANGmordent_BANG] = ACTIONS(264),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(264),
    [anon_sym_BANGroll_BANG] = ACTIONS(264),
    [anon_sym_BANGturn_BANG] = ACTIONS(264),
    [anon_sym_BANGturnx_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(264),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_BANG] = ACTIONS(264),
    [anon_sym_BANGaccent_BANG] = ACTIONS(264),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(264),
    [anon_sym_BANGfermata_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(264),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(264),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(264),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(264),
    [anon_sym_BANGplus_BANG] = ACTIONS(264),
    [anon_sym_BANGsnap_BANG] = ACTIONS(264),
    [anon_sym_BANGslide_BANG] = ACTIONS(264),
    [anon_sym_BANGwedge_BANG] = ACTIONS(264),
    [anon_sym_BANGupbow_BANG] = ACTIONS(264),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(264),
    [anon_sym_BANGopen_BANG] = ACTIONS(264),
    [anon_sym_BANGthumb_BANG] = ACTIONS(264),
    [anon_sym_BANGbreath_BANG] = ACTIONS(264),
    [anon_sym_BANGpppp_BANG] = ACTIONS(264),
    [anon_sym_BANGppp_BANG] = ACTIONS(264),
    [anon_sym_BANGpp_BANG] = ACTIONS(264),
    [anon_sym_BANGp_BANG] = ACTIONS(264),
    [anon_sym_BANGmp_BANG] = ACTIONS(264),
    [anon_sym_BANGmf_BANG] = ACTIONS(264),
    [anon_sym_BANGf_BANG] = ACTIONS(264),
    [anon_sym_BANGff_BANG] = ACTIONS(264),
    [anon_sym_BANGfff_BANG] = ACTIONS(264),
    [anon_sym_BANGffff_BANG] = ACTIONS(264),
    [anon_sym_BANGsfz_BANG] = ACTIONS(264),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGsegno_BANG] = ACTIONS(264),
    [anon_sym_BANGcoda_BANG] = ACTIONS(264),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(264),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(264),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(264),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(264),
    [anon_sym_BANGfine_BANG] = ACTIONS(264),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(264),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(264),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(264),
    [sym_parts_line] = ACTIONS(264),
    [sym_instruction] = ACTIONS(264),
    [sym_key] = ACTIONS(264),
    [sym_macros] = ACTIONS(264),
    [sym_meter] = ACTIONS(264),
    [sym_notes] = ACTIONS(264),
    [anon_sym_X_COLON] = ACTIONS(264),
    [sym_remark] = ACTIONS(264),
    [sym_rhythm_line] = ACTIONS(264),
    [anon_sym_s_COLON] = ACTIONS(264),
    [sym_tempo] = ACTIONS(264),
    [sym_tune_title] = ACTIONS(264),
    [sym_unit_note_length] = ACTIONS(264),
    [anon_sym_U_COLON] = ACTIONS(264),
    [sym_voice] = ACTIONS(264),
    [sym_words_tag] = ACTIONS(264),
    [sym_words_postbody] = ACTIONS(264),
  },
  [21] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(20),
    [ts_builtin_sym_end] = ACTIONS(271),
    [sym__NL] = ACTIONS(273),
    [anon_sym_BSLASH] = ACTIONS(271),
    [anon_sym_DASH] = ACTIONS(275),
    [anon_sym_LBRACK] = ACTIONS(273),
    [sym_COMMENT] = ACTIONS(273),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(271),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(273),
    [sym_slur_open] = ACTIONS(273),
    [sym_slur_close] = ACTIONS(271),
    [sym_alteration] = ACTIONS(271),
    [sym_note_letter] = ACTIONS(271),
    [sym_rest] = ACTIONS(271),
    [anon_sym_Z] = ACTIONS(271),
    [anon_sym_X] = ACTIONS(273),
    [aux_sym_grace_note_token1] = ACTIONS(271),
    [aux_sym_chord_symbol_token1] = ACTIONS(273),
    [sym_annotation] = ACTIONS(271),
    [sym_tuplet_marker] = ACTIONS(271),
    [sym_decoration_shorthand] = ACTIONS(273),
    [anon_sym_COMMA] = ACTIONS(275),
    [sym_bar_line] = ACTIONS(273),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(271),
    [sym_thin_double_bar_line] = ACTIONS(271),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(271),
    [sym_start_of_repeated_section] = ACTIONS(271),
    [sym_end_of_repeated_section] = ACTIONS(271),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(271),
    [anon_sym_BANGtrill_BANG] = ACTIONS(271),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(271),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(271),
    [anon_sym_BANGmordent_BANG] = ACTIONS(271),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(271),
    [anon_sym_BANGroll_BANG] = ACTIONS(271),
    [anon_sym_BANGturn_BANG] = ACTIONS(271),
    [anon_sym_BANGturnx_BANG] = ACTIONS(271),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(271),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(271),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(271),
    [anon_sym_BANG_GT_BANG] = ACTIONS(271),
    [anon_sym_BANGaccent_BANG] = ACTIONS(271),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(271),
    [anon_sym_BANGfermata_BANG] = ACTIONS(271),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(271),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(271),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(271),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(271),
    [anon_sym_BANGplus_BANG] = ACTIONS(271),
    [anon_sym_BANGsnap_BANG] = ACTIONS(271),
    [anon_sym_BANGslide_BANG] = ACTIONS(271),
    [anon_sym_BANGwedge_BANG] = ACTIONS(271),
    [anon_sym_BANGupbow_BANG] = ACTIONS(271),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(271),
    [anon_sym_BANGopen_BANG] = ACTIONS(271),
    [anon_sym_BANGthumb_BANG] = ACTIONS(271),
    [anon_sym_BANGbreath_BANG] = ACTIONS(271),
    [anon_sym_BANGpppp_BANG] = ACTIONS(271),
    [anon_sym_BANGppp_BANG] = ACTIONS(271),
    [anon_sym_BANGpp_BANG] = ACTIONS(271),
    [anon_sym_BANGp_BANG] = ACTIONS(271),
    [anon_sym_BANGmp_BANG] = ACTIONS(271),
    [anon_sym_BANGmf_BANG] = ACTIONS(271),
    [anon_sym_BANGf_BANG] = ACTIONS(271),
    [anon_sym_BANGff_BANG] = ACTIONS(271),
    [anon_sym_BANGfff_BANG] = ACTIONS(271),
    [anon_sym_BANGffff_BANG] = ACTIONS(271),
    [anon_sym_BANGsfz_BANG] = ACTIONS(271),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(271),
    [anon_sym_BANGsegno_BANG] = ACTIONS(271),
    [anon_sym_BANGcoda_BANG] = ACTIONS(271),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(271),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(271),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(271),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(271),
    [anon_sym_BANGfine_BANG] = ACTIONS(271),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(271),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(271),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(271),
    [sym_parts_line] = ACTIONS(271),
    [sym_instruction] = ACTIONS(271),
    [sym_key] = ACTIONS(271),
    [sym_macros] = ACTIONS(271),
    [sym_meter] = ACTIONS(271),
    [sym_notes] = ACTIONS(271),
    [anon_sym_X_COLON] = ACTIONS(271),
    [sym_remark] = ACTIONS(271),
    [sym_rhythm_line] = ACTIONS(271),
    [anon_sym_s_COLON] = ACTIONS(271),
    [sym_tempo] = ACTIONS(271),
    [sym_tune_title] = ACTIONS(271),
    [sym_unit_note_length] = ACTIONS(271),
    [anon_sym_U_COLON] = ACTIONS(271),
    [sym_voice] = ACTIONS(271),
    [sym_words_tag] = ACTIONS(271),
    [sym_words_postbody] = ACTIONS(271),
  },
  [22] = {
    [sym_nth_ending_number] = STATE(58),
    [ts_builtin_sym_end] = ACTIONS(277),
    [sym__NL] = ACTIONS(279),
    [anon_sym_BSLASH] = ACTIONS(277),
    [anon_sym_LBRACK] = ACTIONS(281),
    [sym_COMMENT] = ACTIONS(279),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(277),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(283),
    [sym_slur_open] = ACTIONS(279),
    [sym_slur_close] = ACTIONS(277),
    [sym_alteration] = ACTIONS(277),
    [sym_note_letter] = ACTIONS(277),
    [sym_rest] = ACTIONS(277),
    [anon_sym_Z] = ACTIONS(277),
    [anon_sym_X] = ACTIONS(279),
    [aux_sym_grace_note_token1] = ACTIONS(277),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(277),
    [sym_tuplet_marker] = ACTIONS(277),
    [sym_decoration_shorthand] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(277),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(277),
    [sym_start_of_repeated_section] = ACTIONS(277),
    [sym_end_of_repeated_section] = ACTIONS(277),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(277),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(285),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(287),
    [anon_sym_BANGtrill_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGmordent_BANG] = ACTIONS(277),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(277),
    [anon_sym_BANGroll_BANG] = ACTIONS(277),
    [anon_sym_BANGturn_BANG] = ACTIONS(277),
    [anon_sym_BANGturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_BANG] = ACTIONS(277),
    [anon_sym_BANGaccent_BANG] = ACTIONS(277),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(277),
    [anon_sym_BANGfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(277),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(277),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(277),
    [anon_sym_BANGplus_BANG] = ACTIONS(277),
    [anon_sym_BANGsnap_BANG] = ACTIONS(277),
    [anon_sym_BANGslide_BANG] = ACTIONS(277),
    [anon_sym_BANGwedge_BANG] = ACTIONS(277),
    [anon_sym_BANGupbow_BANG] = ACTIONS(277),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(277),
    [anon_sym_BANGopen_BANG] = ACTIONS(277),
    [anon_sym_BANGthumb_BANG] = ACTIONS(277),
    [anon_sym_BANGbreath_BANG] = ACTIONS(277),
    [anon_sym_BANGpppp_BANG] = ACTIONS(277),
    [anon_sym_BANGppp_BANG] = ACTIONS(277),
    [anon_sym_BANGpp_BANG] = ACTIONS(277),
    [anon_sym_BANGp_BANG] = ACTIONS(277),
    [anon_sym_BANGmp_BANG] = ACTIONS(277),
    [anon_sym_BANGmf_BANG] = ACTIONS(277),
    [anon_sym_BANGf_BANG] = ACTIONS(277),
    [anon_sym_BANGff_BANG] = ACTIONS(277),
    [anon_sym_BANGfff_BANG] = ACTIONS(277),
    [anon_sym_BANGffff_BANG] = ACTIONS(277),
    [anon_sym_BANGsfz_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGsegno_BANG] = ACTIONS(277),
    [anon_sym_BANGcoda_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(277),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(277),
    [anon_sym_BANGfine_BANG] = ACTIONS(277),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(277),
    [sym_parts_line] = ACTIONS(277),
    [sym_instruction] = ACTIONS(277),
    [sym_key] = ACTIONS(277),
    [sym_macros] = ACTIONS(277),
    [sym_meter] = ACTIONS(277),
    [sym_notes] = ACTIONS(277),
    [anon_sym_X_COLON] = ACTIONS(277),
    [sym_remark] = ACTIONS(277),
    [sym_rhythm_line] = ACTIONS(277),
    [anon_sym_s_COLON] = ACTIONS(277),
    [sym_tempo] = ACTIONS(277),
    [sym_tune_title] = ACTIONS(277),
    [sym_unit_note_length] = ACTIONS(277),
    [anon_sym_U_COLON] = ACTIONS(277),
    [sym_voice] = ACTIONS(277),
    [sym_words_tag] = ACTIONS(277),
    [sym_words_postbody] = ACTIONS(277),
  },
  [23] = {
    [sym_nth_ending_number] = STATE(58),
    [ts_builtin_sym_end] = ACTIONS(277),
    [sym__NL] = ACTIONS(279),
    [anon_sym_BSLASH] = ACTIONS(277),
    [anon_sym_LBRACK] = ACTIONS(281),
    [sym_COMMENT] = ACTIONS(279),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(277),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(283),
    [sym_slur_open] = ACTIONS(279),
    [sym_slur_close] = ACTIONS(277),
    [sym_alteration] = ACTIONS(277),
    [sym_note_letter] = ACTIONS(277),
    [sym_rest] = ACTIONS(277),
    [anon_sym_Z] = ACTIONS(277),
    [anon_sym_X] = ACTIONS(279),
    [aux_sym_grace_note_token1] = ACTIONS(277),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(277),
    [sym_tuplet_marker] = ACTIONS(277),
    [sym_decoration_shorthand] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(277),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(277),
    [sym_start_of_repeated_section] = ACTIONS(277),
    [sym_end_of_repeated_section] = ACTIONS(277),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(277),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(289),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(291),
    [anon_sym_BANGtrill_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGmordent_BANG] = ACTIONS(277),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(277),
    [anon_sym_BANGroll_BANG] = ACTIONS(277),
    [anon_sym_BANGturn_BANG] = ACTIONS(277),
    [anon_sym_BANGturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_BANG] = ACTIONS(277),
    [anon_sym_BANGaccent_BANG] = ACTIONS(277),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(277),
    [anon_sym_BANGfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(277),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(277),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(277),
    [anon_sym_BANGplus_BANG] = ACTIONS(277),
    [anon_sym_BANGsnap_BANG] = ACTIONS(277),
    [anon_sym_BANGslide_BANG] = ACTIONS(277),
    [anon_sym_BANGwedge_BANG] = ACTIONS(277),
    [anon_sym_BANGupbow_BANG] = ACTIONS(277),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(277),
    [anon_sym_BANGopen_BANG] = ACTIONS(277),
    [anon_sym_BANGthumb_BANG] = ACTIONS(277),
    [anon_sym_BANGbreath_BANG] = ACTIONS(277),
    [anon_sym_BANGpppp_BANG] = ACTIONS(277),
    [anon_sym_BANGppp_BANG] = ACTIONS(277),
    [anon_sym_BANGpp_BANG] = ACTIONS(277),
    [anon_sym_BANGp_BANG] = ACTIONS(277),
    [anon_sym_BANGmp_BANG] = ACTIONS(277),
    [anon_sym_BANGmf_BANG] = ACTIONS(277),
    [anon_sym_BANGf_BANG] = ACTIONS(277),
    [anon_sym_BANGff_BANG] = ACTIONS(277),
    [anon_sym_BANGfff_BANG] = ACTIONS(277),
    [anon_sym_BANGffff_BANG] = ACTIONS(277),
    [anon_sym_BANGsfz_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGsegno_BANG] = ACTIONS(277),
    [anon_sym_BANGcoda_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(277),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(277),
    [anon_sym_BANGfine_BANG] = ACTIONS(277),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(277),
    [sym_parts_line] = ACTIONS(277),
    [sym_instruction] = ACTIONS(277),
    [sym_key] = ACTIONS(277),
    [sym_macros] = ACTIONS(277),
    [sym_meter] = ACTIONS(277),
    [sym_notes] = ACTIONS(277),
    [anon_sym_X_COLON] = ACTIONS(277),
    [sym_remark] = ACTIONS(277),
    [sym_rhythm_line] = ACTIONS(277),
    [anon_sym_s_COLON] = ACTIONS(277),
    [sym_tempo] = ACTIONS(277),
    [sym_tune_title] = ACTIONS(277),
    [sym_unit_note_length] = ACTIONS(277),
    [anon_sym_U_COLON] = ACTIONS(277),
    [sym_voice] = ACTIONS(277),
    [sym_words_tag] = ACTIONS(277),
    [sym_words_postbody] = ACTIONS(277),
  },
  [24] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(21),
    [ts_builtin_sym_end] = ACTIONS(293),
    [sym__NL] = ACTIONS(295),
    [anon_sym_BSLASH] = ACTIONS(293),
    [anon_sym_DASH] = ACTIONS(275),
    [anon_sym_LBRACK] = ACTIONS(295),
    [sym_COMMENT] = ACTIONS(295),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(293),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(295),
    [sym_slur_open] = ACTIONS(295),
    [sym_slur_close] = ACTIONS(293),
    [sym_alteration] = ACTIONS(293),
    [sym_note_letter] = ACTIONS(293),
    [sym_rest] = ACTIONS(293),
    [anon_sym_Z] = ACTIONS(293),
    [anon_sym_X] = ACTIONS(295),
    [aux_sym_grace_note_token1] = ACTIONS(293),
    [aux_sym_chord_symbol_token1] = ACTIONS(295),
    [sym_annotation] = ACTIONS(293),
    [sym_tuplet_marker] = ACTIONS(293),
    [sym_decoration_shorthand] = ACTIONS(295),
    [anon_sym_COMMA] = ACTIONS(275),
    [sym_bar_line] = ACTIONS(295),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(293),
    [sym_thin_double_bar_line] = ACTIONS(293),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(293),
    [sym_start_of_repeated_section] = ACTIONS(293),
    [sym_end_of_repeated_section] = ACTIONS(293),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(293),
    [anon_sym_BANGtrill_BANG] = ACTIONS(293),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(293),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(293),
    [anon_sym_BANGmordent_BANG] = ACTIONS(293),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(293),
    [anon_sym_BANGroll_BANG] = ACTIONS(293),
    [anon_sym_BANGturn_BANG] = ACTIONS(293),
    [anon_sym_BANGturnx_BANG] = ACTIONS(293),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(293),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(293),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(293),
    [anon_sym_BANG_GT_BANG] = ACTIONS(293),
    [anon_sym_BANGaccent_BANG] = ACTIONS(293),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(293),
    [anon_sym_BANGfermata_BANG] = ACTIONS(293),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(293),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(293),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(293),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(293),
    [anon_sym_BANGplus_BANG] = ACTIONS(293),
    [anon_sym_BANGsnap_BANG] = ACTIONS(293),
    [anon_sym_BANGslide_BANG] = ACTIONS(293),
    [anon_sym_BANGwedge_BANG] = ACTIONS(293),
    [anon_sym_BANGupbow_BANG] = ACTIONS(293),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(293),
    [anon_sym_BANGopen_BANG] = ACTIONS(293),
    [anon_sym_BANGthumb_BANG] = ACTIONS(293),
    [anon_sym_BANGbreath_BANG] = ACTIONS(293),
    [anon_sym_BANGpppp_BANG] = ACTIONS(293),
    [anon_sym_BANGppp_BANG] = ACTIONS(293),
    [anon_sym_BANGpp_BANG] = ACTIONS(293),
    [anon_sym_BANGp_BANG] = ACTIONS(293),
    [anon_sym_BANGmp_BANG] = ACTIONS(293),
    [anon_sym_BANGmf_BANG] = ACTIONS(293),
    [anon_sym_BANGf_BANG] = ACTIONS(293),
    [anon_sym_BANGff_BANG] = ACTIONS(293),
    [anon_sym_BANGfff_BANG] = ACTIONS(293),
    [anon_sym_BANGffff_BANG] = ACTIONS(293),
    [anon_sym_BANGsfz_BANG] = ACTIONS(293),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(293),
    [anon_sym_BANGsegno_BANG] = ACTIONS(293),
    [anon_sym_BANGcoda_BANG] = ACTIONS(293),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(293),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(293),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(293),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(293),
    [anon_sym_BANGfine_BANG] = ACTIONS(293),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(293),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(293),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(293),
    [sym_parts_line] = ACTIONS(293),
    [sym_instruction] = ACTIONS(293),
    [sym_key] = ACTIONS(293),
    [sym_macros] = ACTIONS(293),
    [sym_meter] = ACTIONS(293),
    [sym_notes] = ACTIONS(293),
    [anon_sym_X_COLON] = ACTIONS(293),
    [sym_remark] = ACTIONS(293),
    [sym_rhythm_line] = ACTIONS(293),
    [anon_sym_s_COLON] = ACTIONS(293),
    [sym_tempo] = ACTIONS(293),
    [sym_tune_title] = ACTIONS(293),
    [sym_unit_note_length] = ACTIONS(293),
    [anon_sym_U_COLON] = ACTIONS(293),
    [sym_voice] = ACTIONS(293),
    [sym_words_tag] = ACTIONS(293),
    [sym_words_postbody] = ACTIONS(293),
  },
  [25] = {
    [ts_builtin_sym_end] = ACTIONS(297),
    [sym__NL] = ACTIONS(299),
    [anon_sym_BSLASH] = ACTIONS(297),
    [sym_BEAM_SEPARATOR] = ACTIONS(297),
    [anon_sym_LBRACK] = ACTIONS(299),
    [sym_COMMENT] = ACTIONS(299),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(297),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(299),
    [sym_slur_open] = ACTIONS(299),
    [sym_slur_close] = ACTIONS(297),
    [sym_alteration] = ACTIONS(297),
    [sym_note_letter] = ACTIONS(297),
    [sym_rest] = ACTIONS(297),
    [anon_sym_Z] = ACTIONS(297),
    [anon_sym_X] = ACTIONS(299),
    [sym_rhythm] = ACTIONS(301),
    [aux_sym_grace_note_token1] = ACTIONS(297),
    [aux_sym_chord_symbol_token1] = ACTIONS(299),
    [sym_annotation] = ACTIONS(297),
    [sym_tuplet_marker] = ACTIONS(297),
    [sym_decoration_shorthand] = ACTIONS(299),
    [sym_bar_line] = ACTIONS(299),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(297),
    [sym_thin_double_bar_line] = ACTIONS(297),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(297),
    [sym_start_of_repeated_section] = ACTIONS(297),
    [sym_end_of_repeated_section] = ACTIONS(297),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(297),
    [anon_sym_BANGtrill_BANG] = ACTIONS(297),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(297),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(297),
    [anon_sym_BANGmordent_BANG] = ACTIONS(297),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(297),
    [anon_sym_BANGroll_BANG] = ACTIONS(297),
    [anon_sym_BANGturn_BANG] = ACTIONS(297),
    [anon_sym_BANGturnx_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(297),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_BANG] = ACTIONS(297),
    [anon_sym_BANGaccent_BANG] = ACTIONS(297),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(297),
    [anon_sym_BANGfermata_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(297),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(297),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(297),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(297),
    [anon_sym_BANGplus_BANG] = ACTIONS(297),
    [anon_sym_BANGsnap_BANG] = ACTIONS(297),
    [anon_sym_BANGslide_BANG] = ACTIONS(297),
    [anon_sym_BANGwedge_BANG] = ACTIONS(297),
    [anon_sym_BANGupbow_BANG] = ACTIONS(297),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(297),
    [anon_sym_BANGopen_BANG] = ACTIONS(297),
    [anon_sym_BANGthumb_BANG] = ACTIONS(297),
    [anon_sym_BANGbreath_BANG] = ACTIONS(297),
    [anon_sym_BANGpppp_BANG] = ACTIONS(297),
    [anon_sym_BANGppp_BANG] = ACTIONS(297),
    [anon_sym_BANGpp_BANG] = ACTIONS(297),
    [anon_sym_BANGp_BANG] = ACTIONS(297),
    [anon_sym_BANGmp_BANG] = ACTIONS(297),
    [anon_sym_BANGmf_BANG] = ACTIONS(297),
    [anon_sym_BANGf_BANG] = ACTIONS(297),
    [anon_sym_BANGff_BANG] = ACTIONS(297),
    [anon_sym_BANGfff_BANG] = ACTIONS(297),
    [anon_sym_BANGffff_BANG] = ACTIONS(297),
    [anon_sym_BANGsfz_BANG] = ACTIONS(297),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGsegno_BANG] = ACTIONS(297),
    [anon_sym_BANGcoda_BANG] = ACTIONS(297),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(297),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(297),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(297),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(297),
    [anon_sym_BANGfine_BANG] = ACTIONS(297),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(297),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(297),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(297),
    [sym_parts_line] = ACTIONS(297),
    [sym_instruction] = ACTIONS(297),
    [sym_key] = ACTIONS(297),
    [sym_macros] = ACTIONS(297),
    [sym_meter] = ACTIONS(297),
    [sym_notes] = ACTIONS(297),
    [anon_sym_X_COLON] = ACTIONS(297),
    [sym_remark] = ACTIONS(297),
    [sym_rhythm_line] = ACTIONS(297),
    [anon_sym_s_COLON] = ACTIONS(297),
    [sym_tempo] = ACTIONS(297),
    [sym_tune_title] = ACTIONS(297),
    [sym_unit_note_length] = ACTIONS(297),
    [anon_sym_U_COLON] = ACTIONS(297),
    [sym_voice] = ACTIONS(297),
    [sym_words_tag] = ACTIONS(297),
    [sym_words_postbody] = ACTIONS(297),
  },
  [26] = {
    [ts_builtin_sym_end] = ACTIONS(303),
    [sym__NL] = ACTIONS(305),
    [anon_sym_BSLASH] = ACTIONS(303),
    [sym_BEAM_SEPARATOR] = ACTIONS(303),
    [anon_sym_LBRACK] = ACTIONS(305),
    [sym_COMMENT] = ACTIONS(305),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(303),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(305),
    [sym_slur_open] = ACTIONS(305),
    [sym_slur_close] = ACTIONS(303),
    [sym_alteration] = ACTIONS(303),
    [sym_note_letter] = ACTIONS(303),
    [sym_rest] = ACTIONS(303),
    [anon_sym_Z] = ACTIONS(303),
    [anon_sym_X] = ACTIONS(305),
    [sym_rhythm] = ACTIONS(307),
    [aux_sym_grace_note_token1] = ACTIONS(303),
    [aux_sym_chord_symbol_token1] = ACTIONS(305),
    [sym_annotation] = ACTIONS(303),
    [sym_tuplet_marker] = ACTIONS(303),
    [sym_decoration_shorthand] = ACTIONS(305),
    [sym_bar_line] = ACTIONS(305),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(303),
    [sym_thin_double_bar_line] = ACTIONS(303),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(303),
    [sym_start_of_repeated_section] = ACTIONS(303),
    [sym_end_of_repeated_section] = ACTIONS(303),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(303),
    [anon_sym_BANGtrill_BANG] = ACTIONS(303),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(303),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(303),
    [anon_sym_BANGmordent_BANG] = ACTIONS(303),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(303),
    [anon_sym_BANGroll_BANG] = ACTIONS(303),
    [anon_sym_BANGturn_BANG] = ACTIONS(303),
    [anon_sym_BANGturnx_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(303),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_BANG] = ACTIONS(303),
    [anon_sym_BANGaccent_BANG] = ACTIONS(303),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(303),
    [anon_sym_BANGfermata_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(303),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(303),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(303),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(303),
    [anon_sym_BANGplus_BANG] = ACTIONS(303),
    [anon_sym_BANGsnap_BANG] = ACTIONS(303),
    [anon_sym_BANGslide_BANG] = ACTIONS(303),
    [anon_sym_BANGwedge_BANG] = ACTIONS(303),
    [anon_sym_BANGupbow_BANG] = ACTIONS(303),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(303),
    [anon_sym_BANGopen_BANG] = ACTIONS(303),
    [anon_sym_BANGthumb_BANG] = ACTIONS(303),
    [anon_sym_BANGbreath_BANG] = ACTIONS(303),
    [anon_sym_BANGpppp_BANG] = ACTIONS(303),
    [anon_sym_BANGppp_BANG] = ACTIONS(303),
    [anon_sym_BANGpp_BANG] = ACTIONS(303),
    [anon_sym_BANGp_BANG] = ACTIONS(303),
    [anon_sym_BANGmp_BANG] = ACTIONS(303),
    [anon_sym_BANGmf_BANG] = ACTIONS(303),
    [anon_sym_BANGf_BANG] = ACTIONS(303),
    [anon_sym_BANGff_BANG] = ACTIONS(303),
    [anon_sym_BANGfff_BANG] = ACTIONS(303),
    [anon_sym_BANGffff_BANG] = ACTIONS(303),
    [anon_sym_BANGsfz_BANG] = ACTIONS(303),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGsegno_BANG] = ACTIONS(303),
    [anon_sym_BANGcoda_BANG] = ACTIONS(303),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(303),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(303),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(303),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(303),
    [anon_sym_BANGfine_BANG] = ACTIONS(303),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(303),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(303),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(303),
    [sym_parts_line] = ACTIONS(303),
    [sym_instruction] = ACTIONS(303),
    [sym_key] = ACTIONS(303),
    [sym_macros] = ACTIONS(303),
    [sym_meter] = ACTIONS(303),
    [sym_notes] = ACTIONS(303),
    [anon_sym_X_COLON] = ACTIONS(303),
    [sym_remark] = ACTIONS(303),
    [sym_rhythm_line] = ACTIONS(303),
    [anon_sym_s_COLON] = ACTIONS(303),
    [sym_tempo] = ACTIONS(303),
    [sym_tune_title] = ACTIONS(303),
    [sym_unit_note_length] = ACTIONS(303),
    [anon_sym_U_COLON] = ACTIONS(303),
    [sym_voice] = ACTIONS(303),
    [sym_words_tag] = ACTIONS(303),
    [sym_words_postbody] = ACTIONS(303),
  },
  [27] = {
    [ts_builtin_sym_end] = ACTIONS(309),
    [sym__NL] = ACTIONS(311),
    [anon_sym_BSLASH] = ACTIONS(309),
    [sym_BEAM_SEPARATOR] = ACTIONS(309),
    [anon_sym_LBRACK] = ACTIONS(311),
    [sym_COMMENT] = ACTIONS(311),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(309),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(311),
    [sym_slur_open] = ACTIONS(311),
    [sym_slur_close] = ACTIONS(309),
    [aux_sym_note_construct_token1] = ACTIONS(313),
    [sym_alteration] = ACTIONS(309),
    [sym_note_letter] = ACTIONS(309),
    [sym_rest] = ACTIONS(309),
    [anon_sym_Z] = ACTIONS(309),
    [anon_sym_X] = ACTIONS(311),
    [aux_sym_grace_note_token1] = ACTIONS(309),
    [aux_sym_chord_symbol_token1] = ACTIONS(311),
    [sym_annotation] = ACTIONS(309),
    [sym_tuplet_marker] = ACTIONS(309),
    [sym_decoration_shorthand] = ACTIONS(311),
    [sym_bar_line] = ACTIONS(311),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(309),
    [sym_thin_double_bar_line] = ACTIONS(309),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(309),
    [sym_start_of_repeated_section] = ACTIONS(309),
    [sym_end_of_repeated_section] = ACTIONS(309),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(309),
    [anon_sym_BANGtrill_BANG] = ACTIONS(309),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(309),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(309),
    [anon_sym_BANGmordent_BANG] = ACTIONS(309),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(309),
    [anon_sym_BANGroll_BANG] = ACTIONS(309),
    [anon_sym_BANGturn_BANG] = ACTIONS(309),
    [anon_sym_BANGturnx_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(309),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_BANG] = ACTIONS(309),
    [anon_sym_BANGaccent_BANG] = ACTIONS(309),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(309),
    [anon_sym_BANGfermata_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(309),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(309),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(309),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(309),
    [anon_sym_BANGplus_BANG] = ACTIONS(309),
    [anon_sym_BANGsnap_BANG] = ACTIONS(309),
    [anon_sym_BANGslide_BANG] = ACTIONS(309),
    [anon_sym_BANGwedge_BANG] = ACTIONS(309),
    [anon_sym_BANGupbow_BANG] = ACTIONS(309),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(309),
    [anon_sym_BANGopen_BANG] = ACTIONS(309),
    [anon_sym_BANGthumb_BANG] = ACTIONS(309),
    [anon_sym_BANGbreath_BANG] = ACTIONS(309),
    [anon_sym_BANGpppp_BANG] = ACTIONS(309),
    [anon_sym_BANGppp_BANG] = ACTIONS(309),
    [anon_sym_BANGpp_BANG] = ACTIONS(309),
    [anon_sym_BANGp_BANG] = ACTIONS(309),
    [anon_sym_BANGmp_BANG] = ACTIONS(309),
    [anon_sym_BANGmf_BANG] = ACTIONS(309),
    [anon_sym_BANGf_BANG] = ACTIONS(309),
    [anon_sym_BANGff_BANG] = ACTIONS(309),
    [anon_sym_BANGfff_BANG] = ACTIONS(309),
    [anon_sym_BANGffff_BANG] = ACTIONS(309),
    [anon_sym_BANGsfz_BANG] = ACTIONS(309),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGsegno_BANG] = ACTIONS(309),
    [anon_sym_BANGcoda_BANG] = ACTIONS(309),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(309),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(309),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(309),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(309),
    [anon_sym_BANGfine_BANG] = ACTIONS(309),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(309),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(309),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(309),
    [sym_parts_line] = ACTIONS(309),
    [sym_instruction] = ACTIONS(309),
    [sym_key] = ACTIONS(309),
    [sym_macros] = ACTIONS(309),
    [sym_meter] = ACTIONS(309),
    [sym_notes] = ACTIONS(309),
    [anon_sym_X_COLON] = ACTIONS(309),
    [sym_remark] = ACTIONS(309),
    [sym_rhythm_line] = ACTIONS(309),
    [anon_sym_s_COLON] = ACTIONS(309),
    [sym_tempo] = ACTIONS(309),
    [sym_tune_title] = ACTIONS(309),
    [sym_unit_note_length] = ACTIONS(309),
    [anon_sym_U_COLON] = ACTIONS(309),
    [sym_voice] = ACTIONS(309),
    [sym_words_tag] = ACTIONS(309),
    [sym_words_postbody] = ACTIONS(309),
  },
  [28] = {
    [ts_builtin_sym_end] = ACTIONS(315),
    [sym__NL] = ACTIONS(317),
    [anon_sym_BSLASH] = ACTIONS(315),
    [sym_BEAM_SEPARATOR] = ACTIONS(315),
    [anon_sym_LBRACK] = ACTIONS(317),
    [sym_COMMENT] = ACTIONS(317),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(315),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(317),
    [sym_slur_open] = ACTIONS(317),
    [sym_slur_close] = ACTIONS(315),
    [sym_alteration] = ACTIONS(315),
    [sym_note_letter] = ACTIONS(315),
    [sym_rest] = ACTIONS(315),
    [anon_sym_Z] = ACTIONS(315),
    [anon_sym_X] = ACTIONS(317),
    [sym_rhythm] = ACTIONS(315),
    [aux_sym_grace_note_token1] = ACTIONS(315),
    [aux_sym_chord_symbol_token1] = ACTIONS(317),
    [sym_annotation] = ACTIONS(315),
    [sym_tuplet_marker] = ACTIONS(315),
    [sym_decoration_shorthand] = ACTIONS(317),
    [sym_bar_line] = ACTIONS(317),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(315),
    [sym_thin_double_bar_line] = ACTIONS(315),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(315),
    [sym_start_of_repeated_section] = ACTIONS(315),
    [sym_end_of_repeated_section] = ACTIONS(315),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(315),
    [anon_sym_BANGtrill_BANG] = ACTIONS(315),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(315),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(315),
    [anon_sym_BANGmordent_BANG] = ACTIONS(315),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(315),
    [anon_sym_BANGroll_BANG] = ACTIONS(315),
    [anon_sym_BANGturn_BANG] = ACTIONS(315),
    [anon_sym_BANGturnx_BANG] = ACTIONS(315),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(315),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(315),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(315),
    [anon_sym_BANG_GT_BANG] = ACTIONS(315),
    [anon_sym_BANGaccent_BANG] = ACTIONS(315),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(315),
    [anon_sym_BANGfermata_BANG] = ACTIONS(315),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(315),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(315),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(315),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(315),
    [anon_sym_BANGplus_BANG] = ACTIONS(315),
    [anon_sym_BANGsnap_BANG] = ACTIONS(315),
    [anon_sym_BANGslide_BANG] = ACTIONS(315),
    [anon_sym_BANGwedge_BANG] = ACTIONS(315),
    [anon_sym_BANGupbow_BANG] = ACTIONS(315),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(315),
    [anon_sym_BANGopen_BANG] = ACTIONS(315),
    [anon_sym_BANGthumb_BANG] = ACTIONS(315),
    [anon_sym_BANGbreath_BANG] = ACTIONS(315),
    [anon_sym_BANGpppp_BANG] = ACTIONS(315),
    [anon_sym_BANGppp_BANG] = ACTIONS(315),
    [anon_sym_BANGpp_BANG] = ACTIONS(315),
    [anon_sym_BANGp_BANG] = ACTIONS(315),
    [anon_sym_BANGmp_BANG] = ACTIONS(315),
    [anon_sym_BANGmf_BANG] = ACTIONS(315),
    [anon_sym_BANGf_BANG] = ACTIONS(315),
    [anon_sym_BANGff_BANG] = ACTIONS(315),
    [anon_sym_BANGfff_BANG] = ACTIONS(315),
    [anon_sym_BANGffff_BANG] = ACTIONS(315),
    [anon_sym_BANGsfz_BANG] = ACTIONS(315),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(315),
    [anon_sym_BANGsegno_BANG] = ACTIONS(315),
    [anon_sym_BANGcoda_BANG] = ACTIONS(315),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(315),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(315),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(315),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(315),
    [anon_sym_BANGfine_BANG] = ACTIONS(315),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(315),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(315),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(315),
    [sym_parts_line] = ACTIONS(315),
    [sym_instruction] = ACTIONS(315),
    [sym_key] = ACTIONS(315),
    [sym_macros] = ACTIONS(315),
    [sym_meter] = ACTIONS(315),
    [sym_notes] = ACTIONS(315),
    [anon_sym_X_COLON] = ACTIONS(315),
    [sym_remark] = ACTIONS(315),
    [sym_rhythm_line] = ACTIONS(315),
    [anon_sym_s_COLON] = ACTIONS(315),
    [sym_tempo] = ACTIONS(315),
    [sym_tune_title] = ACTIONS(315),
    [sym_unit_note_length] = ACTIONS(315),
    [anon_sym_U_COLON] = ACTIONS(315),
    [sym_voice] = ACTIONS(315),
    [sym_words_tag] = ACTIONS(315),
    [sym_words_postbody] = ACTIONS(315),
  },
  [29] = {
    [ts_builtin_sym_end] = ACTIONS(319),
    [sym__NL] = ACTIONS(321),
    [anon_sym_BSLASH] = ACTIONS(319),
    [sym_BEAM_SEPARATOR] = ACTIONS(319),
    [anon_sym_LBRACK] = ACTIONS(321),
    [sym_COMMENT] = ACTIONS(321),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(319),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(321),
    [sym_slur_open] = ACTIONS(321),
    [sym_slur_close] = ACTIONS(319),
    [sym_alteration] = ACTIONS(319),
    [sym_note_letter] = ACTIONS(319),
    [sym_rest] = ACTIONS(319),
    [anon_sym_Z] = ACTIONS(319),
    [anon_sym_X] = ACTIONS(321),
    [sym_rhythm] = ACTIONS(323),
    [aux_sym_grace_note_token1] = ACTIONS(319),
    [aux_sym_chord_symbol_token1] = ACTIONS(321),
    [sym_annotation] = ACTIONS(319),
    [sym_tuplet_marker] = ACTIONS(319),
    [sym_decoration_shorthand] = ACTIONS(321),
    [sym_bar_line] = ACTIONS(321),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(319),
    [sym_thin_double_bar_line] = ACTIONS(319),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(319),
    [sym_start_of_repeated_section] = ACTIONS(319),
    [sym_end_of_repeated_section] = ACTIONS(319),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(319),
    [anon_sym_BANGtrill_BANG] = ACTIONS(319),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(319),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(319),
    [anon_sym_BANGmordent_BANG] = ACTIONS(319),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(319),
    [anon_sym_BANGroll_BANG] = ACTIONS(319),
    [anon_sym_BANGturn_BANG] = ACTIONS(319),
    [anon_sym_BANGturnx_BANG] = ACTIONS(319),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(319),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(319),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(319),
    [anon_sym_BANG_GT_BANG] = ACTIONS(319),
    [anon_sym_BANGaccent_BANG] = ACTIONS(319),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(319),
    [anon_sym_BANGfermata_BANG] = ACTIONS(319),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(319),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(319),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(319),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(319),
    [anon_sym_BANGplus_BANG] = ACTIONS(319),
    [anon_sym_BANGsnap_BANG] = ACTIONS(319),
    [anon_sym_BANGslide_BANG] = ACTIONS(319),
    [anon_sym_BANGwedge_BANG] = ACTIONS(319),
    [anon_sym_BANGupbow_BANG] = ACTIONS(319),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(319),
    [anon_sym_BANGopen_BANG] = ACTIONS(319),
    [anon_sym_BANGthumb_BANG] = ACTIONS(319),
    [anon_sym_BANGbreath_BANG] = ACTIONS(319),
    [anon_sym_BANGpppp_BANG] = ACTIONS(319),
    [anon_sym_BANGppp_BANG] = ACTIONS(319),
    [anon_sym_BANGpp_BANG] = ACTIONS(319),
    [anon_sym_BANGp_BANG] = ACTIONS(319),
    [anon_sym_BANGmp_BANG] = ACTIONS(319),
    [anon_sym_BANGmf_BANG] = ACTIONS(319),
    [anon_sym_BANGf_BANG] = ACTIONS(319),
    [anon_sym_BANGff_BANG] = ACTIONS(319),
    [anon_sym_BANGfff_BANG] = ACTIONS(319),
    [anon_sym_BANGffff_BANG] = ACTIONS(319),
    [anon_sym_BANGsfz_BANG] = ACTIONS(319),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(319),
    [anon_sym_BANGsegno_BANG] = ACTIONS(319),
    [anon_sym_BANGcoda_BANG] = ACTIONS(319),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(319),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(319),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(319),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(319),
    [anon_sym_BANGfine_BANG] = ACTIONS(319),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(319),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(319),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(319),
    [sym_parts_line] = ACTIONS(319),
    [sym_instruction] = ACTIONS(319),
    [sym_key] = ACTIONS(319),
    [sym_macros] = ACTIONS(319),
    [sym_meter] = ACTIONS(319),
    [sym_notes] = ACTIONS(319),
    [anon_sym_X_COLON] = ACTIONS(319),
    [sym_remark] = ACTIONS(319),
    [sym_rhythm_line] = ACTIONS(319),
    [anon_sym_s_COLON] = ACTIONS(319),
    [sym_tempo] = ACTIONS(319),
    [sym_tune_title] = ACTIONS(319),
    [sym_unit_note_length] = ACTIONS(319),
    [anon_sym_U_COLON] = ACTIONS(319),
    [sym_voice] = ACTIONS(319),
    [sym_words_tag] = ACTIONS(319),
    [sym_words_postbody] = ACTIONS(319),
  },
  [30] = {
    [ts_builtin_sym_end] = ACTIONS(325),
    [sym__NL] = ACTIONS(327),
    [anon_sym_BSLASH] = ACTIONS(325),
    [sym_BEAM_SEPARATOR] = ACTIONS(325),
    [anon_sym_LBRACK] = ACTIONS(327),
    [sym_COMMENT] = ACTIONS(327),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(325),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(327),
    [sym_slur_open] = ACTIONS(327),
    [sym_slur_close] = ACTIONS(325),
    [aux_sym_note_construct_token1] = ACTIONS(325),
    [sym_alteration] = ACTIONS(325),
    [sym_note_letter] = ACTIONS(325),
    [sym_rest] = ACTIONS(325),
    [anon_sym_Z] = ACTIONS(325),
    [anon_sym_X] = ACTIONS(327),
    [aux_sym_grace_note_token1] = ACTIONS(325),
    [aux_sym_chord_symbol_token1] = ACTIONS(327),
    [sym_annotation] = ACTIONS(325),
    [sym_tuplet_marker] = ACTIONS(325),
    [sym_decoration_shorthand] = ACTIONS(327),
    [sym_bar_line] = ACTIONS(327),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(325),
    [sym_thin_double_bar_line] = ACTIONS(325),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(325),
    [sym_start_of_repeated_section] = ACTIONS(325),
    [sym_end_of_repeated_section] = ACTIONS(325),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(325),
    [anon_sym_BANGtrill_BANG] = ACTIONS(325),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(325),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(325),
    [anon_sym_BANGmordent_BANG] = ACTIONS(325),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(325),
    [anon_sym_BANGroll_BANG] = ACTIONS(325),
    [anon_sym_BANGturn_BANG] = ACTIONS(325),
    [anon_sym_BANGturnx_BANG] = ACTIONS(325),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(325),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(325),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(325),
    [anon_sym_BANG_GT_BANG] = ACTIONS(325),
    [anon_sym_BANGaccent_BANG] = ACTIONS(325),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(325),
    [anon_sym_BANGfermata_BANG] = ACTIONS(325),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(325),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(325),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(325),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(325),
    [anon_sym_BANGplus_BANG] = ACTIONS(325),
    [anon_sym_BANGsnap_BANG] = ACTIONS(325),
    [anon_sym_BANGslide_BANG] = ACTIONS(325),
    [anon_sym_BANGwedge_BANG] = ACTIONS(325),
    [anon_sym_BANGupbow_BANG] = ACTIONS(325),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(325),
    [anon_sym_BANGopen_BANG] = ACTIONS(325),
    [anon_sym_BANGthumb_BANG] = ACTIONS(325),
    [anon_sym_BANGbreath_BANG] = ACTIONS(325),
    [anon_sym_BANGpppp_BANG] = ACTIONS(325),
    [anon_sym_BANGppp_BANG] = ACTIONS(325),
    [anon_sym_BANGpp_BANG] = ACTIONS(325),
    [anon_sym_BANGp_BANG] = ACTIONS(325),
    [anon_sym_BANGmp_BANG] = ACTIONS(325),
    [anon_sym_BANGmf_BANG] = ACTIONS(325),
    [anon_sym_BANGf_BANG] = ACTIONS(325),
    [anon_sym_BANGff_BANG] = ACTIONS(325),
    [anon_sym_BANGfff_BANG] = ACTIONS(325),
    [anon_sym_BANGffff_BANG] = ACTIONS(325),
    [anon_sym_BANGsfz_BANG] = ACTIONS(325),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(325),
    [anon_sym_BANGsegno_BANG] = ACTIONS(325),
    [anon_sym_BANGcoda_BANG] = ACTIONS(325),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(325),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(325),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(325),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(325),
    [anon_sym_BANGfine_BANG] = ACTIONS(325),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(325),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(325),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(325),
    [sym_parts_line] = ACTIONS(325),
    [sym_instruction] = ACTIONS(325),
    [sym_key] = ACTIONS(325),
    [sym_macros] = ACTIONS(325),
    [sym_meter] = ACTIONS(325),
    [sym_notes] = ACTIONS(325),
    [anon_sym_X_COLON] = ACTIONS(325),
    [sym_remark] = ACTIONS(325),
    [sym_rhythm_line] = ACTIONS(325),
    [anon_sym_s_COLON] = ACTIONS(325),
    [sym_tempo] = ACTIONS(325),
    [sym_tune_title] = ACTIONS(325),
    [sym_unit_note_length] = ACTIONS(325),
    [anon_sym_U_COLON] = ACTIONS(325),
    [sym_voice] = ACTIONS(325),
    [sym_words_tag] = ACTIONS(325),
    [sym_words_postbody] = ACTIONS(325),
  },
  [31] = {
    [ts_builtin_sym_end] = ACTIONS(329),
    [sym__NL] = ACTIONS(331),
    [anon_sym_BSLASH] = ACTIONS(329),
    [sym_BEAM_SEPARATOR] = ACTIONS(329),
    [anon_sym_LBRACK] = ACTIONS(331),
    [sym_COMMENT] = ACTIONS(331),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(329),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(331),
    [sym_slur_open] = ACTIONS(331),
    [sym_slur_close] = ACTIONS(329),
    [aux_sym_note_construct_token1] = ACTIONS(333),
    [sym_alteration] = ACTIONS(329),
    [sym_note_letter] = ACTIONS(329),
    [sym_rest] = ACTIONS(329),
    [anon_sym_Z] = ACTIONS(329),
    [anon_sym_X] = ACTIONS(331),
    [aux_sym_grace_note_token1] = ACTIONS(329),
    [aux_sym_chord_symbol_token1] = ACTIONS(331),
    [sym_annotation] = ACTIONS(329),
    [sym_tuplet_marker] = ACTIONS(329),
    [sym_decoration_shorthand] = ACTIONS(331),
    [sym_bar_line] = ACTIONS(331),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(329),
    [sym_thin_double_bar_line] = ACTIONS(329),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(329),
    [sym_start_of_repeated_section] = ACTIONS(329),
    [sym_end_of_repeated_section] = ACTIONS(329),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(329),
    [anon_sym_BANGtrill_BANG] = ACTIONS(329),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(329),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(329),
    [anon_sym_BANGmordent_BANG] = ACTIONS(329),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(329),
    [anon_sym_BANGroll_BANG] = ACTIONS(329),
    [anon_sym_BANGturn_BANG] = ACTIONS(329),
    [anon_sym_BANGturnx_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(329),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_BANG] = ACTIONS(329),
    [anon_sym_BANGaccent_BANG] = ACTIONS(329),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(329),
    [anon_sym_BANGfermata_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(329),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(329),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(329),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(329),
    [anon_sym_BANGplus_BANG] = ACTIONS(329),
    [anon_sym_BANGsnap_BANG] = ACTIONS(329),
    [anon_sym_BANGslide_BANG] = ACTIONS(329),
    [anon_sym_BANGwedge_BANG] = ACTIONS(329),
    [anon_sym_BANGupbow_BANG] = ACTIONS(329),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(329),
    [anon_sym_BANGopen_BANG] = ACTIONS(329),
    [anon_sym_BANGthumb_BANG] = ACTIONS(329),
    [anon_sym_BANGbreath_BANG] = ACTIONS(329),
    [anon_sym_BANGpppp_BANG] = ACTIONS(329),
    [anon_sym_BANGppp_BANG] = ACTIONS(329),
    [anon_sym_BANGpp_BANG] = ACTIONS(329),
    [anon_sym_BANGp_BANG] = ACTIONS(329),
    [anon_sym_BANGmp_BANG] = ACTIONS(329),
    [anon_sym_BANGmf_BANG] = ACTIONS(329),
    [anon_sym_BANGf_BANG] = ACTIONS(329),
    [anon_sym_BANGff_BANG] = ACTIONS(329),
    [anon_sym_BANGfff_BANG] = ACTIONS(329),
    [anon_sym_BANGffff_BANG] = ACTIONS(329),
    [anon_sym_BANGsfz_BANG] = ACTIONS(329),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGsegno_BANG] = ACTIONS(329),
    [anon_sym_BANGcoda_BANG] = ACTIONS(329),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(329),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(329),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(329),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(329),
    [anon_sym_BANGfine_BANG] = ACTIONS(329),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(329),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(329),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(329),
    [sym_parts_line] = ACTIONS(329),
    [sym_instruction] = ACTIONS(329),
    [sym_key] = ACTIONS(329),
    [sym_macros] = ACTIONS(329),
    [sym_meter] = ACTIONS(329),
    [sym_notes] = ACTIONS(329),
    [anon_sym_X_COLON] = ACTIONS(329),
    [sym_remark] = ACTIONS(329),
    [sym_rhythm_line] = ACTIONS(329),
    [anon_sym_s_COLON] = ACTIONS(329),
    [sym_tempo] = ACTIONS(329),
    [sym_tune_title] = ACTIONS(329),
    [sym_unit_note_length] = ACTIONS(329),
    [anon_sym_U_COLON] = ACTIONS(329),
    [sym_voice] = ACTIONS(329),
    [sym_words_tag] = ACTIONS(329),
    [sym_words_postbody] = ACTIONS(329),
  },
  [32] = {
    [ts_builtin_sym_end] = ACTIONS(335),
    [sym__NL] = ACTIONS(337),
    [anon_sym_BSLASH] = ACTIONS(335),
    [sym_BEAM_SEPARATOR] = ACTIONS(335),
    [anon_sym_LBRACK] = ACTIONS(337),
    [sym_COMMENT] = ACTIONS(337),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(335),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(337),
    [sym_slur_open] = ACTIONS(337),
    [sym_slur_close] = ACTIONS(335),
    [sym_alteration] = ACTIONS(335),
    [sym_note_letter] = ACTIONS(335),
    [sym_rest] = ACTIONS(335),
    [anon_sym_Z] = ACTIONS(335),
    [anon_sym_X] = ACTIONS(337),
    [sym_rhythm] = ACTIONS(339),
    [aux_sym_grace_note_token1] = ACTIONS(335),
    [aux_sym_chord_symbol_token1] = ACTIONS(337),
    [sym_annotation] = ACTIONS(335),
    [sym_tuplet_marker] = ACTIONS(335),
    [sym_decoration_shorthand] = ACTIONS(337),
    [sym_bar_line] = ACTIONS(337),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(335),
    [sym_thin_double_bar_line] = ACTIONS(335),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(335),
    [sym_start_of_repeated_section] = ACTIONS(335),
    [sym_end_of_repeated_section] = ACTIONS(335),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(335),
    [anon_sym_BANGtrill_BANG] = ACTIONS(335),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(335),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(335),
    [anon_sym_BANGmordent_BANG] = ACTIONS(335),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(335),
    [anon_sym_BANGroll_BANG] = ACTIONS(335),
    [anon_sym_BANGturn_BANG] = ACTIONS(335),
    [anon_sym_BANGturnx_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(335),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_BANG] = ACTIONS(335),
    [anon_sym_BANGaccent_BANG] = ACTIONS(335),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(335),
    [anon_sym_BANGfermata_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(335),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(335),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(335),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(335),
    [anon_sym_BANGplus_BANG] = ACTIONS(335),
    [anon_sym_BANGsnap_BANG] = ACTIONS(335),
    [anon_sym_BANGslide_BANG] = ACTIONS(335),
    [anon_sym_BANGwedge_BANG] = ACTIONS(335),
    [anon_sym_BANGupbow_BANG] = ACTIONS(335),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(335),
    [anon_sym_BANGopen_BANG] = ACTIONS(335),
    [anon_sym_BANGthumb_BANG] = ACTIONS(335),
    [anon_sym_BANGbreath_BANG] = ACTIONS(335),
    [anon_sym_BANGpppp_BANG] = ACTIONS(335),
    [anon_sym_BANGppp_BANG] = ACTIONS(335),
    [anon_sym_BANGpp_BANG] = ACTIONS(335),
    [anon_sym_BANGp_BANG] = ACTIONS(335),
    [anon_sym_BANGmp_BANG] = ACTIONS(335),
    [anon_sym_BANGmf_BANG] = ACTIONS(335),
    [anon_sym_BANGf_BANG] = ACTIONS(335),
    [anon_sym_BANGff_BANG] = ACTIONS(335),
    [anon_sym_BANGfff_BANG] = ACTIONS(335),
    [anon_sym_BANGffff_BANG] = ACTIONS(335),
    [anon_sym_BANGsfz_BANG] = ACTIONS(335),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGsegno_BANG] = ACTIONS(335),
    [anon_sym_BANGcoda_BANG] = ACTIONS(335),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(335),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(335),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(335),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(335),
    [anon_sym_BANGfine_BANG] = ACTIONS(335),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(335),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(335),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(335),
    [sym_parts_line] = ACTIONS(335),
    [sym_instruction] = ACTIONS(335),
    [sym_key] = ACTIONS(335),
    [sym_macros] = ACTIONS(335),
    [sym_meter] = ACTIONS(335),
    [sym_notes] = ACTIONS(335),
    [anon_sym_X_COLON] = ACTIONS(335),
    [sym_remark] = ACTIONS(335),
    [sym_rhythm_line] = ACTIONS(335),
    [anon_sym_s_COLON] = ACTIONS(335),
    [sym_tempo] = ACTIONS(335),
    [sym_tune_title] = ACTIONS(335),
    [sym_unit_note_length] = ACTIONS(335),
    [anon_sym_U_COLON] = ACTIONS(335),
    [sym_voice] = ACTIONS(335),
    [sym_words_tag] = ACTIONS(335),
    [sym_words_postbody] = ACTIONS(335),
  },
  [33] = {
    [ts_builtin_sym_end] = ACTIONS(341),
    [sym__NL] = ACTIONS(343),
    [anon_sym_BSLASH] = ACTIONS(341),
    [sym_BEAM_SEPARATOR] = ACTIONS(341),
    [anon_sym_LBRACK] = ACTIONS(343),
    [sym_COMMENT] = ACTIONS(343),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(341),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(343),
    [sym_slur_open] = ACTIONS(343),
    [sym_slur_close] = ACTIONS(341),
    [aux_sym_note_construct_token1] = ACTIONS(345),
    [sym_alteration] = ACTIONS(341),
    [sym_note_letter] = ACTIONS(341),
    [sym_rest] = ACTIONS(341),
    [anon_sym_Z] = ACTIONS(341),
    [anon_sym_X] = ACTIONS(343),
    [aux_sym_grace_note_token1] = ACTIONS(341),
    [aux_sym_chord_symbol_token1] = ACTIONS(343),
    [sym_annotation] = ACTIONS(341),
    [sym_tuplet_marker] = ACTIONS(341),
    [sym_decoration_shorthand] = ACTIONS(343),
    [sym_bar_line] = ACTIONS(343),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(341),
    [sym_thin_double_bar_line] = ACTIONS(341),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(341),
    [sym_start_of_repeated_section] = ACTIONS(341),
    [sym_end_of_repeated_section] = ACTIONS(341),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(341),
    [anon_sym_BANGtrill_BANG] = ACTIONS(341),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(341),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(341),
    [anon_sym_BANGmordent_BANG] = ACTIONS(341),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(341),
    [anon_sym_BANGroll_BANG] = ACTIONS(341),
    [anon_sym_BANGturn_BANG] = ACTIONS(341),
    [anon_sym_BANGturnx_BANG] = ACTIONS(341),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(341),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(341),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(341),
    [anon_sym_BANG_GT_BANG] = ACTIONS(341),
    [anon_sym_BANGaccent_BANG] = ACTIONS(341),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(341),
    [anon_sym_BANGfermata_BANG] = ACTIONS(341),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(341),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(341),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(341),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(341),
    [anon_sym_BANGplus_BANG] = ACTIONS(341),
    [anon_sym_BANGsnap_BANG] = ACTIONS(341),
    [anon_sym_BANGslide_BANG] = ACTIONS(341),
    [anon_sym_BANGwedge_BANG] = ACTIONS(341),
    [anon_sym_BANGupbow_BANG] = ACTIONS(341),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(341),
    [anon_sym_BANGopen_BANG] = ACTIONS(341),
    [anon_sym_BANGthumb_BANG] = ACTIONS(341),
    [anon_sym_BANGbreath_BANG] = ACTIONS(341),
    [anon_sym_BANGpppp_BANG] = ACTIONS(341),
    [anon_sym_BANGppp_BANG] = ACTIONS(341),
    [anon_sym_BANGpp_BANG] = ACTIONS(341),
    [anon_sym_BANGp_BANG] = ACTIONS(341),
    [anon_sym_BANGmp_BANG] = ACTIONS(341),
    [anon_sym_BANGmf_BANG] = ACTIONS(341),
    [anon_sym_BANGf_BANG] = ACTIONS(341),
    [anon_sym_BANGff_BANG] = ACTIONS(341),
    [anon_sym_BANGfff_BANG] = ACTIONS(341),
    [anon_sym_BANGffff_BANG] = ACTIONS(341),
    [anon_sym_BANGsfz_BANG] = ACTIONS(341),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(341),
    [anon_sym_BANGsegno_BANG] = ACTIONS(341),
    [anon_sym_BANGcoda_BANG] = ACTIONS(341),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(341),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(341),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(341),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(341),
    [anon_sym_BANGfine_BANG] = ACTIONS(341),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(341),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(341),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(341),
    [sym_parts_line] = ACTIONS(341),
    [sym_instruction] = ACTIONS(341),
    [sym_key] = ACTIONS(341),
    [sym_macros] = ACTIONS(341),
    [sym_meter] = ACTIONS(341),
    [sym_notes] = ACTIONS(341),
    [anon_sym_X_COLON] = ACTIONS(341),
    [sym_remark] = ACTIONS(341),
    [sym_rhythm_line] = ACTIONS(341),
    [anon_sym_s_COLON] = ACTIONS(341),
    [sym_tempo] = ACTIONS(341),
    [sym_tune_title] = ACTIONS(341),
    [sym_unit_note_length] = ACTIONS(341),
    [anon_sym_U_COLON] = ACTIONS(341),
    [sym_voice] = ACTIONS(341),
    [sym_words_tag] = ACTIONS(341),
    [sym_words_postbody] = ACTIONS(341),
  },
  [34] = {
    [ts_builtin_sym_end] = ACTIONS(347),
    [sym__NL] = ACTIONS(349),
    [anon_sym_BSLASH] = ACTIONS(347),
    [sym_BEAM_SEPARATOR] = ACTIONS(347),
    [anon_sym_LBRACK] = ACTIONS(349),
    [sym_COMMENT] = ACTIONS(349),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(347),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(349),
    [sym_slur_open] = ACTIONS(349),
    [sym_slur_close] = ACTIONS(347),
    [sym_alteration] = ACTIONS(347),
    [sym_note_letter] = ACTIONS(347),
    [sym_rest] = ACTIONS(347),
    [anon_sym_Z] = ACTIONS(347),
    [anon_sym_X] = ACTIONS(349),
    [sym_rhythm] = ACTIONS(351),
    [aux_sym_grace_note_token1] = ACTIONS(347),
    [aux_sym_chord_symbol_token1] = ACTIONS(349),
    [sym_annotation] = ACTIONS(347),
    [sym_tuplet_marker] = ACTIONS(347),
    [sym_decoration_shorthand] = ACTIONS(349),
    [sym_bar_line] = ACTIONS(349),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(347),
    [sym_thin_double_bar_line] = ACTIONS(347),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(347),
    [sym_start_of_repeated_section] = ACTIONS(347),
    [sym_end_of_repeated_section] = ACTIONS(347),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(347),
    [anon_sym_BANGtrill_BANG] = ACTIONS(347),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(347),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(347),
    [anon_sym_BANGmordent_BANG] = ACTIONS(347),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(347),
    [anon_sym_BANGroll_BANG] = ACTIONS(347),
    [anon_sym_BANGturn_BANG] = ACTIONS(347),
    [anon_sym_BANGturnx_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(347),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_BANG] = ACTIONS(347),
    [anon_sym_BANGaccent_BANG] = ACTIONS(347),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(347),
    [anon_sym_BANGfermata_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(347),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(347),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(347),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(347),
    [anon_sym_BANGplus_BANG] = ACTIONS(347),
    [anon_sym_BANGsnap_BANG] = ACTIONS(347),
    [anon_sym_BANGslide_BANG] = ACTIONS(347),
    [anon_sym_BANGwedge_BANG] = ACTIONS(347),
    [anon_sym_BANGupbow_BANG] = ACTIONS(347),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(347),
    [anon_sym_BANGopen_BANG] = ACTIONS(347),
    [anon_sym_BANGthumb_BANG] = ACTIONS(347),
    [anon_sym_BANGbreath_BANG] = ACTIONS(347),
    [anon_sym_BANGpppp_BANG] = ACTIONS(347),
    [anon_sym_BANGppp_BANG] = ACTIONS(347),
    [anon_sym_BANGpp_BANG] = ACTIONS(347),
    [anon_sym_BANGp_BANG] = ACTIONS(347),
    [anon_sym_BANGmp_BANG] = ACTIONS(347),
    [anon_sym_BANGmf_BANG] = ACTIONS(347),
    [anon_sym_BANGf_BANG] = ACTIONS(347),
    [anon_sym_BANGff_BANG] = ACTIONS(347),
    [anon_sym_BANGfff_BANG] = ACTIONS(347),
    [anon_sym_BANGffff_BANG] = ACTIONS(347),
    [anon_sym_BANGsfz_BANG] = ACTIONS(347),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGsegno_BANG] = ACTIONS(347),
    [anon_sym_BANGcoda_BANG] = ACTIONS(347),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(347),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(347),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(347),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(347),
    [anon_sym_BANGfine_BANG] = ACTIONS(347),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(347),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(347),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(347),
    [sym_parts_line] = ACTIONS(347),
    [sym_instruction] = ACTIONS(347),
    [sym_key] = ACTIONS(347),
    [sym_macros] = ACTIONS(347),
    [sym_meter] = ACTIONS(347),
    [sym_notes] = ACTIONS(347),
    [anon_sym_X_COLON] = ACTIONS(347),
    [sym_remark] = ACTIONS(347),
    [sym_rhythm_line] = ACTIONS(347),
    [anon_sym_s_COLON] = ACTIONS(347),
    [sym_tempo] = ACTIONS(347),
    [sym_tune_title] = ACTIONS(347),
    [sym_unit_note_length] = ACTIONS(347),
    [anon_sym_U_COLON] = ACTIONS(347),
    [sym_voice] = ACTIONS(347),
    [sym_words_tag] = ACTIONS(347),
    [sym_words_postbody] = ACTIONS(347),
  },
  [35] = {
    [ts_builtin_sym_end] = ACTIONS(353),
    [sym__NL] = ACTIONS(355),
    [anon_sym_BSLASH] = ACTIONS(353),
    [sym_BEAM_SEPARATOR] = ACTIONS(353),
    [anon_sym_LBRACK] = ACTIONS(355),
    [sym_COMMENT] = ACTIONS(355),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(353),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(355),
    [sym_slur_open] = ACTIONS(355),
    [sym_slur_close] = ACTIONS(353),
    [aux_sym_note_construct_token1] = ACTIONS(357),
    [sym_alteration] = ACTIONS(353),
    [sym_note_letter] = ACTIONS(353),
    [sym_rest] = ACTIONS(353),
    [anon_sym_Z] = ACTIONS(353),
    [anon_sym_X] = ACTIONS(355),
    [aux_sym_grace_note_token1] = ACTIONS(353),
    [aux_sym_chord_symbol_token1] = ACTIONS(355),
    [sym_annotation] = ACTIONS(353),
    [sym_tuplet_marker] = ACTIONS(353),
    [sym_decoration_shorthand] = ACTIONS(355),
    [sym_bar_line] = ACTIONS(355),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(353),
    [sym_thin_double_bar_line] = ACTIONS(353),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(353),
    [sym_start_of_repeated_section] = ACTIONS(353),
    [sym_end_of_repeated_section] = ACTIONS(353),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(353),
    [anon_sym_BANGtrill_BANG] = ACTIONS(353),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(353),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(353),
    [anon_sym_BANGmordent_BANG] = ACTIONS(353),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(353),
    [anon_sym_BANGroll_BANG] = ACTIONS(353),
    [anon_sym_BANGturn_BANG] = ACTIONS(353),
    [anon_sym_BANGturnx_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(353),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_BANG] = ACTIONS(353),
    [anon_sym_BANGaccent_BANG] = ACTIONS(353),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(353),
    [anon_sym_BANGfermata_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(353),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(353),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(353),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(353),
    [anon_sym_BANGplus_BANG] = ACTIONS(353),
    [anon_sym_BANGsnap_BANG] = ACTIONS(353),
    [anon_sym_BANGslide_BANG] = ACTIONS(353),
    [anon_sym_BANGwedge_BANG] = ACTIONS(353),
    [anon_sym_BANGupbow_BANG] = ACTIONS(353),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(353),
    [anon_sym_BANGopen_BANG] = ACTIONS(353),
    [anon_sym_BANGthumb_BANG] = ACTIONS(353),
    [anon_sym_BANGbreath_BANG] = ACTIONS(353),
    [anon_sym_BANGpppp_BANG] = ACTIONS(353),
    [anon_sym_BANGppp_BANG] = ACTIONS(353),
    [anon_sym_BANGpp_BANG] = ACTIONS(353),
    [anon_sym_BANGp_BANG] = ACTIONS(353),
    [anon_sym_BANGmp_BANG] = ACTIONS(353),
    [anon_sym_BANGmf_BANG] = ACTIONS(353),
    [anon_sym_BANGf_BANG] = ACTIONS(353),
    [anon_sym_BANGff_BANG] = ACTIONS(353),
    [anon_sym_BANGfff_BANG] = ACTIONS(353),
    [anon_sym_BANGffff_BANG] = ACTIONS(353),
    [anon_sym_BANGsfz_BANG] = ACTIONS(353),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGsegno_BANG] = ACTIONS(353),
    [anon_sym_BANGcoda_BANG] = ACTIONS(353),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(353),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(353),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(353),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(353),
    [anon_sym_BANGfine_BANG] = ACTIONS(353),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(353),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(353),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(353),
    [sym_parts_line] = ACTIONS(353),
    [sym_instruction] = ACTIONS(353),
    [sym_key] = ACTIONS(353),
    [sym_macros] = ACTIONS(353),
    [sym_meter] = ACTIONS(353),
    [sym_notes] = ACTIONS(353),
    [anon_sym_X_COLON] = ACTIONS(353),
    [sym_remark] = ACTIONS(353),
    [sym_rhythm_line] = ACTIONS(353),
    [anon_sym_s_COLON] = ACTIONS(353),
    [sym_tempo] = ACTIONS(353),
    [sym_tune_title] = ACTIONS(353),
    [sym_unit_note_length] = ACTIONS(353),
    [anon_sym_U_COLON] = ACTIONS(353),
    [sym_voice] = ACTIONS(353),
    [sym_words_tag] = ACTIONS(353),
    [sym_words_postbody] = ACTIONS(353),
  },
  [36] = {
    [ts_builtin_sym_end] = ACTIONS(264),
    [sym__NL] = ACTIONS(266),
    [anon_sym_BSLASH] = ACTIONS(264),
    [anon_sym_DASH] = ACTIONS(264),
    [anon_sym_LBRACK] = ACTIONS(266),
    [sym_COMMENT] = ACTIONS(266),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(264),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(266),
    [sym_slur_open] = ACTIONS(266),
    [sym_slur_close] = ACTIONS(264),
    [sym_alteration] = ACTIONS(264),
    [sym_note_letter] = ACTIONS(264),
    [sym_rest] = ACTIONS(264),
    [anon_sym_Z] = ACTIONS(264),
    [anon_sym_X] = ACTIONS(266),
    [aux_sym_grace_note_token1] = ACTIONS(264),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(264),
    [sym_tuplet_marker] = ACTIONS(264),
    [sym_decoration_shorthand] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(264),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(264),
    [sym_thin_double_bar_line] = ACTIONS(264),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(264),
    [sym_start_of_repeated_section] = ACTIONS(264),
    [sym_end_of_repeated_section] = ACTIONS(264),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(264),
    [anon_sym_BANGtrill_BANG] = ACTIONS(264),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(264),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(264),
    [anon_sym_BANGmordent_BANG] = ACTIONS(264),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(264),
    [anon_sym_BANGroll_BANG] = ACTIONS(264),
    [anon_sym_BANGturn_BANG] = ACTIONS(264),
    [anon_sym_BANGturnx_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(264),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_BANG] = ACTIONS(264),
    [anon_sym_BANGaccent_BANG] = ACTIONS(264),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(264),
    [anon_sym_BANGfermata_BANG] = ACTIONS(264),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(264),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(264),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(264),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(264),
    [anon_sym_BANGplus_BANG] = ACTIONS(264),
    [anon_sym_BANGsnap_BANG] = ACTIONS(264),
    [anon_sym_BANGslide_BANG] = ACTIONS(264),
    [anon_sym_BANGwedge_BANG] = ACTIONS(264),
    [anon_sym_BANGupbow_BANG] = ACTIONS(264),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(264),
    [anon_sym_BANGopen_BANG] = ACTIONS(264),
    [anon_sym_BANGthumb_BANG] = ACTIONS(264),
    [anon_sym_BANGbreath_BANG] = ACTIONS(264),
    [anon_sym_BANGpppp_BANG] = ACTIONS(264),
    [anon_sym_BANGppp_BANG] = ACTIONS(264),
    [anon_sym_BANGpp_BANG] = ACTIONS(264),
    [anon_sym_BANGp_BANG] = ACTIONS(264),
    [anon_sym_BANGmp_BANG] = ACTIONS(264),
    [anon_sym_BANGmf_BANG] = ACTIONS(264),
    [anon_sym_BANGf_BANG] = ACTIONS(264),
    [anon_sym_BANGff_BANG] = ACTIONS(264),
    [anon_sym_BANGfff_BANG] = ACTIONS(264),
    [anon_sym_BANGffff_BANG] = ACTIONS(264),
    [anon_sym_BANGsfz_BANG] = ACTIONS(264),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(264),
    [anon_sym_BANGsegno_BANG] = ACTIONS(264),
    [anon_sym_BANGcoda_BANG] = ACTIONS(264),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(264),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(264),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(264),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(264),
    [anon_sym_BANGfine_BANG] = ACTIONS(264),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(264),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(264),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(264),
    [sym_parts_line] = ACTIONS(264),
    [sym_instruction] = ACTIONS(264),
    [sym_key] = ACTIONS(264),
    [sym_macros] = ACTIONS(264),
    [sym_meter] = ACTIONS(264),
    [sym_notes] = ACTIONS(264),
    [anon_sym_X_COLON] = ACTIONS(264),
    [sym_remark] = ACTIONS(264),
    [sym_rhythm_line] = ACTIONS(264),
    [anon_sym_s_COLON] = ACTIONS(264),
    [sym_tempo] = ACTIONS(264),
    [sym_tune_title] = ACTIONS(264),
    [sym_unit_note_length] = ACTIONS(264),
    [anon_sym_U_COLON] = ACTIONS(264),
    [sym_voice] = ACTIONS(264),
    [sym_words_tag] = ACTIONS(264),
    [sym_words_postbody] = ACTIONS(264),
  },
  [37] = {
    [ts_builtin_sym_end] = ACTIONS(359),
    [sym__NL] = ACTIONS(361),
    [anon_sym_BSLASH] = ACTIONS(359),
    [sym_BEAM_SEPARATOR] = ACTIONS(359),
    [anon_sym_LBRACK] = ACTIONS(361),
    [sym_COMMENT] = ACTIONS(361),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(359),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(361),
    [sym_slur_open] = ACTIONS(361),
    [sym_slur_close] = ACTIONS(359),
    [aux_sym_note_construct_token1] = ACTIONS(363),
    [sym_alteration] = ACTIONS(359),
    [sym_note_letter] = ACTIONS(359),
    [sym_rest] = ACTIONS(359),
    [anon_sym_Z] = ACTIONS(359),
    [anon_sym_X] = ACTIONS(361),
    [aux_sym_grace_note_token1] = ACTIONS(359),
    [aux_sym_chord_symbol_token1] = ACTIONS(361),
    [sym_annotation] = ACTIONS(359),
    [sym_tuplet_marker] = ACTIONS(359),
    [sym_decoration_shorthand] = ACTIONS(361),
    [sym_bar_line] = ACTIONS(361),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(359),
    [sym_thin_double_bar_line] = ACTIONS(359),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(359),
    [sym_start_of_repeated_section] = ACTIONS(359),
    [sym_end_of_repeated_section] = ACTIONS(359),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(359),
    [anon_sym_BANGtrill_BANG] = ACTIONS(359),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(359),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(359),
    [anon_sym_BANGmordent_BANG] = ACTIONS(359),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(359),
    [anon_sym_BANGroll_BANG] = ACTIONS(359),
    [anon_sym_BANGturn_BANG] = ACTIONS(359),
    [anon_sym_BANGturnx_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(359),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_BANG] = ACTIONS(359),
    [anon_sym_BANGaccent_BANG] = ACTIONS(359),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(359),
    [anon_sym_BANGfermata_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(359),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(359),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(359),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(359),
    [anon_sym_BANGplus_BANG] = ACTIONS(359),
    [anon_sym_BANGsnap_BANG] = ACTIONS(359),
    [anon_sym_BANGslide_BANG] = ACTIONS(359),
    [anon_sym_BANGwedge_BANG] = ACTIONS(359),
    [anon_sym_BANGupbow_BANG] = ACTIONS(359),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(359),
    [anon_sym_BANGopen_BANG] = ACTIONS(359),
    [anon_sym_BANGthumb_BANG] = ACTIONS(359),
    [anon_sym_BANGbreath_BANG] = ACTIONS(359),
    [anon_sym_BANGpppp_BANG] = ACTIONS(359),
    [anon_sym_BANGppp_BANG] = ACTIONS(359),
    [anon_sym_BANGpp_BANG] = ACTIONS(359),
    [anon_sym_BANGp_BANG] = ACTIONS(359),
    [anon_sym_BANGmp_BANG] = ACTIONS(359),
    [anon_sym_BANGmf_BANG] = ACTIONS(359),
    [anon_sym_BANGf_BANG] = ACTIONS(359),
    [anon_sym_BANGff_BANG] = ACTIONS(359),
    [anon_sym_BANGfff_BANG] = ACTIONS(359),
    [anon_sym_BANGffff_BANG] = ACTIONS(359),
    [anon_sym_BANGsfz_BANG] = ACTIONS(359),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGsegno_BANG] = ACTIONS(359),
    [anon_sym_BANGcoda_BANG] = ACTIONS(359),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(359),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(359),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(359),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(359),
    [anon_sym_BANGfine_BANG] = ACTIONS(359),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(359),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(359),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(359),
    [sym_parts_line] = ACTIONS(359),
    [sym_instruction] = ACTIONS(359),
    [sym_key] = ACTIONS(359),
    [sym_macros] = ACTIONS(359),
    [sym_meter] = ACTIONS(359),
    [sym_notes] = ACTIONS(359),
    [anon_sym_X_COLON] = ACTIONS(359),
    [sym_remark] = ACTIONS(359),
    [sym_rhythm_line] = ACTIONS(359),
    [anon_sym_s_COLON] = ACTIONS(359),
    [sym_tempo] = ACTIONS(359),
    [sym_tune_title] = ACTIONS(359),
    [sym_unit_note_length] = ACTIONS(359),
    [anon_sym_U_COLON] = ACTIONS(359),
    [sym_voice] = ACTIONS(359),
    [sym_words_tag] = ACTIONS(359),
    [sym_words_postbody] = ACTIONS(359),
  },
  [38] = {
    [ts_builtin_sym_end] = ACTIONS(297),
    [sym__NL] = ACTIONS(299),
    [anon_sym_BSLASH] = ACTIONS(297),
    [sym_BEAM_SEPARATOR] = ACTIONS(297),
    [anon_sym_LBRACK] = ACTIONS(299),
    [sym_COMMENT] = ACTIONS(299),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(297),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(299),
    [sym_slur_open] = ACTIONS(299),
    [sym_slur_close] = ACTIONS(297),
    [sym_alteration] = ACTIONS(297),
    [sym_note_letter] = ACTIONS(297),
    [sym_rest] = ACTIONS(297),
    [anon_sym_Z] = ACTIONS(297),
    [anon_sym_X] = ACTIONS(299),
    [aux_sym_grace_note_token1] = ACTIONS(297),
    [aux_sym_chord_symbol_token1] = ACTIONS(299),
    [sym_annotation] = ACTIONS(297),
    [sym_tuplet_marker] = ACTIONS(297),
    [sym_decoration_shorthand] = ACTIONS(299),
    [sym_bar_line] = ACTIONS(299),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(297),
    [sym_thin_double_bar_line] = ACTIONS(297),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(297),
    [sym_start_of_repeated_section] = ACTIONS(297),
    [sym_end_of_repeated_section] = ACTIONS(297),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(297),
    [anon_sym_BANGtrill_BANG] = ACTIONS(297),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(297),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(297),
    [anon_sym_BANGmordent_BANG] = ACTIONS(297),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(297),
    [anon_sym_BANGroll_BANG] = ACTIONS(297),
    [anon_sym_BANGturn_BANG] = ACTIONS(297),
    [anon_sym_BANGturnx_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(297),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_BANG] = ACTIONS(297),
    [anon_sym_BANGaccent_BANG] = ACTIONS(297),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(297),
    [anon_sym_BANGfermata_BANG] = ACTIONS(297),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(297),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(297),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(297),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(297),
    [anon_sym_BANGplus_BANG] = ACTIONS(297),
    [anon_sym_BANGsnap_BANG] = ACTIONS(297),
    [anon_sym_BANGslide_BANG] = ACTIONS(297),
    [anon_sym_BANGwedge_BANG] = ACTIONS(297),
    [anon_sym_BANGupbow_BANG] = ACTIONS(297),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(297),
    [anon_sym_BANGopen_BANG] = ACTIONS(297),
    [anon_sym_BANGthumb_BANG] = ACTIONS(297),
    [anon_sym_BANGbreath_BANG] = ACTIONS(297),
    [anon_sym_BANGpppp_BANG] = ACTIONS(297),
    [anon_sym_BANGppp_BANG] = ACTIONS(297),
    [anon_sym_BANGpp_BANG] = ACTIONS(297),
    [anon_sym_BANGp_BANG] = ACTIONS(297),
    [anon_sym_BANGmp_BANG] = ACTIONS(297),
    [anon_sym_BANGmf_BANG] = ACTIONS(297),
    [anon_sym_BANGf_BANG] = ACTIONS(297),
    [anon_sym_BANGff_BANG] = ACTIONS(297),
    [anon_sym_BANGfff_BANG] = ACTIONS(297),
    [anon_sym_BANGffff_BANG] = ACTIONS(297),
    [anon_sym_BANGsfz_BANG] = ACTIONS(297),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(297),
    [anon_sym_BANGsegno_BANG] = ACTIONS(297),
    [anon_sym_BANGcoda_BANG] = ACTIONS(297),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(297),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(297),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(297),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(297),
    [anon_sym_BANGfine_BANG] = ACTIONS(297),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(297),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(297),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(297),
    [sym_parts_line] = ACTIONS(297),
    [sym_instruction] = ACTIONS(297),
    [sym_key] = ACTIONS(297),
    [sym_macros] = ACTIONS(297),
    [sym_meter] = ACTIONS(297),
    [sym_notes] = ACTIONS(297),
    [anon_sym_X_COLON] = ACTIONS(297),
    [sym_remark] = ACTIONS(297),
    [sym_rhythm_line] = ACTIONS(297),
    [anon_sym_s_COLON] = ACTIONS(297),
    [sym_tempo] = ACTIONS(297),
    [sym_tune_title] = ACTIONS(297),
    [sym_unit_note_length] = ACTIONS(297),
    [anon_sym_U_COLON] = ACTIONS(297),
    [sym_voice] = ACTIONS(297),
    [sym_words_tag] = ACTIONS(297),
    [sym_words_postbody] = ACTIONS(297),
  },
  [39] = {
    [ts_builtin_sym_end] = ACTIONS(188),
    [sym__NL] = ACTIONS(190),
    [anon_sym_BSLASH] = ACTIONS(188),
    [sym_BEAM_SEPARATOR] = ACTIONS(188),
    [anon_sym_LBRACK] = ACTIONS(190),
    [sym_COMMENT] = ACTIONS(190),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(188),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(190),
    [sym_slur_open] = ACTIONS(190),
    [sym_slur_close] = ACTIONS(188),
    [sym_alteration] = ACTIONS(188),
    [sym_note_letter] = ACTIONS(188),
    [sym_rest] = ACTIONS(188),
    [anon_sym_Z] = ACTIONS(188),
    [anon_sym_X] = ACTIONS(190),
    [aux_sym_grace_note_token1] = ACTIONS(188),
    [aux_sym_chord_symbol_token1] = ACTIONS(190),
    [sym_annotation] = ACTIONS(188),
    [sym_tuplet_marker] = ACTIONS(188),
    [sym_decoration_shorthand] = ACTIONS(190),
    [sym_bar_line] = ACTIONS(190),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(188),
    [sym_thin_double_bar_line] = ACTIONS(188),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(188),
    [sym_start_of_repeated_section] = ACTIONS(188),
    [sym_end_of_repeated_section] = ACTIONS(188),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(188),
    [anon_sym_BANGtrill_BANG] = ACTIONS(188),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(188),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(188),
    [anon_sym_BANGmordent_BANG] = ACTIONS(188),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(188),
    [anon_sym_BANGroll_BANG] = ACTIONS(188),
    [anon_sym_BANGturn_BANG] = ACTIONS(188),
    [anon_sym_BANGturnx_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(188),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_BANG] = ACTIONS(188),
    [anon_sym_BANGaccent_BANG] = ACTIONS(188),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(188),
    [anon_sym_BANGfermata_BANG] = ACTIONS(188),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(188),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(188),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(188),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(188),
    [anon_sym_BANGplus_BANG] = ACTIONS(188),
    [anon_sym_BANGsnap_BANG] = ACTIONS(188),
    [anon_sym_BANGslide_BANG] = ACTIONS(188),
    [anon_sym_BANGwedge_BANG] = ACTIONS(188),
    [anon_sym_BANGupbow_BANG] = ACTIONS(188),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(188),
    [anon_sym_BANGopen_BANG] = ACTIONS(188),
    [anon_sym_BANGthumb_BANG] = ACTIONS(188),
    [anon_sym_BANGbreath_BANG] = ACTIONS(188),
    [anon_sym_BANGpppp_BANG] = ACTIONS(188),
    [anon_sym_BANGppp_BANG] = ACTIONS(188),
    [anon_sym_BANGpp_BANG] = ACTIONS(188),
    [anon_sym_BANGp_BANG] = ACTIONS(188),
    [anon_sym_BANGmp_BANG] = ACTIONS(188),
    [anon_sym_BANGmf_BANG] = ACTIONS(188),
    [anon_sym_BANGf_BANG] = ACTIONS(188),
    [anon_sym_BANGff_BANG] = ACTIONS(188),
    [anon_sym_BANGfff_BANG] = ACTIONS(188),
    [anon_sym_BANGffff_BANG] = ACTIONS(188),
    [anon_sym_BANGsfz_BANG] = ACTIONS(188),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(188),
    [anon_sym_BANGsegno_BANG] = ACTIONS(188),
    [anon_sym_BANGcoda_BANG] = ACTIONS(188),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(188),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(188),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(188),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(188),
    [anon_sym_BANGfine_BANG] = ACTIONS(188),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(188),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(188),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(188),
    [sym_parts_line] = ACTIONS(188),
    [sym_instruction] = ACTIONS(188),
    [sym_key] = ACTIONS(188),
    [sym_macros] = ACTIONS(188),
    [sym_meter] = ACTIONS(188),
    [sym_notes] = ACTIONS(188),
    [anon_sym_X_COLON] = ACTIONS(188),
    [sym_remark] = ACTIONS(188),
    [sym_rhythm_line] = ACTIONS(188),
    [anon_sym_s_COLON] = ACTIONS(188),
    [sym_tempo] = ACTIONS(188),
    [sym_tune_title] = ACTIONS(188),
    [sym_unit_note_length] = ACTIONS(188),
    [anon_sym_U_COLON] = ACTIONS(188),
    [sym_voice] = ACTIONS(188),
    [sym_words_tag] = ACTIONS(188),
    [sym_words_postbody] = ACTIONS(188),
  },
  [40] = {
    [ts_builtin_sym_end] = ACTIONS(359),
    [sym__NL] = ACTIONS(361),
    [anon_sym_BSLASH] = ACTIONS(359),
    [sym_BEAM_SEPARATOR] = ACTIONS(359),
    [anon_sym_LBRACK] = ACTIONS(361),
    [sym_COMMENT] = ACTIONS(361),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(359),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(361),
    [sym_slur_open] = ACTIONS(361),
    [sym_slur_close] = ACTIONS(359),
    [sym_alteration] = ACTIONS(359),
    [sym_note_letter] = ACTIONS(359),
    [sym_rest] = ACTIONS(359),
    [anon_sym_Z] = ACTIONS(359),
    [anon_sym_X] = ACTIONS(361),
    [aux_sym_grace_note_token1] = ACTIONS(359),
    [aux_sym_chord_symbol_token1] = ACTIONS(361),
    [sym_annotation] = ACTIONS(359),
    [sym_tuplet_marker] = ACTIONS(359),
    [sym_decoration_shorthand] = ACTIONS(361),
    [sym_bar_line] = ACTIONS(361),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(359),
    [sym_thin_double_bar_line] = ACTIONS(359),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(359),
    [sym_start_of_repeated_section] = ACTIONS(359),
    [sym_end_of_repeated_section] = ACTIONS(359),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(359),
    [anon_sym_BANGtrill_BANG] = ACTIONS(359),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(359),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(359),
    [anon_sym_BANGmordent_BANG] = ACTIONS(359),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(359),
    [anon_sym_BANGroll_BANG] = ACTIONS(359),
    [anon_sym_BANGturn_BANG] = ACTIONS(359),
    [anon_sym_BANGturnx_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(359),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_BANG] = ACTIONS(359),
    [anon_sym_BANGaccent_BANG] = ACTIONS(359),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(359),
    [anon_sym_BANGfermata_BANG] = ACTIONS(359),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(359),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(359),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(359),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(359),
    [anon_sym_BANGplus_BANG] = ACTIONS(359),
    [anon_sym_BANGsnap_BANG] = ACTIONS(359),
    [anon_sym_BANGslide_BANG] = ACTIONS(359),
    [anon_sym_BANGwedge_BANG] = ACTIONS(359),
    [anon_sym_BANGupbow_BANG] = ACTIONS(359),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(359),
    [anon_sym_BANGopen_BANG] = ACTIONS(359),
    [anon_sym_BANGthumb_BANG] = ACTIONS(359),
    [anon_sym_BANGbreath_BANG] = ACTIONS(359),
    [anon_sym_BANGpppp_BANG] = ACTIONS(359),
    [anon_sym_BANGppp_BANG] = ACTIONS(359),
    [anon_sym_BANGpp_BANG] = ACTIONS(359),
    [anon_sym_BANGp_BANG] = ACTIONS(359),
    [anon_sym_BANGmp_BANG] = ACTIONS(359),
    [anon_sym_BANGmf_BANG] = ACTIONS(359),
    [anon_sym_BANGf_BANG] = ACTIONS(359),
    [anon_sym_BANGff_BANG] = ACTIONS(359),
    [anon_sym_BANGfff_BANG] = ACTIONS(359),
    [anon_sym_BANGffff_BANG] = ACTIONS(359),
    [anon_sym_BANGsfz_BANG] = ACTIONS(359),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(359),
    [anon_sym_BANGsegno_BANG] = ACTIONS(359),
    [anon_sym_BANGcoda_BANG] = ACTIONS(359),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(359),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(359),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(359),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(359),
    [anon_sym_BANGfine_BANG] = ACTIONS(359),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(359),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(359),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(359),
    [sym_parts_line] = ACTIONS(359),
    [sym_instruction] = ACTIONS(359),
    [sym_key] = ACTIONS(359),
    [sym_macros] = ACTIONS(359),
    [sym_meter] = ACTIONS(359),
    [sym_notes] = ACTIONS(359),
    [anon_sym_X_COLON] = ACTIONS(359),
    [sym_remark] = ACTIONS(359),
    [sym_rhythm_line] = ACTIONS(359),
    [anon_sym_s_COLON] = ACTIONS(359),
    [sym_tempo] = ACTIONS(359),
    [sym_tune_title] = ACTIONS(359),
    [sym_unit_note_length] = ACTIONS(359),
    [anon_sym_U_COLON] = ACTIONS(359),
    [sym_voice] = ACTIONS(359),
    [sym_words_tag] = ACTIONS(359),
    [sym_words_postbody] = ACTIONS(359),
  },
  [41] = {
    [ts_builtin_sym_end] = ACTIONS(309),
    [sym__NL] = ACTIONS(311),
    [anon_sym_BSLASH] = ACTIONS(309),
    [sym_BEAM_SEPARATOR] = ACTIONS(309),
    [anon_sym_LBRACK] = ACTIONS(311),
    [sym_COMMENT] = ACTIONS(311),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(309),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(311),
    [sym_slur_open] = ACTIONS(311),
    [sym_slur_close] = ACTIONS(309),
    [sym_alteration] = ACTIONS(309),
    [sym_note_letter] = ACTIONS(309),
    [sym_rest] = ACTIONS(309),
    [anon_sym_Z] = ACTIONS(309),
    [anon_sym_X] = ACTIONS(311),
    [aux_sym_grace_note_token1] = ACTIONS(309),
    [aux_sym_chord_symbol_token1] = ACTIONS(311),
    [sym_annotation] = ACTIONS(309),
    [sym_tuplet_marker] = ACTIONS(309),
    [sym_decoration_shorthand] = ACTIONS(311),
    [sym_bar_line] = ACTIONS(311),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(309),
    [sym_thin_double_bar_line] = ACTIONS(309),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(309),
    [sym_start_of_repeated_section] = ACTIONS(309),
    [sym_end_of_repeated_section] = ACTIONS(309),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(309),
    [anon_sym_BANGtrill_BANG] = ACTIONS(309),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(309),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(309),
    [anon_sym_BANGmordent_BANG] = ACTIONS(309),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(309),
    [anon_sym_BANGroll_BANG] = ACTIONS(309),
    [anon_sym_BANGturn_BANG] = ACTIONS(309),
    [anon_sym_BANGturnx_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(309),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_BANG] = ACTIONS(309),
    [anon_sym_BANGaccent_BANG] = ACTIONS(309),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(309),
    [anon_sym_BANGfermata_BANG] = ACTIONS(309),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(309),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(309),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(309),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(309),
    [anon_sym_BANGplus_BANG] = ACTIONS(309),
    [anon_sym_BANGsnap_BANG] = ACTIONS(309),
    [anon_sym_BANGslide_BANG] = ACTIONS(309),
    [anon_sym_BANGwedge_BANG] = ACTIONS(309),
    [anon_sym_BANGupbow_BANG] = ACTIONS(309),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(309),
    [anon_sym_BANGopen_BANG] = ACTIONS(309),
    [anon_sym_BANGthumb_BANG] = ACTIONS(309),
    [anon_sym_BANGbreath_BANG] = ACTIONS(309),
    [anon_sym_BANGpppp_BANG] = ACTIONS(309),
    [anon_sym_BANGppp_BANG] = ACTIONS(309),
    [anon_sym_BANGpp_BANG] = ACTIONS(309),
    [anon_sym_BANGp_BANG] = ACTIONS(309),
    [anon_sym_BANGmp_BANG] = ACTIONS(309),
    [anon_sym_BANGmf_BANG] = ACTIONS(309),
    [anon_sym_BANGf_BANG] = ACTIONS(309),
    [anon_sym_BANGff_BANG] = ACTIONS(309),
    [anon_sym_BANGfff_BANG] = ACTIONS(309),
    [anon_sym_BANGffff_BANG] = ACTIONS(309),
    [anon_sym_BANGsfz_BANG] = ACTIONS(309),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(309),
    [anon_sym_BANGsegno_BANG] = ACTIONS(309),
    [anon_sym_BANGcoda_BANG] = ACTIONS(309),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(309),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(309),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(309),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(309),
    [anon_sym_BANGfine_BANG] = ACTIONS(309),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(309),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(309),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(309),
    [sym_parts_line] = ACTIONS(309),
    [sym_instruction] = ACTIONS(309),
    [sym_key] = ACTIONS(309),
    [sym_macros] = ACTIONS(309),
    [sym_meter] = ACTIONS(309),
    [sym_notes] = ACTIONS(309),
    [anon_sym_X_COLON] = ACTIONS(309),
    [sym_remark] = ACTIONS(309),
    [sym_rhythm_line] = ACTIONS(309),
    [anon_sym_s_COLON] = ACTIONS(309),
    [sym_tempo] = ACTIONS(309),
    [sym_tune_title] = ACTIONS(309),
    [sym_unit_note_length] = ACTIONS(309),
    [anon_sym_U_COLON] = ACTIONS(309),
    [sym_voice] = ACTIONS(309),
    [sym_words_tag] = ACTIONS(309),
    [sym_words_postbody] = ACTIONS(309),
  },
  [42] = {
    [ts_builtin_sym_end] = ACTIONS(365),
    [sym__NL] = ACTIONS(367),
    [anon_sym_BSLASH] = ACTIONS(365),
    [sym_BEAM_SEPARATOR] = ACTIONS(365),
    [anon_sym_LBRACK] = ACTIONS(367),
    [sym_COMMENT] = ACTIONS(367),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(365),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(367),
    [sym_slur_open] = ACTIONS(367),
    [sym_slur_close] = ACTIONS(365),
    [sym_alteration] = ACTIONS(365),
    [sym_note_letter] = ACTIONS(365),
    [sym_rest] = ACTIONS(365),
    [anon_sym_Z] = ACTIONS(365),
    [anon_sym_X] = ACTIONS(367),
    [aux_sym_grace_note_token1] = ACTIONS(365),
    [aux_sym_chord_symbol_token1] = ACTIONS(367),
    [sym_annotation] = ACTIONS(365),
    [sym_tuplet_marker] = ACTIONS(365),
    [sym_decoration_shorthand] = ACTIONS(367),
    [sym_bar_line] = ACTIONS(367),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(365),
    [sym_thin_double_bar_line] = ACTIONS(365),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(365),
    [sym_start_of_repeated_section] = ACTIONS(365),
    [sym_end_of_repeated_section] = ACTIONS(365),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(365),
    [anon_sym_BANGtrill_BANG] = ACTIONS(365),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(365),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(365),
    [anon_sym_BANGmordent_BANG] = ACTIONS(365),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(365),
    [anon_sym_BANGroll_BANG] = ACTIONS(365),
    [anon_sym_BANGturn_BANG] = ACTIONS(365),
    [anon_sym_BANGturnx_BANG] = ACTIONS(365),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(365),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(365),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(365),
    [anon_sym_BANG_GT_BANG] = ACTIONS(365),
    [anon_sym_BANGaccent_BANG] = ACTIONS(365),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(365),
    [anon_sym_BANGfermata_BANG] = ACTIONS(365),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(365),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(365),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(365),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(365),
    [anon_sym_BANGplus_BANG] = ACTIONS(365),
    [anon_sym_BANGsnap_BANG] = ACTIONS(365),
    [anon_sym_BANGslide_BANG] = ACTIONS(365),
    [anon_sym_BANGwedge_BANG] = ACTIONS(365),
    [anon_sym_BANGupbow_BANG] = ACTIONS(365),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(365),
    [anon_sym_BANGopen_BANG] = ACTIONS(365),
    [anon_sym_BANGthumb_BANG] = ACTIONS(365),
    [anon_sym_BANGbreath_BANG] = ACTIONS(365),
    [anon_sym_BANGpppp_BANG] = ACTIONS(365),
    [anon_sym_BANGppp_BANG] = ACTIONS(365),
    [anon_sym_BANGpp_BANG] = ACTIONS(365),
    [anon_sym_BANGp_BANG] = ACTIONS(365),
    [anon_sym_BANGmp_BANG] = ACTIONS(365),
    [anon_sym_BANGmf_BANG] = ACTIONS(365),
    [anon_sym_BANGf_BANG] = ACTIONS(365),
    [anon_sym_BANGff_BANG] = ACTIONS(365),
    [anon_sym_BANGfff_BANG] = ACTIONS(365),
    [anon_sym_BANGffff_BANG] = ACTIONS(365),
    [anon_sym_BANGsfz_BANG] = ACTIONS(365),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(365),
    [anon_sym_BANGsegno_BANG] = ACTIONS(365),
    [anon_sym_BANGcoda_BANG] = ACTIONS(365),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(365),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(365),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(365),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(365),
    [anon_sym_BANGfine_BANG] = ACTIONS(365),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(365),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(365),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(365),
    [sym_parts_line] = ACTIONS(365),
    [sym_instruction] = ACTIONS(365),
    [sym_key] = ACTIONS(365),
    [sym_macros] = ACTIONS(365),
    [sym_meter] = ACTIONS(365),
    [sym_notes] = ACTIONS(365),
    [anon_sym_X_COLON] = ACTIONS(365),
    [sym_remark] = ACTIONS(365),
    [sym_rhythm_line] = ACTIONS(365),
    [anon_sym_s_COLON] = ACTIONS(365),
    [sym_tempo] = ACTIONS(365),
    [sym_tune_title] = ACTIONS(365),
    [sym_unit_note_length] = ACTIONS(365),
    [anon_sym_U_COLON] = ACTIONS(365),
    [sym_voice] = ACTIONS(365),
    [sym_words_tag] = ACTIONS(365),
    [sym_words_postbody] = ACTIONS(365),
  },
  [43] = {
    [ts_builtin_sym_end] = ACTIONS(335),
    [sym__NL] = ACTIONS(337),
    [anon_sym_BSLASH] = ACTIONS(335),
    [sym_BEAM_SEPARATOR] = ACTIONS(335),
    [anon_sym_LBRACK] = ACTIONS(337),
    [sym_COMMENT] = ACTIONS(337),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(335),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(337),
    [sym_slur_open] = ACTIONS(337),
    [sym_slur_close] = ACTIONS(335),
    [sym_alteration] = ACTIONS(335),
    [sym_note_letter] = ACTIONS(335),
    [sym_rest] = ACTIONS(335),
    [anon_sym_Z] = ACTIONS(335),
    [anon_sym_X] = ACTIONS(337),
    [aux_sym_grace_note_token1] = ACTIONS(335),
    [aux_sym_chord_symbol_token1] = ACTIONS(337),
    [sym_annotation] = ACTIONS(335),
    [sym_tuplet_marker] = ACTIONS(335),
    [sym_decoration_shorthand] = ACTIONS(337),
    [sym_bar_line] = ACTIONS(337),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(335),
    [sym_thin_double_bar_line] = ACTIONS(335),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(335),
    [sym_start_of_repeated_section] = ACTIONS(335),
    [sym_end_of_repeated_section] = ACTIONS(335),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(335),
    [anon_sym_BANGtrill_BANG] = ACTIONS(335),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(335),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(335),
    [anon_sym_BANGmordent_BANG] = ACTIONS(335),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(335),
    [anon_sym_BANGroll_BANG] = ACTIONS(335),
    [anon_sym_BANGturn_BANG] = ACTIONS(335),
    [anon_sym_BANGturnx_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(335),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_BANG] = ACTIONS(335),
    [anon_sym_BANGaccent_BANG] = ACTIONS(335),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(335),
    [anon_sym_BANGfermata_BANG] = ACTIONS(335),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(335),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(335),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(335),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(335),
    [anon_sym_BANGplus_BANG] = ACTIONS(335),
    [anon_sym_BANGsnap_BANG] = ACTIONS(335),
    [anon_sym_BANGslide_BANG] = ACTIONS(335),
    [anon_sym_BANGwedge_BANG] = ACTIONS(335),
    [anon_sym_BANGupbow_BANG] = ACTIONS(335),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(335),
    [anon_sym_BANGopen_BANG] = ACTIONS(335),
    [anon_sym_BANGthumb_BANG] = ACTIONS(335),
    [anon_sym_BANGbreath_BANG] = ACTIONS(335),
    [anon_sym_BANGpppp_BANG] = ACTIONS(335),
    [anon_sym_BANGppp_BANG] = ACTIONS(335),
    [anon_sym_BANGpp_BANG] = ACTIONS(335),
    [anon_sym_BANGp_BANG] = ACTIONS(335),
    [anon_sym_BANGmp_BANG] = ACTIONS(335),
    [anon_sym_BANGmf_BANG] = ACTIONS(335),
    [anon_sym_BANGf_BANG] = ACTIONS(335),
    [anon_sym_BANGff_BANG] = ACTIONS(335),
    [anon_sym_BANGfff_BANG] = ACTIONS(335),
    [anon_sym_BANGffff_BANG] = ACTIONS(335),
    [anon_sym_BANGsfz_BANG] = ACTIONS(335),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(335),
    [anon_sym_BANGsegno_BANG] = ACTIONS(335),
    [anon_sym_BANGcoda_BANG] = ACTIONS(335),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(335),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(335),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(335),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(335),
    [anon_sym_BANGfine_BANG] = ACTIONS(335),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(335),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(335),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(335),
    [sym_parts_line] = ACTIONS(335),
    [sym_instruction] = ACTIONS(335),
    [sym_key] = ACTIONS(335),
    [sym_macros] = ACTIONS(335),
    [sym_meter] = ACTIONS(335),
    [sym_notes] = ACTIONS(335),
    [anon_sym_X_COLON] = ACTIONS(335),
    [sym_remark] = ACTIONS(335),
    [sym_rhythm_line] = ACTIONS(335),
    [anon_sym_s_COLON] = ACTIONS(335),
    [sym_tempo] = ACTIONS(335),
    [sym_tune_title] = ACTIONS(335),
    [sym_unit_note_length] = ACTIONS(335),
    [anon_sym_U_COLON] = ACTIONS(335),
    [sym_voice] = ACTIONS(335),
    [sym_words_tag] = ACTIONS(335),
    [sym_words_postbody] = ACTIONS(335),
  },
  [44] = {
    [ts_builtin_sym_end] = ACTIONS(353),
    [sym__NL] = ACTIONS(355),
    [anon_sym_BSLASH] = ACTIONS(353),
    [sym_BEAM_SEPARATOR] = ACTIONS(353),
    [anon_sym_LBRACK] = ACTIONS(355),
    [sym_COMMENT] = ACTIONS(355),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(353),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(355),
    [sym_slur_open] = ACTIONS(355),
    [sym_slur_close] = ACTIONS(353),
    [sym_alteration] = ACTIONS(353),
    [sym_note_letter] = ACTIONS(353),
    [sym_rest] = ACTIONS(353),
    [anon_sym_Z] = ACTIONS(353),
    [anon_sym_X] = ACTIONS(355),
    [aux_sym_grace_note_token1] = ACTIONS(353),
    [aux_sym_chord_symbol_token1] = ACTIONS(355),
    [sym_annotation] = ACTIONS(353),
    [sym_tuplet_marker] = ACTIONS(353),
    [sym_decoration_shorthand] = ACTIONS(355),
    [sym_bar_line] = ACTIONS(355),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(353),
    [sym_thin_double_bar_line] = ACTIONS(353),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(353),
    [sym_start_of_repeated_section] = ACTIONS(353),
    [sym_end_of_repeated_section] = ACTIONS(353),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(353),
    [anon_sym_BANGtrill_BANG] = ACTIONS(353),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(353),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(353),
    [anon_sym_BANGmordent_BANG] = ACTIONS(353),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(353),
    [anon_sym_BANGroll_BANG] = ACTIONS(353),
    [anon_sym_BANGturn_BANG] = ACTIONS(353),
    [anon_sym_BANGturnx_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(353),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_BANG] = ACTIONS(353),
    [anon_sym_BANGaccent_BANG] = ACTIONS(353),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(353),
    [anon_sym_BANGfermata_BANG] = ACTIONS(353),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(353),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(353),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(353),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(353),
    [anon_sym_BANGplus_BANG] = ACTIONS(353),
    [anon_sym_BANGsnap_BANG] = ACTIONS(353),
    [anon_sym_BANGslide_BANG] = ACTIONS(353),
    [anon_sym_BANGwedge_BANG] = ACTIONS(353),
    [anon_sym_BANGupbow_BANG] = ACTIONS(353),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(353),
    [anon_sym_BANGopen_BANG] = ACTIONS(353),
    [anon_sym_BANGthumb_BANG] = ACTIONS(353),
    [anon_sym_BANGbreath_BANG] = ACTIONS(353),
    [anon_sym_BANGpppp_BANG] = ACTIONS(353),
    [anon_sym_BANGppp_BANG] = ACTIONS(353),
    [anon_sym_BANGpp_BANG] = ACTIONS(353),
    [anon_sym_BANGp_BANG] = ACTIONS(353),
    [anon_sym_BANGmp_BANG] = ACTIONS(353),
    [anon_sym_BANGmf_BANG] = ACTIONS(353),
    [anon_sym_BANGf_BANG] = ACTIONS(353),
    [anon_sym_BANGff_BANG] = ACTIONS(353),
    [anon_sym_BANGfff_BANG] = ACTIONS(353),
    [anon_sym_BANGffff_BANG] = ACTIONS(353),
    [anon_sym_BANGsfz_BANG] = ACTIONS(353),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(353),
    [anon_sym_BANGsegno_BANG] = ACTIONS(353),
    [anon_sym_BANGcoda_BANG] = ACTIONS(353),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(353),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(353),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(353),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(353),
    [anon_sym_BANGfine_BANG] = ACTIONS(353),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(353),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(353),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(353),
    [sym_parts_line] = ACTIONS(353),
    [sym_instruction] = ACTIONS(353),
    [sym_key] = ACTIONS(353),
    [sym_macros] = ACTIONS(353),
    [sym_meter] = ACTIONS(353),
    [sym_notes] = ACTIONS(353),
    [anon_sym_X_COLON] = ACTIONS(353),
    [sym_remark] = ACTIONS(353),
    [sym_rhythm_line] = ACTIONS(353),
    [anon_sym_s_COLON] = ACTIONS(353),
    [sym_tempo] = ACTIONS(353),
    [sym_tune_title] = ACTIONS(353),
    [sym_unit_note_length] = ACTIONS(353),
    [anon_sym_U_COLON] = ACTIONS(353),
    [sym_voice] = ACTIONS(353),
    [sym_words_tag] = ACTIONS(353),
    [sym_words_postbody] = ACTIONS(353),
  },
  [45] = {
    [ts_builtin_sym_end] = ACTIONS(303),
    [sym__NL] = ACTIONS(305),
    [anon_sym_BSLASH] = ACTIONS(303),
    [sym_BEAM_SEPARATOR] = ACTIONS(303),
    [anon_sym_LBRACK] = ACTIONS(305),
    [sym_COMMENT] = ACTIONS(305),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(303),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(305),
    [sym_slur_open] = ACTIONS(305),
    [sym_slur_close] = ACTIONS(303),
    [sym_alteration] = ACTIONS(303),
    [sym_note_letter] = ACTIONS(303),
    [sym_rest] = ACTIONS(303),
    [anon_sym_Z] = ACTIONS(303),
    [anon_sym_X] = ACTIONS(305),
    [aux_sym_grace_note_token1] = ACTIONS(303),
    [aux_sym_chord_symbol_token1] = ACTIONS(305),
    [sym_annotation] = ACTIONS(303),
    [sym_tuplet_marker] = ACTIONS(303),
    [sym_decoration_shorthand] = ACTIONS(305),
    [sym_bar_line] = ACTIONS(305),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(303),
    [sym_thin_double_bar_line] = ACTIONS(303),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(303),
    [sym_start_of_repeated_section] = ACTIONS(303),
    [sym_end_of_repeated_section] = ACTIONS(303),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(303),
    [anon_sym_BANGtrill_BANG] = ACTIONS(303),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(303),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(303),
    [anon_sym_BANGmordent_BANG] = ACTIONS(303),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(303),
    [anon_sym_BANGroll_BANG] = ACTIONS(303),
    [anon_sym_BANGturn_BANG] = ACTIONS(303),
    [anon_sym_BANGturnx_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(303),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_BANG] = ACTIONS(303),
    [anon_sym_BANGaccent_BANG] = ACTIONS(303),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(303),
    [anon_sym_BANGfermata_BANG] = ACTIONS(303),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(303),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(303),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(303),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(303),
    [anon_sym_BANGplus_BANG] = ACTIONS(303),
    [anon_sym_BANGsnap_BANG] = ACTIONS(303),
    [anon_sym_BANGslide_BANG] = ACTIONS(303),
    [anon_sym_BANGwedge_BANG] = ACTIONS(303),
    [anon_sym_BANGupbow_BANG] = ACTIONS(303),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(303),
    [anon_sym_BANGopen_BANG] = ACTIONS(303),
    [anon_sym_BANGthumb_BANG] = ACTIONS(303),
    [anon_sym_BANGbreath_BANG] = ACTIONS(303),
    [anon_sym_BANGpppp_BANG] = ACTIONS(303),
    [anon_sym_BANGppp_BANG] = ACTIONS(303),
    [anon_sym_BANGpp_BANG] = ACTIONS(303),
    [anon_sym_BANGp_BANG] = ACTIONS(303),
    [anon_sym_BANGmp_BANG] = ACTIONS(303),
    [anon_sym_BANGmf_BANG] = ACTIONS(303),
    [anon_sym_BANGf_BANG] = ACTIONS(303),
    [anon_sym_BANGff_BANG] = ACTIONS(303),
    [anon_sym_BANGfff_BANG] = ACTIONS(303),
    [anon_sym_BANGffff_BANG] = ACTIONS(303),
    [anon_sym_BANGsfz_BANG] = ACTIONS(303),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(303),
    [anon_sym_BANGsegno_BANG] = ACTIONS(303),
    [anon_sym_BANGcoda_BANG] = ACTIONS(303),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(303),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(303),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(303),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(303),
    [anon_sym_BANGfine_BANG] = ACTIONS(303),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(303),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(303),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(303),
    [sym_parts_line] = ACTIONS(303),
    [sym_instruction] = ACTIONS(303),
    [sym_key] = ACTIONS(303),
    [sym_macros] = ACTIONS(303),
    [sym_meter] = ACTIONS(303),
    [sym_notes] = ACTIONS(303),
    [anon_sym_X_COLON] = ACTIONS(303),
    [sym_remark] = ACTIONS(303),
    [sym_rhythm_line] = ACTIONS(303),
    [anon_sym_s_COLON] = ACTIONS(303),
    [sym_tempo] = ACTIONS(303),
    [sym_tune_title] = ACTIONS(303),
    [sym_unit_note_length] = ACTIONS(303),
    [anon_sym_U_COLON] = ACTIONS(303),
    [sym_voice] = ACTIONS(303),
    [sym_words_tag] = ACTIONS(303),
    [sym_words_postbody] = ACTIONS(303),
  },
  [46] = {
    [ts_builtin_sym_end] = ACTIONS(329),
    [sym__NL] = ACTIONS(331),
    [anon_sym_BSLASH] = ACTIONS(329),
    [sym_BEAM_SEPARATOR] = ACTIONS(329),
    [anon_sym_LBRACK] = ACTIONS(331),
    [sym_COMMENT] = ACTIONS(331),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(329),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(331),
    [sym_slur_open] = ACTIONS(331),
    [sym_slur_close] = ACTIONS(329),
    [sym_alteration] = ACTIONS(329),
    [sym_note_letter] = ACTIONS(329),
    [sym_rest] = ACTIONS(329),
    [anon_sym_Z] = ACTIONS(329),
    [anon_sym_X] = ACTIONS(331),
    [aux_sym_grace_note_token1] = ACTIONS(329),
    [aux_sym_chord_symbol_token1] = ACTIONS(331),
    [sym_annotation] = ACTIONS(329),
    [sym_tuplet_marker] = ACTIONS(329),
    [sym_decoration_shorthand] = ACTIONS(331),
    [sym_bar_line] = ACTIONS(331),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(329),
    [sym_thin_double_bar_line] = ACTIONS(329),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(329),
    [sym_start_of_repeated_section] = ACTIONS(329),
    [sym_end_of_repeated_section] = ACTIONS(329),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(329),
    [anon_sym_BANGtrill_BANG] = ACTIONS(329),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(329),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(329),
    [anon_sym_BANGmordent_BANG] = ACTIONS(329),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(329),
    [anon_sym_BANGroll_BANG] = ACTIONS(329),
    [anon_sym_BANGturn_BANG] = ACTIONS(329),
    [anon_sym_BANGturnx_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(329),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_BANG] = ACTIONS(329),
    [anon_sym_BANGaccent_BANG] = ACTIONS(329),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(329),
    [anon_sym_BANGfermata_BANG] = ACTIONS(329),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(329),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(329),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(329),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(329),
    [anon_sym_BANGplus_BANG] = ACTIONS(329),
    [anon_sym_BANGsnap_BANG] = ACTIONS(329),
    [anon_sym_BANGslide_BANG] = ACTIONS(329),
    [anon_sym_BANGwedge_BANG] = ACTIONS(329),
    [anon_sym_BANGupbow_BANG] = ACTIONS(329),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(329),
    [anon_sym_BANGopen_BANG] = ACTIONS(329),
    [anon_sym_BANGthumb_BANG] = ACTIONS(329),
    [anon_sym_BANGbreath_BANG] = ACTIONS(329),
    [anon_sym_BANGpppp_BANG] = ACTIONS(329),
    [anon_sym_BANGppp_BANG] = ACTIONS(329),
    [anon_sym_BANGpp_BANG] = ACTIONS(329),
    [anon_sym_BANGp_BANG] = ACTIONS(329),
    [anon_sym_BANGmp_BANG] = ACTIONS(329),
    [anon_sym_BANGmf_BANG] = ACTIONS(329),
    [anon_sym_BANGf_BANG] = ACTIONS(329),
    [anon_sym_BANGff_BANG] = ACTIONS(329),
    [anon_sym_BANGfff_BANG] = ACTIONS(329),
    [anon_sym_BANGffff_BANG] = ACTIONS(329),
    [anon_sym_BANGsfz_BANG] = ACTIONS(329),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(329),
    [anon_sym_BANGsegno_BANG] = ACTIONS(329),
    [anon_sym_BANGcoda_BANG] = ACTIONS(329),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(329),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(329),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(329),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(329),
    [anon_sym_BANGfine_BANG] = ACTIONS(329),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(329),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(329),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(329),
    [sym_parts_line] = ACTIONS(329),
    [sym_instruction] = ACTIONS(329),
    [sym_key] = ACTIONS(329),
    [sym_macros] = ACTIONS(329),
    [sym_meter] = ACTIONS(329),
    [sym_notes] = ACTIONS(329),
    [anon_sym_X_COLON] = ACTIONS(329),
    [sym_remark] = ACTIONS(329),
    [sym_rhythm_line] = ACTIONS(329),
    [anon_sym_s_COLON] = ACTIONS(329),
    [sym_tempo] = ACTIONS(329),
    [sym_tune_title] = ACTIONS(329),
    [sym_unit_note_length] = ACTIONS(329),
    [anon_sym_U_COLON] = ACTIONS(329),
    [sym_voice] = ACTIONS(329),
    [sym_words_tag] = ACTIONS(329),
    [sym_words_postbody] = ACTIONS(329),
  },
  [47] = {
    [ts_builtin_sym_end] = ACTIONS(347),
    [sym__NL] = ACTIONS(349),
    [anon_sym_BSLASH] = ACTIONS(347),
    [sym_BEAM_SEPARATOR] = ACTIONS(347),
    [anon_sym_LBRACK] = ACTIONS(349),
    [sym_COMMENT] = ACTIONS(349),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(347),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(349),
    [sym_slur_open] = ACTIONS(349),
    [sym_slur_close] = ACTIONS(347),
    [sym_alteration] = ACTIONS(347),
    [sym_note_letter] = ACTIONS(347),
    [sym_rest] = ACTIONS(347),
    [anon_sym_Z] = ACTIONS(347),
    [anon_sym_X] = ACTIONS(349),
    [aux_sym_grace_note_token1] = ACTIONS(347),
    [aux_sym_chord_symbol_token1] = ACTIONS(349),
    [sym_annotation] = ACTIONS(347),
    [sym_tuplet_marker] = ACTIONS(347),
    [sym_decoration_shorthand] = ACTIONS(349),
    [sym_bar_line] = ACTIONS(349),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(347),
    [sym_thin_double_bar_line] = ACTIONS(347),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(347),
    [sym_start_of_repeated_section] = ACTIONS(347),
    [sym_end_of_repeated_section] = ACTIONS(347),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(347),
    [anon_sym_BANGtrill_BANG] = ACTIONS(347),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(347),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(347),
    [anon_sym_BANGmordent_BANG] = ACTIONS(347),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(347),
    [anon_sym_BANGroll_BANG] = ACTIONS(347),
    [anon_sym_BANGturn_BANG] = ACTIONS(347),
    [anon_sym_BANGturnx_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(347),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_BANG] = ACTIONS(347),
    [anon_sym_BANGaccent_BANG] = ACTIONS(347),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(347),
    [anon_sym_BANGfermata_BANG] = ACTIONS(347),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(347),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(347),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(347),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(347),
    [anon_sym_BANGplus_BANG] = ACTIONS(347),
    [anon_sym_BANGsnap_BANG] = ACTIONS(347),
    [anon_sym_BANGslide_BANG] = ACTIONS(347),
    [anon_sym_BANGwedge_BANG] = ACTIONS(347),
    [anon_sym_BANGupbow_BANG] = ACTIONS(347),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(347),
    [anon_sym_BANGopen_BANG] = ACTIONS(347),
    [anon_sym_BANGthumb_BANG] = ACTIONS(347),
    [anon_sym_BANGbreath_BANG] = ACTIONS(347),
    [anon_sym_BANGpppp_BANG] = ACTIONS(347),
    [anon_sym_BANGppp_BANG] = ACTIONS(347),
    [anon_sym_BANGpp_BANG] = ACTIONS(347),
    [anon_sym_BANGp_BANG] = ACTIONS(347),
    [anon_sym_BANGmp_BANG] = ACTIONS(347),
    [anon_sym_BANGmf_BANG] = ACTIONS(347),
    [anon_sym_BANGf_BANG] = ACTIONS(347),
    [anon_sym_BANGff_BANG] = ACTIONS(347),
    [anon_sym_BANGfff_BANG] = ACTIONS(347),
    [anon_sym_BANGffff_BANG] = ACTIONS(347),
    [anon_sym_BANGsfz_BANG] = ACTIONS(347),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(347),
    [anon_sym_BANGsegno_BANG] = ACTIONS(347),
    [anon_sym_BANGcoda_BANG] = ACTIONS(347),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(347),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(347),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(347),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(347),
    [anon_sym_BANGfine_BANG] = ACTIONS(347),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(347),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(347),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(347),
    [sym_parts_line] = ACTIONS(347),
    [sym_instruction] = ACTIONS(347),
    [sym_key] = ACTIONS(347),
    [sym_macros] = ACTIONS(347),
    [sym_meter] = ACTIONS(347),
    [sym_notes] = ACTIONS(347),
    [anon_sym_X_COLON] = ACTIONS(347),
    [sym_remark] = ACTIONS(347),
    [sym_rhythm_line] = ACTIONS(347),
    [anon_sym_s_COLON] = ACTIONS(347),
    [sym_tempo] = ACTIONS(347),
    [sym_tune_title] = ACTIONS(347),
    [sym_unit_note_length] = ACTIONS(347),
    [anon_sym_U_COLON] = ACTIONS(347),
    [sym_voice] = ACTIONS(347),
    [sym_words_tag] = ACTIONS(347),
    [sym_words_postbody] = ACTIONS(347),
  },
  [48] = {
    [sym_nth_ending_number] = STATE(58),
    [ts_builtin_sym_end] = ACTIONS(277),
    [sym__NL] = ACTIONS(279),
    [anon_sym_BSLASH] = ACTIONS(277),
    [anon_sym_LBRACK] = ACTIONS(281),
    [sym_COMMENT] = ACTIONS(279),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(277),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(283),
    [sym_slur_open] = ACTIONS(279),
    [sym_slur_close] = ACTIONS(277),
    [sym_alteration] = ACTIONS(277),
    [sym_note_letter] = ACTIONS(277),
    [sym_rest] = ACTIONS(277),
    [anon_sym_Z] = ACTIONS(277),
    [anon_sym_X] = ACTIONS(279),
    [aux_sym_grace_note_token1] = ACTIONS(277),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(277),
    [sym_tuplet_marker] = ACTIONS(277),
    [sym_decoration_shorthand] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(277),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(277),
    [sym_start_of_repeated_section] = ACTIONS(277),
    [sym_end_of_repeated_section] = ACTIONS(277),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(277),
    [anon_sym_BANGtrill_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGmordent_BANG] = ACTIONS(277),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(277),
    [anon_sym_BANGroll_BANG] = ACTIONS(277),
    [anon_sym_BANGturn_BANG] = ACTIONS(277),
    [anon_sym_BANGturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_BANG] = ACTIONS(277),
    [anon_sym_BANGaccent_BANG] = ACTIONS(277),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(277),
    [anon_sym_BANGfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(277),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(277),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(277),
    [anon_sym_BANGplus_BANG] = ACTIONS(277),
    [anon_sym_BANGsnap_BANG] = ACTIONS(277),
    [anon_sym_BANGslide_BANG] = ACTIONS(277),
    [anon_sym_BANGwedge_BANG] = ACTIONS(277),
    [anon_sym_BANGupbow_BANG] = ACTIONS(277),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(277),
    [anon_sym_BANGopen_BANG] = ACTIONS(277),
    [anon_sym_BANGthumb_BANG] = ACTIONS(277),
    [anon_sym_BANGbreath_BANG] = ACTIONS(277),
    [anon_sym_BANGpppp_BANG] = ACTIONS(277),
    [anon_sym_BANGppp_BANG] = ACTIONS(277),
    [anon_sym_BANGpp_BANG] = ACTIONS(277),
    [anon_sym_BANGp_BANG] = ACTIONS(277),
    [anon_sym_BANGmp_BANG] = ACTIONS(277),
    [anon_sym_BANGmf_BANG] = ACTIONS(277),
    [anon_sym_BANGf_BANG] = ACTIONS(277),
    [anon_sym_BANGff_BANG] = ACTIONS(277),
    [anon_sym_BANGfff_BANG] = ACTIONS(277),
    [anon_sym_BANGffff_BANG] = ACTIONS(277),
    [anon_sym_BANGsfz_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGsegno_BANG] = ACTIONS(277),
    [anon_sym_BANGcoda_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(277),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(277),
    [anon_sym_BANGfine_BANG] = ACTIONS(277),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(277),
    [sym_parts_line] = ACTIONS(277),
    [sym_instruction] = ACTIONS(277),
    [sym_key] = ACTIONS(277),
    [sym_macros] = ACTIONS(277),
    [sym_meter] = ACTIONS(277),
    [sym_notes] = ACTIONS(277),
    [anon_sym_X_COLON] = ACTIONS(277),
    [sym_remark] = ACTIONS(277),
    [sym_rhythm_line] = ACTIONS(277),
    [anon_sym_s_COLON] = ACTIONS(277),
    [sym_tempo] = ACTIONS(277),
    [sym_tune_title] = ACTIONS(277),
    [sym_unit_note_length] = ACTIONS(277),
    [anon_sym_U_COLON] = ACTIONS(277),
    [sym_voice] = ACTIONS(277),
    [sym_words_tag] = ACTIONS(277),
    [sym_words_postbody] = ACTIONS(277),
  },
  [49] = {
    [ts_builtin_sym_end] = ACTIONS(369),
    [sym__NL] = ACTIONS(371),
    [anon_sym_BSLASH] = ACTIONS(369),
    [sym_BEAM_SEPARATOR] = ACTIONS(369),
    [anon_sym_LBRACK] = ACTIONS(371),
    [sym_COMMENT] = ACTIONS(371),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(369),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(371),
    [sym_slur_open] = ACTIONS(371),
    [sym_slur_close] = ACTIONS(369),
    [sym_alteration] = ACTIONS(369),
    [sym_note_letter] = ACTIONS(369),
    [sym_rest] = ACTIONS(369),
    [anon_sym_Z] = ACTIONS(369),
    [anon_sym_X] = ACTIONS(371),
    [aux_sym_grace_note_token1] = ACTIONS(369),
    [aux_sym_chord_symbol_token1] = ACTIONS(371),
    [sym_annotation] = ACTIONS(369),
    [sym_tuplet_marker] = ACTIONS(369),
    [sym_decoration_shorthand] = ACTIONS(371),
    [sym_bar_line] = ACTIONS(371),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(369),
    [sym_thin_double_bar_line] = ACTIONS(369),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(369),
    [sym_start_of_repeated_section] = ACTIONS(369),
    [sym_end_of_repeated_section] = ACTIONS(369),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(369),
    [anon_sym_BANGtrill_BANG] = ACTIONS(369),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(369),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(369),
    [anon_sym_BANGmordent_BANG] = ACTIONS(369),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(369),
    [anon_sym_BANGroll_BANG] = ACTIONS(369),
    [anon_sym_BANGturn_BANG] = ACTIONS(369),
    [anon_sym_BANGturnx_BANG] = ACTIONS(369),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(369),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(369),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(369),
    [anon_sym_BANG_GT_BANG] = ACTIONS(369),
    [anon_sym_BANGaccent_BANG] = ACTIONS(369),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(369),
    [anon_sym_BANGfermata_BANG] = ACTIONS(369),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(369),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(369),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(369),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(369),
    [anon_sym_BANGplus_BANG] = ACTIONS(369),
    [anon_sym_BANGsnap_BANG] = ACTIONS(369),
    [anon_sym_BANGslide_BANG] = ACTIONS(369),
    [anon_sym_BANGwedge_BANG] = ACTIONS(369),
    [anon_sym_BANGupbow_BANG] = ACTIONS(369),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(369),
    [anon_sym_BANGopen_BANG] = ACTIONS(369),
    [anon_sym_BANGthumb_BANG] = ACTIONS(369),
    [anon_sym_BANGbreath_BANG] = ACTIONS(369),
    [anon_sym_BANGpppp_BANG] = ACTIONS(369),
    [anon_sym_BANGppp_BANG] = ACTIONS(369),
    [anon_sym_BANGpp_BANG] = ACTIONS(369),
    [anon_sym_BANGp_BANG] = ACTIONS(369),
    [anon_sym_BANGmp_BANG] = ACTIONS(369),
    [anon_sym_BANGmf_BANG] = ACTIONS(369),
    [anon_sym_BANGf_BANG] = ACTIONS(369),
    [anon_sym_BANGff_BANG] = ACTIONS(369),
    [anon_sym_BANGfff_BANG] = ACTIONS(369),
    [anon_sym_BANGffff_BANG] = ACTIONS(369),
    [anon_sym_BANGsfz_BANG] = ACTIONS(369),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(369),
    [anon_sym_BANGsegno_BANG] = ACTIONS(369),
    [anon_sym_BANGcoda_BANG] = ACTIONS(369),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(369),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(369),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(369),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(369),
    [anon_sym_BANGfine_BANG] = ACTIONS(369),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(369),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(369),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(369),
    [sym_parts_line] = ACTIONS(369),
    [sym_instruction] = ACTIONS(369),
    [sym_key] = ACTIONS(369),
    [sym_macros] = ACTIONS(369),
    [sym_meter] = ACTIONS(369),
    [sym_notes] = ACTIONS(369),
    [anon_sym_X_COLON] = ACTIONS(369),
    [sym_remark] = ACTIONS(369),
    [sym_rhythm_line] = ACTIONS(369),
    [anon_sym_s_COLON] = ACTIONS(369),
    [sym_tempo] = ACTIONS(369),
    [sym_tune_title] = ACTIONS(369),
    [sym_unit_note_length] = ACTIONS(369),
    [anon_sym_U_COLON] = ACTIONS(369),
    [sym_voice] = ACTIONS(369),
    [sym_words_tag] = ACTIONS(369),
    [sym_words_postbody] = ACTIONS(369),
  },
  [50] = {
    [ts_builtin_sym_end] = ACTIONS(373),
    [sym__NL] = ACTIONS(375),
    [anon_sym_BSLASH] = ACTIONS(373),
    [anon_sym_LBRACK] = ACTIONS(375),
    [sym_COMMENT] = ACTIONS(375),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(373),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(375),
    [sym_slur_open] = ACTIONS(375),
    [sym_slur_close] = ACTIONS(373),
    [sym_alteration] = ACTIONS(373),
    [sym_note_letter] = ACTIONS(373),
    [sym_rest] = ACTIONS(373),
    [anon_sym_Z] = ACTIONS(373),
    [anon_sym_X] = ACTIONS(375),
    [aux_sym_grace_note_token1] = ACTIONS(373),
    [aux_sym_chord_symbol_token1] = ACTIONS(375),
    [sym_annotation] = ACTIONS(373),
    [sym_tuplet_marker] = ACTIONS(373),
    [sym_decoration_shorthand] = ACTIONS(375),
    [sym_bar_line] = ACTIONS(375),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(373),
    [sym_thin_double_bar_line] = ACTIONS(373),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(373),
    [sym_start_of_repeated_section] = ACTIONS(373),
    [sym_end_of_repeated_section] = ACTIONS(373),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(373),
    [anon_sym_BANGtrill_BANG] = ACTIONS(373),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(373),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(373),
    [anon_sym_BANGmordent_BANG] = ACTIONS(373),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(373),
    [anon_sym_BANGroll_BANG] = ACTIONS(373),
    [anon_sym_BANGturn_BANG] = ACTIONS(373),
    [anon_sym_BANGturnx_BANG] = ACTIONS(373),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(373),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(373),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(373),
    [anon_sym_BANG_GT_BANG] = ACTIONS(373),
    [anon_sym_BANGaccent_BANG] = ACTIONS(373),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(373),
    [anon_sym_BANGfermata_BANG] = ACTIONS(373),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(373),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(373),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(373),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(373),
    [anon_sym_BANGplus_BANG] = ACTIONS(373),
    [anon_sym_BANGsnap_BANG] = ACTIONS(373),
    [anon_sym_BANGslide_BANG] = ACTIONS(373),
    [anon_sym_BANGwedge_BANG] = ACTIONS(373),
    [anon_sym_BANGupbow_BANG] = ACTIONS(373),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(373),
    [anon_sym_BANGopen_BANG] = ACTIONS(373),
    [anon_sym_BANGthumb_BANG] = ACTIONS(373),
    [anon_sym_BANGbreath_BANG] = ACTIONS(373),
    [anon_sym_BANGpppp_BANG] = ACTIONS(373),
    [anon_sym_BANGppp_BANG] = ACTIONS(373),
    [anon_sym_BANGpp_BANG] = ACTIONS(373),
    [anon_sym_BANGp_BANG] = ACTIONS(373),
    [anon_sym_BANGmp_BANG] = ACTIONS(373),
    [anon_sym_BANGmf_BANG] = ACTIONS(373),
    [anon_sym_BANGf_BANG] = ACTIONS(373),
    [anon_sym_BANGff_BANG] = ACTIONS(373),
    [anon_sym_BANGfff_BANG] = ACTIONS(373),
    [anon_sym_BANGffff_BANG] = ACTIONS(373),
    [anon_sym_BANGsfz_BANG] = ACTIONS(373),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(373),
    [anon_sym_BANGsegno_BANG] = ACTIONS(373),
    [anon_sym_BANGcoda_BANG] = ACTIONS(373),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(373),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(373),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(373),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(373),
    [anon_sym_BANGfine_BANG] = ACTIONS(373),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(373),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(373),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(373),
    [sym_parts_line] = ACTIONS(373),
    [sym_instruction] = ACTIONS(373),
    [sym_key] = ACTIONS(373),
    [sym_macros] = ACTIONS(373),
    [sym_meter] = ACTIONS(373),
    [sym_notes] = ACTIONS(373),
    [anon_sym_X_COLON] = ACTIONS(373),
    [sym_remark] = ACTIONS(373),
    [sym_rhythm_line] = ACTIONS(373),
    [anon_sym_s_COLON] = ACTIONS(373),
    [sym_tempo] = ACTIONS(373),
    [sym_tune_title] = ACTIONS(373),
    [sym_unit_note_length] = ACTIONS(373),
    [anon_sym_U_COLON] = ACTIONS(373),
    [sym_voice] = ACTIONS(373),
    [sym_words_tag] = ACTIONS(373),
    [sym_words_postbody] = ACTIONS(373),
  },
  [51] = {
    [ts_builtin_sym_end] = ACTIONS(377),
    [sym__NL] = ACTIONS(379),
    [anon_sym_BSLASH] = ACTIONS(377),
    [anon_sym_LBRACK] = ACTIONS(379),
    [sym_COMMENT] = ACTIONS(379),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(377),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(379),
    [sym_slur_open] = ACTIONS(379),
    [sym_slur_close] = ACTIONS(377),
    [sym_alteration] = ACTIONS(377),
    [sym_note_letter] = ACTIONS(377),
    [sym_rest] = ACTIONS(377),
    [anon_sym_Z] = ACTIONS(377),
    [anon_sym_X] = ACTIONS(379),
    [aux_sym_grace_note_token1] = ACTIONS(377),
    [aux_sym_chord_symbol_token1] = ACTIONS(379),
    [sym_annotation] = ACTIONS(377),
    [sym_tuplet_marker] = ACTIONS(377),
    [sym_decoration_shorthand] = ACTIONS(379),
    [sym_bar_line] = ACTIONS(379),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(377),
    [sym_thin_double_bar_line] = ACTIONS(377),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(377),
    [sym_start_of_repeated_section] = ACTIONS(377),
    [sym_end_of_repeated_section] = ACTIONS(377),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(377),
    [anon_sym_BANGtrill_BANG] = ACTIONS(377),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(377),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(377),
    [anon_sym_BANGmordent_BANG] = ACTIONS(377),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(377),
    [anon_sym_BANGroll_BANG] = ACTIONS(377),
    [anon_sym_BANGturn_BANG] = ACTIONS(377),
    [anon_sym_BANGturnx_BANG] = ACTIONS(377),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(377),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(377),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(377),
    [anon_sym_BANG_GT_BANG] = ACTIONS(377),
    [anon_sym_BANGaccent_BANG] = ACTIONS(377),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(377),
    [anon_sym_BANGfermata_BANG] = ACTIONS(377),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(377),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(377),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(377),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(377),
    [anon_sym_BANGplus_BANG] = ACTIONS(377),
    [anon_sym_BANGsnap_BANG] = ACTIONS(377),
    [anon_sym_BANGslide_BANG] = ACTIONS(377),
    [anon_sym_BANGwedge_BANG] = ACTIONS(377),
    [anon_sym_BANGupbow_BANG] = ACTIONS(377),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(377),
    [anon_sym_BANGopen_BANG] = ACTIONS(377),
    [anon_sym_BANGthumb_BANG] = ACTIONS(377),
    [anon_sym_BANGbreath_BANG] = ACTIONS(377),
    [anon_sym_BANGpppp_BANG] = ACTIONS(377),
    [anon_sym_BANGppp_BANG] = ACTIONS(377),
    [anon_sym_BANGpp_BANG] = ACTIONS(377),
    [anon_sym_BANGp_BANG] = ACTIONS(377),
    [anon_sym_BANGmp_BANG] = ACTIONS(377),
    [anon_sym_BANGmf_BANG] = ACTIONS(377),
    [anon_sym_BANGf_BANG] = ACTIONS(377),
    [anon_sym_BANGff_BANG] = ACTIONS(377),
    [anon_sym_BANGfff_BANG] = ACTIONS(377),
    [anon_sym_BANGffff_BANG] = ACTIONS(377),
    [anon_sym_BANGsfz_BANG] = ACTIONS(377),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(377),
    [anon_sym_BANGsegno_BANG] = ACTIONS(377),
    [anon_sym_BANGcoda_BANG] = ACTIONS(377),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(377),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(377),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(377),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(377),
    [anon_sym_BANGfine_BANG] = ACTIONS(377),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(377),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(377),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(377),
    [sym_parts_line] = ACTIONS(377),
    [sym_instruction] = ACTIONS(377),
    [sym_key] = ACTIONS(377),
    [sym_macros] = ACTIONS(377),
    [sym_meter] = ACTIONS(377),
    [sym_notes] = ACTIONS(377),
    [anon_sym_X_COLON] = ACTIONS(377),
    [sym_remark] = ACTIONS(377),
    [sym_rhythm_line] = ACTIONS(377),
    [anon_sym_s_COLON] = ACTIONS(377),
    [sym_tempo] = ACTIONS(377),
    [sym_tune_title] = ACTIONS(377),
    [sym_unit_note_length] = ACTIONS(377),
    [anon_sym_U_COLON] = ACTIONS(377),
    [sym_voice] = ACTIONS(377),
    [sym_words_tag] = ACTIONS(377),
    [sym_words_postbody] = ACTIONS(377),
  },
  [52] = {
    [ts_builtin_sym_end] = ACTIONS(381),
    [sym__NL] = ACTIONS(383),
    [anon_sym_BSLASH] = ACTIONS(381),
    [anon_sym_LBRACK] = ACTIONS(383),
    [sym_COMMENT] = ACTIONS(383),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(381),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(383),
    [sym_slur_open] = ACTIONS(383),
    [sym_slur_close] = ACTIONS(381),
    [sym_alteration] = ACTIONS(381),
    [sym_note_letter] = ACTIONS(381),
    [sym_rest] = ACTIONS(381),
    [anon_sym_Z] = ACTIONS(381),
    [anon_sym_X] = ACTIONS(383),
    [aux_sym_grace_note_token1] = ACTIONS(381),
    [aux_sym_chord_symbol_token1] = ACTIONS(383),
    [sym_annotation] = ACTIONS(381),
    [sym_tuplet_marker] = ACTIONS(381),
    [sym_decoration_shorthand] = ACTIONS(383),
    [sym_bar_line] = ACTIONS(383),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(381),
    [sym_thin_double_bar_line] = ACTIONS(381),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(381),
    [sym_start_of_repeated_section] = ACTIONS(381),
    [sym_end_of_repeated_section] = ACTIONS(381),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(381),
    [anon_sym_BANGtrill_BANG] = ACTIONS(381),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(381),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(381),
    [anon_sym_BANGmordent_BANG] = ACTIONS(381),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(381),
    [anon_sym_BANGroll_BANG] = ACTIONS(381),
    [anon_sym_BANGturn_BANG] = ACTIONS(381),
    [anon_sym_BANGturnx_BANG] = ACTIONS(381),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(381),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(381),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(381),
    [anon_sym_BANG_GT_BANG] = ACTIONS(381),
    [anon_sym_BANGaccent_BANG] = ACTIONS(381),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(381),
    [anon_sym_BANGfermata_BANG] = ACTIONS(381),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(381),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(381),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(381),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(381),
    [anon_sym_BANGplus_BANG] = ACTIONS(381),
    [anon_sym_BANGsnap_BANG] = ACTIONS(381),
    [anon_sym_BANGslide_BANG] = ACTIONS(381),
    [anon_sym_BANGwedge_BANG] = ACTIONS(381),
    [anon_sym_BANGupbow_BANG] = ACTIONS(381),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(381),
    [anon_sym_BANGopen_BANG] = ACTIONS(381),
    [anon_sym_BANGthumb_BANG] = ACTIONS(381),
    [anon_sym_BANGbreath_BANG] = ACTIONS(381),
    [anon_sym_BANGpppp_BANG] = ACTIONS(381),
    [anon_sym_BANGppp_BANG] = ACTIONS(381),
    [anon_sym_BANGpp_BANG] = ACTIONS(381),
    [anon_sym_BANGp_BANG] = ACTIONS(381),
    [anon_sym_BANGmp_BANG] = ACTIONS(381),
    [anon_sym_BANGmf_BANG] = ACTIONS(381),
    [anon_sym_BANGf_BANG] = ACTIONS(381),
    [anon_sym_BANGff_BANG] = ACTIONS(381),
    [anon_sym_BANGfff_BANG] = ACTIONS(381),
    [anon_sym_BANGffff_BANG] = ACTIONS(381),
    [anon_sym_BANGsfz_BANG] = ACTIONS(381),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(381),
    [anon_sym_BANGsegno_BANG] = ACTIONS(381),
    [anon_sym_BANGcoda_BANG] = ACTIONS(381),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(381),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(381),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(381),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(381),
    [anon_sym_BANGfine_BANG] = ACTIONS(381),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(381),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(381),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(381),
    [sym_parts_line] = ACTIONS(381),
    [sym_instruction] = ACTIONS(381),
    [sym_key] = ACTIONS(381),
    [sym_macros] = ACTIONS(381),
    [sym_meter] = ACTIONS(381),
    [sym_notes] = ACTIONS(381),
    [anon_sym_X_COLON] = ACTIONS(381),
    [sym_remark] = ACTIONS(381),
    [sym_rhythm_line] = ACTIONS(381),
    [anon_sym_s_COLON] = ACTIONS(381),
    [sym_tempo] = ACTIONS(381),
    [sym_tune_title] = ACTIONS(381),
    [sym_unit_note_length] = ACTIONS(381),
    [anon_sym_U_COLON] = ACTIONS(381),
    [sym_voice] = ACTIONS(381),
    [sym_words_tag] = ACTIONS(381),
    [sym_words_postbody] = ACTIONS(381),
  },
  [53] = {
    [ts_builtin_sym_end] = ACTIONS(385),
    [sym__NL] = ACTIONS(387),
    [anon_sym_BSLASH] = ACTIONS(385),
    [anon_sym_LBRACK] = ACTIONS(387),
    [sym_COMMENT] = ACTIONS(387),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(385),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(387),
    [sym_slur_open] = ACTIONS(387),
    [sym_slur_close] = ACTIONS(385),
    [sym_alteration] = ACTIONS(385),
    [sym_note_letter] = ACTIONS(385),
    [sym_rest] = ACTIONS(385),
    [anon_sym_Z] = ACTIONS(385),
    [anon_sym_X] = ACTIONS(387),
    [aux_sym_grace_note_token1] = ACTIONS(385),
    [aux_sym_chord_symbol_token1] = ACTIONS(387),
    [sym_annotation] = ACTIONS(385),
    [sym_tuplet_marker] = ACTIONS(385),
    [sym_decoration_shorthand] = ACTIONS(387),
    [sym_bar_line] = ACTIONS(387),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(385),
    [sym_thin_double_bar_line] = ACTIONS(385),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(385),
    [sym_start_of_repeated_section] = ACTIONS(385),
    [sym_end_of_repeated_section] = ACTIONS(385),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(385),
    [anon_sym_BANGtrill_BANG] = ACTIONS(385),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(385),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(385),
    [anon_sym_BANGmordent_BANG] = ACTIONS(385),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(385),
    [anon_sym_BANGroll_BANG] = ACTIONS(385),
    [anon_sym_BANGturn_BANG] = ACTIONS(385),
    [anon_sym_BANGturnx_BANG] = ACTIONS(385),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(385),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(385),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(385),
    [anon_sym_BANG_GT_BANG] = ACTIONS(385),
    [anon_sym_BANGaccent_BANG] = ACTIONS(385),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(385),
    [anon_sym_BANGfermata_BANG] = ACTIONS(385),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(385),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(385),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(385),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(385),
    [anon_sym_BANGplus_BANG] = ACTIONS(385),
    [anon_sym_BANGsnap_BANG] = ACTIONS(385),
    [anon_sym_BANGslide_BANG] = ACTIONS(385),
    [anon_sym_BANGwedge_BANG] = ACTIONS(385),
    [anon_sym_BANGupbow_BANG] = ACTIONS(385),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(385),
    [anon_sym_BANGopen_BANG] = ACTIONS(385),
    [anon_sym_BANGthumb_BANG] = ACTIONS(385),
    [anon_sym_BANGbreath_BANG] = ACTIONS(385),
    [anon_sym_BANGpppp_BANG] = ACTIONS(385),
    [anon_sym_BANGppp_BANG] = ACTIONS(385),
    [anon_sym_BANGpp_BANG] = ACTIONS(385),
    [anon_sym_BANGp_BANG] = ACTIONS(385),
    [anon_sym_BANGmp_BANG] = ACTIONS(385),
    [anon_sym_BANGmf_BANG] = ACTIONS(385),
    [anon_sym_BANGf_BANG] = ACTIONS(385),
    [anon_sym_BANGff_BANG] = ACTIONS(385),
    [anon_sym_BANGfff_BANG] = ACTIONS(385),
    [anon_sym_BANGffff_BANG] = ACTIONS(385),
    [anon_sym_BANGsfz_BANG] = ACTIONS(385),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(385),
    [anon_sym_BANGsegno_BANG] = ACTIONS(385),
    [anon_sym_BANGcoda_BANG] = ACTIONS(385),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(385),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(385),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(385),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(385),
    [anon_sym_BANGfine_BANG] = ACTIONS(385),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(385),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(385),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(385),
    [sym_parts_line] = ACTIONS(385),
    [sym_instruction] = ACTIONS(385),
    [sym_key] = ACTIONS(385),
    [sym_macros] = ACTIONS(385),
    [sym_meter] = ACTIONS(385),
    [sym_notes] = ACTIONS(385),
    [anon_sym_X_COLON] = ACTIONS(385),
    [sym_remark] = ACTIONS(385),
    [sym_rhythm_line] = ACTIONS(385),
    [anon_sym_s_COLON] = ACTIONS(385),
    [sym_tempo] = ACTIONS(385),
    [sym_tune_title] = ACTIONS(385),
    [sym_unit_note_length] = ACTIONS(385),
    [anon_sym_U_COLON] = ACTIONS(385),
    [sym_voice] = ACTIONS(385),
    [sym_words_tag] = ACTIONS(385),
    [sym_words_postbody] = ACTIONS(385),
  },
  [54] = {
    [ts_builtin_sym_end] = ACTIONS(389),
    [sym__NL] = ACTIONS(391),
    [anon_sym_BSLASH] = ACTIONS(389),
    [anon_sym_LBRACK] = ACTIONS(391),
    [sym_COMMENT] = ACTIONS(391),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(389),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(393),
    [sym_slur_open] = ACTIONS(391),
    [sym_slur_close] = ACTIONS(389),
    [sym_alteration] = ACTIONS(389),
    [sym_note_letter] = ACTIONS(389),
    [sym_rest] = ACTIONS(389),
    [anon_sym_Z] = ACTIONS(389),
    [anon_sym_X] = ACTIONS(391),
    [aux_sym_grace_note_token1] = ACTIONS(389),
    [aux_sym_chord_symbol_token1] = ACTIONS(391),
    [sym_annotation] = ACTIONS(389),
    [sym_tuplet_marker] = ACTIONS(389),
    [sym_decoration_shorthand] = ACTIONS(391),
    [sym_bar_line] = ACTIONS(391),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(389),
    [sym_thin_double_bar_line] = ACTIONS(389),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(389),
    [sym_start_of_repeated_section] = ACTIONS(389),
    [sym_end_of_repeated_section] = ACTIONS(389),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(389),
    [anon_sym_BANGtrill_BANG] = ACTIONS(389),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(389),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(389),
    [anon_sym_BANGmordent_BANG] = ACTIONS(389),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(389),
    [anon_sym_BANGroll_BANG] = ACTIONS(389),
    [anon_sym_BANGturn_BANG] = ACTIONS(389),
    [anon_sym_BANGturnx_BANG] = ACTIONS(389),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(389),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(389),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(389),
    [anon_sym_BANG_GT_BANG] = ACTIONS(389),
    [anon_sym_BANGaccent_BANG] = ACTIONS(389),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(389),
    [anon_sym_BANGfermata_BANG] = ACTIONS(389),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(389),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(389),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(389),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(389),
    [anon_sym_BANGplus_BANG] = ACTIONS(389),
    [anon_sym_BANGsnap_BANG] = ACTIONS(389),
    [anon_sym_BANGslide_BANG] = ACTIONS(389),
    [anon_sym_BANGwedge_BANG] = ACTIONS(389),
    [anon_sym_BANGupbow_BANG] = ACTIONS(389),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(389),
    [anon_sym_BANGopen_BANG] = ACTIONS(389),
    [anon_sym_BANGthumb_BANG] = ACTIONS(389),
    [anon_sym_BANGbreath_BANG] = ACTIONS(389),
    [anon_sym_BANGpppp_BANG] = ACTIONS(389),
    [anon_sym_BANGppp_BANG] = ACTIONS(389),
    [anon_sym_BANGpp_BANG] = ACTIONS(389),
    [anon_sym_BANGp_BANG] = ACTIONS(389),
    [anon_sym_BANGmp_BANG] = ACTIONS(389),
    [anon_sym_BANGmf_BANG] = ACTIONS(389),
    [anon_sym_BANGf_BANG] = ACTIONS(389),
    [anon_sym_BANGff_BANG] = ACTIONS(389),
    [anon_sym_BANGfff_BANG] = ACTIONS(389),
    [anon_sym_BANGffff_BANG] = ACTIONS(389),
    [anon_sym_BANGsfz_BANG] = ACTIONS(389),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(389),
    [anon_sym_BANGsegno_BANG] = ACTIONS(389),
    [anon_sym_BANGcoda_BANG] = ACTIONS(389),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(389),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(389),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(389),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(389),
    [anon_sym_BANGfine_BANG] = ACTIONS(389),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(389),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(389),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(389),
    [sym_parts_line] = ACTIONS(389),
    [sym_instruction] = ACTIONS(389),
    [sym_key] = ACTIONS(389),
    [sym_macros] = ACTIONS(389),
    [sym_meter] = ACTIONS(389),
    [sym_notes] = ACTIONS(389),
    [anon_sym_X_COLON] = ACTIONS(389),
    [sym_remark] = ACTIONS(389),
    [sym_rhythm_line] = ACTIONS(389),
    [anon_sym_s_COLON] = ACTIONS(389),
    [sym_tempo] = ACTIONS(389),
    [sym_tune_title] = ACTIONS(389),
    [sym_unit_note_length] = ACTIONS(389),
    [anon_sym_U_COLON] = ACTIONS(389),
    [sym_voice] = ACTIONS(389),
    [sym_words_tag] = ACTIONS(389),
    [sym_words_postbody] = ACTIONS(389),
  },
  [55] = {
    [ts_builtin_sym_end] = ACTIONS(395),
    [sym__NL] = ACTIONS(397),
    [anon_sym_BSLASH] = ACTIONS(395),
    [anon_sym_LBRACK] = ACTIONS(397),
    [sym_COMMENT] = ACTIONS(397),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(395),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(397),
    [sym_slur_open] = ACTIONS(397),
    [sym_slur_close] = ACTIONS(395),
    [sym_alteration] = ACTIONS(395),
    [sym_note_letter] = ACTIONS(395),
    [sym_rest] = ACTIONS(395),
    [anon_sym_Z] = ACTIONS(395),
    [anon_sym_X] = ACTIONS(397),
    [aux_sym_grace_note_token1] = ACTIONS(395),
    [aux_sym_chord_symbol_token1] = ACTIONS(397),
    [sym_annotation] = ACTIONS(395),
    [sym_tuplet_marker] = ACTIONS(395),
    [sym_decoration_shorthand] = ACTIONS(397),
    [sym_bar_line] = ACTIONS(397),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(395),
    [sym_thin_double_bar_line] = ACTIONS(395),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(395),
    [sym_start_of_repeated_section] = ACTIONS(395),
    [sym_end_of_repeated_section] = ACTIONS(395),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(395),
    [anon_sym_BANGtrill_BANG] = ACTIONS(395),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(395),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(395),
    [anon_sym_BANGmordent_BANG] = ACTIONS(395),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(395),
    [anon_sym_BANGroll_BANG] = ACTIONS(395),
    [anon_sym_BANGturn_BANG] = ACTIONS(395),
    [anon_sym_BANGturnx_BANG] = ACTIONS(395),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(395),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(395),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(395),
    [anon_sym_BANG_GT_BANG] = ACTIONS(395),
    [anon_sym_BANGaccent_BANG] = ACTIONS(395),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(395),
    [anon_sym_BANGfermata_BANG] = ACTIONS(395),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(395),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(395),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(395),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(395),
    [anon_sym_BANGplus_BANG] = ACTIONS(395),
    [anon_sym_BANGsnap_BANG] = ACTIONS(395),
    [anon_sym_BANGslide_BANG] = ACTIONS(395),
    [anon_sym_BANGwedge_BANG] = ACTIONS(395),
    [anon_sym_BANGupbow_BANG] = ACTIONS(395),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(395),
    [anon_sym_BANGopen_BANG] = ACTIONS(395),
    [anon_sym_BANGthumb_BANG] = ACTIONS(395),
    [anon_sym_BANGbreath_BANG] = ACTIONS(395),
    [anon_sym_BANGpppp_BANG] = ACTIONS(395),
    [anon_sym_BANGppp_BANG] = ACTIONS(395),
    [anon_sym_BANGpp_BANG] = ACTIONS(395),
    [anon_sym_BANGp_BANG] = ACTIONS(395),
    [anon_sym_BANGmp_BANG] = ACTIONS(395),
    [anon_sym_BANGmf_BANG] = ACTIONS(395),
    [anon_sym_BANGf_BANG] = ACTIONS(395),
    [anon_sym_BANGff_BANG] = ACTIONS(395),
    [anon_sym_BANGfff_BANG] = ACTIONS(395),
    [anon_sym_BANGffff_BANG] = ACTIONS(395),
    [anon_sym_BANGsfz_BANG] = ACTIONS(395),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(395),
    [anon_sym_BANGsegno_BANG] = ACTIONS(395),
    [anon_sym_BANGcoda_BANG] = ACTIONS(395),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(395),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(395),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(395),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(395),
    [anon_sym_BANGfine_BANG] = ACTIONS(395),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(395),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(395),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(395),
    [sym_parts_line] = ACTIONS(395),
    [sym_instruction] = ACTIONS(395),
    [sym_key] = ACTIONS(395),
    [sym_macros] = ACTIONS(395),
    [sym_meter] = ACTIONS(395),
    [sym_notes] = ACTIONS(395),
    [anon_sym_X_COLON] = ACTIONS(395),
    [sym_remark] = ACTIONS(395),
    [sym_rhythm_line] = ACTIONS(395),
    [anon_sym_s_COLON] = ACTIONS(395),
    [sym_tempo] = ACTIONS(395),
    [sym_tune_title] = ACTIONS(395),
    [sym_unit_note_length] = ACTIONS(395),
    [anon_sym_U_COLON] = ACTIONS(395),
    [sym_voice] = ACTIONS(395),
    [sym_words_tag] = ACTIONS(395),
    [sym_words_postbody] = ACTIONS(395),
  },
  [56] = {
    [ts_builtin_sym_end] = ACTIONS(399),
    [sym__NL] = ACTIONS(401),
    [anon_sym_BSLASH] = ACTIONS(399),
    [anon_sym_LBRACK] = ACTIONS(401),
    [sym_COMMENT] = ACTIONS(401),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(399),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(401),
    [sym_slur_open] = ACTIONS(401),
    [sym_slur_close] = ACTIONS(399),
    [sym_alteration] = ACTIONS(399),
    [sym_note_letter] = ACTIONS(399),
    [sym_rest] = ACTIONS(399),
    [anon_sym_Z] = ACTIONS(399),
    [anon_sym_X] = ACTIONS(401),
    [aux_sym_grace_note_token1] = ACTIONS(399),
    [aux_sym_chord_symbol_token1] = ACTIONS(401),
    [sym_annotation] = ACTIONS(399),
    [sym_tuplet_marker] = ACTIONS(399),
    [sym_decoration_shorthand] = ACTIONS(401),
    [sym_bar_line] = ACTIONS(401),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(399),
    [sym_thin_double_bar_line] = ACTIONS(399),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(399),
    [sym_start_of_repeated_section] = ACTIONS(399),
    [sym_end_of_repeated_section] = ACTIONS(399),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(399),
    [anon_sym_BANGtrill_BANG] = ACTIONS(399),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(399),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(399),
    [anon_sym_BANGmordent_BANG] = ACTIONS(399),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(399),
    [anon_sym_BANGroll_BANG] = ACTIONS(399),
    [anon_sym_BANGturn_BANG] = ACTIONS(399),
    [anon_sym_BANGturnx_BANG] = ACTIONS(399),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(399),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(399),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(399),
    [anon_sym_BANG_GT_BANG] = ACTIONS(399),
    [anon_sym_BANGaccent_BANG] = ACTIONS(399),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(399),
    [anon_sym_BANGfermata_BANG] = ACTIONS(399),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(399),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(399),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(399),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(399),
    [anon_sym_BANGplus_BANG] = ACTIONS(399),
    [anon_sym_BANGsnap_BANG] = ACTIONS(399),
    [anon_sym_BANGslide_BANG] = ACTIONS(399),
    [anon_sym_BANGwedge_BANG] = ACTIONS(399),
    [anon_sym_BANGupbow_BANG] = ACTIONS(399),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(399),
    [anon_sym_BANGopen_BANG] = ACTIONS(399),
    [anon_sym_BANGthumb_BANG] = ACTIONS(399),
    [anon_sym_BANGbreath_BANG] = ACTIONS(399),
    [anon_sym_BANGpppp_BANG] = ACTIONS(399),
    [anon_sym_BANGppp_BANG] = ACTIONS(399),
    [anon_sym_BANGpp_BANG] = ACTIONS(399),
    [anon_sym_BANGp_BANG] = ACTIONS(399),
    [anon_sym_BANGmp_BANG] = ACTIONS(399),
    [anon_sym_BANGmf_BANG] = ACTIONS(399),
    [anon_sym_BANGf_BANG] = ACTIONS(399),
    [anon_sym_BANGff_BANG] = ACTIONS(399),
    [anon_sym_BANGfff_BANG] = ACTIONS(399),
    [anon_sym_BANGffff_BANG] = ACTIONS(399),
    [anon_sym_BANGsfz_BANG] = ACTIONS(399),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(399),
    [anon_sym_BANGsegno_BANG] = ACTIONS(399),
    [anon_sym_BANGcoda_BANG] = ACTIONS(399),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(399),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(399),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(399),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(399),
    [anon_sym_BANGfine_BANG] = ACTIONS(399),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(399),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(399),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(399),
    [sym_parts_line] = ACTIONS(399),
    [sym_instruction] = ACTIONS(399),
    [sym_key] = ACTIONS(399),
    [sym_macros] = ACTIONS(399),
    [sym_meter] = ACTIONS(399),
    [sym_notes] = ACTIONS(399),
    [anon_sym_X_COLON] = ACTIONS(399),
    [sym_remark] = ACTIONS(399),
    [sym_rhythm_line] = ACTIONS(399),
    [anon_sym_s_COLON] = ACTIONS(399),
    [sym_tempo] = ACTIONS(399),
    [sym_tune_title] = ACTIONS(399),
    [sym_unit_note_length] = ACTIONS(399),
    [anon_sym_U_COLON] = ACTIONS(399),
    [sym_voice] = ACTIONS(399),
    [sym_words_tag] = ACTIONS(399),
    [sym_words_postbody] = ACTIONS(399),
  },
  [57] = {
    [ts_builtin_sym_end] = ACTIONS(403),
    [sym__NL] = ACTIONS(405),
    [anon_sym_BSLASH] = ACTIONS(403),
    [anon_sym_LBRACK] = ACTIONS(405),
    [sym_COMMENT] = ACTIONS(405),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(403),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(405),
    [sym_slur_open] = ACTIONS(405),
    [sym_slur_close] = ACTIONS(403),
    [sym_alteration] = ACTIONS(403),
    [sym_note_letter] = ACTIONS(403),
    [sym_rest] = ACTIONS(403),
    [anon_sym_Z] = ACTIONS(403),
    [anon_sym_X] = ACTIONS(405),
    [aux_sym_grace_note_token1] = ACTIONS(403),
    [aux_sym_chord_symbol_token1] = ACTIONS(405),
    [sym_annotation] = ACTIONS(403),
    [sym_tuplet_marker] = ACTIONS(403),
    [sym_decoration_shorthand] = ACTIONS(405),
    [sym_bar_line] = ACTIONS(405),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(403),
    [sym_thin_double_bar_line] = ACTIONS(403),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(403),
    [sym_start_of_repeated_section] = ACTIONS(403),
    [sym_end_of_repeated_section] = ACTIONS(403),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(403),
    [anon_sym_BANGtrill_BANG] = ACTIONS(403),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(403),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(403),
    [anon_sym_BANGmordent_BANG] = ACTIONS(403),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(403),
    [anon_sym_BANGroll_BANG] = ACTIONS(403),
    [anon_sym_BANGturn_BANG] = ACTIONS(403),
    [anon_sym_BANGturnx_BANG] = ACTIONS(403),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(403),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(403),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(403),
    [anon_sym_BANG_GT_BANG] = ACTIONS(403),
    [anon_sym_BANGaccent_BANG] = ACTIONS(403),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(403),
    [anon_sym_BANGfermata_BANG] = ACTIONS(403),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(403),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(403),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(403),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(403),
    [anon_sym_BANGplus_BANG] = ACTIONS(403),
    [anon_sym_BANGsnap_BANG] = ACTIONS(403),
    [anon_sym_BANGslide_BANG] = ACTIONS(403),
    [anon_sym_BANGwedge_BANG] = ACTIONS(403),
    [anon_sym_BANGupbow_BANG] = ACTIONS(403),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(403),
    [anon_sym_BANGopen_BANG] = ACTIONS(403),
    [anon_sym_BANGthumb_BANG] = ACTIONS(403),
    [anon_sym_BANGbreath_BANG] = ACTIONS(403),
    [anon_sym_BANGpppp_BANG] = ACTIONS(403),
    [anon_sym_BANGppp_BANG] = ACTIONS(403),
    [anon_sym_BANGpp_BANG] = ACTIONS(403),
    [anon_sym_BANGp_BANG] = ACTIONS(403),
    [anon_sym_BANGmp_BANG] = ACTIONS(403),
    [anon_sym_BANGmf_BANG] = ACTIONS(403),
    [anon_sym_BANGf_BANG] = ACTIONS(403),
    [anon_sym_BANGff_BANG] = ACTIONS(403),
    [anon_sym_BANGfff_BANG] = ACTIONS(403),
    [anon_sym_BANGffff_BANG] = ACTIONS(403),
    [anon_sym_BANGsfz_BANG] = ACTIONS(403),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(403),
    [anon_sym_BANGsegno_BANG] = ACTIONS(403),
    [anon_sym_BANGcoda_BANG] = ACTIONS(403),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(403),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(403),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(403),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(403),
    [anon_sym_BANGfine_BANG] = ACTIONS(403),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(403),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(403),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(403),
    [sym_parts_line] = ACTIONS(403),
    [sym_instruction] = ACTIONS(403),
    [sym_key] = ACTIONS(403),
    [sym_macros] = ACTIONS(403),
    [sym_meter] = ACTIONS(403),
    [sym_notes] = ACTIONS(403),
    [anon_sym_X_COLON] = ACTIONS(403),
    [sym_remark] = ACTIONS(403),
    [sym_rhythm_line] = ACTIONS(403),
    [anon_sym_s_COLON] = ACTIONS(403),
    [sym_tempo] = ACTIONS(403),
    [sym_tune_title] = ACTIONS(403),
    [sym_unit_note_length] = ACTIONS(403),
    [anon_sym_U_COLON] = ACTIONS(403),
    [sym_voice] = ACTIONS(403),
    [sym_words_tag] = ACTIONS(403),
    [sym_words_postbody] = ACTIONS(403),
  },
  [58] = {
    [ts_builtin_sym_end] = ACTIONS(407),
    [sym__NL] = ACTIONS(409),
    [anon_sym_BSLASH] = ACTIONS(407),
    [anon_sym_LBRACK] = ACTIONS(409),
    [sym_COMMENT] = ACTIONS(409),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(407),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(409),
    [sym_slur_open] = ACTIONS(409),
    [sym_slur_close] = ACTIONS(407),
    [sym_alteration] = ACTIONS(407),
    [sym_note_letter] = ACTIONS(407),
    [sym_rest] = ACTIONS(407),
    [anon_sym_Z] = ACTIONS(407),
    [anon_sym_X] = ACTIONS(409),
    [aux_sym_grace_note_token1] = ACTIONS(407),
    [aux_sym_chord_symbol_token1] = ACTIONS(409),
    [sym_annotation] = ACTIONS(407),
    [sym_tuplet_marker] = ACTIONS(407),
    [sym_decoration_shorthand] = ACTIONS(409),
    [sym_bar_line] = ACTIONS(409),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(407),
    [sym_thin_double_bar_line] = ACTIONS(407),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(407),
    [sym_start_of_repeated_section] = ACTIONS(407),
    [sym_end_of_repeated_section] = ACTIONS(407),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(407),
    [anon_sym_BANGtrill_BANG] = ACTIONS(407),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(407),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(407),
    [anon_sym_BANGmordent_BANG] = ACTIONS(407),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(407),
    [anon_sym_BANGroll_BANG] = ACTIONS(407),
    [anon_sym_BANGturn_BANG] = ACTIONS(407),
    [anon_sym_BANGturnx_BANG] = ACTIONS(407),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(407),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(407),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(407),
    [anon_sym_BANG_GT_BANG] = ACTIONS(407),
    [anon_sym_BANGaccent_BANG] = ACTIONS(407),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(407),
    [anon_sym_BANGfermata_BANG] = ACTIONS(407),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(407),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(407),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(407),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(407),
    [anon_sym_BANGplus_BANG] = ACTIONS(407),
    [anon_sym_BANGsnap_BANG] = ACTIONS(407),
    [anon_sym_BANGslide_BANG] = ACTIONS(407),
    [anon_sym_BANGwedge_BANG] = ACTIONS(407),
    [anon_sym_BANGupbow_BANG] = ACTIONS(407),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(407),
    [anon_sym_BANGopen_BANG] = ACTIONS(407),
    [anon_sym_BANGthumb_BANG] = ACTIONS(407),
    [anon_sym_BANGbreath_BANG] = ACTIONS(407),
    [anon_sym_BANGpppp_BANG] = ACTIONS(407),
    [anon_sym_BANGppp_BANG] = ACTIONS(407),
    [anon_sym_BANGpp_BANG] = ACTIONS(407),
    [anon_sym_BANGp_BANG] = ACTIONS(407),
    [anon_sym_BANGmp_BANG] = ACTIONS(407),
    [anon_sym_BANGmf_BANG] = ACTIONS(407),
    [anon_sym_BANGf_BANG] = ACTIONS(407),
    [anon_sym_BANGff_BANG] = ACTIONS(407),
    [anon_sym_BANGfff_BANG] = ACTIONS(407),
    [anon_sym_BANGffff_BANG] = ACTIONS(407),
    [anon_sym_BANGsfz_BANG] = ACTIONS(407),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(407),
    [anon_sym_BANGsegno_BANG] = ACTIONS(407),
    [anon_sym_BANGcoda_BANG] = ACTIONS(407),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(407),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(407),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(407),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(407),
    [anon_sym_BANGfine_BANG] = ACTIONS(407),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(407),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(407),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(407),
    [sym_parts_line] = ACTIONS(407),
    [sym_instruction] = ACTIONS(407),
    [sym_key] = ACTIONS(407),
    [sym_macros] = ACTIONS(407),
    [sym_meter] = ACTIONS(407),
    [sym_notes] = ACTIONS(407),
    [anon_sym_X_COLON] = ACTIONS(407),
    [sym_remark] = ACTIONS(407),
    [sym_rhythm_line] = ACTIONS(407),
    [anon_sym_s_COLON] = ACTIONS(407),
    [sym_tempo] = ACTIONS(407),
    [sym_tune_title] = ACTIONS(407),
    [sym_unit_note_length] = ACTIONS(407),
    [anon_sym_U_COLON] = ACTIONS(407),
    [sym_voice] = ACTIONS(407),
    [sym_words_tag] = ACTIONS(407),
    [sym_words_postbody] = ACTIONS(407),
  },
  [59] = {
    [ts_builtin_sym_end] = ACTIONS(277),
    [sym__NL] = ACTIONS(279),
    [anon_sym_BSLASH] = ACTIONS(277),
    [anon_sym_LBRACK] = ACTIONS(279),
    [sym_COMMENT] = ACTIONS(279),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(277),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(279),
    [sym_slur_open] = ACTIONS(279),
    [sym_slur_close] = ACTIONS(277),
    [sym_alteration] = ACTIONS(277),
    [sym_note_letter] = ACTIONS(277),
    [sym_rest] = ACTIONS(277),
    [anon_sym_Z] = ACTIONS(277),
    [anon_sym_X] = ACTIONS(279),
    [aux_sym_grace_note_token1] = ACTIONS(277),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(277),
    [sym_tuplet_marker] = ACTIONS(277),
    [sym_decoration_shorthand] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(277),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(277),
    [sym_start_of_repeated_section] = ACTIONS(277),
    [sym_end_of_repeated_section] = ACTIONS(277),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(277),
    [anon_sym_BANGtrill_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(277),
    [anon_sym_BANGmordent_BANG] = ACTIONS(277),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(277),
    [anon_sym_BANGroll_BANG] = ACTIONS(277),
    [anon_sym_BANGturn_BANG] = ACTIONS(277),
    [anon_sym_BANGturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(277),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_BANG] = ACTIONS(277),
    [anon_sym_BANGaccent_BANG] = ACTIONS(277),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(277),
    [anon_sym_BANGfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(277),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(277),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(277),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(277),
    [anon_sym_BANGplus_BANG] = ACTIONS(277),
    [anon_sym_BANGsnap_BANG] = ACTIONS(277),
    [anon_sym_BANGslide_BANG] = ACTIONS(277),
    [anon_sym_BANGwedge_BANG] = ACTIONS(277),
    [anon_sym_BANGupbow_BANG] = ACTIONS(277),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(277),
    [anon_sym_BANGopen_BANG] = ACTIONS(277),
    [anon_sym_BANGthumb_BANG] = ACTIONS(277),
    [anon_sym_BANGbreath_BANG] = ACTIONS(277),
    [anon_sym_BANGpppp_BANG] = ACTIONS(277),
    [anon_sym_BANGppp_BANG] = ACTIONS(277),
    [anon_sym_BANGpp_BANG] = ACTIONS(277),
    [anon_sym_BANGp_BANG] = ACTIONS(277),
    [anon_sym_BANGmp_BANG] = ACTIONS(277),
    [anon_sym_BANGmf_BANG] = ACTIONS(277),
    [anon_sym_BANGf_BANG] = ACTIONS(277),
    [anon_sym_BANGff_BANG] = ACTIONS(277),
    [anon_sym_BANGfff_BANG] = ACTIONS(277),
    [anon_sym_BANGffff_BANG] = ACTIONS(277),
    [anon_sym_BANGsfz_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(277),
    [anon_sym_BANGsegno_BANG] = ACTIONS(277),
    [anon_sym_BANGcoda_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(277),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(277),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(277),
    [anon_sym_BANGfine_BANG] = ACTIONS(277),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(277),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(277),
    [sym_parts_line] = ACTIONS(277),
    [sym_instruction] = ACTIONS(277),
    [sym_key] = ACTIONS(277),
    [sym_macros] = ACTIONS(277),
    [sym_meter] = ACTIONS(277),
    [sym_notes] = ACTIONS(277),
    [anon_sym_X_COLON] = ACTIONS(277),
    [sym_remark] = ACTIONS(277),
    [sym_rhythm_line] = ACTIONS(277),
    [anon_sym_s_COLON] = ACTIONS(277),
    [sym_tempo] = ACTIONS(277),
    [sym_tune_title] = ACTIONS(277),
    [sym_unit_note_length] = ACTIONS(277),
    [anon_sym_U_COLON] = ACTIONS(277),
    [sym_voice] = ACTIONS(277),
    [sym_words_tag] = ACTIONS(277),
    [sym_words_postbody] = ACTIONS(277),
  },
  [60] = {
    [ts_builtin_sym_end] = ACTIONS(411),
    [sym__NL] = ACTIONS(413),
    [anon_sym_BSLASH] = ACTIONS(411),
    [anon_sym_LBRACK] = ACTIONS(413),
    [sym_COMMENT] = ACTIONS(413),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(411),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(413),
    [sym_slur_open] = ACTIONS(413),
    [sym_slur_close] = ACTIONS(411),
    [sym_alteration] = ACTIONS(411),
    [sym_note_letter] = ACTIONS(411),
    [sym_rest] = ACTIONS(411),
    [anon_sym_Z] = ACTIONS(411),
    [anon_sym_X] = ACTIONS(413),
    [aux_sym_grace_note_token1] = ACTIONS(411),
    [aux_sym_chord_symbol_token1] = ACTIONS(413),
    [sym_annotation] = ACTIONS(411),
    [sym_tuplet_marker] = ACTIONS(411),
    [sym_decoration_shorthand] = ACTIONS(413),
    [sym_bar_line] = ACTIONS(413),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(411),
    [sym_thin_double_bar_line] = ACTIONS(411),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(411),
    [sym_start_of_repeated_section] = ACTIONS(411),
    [sym_end_of_repeated_section] = ACTIONS(411),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(411),
    [anon_sym_BANGtrill_BANG] = ACTIONS(411),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(411),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(411),
    [anon_sym_BANGmordent_BANG] = ACTIONS(411),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(411),
    [anon_sym_BANGroll_BANG] = ACTIONS(411),
    [anon_sym_BANGturn_BANG] = ACTIONS(411),
    [anon_sym_BANGturnx_BANG] = ACTIONS(411),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(411),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(411),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(411),
    [anon_sym_BANG_GT_BANG] = ACTIONS(411),
    [anon_sym_BANGaccent_BANG] = ACTIONS(411),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(411),
    [anon_sym_BANGfermata_BANG] = ACTIONS(411),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(411),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(411),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(411),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(411),
    [anon_sym_BANGplus_BANG] = ACTIONS(411),
    [anon_sym_BANGsnap_BANG] = ACTIONS(411),
    [anon_sym_BANGslide_BANG] = ACTIONS(411),
    [anon_sym_BANGwedge_BANG] = ACTIONS(411),
    [anon_sym_BANGupbow_BANG] = ACTIONS(411),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(411),
    [anon_sym_BANGopen_BANG] = ACTIONS(411),
    [anon_sym_BANGthumb_BANG] = ACTIONS(411),
    [anon_sym_BANGbreath_BANG] = ACTIONS(411),
    [anon_sym_BANGpppp_BANG] = ACTIONS(411),
    [anon_sym_BANGppp_BANG] = ACTIONS(411),
    [anon_sym_BANGpp_BANG] = ACTIONS(411),
    [anon_sym_BANGp_BANG] = ACTIONS(411),
    [anon_sym_BANGmp_BANG] = ACTIONS(411),
    [anon_sym_BANGmf_BANG] = ACTIONS(411),
    [anon_sym_BANGf_BANG] = ACTIONS(411),
    [anon_sym_BANGff_BANG] = ACTIONS(411),
    [anon_sym_BANGfff_BANG] = ACTIONS(411),
    [anon_sym_BANGffff_BANG] = ACTIONS(411),
    [anon_sym_BANGsfz_BANG] = ACTIONS(411),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(411),
    [anon_sym_BANGsegno_BANG] = ACTIONS(411),
    [anon_sym_BANGcoda_BANG] = ACTIONS(411),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(411),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(411),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(411),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(411),
    [anon_sym_BANGfine_BANG] = ACTIONS(411),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(411),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(411),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(411),
    [sym_parts_line] = ACTIONS(411),
    [sym_instruction] = ACTIONS(411),
    [sym_key] = ACTIONS(411),
    [sym_macros] = ACTIONS(411),
    [sym_meter] = ACTIONS(411),
    [sym_notes] = ACTIONS(411),
    [anon_sym_X_COLON] = ACTIONS(411),
    [sym_remark] = ACTIONS(411),
    [sym_rhythm_line] = ACTIONS(411),
    [anon_sym_s_COLON] = ACTIONS(411),
    [sym_tempo] = ACTIONS(411),
    [sym_tune_title] = ACTIONS(411),
    [sym_unit_note_length] = ACTIONS(411),
    [anon_sym_U_COLON] = ACTIONS(411),
    [sym_voice] = ACTIONS(411),
    [sym_words_tag] = ACTIONS(411),
    [sym_words_postbody] = ACTIONS(411),
  },
  [61] = {
    [ts_builtin_sym_end] = ACTIONS(415),
    [sym__NL] = ACTIONS(417),
    [anon_sym_BSLASH] = ACTIONS(415),
    [anon_sym_LBRACK] = ACTIONS(417),
    [sym_COMMENT] = ACTIONS(417),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(415),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(417),
    [sym_slur_open] = ACTIONS(417),
    [sym_slur_close] = ACTIONS(415),
    [sym_alteration] = ACTIONS(415),
    [sym_note_letter] = ACTIONS(415),
    [sym_rest] = ACTIONS(415),
    [anon_sym_Z] = ACTIONS(415),
    [anon_sym_X] = ACTIONS(417),
    [aux_sym_grace_note_token1] = ACTIONS(415),
    [aux_sym_chord_symbol_token1] = ACTIONS(417),
    [sym_annotation] = ACTIONS(415),
    [sym_tuplet_marker] = ACTIONS(415),
    [sym_decoration_shorthand] = ACTIONS(417),
    [sym_bar_line] = ACTIONS(417),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(415),
    [sym_thin_double_bar_line] = ACTIONS(415),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(415),
    [sym_start_of_repeated_section] = ACTIONS(415),
    [sym_end_of_repeated_section] = ACTIONS(415),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(415),
    [anon_sym_BANGtrill_BANG] = ACTIONS(415),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(415),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(415),
    [anon_sym_BANGmordent_BANG] = ACTIONS(415),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(415),
    [anon_sym_BANGroll_BANG] = ACTIONS(415),
    [anon_sym_BANGturn_BANG] = ACTIONS(415),
    [anon_sym_BANGturnx_BANG] = ACTIONS(415),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(415),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(415),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(415),
    [anon_sym_BANG_GT_BANG] = ACTIONS(415),
    [anon_sym_BANGaccent_BANG] = ACTIONS(415),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(415),
    [anon_sym_BANGfermata_BANG] = ACTIONS(415),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(415),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(415),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(415),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(415),
    [anon_sym_BANGplus_BANG] = ACTIONS(415),
    [anon_sym_BANGsnap_BANG] = ACTIONS(415),
    [anon_sym_BANGslide_BANG] = ACTIONS(415),
    [anon_sym_BANGwedge_BANG] = ACTIONS(415),
    [anon_sym_BANGupbow_BANG] = ACTIONS(415),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(415),
    [anon_sym_BANGopen_BANG] = ACTIONS(415),
    [anon_sym_BANGthumb_BANG] = ACTIONS(415),
    [anon_sym_BANGbreath_BANG] = ACTIONS(415),
    [anon_sym_BANGpppp_BANG] = ACTIONS(415),
    [anon_sym_BANGppp_BANG] = ACTIONS(415),
    [anon_sym_BANGpp_BANG] = ACTIONS(415),
    [anon_sym_BANGp_BANG] = ACTIONS(415),
    [anon_sym_BANGmp_BANG] = ACTIONS(415),
    [anon_sym_BANGmf_BANG] = ACTIONS(415),
    [anon_sym_BANGf_BANG] = ACTIONS(415),
    [anon_sym_BANGff_BANG] = ACTIONS(415),
    [anon_sym_BANGfff_BANG] = ACTIONS(415),
    [anon_sym_BANGffff_BANG] = ACTIONS(415),
    [anon_sym_BANGsfz_BANG] = ACTIONS(415),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(415),
    [anon_sym_BANGsegno_BANG] = ACTIONS(415),
    [anon_sym_BANGcoda_BANG] = ACTIONS(415),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(415),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(415),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(415),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(415),
    [anon_sym_BANGfine_BANG] = ACTIONS(415),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(415),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(415),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(415),
    [sym_parts_line] = ACTIONS(415),
    [sym_instruction] = ACTIONS(415),
    [sym_key] = ACTIONS(415),
    [sym_macros] = ACTIONS(415),
    [sym_meter] = ACTIONS(415),
    [sym_notes] = ACTIONS(415),
    [anon_sym_X_COLON] = ACTIONS(415),
    [sym_remark] = ACTIONS(415),
    [sym_rhythm_line] = ACTIONS(415),
    [anon_sym_s_COLON] = ACTIONS(415),
    [sym_tempo] = ACTIONS(415),
    [sym_tune_title] = ACTIONS(415),
    [sym_unit_note_length] = ACTIONS(415),
    [anon_sym_U_COLON] = ACTIONS(415),
    [sym_voice] = ACTIONS(415),
    [sym_words_tag] = ACTIONS(415),
    [sym_words_postbody] = ACTIONS(415),
  },
  [62] = {
    [ts_builtin_sym_end] = ACTIONS(419),
    [sym__NL] = ACTIONS(419),
    [anon_sym_BSLASH] = ACTIONS(419),
    [anon_sym_LBRACK] = ACTIONS(421),
    [sym_COMMENT] = ACTIONS(421),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(419),
    [sym_slur_open] = ACTIONS(421),
    [sym_slur_close] = ACTIONS(419),
    [sym_alteration] = ACTIONS(419),
    [sym_note_letter] = ACTIONS(419),
    [sym_rest] = ACTIONS(419),
    [anon_sym_Z] = ACTIONS(419),
    [anon_sym_X] = ACTIONS(421),
    [aux_sym_grace_note_token1] = ACTIONS(419),
    [aux_sym_chord_symbol_token1] = ACTIONS(421),
    [sym_annotation] = ACTIONS(419),
    [sym_tuplet_marker] = ACTIONS(419),
    [sym_decoration_shorthand] = ACTIONS(421),
    [sym_bar_line] = ACTIONS(421),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(419),
    [sym_thin_double_bar_line] = ACTIONS(419),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(419),
    [sym_start_of_repeated_section] = ACTIONS(419),
    [sym_end_of_repeated_section] = ACTIONS(419),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(419),
    [anon_sym_BANGtrill_BANG] = ACTIONS(419),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(419),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(419),
    [anon_sym_BANGmordent_BANG] = ACTIONS(419),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(419),
    [anon_sym_BANGroll_BANG] = ACTIONS(419),
    [anon_sym_BANGturn_BANG] = ACTIONS(419),
    [anon_sym_BANGturnx_BANG] = ACTIONS(419),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(419),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(419),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(419),
    [anon_sym_BANG_GT_BANG] = ACTIONS(419),
    [anon_sym_BANGaccent_BANG] = ACTIONS(419),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(419),
    [anon_sym_BANGfermata_BANG] = ACTIONS(419),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(419),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(419),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(419),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(419),
    [anon_sym_BANGplus_BANG] = ACTIONS(419),
    [anon_sym_BANGsnap_BANG] = ACTIONS(419),
    [anon_sym_BANGslide_BANG] = ACTIONS(419),
    [anon_sym_BANGwedge_BANG] = ACTIONS(419),
    [anon_sym_BANGupbow_BANG] = ACTIONS(419),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(419),
    [anon_sym_BANGopen_BANG] = ACTIONS(419),
    [anon_sym_BANGthumb_BANG] = ACTIONS(419),
    [anon_sym_BANGbreath_BANG] = ACTIONS(419),
    [anon_sym_BANGpppp_BANG] = ACTIONS(419),
    [anon_sym_BANGppp_BANG] = ACTIONS(419),
    [anon_sym_BANGpp_BANG] = ACTIONS(419),
    [anon_sym_BANGp_BANG] = ACTIONS(419),
    [anon_sym_BANGmp_BANG] = ACTIONS(419),
    [anon_sym_BANGmf_BANG] = ACTIONS(419),
    [anon_sym_BANGf_BANG] = ACTIONS(419),
    [anon_sym_BANGff_BANG] = ACTIONS(419),
    [anon_sym_BANGfff_BANG] = ACTIONS(419),
    [anon_sym_BANGffff_BANG] = ACTIONS(419),
    [anon_sym_BANGsfz_BANG] = ACTIONS(419),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(419),
    [anon_sym_BANGsegno_BANG] = ACTIONS(419),
    [anon_sym_BANGcoda_BANG] = ACTIONS(419),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(419),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(419),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(419),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(419),
    [anon_sym_BANGfine_BANG] = ACTIONS(419),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(419),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(419),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(419),
    [sym_parts_line] = ACTIONS(419),
    [sym_instruction] = ACTIONS(419),
    [sym_key] = ACTIONS(419),
    [sym_macros] = ACTIONS(419),
    [sym_meter] = ACTIONS(419),
    [sym_notes] = ACTIONS(419),
    [anon_sym_X_COLON] = ACTIONS(419),
    [sym_remark] = ACTIONS(419),
    [sym_rhythm_line] = ACTIONS(419),
    [anon_sym_s_COLON] = ACTIONS(419),
    [sym_tempo] = ACTIONS(419),
    [sym_tune_title] = ACTIONS(419),
    [sym_unit_note_length] = ACTIONS(419),
    [anon_sym_U_COLON] = ACTIONS(419),
    [sym_voice] = ACTIONS(419),
    [sym_words_tag] = ACTIONS(419),
    [sym_words_postbody] = ACTIONS(419),
  },
  [63] = {
    [ts_builtin_sym_end] = ACTIONS(59),
    [sym__NL] = ACTIONS(59),
    [anon_sym_BSLASH] = ACTIONS(59),
    [anon_sym_LBRACK] = ACTIONS(423),
    [sym_COMMENT] = ACTIONS(423),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(59),
    [sym_slur_open] = ACTIONS(423),
    [sym_slur_close] = ACTIONS(59),
    [sym_alteration] = ACTIONS(59),
    [sym_note_letter] = ACTIONS(59),
    [sym_rest] = ACTIONS(59),
    [anon_sym_Z] = ACTIONS(59),
    [anon_sym_X] = ACTIONS(423),
    [aux_sym_grace_note_token1] = ACTIONS(59),
    [aux_sym_chord_symbol_token1] = ACTIONS(423),
    [sym_annotation] = ACTIONS(59),
    [sym_tuplet_marker] = ACTIONS(59),
    [sym_decoration_shorthand] = ACTIONS(423),
    [sym_bar_line] = ACTIONS(423),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(59),
    [sym_thin_double_bar_line] = ACTIONS(59),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(59),
    [sym_start_of_repeated_section] = ACTIONS(59),
    [sym_end_of_repeated_section] = ACTIONS(59),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(59),
    [anon_sym_BANGtrill_BANG] = ACTIONS(59),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(59),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(59),
    [anon_sym_BANGmordent_BANG] = ACTIONS(59),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(59),
    [anon_sym_BANGroll_BANG] = ACTIONS(59),
    [anon_sym_BANGturn_BANG] = ACTIONS(59),
    [anon_sym_BANGturnx_BANG] = ACTIONS(59),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(59),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(59),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(59),
    [anon_sym_BANG_GT_BANG] = ACTIONS(59),
    [anon_sym_BANGaccent_BANG] = ACTIONS(59),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(59),
    [anon_sym_BANGfermata_BANG] = ACTIONS(59),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(59),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(59),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(59),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(59),
    [anon_sym_BANGplus_BANG] = ACTIONS(59),
    [anon_sym_BANGsnap_BANG] = ACTIONS(59),
    [anon_sym_BANGslide_BANG] = ACTIONS(59),
    [anon_sym_BANGwedge_BANG] = ACTIONS(59),
    [anon_sym_BANGupbow_BANG] = ACTIONS(59),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(59),
    [anon_sym_BANGopen_BANG] = ACTIONS(59),
    [anon_sym_BANGthumb_BANG] = ACTIONS(59),
    [anon_sym_BANGbreath_BANG] = ACTIONS(59),
    [anon_sym_BANGpppp_BANG] = ACTIONS(59),
    [anon_sym_BANGppp_BANG] = ACTIONS(59),
    [anon_sym_BANGpp_BANG] = ACTIONS(59),
    [anon_sym_BANGp_BANG] = ACTIONS(59),
    [anon_sym_BANGmp_BANG] = ACTIONS(59),
    [anon_sym_BANGmf_BANG] = ACTIONS(59),
    [anon_sym_BANGf_BANG] = ACTIONS(59),
    [anon_sym_BANGff_BANG] = ACTIONS(59),
    [anon_sym_BANGfff_BANG] = ACTIONS(59),
    [anon_sym_BANGffff_BANG] = ACTIONS(59),
    [anon_sym_BANGsfz_BANG] = ACTIONS(59),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(59),
    [anon_sym_BANGsegno_BANG] = ACTIONS(59),
    [anon_sym_BANGcoda_BANG] = ACTIONS(59),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(59),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(59),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(59),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(59),
    [anon_sym_BANGfine_BANG] = ACTIONS(59),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(59),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(59),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(59),
    [sym_parts_line] = ACTIONS(59),
    [sym_instruction] = ACTIONS(59),
    [sym_key] = ACTIONS(59),
    [sym_macros] = ACTIONS(59),
    [sym_meter] = ACTIONS(59),
    [sym_notes] = ACTIONS(59),
    [anon_sym_X_COLON] = ACTIONS(59),
    [sym_remark] = ACTIONS(59),
    [sym_rhythm_line] = ACTIONS(59),
    [anon_sym_s_COLON] = ACTIONS(59),
    [sym_tempo] = ACTIONS(59),
    [sym_tune_title] = ACTIONS(59),
    [sym_unit_note_length] = ACTIONS(59),
    [anon_sym_U_COLON] = ACTIONS(59),
    [sym_voice] = ACTIONS(59),
    [sym_words_tag] = ACTIONS(59),
    [sym_words_postbody] = ACTIONS(59),
  },
  [64] = {
    [ts_builtin_sym_end] = ACTIONS(425),
    [sym__NL] = ACTIONS(425),
    [anon_sym_BSLASH] = ACTIONS(425),
    [anon_sym_LBRACK] = ACTIONS(427),
    [sym_COMMENT] = ACTIONS(427),
    [anon_sym_PERCENT_PERCENT] = ACTIONS(425),
    [sym_slur_open] = ACTIONS(427),
    [sym_slur_close] = ACTIONS(425),
    [sym_alteration] = ACTIONS(425),
    [sym_note_letter] = ACTIONS(425),
    [sym_rest] = ACTIONS(425),
    [anon_sym_Z] = ACTIONS(425),
    [anon_sym_X] = ACTIONS(427),
    [aux_sym_grace_note_token1] = ACTIONS(425),
    [aux_sym_chord_symbol_token1] = ACTIONS(427),
    [sym_annotation] = ACTIONS(425),
    [sym_tuplet_marker] = ACTIONS(425),
    [sym_decoration_shorthand] = ACTIONS(427),
    [sym_bar_line] = ACTIONS(427),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(425),
    [sym_thin_double_bar_line] = ACTIONS(425),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(425),
    [sym_start_of_repeated_section] = ACTIONS(425),
    [sym_end_of_repeated_section] = ACTIONS(425),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(425),
    [anon_sym_BANGtrill_BANG] = ACTIONS(425),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(425),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(425),
    [anon_sym_BANGmordent_BANG] = ACTIONS(425),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(425),
    [anon_sym_BANGroll_BANG] = ACTIONS(425),
    [anon_sym_BANGturn_BANG] = ACTIONS(425),
    [anon_sym_BANGturnx_BANG] = ACTIONS(425),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(425),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(425),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(425),
    [anon_sym_BANG_GT_BANG] = ACTIONS(425),
    [anon_sym_BANGaccent_BANG] = ACTIONS(425),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(425),
    [anon_sym_BANGfermata_BANG] = ACTIONS(425),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(425),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(425),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(425),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(425),
    [anon_sym_BANGplus_BANG] = ACTIONS(425),
    [anon_sym_BANGsnap_BANG] = ACTIONS(425),
    [anon_sym_BANGslide_BANG] = ACTIONS(425),
    [anon_sym_BANGwedge_BANG] = ACTIONS(425),
    [anon_sym_BANGupbow_BANG] = ACTIONS(425),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(425),
    [anon_sym_BANGopen_BANG] = ACTIONS(425),
    [anon_sym_BANGthumb_BANG] = ACTIONS(425),
    [anon_sym_BANGbreath_BANG] = ACTIONS(425),
    [anon_sym_BANGpppp_BANG] = ACTIONS(425),
    [anon_sym_BANGppp_BANG] = ACTIONS(425),
    [anon_sym_BANGpp_BANG] = ACTIONS(425),
    [anon_sym_BANGp_BANG] = ACTIONS(425),
    [anon_sym_BANGmp_BANG] = ACTIONS(425),
    [anon_sym_BANGmf_BANG] = ACTIONS(425),
    [anon_sym_BANGf_BANG] = ACTIONS(425),
    [anon_sym_BANGff_BANG] = ACTIONS(425),
    [anon_sym_BANGfff_BANG] = ACTIONS(425),
    [anon_sym_BANGffff_BANG] = ACTIONS(425),
    [anon_sym_BANGsfz_BANG] = ACTIONS(425),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(425),
    [anon_sym_BANGsegno_BANG] = ACTIONS(425),
    [anon_sym_BANGcoda_BANG] = ACTIONS(425),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(425),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(425),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(425),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(425),
    [anon_sym_BANGfine_BANG] = ACTIONS(425),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(425),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(425),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(425),
    [sym_parts_line] = ACTIONS(425),
    [sym_instruction] = ACTIONS(425),
    [sym_key] = ACTIONS(425),
    [sym_macros] = ACTIONS(425),
    [sym_meter] = ACTIONS(425),
    [sym_notes] = ACTIONS(425),
    [anon_sym_X_COLON] = ACTIONS(425),
    [sym_remark] = ACTIONS(425),
    [sym_rhythm_line] = ACTIONS(425),
    [anon_sym_s_COLON] = ACTIONS(425),
    [sym_tempo] = ACTIONS(425),
    [sym_tune_title] = ACTIONS(425),
    [sym_unit_note_length] = ACTIONS(425),
    [anon_sym_U_COLON] = ACTIONS(425),
    [sym_voice] = ACTIONS(425),
    [sym_words_tag] = ACTIONS(425),
    [sym_words_postbody] = ACTIONS(425),
  },
  [65] = {
    [sym_chord_symbol] = STATE(65),
    [sym_nth_ending_barline] = STATE(86),
    [sym_generic_bar_line] = STATE(65),
    [sym_first_repeat_bar] = STATE(86),
    [sym_second_repeat_bar] = STATE(86),
    [sym_symbol] = STATE(65),
    [aux_sym_symbol_line_repeat1] = STATE(65),
    [sym_noCommentText] = ACTIONS(429),
    [sym_note_skip] = ACTIONS(431),
    [aux_sym_chord_symbol_token1] = ACTIONS(434),
    [sym_annotation] = ACTIONS(431),
    [sym_bar_line] = ACTIONS(437),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(440),
    [sym_thin_double_bar_line] = ACTIONS(440),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(440),
    [sym_start_of_repeated_section] = ACTIONS(443),
    [sym_end_of_repeated_section] = ACTIONS(446),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(443),
    [anon_sym_BANGtrill_BANG] = ACTIONS(449),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(449),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(449),
    [anon_sym_BANGmordent_BANG] = ACTIONS(449),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(449),
    [anon_sym_BANGroll_BANG] = ACTIONS(449),
    [anon_sym_BANGturn_BANG] = ACTIONS(449),
    [anon_sym_BANGturnx_BANG] = ACTIONS(449),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(449),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(449),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(449),
    [anon_sym_BANG_GT_BANG] = ACTIONS(449),
    [anon_sym_BANGaccent_BANG] = ACTIONS(449),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(449),
    [anon_sym_BANGfermata_BANG] = ACTIONS(449),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(449),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(449),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(449),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(449),
    [anon_sym_BANGplus_BANG] = ACTIONS(449),
    [anon_sym_BANGsnap_BANG] = ACTIONS(449),
    [anon_sym_BANGslide_BANG] = ACTIONS(449),
    [anon_sym_BANGwedge_BANG] = ACTIONS(449),
    [anon_sym_BANGupbow_BANG] = ACTIONS(449),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(449),
    [anon_sym_BANGopen_BANG] = ACTIONS(449),
    [anon_sym_BANGthumb_BANG] = ACTIONS(449),
    [anon_sym_BANGbreath_BANG] = ACTIONS(449),
    [anon_sym_BANGpppp_BANG] = ACTIONS(449),
    [anon_sym_BANGppp_BANG] = ACTIONS(449),
    [anon_sym_BANGpp_BANG] = ACTIONS(449),
    [anon_sym_BANGp_BANG] = ACTIONS(449),
    [anon_sym_BANGmp_BANG] = ACTIONS(449),
    [anon_sym_BANGmf_BANG] = ACTIONS(449),
    [anon_sym_BANGf_BANG] = ACTIONS(449),
    [anon_sym_BANGff_BANG] = ACTIONS(449),
    [anon_sym_BANGfff_BANG] = ACTIONS(449),
    [anon_sym_BANGffff_BANG] = ACTIONS(449),
    [anon_sym_BANGsfz_BANG] = ACTIONS(449),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(449),
    [anon_sym_BANGsegno_BANG] = ACTIONS(449),
    [anon_sym_BANGcoda_BANG] = ACTIONS(449),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(449),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(449),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(449),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(449),
    [anon_sym_BANGfine_BANG] = ACTIONS(449),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(449),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(449),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(449),
  },
  [66] = {
    [sym_chord_symbol] = STATE(66),
    [sym_nth_ending_barline] = STATE(89),
    [sym_generic_bar_line] = STATE(66),
    [sym_first_repeat_bar] = STATE(89),
    [sym_second_repeat_bar] = STATE(89),
    [sym_symbol] = STATE(66),
    [aux_sym_symbol_line_repeat1] = STATE(66),
    [sym_note_skip] = ACTIONS(452),
    [aux_sym_chord_symbol_token1] = ACTIONS(455),
    [sym_annotation] = ACTIONS(452),
    [sym_bar_line] = ACTIONS(458),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(461),
    [sym_thin_double_bar_line] = ACTIONS(464),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(464),
    [sym_start_of_repeated_section] = ACTIONS(467),
    [sym_end_of_repeated_section] = ACTIONS(470),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(467),
    [anon_sym_BANGtrill_BANG] = ACTIONS(473),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(473),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(473),
    [anon_sym_BANGmordent_BANG] = ACTIONS(473),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(473),
    [anon_sym_BANGroll_BANG] = ACTIONS(473),
    [anon_sym_BANGturn_BANG] = ACTIONS(473),
    [anon_sym_BANGturnx_BANG] = ACTIONS(473),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(473),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(473),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(473),
    [anon_sym_BANG_GT_BANG] = ACTIONS(473),
    [anon_sym_BANGaccent_BANG] = ACTIONS(473),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(473),
    [anon_sym_BANGfermata_BANG] = ACTIONS(473),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(473),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(473),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(473),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(473),
    [anon_sym_BANGplus_BANG] = ACTIONS(473),
    [anon_sym_BANGsnap_BANG] = ACTIONS(473),
    [anon_sym_BANGslide_BANG] = ACTIONS(473),
    [anon_sym_BANGwedge_BANG] = ACTIONS(473),
    [anon_sym_BANGupbow_BANG] = ACTIONS(473),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(473),
    [anon_sym_BANGopen_BANG] = ACTIONS(473),
    [anon_sym_BANGthumb_BANG] = ACTIONS(473),
    [anon_sym_BANGbreath_BANG] = ACTIONS(473),
    [anon_sym_BANGpppp_BANG] = ACTIONS(473),
    [anon_sym_BANGppp_BANG] = ACTIONS(473),
    [anon_sym_BANGpp_BANG] = ACTIONS(473),
    [anon_sym_BANGp_BANG] = ACTIONS(473),
    [anon_sym_BANGmp_BANG] = ACTIONS(473),
    [anon_sym_BANGmf_BANG] = ACTIONS(473),
    [anon_sym_BANGf_BANG] = ACTIONS(473),
    [anon_sym_BANGff_BANG] = ACTIONS(473),
    [anon_sym_BANGfff_BANG] = ACTIONS(473),
    [anon_sym_BANGffff_BANG] = ACTIONS(473),
    [anon_sym_BANGsfz_BANG] = ACTIONS(473),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(473),
    [anon_sym_BANGsegno_BANG] = ACTIONS(473),
    [anon_sym_BANGcoda_BANG] = ACTIONS(473),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(473),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(473),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(473),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(473),
    [anon_sym_BANGfine_BANG] = ACTIONS(473),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(473),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(473),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(473),
    [aux_sym_body_inline_info_token1] = ACTIONS(429),
  },
  [67] = {
    [sym_chord_symbol] = STATE(66),
    [sym_nth_ending_barline] = STATE(89),
    [sym_generic_bar_line] = STATE(66),
    [sym_first_repeat_bar] = STATE(89),
    [sym_second_repeat_bar] = STATE(89),
    [sym_symbol] = STATE(66),
    [aux_sym_symbol_line_repeat1] = STATE(66),
    [sym_note_skip] = ACTIONS(476),
    [aux_sym_chord_symbol_token1] = ACTIONS(478),
    [sym_annotation] = ACTIONS(476),
    [sym_bar_line] = ACTIONS(480),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(482),
    [sym_thin_double_bar_line] = ACTIONS(484),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(484),
    [sym_start_of_repeated_section] = ACTIONS(486),
    [sym_end_of_repeated_section] = ACTIONS(488),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(486),
    [anon_sym_BANGtrill_BANG] = ACTIONS(490),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(490),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(490),
    [anon_sym_BANGmordent_BANG] = ACTIONS(490),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(490),
    [anon_sym_BANGroll_BANG] = ACTIONS(490),
    [anon_sym_BANGturn_BANG] = ACTIONS(490),
    [anon_sym_BANGturnx_BANG] = ACTIONS(490),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(490),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(490),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(490),
    [anon_sym_BANG_GT_BANG] = ACTIONS(490),
    [anon_sym_BANGaccent_BANG] = ACTIONS(490),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(490),
    [anon_sym_BANGfermata_BANG] = ACTIONS(490),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(490),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(490),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(490),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(490),
    [anon_sym_BANGplus_BANG] = ACTIONS(490),
    [anon_sym_BANGsnap_BANG] = ACTIONS(490),
    [anon_sym_BANGslide_BANG] = ACTIONS(490),
    [anon_sym_BANGwedge_BANG] = ACTIONS(490),
    [anon_sym_BANGupbow_BANG] = ACTIONS(490),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(490),
    [anon_sym_BANGopen_BANG] = ACTIONS(490),
    [anon_sym_BANGthumb_BANG] = ACTIONS(490),
    [anon_sym_BANGbreath_BANG] = ACTIONS(490),
    [anon_sym_BANGpppp_BANG] = ACTIONS(490),
    [anon_sym_BANGppp_BANG] = ACTIONS(490),
    [anon_sym_BANGpp_BANG] = ACTIONS(490),
    [anon_sym_BANGp_BANG] = ACTIONS(490),
    [anon_sym_BANGmp_BANG] = ACTIONS(490),
    [anon_sym_BANGmf_BANG] = ACTIONS(490),
    [anon_sym_BANGf_BANG] = ACTIONS(490),
    [anon_sym_BANGff_BANG] = ACTIONS(490),
    [anon_sym_BANGfff_BANG] = ACTIONS(490),
    [anon_sym_BANGffff_BANG] = ACTIONS(490),
    [anon_sym_BANGsfz_BANG] = ACTIONS(490),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(490),
    [anon_sym_BANGsegno_BANG] = ACTIONS(490),
    [anon_sym_BANGcoda_BANG] = ACTIONS(490),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(490),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(490),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(490),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(490),
    [anon_sym_BANGfine_BANG] = ACTIONS(490),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(490),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(490),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(490),
    [aux_sym_body_inline_info_token1] = ACTIONS(492),
  },
  [68] = {
    [sym_chord_symbol] = STATE(65),
    [sym_nth_ending_barline] = STATE(86),
    [sym_generic_bar_line] = STATE(65),
    [sym_first_repeat_bar] = STATE(86),
    [sym_second_repeat_bar] = STATE(86),
    [sym_symbol] = STATE(65),
    [aux_sym_symbol_line_repeat1] = STATE(65),
    [sym_noCommentText] = ACTIONS(492),
    [sym_note_skip] = ACTIONS(494),
    [aux_sym_chord_symbol_token1] = ACTIONS(496),
    [sym_annotation] = ACTIONS(494),
    [sym_bar_line] = ACTIONS(498),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(500),
    [sym_thin_double_bar_line] = ACTIONS(500),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(500),
    [sym_start_of_repeated_section] = ACTIONS(502),
    [sym_end_of_repeated_section] = ACTIONS(504),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(502),
    [anon_sym_BANGtrill_BANG] = ACTIONS(506),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(506),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(506),
    [anon_sym_BANGmordent_BANG] = ACTIONS(506),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(506),
    [anon_sym_BANGroll_BANG] = ACTIONS(506),
    [anon_sym_BANGturn_BANG] = ACTIONS(506),
    [anon_sym_BANGturnx_BANG] = ACTIONS(506),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(506),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(506),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(506),
    [anon_sym_BANG_GT_BANG] = ACTIONS(506),
    [anon_sym_BANGaccent_BANG] = ACTIONS(506),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(506),
    [anon_sym_BANGfermata_BANG] = ACTIONS(506),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(506),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(506),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(506),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(506),
    [anon_sym_BANGplus_BANG] = ACTIONS(506),
    [anon_sym_BANGsnap_BANG] = ACTIONS(506),
    [anon_sym_BANGslide_BANG] = ACTIONS(506),
    [anon_sym_BANGwedge_BANG] = ACTIONS(506),
    [anon_sym_BANGupbow_BANG] = ACTIONS(506),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(506),
    [anon_sym_BANGopen_BANG] = ACTIONS(506),
    [anon_sym_BANGthumb_BANG] = ACTIONS(506),
    [anon_sym_BANGbreath_BANG] = ACTIONS(506),
    [anon_sym_BANGpppp_BANG] = ACTIONS(506),
    [anon_sym_BANGppp_BANG] = ACTIONS(506),
    [anon_sym_BANGpp_BANG] = ACTIONS(506),
    [anon_sym_BANGp_BANG] = ACTIONS(506),
    [anon_sym_BANGmp_BANG] = ACTIONS(506),
    [anon_sym_BANGmf_BANG] = ACTIONS(506),
    [anon_sym_BANGf_BANG] = ACTIONS(506),
    [anon_sym_BANGff_BANG] = ACTIONS(506),
    [anon_sym_BANGfff_BANG] = ACTIONS(506),
    [anon_sym_BANGffff_BANG] = ACTIONS(506),
    [anon_sym_BANGsfz_BANG] = ACTIONS(506),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(506),
    [anon_sym_BANGsegno_BANG] = ACTIONS(506),
    [anon_sym_BANGcoda_BANG] = ACTIONS(506),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(506),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(506),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(506),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(506),
    [anon_sym_BANGfine_BANG] = ACTIONS(506),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(506),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(506),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(506),
  },
  [69] = {
    [sym_chord_symbol] = STATE(67),
    [sym_nth_ending_barline] = STATE(89),
    [sym_generic_bar_line] = STATE(67),
    [sym_first_repeat_bar] = STATE(89),
    [sym_second_repeat_bar] = STATE(89),
    [sym_symbol] = STATE(67),
    [aux_sym_symbol_line_repeat1] = STATE(67),
    [sym_note_skip] = ACTIONS(508),
    [aux_sym_chord_symbol_token1] = ACTIONS(478),
    [sym_annotation] = ACTIONS(508),
    [sym_bar_line] = ACTIONS(480),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(482),
    [sym_thin_double_bar_line] = ACTIONS(482),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(482),
    [sym_start_of_repeated_section] = ACTIONS(510),
    [sym_end_of_repeated_section] = ACTIONS(512),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(510),
    [anon_sym_BANGtrill_BANG] = ACTIONS(514),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(514),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(514),
    [anon_sym_BANGmordent_BANG] = ACTIONS(514),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(514),
    [anon_sym_BANGroll_BANG] = ACTIONS(514),
    [anon_sym_BANGturn_BANG] = ACTIONS(514),
    [anon_sym_BANGturnx_BANG] = ACTIONS(514),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(514),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(514),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(514),
    [anon_sym_BANG_GT_BANG] = ACTIONS(514),
    [anon_sym_BANGaccent_BANG] = ACTIONS(514),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(514),
    [anon_sym_BANGfermata_BANG] = ACTIONS(514),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(514),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(514),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(514),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(514),
    [anon_sym_BANGplus_BANG] = ACTIONS(514),
    [anon_sym_BANGsnap_BANG] = ACTIONS(514),
    [anon_sym_BANGslide_BANG] = ACTIONS(514),
    [anon_sym_BANGwedge_BANG] = ACTIONS(514),
    [anon_sym_BANGupbow_BANG] = ACTIONS(514),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(514),
    [anon_sym_BANGopen_BANG] = ACTIONS(514),
    [anon_sym_BANGthumb_BANG] = ACTIONS(514),
    [anon_sym_BANGbreath_BANG] = ACTIONS(514),
    [anon_sym_BANGpppp_BANG] = ACTIONS(514),
    [anon_sym_BANGppp_BANG] = ACTIONS(514),
    [anon_sym_BANGpp_BANG] = ACTIONS(514),
    [anon_sym_BANGp_BANG] = ACTIONS(514),
    [anon_sym_BANGmp_BANG] = ACTIONS(514),
    [anon_sym_BANGmf_BANG] = ACTIONS(514),
    [anon_sym_BANGf_BANG] = ACTIONS(514),
    [anon_sym_BANGff_BANG] = ACTIONS(514),
    [anon_sym_BANGfff_BANG] = ACTIONS(514),
    [anon_sym_BANGffff_BANG] = ACTIONS(514),
    [anon_sym_BANGsfz_BANG] = ACTIONS(514),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(514),
    [anon_sym_BANGsegno_BANG] = ACTIONS(514),
    [anon_sym_BANGcoda_BANG] = ACTIONS(514),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(514),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(514),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(514),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(514),
    [anon_sym_BANGfine_BANG] = ACTIONS(514),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(514),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(514),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(514),
  },
  [70] = {
    [sym_chord_symbol] = STATE(68),
    [sym_nth_ending_barline] = STATE(86),
    [sym_generic_bar_line] = STATE(68),
    [sym_first_repeat_bar] = STATE(86),
    [sym_second_repeat_bar] = STATE(86),
    [sym_symbol] = STATE(68),
    [aux_sym_symbol_line_repeat1] = STATE(68),
    [sym_note_skip] = ACTIONS(516),
    [aux_sym_chord_symbol_token1] = ACTIONS(496),
    [sym_annotation] = ACTIONS(516),
    [sym_bar_line] = ACTIONS(498),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(518),
    [sym_thin_double_bar_line] = ACTIONS(518),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(518),
    [sym_start_of_repeated_section] = ACTIONS(520),
    [sym_end_of_repeated_section] = ACTIONS(522),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(520),
    [anon_sym_BANGtrill_BANG] = ACTIONS(524),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(524),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(524),
    [anon_sym_BANGmordent_BANG] = ACTIONS(524),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(524),
    [anon_sym_BANGroll_BANG] = ACTIONS(524),
    [anon_sym_BANGturn_BANG] = ACTIONS(524),
    [anon_sym_BANGturnx_BANG] = ACTIONS(524),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(524),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(524),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(524),
    [anon_sym_BANG_GT_BANG] = ACTIONS(524),
    [anon_sym_BANGaccent_BANG] = ACTIONS(524),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(524),
    [anon_sym_BANGfermata_BANG] = ACTIONS(524),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(524),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(524),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(524),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(524),
    [anon_sym_BANGplus_BANG] = ACTIONS(524),
    [anon_sym_BANGsnap_BANG] = ACTIONS(524),
    [anon_sym_BANGslide_BANG] = ACTIONS(524),
    [anon_sym_BANGwedge_BANG] = ACTIONS(524),
    [anon_sym_BANGupbow_BANG] = ACTIONS(524),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(524),
    [anon_sym_BANGopen_BANG] = ACTIONS(524),
    [anon_sym_BANGthumb_BANG] = ACTIONS(524),
    [anon_sym_BANGbreath_BANG] = ACTIONS(524),
    [anon_sym_BANGpppp_BANG] = ACTIONS(524),
    [anon_sym_BANGppp_BANG] = ACTIONS(524),
    [anon_sym_BANGpp_BANG] = ACTIONS(524),
    [anon_sym_BANGp_BANG] = ACTIONS(524),
    [anon_sym_BANGmp_BANG] = ACTIONS(524),
    [anon_sym_BANGmf_BANG] = ACTIONS(524),
    [anon_sym_BANGf_BANG] = ACTIONS(524),
    [anon_sym_BANGff_BANG] = ACTIONS(524),
    [anon_sym_BANGfff_BANG] = ACTIONS(524),
    [anon_sym_BANGffff_BANG] = ACTIONS(524),
    [anon_sym_BANGsfz_BANG] = ACTIONS(524),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(524),
    [anon_sym_BANGsegno_BANG] = ACTIONS(524),
    [anon_sym_BANGcoda_BANG] = ACTIONS(524),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(524),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(524),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(524),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(524),
    [anon_sym_BANGfine_BANG] = ACTIONS(524),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(524),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(524),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(524),
  },
  [71] = {
    [sym_nth_ending_number] = STATE(96),
    [sym_noCommentText] = ACTIONS(279),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(526),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(528),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(279),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(530),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(532),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
  },
  [72] = {
    [sym_nth_ending_number] = STATE(96),
    [sym_noCommentText] = ACTIONS(279),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(526),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(528),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(279),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(534),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(536),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
  },
  [73] = {
    [sym_nth_ending_number] = STATE(94),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(538),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(540),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(542),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(544),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
    [aux_sym_body_inline_info_token1] = ACTIONS(279),
  },
  [74] = {
    [sym_nth_ending_number] = STATE(94),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(538),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(540),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [aux_sym_first_repeat_bar_token1] = ACTIONS(546),
    [aux_sym_first_repeat_bar_token2] = ACTIONS(548),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
    [aux_sym_body_inline_info_token1] = ACTIONS(279),
  },
  [75] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(75),
    [sym_note_skip] = ACTIONS(266),
    [anon_sym_DASH] = ACTIONS(550),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(550),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(264),
    [sym_thin_double_bar_line] = ACTIONS(266),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(266),
    [sym_start_of_repeated_section] = ACTIONS(266),
    [sym_end_of_repeated_section] = ACTIONS(266),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(266),
    [anon_sym_BANGtrill_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGmordent_BANG] = ACTIONS(266),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(266),
    [anon_sym_BANGroll_BANG] = ACTIONS(266),
    [anon_sym_BANGturn_BANG] = ACTIONS(266),
    [anon_sym_BANGturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_BANG] = ACTIONS(266),
    [anon_sym_BANGaccent_BANG] = ACTIONS(266),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(266),
    [anon_sym_BANGfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(266),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(266),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(266),
    [anon_sym_BANGplus_BANG] = ACTIONS(266),
    [anon_sym_BANGsnap_BANG] = ACTIONS(266),
    [anon_sym_BANGslide_BANG] = ACTIONS(266),
    [anon_sym_BANGwedge_BANG] = ACTIONS(266),
    [anon_sym_BANGupbow_BANG] = ACTIONS(266),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(266),
    [anon_sym_BANGopen_BANG] = ACTIONS(266),
    [anon_sym_BANGthumb_BANG] = ACTIONS(266),
    [anon_sym_BANGbreath_BANG] = ACTIONS(266),
    [anon_sym_BANGpppp_BANG] = ACTIONS(266),
    [anon_sym_BANGppp_BANG] = ACTIONS(266),
    [anon_sym_BANGpp_BANG] = ACTIONS(266),
    [anon_sym_BANGp_BANG] = ACTIONS(266),
    [anon_sym_BANGmp_BANG] = ACTIONS(266),
    [anon_sym_BANGmf_BANG] = ACTIONS(266),
    [anon_sym_BANGf_BANG] = ACTIONS(266),
    [anon_sym_BANGff_BANG] = ACTIONS(266),
    [anon_sym_BANGfff_BANG] = ACTIONS(266),
    [anon_sym_BANGffff_BANG] = ACTIONS(266),
    [anon_sym_BANGsfz_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGsegno_BANG] = ACTIONS(266),
    [anon_sym_BANGcoda_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(266),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(266),
    [anon_sym_BANGfine_BANG] = ACTIONS(266),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(266),
    [aux_sym_body_inline_info_token1] = ACTIONS(266),
  },
  [76] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(75),
    [sym_note_skip] = ACTIONS(273),
    [anon_sym_DASH] = ACTIONS(553),
    [aux_sym_chord_symbol_token1] = ACTIONS(273),
    [sym_annotation] = ACTIONS(273),
    [anon_sym_COMMA] = ACTIONS(553),
    [sym_bar_line] = ACTIONS(273),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(271),
    [sym_thin_double_bar_line] = ACTIONS(273),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(273),
    [sym_start_of_repeated_section] = ACTIONS(273),
    [sym_end_of_repeated_section] = ACTIONS(273),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(273),
    [anon_sym_BANGtrill_BANG] = ACTIONS(273),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(273),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(273),
    [anon_sym_BANGmordent_BANG] = ACTIONS(273),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(273),
    [anon_sym_BANGroll_BANG] = ACTIONS(273),
    [anon_sym_BANGturn_BANG] = ACTIONS(273),
    [anon_sym_BANGturnx_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(273),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_BANG] = ACTIONS(273),
    [anon_sym_BANGaccent_BANG] = ACTIONS(273),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(273),
    [anon_sym_BANGfermata_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(273),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(273),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(273),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(273),
    [anon_sym_BANGplus_BANG] = ACTIONS(273),
    [anon_sym_BANGsnap_BANG] = ACTIONS(273),
    [anon_sym_BANGslide_BANG] = ACTIONS(273),
    [anon_sym_BANGwedge_BANG] = ACTIONS(273),
    [anon_sym_BANGupbow_BANG] = ACTIONS(273),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(273),
    [anon_sym_BANGopen_BANG] = ACTIONS(273),
    [anon_sym_BANGthumb_BANG] = ACTIONS(273),
    [anon_sym_BANGbreath_BANG] = ACTIONS(273),
    [anon_sym_BANGpppp_BANG] = ACTIONS(273),
    [anon_sym_BANGppp_BANG] = ACTIONS(273),
    [anon_sym_BANGpp_BANG] = ACTIONS(273),
    [anon_sym_BANGp_BANG] = ACTIONS(273),
    [anon_sym_BANGmp_BANG] = ACTIONS(273),
    [anon_sym_BANGmf_BANG] = ACTIONS(273),
    [anon_sym_BANGf_BANG] = ACTIONS(273),
    [anon_sym_BANGff_BANG] = ACTIONS(273),
    [anon_sym_BANGfff_BANG] = ACTIONS(273),
    [anon_sym_BANGffff_BANG] = ACTIONS(273),
    [anon_sym_BANGsfz_BANG] = ACTIONS(273),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGsegno_BANG] = ACTIONS(273),
    [anon_sym_BANGcoda_BANG] = ACTIONS(273),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(273),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(273),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(273),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(273),
    [anon_sym_BANGfine_BANG] = ACTIONS(273),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(273),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(273),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(273),
    [aux_sym_body_inline_info_token1] = ACTIONS(273),
  },
  [77] = {
    [sym_nth_ending_number] = STATE(96),
    [sym_noCommentText] = ACTIONS(279),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(526),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(528),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(279),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
  },
  [78] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(81),
    [sym_noCommentText] = ACTIONS(295),
    [sym_note_skip] = ACTIONS(295),
    [anon_sym_DASH] = ACTIONS(555),
    [aux_sym_chord_symbol_token1] = ACTIONS(295),
    [sym_annotation] = ACTIONS(295),
    [anon_sym_COMMA] = ACTIONS(555),
    [sym_bar_line] = ACTIONS(295),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(295),
    [sym_thin_double_bar_line] = ACTIONS(295),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(295),
    [sym_start_of_repeated_section] = ACTIONS(295),
    [sym_end_of_repeated_section] = ACTIONS(295),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(295),
    [anon_sym_BANGtrill_BANG] = ACTIONS(295),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(295),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(295),
    [anon_sym_BANGmordent_BANG] = ACTIONS(295),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(295),
    [anon_sym_BANGroll_BANG] = ACTIONS(295),
    [anon_sym_BANGturn_BANG] = ACTIONS(295),
    [anon_sym_BANGturnx_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(295),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_BANG] = ACTIONS(295),
    [anon_sym_BANGaccent_BANG] = ACTIONS(295),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(295),
    [anon_sym_BANGfermata_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(295),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(295),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(295),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(295),
    [anon_sym_BANGplus_BANG] = ACTIONS(295),
    [anon_sym_BANGsnap_BANG] = ACTIONS(295),
    [anon_sym_BANGslide_BANG] = ACTIONS(295),
    [anon_sym_BANGwedge_BANG] = ACTIONS(295),
    [anon_sym_BANGupbow_BANG] = ACTIONS(295),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(295),
    [anon_sym_BANGopen_BANG] = ACTIONS(295),
    [anon_sym_BANGthumb_BANG] = ACTIONS(295),
    [anon_sym_BANGbreath_BANG] = ACTIONS(295),
    [anon_sym_BANGpppp_BANG] = ACTIONS(295),
    [anon_sym_BANGppp_BANG] = ACTIONS(295),
    [anon_sym_BANGpp_BANG] = ACTIONS(295),
    [anon_sym_BANGp_BANG] = ACTIONS(295),
    [anon_sym_BANGmp_BANG] = ACTIONS(295),
    [anon_sym_BANGmf_BANG] = ACTIONS(295),
    [anon_sym_BANGf_BANG] = ACTIONS(295),
    [anon_sym_BANGff_BANG] = ACTIONS(295),
    [anon_sym_BANGfff_BANG] = ACTIONS(295),
    [anon_sym_BANGffff_BANG] = ACTIONS(295),
    [anon_sym_BANGsfz_BANG] = ACTIONS(295),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGsegno_BANG] = ACTIONS(295),
    [anon_sym_BANGcoda_BANG] = ACTIONS(295),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(295),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(295),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(295),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(295),
    [anon_sym_BANGfine_BANG] = ACTIONS(295),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(295),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(295),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(295),
  },
  [79] = {
    [sym_nth_ending_number] = STATE(94),
    [sym_note_skip] = ACTIONS(279),
    [anon_sym_LBRACK] = ACTIONS(538),
    [aux_sym__MUSIC_CODE_token1] = ACTIONS(540),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
    [aux_sym_body_inline_info_token1] = ACTIONS(279),
  },
  [80] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(80),
    [sym_noCommentText] = ACTIONS(266),
    [sym_note_skip] = ACTIONS(266),
    [anon_sym_DASH] = ACTIONS(557),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(557),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(266),
    [sym_thin_double_bar_line] = ACTIONS(266),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(266),
    [sym_start_of_repeated_section] = ACTIONS(266),
    [sym_end_of_repeated_section] = ACTIONS(266),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(266),
    [anon_sym_BANGtrill_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGmordent_BANG] = ACTIONS(266),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(266),
    [anon_sym_BANGroll_BANG] = ACTIONS(266),
    [anon_sym_BANGturn_BANG] = ACTIONS(266),
    [anon_sym_BANGturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_BANG] = ACTIONS(266),
    [anon_sym_BANGaccent_BANG] = ACTIONS(266),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(266),
    [anon_sym_BANGfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(266),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(266),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(266),
    [anon_sym_BANGplus_BANG] = ACTIONS(266),
    [anon_sym_BANGsnap_BANG] = ACTIONS(266),
    [anon_sym_BANGslide_BANG] = ACTIONS(266),
    [anon_sym_BANGwedge_BANG] = ACTIONS(266),
    [anon_sym_BANGupbow_BANG] = ACTIONS(266),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(266),
    [anon_sym_BANGopen_BANG] = ACTIONS(266),
    [anon_sym_BANGthumb_BANG] = ACTIONS(266),
    [anon_sym_BANGbreath_BANG] = ACTIONS(266),
    [anon_sym_BANGpppp_BANG] = ACTIONS(266),
    [anon_sym_BANGppp_BANG] = ACTIONS(266),
    [anon_sym_BANGpp_BANG] = ACTIONS(266),
    [anon_sym_BANGp_BANG] = ACTIONS(266),
    [anon_sym_BANGmp_BANG] = ACTIONS(266),
    [anon_sym_BANGmf_BANG] = ACTIONS(266),
    [anon_sym_BANGf_BANG] = ACTIONS(266),
    [anon_sym_BANGff_BANG] = ACTIONS(266),
    [anon_sym_BANGfff_BANG] = ACTIONS(266),
    [anon_sym_BANGffff_BANG] = ACTIONS(266),
    [anon_sym_BANGsfz_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGsegno_BANG] = ACTIONS(266),
    [anon_sym_BANGcoda_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(266),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(266),
    [anon_sym_BANGfine_BANG] = ACTIONS(266),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(266),
  },
  [81] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(80),
    [sym_noCommentText] = ACTIONS(273),
    [sym_note_skip] = ACTIONS(273),
    [anon_sym_DASH] = ACTIONS(555),
    [aux_sym_chord_symbol_token1] = ACTIONS(273),
    [sym_annotation] = ACTIONS(273),
    [anon_sym_COMMA] = ACTIONS(555),
    [sym_bar_line] = ACTIONS(273),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(273),
    [sym_thin_double_bar_line] = ACTIONS(273),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(273),
    [sym_start_of_repeated_section] = ACTIONS(273),
    [sym_end_of_repeated_section] = ACTIONS(273),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(273),
    [anon_sym_BANGtrill_BANG] = ACTIONS(273),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(273),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(273),
    [anon_sym_BANGmordent_BANG] = ACTIONS(273),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(273),
    [anon_sym_BANGroll_BANG] = ACTIONS(273),
    [anon_sym_BANGturn_BANG] = ACTIONS(273),
    [anon_sym_BANGturnx_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(273),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_BANG] = ACTIONS(273),
    [anon_sym_BANGaccent_BANG] = ACTIONS(273),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(273),
    [anon_sym_BANGfermata_BANG] = ACTIONS(273),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(273),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(273),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(273),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(273),
    [anon_sym_BANGplus_BANG] = ACTIONS(273),
    [anon_sym_BANGsnap_BANG] = ACTIONS(273),
    [anon_sym_BANGslide_BANG] = ACTIONS(273),
    [anon_sym_BANGwedge_BANG] = ACTIONS(273),
    [anon_sym_BANGupbow_BANG] = ACTIONS(273),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(273),
    [anon_sym_BANGopen_BANG] = ACTIONS(273),
    [anon_sym_BANGthumb_BANG] = ACTIONS(273),
    [anon_sym_BANGbreath_BANG] = ACTIONS(273),
    [anon_sym_BANGpppp_BANG] = ACTIONS(273),
    [anon_sym_BANGppp_BANG] = ACTIONS(273),
    [anon_sym_BANGpp_BANG] = ACTIONS(273),
    [anon_sym_BANGp_BANG] = ACTIONS(273),
    [anon_sym_BANGmp_BANG] = ACTIONS(273),
    [anon_sym_BANGmf_BANG] = ACTIONS(273),
    [anon_sym_BANGf_BANG] = ACTIONS(273),
    [anon_sym_BANGff_BANG] = ACTIONS(273),
    [anon_sym_BANGfff_BANG] = ACTIONS(273),
    [anon_sym_BANGffff_BANG] = ACTIONS(273),
    [anon_sym_BANGsfz_BANG] = ACTIONS(273),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(273),
    [anon_sym_BANGsegno_BANG] = ACTIONS(273),
    [anon_sym_BANGcoda_BANG] = ACTIONS(273),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(273),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(273),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(273),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(273),
    [anon_sym_BANGfine_BANG] = ACTIONS(273),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(273),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(273),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(273),
  },
  [82] = {
    [aux_sym_nth_ending_number_repeat1] = STATE(76),
    [sym_note_skip] = ACTIONS(295),
    [anon_sym_DASH] = ACTIONS(553),
    [aux_sym_chord_symbol_token1] = ACTIONS(295),
    [sym_annotation] = ACTIONS(295),
    [anon_sym_COMMA] = ACTIONS(553),
    [sym_bar_line] = ACTIONS(295),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(293),
    [sym_thin_double_bar_line] = ACTIONS(295),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(295),
    [sym_start_of_repeated_section] = ACTIONS(295),
    [sym_end_of_repeated_section] = ACTIONS(295),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(295),
    [anon_sym_BANGtrill_BANG] = ACTIONS(295),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(295),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(295),
    [anon_sym_BANGmordent_BANG] = ACTIONS(295),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(295),
    [anon_sym_BANGroll_BANG] = ACTIONS(295),
    [anon_sym_BANGturn_BANG] = ACTIONS(295),
    [anon_sym_BANGturnx_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(295),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_BANG] = ACTIONS(295),
    [anon_sym_BANGaccent_BANG] = ACTIONS(295),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(295),
    [anon_sym_BANGfermata_BANG] = ACTIONS(295),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(295),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(295),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(295),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(295),
    [anon_sym_BANGplus_BANG] = ACTIONS(295),
    [anon_sym_BANGsnap_BANG] = ACTIONS(295),
    [anon_sym_BANGslide_BANG] = ACTIONS(295),
    [anon_sym_BANGwedge_BANG] = ACTIONS(295),
    [anon_sym_BANGupbow_BANG] = ACTIONS(295),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(295),
    [anon_sym_BANGopen_BANG] = ACTIONS(295),
    [anon_sym_BANGthumb_BANG] = ACTIONS(295),
    [anon_sym_BANGbreath_BANG] = ACTIONS(295),
    [anon_sym_BANGpppp_BANG] = ACTIONS(295),
    [anon_sym_BANGppp_BANG] = ACTIONS(295),
    [anon_sym_BANGpp_BANG] = ACTIONS(295),
    [anon_sym_BANGp_BANG] = ACTIONS(295),
    [anon_sym_BANGmp_BANG] = ACTIONS(295),
    [anon_sym_BANGmf_BANG] = ACTIONS(295),
    [anon_sym_BANGf_BANG] = ACTIONS(295),
    [anon_sym_BANGff_BANG] = ACTIONS(295),
    [anon_sym_BANGfff_BANG] = ACTIONS(295),
    [anon_sym_BANGffff_BANG] = ACTIONS(295),
    [anon_sym_BANGsfz_BANG] = ACTIONS(295),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(295),
    [anon_sym_BANGsegno_BANG] = ACTIONS(295),
    [anon_sym_BANGcoda_BANG] = ACTIONS(295),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(295),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(295),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(295),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(295),
    [anon_sym_BANGfine_BANG] = ACTIONS(295),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(295),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(295),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(295),
    [aux_sym_body_inline_info_token1] = ACTIONS(295),
  },
  [83] = {
    [sym_noCommentText] = ACTIONS(266),
    [sym_note_skip] = ACTIONS(266),
    [anon_sym_DASH] = ACTIONS(266),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(266),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(266),
    [sym_thin_double_bar_line] = ACTIONS(266),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(266),
    [sym_start_of_repeated_section] = ACTIONS(266),
    [sym_end_of_repeated_section] = ACTIONS(266),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(266),
    [anon_sym_BANGtrill_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGmordent_BANG] = ACTIONS(266),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(266),
    [anon_sym_BANGroll_BANG] = ACTIONS(266),
    [anon_sym_BANGturn_BANG] = ACTIONS(266),
    [anon_sym_BANGturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_BANG] = ACTIONS(266),
    [anon_sym_BANGaccent_BANG] = ACTIONS(266),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(266),
    [anon_sym_BANGfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(266),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(266),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(266),
    [anon_sym_BANGplus_BANG] = ACTIONS(266),
    [anon_sym_BANGsnap_BANG] = ACTIONS(266),
    [anon_sym_BANGslide_BANG] = ACTIONS(266),
    [anon_sym_BANGwedge_BANG] = ACTIONS(266),
    [anon_sym_BANGupbow_BANG] = ACTIONS(266),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(266),
    [anon_sym_BANGopen_BANG] = ACTIONS(266),
    [anon_sym_BANGthumb_BANG] = ACTIONS(266),
    [anon_sym_BANGbreath_BANG] = ACTIONS(266),
    [anon_sym_BANGpppp_BANG] = ACTIONS(266),
    [anon_sym_BANGppp_BANG] = ACTIONS(266),
    [anon_sym_BANGpp_BANG] = ACTIONS(266),
    [anon_sym_BANGp_BANG] = ACTIONS(266),
    [anon_sym_BANGmp_BANG] = ACTIONS(266),
    [anon_sym_BANGmf_BANG] = ACTIONS(266),
    [anon_sym_BANGf_BANG] = ACTIONS(266),
    [anon_sym_BANGff_BANG] = ACTIONS(266),
    [anon_sym_BANGfff_BANG] = ACTIONS(266),
    [anon_sym_BANGffff_BANG] = ACTIONS(266),
    [anon_sym_BANGsfz_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGsegno_BANG] = ACTIONS(266),
    [anon_sym_BANGcoda_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(266),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(266),
    [anon_sym_BANGfine_BANG] = ACTIONS(266),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(266),
  },
  [84] = {
    [sym_note_skip] = ACTIONS(266),
    [anon_sym_DASH] = ACTIONS(266),
    [aux_sym_chord_symbol_token1] = ACTIONS(266),
    [sym_annotation] = ACTIONS(266),
    [anon_sym_COMMA] = ACTIONS(266),
    [sym_bar_line] = ACTIONS(266),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(264),
    [sym_thin_double_bar_line] = ACTIONS(266),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(266),
    [sym_start_of_repeated_section] = ACTIONS(266),
    [sym_end_of_repeated_section] = ACTIONS(266),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(266),
    [anon_sym_BANGtrill_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(266),
    [anon_sym_BANGmordent_BANG] = ACTIONS(266),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(266),
    [anon_sym_BANGroll_BANG] = ACTIONS(266),
    [anon_sym_BANGturn_BANG] = ACTIONS(266),
    [anon_sym_BANGturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(266),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_BANG] = ACTIONS(266),
    [anon_sym_BANGaccent_BANG] = ACTIONS(266),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(266),
    [anon_sym_BANGfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(266),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(266),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(266),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(266),
    [anon_sym_BANGplus_BANG] = ACTIONS(266),
    [anon_sym_BANGsnap_BANG] = ACTIONS(266),
    [anon_sym_BANGslide_BANG] = ACTIONS(266),
    [anon_sym_BANGwedge_BANG] = ACTIONS(266),
    [anon_sym_BANGupbow_BANG] = ACTIONS(266),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(266),
    [anon_sym_BANGopen_BANG] = ACTIONS(266),
    [anon_sym_BANGthumb_BANG] = ACTIONS(266),
    [anon_sym_BANGbreath_BANG] = ACTIONS(266),
    [anon_sym_BANGpppp_BANG] = ACTIONS(266),
    [anon_sym_BANGppp_BANG] = ACTIONS(266),
    [anon_sym_BANGpp_BANG] = ACTIONS(266),
    [anon_sym_BANGp_BANG] = ACTIONS(266),
    [anon_sym_BANGmp_BANG] = ACTIONS(266),
    [anon_sym_BANGmf_BANG] = ACTIONS(266),
    [anon_sym_BANGf_BANG] = ACTIONS(266),
    [anon_sym_BANGff_BANG] = ACTIONS(266),
    [anon_sym_BANGfff_BANG] = ACTIONS(266),
    [anon_sym_BANGffff_BANG] = ACTIONS(266),
    [anon_sym_BANGsfz_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(266),
    [anon_sym_BANGsegno_BANG] = ACTIONS(266),
    [anon_sym_BANGcoda_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(266),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(266),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(266),
    [anon_sym_BANGfine_BANG] = ACTIONS(266),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(266),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(266),
    [aux_sym_body_inline_info_token1] = ACTIONS(266),
  },
  [85] = {
    [sym_noCommentText] = ACTIONS(560),
    [sym_note_skip] = ACTIONS(560),
    [aux_sym_chord_symbol_token1] = ACTIONS(560),
    [sym_annotation] = ACTIONS(560),
    [sym_bar_line] = ACTIONS(560),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(560),
    [sym_thin_double_bar_line] = ACTIONS(560),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(560),
    [sym_start_of_repeated_section] = ACTIONS(560),
    [sym_end_of_repeated_section] = ACTIONS(560),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(560),
    [anon_sym_BANGtrill_BANG] = ACTIONS(560),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(560),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(560),
    [anon_sym_BANGmordent_BANG] = ACTIONS(560),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(560),
    [anon_sym_BANGroll_BANG] = ACTIONS(560),
    [anon_sym_BANGturn_BANG] = ACTIONS(560),
    [anon_sym_BANGturnx_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(560),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_BANG] = ACTIONS(560),
    [anon_sym_BANGaccent_BANG] = ACTIONS(560),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(560),
    [anon_sym_BANGfermata_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(560),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(560),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(560),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(560),
    [anon_sym_BANGplus_BANG] = ACTIONS(560),
    [anon_sym_BANGsnap_BANG] = ACTIONS(560),
    [anon_sym_BANGslide_BANG] = ACTIONS(560),
    [anon_sym_BANGwedge_BANG] = ACTIONS(560),
    [anon_sym_BANGupbow_BANG] = ACTIONS(560),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(560),
    [anon_sym_BANGopen_BANG] = ACTIONS(560),
    [anon_sym_BANGthumb_BANG] = ACTIONS(560),
    [anon_sym_BANGbreath_BANG] = ACTIONS(560),
    [anon_sym_BANGpppp_BANG] = ACTIONS(560),
    [anon_sym_BANGppp_BANG] = ACTIONS(560),
    [anon_sym_BANGpp_BANG] = ACTIONS(560),
    [anon_sym_BANGp_BANG] = ACTIONS(560),
    [anon_sym_BANGmp_BANG] = ACTIONS(560),
    [anon_sym_BANGmf_BANG] = ACTIONS(560),
    [anon_sym_BANGf_BANG] = ACTIONS(560),
    [anon_sym_BANGff_BANG] = ACTIONS(560),
    [anon_sym_BANGfff_BANG] = ACTIONS(560),
    [anon_sym_BANGffff_BANG] = ACTIONS(560),
    [anon_sym_BANGsfz_BANG] = ACTIONS(560),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGsegno_BANG] = ACTIONS(560),
    [anon_sym_BANGcoda_BANG] = ACTIONS(560),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(560),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(560),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(560),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(560),
    [anon_sym_BANGfine_BANG] = ACTIONS(560),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(560),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(560),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(560),
  },
  [86] = {
    [sym_noCommentText] = ACTIONS(279),
    [sym_note_skip] = ACTIONS(279),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(279),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
  },
  [87] = {
    [sym_noCommentText] = ACTIONS(562),
    [sym_note_skip] = ACTIONS(562),
    [aux_sym_chord_symbol_token1] = ACTIONS(562),
    [sym_annotation] = ACTIONS(562),
    [sym_bar_line] = ACTIONS(562),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(562),
    [sym_thin_double_bar_line] = ACTIONS(562),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(562),
    [sym_start_of_repeated_section] = ACTIONS(562),
    [sym_end_of_repeated_section] = ACTIONS(562),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(562),
    [anon_sym_BANGtrill_BANG] = ACTIONS(562),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(562),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(562),
    [anon_sym_BANGmordent_BANG] = ACTIONS(562),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(562),
    [anon_sym_BANGroll_BANG] = ACTIONS(562),
    [anon_sym_BANGturn_BANG] = ACTIONS(562),
    [anon_sym_BANGturnx_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(562),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_BANG] = ACTIONS(562),
    [anon_sym_BANGaccent_BANG] = ACTIONS(562),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(562),
    [anon_sym_BANGfermata_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(562),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(562),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(562),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(562),
    [anon_sym_BANGplus_BANG] = ACTIONS(562),
    [anon_sym_BANGsnap_BANG] = ACTIONS(562),
    [anon_sym_BANGslide_BANG] = ACTIONS(562),
    [anon_sym_BANGwedge_BANG] = ACTIONS(562),
    [anon_sym_BANGupbow_BANG] = ACTIONS(562),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(562),
    [anon_sym_BANGopen_BANG] = ACTIONS(562),
    [anon_sym_BANGthumb_BANG] = ACTIONS(562),
    [anon_sym_BANGbreath_BANG] = ACTIONS(562),
    [anon_sym_BANGpppp_BANG] = ACTIONS(562),
    [anon_sym_BANGppp_BANG] = ACTIONS(562),
    [anon_sym_BANGpp_BANG] = ACTIONS(562),
    [anon_sym_BANGp_BANG] = ACTIONS(562),
    [anon_sym_BANGmp_BANG] = ACTIONS(562),
    [anon_sym_BANGmf_BANG] = ACTIONS(562),
    [anon_sym_BANGf_BANG] = ACTIONS(562),
    [anon_sym_BANGff_BANG] = ACTIONS(562),
    [anon_sym_BANGfff_BANG] = ACTIONS(562),
    [anon_sym_BANGffff_BANG] = ACTIONS(562),
    [anon_sym_BANGsfz_BANG] = ACTIONS(562),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGsegno_BANG] = ACTIONS(562),
    [anon_sym_BANGcoda_BANG] = ACTIONS(562),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(562),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(562),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(562),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(562),
    [anon_sym_BANGfine_BANG] = ACTIONS(562),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(562),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(562),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(562),
  },
  [88] = {
    [sym_noCommentText] = ACTIONS(387),
    [sym_note_skip] = ACTIONS(387),
    [aux_sym_chord_symbol_token1] = ACTIONS(387),
    [sym_annotation] = ACTIONS(387),
    [sym_bar_line] = ACTIONS(387),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(387),
    [sym_thin_double_bar_line] = ACTIONS(387),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(387),
    [sym_start_of_repeated_section] = ACTIONS(387),
    [sym_end_of_repeated_section] = ACTIONS(387),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(387),
    [anon_sym_BANGtrill_BANG] = ACTIONS(387),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(387),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(387),
    [anon_sym_BANGmordent_BANG] = ACTIONS(387),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(387),
    [anon_sym_BANGroll_BANG] = ACTIONS(387),
    [anon_sym_BANGturn_BANG] = ACTIONS(387),
    [anon_sym_BANGturnx_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(387),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_BANG] = ACTIONS(387),
    [anon_sym_BANGaccent_BANG] = ACTIONS(387),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(387),
    [anon_sym_BANGfermata_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(387),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(387),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(387),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(387),
    [anon_sym_BANGplus_BANG] = ACTIONS(387),
    [anon_sym_BANGsnap_BANG] = ACTIONS(387),
    [anon_sym_BANGslide_BANG] = ACTIONS(387),
    [anon_sym_BANGwedge_BANG] = ACTIONS(387),
    [anon_sym_BANGupbow_BANG] = ACTIONS(387),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(387),
    [anon_sym_BANGopen_BANG] = ACTIONS(387),
    [anon_sym_BANGthumb_BANG] = ACTIONS(387),
    [anon_sym_BANGbreath_BANG] = ACTIONS(387),
    [anon_sym_BANGpppp_BANG] = ACTIONS(387),
    [anon_sym_BANGppp_BANG] = ACTIONS(387),
    [anon_sym_BANGpp_BANG] = ACTIONS(387),
    [anon_sym_BANGp_BANG] = ACTIONS(387),
    [anon_sym_BANGmp_BANG] = ACTIONS(387),
    [anon_sym_BANGmf_BANG] = ACTIONS(387),
    [anon_sym_BANGf_BANG] = ACTIONS(387),
    [anon_sym_BANGff_BANG] = ACTIONS(387),
    [anon_sym_BANGfff_BANG] = ACTIONS(387),
    [anon_sym_BANGffff_BANG] = ACTIONS(387),
    [anon_sym_BANGsfz_BANG] = ACTIONS(387),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGsegno_BANG] = ACTIONS(387),
    [anon_sym_BANGcoda_BANG] = ACTIONS(387),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(387),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(387),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(387),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(387),
    [anon_sym_BANGfine_BANG] = ACTIONS(387),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(387),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(387),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(387),
  },
  [89] = {
    [sym_note_skip] = ACTIONS(279),
    [aux_sym_chord_symbol_token1] = ACTIONS(279),
    [sym_annotation] = ACTIONS(279),
    [sym_bar_line] = ACTIONS(279),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(277),
    [sym_thin_double_bar_line] = ACTIONS(279),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(279),
    [sym_start_of_repeated_section] = ACTIONS(279),
    [sym_end_of_repeated_section] = ACTIONS(279),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(279),
    [anon_sym_BANGtrill_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(279),
    [anon_sym_BANGmordent_BANG] = ACTIONS(279),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(279),
    [anon_sym_BANGroll_BANG] = ACTIONS(279),
    [anon_sym_BANGturn_BANG] = ACTIONS(279),
    [anon_sym_BANGturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(279),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_BANG] = ACTIONS(279),
    [anon_sym_BANGaccent_BANG] = ACTIONS(279),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(279),
    [anon_sym_BANGfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(279),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(279),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(279),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(279),
    [anon_sym_BANGplus_BANG] = ACTIONS(279),
    [anon_sym_BANGsnap_BANG] = ACTIONS(279),
    [anon_sym_BANGslide_BANG] = ACTIONS(279),
    [anon_sym_BANGwedge_BANG] = ACTIONS(279),
    [anon_sym_BANGupbow_BANG] = ACTIONS(279),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(279),
    [anon_sym_BANGopen_BANG] = ACTIONS(279),
    [anon_sym_BANGthumb_BANG] = ACTIONS(279),
    [anon_sym_BANGbreath_BANG] = ACTIONS(279),
    [anon_sym_BANGpppp_BANG] = ACTIONS(279),
    [anon_sym_BANGppp_BANG] = ACTIONS(279),
    [anon_sym_BANGpp_BANG] = ACTIONS(279),
    [anon_sym_BANGp_BANG] = ACTIONS(279),
    [anon_sym_BANGmp_BANG] = ACTIONS(279),
    [anon_sym_BANGmf_BANG] = ACTIONS(279),
    [anon_sym_BANGf_BANG] = ACTIONS(279),
    [anon_sym_BANGff_BANG] = ACTIONS(279),
    [anon_sym_BANGfff_BANG] = ACTIONS(279),
    [anon_sym_BANGffff_BANG] = ACTIONS(279),
    [anon_sym_BANGsfz_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(279),
    [anon_sym_BANGsegno_BANG] = ACTIONS(279),
    [anon_sym_BANGcoda_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(279),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(279),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(279),
    [anon_sym_BANGfine_BANG] = ACTIONS(279),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(279),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(279),
    [aux_sym_body_inline_info_token1] = ACTIONS(279),
  },
  [90] = {
    [sym_noCommentText] = ACTIONS(383),
    [sym_note_skip] = ACTIONS(383),
    [aux_sym_chord_symbol_token1] = ACTIONS(383),
    [sym_annotation] = ACTIONS(383),
    [sym_bar_line] = ACTIONS(383),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(383),
    [sym_thin_double_bar_line] = ACTIONS(383),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(383),
    [sym_start_of_repeated_section] = ACTIONS(383),
    [sym_end_of_repeated_section] = ACTIONS(383),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(383),
    [anon_sym_BANGtrill_BANG] = ACTIONS(383),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(383),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(383),
    [anon_sym_BANGmordent_BANG] = ACTIONS(383),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(383),
    [anon_sym_BANGroll_BANG] = ACTIONS(383),
    [anon_sym_BANGturn_BANG] = ACTIONS(383),
    [anon_sym_BANGturnx_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(383),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_BANG] = ACTIONS(383),
    [anon_sym_BANGaccent_BANG] = ACTIONS(383),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(383),
    [anon_sym_BANGfermata_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(383),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(383),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(383),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(383),
    [anon_sym_BANGplus_BANG] = ACTIONS(383),
    [anon_sym_BANGsnap_BANG] = ACTIONS(383),
    [anon_sym_BANGslide_BANG] = ACTIONS(383),
    [anon_sym_BANGwedge_BANG] = ACTIONS(383),
    [anon_sym_BANGupbow_BANG] = ACTIONS(383),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(383),
    [anon_sym_BANGopen_BANG] = ACTIONS(383),
    [anon_sym_BANGthumb_BANG] = ACTIONS(383),
    [anon_sym_BANGbreath_BANG] = ACTIONS(383),
    [anon_sym_BANGpppp_BANG] = ACTIONS(383),
    [anon_sym_BANGppp_BANG] = ACTIONS(383),
    [anon_sym_BANGpp_BANG] = ACTIONS(383),
    [anon_sym_BANGp_BANG] = ACTIONS(383),
    [anon_sym_BANGmp_BANG] = ACTIONS(383),
    [anon_sym_BANGmf_BANG] = ACTIONS(383),
    [anon_sym_BANGf_BANG] = ACTIONS(383),
    [anon_sym_BANGff_BANG] = ACTIONS(383),
    [anon_sym_BANGfff_BANG] = ACTIONS(383),
    [anon_sym_BANGffff_BANG] = ACTIONS(383),
    [anon_sym_BANGsfz_BANG] = ACTIONS(383),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGsegno_BANG] = ACTIONS(383),
    [anon_sym_BANGcoda_BANG] = ACTIONS(383),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(383),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(383),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(383),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(383),
    [anon_sym_BANGfine_BANG] = ACTIONS(383),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(383),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(383),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(383),
  },
  [91] = {
    [sym_noCommentText] = ACTIONS(397),
    [sym_note_skip] = ACTIONS(397),
    [aux_sym_chord_symbol_token1] = ACTIONS(397),
    [sym_annotation] = ACTIONS(397),
    [sym_bar_line] = ACTIONS(397),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(397),
    [sym_thin_double_bar_line] = ACTIONS(397),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(397),
    [sym_start_of_repeated_section] = ACTIONS(397),
    [sym_end_of_repeated_section] = ACTIONS(397),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(397),
    [anon_sym_BANGtrill_BANG] = ACTIONS(397),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(397),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(397),
    [anon_sym_BANGmordent_BANG] = ACTIONS(397),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(397),
    [anon_sym_BANGroll_BANG] = ACTIONS(397),
    [anon_sym_BANGturn_BANG] = ACTIONS(397),
    [anon_sym_BANGturnx_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(397),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_BANG] = ACTIONS(397),
    [anon_sym_BANGaccent_BANG] = ACTIONS(397),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(397),
    [anon_sym_BANGfermata_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(397),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(397),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(397),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(397),
    [anon_sym_BANGplus_BANG] = ACTIONS(397),
    [anon_sym_BANGsnap_BANG] = ACTIONS(397),
    [anon_sym_BANGslide_BANG] = ACTIONS(397),
    [anon_sym_BANGwedge_BANG] = ACTIONS(397),
    [anon_sym_BANGupbow_BANG] = ACTIONS(397),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(397),
    [anon_sym_BANGopen_BANG] = ACTIONS(397),
    [anon_sym_BANGthumb_BANG] = ACTIONS(397),
    [anon_sym_BANGbreath_BANG] = ACTIONS(397),
    [anon_sym_BANGpppp_BANG] = ACTIONS(397),
    [anon_sym_BANGppp_BANG] = ACTIONS(397),
    [anon_sym_BANGpp_BANG] = ACTIONS(397),
    [anon_sym_BANGp_BANG] = ACTIONS(397),
    [anon_sym_BANGmp_BANG] = ACTIONS(397),
    [anon_sym_BANGmf_BANG] = ACTIONS(397),
    [anon_sym_BANGf_BANG] = ACTIONS(397),
    [anon_sym_BANGff_BANG] = ACTIONS(397),
    [anon_sym_BANGfff_BANG] = ACTIONS(397),
    [anon_sym_BANGffff_BANG] = ACTIONS(397),
    [anon_sym_BANGsfz_BANG] = ACTIONS(397),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGsegno_BANG] = ACTIONS(397),
    [anon_sym_BANGcoda_BANG] = ACTIONS(397),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(397),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(397),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(397),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(397),
    [anon_sym_BANGfine_BANG] = ACTIONS(397),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(397),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(397),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(397),
  },
  [92] = {
    [sym_note_skip] = ACTIONS(562),
    [aux_sym_chord_symbol_token1] = ACTIONS(562),
    [sym_annotation] = ACTIONS(562),
    [sym_bar_line] = ACTIONS(562),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(564),
    [sym_thin_double_bar_line] = ACTIONS(562),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(562),
    [sym_start_of_repeated_section] = ACTIONS(562),
    [sym_end_of_repeated_section] = ACTIONS(562),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(562),
    [anon_sym_BANGtrill_BANG] = ACTIONS(562),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(562),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(562),
    [anon_sym_BANGmordent_BANG] = ACTIONS(562),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(562),
    [anon_sym_BANGroll_BANG] = ACTIONS(562),
    [anon_sym_BANGturn_BANG] = ACTIONS(562),
    [anon_sym_BANGturnx_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(562),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_BANG] = ACTIONS(562),
    [anon_sym_BANGaccent_BANG] = ACTIONS(562),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(562),
    [anon_sym_BANGfermata_BANG] = ACTIONS(562),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(562),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(562),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(562),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(562),
    [anon_sym_BANGplus_BANG] = ACTIONS(562),
    [anon_sym_BANGsnap_BANG] = ACTIONS(562),
    [anon_sym_BANGslide_BANG] = ACTIONS(562),
    [anon_sym_BANGwedge_BANG] = ACTIONS(562),
    [anon_sym_BANGupbow_BANG] = ACTIONS(562),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(562),
    [anon_sym_BANGopen_BANG] = ACTIONS(562),
    [anon_sym_BANGthumb_BANG] = ACTIONS(562),
    [anon_sym_BANGbreath_BANG] = ACTIONS(562),
    [anon_sym_BANGpppp_BANG] = ACTIONS(562),
    [anon_sym_BANGppp_BANG] = ACTIONS(562),
    [anon_sym_BANGpp_BANG] = ACTIONS(562),
    [anon_sym_BANGp_BANG] = ACTIONS(562),
    [anon_sym_BANGmp_BANG] = ACTIONS(562),
    [anon_sym_BANGmf_BANG] = ACTIONS(562),
    [anon_sym_BANGf_BANG] = ACTIONS(562),
    [anon_sym_BANGff_BANG] = ACTIONS(562),
    [anon_sym_BANGfff_BANG] = ACTIONS(562),
    [anon_sym_BANGffff_BANG] = ACTIONS(562),
    [anon_sym_BANGsfz_BANG] = ACTIONS(562),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(562),
    [anon_sym_BANGsegno_BANG] = ACTIONS(562),
    [anon_sym_BANGcoda_BANG] = ACTIONS(562),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(562),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(562),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(562),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(562),
    [anon_sym_BANGfine_BANG] = ACTIONS(562),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(562),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(562),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(562),
    [aux_sym_body_inline_info_token1] = ACTIONS(562),
  },
  [93] = {
    [sym_noCommentText] = ACTIONS(375),
    [sym_note_skip] = ACTIONS(375),
    [aux_sym_chord_symbol_token1] = ACTIONS(375),
    [sym_annotation] = ACTIONS(375),
    [sym_bar_line] = ACTIONS(375),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(375),
    [sym_thin_double_bar_line] = ACTIONS(375),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(375),
    [sym_start_of_repeated_section] = ACTIONS(375),
    [sym_end_of_repeated_section] = ACTIONS(375),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(375),
    [anon_sym_BANGtrill_BANG] = ACTIONS(375),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(375),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(375),
    [anon_sym_BANGmordent_BANG] = ACTIONS(375),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(375),
    [anon_sym_BANGroll_BANG] = ACTIONS(375),
    [anon_sym_BANGturn_BANG] = ACTIONS(375),
    [anon_sym_BANGturnx_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(375),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_BANG] = ACTIONS(375),
    [anon_sym_BANGaccent_BANG] = ACTIONS(375),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(375),
    [anon_sym_BANGfermata_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(375),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(375),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(375),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(375),
    [anon_sym_BANGplus_BANG] = ACTIONS(375),
    [anon_sym_BANGsnap_BANG] = ACTIONS(375),
    [anon_sym_BANGslide_BANG] = ACTIONS(375),
    [anon_sym_BANGwedge_BANG] = ACTIONS(375),
    [anon_sym_BANGupbow_BANG] = ACTIONS(375),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(375),
    [anon_sym_BANGopen_BANG] = ACTIONS(375),
    [anon_sym_BANGthumb_BANG] = ACTIONS(375),
    [anon_sym_BANGbreath_BANG] = ACTIONS(375),
    [anon_sym_BANGpppp_BANG] = ACTIONS(375),
    [anon_sym_BANGppp_BANG] = ACTIONS(375),
    [anon_sym_BANGpp_BANG] = ACTIONS(375),
    [anon_sym_BANGp_BANG] = ACTIONS(375),
    [anon_sym_BANGmp_BANG] = ACTIONS(375),
    [anon_sym_BANGmf_BANG] = ACTIONS(375),
    [anon_sym_BANGf_BANG] = ACTIONS(375),
    [anon_sym_BANGff_BANG] = ACTIONS(375),
    [anon_sym_BANGfff_BANG] = ACTIONS(375),
    [anon_sym_BANGffff_BANG] = ACTIONS(375),
    [anon_sym_BANGsfz_BANG] = ACTIONS(375),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGsegno_BANG] = ACTIONS(375),
    [anon_sym_BANGcoda_BANG] = ACTIONS(375),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(375),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(375),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(375),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(375),
    [anon_sym_BANGfine_BANG] = ACTIONS(375),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(375),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(375),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(375),
  },
  [94] = {
    [sym_note_skip] = ACTIONS(409),
    [aux_sym_chord_symbol_token1] = ACTIONS(409),
    [sym_annotation] = ACTIONS(409),
    [sym_bar_line] = ACTIONS(409),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(407),
    [sym_thin_double_bar_line] = ACTIONS(409),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(409),
    [sym_start_of_repeated_section] = ACTIONS(409),
    [sym_end_of_repeated_section] = ACTIONS(409),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(409),
    [anon_sym_BANGtrill_BANG] = ACTIONS(409),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(409),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(409),
    [anon_sym_BANGmordent_BANG] = ACTIONS(409),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(409),
    [anon_sym_BANGroll_BANG] = ACTIONS(409),
    [anon_sym_BANGturn_BANG] = ACTIONS(409),
    [anon_sym_BANGturnx_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(409),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_BANG] = ACTIONS(409),
    [anon_sym_BANGaccent_BANG] = ACTIONS(409),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(409),
    [anon_sym_BANGfermata_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(409),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(409),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(409),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(409),
    [anon_sym_BANGplus_BANG] = ACTIONS(409),
    [anon_sym_BANGsnap_BANG] = ACTIONS(409),
    [anon_sym_BANGslide_BANG] = ACTIONS(409),
    [anon_sym_BANGwedge_BANG] = ACTIONS(409),
    [anon_sym_BANGupbow_BANG] = ACTIONS(409),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(409),
    [anon_sym_BANGopen_BANG] = ACTIONS(409),
    [anon_sym_BANGthumb_BANG] = ACTIONS(409),
    [anon_sym_BANGbreath_BANG] = ACTIONS(409),
    [anon_sym_BANGpppp_BANG] = ACTIONS(409),
    [anon_sym_BANGppp_BANG] = ACTIONS(409),
    [anon_sym_BANGpp_BANG] = ACTIONS(409),
    [anon_sym_BANGp_BANG] = ACTIONS(409),
    [anon_sym_BANGmp_BANG] = ACTIONS(409),
    [anon_sym_BANGmf_BANG] = ACTIONS(409),
    [anon_sym_BANGf_BANG] = ACTIONS(409),
    [anon_sym_BANGff_BANG] = ACTIONS(409),
    [anon_sym_BANGfff_BANG] = ACTIONS(409),
    [anon_sym_BANGffff_BANG] = ACTIONS(409),
    [anon_sym_BANGsfz_BANG] = ACTIONS(409),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGsegno_BANG] = ACTIONS(409),
    [anon_sym_BANGcoda_BANG] = ACTIONS(409),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(409),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(409),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(409),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(409),
    [anon_sym_BANGfine_BANG] = ACTIONS(409),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(409),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(409),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(409),
    [aux_sym_body_inline_info_token1] = ACTIONS(409),
  },
  [95] = {
    [sym_noCommentText] = ACTIONS(566),
    [sym_note_skip] = ACTIONS(566),
    [aux_sym_chord_symbol_token1] = ACTIONS(566),
    [sym_annotation] = ACTIONS(566),
    [sym_bar_line] = ACTIONS(566),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(566),
    [sym_thin_double_bar_line] = ACTIONS(566),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(566),
    [sym_start_of_repeated_section] = ACTIONS(566),
    [sym_end_of_repeated_section] = ACTIONS(566),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(566),
    [anon_sym_BANGtrill_BANG] = ACTIONS(566),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(566),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(566),
    [anon_sym_BANGmordent_BANG] = ACTIONS(566),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(566),
    [anon_sym_BANGroll_BANG] = ACTIONS(566),
    [anon_sym_BANGturn_BANG] = ACTIONS(566),
    [anon_sym_BANGturnx_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(566),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_BANG] = ACTIONS(566),
    [anon_sym_BANGaccent_BANG] = ACTIONS(566),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(566),
    [anon_sym_BANGfermata_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(566),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(566),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(566),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(566),
    [anon_sym_BANGplus_BANG] = ACTIONS(566),
    [anon_sym_BANGsnap_BANG] = ACTIONS(566),
    [anon_sym_BANGslide_BANG] = ACTIONS(566),
    [anon_sym_BANGwedge_BANG] = ACTIONS(566),
    [anon_sym_BANGupbow_BANG] = ACTIONS(566),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(566),
    [anon_sym_BANGopen_BANG] = ACTIONS(566),
    [anon_sym_BANGthumb_BANG] = ACTIONS(566),
    [anon_sym_BANGbreath_BANG] = ACTIONS(566),
    [anon_sym_BANGpppp_BANG] = ACTIONS(566),
    [anon_sym_BANGppp_BANG] = ACTIONS(566),
    [anon_sym_BANGpp_BANG] = ACTIONS(566),
    [anon_sym_BANGp_BANG] = ACTIONS(566),
    [anon_sym_BANGmp_BANG] = ACTIONS(566),
    [anon_sym_BANGmf_BANG] = ACTIONS(566),
    [anon_sym_BANGf_BANG] = ACTIONS(566),
    [anon_sym_BANGff_BANG] = ACTIONS(566),
    [anon_sym_BANGfff_BANG] = ACTIONS(566),
    [anon_sym_BANGffff_BANG] = ACTIONS(566),
    [anon_sym_BANGsfz_BANG] = ACTIONS(566),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGsegno_BANG] = ACTIONS(566),
    [anon_sym_BANGcoda_BANG] = ACTIONS(566),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(566),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(566),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(566),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(566),
    [anon_sym_BANGfine_BANG] = ACTIONS(566),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(566),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(566),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(566),
  },
  [96] = {
    [sym_noCommentText] = ACTIONS(409),
    [sym_note_skip] = ACTIONS(409),
    [aux_sym_chord_symbol_token1] = ACTIONS(409),
    [sym_annotation] = ACTIONS(409),
    [sym_bar_line] = ACTIONS(409),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(409),
    [sym_thin_double_bar_line] = ACTIONS(409),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(409),
    [sym_start_of_repeated_section] = ACTIONS(409),
    [sym_end_of_repeated_section] = ACTIONS(409),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(409),
    [anon_sym_BANGtrill_BANG] = ACTIONS(409),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(409),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(409),
    [anon_sym_BANGmordent_BANG] = ACTIONS(409),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(409),
    [anon_sym_BANGroll_BANG] = ACTIONS(409),
    [anon_sym_BANGturn_BANG] = ACTIONS(409),
    [anon_sym_BANGturnx_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(409),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_BANG] = ACTIONS(409),
    [anon_sym_BANGaccent_BANG] = ACTIONS(409),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(409),
    [anon_sym_BANGfermata_BANG] = ACTIONS(409),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(409),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(409),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(409),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(409),
    [anon_sym_BANGplus_BANG] = ACTIONS(409),
    [anon_sym_BANGsnap_BANG] = ACTIONS(409),
    [anon_sym_BANGslide_BANG] = ACTIONS(409),
    [anon_sym_BANGwedge_BANG] = ACTIONS(409),
    [anon_sym_BANGupbow_BANG] = ACTIONS(409),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(409),
    [anon_sym_BANGopen_BANG] = ACTIONS(409),
    [anon_sym_BANGthumb_BANG] = ACTIONS(409),
    [anon_sym_BANGbreath_BANG] = ACTIONS(409),
    [anon_sym_BANGpppp_BANG] = ACTIONS(409),
    [anon_sym_BANGppp_BANG] = ACTIONS(409),
    [anon_sym_BANGpp_BANG] = ACTIONS(409),
    [anon_sym_BANGp_BANG] = ACTIONS(409),
    [anon_sym_BANGmp_BANG] = ACTIONS(409),
    [anon_sym_BANGmf_BANG] = ACTIONS(409),
    [anon_sym_BANGf_BANG] = ACTIONS(409),
    [anon_sym_BANGff_BANG] = ACTIONS(409),
    [anon_sym_BANGfff_BANG] = ACTIONS(409),
    [anon_sym_BANGffff_BANG] = ACTIONS(409),
    [anon_sym_BANGsfz_BANG] = ACTIONS(409),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(409),
    [anon_sym_BANGsegno_BANG] = ACTIONS(409),
    [anon_sym_BANGcoda_BANG] = ACTIONS(409),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(409),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(409),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(409),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(409),
    [anon_sym_BANGfine_BANG] = ACTIONS(409),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(409),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(409),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(409),
  },
  [97] = {
    [sym_note_skip] = ACTIONS(405),
    [aux_sym_chord_symbol_token1] = ACTIONS(405),
    [sym_annotation] = ACTIONS(405),
    [sym_bar_line] = ACTIONS(405),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(403),
    [sym_thin_double_bar_line] = ACTIONS(405),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(405),
    [sym_start_of_repeated_section] = ACTIONS(405),
    [sym_end_of_repeated_section] = ACTIONS(405),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(405),
    [anon_sym_BANGtrill_BANG] = ACTIONS(405),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(405),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(405),
    [anon_sym_BANGmordent_BANG] = ACTIONS(405),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(405),
    [anon_sym_BANGroll_BANG] = ACTIONS(405),
    [anon_sym_BANGturn_BANG] = ACTIONS(405),
    [anon_sym_BANGturnx_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(405),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_BANG] = ACTIONS(405),
    [anon_sym_BANGaccent_BANG] = ACTIONS(405),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(405),
    [anon_sym_BANGfermata_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(405),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(405),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(405),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(405),
    [anon_sym_BANGplus_BANG] = ACTIONS(405),
    [anon_sym_BANGsnap_BANG] = ACTIONS(405),
    [anon_sym_BANGslide_BANG] = ACTIONS(405),
    [anon_sym_BANGwedge_BANG] = ACTIONS(405),
    [anon_sym_BANGupbow_BANG] = ACTIONS(405),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(405),
    [anon_sym_BANGopen_BANG] = ACTIONS(405),
    [anon_sym_BANGthumb_BANG] = ACTIONS(405),
    [anon_sym_BANGbreath_BANG] = ACTIONS(405),
    [anon_sym_BANGpppp_BANG] = ACTIONS(405),
    [anon_sym_BANGppp_BANG] = ACTIONS(405),
    [anon_sym_BANGpp_BANG] = ACTIONS(405),
    [anon_sym_BANGp_BANG] = ACTIONS(405),
    [anon_sym_BANGmp_BANG] = ACTIONS(405),
    [anon_sym_BANGmf_BANG] = ACTIONS(405),
    [anon_sym_BANGf_BANG] = ACTIONS(405),
    [anon_sym_BANGff_BANG] = ACTIONS(405),
    [anon_sym_BANGfff_BANG] = ACTIONS(405),
    [anon_sym_BANGffff_BANG] = ACTIONS(405),
    [anon_sym_BANGsfz_BANG] = ACTIONS(405),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGsegno_BANG] = ACTIONS(405),
    [anon_sym_BANGcoda_BANG] = ACTIONS(405),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(405),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(405),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(405),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(405),
    [anon_sym_BANGfine_BANG] = ACTIONS(405),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(405),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(405),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(405),
    [aux_sym_body_inline_info_token1] = ACTIONS(405),
  },
  [98] = {
    [sym_note_skip] = ACTIONS(560),
    [aux_sym_chord_symbol_token1] = ACTIONS(560),
    [sym_annotation] = ACTIONS(560),
    [sym_bar_line] = ACTIONS(560),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(568),
    [sym_thin_double_bar_line] = ACTIONS(560),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(560),
    [sym_start_of_repeated_section] = ACTIONS(560),
    [sym_end_of_repeated_section] = ACTIONS(560),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(560),
    [anon_sym_BANGtrill_BANG] = ACTIONS(560),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(560),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(560),
    [anon_sym_BANGmordent_BANG] = ACTIONS(560),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(560),
    [anon_sym_BANGroll_BANG] = ACTIONS(560),
    [anon_sym_BANGturn_BANG] = ACTIONS(560),
    [anon_sym_BANGturnx_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(560),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_BANG] = ACTIONS(560),
    [anon_sym_BANGaccent_BANG] = ACTIONS(560),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(560),
    [anon_sym_BANGfermata_BANG] = ACTIONS(560),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(560),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(560),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(560),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(560),
    [anon_sym_BANGplus_BANG] = ACTIONS(560),
    [anon_sym_BANGsnap_BANG] = ACTIONS(560),
    [anon_sym_BANGslide_BANG] = ACTIONS(560),
    [anon_sym_BANGwedge_BANG] = ACTIONS(560),
    [anon_sym_BANGupbow_BANG] = ACTIONS(560),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(560),
    [anon_sym_BANGopen_BANG] = ACTIONS(560),
    [anon_sym_BANGthumb_BANG] = ACTIONS(560),
    [anon_sym_BANGbreath_BANG] = ACTIONS(560),
    [anon_sym_BANGpppp_BANG] = ACTIONS(560),
    [anon_sym_BANGppp_BANG] = ACTIONS(560),
    [anon_sym_BANGpp_BANG] = ACTIONS(560),
    [anon_sym_BANGp_BANG] = ACTIONS(560),
    [anon_sym_BANGmp_BANG] = ACTIONS(560),
    [anon_sym_BANGmf_BANG] = ACTIONS(560),
    [anon_sym_BANGf_BANG] = ACTIONS(560),
    [anon_sym_BANGff_BANG] = ACTIONS(560),
    [anon_sym_BANGfff_BANG] = ACTIONS(560),
    [anon_sym_BANGffff_BANG] = ACTIONS(560),
    [anon_sym_BANGsfz_BANG] = ACTIONS(560),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(560),
    [anon_sym_BANGsegno_BANG] = ACTIONS(560),
    [anon_sym_BANGcoda_BANG] = ACTIONS(560),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(560),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(560),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(560),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(560),
    [anon_sym_BANGfine_BANG] = ACTIONS(560),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(560),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(560),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(560),
    [aux_sym_body_inline_info_token1] = ACTIONS(560),
  },
  [99] = {
    [sym_noCommentText] = ACTIONS(379),
    [sym_note_skip] = ACTIONS(379),
    [aux_sym_chord_symbol_token1] = ACTIONS(379),
    [sym_annotation] = ACTIONS(379),
    [sym_bar_line] = ACTIONS(379),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(379),
    [sym_thin_double_bar_line] = ACTIONS(379),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(379),
    [sym_start_of_repeated_section] = ACTIONS(379),
    [sym_end_of_repeated_section] = ACTIONS(379),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(379),
    [anon_sym_BANGtrill_BANG] = ACTIONS(379),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(379),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(379),
    [anon_sym_BANGmordent_BANG] = ACTIONS(379),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(379),
    [anon_sym_BANGroll_BANG] = ACTIONS(379),
    [anon_sym_BANGturn_BANG] = ACTIONS(379),
    [anon_sym_BANGturnx_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(379),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_BANG] = ACTIONS(379),
    [anon_sym_BANGaccent_BANG] = ACTIONS(379),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(379),
    [anon_sym_BANGfermata_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(379),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(379),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(379),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(379),
    [anon_sym_BANGplus_BANG] = ACTIONS(379),
    [anon_sym_BANGsnap_BANG] = ACTIONS(379),
    [anon_sym_BANGslide_BANG] = ACTIONS(379),
    [anon_sym_BANGwedge_BANG] = ACTIONS(379),
    [anon_sym_BANGupbow_BANG] = ACTIONS(379),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(379),
    [anon_sym_BANGopen_BANG] = ACTIONS(379),
    [anon_sym_BANGthumb_BANG] = ACTIONS(379),
    [anon_sym_BANGbreath_BANG] = ACTIONS(379),
    [anon_sym_BANGpppp_BANG] = ACTIONS(379),
    [anon_sym_BANGppp_BANG] = ACTIONS(379),
    [anon_sym_BANGpp_BANG] = ACTIONS(379),
    [anon_sym_BANGp_BANG] = ACTIONS(379),
    [anon_sym_BANGmp_BANG] = ACTIONS(379),
    [anon_sym_BANGmf_BANG] = ACTIONS(379),
    [anon_sym_BANGf_BANG] = ACTIONS(379),
    [anon_sym_BANGff_BANG] = ACTIONS(379),
    [anon_sym_BANGfff_BANG] = ACTIONS(379),
    [anon_sym_BANGffff_BANG] = ACTIONS(379),
    [anon_sym_BANGsfz_BANG] = ACTIONS(379),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGsegno_BANG] = ACTIONS(379),
    [anon_sym_BANGcoda_BANG] = ACTIONS(379),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(379),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(379),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(379),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(379),
    [anon_sym_BANGfine_BANG] = ACTIONS(379),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(379),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(379),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(379),
  },
  [100] = {
    [sym_note_skip] = ACTIONS(379),
    [aux_sym_chord_symbol_token1] = ACTIONS(379),
    [sym_annotation] = ACTIONS(379),
    [sym_bar_line] = ACTIONS(379),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(377),
    [sym_thin_double_bar_line] = ACTIONS(379),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(379),
    [sym_start_of_repeated_section] = ACTIONS(379),
    [sym_end_of_repeated_section] = ACTIONS(379),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(379),
    [anon_sym_BANGtrill_BANG] = ACTIONS(379),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(379),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(379),
    [anon_sym_BANGmordent_BANG] = ACTIONS(379),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(379),
    [anon_sym_BANGroll_BANG] = ACTIONS(379),
    [anon_sym_BANGturn_BANG] = ACTIONS(379),
    [anon_sym_BANGturnx_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(379),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_BANG] = ACTIONS(379),
    [anon_sym_BANGaccent_BANG] = ACTIONS(379),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(379),
    [anon_sym_BANGfermata_BANG] = ACTIONS(379),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(379),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(379),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(379),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(379),
    [anon_sym_BANGplus_BANG] = ACTIONS(379),
    [anon_sym_BANGsnap_BANG] = ACTIONS(379),
    [anon_sym_BANGslide_BANG] = ACTIONS(379),
    [anon_sym_BANGwedge_BANG] = ACTIONS(379),
    [anon_sym_BANGupbow_BANG] = ACTIONS(379),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(379),
    [anon_sym_BANGopen_BANG] = ACTIONS(379),
    [anon_sym_BANGthumb_BANG] = ACTIONS(379),
    [anon_sym_BANGbreath_BANG] = ACTIONS(379),
    [anon_sym_BANGpppp_BANG] = ACTIONS(379),
    [anon_sym_BANGppp_BANG] = ACTIONS(379),
    [anon_sym_BANGpp_BANG] = ACTIONS(379),
    [anon_sym_BANGp_BANG] = ACTIONS(379),
    [anon_sym_BANGmp_BANG] = ACTIONS(379),
    [anon_sym_BANGmf_BANG] = ACTIONS(379),
    [anon_sym_BANGf_BANG] = ACTIONS(379),
    [anon_sym_BANGff_BANG] = ACTIONS(379),
    [anon_sym_BANGfff_BANG] = ACTIONS(379),
    [anon_sym_BANGffff_BANG] = ACTIONS(379),
    [anon_sym_BANGsfz_BANG] = ACTIONS(379),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(379),
    [anon_sym_BANGsegno_BANG] = ACTIONS(379),
    [anon_sym_BANGcoda_BANG] = ACTIONS(379),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(379),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(379),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(379),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(379),
    [anon_sym_BANGfine_BANG] = ACTIONS(379),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(379),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(379),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(379),
    [aux_sym_body_inline_info_token1] = ACTIONS(379),
  },
  [101] = {
    [sym_note_skip] = ACTIONS(387),
    [aux_sym_chord_symbol_token1] = ACTIONS(387),
    [sym_annotation] = ACTIONS(387),
    [sym_bar_line] = ACTIONS(387),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(385),
    [sym_thin_double_bar_line] = ACTIONS(387),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(387),
    [sym_start_of_repeated_section] = ACTIONS(387),
    [sym_end_of_repeated_section] = ACTIONS(387),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(387),
    [anon_sym_BANGtrill_BANG] = ACTIONS(387),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(387),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(387),
    [anon_sym_BANGmordent_BANG] = ACTIONS(387),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(387),
    [anon_sym_BANGroll_BANG] = ACTIONS(387),
    [anon_sym_BANGturn_BANG] = ACTIONS(387),
    [anon_sym_BANGturnx_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(387),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_BANG] = ACTIONS(387),
    [anon_sym_BANGaccent_BANG] = ACTIONS(387),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(387),
    [anon_sym_BANGfermata_BANG] = ACTIONS(387),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(387),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(387),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(387),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(387),
    [anon_sym_BANGplus_BANG] = ACTIONS(387),
    [anon_sym_BANGsnap_BANG] = ACTIONS(387),
    [anon_sym_BANGslide_BANG] = ACTIONS(387),
    [anon_sym_BANGwedge_BANG] = ACTIONS(387),
    [anon_sym_BANGupbow_BANG] = ACTIONS(387),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(387),
    [anon_sym_BANGopen_BANG] = ACTIONS(387),
    [anon_sym_BANGthumb_BANG] = ACTIONS(387),
    [anon_sym_BANGbreath_BANG] = ACTIONS(387),
    [anon_sym_BANGpppp_BANG] = ACTIONS(387),
    [anon_sym_BANGppp_BANG] = ACTIONS(387),
    [anon_sym_BANGpp_BANG] = ACTIONS(387),
    [anon_sym_BANGp_BANG] = ACTIONS(387),
    [anon_sym_BANGmp_BANG] = ACTIONS(387),
    [anon_sym_BANGmf_BANG] = ACTIONS(387),
    [anon_sym_BANGf_BANG] = ACTIONS(387),
    [anon_sym_BANGff_BANG] = ACTIONS(387),
    [anon_sym_BANGfff_BANG] = ACTIONS(387),
    [anon_sym_BANGffff_BANG] = ACTIONS(387),
    [anon_sym_BANGsfz_BANG] = ACTIONS(387),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(387),
    [anon_sym_BANGsegno_BANG] = ACTIONS(387),
    [anon_sym_BANGcoda_BANG] = ACTIONS(387),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(387),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(387),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(387),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(387),
    [anon_sym_BANGfine_BANG] = ACTIONS(387),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(387),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(387),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(387),
    [aux_sym_body_inline_info_token1] = ACTIONS(387),
  },
  [102] = {
    [sym_noCommentText] = ACTIONS(405),
    [sym_note_skip] = ACTIONS(405),
    [aux_sym_chord_symbol_token1] = ACTIONS(405),
    [sym_annotation] = ACTIONS(405),
    [sym_bar_line] = ACTIONS(405),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(405),
    [sym_thin_double_bar_line] = ACTIONS(405),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(405),
    [sym_start_of_repeated_section] = ACTIONS(405),
    [sym_end_of_repeated_section] = ACTIONS(405),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(405),
    [anon_sym_BANGtrill_BANG] = ACTIONS(405),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(405),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(405),
    [anon_sym_BANGmordent_BANG] = ACTIONS(405),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(405),
    [anon_sym_BANGroll_BANG] = ACTIONS(405),
    [anon_sym_BANGturn_BANG] = ACTIONS(405),
    [anon_sym_BANGturnx_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(405),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_BANG] = ACTIONS(405),
    [anon_sym_BANGaccent_BANG] = ACTIONS(405),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(405),
    [anon_sym_BANGfermata_BANG] = ACTIONS(405),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(405),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(405),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(405),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(405),
    [anon_sym_BANGplus_BANG] = ACTIONS(405),
    [anon_sym_BANGsnap_BANG] = ACTIONS(405),
    [anon_sym_BANGslide_BANG] = ACTIONS(405),
    [anon_sym_BANGwedge_BANG] = ACTIONS(405),
    [anon_sym_BANGupbow_BANG] = ACTIONS(405),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(405),
    [anon_sym_BANGopen_BANG] = ACTIONS(405),
    [anon_sym_BANGthumb_BANG] = ACTIONS(405),
    [anon_sym_BANGbreath_BANG] = ACTIONS(405),
    [anon_sym_BANGpppp_BANG] = ACTIONS(405),
    [anon_sym_BANGppp_BANG] = ACTIONS(405),
    [anon_sym_BANGpp_BANG] = ACTIONS(405),
    [anon_sym_BANGp_BANG] = ACTIONS(405),
    [anon_sym_BANGmp_BANG] = ACTIONS(405),
    [anon_sym_BANGmf_BANG] = ACTIONS(405),
    [anon_sym_BANGf_BANG] = ACTIONS(405),
    [anon_sym_BANGff_BANG] = ACTIONS(405),
    [anon_sym_BANGfff_BANG] = ACTIONS(405),
    [anon_sym_BANGffff_BANG] = ACTIONS(405),
    [anon_sym_BANGsfz_BANG] = ACTIONS(405),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(405),
    [anon_sym_BANGsegno_BANG] = ACTIONS(405),
    [anon_sym_BANGcoda_BANG] = ACTIONS(405),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(405),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(405),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(405),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(405),
    [anon_sym_BANGfine_BANG] = ACTIONS(405),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(405),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(405),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(405),
  },
  [103] = {
    [sym_note_skip] = ACTIONS(566),
    [aux_sym_chord_symbol_token1] = ACTIONS(566),
    [sym_annotation] = ACTIONS(566),
    [sym_bar_line] = ACTIONS(566),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(570),
    [sym_thin_double_bar_line] = ACTIONS(566),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(566),
    [sym_start_of_repeated_section] = ACTIONS(566),
    [sym_end_of_repeated_section] = ACTIONS(566),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(566),
    [anon_sym_BANGtrill_BANG] = ACTIONS(566),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(566),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(566),
    [anon_sym_BANGmordent_BANG] = ACTIONS(566),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(566),
    [anon_sym_BANGroll_BANG] = ACTIONS(566),
    [anon_sym_BANGturn_BANG] = ACTIONS(566),
    [anon_sym_BANGturnx_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(566),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_BANG] = ACTIONS(566),
    [anon_sym_BANGaccent_BANG] = ACTIONS(566),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(566),
    [anon_sym_BANGfermata_BANG] = ACTIONS(566),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(566),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(566),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(566),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(566),
    [anon_sym_BANGplus_BANG] = ACTIONS(566),
    [anon_sym_BANGsnap_BANG] = ACTIONS(566),
    [anon_sym_BANGslide_BANG] = ACTIONS(566),
    [anon_sym_BANGwedge_BANG] = ACTIONS(566),
    [anon_sym_BANGupbow_BANG] = ACTIONS(566),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(566),
    [anon_sym_BANGopen_BANG] = ACTIONS(566),
    [anon_sym_BANGthumb_BANG] = ACTIONS(566),
    [anon_sym_BANGbreath_BANG] = ACTIONS(566),
    [anon_sym_BANGpppp_BANG] = ACTIONS(566),
    [anon_sym_BANGppp_BANG] = ACTIONS(566),
    [anon_sym_BANGpp_BANG] = ACTIONS(566),
    [anon_sym_BANGp_BANG] = ACTIONS(566),
    [anon_sym_BANGmp_BANG] = ACTIONS(566),
    [anon_sym_BANGmf_BANG] = ACTIONS(566),
    [anon_sym_BANGf_BANG] = ACTIONS(566),
    [anon_sym_BANGff_BANG] = ACTIONS(566),
    [anon_sym_BANGfff_BANG] = ACTIONS(566),
    [anon_sym_BANGffff_BANG] = ACTIONS(566),
    [anon_sym_BANGsfz_BANG] = ACTIONS(566),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(566),
    [anon_sym_BANGsegno_BANG] = ACTIONS(566),
    [anon_sym_BANGcoda_BANG] = ACTIONS(566),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(566),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(566),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(566),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(566),
    [anon_sym_BANGfine_BANG] = ACTIONS(566),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(566),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(566),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(566),
    [aux_sym_body_inline_info_token1] = ACTIONS(566),
  },
  [104] = {
    [sym_note_skip] = ACTIONS(375),
    [aux_sym_chord_symbol_token1] = ACTIONS(375),
    [sym_annotation] = ACTIONS(375),
    [sym_bar_line] = ACTIONS(375),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(373),
    [sym_thin_double_bar_line] = ACTIONS(375),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(375),
    [sym_start_of_repeated_section] = ACTIONS(375),
    [sym_end_of_repeated_section] = ACTIONS(375),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(375),
    [anon_sym_BANGtrill_BANG] = ACTIONS(375),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(375),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(375),
    [anon_sym_BANGmordent_BANG] = ACTIONS(375),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(375),
    [anon_sym_BANGroll_BANG] = ACTIONS(375),
    [anon_sym_BANGturn_BANG] = ACTIONS(375),
    [anon_sym_BANGturnx_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(375),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_BANG] = ACTIONS(375),
    [anon_sym_BANGaccent_BANG] = ACTIONS(375),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(375),
    [anon_sym_BANGfermata_BANG] = ACTIONS(375),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(375),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(375),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(375),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(375),
    [anon_sym_BANGplus_BANG] = ACTIONS(375),
    [anon_sym_BANGsnap_BANG] = ACTIONS(375),
    [anon_sym_BANGslide_BANG] = ACTIONS(375),
    [anon_sym_BANGwedge_BANG] = ACTIONS(375),
    [anon_sym_BANGupbow_BANG] = ACTIONS(375),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(375),
    [anon_sym_BANGopen_BANG] = ACTIONS(375),
    [anon_sym_BANGthumb_BANG] = ACTIONS(375),
    [anon_sym_BANGbreath_BANG] = ACTIONS(375),
    [anon_sym_BANGpppp_BANG] = ACTIONS(375),
    [anon_sym_BANGppp_BANG] = ACTIONS(375),
    [anon_sym_BANGpp_BANG] = ACTIONS(375),
    [anon_sym_BANGp_BANG] = ACTIONS(375),
    [anon_sym_BANGmp_BANG] = ACTIONS(375),
    [anon_sym_BANGmf_BANG] = ACTIONS(375),
    [anon_sym_BANGf_BANG] = ACTIONS(375),
    [anon_sym_BANGff_BANG] = ACTIONS(375),
    [anon_sym_BANGfff_BANG] = ACTIONS(375),
    [anon_sym_BANGffff_BANG] = ACTIONS(375),
    [anon_sym_BANGsfz_BANG] = ACTIONS(375),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(375),
    [anon_sym_BANGsegno_BANG] = ACTIONS(375),
    [anon_sym_BANGcoda_BANG] = ACTIONS(375),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(375),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(375),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(375),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(375),
    [anon_sym_BANGfine_BANG] = ACTIONS(375),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(375),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(375),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(375),
    [aux_sym_body_inline_info_token1] = ACTIONS(375),
  },
  [105] = {
    [sym_note_skip] = ACTIONS(383),
    [aux_sym_chord_symbol_token1] = ACTIONS(383),
    [sym_annotation] = ACTIONS(383),
    [sym_bar_line] = ACTIONS(383),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(381),
    [sym_thin_double_bar_line] = ACTIONS(383),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(383),
    [sym_start_of_repeated_section] = ACTIONS(383),
    [sym_end_of_repeated_section] = ACTIONS(383),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(383),
    [anon_sym_BANGtrill_BANG] = ACTIONS(383),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(383),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(383),
    [anon_sym_BANGmordent_BANG] = ACTIONS(383),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(383),
    [anon_sym_BANGroll_BANG] = ACTIONS(383),
    [anon_sym_BANGturn_BANG] = ACTIONS(383),
    [anon_sym_BANGturnx_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(383),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_BANG] = ACTIONS(383),
    [anon_sym_BANGaccent_BANG] = ACTIONS(383),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(383),
    [anon_sym_BANGfermata_BANG] = ACTIONS(383),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(383),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(383),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(383),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(383),
    [anon_sym_BANGplus_BANG] = ACTIONS(383),
    [anon_sym_BANGsnap_BANG] = ACTIONS(383),
    [anon_sym_BANGslide_BANG] = ACTIONS(383),
    [anon_sym_BANGwedge_BANG] = ACTIONS(383),
    [anon_sym_BANGupbow_BANG] = ACTIONS(383),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(383),
    [anon_sym_BANGopen_BANG] = ACTIONS(383),
    [anon_sym_BANGthumb_BANG] = ACTIONS(383),
    [anon_sym_BANGbreath_BANG] = ACTIONS(383),
    [anon_sym_BANGpppp_BANG] = ACTIONS(383),
    [anon_sym_BANGppp_BANG] = ACTIONS(383),
    [anon_sym_BANGpp_BANG] = ACTIONS(383),
    [anon_sym_BANGp_BANG] = ACTIONS(383),
    [anon_sym_BANGmp_BANG] = ACTIONS(383),
    [anon_sym_BANGmf_BANG] = ACTIONS(383),
    [anon_sym_BANGf_BANG] = ACTIONS(383),
    [anon_sym_BANGff_BANG] = ACTIONS(383),
    [anon_sym_BANGfff_BANG] = ACTIONS(383),
    [anon_sym_BANGffff_BANG] = ACTIONS(383),
    [anon_sym_BANGsfz_BANG] = ACTIONS(383),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(383),
    [anon_sym_BANGsegno_BANG] = ACTIONS(383),
    [anon_sym_BANGcoda_BANG] = ACTIONS(383),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(383),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(383),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(383),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(383),
    [anon_sym_BANGfine_BANG] = ACTIONS(383),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(383),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(383),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(383),
    [aux_sym_body_inline_info_token1] = ACTIONS(383),
  },
  [106] = {
    [sym_note_skip] = ACTIONS(572),
    [aux_sym_chord_symbol_token1] = ACTIONS(572),
    [sym_annotation] = ACTIONS(572),
    [sym_bar_line] = ACTIONS(572),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(574),
    [sym_thin_double_bar_line] = ACTIONS(572),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(572),
    [sym_start_of_repeated_section] = ACTIONS(572),
    [sym_end_of_repeated_section] = ACTIONS(572),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(572),
    [anon_sym_BANGtrill_BANG] = ACTIONS(572),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(572),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(572),
    [anon_sym_BANGmordent_BANG] = ACTIONS(572),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(572),
    [anon_sym_BANGroll_BANG] = ACTIONS(572),
    [anon_sym_BANGturn_BANG] = ACTIONS(572),
    [anon_sym_BANGturnx_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(572),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_BANG] = ACTIONS(572),
    [anon_sym_BANGaccent_BANG] = ACTIONS(572),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(572),
    [anon_sym_BANGfermata_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(572),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(572),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(572),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(572),
    [anon_sym_BANGplus_BANG] = ACTIONS(572),
    [anon_sym_BANGsnap_BANG] = ACTIONS(572),
    [anon_sym_BANGslide_BANG] = ACTIONS(572),
    [anon_sym_BANGwedge_BANG] = ACTIONS(572),
    [anon_sym_BANGupbow_BANG] = ACTIONS(572),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(572),
    [anon_sym_BANGopen_BANG] = ACTIONS(572),
    [anon_sym_BANGthumb_BANG] = ACTIONS(572),
    [anon_sym_BANGbreath_BANG] = ACTIONS(572),
    [anon_sym_BANGpppp_BANG] = ACTIONS(572),
    [anon_sym_BANGppp_BANG] = ACTIONS(572),
    [anon_sym_BANGpp_BANG] = ACTIONS(572),
    [anon_sym_BANGp_BANG] = ACTIONS(572),
    [anon_sym_BANGmp_BANG] = ACTIONS(572),
    [anon_sym_BANGmf_BANG] = ACTIONS(572),
    [anon_sym_BANGf_BANG] = ACTIONS(572),
    [anon_sym_BANGff_BANG] = ACTIONS(572),
    [anon_sym_BANGfff_BANG] = ACTIONS(572),
    [anon_sym_BANGffff_BANG] = ACTIONS(572),
    [anon_sym_BANGsfz_BANG] = ACTIONS(572),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGsegno_BANG] = ACTIONS(572),
    [anon_sym_BANGcoda_BANG] = ACTIONS(572),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(572),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(572),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(572),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(572),
    [anon_sym_BANGfine_BANG] = ACTIONS(572),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(572),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(572),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(572),
    [aux_sym_body_inline_info_token1] = ACTIONS(572),
  },
  [107] = {
    [sym_note_skip] = ACTIONS(397),
    [aux_sym_chord_symbol_token1] = ACTIONS(397),
    [sym_annotation] = ACTIONS(397),
    [sym_bar_line] = ACTIONS(397),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(395),
    [sym_thin_double_bar_line] = ACTIONS(397),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(397),
    [sym_start_of_repeated_section] = ACTIONS(397),
    [sym_end_of_repeated_section] = ACTIONS(397),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(397),
    [anon_sym_BANGtrill_BANG] = ACTIONS(397),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(397),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(397),
    [anon_sym_BANGmordent_BANG] = ACTIONS(397),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(397),
    [anon_sym_BANGroll_BANG] = ACTIONS(397),
    [anon_sym_BANGturn_BANG] = ACTIONS(397),
    [anon_sym_BANGturnx_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(397),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_BANG] = ACTIONS(397),
    [anon_sym_BANGaccent_BANG] = ACTIONS(397),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(397),
    [anon_sym_BANGfermata_BANG] = ACTIONS(397),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(397),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(397),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(397),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(397),
    [anon_sym_BANGplus_BANG] = ACTIONS(397),
    [anon_sym_BANGsnap_BANG] = ACTIONS(397),
    [anon_sym_BANGslide_BANG] = ACTIONS(397),
    [anon_sym_BANGwedge_BANG] = ACTIONS(397),
    [anon_sym_BANGupbow_BANG] = ACTIONS(397),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(397),
    [anon_sym_BANGopen_BANG] = ACTIONS(397),
    [anon_sym_BANGthumb_BANG] = ACTIONS(397),
    [anon_sym_BANGbreath_BANG] = ACTIONS(397),
    [anon_sym_BANGpppp_BANG] = ACTIONS(397),
    [anon_sym_BANGppp_BANG] = ACTIONS(397),
    [anon_sym_BANGpp_BANG] = ACTIONS(397),
    [anon_sym_BANGp_BANG] = ACTIONS(397),
    [anon_sym_BANGmp_BANG] = ACTIONS(397),
    [anon_sym_BANGmf_BANG] = ACTIONS(397),
    [anon_sym_BANGf_BANG] = ACTIONS(397),
    [anon_sym_BANGff_BANG] = ACTIONS(397),
    [anon_sym_BANGfff_BANG] = ACTIONS(397),
    [anon_sym_BANGffff_BANG] = ACTIONS(397),
    [anon_sym_BANGsfz_BANG] = ACTIONS(397),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(397),
    [anon_sym_BANGsegno_BANG] = ACTIONS(397),
    [anon_sym_BANGcoda_BANG] = ACTIONS(397),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(397),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(397),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(397),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(397),
    [anon_sym_BANGfine_BANG] = ACTIONS(397),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(397),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(397),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(397),
    [aux_sym_body_inline_info_token1] = ACTIONS(397),
  },
  [108] = {
    [sym_noCommentText] = ACTIONS(572),
    [sym_note_skip] = ACTIONS(572),
    [aux_sym_chord_symbol_token1] = ACTIONS(572),
    [sym_annotation] = ACTIONS(572),
    [sym_bar_line] = ACTIONS(572),
    [sym_close_thin_thick_double_bar_line] = ACTIONS(572),
    [sym_thin_double_bar_line] = ACTIONS(572),
    [sym_open_thick_thin_double_bar_line] = ACTIONS(572),
    [sym_start_of_repeated_section] = ACTIONS(572),
    [sym_end_of_repeated_section] = ACTIONS(572),
    [sym_start_end_of_two_repeated_sections] = ACTIONS(572),
    [anon_sym_BANGtrill_BANG] = ACTIONS(572),
    [anon_sym_BANGtrill_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGtrill_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGlowermordent_BANG] = ACTIONS(572),
    [anon_sym_BANGuppermordent_BANG] = ACTIONS(572),
    [anon_sym_BANGmordent_BANG] = ACTIONS(572),
    [anon_sym_BANGpralltriller_BANG] = ACTIONS(572),
    [anon_sym_BANGroll_BANG] = ACTIONS(572),
    [anon_sym_BANGturn_BANG] = ACTIONS(572),
    [anon_sym_BANGturnx_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedturn_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedturnx_BANG] = ACTIONS(572),
    [anon_sym_BANGarpeggio_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_BANG] = ACTIONS(572),
    [anon_sym_BANGaccent_BANG] = ACTIONS(572),
    [anon_sym_BANGemphasis_BANG] = ACTIONS(572),
    [anon_sym_BANGfermata_BANG] = ACTIONS(572),
    [anon_sym_BANGinvertedfermata_BANG] = ACTIONS(572),
    [anon_sym_BANGtenuto_BANG] = ACTIONS(572),
    [anon_sym_BANG0_BANG_DASH_BANG5_BANG] = ACTIONS(572),
    [anon_sym_BANG_PLUS_BANG] = ACTIONS(572),
    [anon_sym_BANGplus_BANG] = ACTIONS(572),
    [anon_sym_BANGsnap_BANG] = ACTIONS(572),
    [anon_sym_BANGslide_BANG] = ACTIONS(572),
    [anon_sym_BANGwedge_BANG] = ACTIONS(572),
    [anon_sym_BANGupbow_BANG] = ACTIONS(572),
    [anon_sym_BANGdownbow_BANG] = ACTIONS(572),
    [anon_sym_BANGopen_BANG] = ACTIONS(572),
    [anon_sym_BANGthumb_BANG] = ACTIONS(572),
    [anon_sym_BANGbreath_BANG] = ACTIONS(572),
    [anon_sym_BANGpppp_BANG] = ACTIONS(572),
    [anon_sym_BANGppp_BANG] = ACTIONS(572),
    [anon_sym_BANGpp_BANG] = ACTIONS(572),
    [anon_sym_BANGp_BANG] = ACTIONS(572),
    [anon_sym_BANGmp_BANG] = ACTIONS(572),
    [anon_sym_BANGmf_BANG] = ACTIONS(572),
    [anon_sym_BANGf_BANG] = ACTIONS(572),
    [anon_sym_BANGff_BANG] = ACTIONS(572),
    [anon_sym_BANGfff_BANG] = ACTIONS(572),
    [anon_sym_BANGffff_BANG] = ACTIONS(572),
    [anon_sym_BANGsfz_BANG] = ACTIONS(572),
    [anon_sym_BANGcrescendo_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_LT_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGcrescendo_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_LT_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGdiminuendo_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_LPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGdiminuendo_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANG_GT_RPAREN_BANG] = ACTIONS(572),
    [anon_sym_BANGsegno_BANG] = ACTIONS(572),
    [anon_sym_BANGcoda_BANG] = ACTIONS(572),
    [anon_sym_BANGD_DOTS_DOT_BANG] = ACTIONS(572),
    [anon_sym_BANGD_DOTC_DOT_BANG] = ACTIONS(572),
    [anon_sym_BANGdacoda_BANG] = ACTIONS(572),
    [anon_sym_BANGdacapo_BANG] = ACTIONS(572),
    [anon_sym_BANGfine_BANG] = ACTIONS(572),
    [anon_sym_BANGshortphrase_BANG] = ACTIONS(572),
    [anon_sym_BANGmediumphrase_BANG] = ACTIONS(572),
    [anon_sym_BANGlongphrase_BANG] = ACTIONS(572),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 2,
    STATE(302), 1,
      sym_symbol,
    ACTIONS(576), 59,
      anon_sym_BANGtrill_BANG,
      anon_sym_BANGtrill_LPAREN_BANG,
      anon_sym_BANGtrill_RPAREN_BANG,
      anon_sym_BANGlowermordent_BANG,
      anon_sym_BANGuppermordent_BANG,
      anon_sym_BANGmordent_BANG,
      anon_sym_BANGpralltriller_BANG,
      anon_sym_BANGroll_BANG,
      anon_sym_BANGturn_BANG,
      anon_sym_BANGturnx_BANG,
      anon_sym_BANGinvertedturn_BANG,
      anon_sym_BANGinvertedturnx_BANG,
      anon_sym_BANGarpeggio_BANG,
      anon_sym_BANG_GT_BANG,
      anon_sym_BANGaccent_BANG,
      anon_sym_BANGemphasis_BANG,
      anon_sym_BANGfermata_BANG,
      anon_sym_BANGinvertedfermata_BANG,
      anon_sym_BANGtenuto_BANG,
      anon_sym_BANG0_BANG_DASH_BANG5_BANG,
      anon_sym_BANG_PLUS_BANG,
      anon_sym_BANGplus_BANG,
      anon_sym_BANGsnap_BANG,
      anon_sym_BANGslide_BANG,
      anon_sym_BANGwedge_BANG,
      anon_sym_BANGupbow_BANG,
      anon_sym_BANGdownbow_BANG,
      anon_sym_BANGopen_BANG,
      anon_sym_BANGthumb_BANG,
      anon_sym_BANGbreath_BANG,
      anon_sym_BANGpppp_BANG,
      anon_sym_BANGppp_BANG,
      anon_sym_BANGpp_BANG,
      anon_sym_BANGp_BANG,
      anon_sym_BANGmp_BANG,
      anon_sym_BANGmf_BANG,
      anon_sym_BANGf_BANG,
      anon_sym_BANGff_BANG,
      anon_sym_BANGfff_BANG,
      anon_sym_BANGffff_BANG,
      anon_sym_BANGsfz_BANG,
      anon_sym_BANGcrescendo_LPAREN_BANG,
      anon_sym_BANG_LT_LPAREN_BANG,
      anon_sym_BANGcrescendo_RPAREN_BANG,
      anon_sym_BANG_LT_RPAREN_BANG,
      anon_sym_BANGdiminuendo_LPAREN_BANG,
      anon_sym_BANG_GT_LPAREN_BANG,
      anon_sym_BANGdiminuendo_RPAREN_BANG,
      anon_sym_BANG_GT_RPAREN_BANG,
      anon_sym_BANGsegno_BANG,
      anon_sym_BANGcoda_BANG,
      anon_sym_BANGD_DOTS_DOT_BANG,
      anon_sym_BANGD_DOTC_DOT_BANG,
      anon_sym_BANGdacoda_BANG,
      anon_sym_BANGdacapo_BANG,
      anon_sym_BANGfine_BANG,
      anon_sym_BANGshortphrase_BANG,
      anon_sym_BANGmediumphrase_BANG,
      anon_sym_BANGlongphrase_BANG,
  [65] = 2,
    STATE(255), 1,
      sym_symbol,
    ACTIONS(578), 59,
      anon_sym_BANGtrill_BANG,
      anon_sym_BANGtrill_LPAREN_BANG,
      anon_sym_BANGtrill_RPAREN_BANG,
      anon_sym_BANGlowermordent_BANG,
      anon_sym_BANGuppermordent_BANG,
      anon_sym_BANGmordent_BANG,
      anon_sym_BANGpralltriller_BANG,
      anon_sym_BANGroll_BANG,
      anon_sym_BANGturn_BANG,
      anon_sym_BANGturnx_BANG,
      anon_sym_BANGinvertedturn_BANG,
      anon_sym_BANGinvertedturnx_BANG,
      anon_sym_BANGarpeggio_BANG,
      anon_sym_BANG_GT_BANG,
      anon_sym_BANGaccent_BANG,
      anon_sym_BANGemphasis_BANG,
      anon_sym_BANGfermata_BANG,
      anon_sym_BANGinvertedfermata_BANG,
      anon_sym_BANGtenuto_BANG,
      anon_sym_BANG0_BANG_DASH_BANG5_BANG,
      anon_sym_BANG_PLUS_BANG,
      anon_sym_BANGplus_BANG,
      anon_sym_BANGsnap_BANG,
      anon_sym_BANGslide_BANG,
      anon_sym_BANGwedge_BANG,
      anon_sym_BANGupbow_BANG,
      anon_sym_BANGdownbow_BANG,
      anon_sym_BANGopen_BANG,
      anon_sym_BANGthumb_BANG,
      anon_sym_BANGbreath_BANG,
      anon_sym_BANGpppp_BANG,
      anon_sym_BANGppp_BANG,
      anon_sym_BANGpp_BANG,
      anon_sym_BANGp_BANG,
      anon_sym_BANGmp_BANG,
      anon_sym_BANGmf_BANG,
      anon_sym_BANGf_BANG,
      anon_sym_BANGff_BANG,
      anon_sym_BANGfff_BANG,
      anon_sym_BANGffff_BANG,
      anon_sym_BANGsfz_BANG,
      anon_sym_BANGcrescendo_LPAREN_BANG,
      anon_sym_BANG_LT_LPAREN_BANG,
      anon_sym_BANGcrescendo_RPAREN_BANG,
      anon_sym_BANG_LT_RPAREN_BANG,
      anon_sym_BANGdiminuendo_LPAREN_BANG,
      anon_sym_BANG_GT_LPAREN_BANG,
      anon_sym_BANGdiminuendo_RPAREN_BANG,
      anon_sym_BANG_GT_RPAREN_BANG,
      anon_sym_BANGsegno_BANG,
      anon_sym_BANGcoda_BANG,
      anon_sym_BANGD_DOTS_DOT_BANG,
      anon_sym_BANGD_DOTC_DOT_BANG,
      anon_sym_BANGdacoda_BANG,
      anon_sym_BANGdacapo_BANG,
      anon_sym_BANGfine_BANG,
      anon_sym_BANGshortphrase_BANG,
      anon_sym_BANGmediumphrase_BANG,
      anon_sym_BANGlongphrase_BANG,
  [130] = 9,
    ACTIONS(580), 1,
      sym_COMMENT,
    ACTIONS(583), 1,
      anon_sym_PERCENT_PERCENT,
    ACTIONS(586), 1,
      anon_sym_PERCENTabc_DASH,
    ACTIONS(592), 1,
      anon_sym_X_COLON,
    ACTIONS(594), 1,
      anon_sym_U_COLON,
    STATE(111), 1,
      aux_sym_file_header_repeat1,
    STATE(242), 2,
      sym_stylesheet_directives,
      sym_file_header_info_line,
    STATE(279), 2,
      sym_abc_version,
      sym_user_defined,
    ACTIONS(589), 17,
      sym_area,
      sym_book,
      sym_composer,
      sym_discography,
      sym_file,
      sym_group,
      sym_history,
      sym_instruction,
      sym_macros,
      sym_meter,
      sym_notes,
      sym_origin,
      sym_remark,
      sym_rhythm_line,
      sym_source,
      sym_transcription,
      sym_unit_note_length,
  [176] = 9,
    ACTIONS(3), 1,
      sym_COMMENT,
    ACTIONS(5), 1,
      anon_sym_PERCENT_PERCENT,
    ACTIONS(7), 1,
      anon_sym_PERCENTabc_DASH,
    ACTIONS(13), 1,
      anon_sym_U_COLON,
    ACTIONS(597), 1,
      anon_sym_X_COLON,
    STATE(111), 1,
      aux_sym_file_header_repeat1,
    STATE(242), 2,
      sym_stylesheet_directives,
      sym_file_header_info_line,
    STATE(279), 2,
      sym_abc_version,
      sym_user_defined,
    ACTIONS(9), 17,
      sym_area,
      sym_book,
      sym_composer,
      sym_discography,
      sym_file,
      sym_group,
      sym_history,
      sym_instruction,
      sym_macros,
      sym_meter,
      sym_notes,
      sym_origin,
      sym_remark,
      sym_rhythm_line,
      sym_source,
      sym_transcription,
      sym_unit_note_length,
  [222] = 4,
    ACTIONS(601), 1,
      sym__NL,
    STATE(113), 1,
      aux_sym_file_structure_repeat1,
    ACTIONS(604), 2,
      sym_COMMENT,
      anon_sym_PERCENTabc_DASH,
    ACTIONS(599), 22,
      ts_builtin_sym_end,
      anon_sym_PERCENT_PERCENT,
      sym_area,
      sym_book,
      sym_composer,
      sym_discography,
      sym_file,
      sym_group,
      sym_history,
      sym_instruction,
      sym_macros,
      sym_meter,
      sym_notes,
      sym_origin,
      anon_sym_X_COLON,
      sym_remark,
      sym_rhythm_line,
      sym_source,
      sym_transcription,
      sym_unit_note_length,
      anon_sym_U_COLON,
      sym_words_postbody,
  [257] = 4,
    ACTIONS(606), 1,
      sym__NL,
    STATE(113), 1,
      aux_sym_file_structure_repeat1,
    ACTIONS(608), 2,
      sym_COMMENT,
      anon_sym_PERCENTabc_DASH,
    ACTIONS(592), 20,
      anon_sym_PERCENT_PERCENT,
      sym_area,
      sym_book,
      sym_composer,
      sym_discography,
      sym_file,
      sym_group,
      sym_history,
      sym_instruction,
      sym_macros,
      sym_meter,
      sym_notes,
      sym_origin,
      anon_sym_X_COLON,
      sym_remark,
      sym_rhythm_line,
      sym_source,
      sym_transcription,
      sym_unit_note_length,
      anon_sym_U_COLON,
  [290] = 17,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(162), 1,
      sym_BEAM_SEPARATOR,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(612), 1,
      aux_sym__MUSIC_CODE_token1,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    ACTIONS(620), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(622), 1,
      sym_tuplet_marker,
    STATE(19), 1,
      sym__pitch,
    STATE(33), 1,
      sym_note,
    STATE(142), 1,
      sym_open_chord,
    STATE(169), 1,
      sym_chord_symbol,
    STATE(190), 1,
      sym_decoration,
    STATE(123), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
    STATE(8), 4,
      sym__nte_or_chrd,
      sym_note_construct,
      sym__chord_cstrct,
      aux_sym_beam_repeat1,
  [346] = 6,
    ACTIONS(626), 1,
      sym_decoration_shorthand,
    ACTIONS(630), 1,
      anon_sym_s_COLON,
    ACTIONS(632), 1,
      anon_sym_U_COLON,
    STATE(268), 2,
      sym_symbol_line,
      sym_user_defined,
    ACTIONS(624), 4,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
    ACTIONS(628), 11,
      sym_parts_line,
      sym_instruction,
      sym_key,
      sym_macros,
      sym_meter,
      sym_notes,
      sym_remark,
      sym_rhythm_line,
      sym_tempo,
      sym_unit_note_length,
      sym_voice,
  [379] = 15,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    ACTIONS(620), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(622), 1,
      sym_tuplet_marker,
    STATE(19), 1,
      sym__pitch,
    STATE(33), 1,
      sym_note,
    STATE(142), 1,
      sym_open_chord,
    STATE(169), 1,
      sym_chord_symbol,
    STATE(190), 1,
      sym_decoration,
    STATE(123), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
    STATE(39), 3,
      sym__nte_or_chrd,
      sym_note_construct,
      sym__chord_cstrct,
  [428] = 3,
    ACTIONS(634), 1,
      sym_octave,
    STATE(121), 1,
      aux_sym__pitch_repeat1,
    ACTIONS(240), 14,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_grace_note_token2,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [451] = 3,
    ACTIONS(636), 1,
      sym_octave,
    STATE(120), 1,
      aux_sym__pitch_repeat1,
    ACTIONS(240), 14,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_grace_note_token2,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [474] = 3,
    ACTIONS(638), 1,
      sym_octave,
    STATE(120), 1,
      aux_sym__pitch_repeat1,
    ACTIONS(233), 14,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_grace_note_token2,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [497] = 3,
    ACTIONS(636), 1,
      sym_octave,
    STATE(120), 1,
      aux_sym__pitch_repeat1,
    ACTIONS(248), 14,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_grace_note_token2,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [520] = 3,
    ACTIONS(641), 1,
      sym_octave,
    STATE(119), 1,
      aux_sym__pitch_repeat1,
    ACTIONS(252), 14,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_grace_note_token2,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [543] = 14,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    ACTIONS(620), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(643), 1,
      sym_tuplet_marker,
    STATE(19), 1,
      sym__pitch,
    STATE(37), 1,
      sym_note,
    STATE(146), 1,
      sym_open_chord,
    STATE(162), 1,
      sym_chord_symbol,
    STATE(184), 1,
      sym_decoration,
    STATE(163), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [587] = 14,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(620), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(645), 1,
      sym_tuplet_marker,
    STATE(135), 1,
      sym__pitch,
    STATE(141), 1,
      sym_open_chord,
    STATE(151), 1,
      sym_note,
    STATE(161), 1,
      sym_chord_symbol,
    STATE(188), 1,
      sym_decoration,
    STATE(163), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [631] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(647), 1,
      anon_sym_RBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(156), 1,
      sym_close_chord,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [670] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(647), 1,
      anon_sym_RBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(149), 1,
      sym_close_chord,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [709] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(647), 1,
      anon_sym_RBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(150), 1,
      sym_close_chord,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [748] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(647), 1,
      anon_sym_RBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(153), 1,
      sym_close_chord,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [787] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(647), 1,
      anon_sym_RBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(157), 1,
      sym_close_chord,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [826] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(649), 1,
      anon_sym_RBRACK,
    STATE(34), 1,
      sym_close_chord,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [865] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(649), 1,
      anon_sym_RBRACK,
    STATE(32), 1,
      sym_close_chord,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [904] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(649), 1,
      anon_sym_RBRACK,
    STATE(25), 1,
      sym_close_chord,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [943] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(649), 1,
      anon_sym_RBRACK,
    STATE(29), 1,
      sym_close_chord,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [982] = 12,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(649), 1,
      anon_sym_RBRACK,
    STATE(26), 1,
      sym_close_chord,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1021] = 2,
    ACTIONS(651), 1,
      sym_rhythm,
    ACTIONS(258), 12,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1039] = 11,
    ACTIONS(653), 1,
      anon_sym_RBRACK,
    ACTIONS(655), 1,
      sym_alteration,
    ACTIONS(658), 1,
      sym_note_letter,
    ACTIONS(661), 1,
      sym_rest,
    ACTIONS(664), 1,
      aux_sym_grace_note_token1,
    ACTIONS(667), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(136), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1075] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(134), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1108] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(127), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1141] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(129), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1174] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(125), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1207] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(128), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1240] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(133), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1273] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(126), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1306] = 1,
    ACTIONS(325), 12,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      anon_sym_RBRACK,
      aux_sym__MUSIC_CODE_token1,
      aux_sym_note_construct_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1321] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(132), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1354] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(131), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1387] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(192), 1,
      sym_note,
    STATE(206), 1,
      sym_decoration,
    STATE(130), 2,
      sym_chord_note,
      aux_sym__chord_cstrct_repeat1,
    STATE(168), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1420] = 1,
    ACTIONS(315), 11,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_rhythm,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1434] = 2,
    ACTIONS(670), 1,
      sym_rhythm,
    ACTIONS(303), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1450] = 2,
    ACTIONS(672), 1,
      sym_rhythm,
    ACTIONS(297), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1466] = 2,
    ACTIONS(674), 1,
      aux_sym_note_construct_token1,
    ACTIONS(359), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1482] = 2,
    ACTIONS(676), 1,
      aux_sym_note_construct_token1,
    ACTIONS(341), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1498] = 2,
    ACTIONS(678), 1,
      sym_rhythm,
    ACTIONS(335), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1514] = 2,
    ACTIONS(680), 1,
      aux_sym_note_construct_token1,
    ACTIONS(309), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1530] = 2,
    ACTIONS(682), 1,
      aux_sym_note_construct_token1,
    ACTIONS(353), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1546] = 2,
    ACTIONS(684), 1,
      sym_rhythm,
    ACTIONS(347), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1562] = 2,
    ACTIONS(686), 1,
      sym_rhythm,
    ACTIONS(319), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1578] = 2,
    ACTIONS(688), 1,
      aux_sym_note_construct_token1,
    ACTIONS(329), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1594] = 1,
    ACTIONS(329), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1607] = 1,
    ACTIONS(359), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1620] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(690), 1,
      sym_tuplet_marker,
    STATE(135), 1,
      sym__pitch,
    STATE(143), 1,
      sym_open_chord,
    STATE(154), 1,
      sym_note,
    STATE(187), 1,
      sym_decoration,
  [1651] = 10,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    ACTIONS(692), 1,
      sym_tuplet_marker,
    STATE(19), 1,
      sym__pitch,
    STATE(27), 1,
      sym_note,
    STATE(137), 1,
      sym_open_chord,
    STATE(191), 1,
      sym_decoration,
  [1682] = 3,
    ACTIONS(696), 1,
      aux_sym_grace_note_token1,
    STATE(163), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
    ACTIONS(694), 7,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1699] = 1,
    ACTIONS(303), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1712] = 1,
    ACTIONS(309), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1725] = 1,
    ACTIONS(335), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1738] = 1,
    ACTIONS(353), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1751] = 9,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(35), 1,
      aux_sym_grace_note_token1,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    STATE(135), 1,
      sym__pitch,
    STATE(199), 1,
      sym_note,
    STATE(209), 1,
      sym_decoration,
    STATE(163), 2,
      sym_grace_note,
      aux_sym_note_construct_repeat1,
  [1780] = 10,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    ACTIONS(643), 1,
      sym_tuplet_marker,
    STATE(19), 1,
      sym__pitch,
    STATE(37), 1,
      sym_note,
    STATE(146), 1,
      sym_open_chord,
    STATE(184), 1,
      sym_decoration,
  [1811] = 1,
    ACTIONS(369), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1824] = 1,
    ACTIONS(347), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1837] = 10,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(645), 1,
      sym_tuplet_marker,
    STATE(135), 1,
      sym__pitch,
    STATE(141), 1,
      sym_open_chord,
    STATE(151), 1,
      sym_note,
    STATE(188), 1,
      sym_decoration,
  [1868] = 1,
    ACTIONS(297), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1881] = 1,
    ACTIONS(365), 10,
      sym_BEAM_SEPARATOR,
      anon_sym_LBRACK,
      aux_sym__MUSIC_CODE_token1,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [1894] = 9,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(140), 1,
      sym_open_chord,
    STATE(158), 1,
      sym_note,
    STATE(185), 1,
      sym_decoration,
  [1922] = 9,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(37), 1,
      sym_note,
    STATE(146), 1,
      sym_open_chord,
    STATE(184), 1,
      sym_decoration,
  [1950] = 9,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(27), 1,
      sym_note,
    STATE(137), 1,
      sym_open_chord,
    STATE(191), 1,
      sym_decoration,
  [1978] = 9,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(143), 1,
      sym_open_chord,
    STATE(154), 1,
      sym_note,
    STATE(187), 1,
      sym_decoration,
  [2006] = 9,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(141), 1,
      sym_open_chord,
    STATE(151), 1,
      sym_note,
    STATE(188), 1,
      sym_decoration,
  [2034] = 9,
    ACTIONS(229), 1,
      sym_decoration_shorthand,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(31), 1,
      sym_note,
    STATE(147), 1,
      sym_open_chord,
    STATE(189), 1,
      sym_decoration,
  [2062] = 5,
    ACTIONS(231), 1,
      sym_words_postbody,
    STATE(183), 1,
      aux_sym_file_structure_repeat1,
    STATE(217), 1,
      sym_lyric_section,
    STATE(202), 2,
      sym_lyric_line,
      aux_sym_lyric_section_repeat1,
    ACTIONS(699), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2081] = 1,
    ACTIONS(701), 8,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      aux_sym_chord_symbol_token1,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [2092] = 5,
    ACTIONS(231), 1,
      sym_words_postbody,
    STATE(113), 1,
      aux_sym_file_structure_repeat1,
    STATE(216), 1,
      sym_lyric_section,
    STATE(202), 2,
      sym_lyric_line,
      aux_sym_lyric_section_repeat1,
    ACTIONS(703), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2111] = 7,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(27), 1,
      sym_note,
    STATE(137), 1,
      sym_open_chord,
  [2133] = 7,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(138), 1,
      sym_open_chord,
    STATE(155), 1,
      sym_note,
  [2155] = 7,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(141), 1,
      sym_open_chord,
    STATE(151), 1,
      sym_note,
  [2177] = 7,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(140), 1,
      sym_open_chord,
    STATE(158), 1,
      sym_note,
  [2199] = 7,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    STATE(135), 1,
      sym__pitch,
    STATE(143), 1,
      sym_open_chord,
    STATE(154), 1,
      sym_note,
  [2221] = 7,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(35), 1,
      sym_note,
    STATE(145), 1,
      sym_open_chord,
  [2243] = 7,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(37), 1,
      sym_note,
    STATE(146), 1,
      sym_open_chord,
  [2265] = 7,
    ACTIONS(610), 1,
      anon_sym_LBRACK,
    ACTIONS(614), 1,
      sym_alteration,
    ACTIONS(616), 1,
      sym_note_letter,
    ACTIONS(618), 1,
      sym_rest,
    STATE(19), 1,
      sym__pitch,
    STATE(31), 1,
      sym_note,
    STATE(147), 1,
      sym_open_chord,
  [2287] = 1,
    ACTIONS(705), 6,
      anon_sym_RBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      sym_decoration_shorthand,
  [2296] = 6,
    ACTIONS(11), 1,
      anon_sym_X_COLON,
    ACTIONS(707), 1,
      ts_builtin_sym_end,
    STATE(7), 1,
      sym_reference_number_line,
    STATE(13), 1,
      sym_tune_header,
    STATE(200), 1,
      aux_sym_file_structure_repeat2,
    STATE(210), 1,
      sym_tune,
  [2315] = 6,
    ACTIONS(11), 1,
      anon_sym_X_COLON,
    ACTIONS(709), 1,
      ts_builtin_sym_end,
    STATE(7), 1,
      sym_reference_number_line,
    STATE(13), 1,
      sym_tune_header,
    STATE(200), 1,
      aux_sym_file_structure_repeat2,
    STATE(210), 1,
      sym_tune,
  [2334] = 1,
    ACTIONS(711), 6,
      anon_sym_RBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      sym_decoration_shorthand,
  [2343] = 1,
    ACTIONS(568), 6,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [2352] = 1,
    ACTIONS(574), 6,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [2361] = 1,
    ACTIONS(570), 6,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [2370] = 1,
    ACTIONS(713), 6,
      anon_sym_RBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      sym_decoration_shorthand,
  [2379] = 6,
    ACTIONS(715), 1,
      ts_builtin_sym_end,
    ACTIONS(717), 1,
      anon_sym_X_COLON,
    STATE(7), 1,
      sym_reference_number_line,
    STATE(13), 1,
      sym_tune_header,
    STATE(200), 1,
      aux_sym_file_structure_repeat2,
    STATE(210), 1,
      sym_tune,
  [2398] = 1,
    ACTIONS(564), 6,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      sym_tuplet_marker,
      sym_decoration_shorthand,
  [2407] = 3,
    ACTIONS(231), 1,
      sym_words_postbody,
    STATE(203), 2,
      sym_lyric_line,
      aux_sym_lyric_section_repeat1,
    ACTIONS(720), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2420] = 3,
    ACTIONS(724), 1,
      sym_words_postbody,
    STATE(203), 2,
      sym_lyric_line,
      aux_sym_lyric_section_repeat1,
    ACTIONS(722), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2433] = 1,
    ACTIONS(624), 5,
      sym_alteration,
      sym_note_letter,
      sym_rest,
      aux_sym_grace_note_token1,
      sym_decoration_shorthand,
  [2441] = 4,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(727), 1,
      aux_sym_grace_note_token2,
    STATE(207), 2,
      sym__pitch,
      aux_sym_grace_note_repeat1,
  [2455] = 5,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    STATE(135), 1,
      sym__pitch,
    STATE(199), 1,
      sym_note,
  [2471] = 4,
    ACTIONS(729), 1,
      sym_alteration,
    ACTIONS(732), 1,
      sym_note_letter,
    ACTIONS(735), 1,
      aux_sym_grace_note_token2,
    STATE(207), 2,
      sym__pitch,
      aux_sym_grace_note_repeat1,
  [2485] = 5,
    ACTIONS(11), 1,
      anon_sym_X_COLON,
    STATE(7), 1,
      sym_reference_number_line,
    STATE(13), 1,
      sym_tune_header,
    STATE(194), 1,
      aux_sym_file_structure_repeat2,
    STATE(210), 1,
      sym_tune,
  [2501] = 5,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    ACTIONS(29), 1,
      sym_rest,
    STATE(135), 1,
      sym__pitch,
    STATE(195), 1,
      sym_note,
  [2517] = 3,
    ACTIONS(739), 1,
      sym__NL,
    STATE(213), 1,
      aux_sym_file_structure_repeat1,
    ACTIONS(737), 2,
      ts_builtin_sym_end,
      anon_sym_X_COLON,
  [2528] = 1,
    ACTIONS(741), 4,
      anon_sym_LBRACK,
      sym_alteration,
      sym_note_letter,
      sym_rest,
  [2535] = 3,
    ACTIONS(25), 1,
      sym_alteration,
    ACTIONS(27), 1,
      sym_note_letter,
    STATE(205), 2,
      sym__pitch,
      aux_sym_grace_note_repeat1,
  [2546] = 3,
    ACTIONS(606), 1,
      sym__NL,
    STATE(113), 1,
      aux_sym_file_structure_repeat1,
    ACTIONS(715), 2,
      ts_builtin_sym_end,
      anon_sym_X_COLON,
  [2557] = 2,
    ACTIONS(743), 1,
      sym_alteration,
    ACTIONS(745), 3,
      aux_sym_chord_symbol_token1,
      aux_sym_chord_symbol_token2,
      sym_chord_type,
  [2566] = 1,
    ACTIONS(747), 4,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
      sym_words_postbody,
  [2573] = 1,
    ACTIONS(749), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2579] = 1,
    ACTIONS(703), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2585] = 3,
    ACTIONS(751), 1,
      sym_noCommentText,
    ACTIONS(753), 1,
      anon_sym_DOT,
    STATE(219), 1,
      aux_sym_abc_version_repeat1,
  [2595] = 3,
    ACTIONS(755), 1,
      sym_noCommentText,
    ACTIONS(757), 1,
      anon_sym_DOT,
    STATE(219), 1,
      aux_sym_abc_version_repeat1,
  [2605] = 3,
    ACTIONS(760), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(762), 1,
      aux_sym_chord_symbol_token2,
    ACTIONS(764), 1,
      sym_chord_type,
  [2615] = 1,
    ACTIONS(766), 3,
      aux_sym_chord_symbol_token1,
      aux_sym_chord_symbol_token2,
      sym_chord_type,
  [2621] = 3,
    ACTIONS(753), 1,
      anon_sym_DOT,
    ACTIONS(768), 1,
      sym_noCommentText,
    STATE(218), 1,
      aux_sym_abc_version_repeat1,
  [2631] = 1,
    ACTIONS(699), 3,
      ts_builtin_sym_end,
      sym__NL,
      anon_sym_X_COLON,
  [2637] = 3,
    ACTIONS(770), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(772), 1,
      aux_sym_chord_symbol_token2,
    ACTIONS(774), 1,
      sym_chord_type,
  [2647] = 3,
    ACTIONS(776), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(778), 1,
      aux_sym_chord_symbol_token2,
    ACTIONS(780), 1,
      sym_chord_type,
  [2657] = 2,
    ACTIONS(745), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(782), 1,
      sym_alteration,
  [2664] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(275), 1,
      sym_chord_symbol_note,
  [2671] = 2,
    ACTIONS(786), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(788), 1,
      aux_sym_chord_symbol_token2,
  [2678] = 1,
    ACTIONS(790), 2,
      aux_sym_user_defined_token1,
      aux_sym_user_defined_token2,
  [2683] = 2,
    ACTIONS(792), 1,
      anon_sym_LBRACK,
    STATE(99), 1,
      sym_nth_ending_number,
  [2690] = 1,
    ACTIONS(794), 2,
      aux_sym_user_defined_token1,
      aux_sym_user_defined_token2,
  [2695] = 2,
    ACTIONS(796), 1,
      anon_sym_LBRACK,
    STATE(100), 1,
      sym_nth_ending_number,
  [2702] = 2,
    ACTIONS(798), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(800), 1,
      aux_sym_chord_symbol_token2,
  [2709] = 2,
    ACTIONS(802), 1,
      sym_note_letter,
    STATE(220), 1,
      sym_chord_symbol_note,
  [2716] = 2,
    ACTIONS(804), 1,
      sym__NL,
    ACTIONS(806), 1,
      sym_COMMENT,
  [2723] = 2,
    ACTIONS(808), 1,
      aux_sym_chord_symbol_token1,
    ACTIONS(810), 1,
      aux_sym_chord_symbol_token2,
  [2730] = 2,
    ACTIONS(812), 1,
      anon_sym_LBRACK,
    STATE(51), 1,
      sym_nth_ending_number,
  [2737] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(278), 1,
      sym_chord_symbol_note,
  [2744] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(300), 1,
      sym_chord_symbol_note,
  [2751] = 2,
    ACTIONS(814), 1,
      sym__NL,
    ACTIONS(816), 1,
      sym_COMMENT,
  [2758] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(277), 1,
      sym_chord_symbol_note,
  [2765] = 2,
    ACTIONS(818), 1,
      sym__NL,
    STATE(114), 1,
      aux_sym_file_structure_repeat1,
  [2772] = 2,
    ACTIONS(802), 1,
      sym_note_letter,
    STATE(225), 1,
      sym_chord_symbol_note,
  [2779] = 2,
    ACTIONS(820), 1,
      sym__NL,
    ACTIONS(822), 1,
      sym_COMMENT,
  [2786] = 2,
    ACTIONS(824), 1,
      sym__NL,
    ACTIONS(826), 1,
      sym_COMMENT,
  [2793] = 1,
    ACTIONS(755), 2,
      sym_noCommentText,
      anon_sym_DOT,
  [2798] = 2,
    ACTIONS(802), 1,
      sym_note_letter,
    STATE(224), 1,
      sym_chord_symbol_note,
  [2805] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(286), 1,
      sym_chord_symbol_note,
  [2812] = 2,
    ACTIONS(784), 1,
      sym_note_letter,
    STATE(288), 1,
      sym_chord_symbol_note,
  [2819] = 1,
    ACTIONS(828), 1,
      aux_sym_multimeasure_rest_token1,
  [2823] = 1,
    ACTIONS(830), 1,
      aux_sym_nth_ending_number_token1,
  [2827] = 1,
    ACTIONS(832), 1,
      aux_sym_lyric_line_token1,
  [2831] = 1,
    ACTIONS(834), 1,
      anon_sym_RBRACK,
  [2835] = 1,
    ACTIONS(836), 1,
      sym_note_letter,
  [2839] = 1,
    ACTIONS(838), 1,
      aux_sym_body_inline_info_token1,
  [2843] = 1,
    ACTIONS(840), 1,
      aux_sym_nth_ending_number_token1,
  [2847] = 1,
    ACTIONS(842), 1,
      anon_sym_EQ,
  [2851] = 1,
    ACTIONS(844), 1,
      aux_sym_lyric_line_token1,
  [2855] = 1,
    ACTIONS(385), 1,
      sym_noCommentText,
  [2859] = 1,
    ACTIONS(846), 1,
      sym__NL,
  [2863] = 1,
    ACTIONS(848), 1,
      sym__NL,
  [2867] = 1,
    ACTIONS(385), 1,
      aux_sym_body_inline_info_token1,
  [2871] = 1,
    ACTIONS(850), 1,
      sym_note_letter,
  [2875] = 1,
    ACTIONS(852), 1,
      sym_noCommentText,
  [2879] = 1,
    ACTIONS(854), 1,
      aux_sym_nth_ending_number_token1,
  [2883] = 1,
    ACTIONS(856), 1,
      sym__NL,
  [2887] = 1,
    ACTIONS(858), 1,
      aux_sym_nth_ending_number_token1,
  [2891] = 1,
    ACTIONS(860), 1,
      aux_sym_body_inline_info_token1,
  [2895] = 1,
    ACTIONS(862), 1,
      ts_builtin_sym_end,
  [2899] = 1,
    ACTIONS(864), 1,
      sym__NL,
  [2903] = 1,
    ACTIONS(866), 1,
      ts_builtin_sym_end,
  [2907] = 1,
    ACTIONS(868), 1,
      aux_sym_nth_ending_number_token1,
  [2911] = 1,
    ACTIONS(870), 1,
      aux_sym_first_repeat_bar_token2,
  [2915] = 1,
    ACTIONS(872), 1,
      aux_sym_first_repeat_bar_token2,
  [2919] = 1,
    ACTIONS(874), 1,
      aux_sym_chord_symbol_token1,
  [2923] = 1,
    ACTIONS(876), 1,
      aux_sym_nth_ending_number_token1,
  [2927] = 1,
    ACTIONS(878), 1,
      aux_sym_chord_symbol_token1,
  [2931] = 1,
    ACTIONS(880), 1,
      aux_sym_chord_symbol_token1,
  [2935] = 1,
    ACTIONS(882), 1,
      sym_noCommentText,
  [2939] = 1,
    ACTIONS(884), 1,
      anon_sym_LBRACK,
  [2943] = 1,
    ACTIONS(886), 1,
      aux_sym_nth_ending_number_token1,
  [2947] = 1,
    ACTIONS(888), 1,
      aux_sym_first_repeat_bar_token2,
  [2951] = 1,
    ACTIONS(890), 1,
      anon_sym_LBRACK,
  [2955] = 1,
    ACTIONS(892), 1,
      aux_sym_first_repeat_bar_token2,
  [2959] = 1,
    ACTIONS(894), 1,
      aux_sym_first_repeat_bar_token2,
  [2963] = 1,
    ACTIONS(896), 1,
      aux_sym_chord_symbol_token1,
  [2967] = 1,
    ACTIONS(898), 1,
      aux_sym_nth_ending_number_token1,
  [2971] = 1,
    ACTIONS(900), 1,
      aux_sym_chord_symbol_token1,
  [2975] = 1,
    ACTIONS(902), 1,
      anon_sym_EQ,
  [2979] = 1,
    ACTIONS(904), 1,
      sym__NL,
  [2983] = 1,
    ACTIONS(906), 1,
      aux_sym_nth_ending_number_token1,
  [2987] = 1,
    ACTIONS(908), 1,
      sym_noCommentText,
  [2991] = 1,
    ACTIONS(910), 1,
      anon_sym_LBRACK,
  [2995] = 1,
    ACTIONS(912), 1,
      anon_sym_LBRACK,
  [2999] = 1,
    ACTIONS(914), 1,
      aux_sym_first_repeat_bar_token2,
  [3003] = 1,
    ACTIONS(916), 1,
      sym__NL,
  [3007] = 1,
    ACTIONS(918), 1,
      sym_noCommentText,
  [3011] = 1,
    ACTIONS(920), 1,
      anon_sym_LBRACK,
  [3015] = 1,
    ACTIONS(922), 1,
      anon_sym_LBRACK,
  [3019] = 1,
    ACTIONS(924), 1,
      aux_sym_chord_symbol_token1,
  [3023] = 1,
    ACTIONS(926), 1,
      sym__NL,
  [3027] = 1,
    ACTIONS(838), 1,
      sym_noCommentText,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(109)] = 0,
  [SMALL_STATE(110)] = 65,
  [SMALL_STATE(111)] = 130,
  [SMALL_STATE(112)] = 176,
  [SMALL_STATE(113)] = 222,
  [SMALL_STATE(114)] = 257,
  [SMALL_STATE(115)] = 290,
  [SMALL_STATE(116)] = 346,
  [SMALL_STATE(117)] = 379,
  [SMALL_STATE(118)] = 428,
  [SMALL_STATE(119)] = 451,
  [SMALL_STATE(120)] = 474,
  [SMALL_STATE(121)] = 497,
  [SMALL_STATE(122)] = 520,
  [SMALL_STATE(123)] = 543,
  [SMALL_STATE(124)] = 587,
  [SMALL_STATE(125)] = 631,
  [SMALL_STATE(126)] = 670,
  [SMALL_STATE(127)] = 709,
  [SMALL_STATE(128)] = 748,
  [SMALL_STATE(129)] = 787,
  [SMALL_STATE(130)] = 826,
  [SMALL_STATE(131)] = 865,
  [SMALL_STATE(132)] = 904,
  [SMALL_STATE(133)] = 943,
  [SMALL_STATE(134)] = 982,
  [SMALL_STATE(135)] = 1021,
  [SMALL_STATE(136)] = 1039,
  [SMALL_STATE(137)] = 1075,
  [SMALL_STATE(138)] = 1108,
  [SMALL_STATE(139)] = 1141,
  [SMALL_STATE(140)] = 1174,
  [SMALL_STATE(141)] = 1207,
  [SMALL_STATE(142)] = 1240,
  [SMALL_STATE(143)] = 1273,
  [SMALL_STATE(144)] = 1306,
  [SMALL_STATE(145)] = 1321,
  [SMALL_STATE(146)] = 1354,
  [SMALL_STATE(147)] = 1387,
  [SMALL_STATE(148)] = 1420,
  [SMALL_STATE(149)] = 1434,
  [SMALL_STATE(150)] = 1450,
  [SMALL_STATE(151)] = 1466,
  [SMALL_STATE(152)] = 1482,
  [SMALL_STATE(153)] = 1498,
  [SMALL_STATE(154)] = 1514,
  [SMALL_STATE(155)] = 1530,
  [SMALL_STATE(156)] = 1546,
  [SMALL_STATE(157)] = 1562,
  [SMALL_STATE(158)] = 1578,
  [SMALL_STATE(159)] = 1594,
  [SMALL_STATE(160)] = 1607,
  [SMALL_STATE(161)] = 1620,
  [SMALL_STATE(162)] = 1651,
  [SMALL_STATE(163)] = 1682,
  [SMALL_STATE(164)] = 1699,
  [SMALL_STATE(165)] = 1712,
  [SMALL_STATE(166)] = 1725,
  [SMALL_STATE(167)] = 1738,
  [SMALL_STATE(168)] = 1751,
  [SMALL_STATE(169)] = 1780,
  [SMALL_STATE(170)] = 1811,
  [SMALL_STATE(171)] = 1824,
  [SMALL_STATE(172)] = 1837,
  [SMALL_STATE(173)] = 1868,
  [SMALL_STATE(174)] = 1881,
  [SMALL_STATE(175)] = 1894,
  [SMALL_STATE(176)] = 1922,
  [SMALL_STATE(177)] = 1950,
  [SMALL_STATE(178)] = 1978,
  [SMALL_STATE(179)] = 2006,
  [SMALL_STATE(180)] = 2034,
  [SMALL_STATE(181)] = 2062,
  [SMALL_STATE(182)] = 2081,
  [SMALL_STATE(183)] = 2092,
  [SMALL_STATE(184)] = 2111,
  [SMALL_STATE(185)] = 2133,
  [SMALL_STATE(186)] = 2155,
  [SMALL_STATE(187)] = 2177,
  [SMALL_STATE(188)] = 2199,
  [SMALL_STATE(189)] = 2221,
  [SMALL_STATE(190)] = 2243,
  [SMALL_STATE(191)] = 2265,
  [SMALL_STATE(192)] = 2287,
  [SMALL_STATE(193)] = 2296,
  [SMALL_STATE(194)] = 2315,
  [SMALL_STATE(195)] = 2334,
  [SMALL_STATE(196)] = 2343,
  [SMALL_STATE(197)] = 2352,
  [SMALL_STATE(198)] = 2361,
  [SMALL_STATE(199)] = 2370,
  [SMALL_STATE(200)] = 2379,
  [SMALL_STATE(201)] = 2398,
  [SMALL_STATE(202)] = 2407,
  [SMALL_STATE(203)] = 2420,
  [SMALL_STATE(204)] = 2433,
  [SMALL_STATE(205)] = 2441,
  [SMALL_STATE(206)] = 2455,
  [SMALL_STATE(207)] = 2471,
  [SMALL_STATE(208)] = 2485,
  [SMALL_STATE(209)] = 2501,
  [SMALL_STATE(210)] = 2517,
  [SMALL_STATE(211)] = 2528,
  [SMALL_STATE(212)] = 2535,
  [SMALL_STATE(213)] = 2546,
  [SMALL_STATE(214)] = 2557,
  [SMALL_STATE(215)] = 2566,
  [SMALL_STATE(216)] = 2573,
  [SMALL_STATE(217)] = 2579,
  [SMALL_STATE(218)] = 2585,
  [SMALL_STATE(219)] = 2595,
  [SMALL_STATE(220)] = 2605,
  [SMALL_STATE(221)] = 2615,
  [SMALL_STATE(222)] = 2621,
  [SMALL_STATE(223)] = 2631,
  [SMALL_STATE(224)] = 2637,
  [SMALL_STATE(225)] = 2647,
  [SMALL_STATE(226)] = 2657,
  [SMALL_STATE(227)] = 2664,
  [SMALL_STATE(228)] = 2671,
  [SMALL_STATE(229)] = 2678,
  [SMALL_STATE(230)] = 2683,
  [SMALL_STATE(231)] = 2690,
  [SMALL_STATE(232)] = 2695,
  [SMALL_STATE(233)] = 2702,
  [SMALL_STATE(234)] = 2709,
  [SMALL_STATE(235)] = 2716,
  [SMALL_STATE(236)] = 2723,
  [SMALL_STATE(237)] = 2730,
  [SMALL_STATE(238)] = 2737,
  [SMALL_STATE(239)] = 2744,
  [SMALL_STATE(240)] = 2751,
  [SMALL_STATE(241)] = 2758,
  [SMALL_STATE(242)] = 2765,
  [SMALL_STATE(243)] = 2772,
  [SMALL_STATE(244)] = 2779,
  [SMALL_STATE(245)] = 2786,
  [SMALL_STATE(246)] = 2793,
  [SMALL_STATE(247)] = 2798,
  [SMALL_STATE(248)] = 2805,
  [SMALL_STATE(249)] = 2812,
  [SMALL_STATE(250)] = 2819,
  [SMALL_STATE(251)] = 2823,
  [SMALL_STATE(252)] = 2827,
  [SMALL_STATE(253)] = 2831,
  [SMALL_STATE(254)] = 2835,
  [SMALL_STATE(255)] = 2839,
  [SMALL_STATE(256)] = 2843,
  [SMALL_STATE(257)] = 2847,
  [SMALL_STATE(258)] = 2851,
  [SMALL_STATE(259)] = 2855,
  [SMALL_STATE(260)] = 2859,
  [SMALL_STATE(261)] = 2863,
  [SMALL_STATE(262)] = 2867,
  [SMALL_STATE(263)] = 2871,
  [SMALL_STATE(264)] = 2875,
  [SMALL_STATE(265)] = 2879,
  [SMALL_STATE(266)] = 2883,
  [SMALL_STATE(267)] = 2887,
  [SMALL_STATE(268)] = 2891,
  [SMALL_STATE(269)] = 2895,
  [SMALL_STATE(270)] = 2899,
  [SMALL_STATE(271)] = 2903,
  [SMALL_STATE(272)] = 2907,
  [SMALL_STATE(273)] = 2911,
  [SMALL_STATE(274)] = 2915,
  [SMALL_STATE(275)] = 2919,
  [SMALL_STATE(276)] = 2923,
  [SMALL_STATE(277)] = 2927,
  [SMALL_STATE(278)] = 2931,
  [SMALL_STATE(279)] = 2935,
  [SMALL_STATE(280)] = 2939,
  [SMALL_STATE(281)] = 2943,
  [SMALL_STATE(282)] = 2947,
  [SMALL_STATE(283)] = 2951,
  [SMALL_STATE(284)] = 2955,
  [SMALL_STATE(285)] = 2959,
  [SMALL_STATE(286)] = 2963,
  [SMALL_STATE(287)] = 2967,
  [SMALL_STATE(288)] = 2971,
  [SMALL_STATE(289)] = 2975,
  [SMALL_STATE(290)] = 2979,
  [SMALL_STATE(291)] = 2983,
  [SMALL_STATE(292)] = 2987,
  [SMALL_STATE(293)] = 2991,
  [SMALL_STATE(294)] = 2995,
  [SMALL_STATE(295)] = 2999,
  [SMALL_STATE(296)] = 3003,
  [SMALL_STATE(297)] = 3007,
  [SMALL_STATE(298)] = 3011,
  [SMALL_STATE(299)] = 3015,
  [SMALL_STATE(300)] = 3019,
  [SMALL_STATE(301)] = 3023,
  [SMALL_STATE(302)] = 3027,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = false}}, SHIFT(242),
  [5] = {.entry = {.count = 1, .reusable = true}}, SHIFT(292),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(291),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(279),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(251),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(231),
  [15] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_body, 1),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(54),
  [19] = {.entry = {.count = 1, .reusable = false}}, SHIFT(116),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(296),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(54),
  [25] = {.entry = {.count = 1, .reusable = true}}, SHIFT(254),
  [27] = {.entry = {.count = 1, .reusable = true}}, SHIFT(122),
  [29] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [31] = {.entry = {.count = 1, .reusable = true}}, SHIFT(250),
  [33] = {.entry = {.count = 1, .reusable = false}}, SHIFT(250),
  [35] = {.entry = {.count = 1, .reusable = true}}, SHIFT(212),
  [37] = {.entry = {.count = 1, .reusable = false}}, SHIFT(247),
  [39] = {.entry = {.count = 1, .reusable = true}}, SHIFT(179),
  [41] = {.entry = {.count = 1, .reusable = false}}, SHIFT(211),
  [43] = {.entry = {.count = 1, .reusable = false}}, SHIFT(23),
  [45] = {.entry = {.count = 1, .reusable = true}}, SHIFT(48),
  [47] = {.entry = {.count = 1, .reusable = true}}, SHIFT(59),
  [49] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [51] = {.entry = {.count = 1, .reusable = true}}, SHIFT(53),
  [53] = {.entry = {.count = 1, .reusable = true}}, SHIFT(297),
  [55] = {.entry = {.count = 1, .reusable = true}}, SHIFT(70),
  [57] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_body, 2),
  [59] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2),
  [61] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(54),
  [64] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(116),
  [67] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(296),
  [70] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(292),
  [73] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(54),
  [76] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(254),
  [79] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(122),
  [82] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(135),
  [85] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(250),
  [88] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(250),
  [91] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(212),
  [94] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(247),
  [97] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(179),
  [100] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(211),
  [103] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(23),
  [106] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(48),
  [109] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(59),
  [112] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(22),
  [115] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(53),
  [118] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(297),
  [121] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(70),
  [124] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_body_repeat1, 2), SHIFT_REPEAT(231),
  [127] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_header, 2),
  [129] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tune_header, 2),
  [131] = {.entry = {.count = 1, .reusable = false}}, SHIFT(261),
  [133] = {.entry = {.count = 1, .reusable = true}}, SHIFT(264),
  [135] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_tune_header_repeat1, 2),
  [137] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tune_header_repeat1, 2),
  [139] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_header_repeat1, 2), SHIFT_REPEAT(261),
  [142] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_header_repeat1, 2), SHIFT_REPEAT(264),
  [145] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_tune_header_repeat1, 2), SHIFT_REPEAT(291),
  [148] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_header_repeat1, 2), SHIFT_REPEAT(70),
  [151] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_tune_header_repeat1, 2), SHIFT_REPEAT(231),
  [154] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_header, 1),
  [156] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_tune_header, 1),
  [158] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_beam, 2),
  [160] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_beam, 2),
  [162] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [164] = {.entry = {.count = 2, .reusable = false}}, REDUCE(sym_beam, 2), SHIFT(204),
  [167] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_beam, 2), SHIFT(263),
  [170] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_beam, 2), SHIFT(18),
  [173] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_beam, 2), SHIFT(19),
  [176] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_beam, 2), SHIFT(212),
  [179] = {.entry = {.count = 2, .reusable = false}}, REDUCE(sym_beam, 2), SHIFT(247),
  [182] = {.entry = {.count = 2, .reusable = true}}, REDUCE(sym_beam, 2), SHIFT(176),
  [185] = {.entry = {.count = 2, .reusable = false}}, REDUCE(sym_beam, 2), SHIFT(211),
  [188] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2),
  [190] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_beam_repeat1, 2),
  [192] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(117),
  [195] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(204),
  [198] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(263),
  [201] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(18),
  [204] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(19),
  [207] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(212),
  [210] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(247),
  [213] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(176),
  [216] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_beam_repeat1, 2), SHIFT_REPEAT(211),
  [219] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_reference_number_line, 4),
  [221] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_reference_number_line, 4),
  [223] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_reference_number_line, 5),
  [225] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_reference_number_line, 5),
  [227] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune, 1),
  [229] = {.entry = {.count = 1, .reusable = true}}, SHIFT(211),
  [231] = {.entry = {.count = 1, .reusable = true}}, SHIFT(252),
  [233] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__pitch_repeat1, 2),
  [235] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__pitch_repeat1, 2),
  [237] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__pitch_repeat1, 2), SHIFT_REPEAT(14),
  [240] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__pitch, 2),
  [242] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__pitch, 2),
  [244] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [246] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [248] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__pitch, 3),
  [250] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__pitch, 3),
  [252] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__pitch, 1),
  [254] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__pitch, 1),
  [256] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [258] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note, 1),
  [260] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note, 1),
  [262] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [264] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_nth_ending_number_repeat1, 2),
  [266] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_nth_ending_number_repeat1, 2),
  [268] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_nth_ending_number_repeat1, 2), SHIFT_REPEAT(256),
  [271] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_nth_ending_number, 3),
  [273] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_nth_ending_number, 3),
  [275] = {.entry = {.count = 1, .reusable = true}}, SHIFT(256),
  [277] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_generic_bar_line, 1),
  [279] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_generic_bar_line, 1),
  [281] = {.entry = {.count = 1, .reusable = false}}, SHIFT(272),
  [283] = {.entry = {.count = 1, .reusable = false}}, SHIFT(237),
  [285] = {.entry = {.count = 1, .reusable = false}}, SHIFT(283),
  [287] = {.entry = {.count = 1, .reusable = true}}, SHIFT(57),
  [289] = {.entry = {.count = 1, .reusable = false}}, SHIFT(280),
  [291] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [293] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_nth_ending_number, 2),
  [295] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_nth_ending_number, 2),
  [297] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 7),
  [299] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 7),
  [301] = {.entry = {.count = 1, .reusable = true}}, SHIFT(49),
  [303] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 5),
  [305] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 5),
  [307] = {.entry = {.count = 1, .reusable = true}}, SHIFT(47),
  [309] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 3),
  [311] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 3),
  [313] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [315] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_close_chord, 1),
  [317] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_close_chord, 1),
  [319] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 3),
  [321] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 3),
  [323] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [325] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note, 2),
  [327] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note, 2),
  [329] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 4),
  [331] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 4),
  [333] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [335] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 4),
  [337] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 4),
  [339] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
  [341] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 1),
  [343] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 1),
  [345] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [347] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 6),
  [349] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 6),
  [351] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [353] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 5),
  [355] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 5),
  [357] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [359] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 2),
  [361] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 2),
  [363] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [365] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_note_construct, 6),
  [367] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_note_construct, 6),
  [369] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__chord_cstrct, 8),
  [371] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__chord_cstrct, 8),
  [373] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_first_repeat_bar, 2),
  [375] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_first_repeat_bar, 2),
  [377] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_nth_ending_barline, 3),
  [379] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_nth_ending_barline, 3),
  [381] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_second_repeat_bar, 4),
  [383] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_second_repeat_bar, 4),
  [385] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_symbol, 1),
  [387] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_symbol, 1),
  [389] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__MUSIC_CODE, 1),
  [391] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__MUSIC_CODE, 1),
  [393] = {.entry = {.count = 1, .reusable = false}}, SHIFT(64),
  [395] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_first_repeat_bar, 4),
  [397] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_first_repeat_bar, 4),
  [399] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_body_inline_info, 4),
  [401] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_body_inline_info, 4),
  [403] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_second_repeat_bar, 2),
  [405] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_second_repeat_bar, 2),
  [407] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_nth_ending_barline, 2),
  [409] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_nth_ending_barline, 2),
  [411] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__music_content, 2),
  [413] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__music_content, 2),
  [415] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_multimeasure_rest, 2),
  [417] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_multimeasure_rest, 2),
  [419] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_stylesheet_directives, 2),
  [421] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_stylesheet_directives, 2),
  [423] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_tune_body_repeat1, 2),
  [425] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__MUSIC_CODE, 2),
  [427] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__MUSIC_CODE, 2),
  [429] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2),
  [431] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(65),
  [434] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(234),
  [437] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(72),
  [440] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(77),
  [443] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(86),
  [446] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(71),
  [449] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(88),
  [452] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(66),
  [455] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(243),
  [458] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(74),
  [461] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(79),
  [464] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(79),
  [467] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(89),
  [470] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(73),
  [473] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_symbol_line_repeat1, 2), SHIFT_REPEAT(101),
  [476] = {.entry = {.count = 1, .reusable = false}}, SHIFT(66),
  [478] = {.entry = {.count = 1, .reusable = false}}, SHIFT(243),
  [480] = {.entry = {.count = 1, .reusable = false}}, SHIFT(74),
  [482] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [484] = {.entry = {.count = 1, .reusable = false}}, SHIFT(79),
  [486] = {.entry = {.count = 1, .reusable = false}}, SHIFT(89),
  [488] = {.entry = {.count = 1, .reusable = false}}, SHIFT(73),
  [490] = {.entry = {.count = 1, .reusable = false}}, SHIFT(101),
  [492] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_symbol_line, 2),
  [494] = {.entry = {.count = 1, .reusable = false}}, SHIFT(65),
  [496] = {.entry = {.count = 1, .reusable = false}}, SHIFT(234),
  [498] = {.entry = {.count = 1, .reusable = false}}, SHIFT(72),
  [500] = {.entry = {.count = 1, .reusable = false}}, SHIFT(77),
  [502] = {.entry = {.count = 1, .reusable = false}}, SHIFT(86),
  [504] = {.entry = {.count = 1, .reusable = false}}, SHIFT(71),
  [506] = {.entry = {.count = 1, .reusable = false}}, SHIFT(88),
  [508] = {.entry = {.count = 1, .reusable = true}}, SHIFT(67),
  [510] = {.entry = {.count = 1, .reusable = true}}, SHIFT(89),
  [512] = {.entry = {.count = 1, .reusable = true}}, SHIFT(73),
  [514] = {.entry = {.count = 1, .reusable = true}}, SHIFT(101),
  [516] = {.entry = {.count = 1, .reusable = true}}, SHIFT(68),
  [518] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [520] = {.entry = {.count = 1, .reusable = true}}, SHIFT(86),
  [522] = {.entry = {.count = 1, .reusable = true}}, SHIFT(71),
  [524] = {.entry = {.count = 1, .reusable = true}}, SHIFT(88),
  [526] = {.entry = {.count = 1, .reusable = false}}, SHIFT(267),
  [528] = {.entry = {.count = 1, .reusable = false}}, SHIFT(230),
  [530] = {.entry = {.count = 1, .reusable = false}}, SHIFT(294),
  [532] = {.entry = {.count = 1, .reusable = false}}, SHIFT(102),
  [534] = {.entry = {.count = 1, .reusable = false}}, SHIFT(293),
  [536] = {.entry = {.count = 1, .reusable = false}}, SHIFT(93),
  [538] = {.entry = {.count = 1, .reusable = false}}, SHIFT(281),
  [540] = {.entry = {.count = 1, .reusable = false}}, SHIFT(232),
  [542] = {.entry = {.count = 1, .reusable = false}}, SHIFT(299),
  [544] = {.entry = {.count = 1, .reusable = false}}, SHIFT(97),
  [546] = {.entry = {.count = 1, .reusable = false}}, SHIFT(298),
  [548] = {.entry = {.count = 1, .reusable = false}}, SHIFT(104),
  [550] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_nth_ending_number_repeat1, 2), SHIFT_REPEAT(287),
  [553] = {.entry = {.count = 1, .reusable = false}}, SHIFT(287),
  [555] = {.entry = {.count = 1, .reusable = false}}, SHIFT(276),
  [557] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_nth_ending_number_repeat1, 2), SHIFT_REPEAT(276),
  [560] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chord_symbol, 3),
  [562] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chord_symbol, 5),
  [564] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol, 5),
  [566] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chord_symbol, 4),
  [568] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol, 3),
  [570] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol, 4),
  [572] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chord_symbol, 6),
  [574] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol, 6),
  [576] = {.entry = {.count = 1, .reusable = true}}, SHIFT(259),
  [578] = {.entry = {.count = 1, .reusable = true}}, SHIFT(262),
  [580] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_file_header_repeat1, 2), SHIFT_REPEAT(242),
  [583] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_file_header_repeat1, 2), SHIFT_REPEAT(292),
  [586] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_file_header_repeat1, 2), SHIFT_REPEAT(291),
  [589] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_file_header_repeat1, 2), SHIFT_REPEAT(279),
  [592] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_file_header_repeat1, 2),
  [594] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_file_header_repeat1, 2), SHIFT_REPEAT(231),
  [597] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_file_header, 1),
  [599] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_file_structure_repeat1, 2),
  [601] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_file_structure_repeat1, 2), SHIFT_REPEAT(113),
  [604] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_file_structure_repeat1, 2),
  [606] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [608] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_file_header_repeat1, 2),
  [610] = {.entry = {.count = 1, .reusable = true}}, SHIFT(204),
  [612] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [614] = {.entry = {.count = 1, .reusable = true}}, SHIFT(263),
  [616] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [618] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [620] = {.entry = {.count = 1, .reusable = true}}, SHIFT(247),
  [622] = {.entry = {.count = 1, .reusable = true}}, SHIFT(176),
  [624] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_open_chord, 1),
  [626] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_open_chord, 1),
  [628] = {.entry = {.count = 1, .reusable = true}}, SHIFT(268),
  [630] = {.entry = {.count = 1, .reusable = true}}, SHIFT(69),
  [632] = {.entry = {.count = 1, .reusable = true}}, SHIFT(229),
  [634] = {.entry = {.count = 1, .reusable = true}}, SHIFT(121),
  [636] = {.entry = {.count = 1, .reusable = true}}, SHIFT(120),
  [638] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__pitch_repeat1, 2), SHIFT_REPEAT(120),
  [641] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [643] = {.entry = {.count = 1, .reusable = true}}, SHIFT(177),
  [645] = {.entry = {.count = 1, .reusable = true}}, SHIFT(178),
  [647] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [649] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [651] = {.entry = {.count = 1, .reusable = true}}, SHIFT(144),
  [653] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2),
  [655] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2), SHIFT_REPEAT(254),
  [658] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2), SHIFT_REPEAT(122),
  [661] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2), SHIFT_REPEAT(135),
  [664] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2), SHIFT_REPEAT(212),
  [667] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__chord_cstrct_repeat1, 2), SHIFT_REPEAT(211),
  [670] = {.entry = {.count = 1, .reusable = true}}, SHIFT(171),
  [672] = {.entry = {.count = 1, .reusable = true}}, SHIFT(170),
  [674] = {.entry = {.count = 1, .reusable = true}}, SHIFT(165),
  [676] = {.entry = {.count = 1, .reusable = true}}, SHIFT(160),
  [678] = {.entry = {.count = 1, .reusable = true}}, SHIFT(164),
  [680] = {.entry = {.count = 1, .reusable = true}}, SHIFT(159),
  [682] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [684] = {.entry = {.count = 1, .reusable = true}}, SHIFT(173),
  [686] = {.entry = {.count = 1, .reusable = true}}, SHIFT(166),
  [688] = {.entry = {.count = 1, .reusable = true}}, SHIFT(167),
  [690] = {.entry = {.count = 1, .reusable = true}}, SHIFT(175),
  [692] = {.entry = {.count = 1, .reusable = true}}, SHIFT(180),
  [694] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_note_construct_repeat1, 2),
  [696] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_note_construct_repeat1, 2), SHIFT_REPEAT(212),
  [699] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune, 2),
  [701] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_grace_note, 3),
  [703] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune, 3),
  [705] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_note, 1),
  [707] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_file_structure, 1),
  [709] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_file_structure, 2),
  [711] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_note, 3),
  [713] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_note, 2),
  [715] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_file_structure_repeat2, 2),
  [717] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_file_structure_repeat2, 2), SHIFT_REPEAT(251),
  [720] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_lyric_section, 1),
  [722] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_lyric_section_repeat1, 2),
  [724] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_lyric_section_repeat1, 2), SHIFT_REPEAT(252),
  [727] = {.entry = {.count = 1, .reusable = true}}, SHIFT(182),
  [729] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_grace_note_repeat1, 2), SHIFT_REPEAT(254),
  [732] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_grace_note_repeat1, 2), SHIFT_REPEAT(122),
  [735] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_grace_note_repeat1, 2),
  [737] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_file_structure_repeat2, 1),
  [739] = {.entry = {.count = 1, .reusable = true}}, SHIFT(213),
  [741] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_decoration, 1),
  [743] = {.entry = {.count = 1, .reusable = false}}, SHIFT(221),
  [745] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol_note, 1),
  [747] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_lyric_line, 3),
  [749] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune, 4),
  [751] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_abc_version, 3),
  [753] = {.entry = {.count = 1, .reusable = false}}, SHIFT(265),
  [755] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_abc_version_repeat1, 2),
  [757] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_abc_version_repeat1, 2), SHIFT_REPEAT(265),
  [760] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [762] = {.entry = {.count = 1, .reusable = true}}, SHIFT(227),
  [764] = {.entry = {.count = 1, .reusable = true}}, SHIFT(228),
  [766] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chord_symbol_note, 2),
  [768] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_abc_version, 2),
  [770] = {.entry = {.count = 1, .reusable = true}}, SHIFT(196),
  [772] = {.entry = {.count = 1, .reusable = true}}, SHIFT(241),
  [774] = {.entry = {.count = 1, .reusable = true}}, SHIFT(236),
  [776] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [778] = {.entry = {.count = 1, .reusable = true}}, SHIFT(248),
  [780] = {.entry = {.count = 1, .reusable = true}}, SHIFT(233),
  [782] = {.entry = {.count = 1, .reusable = true}}, SHIFT(221),
  [784] = {.entry = {.count = 1, .reusable = true}}, SHIFT(226),
  [786] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [788] = {.entry = {.count = 1, .reusable = true}}, SHIFT(238),
  [790] = {.entry = {.count = 1, .reusable = true}}, SHIFT(289),
  [792] = {.entry = {.count = 1, .reusable = true}}, SHIFT(267),
  [794] = {.entry = {.count = 1, .reusable = true}}, SHIFT(257),
  [796] = {.entry = {.count = 1, .reusable = true}}, SHIFT(281),
  [798] = {.entry = {.count = 1, .reusable = true}}, SHIFT(103),
  [800] = {.entry = {.count = 1, .reusable = true}}, SHIFT(249),
  [802] = {.entry = {.count = 1, .reusable = true}}, SHIFT(214),
  [804] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [806] = {.entry = {.count = 1, .reusable = true}}, SHIFT(301),
  [808] = {.entry = {.count = 1, .reusable = true}}, SHIFT(198),
  [810] = {.entry = {.count = 1, .reusable = true}}, SHIFT(239),
  [812] = {.entry = {.count = 1, .reusable = true}}, SHIFT(272),
  [814] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_body_info_line, 2, .production_id = 1),
  [816] = {.entry = {.count = 1, .reusable = true}}, SHIFT(270),
  [818] = {.entry = {.count = 1, .reusable = true}}, SHIFT(114),
  [820] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_header_info_line, 2, .production_id = 1),
  [822] = {.entry = {.count = 1, .reusable = true}}, SHIFT(260),
  [824] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_file_header_info_line, 2, .production_id = 1),
  [826] = {.entry = {.count = 1, .reusable = true}}, SHIFT(266),
  [828] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [830] = {.entry = {.count = 1, .reusable = true}}, SHIFT(258),
  [832] = {.entry = {.count = 1, .reusable = true}}, SHIFT(290),
  [834] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [836] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [838] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_user_defined, 4),
  [840] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [842] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [844] = {.entry = {.count = 1, .reusable = true}}, SHIFT(235),
  [846] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_tune_header_info_line, 3, .production_id = 1),
  [848] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [850] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [852] = {.entry = {.count = 1, .reusable = true}}, SHIFT(244),
  [854] = {.entry = {.count = 1, .reusable = true}}, SHIFT(246),
  [856] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_file_header_info_line, 3, .production_id = 1),
  [858] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [860] = {.entry = {.count = 1, .reusable = true}}, SHIFT(253),
  [862] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1),
  [864] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_body_info_line, 3, .production_id = 1),
  [866] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [868] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [870] = {.entry = {.count = 1, .reusable = true}}, SHIFT(91),
  [872] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [874] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [876] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [878] = {.entry = {.count = 1, .reusable = true}}, SHIFT(201),
  [880] = {.entry = {.count = 1, .reusable = true}}, SHIFT(108),
  [882] = {.entry = {.count = 1, .reusable = true}}, SHIFT(245),
  [884] = {.entry = {.count = 1, .reusable = true}}, SHIFT(295),
  [886] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [888] = {.entry = {.count = 1, .reusable = true}}, SHIFT(52),
  [890] = {.entry = {.count = 1, .reusable = true}}, SHIFT(282),
  [892] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [894] = {.entry = {.count = 1, .reusable = true}}, SHIFT(105),
  [896] = {.entry = {.count = 1, .reusable = true}}, SHIFT(92),
  [898] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [900] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [902] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [904] = {.entry = {.count = 1, .reusable = true}}, SHIFT(215),
  [906] = {.entry = {.count = 1, .reusable = true}}, SHIFT(222),
  [908] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [910] = {.entry = {.count = 1, .reusable = true}}, SHIFT(273),
  [912] = {.entry = {.count = 1, .reusable = true}}, SHIFT(274),
  [914] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [916] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [918] = {.entry = {.count = 1, .reusable = true}}, SHIFT(240),
  [920] = {.entry = {.count = 1, .reusable = true}}, SHIFT(284),
  [922] = {.entry = {.count = 1, .reusable = true}}, SHIFT(285),
  [924] = {.entry = {.count = 1, .reusable = true}}, SHIFT(197),
  [926] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_abc(void) {
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
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
