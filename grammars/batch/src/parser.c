#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
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
#define STATE_COUNT 45
#define LARGE_STATE_COUNT 25
#define SYMBOL_COUNT 129
#define ALIAS_COUNT 2
#define TOKEN_COUNT 119
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 0
#define MAX_ALIAS_SEQUENCE_LENGTH 6
#define PRODUCTION_ID_COUNT 4

enum ts_symbol_identifiers {
  anon_sym_AT = 1,
  anon_sym_echooff = 2,
  anon_sym_COLON_COLON = 3,
  aux_sym_comment_token1 = 4,
  anon_sym_REM = 5,
  anon_sym_Rem = 6,
  anon_sym_rem = 7,
  anon_sym_SET = 8,
  anon_sym_Set = 9,
  anon_sym_set = 10,
  anon_sym_SLASHA = 11,
  anon_sym_EQ = 12,
  anon_sym_PERCENT = 13,
  anon_sym_ECHO = 14,
  anon_sym_IF = 15,
  anon_sym_GOTO = 16,
  anon_sym_EXIT = 17,
  anon_sym_FOR = 18,
  anon_sym_PAUSE = 19,
  anon_sym_CLS = 20,
  anon_sym_echo = 21,
  anon_sym_if = 22,
  anon_sym_goto = 23,
  anon_sym_exit = 24,
  anon_sym_for = 25,
  anon_sym_pause = 26,
  anon_sym_cls = 27,
  anon_sym_VER = 28,
  anon_sym_ASSOC = 29,
  anon_sym_CD = 30,
  anon_sym_COPY = 31,
  anon_sym_DEL = 32,
  anon_sym_DIR = 33,
  anon_sym_DATE = 34,
  anon_sym_MD = 35,
  anon_sym_MOVE = 36,
  anon_sym_PATH = 37,
  anon_sym_PROMPT = 38,
  anon_sym_RD = 39,
  anon_sym_REN = 40,
  anon_sym_START = 41,
  anon_sym_TIME = 42,
  anon_sym_TYPE = 43,
  anon_sym_VOL = 44,
  anon_sym_ATTRIB = 45,
  anon_sym_CHKDSK = 46,
  anon_sym_CHOICE = 47,
  anon_sym_CMD = 48,
  anon_sym_COMP = 49,
  anon_sym_CONVERT = 50,
  anon_sym_DRIVERQUERY = 51,
  anon_sym_EXPAND = 52,
  anon_sym_FIND = 53,
  anon_sym_FORMAT = 54,
  anon_sym_HELP = 55,
  anon_sym_IPCONFIG = 56,
  anon_sym_LABEL = 57,
  anon_sym_NET = 58,
  anon_sym_PING = 59,
  anon_sym_SHUTDOWN = 60,
  anon_sym_SORT = 61,
  anon_sym_SUBST = 62,
  anon_sym_SYSTEMINFO = 63,
  anon_sym_TASKKILL = 64,
  anon_sym_TASKLIST = 65,
  anon_sym_XCOPY = 66,
  anon_sym_TREE = 67,
  anon_sym_FC = 68,
  anon_sym_DISKPART = 69,
  anon_sym_TITLE = 70,
  anon_sym_ver = 71,
  anon_sym_assoc = 72,
  anon_sym_cd = 73,
  anon_sym_copy = 74,
  anon_sym_del = 75,
  anon_sym_dir = 76,
  anon_sym_date = 77,
  anon_sym_md = 78,
  anon_sym_move = 79,
  anon_sym_path = 80,
  anon_sym_prompt = 81,
  anon_sym_rd = 82,
  anon_sym_ren = 83,
  anon_sym_start = 84,
  anon_sym_time = 85,
  anon_sym_type = 86,
  anon_sym_vol = 87,
  anon_sym_attrib = 88,
  anon_sym_chkdsk = 89,
  anon_sym_choice = 90,
  anon_sym_cmd = 91,
  anon_sym_comp = 92,
  anon_sym_convert = 93,
  anon_sym_driverquery = 94,
  anon_sym_expand = 95,
  anon_sym_find = 96,
  anon_sym_format = 97,
  anon_sym_help = 98,
  anon_sym_ipconfig = 99,
  anon_sym_label = 100,
  anon_sym_net = 101,
  anon_sym_ping = 102,
  anon_sym_shutdown = 103,
  anon_sym_sort = 104,
  anon_sym_subst = 105,
  anon_sym_systeminfo = 106,
  anon_sym_taskkill = 107,
  anon_sym_tasklist = 108,
  anon_sym_xcopy = 109,
  anon_sym_tree = 110,
  anon_sym_fc = 111,
  anon_sym_diskpart = 112,
  anon_sym_title = 113,
  anon_sym_COLON = 114,
  sym_identifier = 115,
  anon_sym_DQUOTE = 116,
  aux_sym_string_token1 = 117,
  sym_number = 118,
  sym_program = 119,
  sym_echooff = 120,
  sym_comment = 121,
  sym_variable_declaration = 122,
  sym_variable_reference = 123,
  sym_keyword = 124,
  sym_function_definition = 125,
  sym_string = 126,
  aux_sym_program_repeat1 = 127,
  aux_sym_string_repeat1 = 128,
  anon_alias_sym_function_name = 129,
  anon_alias_sym_variable_name = 130,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_AT] = "@",
  [anon_sym_echooff] = "echo off",
  [anon_sym_COLON_COLON] = "::",
  [aux_sym_comment_token1] = "comment_token1",
  [anon_sym_REM] = "REM",
  [anon_sym_Rem] = "Rem",
  [anon_sym_rem] = "rem",
  [anon_sym_SET] = "SET",
  [anon_sym_Set] = "Set",
  [anon_sym_set] = "set",
  [anon_sym_SLASHA] = "/A",
  [anon_sym_EQ] = "=",
  [anon_sym_PERCENT] = "%",
  [anon_sym_ECHO] = "ECHO",
  [anon_sym_IF] = "IF",
  [anon_sym_GOTO] = "GOTO",
  [anon_sym_EXIT] = "EXIT",
  [anon_sym_FOR] = "FOR",
  [anon_sym_PAUSE] = "PAUSE",
  [anon_sym_CLS] = "CLS",
  [anon_sym_echo] = "echo",
  [anon_sym_if] = "if",
  [anon_sym_goto] = "goto",
  [anon_sym_exit] = "exit",
  [anon_sym_for] = "for",
  [anon_sym_pause] = "pause",
  [anon_sym_cls] = "cls",
  [anon_sym_VER] = "VER",
  [anon_sym_ASSOC] = "ASSOC",
  [anon_sym_CD] = "CD",
  [anon_sym_COPY] = "COPY",
  [anon_sym_DEL] = "DEL",
  [anon_sym_DIR] = "DIR",
  [anon_sym_DATE] = "DATE",
  [anon_sym_MD] = "MD",
  [anon_sym_MOVE] = "MOVE",
  [anon_sym_PATH] = "PATH",
  [anon_sym_PROMPT] = "PROMPT",
  [anon_sym_RD] = "RD",
  [anon_sym_REN] = "REN",
  [anon_sym_START] = "START",
  [anon_sym_TIME] = "TIME",
  [anon_sym_TYPE] = "TYPE",
  [anon_sym_VOL] = "VOL",
  [anon_sym_ATTRIB] = "ATTRIB",
  [anon_sym_CHKDSK] = "CHKDSK",
  [anon_sym_CHOICE] = "CHOICE",
  [anon_sym_CMD] = "CMD",
  [anon_sym_COMP] = "COMP",
  [anon_sym_CONVERT] = "CONVERT",
  [anon_sym_DRIVERQUERY] = "DRIVERQUERY",
  [anon_sym_EXPAND] = "EXPAND",
  [anon_sym_FIND] = "FIND",
  [anon_sym_FORMAT] = "FORMAT",
  [anon_sym_HELP] = "HELP",
  [anon_sym_IPCONFIG] = "IPCONFIG",
  [anon_sym_LABEL] = "LABEL",
  [anon_sym_NET] = "NET",
  [anon_sym_PING] = "PING",
  [anon_sym_SHUTDOWN] = "SHUTDOWN",
  [anon_sym_SORT] = "SORT",
  [anon_sym_SUBST] = "SUBST",
  [anon_sym_SYSTEMINFO] = "SYSTEMINFO",
  [anon_sym_TASKKILL] = "TASKKILL",
  [anon_sym_TASKLIST] = "TASKLIST",
  [anon_sym_XCOPY] = "XCOPY",
  [anon_sym_TREE] = "TREE",
  [anon_sym_FC] = "FC",
  [anon_sym_DISKPART] = "DISKPART",
  [anon_sym_TITLE] = "TITLE",
  [anon_sym_ver] = "ver",
  [anon_sym_assoc] = "assoc",
  [anon_sym_cd] = "cd",
  [anon_sym_copy] = "copy",
  [anon_sym_del] = "del",
  [anon_sym_dir] = "dir",
  [anon_sym_date] = "date",
  [anon_sym_md] = "md",
  [anon_sym_move] = "move",
  [anon_sym_path] = "path",
  [anon_sym_prompt] = "prompt",
  [anon_sym_rd] = "rd",
  [anon_sym_ren] = "ren",
  [anon_sym_start] = "start",
  [anon_sym_time] = "time",
  [anon_sym_type] = "type",
  [anon_sym_vol] = "vol",
  [anon_sym_attrib] = "attrib",
  [anon_sym_chkdsk] = "chkdsk",
  [anon_sym_choice] = "choice",
  [anon_sym_cmd] = "cmd",
  [anon_sym_comp] = "comp",
  [anon_sym_convert] = "convert",
  [anon_sym_driverquery] = "driverquery",
  [anon_sym_expand] = "expand",
  [anon_sym_find] = "find",
  [anon_sym_format] = "format",
  [anon_sym_help] = "help",
  [anon_sym_ipconfig] = "ipconfig",
  [anon_sym_label] = "label",
  [anon_sym_net] = "net",
  [anon_sym_ping] = "ping",
  [anon_sym_shutdown] = "shutdown",
  [anon_sym_sort] = "sort",
  [anon_sym_subst] = "subst",
  [anon_sym_systeminfo] = "systeminfo",
  [anon_sym_taskkill] = "taskkill",
  [anon_sym_tasklist] = "tasklist",
  [anon_sym_xcopy] = "xcopy",
  [anon_sym_tree] = "tree",
  [anon_sym_fc] = "fc",
  [anon_sym_diskpart] = "diskpart",
  [anon_sym_title] = "title",
  [anon_sym_COLON] = ":",
  [sym_identifier] = "identifier",
  [anon_sym_DQUOTE] = "\"",
  [aux_sym_string_token1] = "string_token1",
  [sym_number] = "number",
  [sym_program] = "program",
  [sym_echooff] = "echooff",
  [sym_comment] = "comment",
  [sym_variable_declaration] = "variable_declaration",
  [sym_variable_reference] = "variable_reference",
  [sym_keyword] = "keyword",
  [sym_function_definition] = "function_definition",
  [sym_string] = "string",
  [aux_sym_program_repeat1] = "program_repeat1",
  [aux_sym_string_repeat1] = "string_repeat1",
  [anon_alias_sym_function_name] = "function_name",
  [anon_alias_sym_variable_name] = "variable_name",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_AT] = anon_sym_AT,
  [anon_sym_echooff] = anon_sym_echooff,
  [anon_sym_COLON_COLON] = anon_sym_COLON_COLON,
  [aux_sym_comment_token1] = aux_sym_comment_token1,
  [anon_sym_REM] = anon_sym_REM,
  [anon_sym_Rem] = anon_sym_Rem,
  [anon_sym_rem] = anon_sym_rem,
  [anon_sym_SET] = anon_sym_SET,
  [anon_sym_Set] = anon_sym_Set,
  [anon_sym_set] = anon_sym_set,
  [anon_sym_SLASHA] = anon_sym_SLASHA,
  [anon_sym_EQ] = anon_sym_EQ,
  [anon_sym_PERCENT] = anon_sym_PERCENT,
  [anon_sym_ECHO] = anon_sym_ECHO,
  [anon_sym_IF] = anon_sym_IF,
  [anon_sym_GOTO] = anon_sym_GOTO,
  [anon_sym_EXIT] = anon_sym_EXIT,
  [anon_sym_FOR] = anon_sym_FOR,
  [anon_sym_PAUSE] = anon_sym_PAUSE,
  [anon_sym_CLS] = anon_sym_CLS,
  [anon_sym_echo] = anon_sym_echo,
  [anon_sym_if] = anon_sym_if,
  [anon_sym_goto] = anon_sym_goto,
  [anon_sym_exit] = anon_sym_exit,
  [anon_sym_for] = anon_sym_for,
  [anon_sym_pause] = anon_sym_pause,
  [anon_sym_cls] = anon_sym_cls,
  [anon_sym_VER] = anon_sym_VER,
  [anon_sym_ASSOC] = anon_sym_ASSOC,
  [anon_sym_CD] = anon_sym_CD,
  [anon_sym_COPY] = anon_sym_COPY,
  [anon_sym_DEL] = anon_sym_DEL,
  [anon_sym_DIR] = anon_sym_DIR,
  [anon_sym_DATE] = anon_sym_DATE,
  [anon_sym_MD] = anon_sym_MD,
  [anon_sym_MOVE] = anon_sym_MOVE,
  [anon_sym_PATH] = anon_sym_PATH,
  [anon_sym_PROMPT] = anon_sym_PROMPT,
  [anon_sym_RD] = anon_sym_RD,
  [anon_sym_REN] = anon_sym_REN,
  [anon_sym_START] = anon_sym_START,
  [anon_sym_TIME] = anon_sym_TIME,
  [anon_sym_TYPE] = anon_sym_TYPE,
  [anon_sym_VOL] = anon_sym_VOL,
  [anon_sym_ATTRIB] = anon_sym_ATTRIB,
  [anon_sym_CHKDSK] = anon_sym_CHKDSK,
  [anon_sym_CHOICE] = anon_sym_CHOICE,
  [anon_sym_CMD] = anon_sym_CMD,
  [anon_sym_COMP] = anon_sym_COMP,
  [anon_sym_CONVERT] = anon_sym_CONVERT,
  [anon_sym_DRIVERQUERY] = anon_sym_DRIVERQUERY,
  [anon_sym_EXPAND] = anon_sym_EXPAND,
  [anon_sym_FIND] = anon_sym_FIND,
  [anon_sym_FORMAT] = anon_sym_FORMAT,
  [anon_sym_HELP] = anon_sym_HELP,
  [anon_sym_IPCONFIG] = anon_sym_IPCONFIG,
  [anon_sym_LABEL] = anon_sym_LABEL,
  [anon_sym_NET] = anon_sym_NET,
  [anon_sym_PING] = anon_sym_PING,
  [anon_sym_SHUTDOWN] = anon_sym_SHUTDOWN,
  [anon_sym_SORT] = anon_sym_SORT,
  [anon_sym_SUBST] = anon_sym_SUBST,
  [anon_sym_SYSTEMINFO] = anon_sym_SYSTEMINFO,
  [anon_sym_TASKKILL] = anon_sym_TASKKILL,
  [anon_sym_TASKLIST] = anon_sym_TASKLIST,
  [anon_sym_XCOPY] = anon_sym_XCOPY,
  [anon_sym_TREE] = anon_sym_TREE,
  [anon_sym_FC] = anon_sym_FC,
  [anon_sym_DISKPART] = anon_sym_DISKPART,
  [anon_sym_TITLE] = anon_sym_TITLE,
  [anon_sym_ver] = anon_sym_ver,
  [anon_sym_assoc] = anon_sym_assoc,
  [anon_sym_cd] = anon_sym_cd,
  [anon_sym_copy] = anon_sym_copy,
  [anon_sym_del] = anon_sym_del,
  [anon_sym_dir] = anon_sym_dir,
  [anon_sym_date] = anon_sym_date,
  [anon_sym_md] = anon_sym_md,
  [anon_sym_move] = anon_sym_move,
  [anon_sym_path] = anon_sym_path,
  [anon_sym_prompt] = anon_sym_prompt,
  [anon_sym_rd] = anon_sym_rd,
  [anon_sym_ren] = anon_sym_ren,
  [anon_sym_start] = anon_sym_start,
  [anon_sym_time] = anon_sym_time,
  [anon_sym_type] = anon_sym_type,
  [anon_sym_vol] = anon_sym_vol,
  [anon_sym_attrib] = anon_sym_attrib,
  [anon_sym_chkdsk] = anon_sym_chkdsk,
  [anon_sym_choice] = anon_sym_choice,
  [anon_sym_cmd] = anon_sym_cmd,
  [anon_sym_comp] = anon_sym_comp,
  [anon_sym_convert] = anon_sym_convert,
  [anon_sym_driverquery] = anon_sym_driverquery,
  [anon_sym_expand] = anon_sym_expand,
  [anon_sym_find] = anon_sym_find,
  [anon_sym_format] = anon_sym_format,
  [anon_sym_help] = anon_sym_help,
  [anon_sym_ipconfig] = anon_sym_ipconfig,
  [anon_sym_label] = anon_sym_label,
  [anon_sym_net] = anon_sym_net,
  [anon_sym_ping] = anon_sym_ping,
  [anon_sym_shutdown] = anon_sym_shutdown,
  [anon_sym_sort] = anon_sym_sort,
  [anon_sym_subst] = anon_sym_subst,
  [anon_sym_systeminfo] = anon_sym_systeminfo,
  [anon_sym_taskkill] = anon_sym_taskkill,
  [anon_sym_tasklist] = anon_sym_tasklist,
  [anon_sym_xcopy] = anon_sym_xcopy,
  [anon_sym_tree] = anon_sym_tree,
  [anon_sym_fc] = anon_sym_fc,
  [anon_sym_diskpart] = anon_sym_diskpart,
  [anon_sym_title] = anon_sym_title,
  [anon_sym_COLON] = anon_sym_COLON,
  [sym_identifier] = sym_identifier,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [aux_sym_string_token1] = aux_sym_string_token1,
  [sym_number] = sym_number,
  [sym_program] = sym_program,
  [sym_echooff] = sym_echooff,
  [sym_comment] = sym_comment,
  [sym_variable_declaration] = sym_variable_declaration,
  [sym_variable_reference] = sym_variable_reference,
  [sym_keyword] = sym_keyword,
  [sym_function_definition] = sym_function_definition,
  [sym_string] = sym_string,
  [aux_sym_program_repeat1] = aux_sym_program_repeat1,
  [aux_sym_string_repeat1] = aux_sym_string_repeat1,
  [anon_alias_sym_function_name] = anon_alias_sym_function_name,
  [anon_alias_sym_variable_name] = anon_alias_sym_variable_name,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_AT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_echooff] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON_COLON] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_comment_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_REM] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_Rem] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_rem] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SET] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_Set] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_set] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SLASHA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PERCENT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ECHO] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_IF] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GOTO] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EXIT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_FOR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PAUSE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CLS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_echo] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_if] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_goto] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_exit] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_for] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_pause] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_cls] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_VER] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ASSOC] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CD] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COPY] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DEL] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DIR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DATE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_MD] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_MOVE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PATH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PROMPT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RD] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_REN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_START] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TIME] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TYPE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_VOL] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ATTRIB] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CHKDSK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CHOICE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CMD] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMP] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CONVERT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DRIVERQUERY] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EXPAND] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_FIND] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_FORMAT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_HELP] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_IPCONFIG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LABEL] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_NET] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PING] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SHUTDOWN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SORT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SUBST] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SYSTEMINFO] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TASKKILL] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TASKLIST] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_XCOPY] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TREE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_FC] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DISKPART] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TITLE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ver] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_assoc] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_cd] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_copy] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_del] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_dir] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_date] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_md] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_move] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_path] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_prompt] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_rd] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ren] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_start] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_time] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_type] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_vol] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_attrib] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_chkdsk] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_choice] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_cmd] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_comp] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_convert] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_driverquery] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_expand] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_find] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_format] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_help] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ipconfig] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_label] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_net] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_ping] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_shutdown] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sort] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_subst] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_systeminfo] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_taskkill] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tasklist] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_xcopy] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_tree] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_fc] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_diskpart] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_title] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_string_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_number] = {
    .visible = true,
    .named = true,
  },
  [sym_program] = {
    .visible = true,
    .named = true,
  },
  [sym_echooff] = {
    .visible = true,
    .named = true,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_variable_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_variable_reference] = {
    .visible = true,
    .named = true,
  },
  [sym_keyword] = {
    .visible = true,
    .named = true,
  },
  [sym_function_definition] = {
    .visible = true,
    .named = true,
  },
  [sym_string] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_program_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_string_repeat1] = {
    .visible = false,
    .named = false,
  },
  [anon_alias_sym_function_name] = {
    .visible = true,
    .named = false,
  },
  [anon_alias_sym_variable_name] = {
    .visible = true,
    .named = false,
  },
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
  [1] = {
    [1] = anon_alias_sym_function_name,
  },
  [2] = {
    [2] = anon_alias_sym_function_name,
  },
  [3] = {
    [1] = anon_alias_sym_variable_name,
  },
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
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(294);
      ADVANCE_MAP(
        '"', 1206,
        '%', 615,
        '/', 3,
        ':', 917,
        '=', 614,
        '@', 295,
        'A', 1026,
        'C', 936,
        'D', 921,
        'E', 930,
        'F', 931,
        'G', 1006,
        'H', 952,
        'I', 962,
        'L', 919,
        'M', 937,
        'N', 955,
        'P', 920,
        'R', 938,
        'S', 956,
        'T', 922,
        'V', 954,
        'X', 933,
        'a', 1169,
        'c', 1078,
        'd', 1063,
        'e', 1072,
        'f', 1073,
        'g', 1150,
        'h', 1094,
        'i', 1104,
        'l', 1061,
        'm', 1079,
        'n', 1097,
        'p', 1062,
        'r', 1080,
        's', 1100,
        't', 1064,
        'v', 1096,
        'x', 1075,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(1210);
      if (('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1:
      if (lookahead == '\n') SKIP(1);
      if (lookahead == '"') ADVANCE(1206);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(1209);
      if (lookahead != 0) ADVANCE(1208);
      END_STATE();
    case 2:
      if (lookahead == '/') ADVANCE(3);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(2);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 3:
      if (lookahead == 'A') ADVANCE(613);
      END_STATE();
    case 4:
      if (lookahead == 'A') ADVANCE(14);
      END_STATE();
    case 5:
      if (lookahead == 'A') ADVANCE(131);
      if (lookahead == 'I') ADVANCE(79);
      if (lookahead == 'R') ADVANCE(85);
      END_STATE();
    case 6:
      if (lookahead == 'A') ADVANCE(132);
      if (lookahead == 'E') ADVANCE(67);
      if (lookahead == 'I') ADVANCE(101);
      if (lookahead == 'R') ADVANCE(61);
      END_STATE();
    case 7:
      if (lookahead == 'A') ADVANCE(113);
      if (lookahead == 'I') ADVANCE(76);
      if (lookahead == 'R') ADVANCE(45);
      if (lookahead == 'Y') ADVANCE(97);
      END_STATE();
    case 8:
      if (lookahead == 'A') ADVANCE(126);
      END_STATE();
    case 9:
      if (lookahead == 'A') ADVANCE(84);
      END_STATE();
    case 10:
      if (lookahead == 'A') ADVANCE(108);
      END_STATE();
    case 11:
      if (lookahead == 'A') ADVANCE(110);
      END_STATE();
    case 12:
      if (lookahead == 'B') ADVANCE(710);
      END_STATE();
    case 13:
      if (lookahead == 'B') ADVANCE(116);
      END_STATE();
    case 14:
      if (lookahead == 'B') ADVANCE(42);
      END_STATE();
    case 15:
      if (lookahead == 'C') ADVANCE(53);
      if (lookahead == 'X') ADVANCE(58);
      END_STATE();
    case 16:
      if (lookahead == 'C') ADVANCE(779);
      if (lookahead == 'I') ADVANCE(83);
      if (lookahead == 'O') ADVANCE(102);
      END_STATE();
    case 17:
      if (lookahead == 'C') ADVANCE(662);
      END_STATE();
    case 18:
      if (lookahead == 'C') ADVANCE(93);
      END_STATE();
    case 19:
      if (lookahead == 'C') ADVANCE(92);
      END_STATE();
    case 20:
      if (lookahead == 'C') ADVANCE(36);
      END_STATE();
    case 21:
      if (lookahead == 'D') ADVANCE(665);
      if (lookahead == 'H') ADVANCE(64);
      if (lookahead == 'L') ADVANCE(112);
      if (lookahead == 'M') ADVANCE(24);
      if (lookahead == 'O') ADVANCE(75);
      END_STATE();
    case 22:
      if (lookahead == 'D') ADVANCE(680);
      if (lookahead == 'O') ADVANCE(139);
      END_STATE();
    case 23:
      if (lookahead == 'D') ADVANCE(692);
      if (lookahead == 'E') ADVANCE(74);
      if (lookahead == 'e') ADVANCE(218);
      END_STATE();
    case 24:
      if (lookahead == 'D') ADVANCE(719);
      END_STATE();
    case 25:
      if (lookahead == 'D') ADVANCE(734);
      END_STATE();
    case 26:
      if (lookahead == 'D') ADVANCE(731);
      END_STATE();
    case 27:
      if (lookahead == 'D') ADVANCE(114);
      END_STATE();
    case 28:
      if (lookahead == 'D') ADVANCE(88);
      END_STATE();
    case 29:
      if (lookahead == 'E') ADVANCE(677);
      END_STATE();
    case 30:
      if (lookahead == 'E') ADVANCE(683);
      END_STATE();
    case 31:
      if (lookahead == 'E') ADVANCE(701);
      END_STATE();
    case 32:
      if (lookahead == 'E') ADVANCE(776);
      END_STATE();
    case 33:
      if (lookahead == 'E') ADVANCE(704);
      END_STATE();
    case 34:
      if (lookahead == 'E') ADVANCE(632);
      END_STATE();
    case 35:
      if (lookahead == 'E') ADVANCE(785);
      END_STATE();
    case 36:
      if (lookahead == 'E') ADVANCE(716);
      END_STATE();
    case 37:
      if (lookahead == 'E') ADVANCE(71);
      END_STATE();
    case 38:
      if (lookahead == 'E') ADVANCE(78);
      END_STATE();
    case 39:
      if (lookahead == 'E') ADVANCE(103);
      if (lookahead == 'O') ADVANCE(68);
      END_STATE();
    case 40:
      if (lookahead == 'E') ADVANCE(120);
      END_STATE();
    case 41:
      if (lookahead == 'E') ADVANCE(121);
      if (lookahead == 'H') ADVANCE(137);
      if (lookahead == 'O') ADVANCE(107);
      if (lookahead == 'T') ADVANCE(10);
      if (lookahead == 'U') ADVANCE(13);
      if (lookahead == 'Y') ADVANCE(119);
      if (lookahead == 'e') ADVANCE(266);
      END_STATE();
    case 42:
      if (lookahead == 'E') ADVANCE(69);
      END_STATE();
    case 43:
      if (lookahead == 'E') ADVANCE(104);
      END_STATE();
    case 44:
      if (lookahead == 'E') ADVANCE(106);
      END_STATE();
    case 45:
      if (lookahead == 'E') ADVANCE(32);
      END_STATE();
    case 46:
      if (lookahead == 'E') ADVANCE(109);
      END_STATE();
    case 47:
      if (lookahead == 'F') ADVANCE(620);
      if (lookahead == 'P') ADVANCE(19);
      END_STATE();
    case 48:
      if (lookahead == 'F') ADVANCE(54);
      END_STATE();
    case 49:
      if (lookahead == 'F') ADVANCE(89);
      END_STATE();
    case 50:
      if (lookahead == 'G') ADVANCE(752);
      END_STATE();
    case 51:
      if (lookahead == 'G') ADVANCE(743);
      END_STATE();
    case 52:
      if (lookahead == 'H') ADVANCE(686);
      END_STATE();
    case 53:
      if (lookahead == 'H') ADVANCE(86);
      END_STATE();
    case 54:
      if (lookahead == 'I') ADVANCE(51);
      END_STATE();
    case 55:
      if (lookahead == 'I') ADVANCE(20);
      END_STATE();
    case 56:
      if (lookahead == 'I') ADVANCE(12);
      END_STATE();
    case 57:
      if (lookahead == 'I') ADVANCE(82);
      END_STATE();
    case 58:
      if (lookahead == 'I') ADVANCE(122);
      if (lookahead == 'P') ADVANCE(9);
      END_STATE();
    case 59:
      if (lookahead == 'I') ADVANCE(72);
      END_STATE();
    case 60:
      if (lookahead == 'I') ADVANCE(118);
      END_STATE();
    case 61:
      if (lookahead == 'I') ADVANCE(141);
      END_STATE();
    case 62:
      if (lookahead == 'K') ADVANCE(65);
      END_STATE();
    case 63:
      if (lookahead == 'K') ADVANCE(713);
      END_STATE();
    case 64:
      if (lookahead == 'K') ADVANCE(27);
      if (lookahead == 'O') ADVANCE(55);
      END_STATE();
    case 65:
      if (lookahead == 'K') ADVANCE(59);
      if (lookahead == 'L') ADVANCE(60);
      END_STATE();
    case 66:
      if (lookahead == 'K') ADVANCE(99);
      END_STATE();
    case 67:
      if (lookahead == 'L') ADVANCE(671);
      END_STATE();
    case 68:
      if (lookahead == 'L') ADVANCE(707);
      END_STATE();
    case 69:
      if (lookahead == 'L') ADVANCE(746);
      END_STATE();
    case 70:
      if (lookahead == 'L') ADVANCE(767);
      END_STATE();
    case 71:
      if (lookahead == 'L') ADVANCE(95);
      END_STATE();
    case 72:
      if (lookahead == 'L') ADVANCE(70);
      END_STATE();
    case 73:
      if (lookahead == 'L') ADVANCE(35);
      END_STATE();
    case 74:
      if (lookahead == 'M') ADVANCE(595);
      if (lookahead == 'N') ADVANCE(695);
      END_STATE();
    case 75:
      if (lookahead == 'M') ADVANCE(94);
      if (lookahead == 'N') ADVANCE(140);
      if (lookahead == 'P') ADVANCE(143);
      END_STATE();
    case 76:
      if (lookahead == 'M') ADVANCE(31);
      if (lookahead == 'T') ADVANCE(73);
      END_STATE();
    case 77:
      if (lookahead == 'M') ADVANCE(98);
      END_STATE();
    case 78:
      if (lookahead == 'M') ADVANCE(57);
      END_STATE();
    case 79:
      if (lookahead == 'N') ADVANCE(50);
      END_STATE();
    case 80:
      if (lookahead == 'N') ADVANCE(48);
      END_STATE();
    case 81:
      if (lookahead == 'N') ADVANCE(755);
      END_STATE();
    case 82:
      if (lookahead == 'N') ADVANCE(49);
      END_STATE();
    case 83:
      if (lookahead == 'N') ADVANCE(25);
      END_STATE();
    case 84:
      if (lookahead == 'N') ADVANCE(26);
      END_STATE();
    case 85:
      if (lookahead == 'O') ADVANCE(77);
      END_STATE();
    case 86:
      if (lookahead == 'O') ADVANCE(617);
      END_STATE();
    case 87:
      if (lookahead == 'O') ADVANCE(623);
      END_STATE();
    case 88:
      if (lookahead == 'O') ADVANCE(142);
      END_STATE();
    case 89:
      if (lookahead == 'O') ADVANCE(764);
      END_STATE();
    case 90:
      if (lookahead == 'O') ADVANCE(17);
      END_STATE();
    case 91:
      if (lookahead == 'O') ADVANCE(135);
      END_STATE();
    case 92:
      if (lookahead == 'O') ADVANCE(80);
      END_STATE();
    case 93:
      if (lookahead == 'O') ADVANCE(96);
      END_STATE();
    case 94:
      if (lookahead == 'P') ADVANCE(722);
      END_STATE();
    case 95:
      if (lookahead == 'P') ADVANCE(740);
      END_STATE();
    case 96:
      if (lookahead == 'P') ADVANCE(144);
      END_STATE();
    case 97:
      if (lookahead == 'P') ADVANCE(33);
      END_STATE();
    case 98:
      if (lookahead == 'P') ADVANCE(127);
      END_STATE();
    case 99:
      if (lookahead == 'P') ADVANCE(11);
      END_STATE();
    case 100:
      if (lookahead == 'Q') ADVANCE(138);
      END_STATE();
    case 101:
      if (lookahead == 'R') ADVANCE(674);
      if (lookahead == 'S') ADVANCE(66);
      END_STATE();
    case 102:
      if (lookahead == 'R') ADVANCE(631);
      END_STATE();
    case 103:
      if (lookahead == 'R') ADVANCE(659);
      END_STATE();
    case 104:
      if (lookahead == 'R') ADVANCE(100);
      END_STATE();
    case 105:
      if (lookahead == 'R') ADVANCE(56);
      END_STATE();
    case 106:
      if (lookahead == 'R') ADVANCE(145);
      END_STATE();
    case 107:
      if (lookahead == 'R') ADVANCE(123);
      END_STATE();
    case 108:
      if (lookahead == 'R') ADVANCE(124);
      END_STATE();
    case 109:
      if (lookahead == 'R') ADVANCE(128);
      END_STATE();
    case 110:
      if (lookahead == 'R') ADVANCE(129);
      END_STATE();
    case 111:
      if (lookahead == 'S') ADVANCE(115);
      if (lookahead == 'T') ADVANCE(134);
      END_STATE();
    case 112:
      if (lookahead == 'S') ADVANCE(635);
      END_STATE();
    case 113:
      if (lookahead == 'S') ADVANCE(62);
      END_STATE();
    case 114:
      if (lookahead == 'S') ADVANCE(63);
      END_STATE();
    case 115:
      if (lookahead == 'S') ADVANCE(90);
      END_STATE();
    case 116:
      if (lookahead == 'S') ADVANCE(125);
      END_STATE();
    case 117:
      if (lookahead == 'S') ADVANCE(34);
      END_STATE();
    case 118:
      if (lookahead == 'S') ADVANCE(130);
      END_STATE();
    case 119:
      if (lookahead == 'S') ADVANCE(136);
      END_STATE();
    case 120:
      if (lookahead == 'T') ADVANCE(749);
      END_STATE();
    case 121:
      if (lookahead == 'T') ADVANCE(604);
      END_STATE();
    case 122:
      if (lookahead == 'T') ADVANCE(626);
      END_STATE();
    case 123:
      if (lookahead == 'T') ADVANCE(758);
      END_STATE();
    case 124:
      if (lookahead == 'T') ADVANCE(698);
      END_STATE();
    case 125:
      if (lookahead == 'T') ADVANCE(761);
      END_STATE();
    case 126:
      if (lookahead == 'T') ADVANCE(737);
      END_STATE();
    case 127:
      if (lookahead == 'T') ADVANCE(689);
      END_STATE();
    case 128:
      if (lookahead == 'T') ADVANCE(725);
      END_STATE();
    case 129:
      if (lookahead == 'T') ADVANCE(782);
      END_STATE();
    case 130:
      if (lookahead == 'T') ADVANCE(770);
      END_STATE();
    case 131:
      if (lookahead == 'T') ADVANCE(52);
      if (lookahead == 'U') ADVANCE(117);
      END_STATE();
    case 132:
      if (lookahead == 'T') ADVANCE(29);
      END_STATE();
    case 133:
      if (lookahead == 'T') ADVANCE(28);
      END_STATE();
    case 134:
      if (lookahead == 'T') ADVANCE(105);
      END_STATE();
    case 135:
      if (lookahead == 'T') ADVANCE(87);
      END_STATE();
    case 136:
      if (lookahead == 'T') ADVANCE(38);
      END_STATE();
    case 137:
      if (lookahead == 'U') ADVANCE(133);
      END_STATE();
    case 138:
      if (lookahead == 'U') ADVANCE(44);
      END_STATE();
    case 139:
      if (lookahead == 'V') ADVANCE(30);
      END_STATE();
    case 140:
      if (lookahead == 'V') ADVANCE(46);
      END_STATE();
    case 141:
      if (lookahead == 'V') ADVANCE(43);
      END_STATE();
    case 142:
      if (lookahead == 'W') ADVANCE(81);
      END_STATE();
    case 143:
      if (lookahead == 'Y') ADVANCE(668);
      END_STATE();
    case 144:
      if (lookahead == 'Y') ADVANCE(773);
      END_STATE();
    case 145:
      if (lookahead == 'Y') ADVANCE(728);
      END_STATE();
    case 146:
      if (lookahead == 'a') ADVANCE(156);
      END_STATE();
    case 147:
      if (lookahead == 'a') ADVANCE(278);
      if (lookahead == 'i') ADVANCE(224);
      if (lookahead == 'r') ADVANCE(236);
      END_STATE();
    case 148:
      if (lookahead == 'a') ADVANCE(279);
      if (lookahead == 'e') ADVANCE(211);
      if (lookahead == 'i') ADVANCE(247);
      if (lookahead == 'r') ADVANCE(205);
      END_STATE();
    case 149:
      if (lookahead == 'a') ADVANCE(259);
      if (lookahead == 'i') ADVANCE(221);
      if (lookahead == 'r') ADVANCE(187);
      if (lookahead == 'y') ADVANCE(243);
      END_STATE();
    case 150:
      if (lookahead == 'a') ADVANCE(273);
      END_STATE();
    case 151:
      if (lookahead == 'a') ADVANCE(229);
      END_STATE();
    case 152:
      if (lookahead == 'a') ADVANCE(254);
      END_STATE();
    case 153:
      if (lookahead == 'a') ADVANCE(256);
      END_STATE();
    case 154:
      if (lookahead == 'b') ADVANCE(839);
      END_STATE();
    case 155:
      if (lookahead == 'b') ADVANCE(263);
      END_STATE();
    case 156:
      if (lookahead == 'b') ADVANCE(183);
      END_STATE();
    case 157:
      if (lookahead == 'c') ADVANCE(197);
      if (lookahead == 'x') ADVANCE(203);
      END_STATE();
    case 158:
      if (lookahead == 'c') ADVANCE(908);
      if (lookahead == 'i') ADVANCE(227);
      if (lookahead == 'o') ADVANCE(248);
      END_STATE();
    case 159:
      if (lookahead == 'c') ADVANCE(791);
      END_STATE();
    case 160:
      if (lookahead == 'c') ADVANCE(239);
      END_STATE();
    case 161:
      if (lookahead == 'c') ADVANCE(237);
      END_STATE();
    case 162:
      if (lookahead == 'c') ADVANCE(178);
      END_STATE();
    case 163:
      if (lookahead == 'd') ADVANCE(794);
      if (lookahead == 'h') ADVANCE(208);
      if (lookahead == 'l') ADVANCE(258);
      if (lookahead == 'm') ADVANCE(166);
      if (lookahead == 'o') ADVANCE(220);
      END_STATE();
    case 164:
      if (lookahead == 'd') ADVANCE(809);
      if (lookahead == 'o') ADVANCE(286);
      END_STATE();
    case 165:
      if (lookahead == 'd') ADVANCE(821);
      if (lookahead == 'e') ADVANCE(219);
      END_STATE();
    case 166:
      if (lookahead == 'd') ADVANCE(848);
      END_STATE();
    case 167:
      if (lookahead == 'd') ADVANCE(863);
      END_STATE();
    case 168:
      if (lookahead == 'd') ADVANCE(860);
      END_STATE();
    case 169:
      if (lookahead == 'd') ADVANCE(260);
      END_STATE();
    case 170:
      if (lookahead == 'd') ADVANCE(233);
      END_STATE();
    case 171:
      if (lookahead == 'e') ADVANCE(806);
      END_STATE();
    case 172:
      if (lookahead == 'e') ADVANCE(812);
      END_STATE();
    case 173:
      if (lookahead == 'e') ADVANCE(830);
      END_STATE();
    case 174:
      if (lookahead == 'e') ADVANCE(905);
      END_STATE();
    case 175:
      if (lookahead == 'e') ADVANCE(833);
      END_STATE();
    case 176:
      if (lookahead == 'e') ADVANCE(653);
      END_STATE();
    case 177:
      if (lookahead == 'e') ADVANCE(914);
      END_STATE();
    case 178:
      if (lookahead == 'e') ADVANCE(845);
      END_STATE();
    case 179:
      if (lookahead == 'e') ADVANCE(215);
      END_STATE();
    case 180:
      if (lookahead == 'e') ADVANCE(223);
      END_STATE();
    case 181:
      if (lookahead == 'e') ADVANCE(249);
      if (lookahead == 'o') ADVANCE(212);
      END_STATE();
    case 182:
      if (lookahead == 'e') ADVANCE(267);
      END_STATE();
    case 183:
      if (lookahead == 'e') ADVANCE(213);
      END_STATE();
    case 184:
      if (lookahead == 'e') ADVANCE(250);
      END_STATE();
    case 185:
      if (lookahead == 'e') ADVANCE(268);
      if (lookahead == 'h') ADVANCE(284);
      if (lookahead == 'o') ADVANCE(253);
      if (lookahead == 't') ADVANCE(152);
      if (lookahead == 'u') ADVANCE(155);
      if (lookahead == 'y') ADVANCE(265);
      END_STATE();
    case 186:
      if (lookahead == 'e') ADVANCE(252);
      END_STATE();
    case 187:
      if (lookahead == 'e') ADVANCE(174);
      END_STATE();
    case 188:
      if (lookahead == 'e') ADVANCE(255);
      END_STATE();
    case 189:
      if (lookahead == 'f') ADVANCE(297);
      END_STATE();
    case 190:
      if (lookahead == 'f') ADVANCE(641);
      if (lookahead == 'p') ADVANCE(161);
      END_STATE();
    case 191:
      if (lookahead == 'f') ADVANCE(189);
      END_STATE();
    case 192:
      if (lookahead == 'f') ADVANCE(198);
      END_STATE();
    case 193:
      if (lookahead == 'f') ADVANCE(234);
      END_STATE();
    case 194:
      if (lookahead == 'g') ADVANCE(881);
      END_STATE();
    case 195:
      if (lookahead == 'g') ADVANCE(872);
      END_STATE();
    case 196:
      if (lookahead == 'h') ADVANCE(815);
      END_STATE();
    case 197:
      if (lookahead == 'h') ADVANCE(231);
      END_STATE();
    case 198:
      if (lookahead == 'i') ADVANCE(195);
      END_STATE();
    case 199:
      if (lookahead == 'i') ADVANCE(162);
      END_STATE();
    case 200:
      if (lookahead == 'i') ADVANCE(154);
      END_STATE();
    case 201:
      if (lookahead == 'i') ADVANCE(228);
      END_STATE();
    case 202:
      if (lookahead == 'i') ADVANCE(216);
      END_STATE();
    case 203:
      if (lookahead == 'i') ADVANCE(269);
      if (lookahead == 'p') ADVANCE(151);
      END_STATE();
    case 204:
      if (lookahead == 'i') ADVANCE(264);
      END_STATE();
    case 205:
      if (lookahead == 'i') ADVANCE(288);
      END_STATE();
    case 206:
      if (lookahead == 'k') ADVANCE(209);
      END_STATE();
    case 207:
      if (lookahead == 'k') ADVANCE(842);
      END_STATE();
    case 208:
      if (lookahead == 'k') ADVANCE(169);
      if (lookahead == 'o') ADVANCE(199);
      END_STATE();
    case 209:
      if (lookahead == 'k') ADVANCE(202);
      if (lookahead == 'l') ADVANCE(204);
      END_STATE();
    case 210:
      if (lookahead == 'k') ADVANCE(245);
      END_STATE();
    case 211:
      if (lookahead == 'l') ADVANCE(800);
      END_STATE();
    case 212:
      if (lookahead == 'l') ADVANCE(836);
      END_STATE();
    case 213:
      if (lookahead == 'l') ADVANCE(875);
      END_STATE();
    case 214:
      if (lookahead == 'l') ADVANCE(896);
      END_STATE();
    case 215:
      if (lookahead == 'l') ADVANCE(241);
      END_STATE();
    case 216:
      if (lookahead == 'l') ADVANCE(214);
      END_STATE();
    case 217:
      if (lookahead == 'l') ADVANCE(177);
      END_STATE();
    case 218:
      if (lookahead == 'm') ADVANCE(598);
      END_STATE();
    case 219:
      if (lookahead == 'm') ADVANCE(601);
      if (lookahead == 'n') ADVANCE(824);
      END_STATE();
    case 220:
      if (lookahead == 'm') ADVANCE(240);
      if (lookahead == 'n') ADVANCE(287);
      if (lookahead == 'p') ADVANCE(290);
      END_STATE();
    case 221:
      if (lookahead == 'm') ADVANCE(173);
      if (lookahead == 't') ADVANCE(217);
      END_STATE();
    case 222:
      if (lookahead == 'm') ADVANCE(244);
      END_STATE();
    case 223:
      if (lookahead == 'm') ADVANCE(201);
      END_STATE();
    case 224:
      if (lookahead == 'n') ADVANCE(194);
      END_STATE();
    case 225:
      if (lookahead == 'n') ADVANCE(884);
      END_STATE();
    case 226:
      if (lookahead == 'n') ADVANCE(192);
      END_STATE();
    case 227:
      if (lookahead == 'n') ADVANCE(167);
      END_STATE();
    case 228:
      if (lookahead == 'n') ADVANCE(193);
      END_STATE();
    case 229:
      if (lookahead == 'n') ADVANCE(168);
      END_STATE();
    case 230:
      if (lookahead == 'o') ADVANCE(191);
      END_STATE();
    case 231:
      if (lookahead == 'o') ADVANCE(638);
      END_STATE();
    case 232:
      if (lookahead == 'o') ADVANCE(644);
      END_STATE();
    case 233:
      if (lookahead == 'o') ADVANCE(289);
      END_STATE();
    case 234:
      if (lookahead == 'o') ADVANCE(893);
      END_STATE();
    case 235:
      if (lookahead == 'o') ADVANCE(159);
      END_STATE();
    case 236:
      if (lookahead == 'o') ADVANCE(222);
      END_STATE();
    case 237:
      if (lookahead == 'o') ADVANCE(226);
      END_STATE();
    case 238:
      if (lookahead == 'o') ADVANCE(282);
      END_STATE();
    case 239:
      if (lookahead == 'o') ADVANCE(242);
      END_STATE();
    case 240:
      if (lookahead == 'p') ADVANCE(851);
      END_STATE();
    case 241:
      if (lookahead == 'p') ADVANCE(869);
      END_STATE();
    case 242:
      if (lookahead == 'p') ADVANCE(291);
      END_STATE();
    case 243:
      if (lookahead == 'p') ADVANCE(175);
      END_STATE();
    case 244:
      if (lookahead == 'p') ADVANCE(274);
      END_STATE();
    case 245:
      if (lookahead == 'p') ADVANCE(153);
      END_STATE();
    case 246:
      if (lookahead == 'q') ADVANCE(285);
      END_STATE();
    case 247:
      if (lookahead == 'r') ADVANCE(803);
      if (lookahead == 's') ADVANCE(210);
      END_STATE();
    case 248:
      if (lookahead == 'r') ADVANCE(651);
      END_STATE();
    case 249:
      if (lookahead == 'r') ADVANCE(788);
      END_STATE();
    case 250:
      if (lookahead == 'r') ADVANCE(246);
      END_STATE();
    case 251:
      if (lookahead == 'r') ADVANCE(200);
      END_STATE();
    case 252:
      if (lookahead == 'r') ADVANCE(292);
      END_STATE();
    case 253:
      if (lookahead == 'r') ADVANCE(270);
      END_STATE();
    case 254:
      if (lookahead == 'r') ADVANCE(271);
      END_STATE();
    case 255:
      if (lookahead == 'r') ADVANCE(275);
      END_STATE();
    case 256:
      if (lookahead == 'r') ADVANCE(276);
      END_STATE();
    case 257:
      if (lookahead == 's') ADVANCE(261);
      if (lookahead == 't') ADVANCE(281);
      END_STATE();
    case 258:
      if (lookahead == 's') ADVANCE(656);
      END_STATE();
    case 259:
      if (lookahead == 's') ADVANCE(206);
      END_STATE();
    case 260:
      if (lookahead == 's') ADVANCE(207);
      END_STATE();
    case 261:
      if (lookahead == 's') ADVANCE(235);
      END_STATE();
    case 262:
      if (lookahead == 's') ADVANCE(176);
      END_STATE();
    case 263:
      if (lookahead == 's') ADVANCE(272);
      END_STATE();
    case 264:
      if (lookahead == 's') ADVANCE(277);
      END_STATE();
    case 265:
      if (lookahead == 's') ADVANCE(283);
      END_STATE();
    case 266:
      if (lookahead == 't') ADVANCE(607);
      END_STATE();
    case 267:
      if (lookahead == 't') ADVANCE(878);
      END_STATE();
    case 268:
      if (lookahead == 't') ADVANCE(610);
      END_STATE();
    case 269:
      if (lookahead == 't') ADVANCE(647);
      END_STATE();
    case 270:
      if (lookahead == 't') ADVANCE(887);
      END_STATE();
    case 271:
      if (lookahead == 't') ADVANCE(827);
      END_STATE();
    case 272:
      if (lookahead == 't') ADVANCE(890);
      END_STATE();
    case 273:
      if (lookahead == 't') ADVANCE(866);
      END_STATE();
    case 274:
      if (lookahead == 't') ADVANCE(818);
      END_STATE();
    case 275:
      if (lookahead == 't') ADVANCE(854);
      END_STATE();
    case 276:
      if (lookahead == 't') ADVANCE(911);
      END_STATE();
    case 277:
      if (lookahead == 't') ADVANCE(899);
      END_STATE();
    case 278:
      if (lookahead == 't') ADVANCE(196);
      if (lookahead == 'u') ADVANCE(262);
      END_STATE();
    case 279:
      if (lookahead == 't') ADVANCE(171);
      END_STATE();
    case 280:
      if (lookahead == 't') ADVANCE(170);
      END_STATE();
    case 281:
      if (lookahead == 't') ADVANCE(251);
      END_STATE();
    case 282:
      if (lookahead == 't') ADVANCE(232);
      END_STATE();
    case 283:
      if (lookahead == 't') ADVANCE(180);
      END_STATE();
    case 284:
      if (lookahead == 'u') ADVANCE(280);
      END_STATE();
    case 285:
      if (lookahead == 'u') ADVANCE(186);
      END_STATE();
    case 286:
      if (lookahead == 'v') ADVANCE(172);
      END_STATE();
    case 287:
      if (lookahead == 'v') ADVANCE(188);
      END_STATE();
    case 288:
      if (lookahead == 'v') ADVANCE(184);
      END_STATE();
    case 289:
      if (lookahead == 'w') ADVANCE(225);
      END_STATE();
    case 290:
      if (lookahead == 'y') ADVANCE(797);
      END_STATE();
    case 291:
      if (lookahead == 'y') ADVANCE(902);
      END_STATE();
    case 292:
      if (lookahead == 'y') ADVANCE(857);
      END_STATE();
    case 293:
      if (eof) ADVANCE(294);
      ADVANCE_MAP(
        '"', 1206,
        '%', 615,
        ':', 917,
        '@', 295,
        'A', 111,
        'C', 21,
        'D', 6,
        'E', 15,
        'F', 16,
        'G', 91,
        'H', 37,
        'I', 47,
        'L', 4,
        'M', 22,
        'N', 40,
        'P', 5,
        'R', 23,
        'S', 41,
        'T', 7,
        'V', 39,
        'X', 18,
        'a', 257,
        'c', 163,
        'd', 148,
        'e', 157,
        'f', 158,
        'g', 238,
        'h', 179,
        'i', 190,
        'l', 146,
        'm', 164,
        'n', 182,
        'p', 147,
        'r', 165,
        's', 185,
        't', 149,
        'v', 181,
        'x', 160,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(293);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(1210);
      END_STATE();
    case 294:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 295:
      ACCEPT_TOKEN(anon_sym_AT);
      END_STATE();
    case 296:
      ACCEPT_TOKEN(anon_sym_AT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 297:
      ACCEPT_TOKEN(anon_sym_echooff);
      END_STATE();
    case 298:
      ACCEPT_TOKEN(anon_sym_echooff);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 299:
      ACCEPT_TOKEN(anon_sym_COLON_COLON);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(anon_sym_COLON_COLON);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 301:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      ADVANCE_MAP(
        '"', 1207,
        '%', 616,
        ':', 918,
        '@', 296,
        'A', 409,
        'C', 319,
        'D', 304,
        'E', 313,
        'F', 314,
        'G', 389,
        'H', 335,
        'I', 345,
        'L', 302,
        'M', 320,
        'N', 338,
        'P', 303,
        'R', 321,
        'S', 339,
        'T', 305,
        'V', 337,
        'X', 316,
        'a', 555,
        'c', 461,
        'd', 446,
        'e', 455,
        'f', 456,
        'g', 536,
        'h', 477,
        'i', 487,
        'l', 444,
        'm', 462,
        'n', 480,
        'p', 445,
        'r', 463,
        's', 483,
        't', 447,
        'v', 479,
        'x', 458,
      );
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(301);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(592);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(593);
      END_STATE();
    case 302:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(312);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 303:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(429);
      if (lookahead == 'I') ADVANCE(377);
      if (lookahead == 'R') ADVANCE(383);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 304:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(430);
      if (lookahead == 'E') ADVANCE(365);
      if (lookahead == 'I') ADVANCE(399);
      if (lookahead == 'R') ADVANCE(359);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 305:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(411);
      if (lookahead == 'I') ADVANCE(374);
      if (lookahead == 'R') ADVANCE(343);
      if (lookahead == 'Y') ADVANCE(395);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 306:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(424);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 307:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(382);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 308:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(406);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 309:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'A') ADVANCE(408);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 310:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'B') ADVANCE(712);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 311:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'B') ADVANCE(414);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 312:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'B') ADVANCE(340);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(351);
      if (lookahead == 'X') ADVANCE(356);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 314:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(781);
      if (lookahead == 'I') ADVANCE(381);
      if (lookahead == 'O') ADVANCE(400);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 315:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(664);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 316:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(391);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 317:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(390);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 318:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'C') ADVANCE(334);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 319:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(667);
      if (lookahead == 'H') ADVANCE(362);
      if (lookahead == 'L') ADVANCE(410);
      if (lookahead == 'M') ADVANCE(322);
      if (lookahead == 'O') ADVANCE(373);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 320:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(682);
      if (lookahead == 'O') ADVANCE(437);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(694);
      if (lookahead == 'E') ADVANCE(372);
      if (lookahead == 'e') ADVANCE(516);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 322:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(721);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 323:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(736);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 324:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(733);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 325:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(412);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 326:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'D') ADVANCE(386);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 327:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(679);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(685);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 329:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(703);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 330:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(778);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 331:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(706);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 332:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(634);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 333:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(787);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 334:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(718);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 335:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(369);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(376);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 337:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(401);
      if (lookahead == 'O') ADVANCE(366);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 338:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(418);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 339:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(419);
      if (lookahead == 'H') ADVANCE(435);
      if (lookahead == 'O') ADVANCE(405);
      if (lookahead == 'T') ADVANCE(308);
      if (lookahead == 'U') ADVANCE(311);
      if (lookahead == 'Y') ADVANCE(417);
      if (lookahead == 'e') ADVANCE(564);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 340:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(367);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 341:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(402);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(404);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 343:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(330);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 344:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'E') ADVANCE(407);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 345:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'F') ADVANCE(622);
      if (lookahead == 'P') ADVANCE(317);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 346:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'F') ADVANCE(352);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 347:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'F') ADVANCE(387);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 348:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'G') ADVANCE(754);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 349:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'G') ADVANCE(745);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 350:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'H') ADVANCE(688);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 351:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'H') ADVANCE(384);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 352:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(349);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 353:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(318);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 354:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(310);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 355:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(380);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 356:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(420);
      if (lookahead == 'P') ADVANCE(307);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 357:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(370);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(416);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 359:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'I') ADVANCE(439);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 360:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'K') ADVANCE(363);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 361:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'K') ADVANCE(715);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 362:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'K') ADVANCE(325);
      if (lookahead == 'O') ADVANCE(353);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 363:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'K') ADVANCE(357);
      if (lookahead == 'L') ADVANCE(358);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 364:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'K') ADVANCE(397);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 365:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(673);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 366:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(709);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 367:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(748);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 368:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(769);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 369:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(393);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 370:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(368);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 371:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'L') ADVANCE(333);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 372:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'M') ADVANCE(597);
      if (lookahead == 'N') ADVANCE(697);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 373:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'M') ADVANCE(392);
      if (lookahead == 'N') ADVANCE(438);
      if (lookahead == 'P') ADVANCE(441);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 374:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'M') ADVANCE(329);
      if (lookahead == 'T') ADVANCE(371);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 375:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'M') ADVANCE(396);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 376:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'M') ADVANCE(355);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 377:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(348);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 378:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(346);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 379:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(757);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 380:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(347);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 381:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(323);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 382:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'N') ADVANCE(324);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 383:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(375);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 384:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(619);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 385:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(625);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 386:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(440);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 387:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(766);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 388:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(315);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 389:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(433);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 390:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(378);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 391:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'O') ADVANCE(394);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 392:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(724);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 393:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(742);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 394:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(442);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 395:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(331);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 396:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(425);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 397:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'P') ADVANCE(309);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 398:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'Q') ADVANCE(436);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 399:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(676);
      if (lookahead == 'S') ADVANCE(364);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 400:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(630);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 401:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(661);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 402:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(398);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 403:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(354);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 404:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(443);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 405:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(421);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 406:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(422);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 407:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(426);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 408:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'R') ADVANCE(427);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 409:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(413);
      if (lookahead == 'T') ADVANCE(432);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 410:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(637);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 411:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(360);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 412:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(361);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 413:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(388);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 414:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(423);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 415:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(332);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 416:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(428);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 417:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'S') ADVANCE(434);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 418:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(751);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 419:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(606);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 420:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(628);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 421:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(760);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 422:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(700);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 423:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(763);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 424:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(739);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 425:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(691);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 426:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(727);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 427:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(784);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 428:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(772);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 429:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(350);
      if (lookahead == 'U') ADVANCE(415);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 430:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(327);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 431:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(326);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 432:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(403);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 433:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(385);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 434:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'T') ADVANCE(336);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 435:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'U') ADVANCE(431);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 436:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'U') ADVANCE(342);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 437:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'V') ADVANCE(328);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 438:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'V') ADVANCE(344);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 439:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'V') ADVANCE(341);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 440:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'W') ADVANCE(379);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 441:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'Y') ADVANCE(670);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 442:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'Y') ADVANCE(775);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 443:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'Y') ADVANCE(730);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 444:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(454);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 445:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(576);
      if (lookahead == 'i') ADVANCE(522);
      if (lookahead == 'r') ADVANCE(533);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 446:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(577);
      if (lookahead == 'e') ADVANCE(509);
      if (lookahead == 'i') ADVANCE(545);
      if (lookahead == 'r') ADVANCE(503);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 447:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(557);
      if (lookahead == 'i') ADVANCE(519);
      if (lookahead == 'r') ADVANCE(485);
      if (lookahead == 'y') ADVANCE(541);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 448:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(571);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 449:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(527);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 450:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(552);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 451:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'a') ADVANCE(554);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 452:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'b') ADVANCE(841);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 453:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'b') ADVANCE(561);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 454:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'b') ADVANCE(481);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 455:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(495);
      if (lookahead == 'x') ADVANCE(501);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 456:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(910);
      if (lookahead == 'i') ADVANCE(525);
      if (lookahead == 'o') ADVANCE(546);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 457:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(793);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 458:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(537);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 459:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(535);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 460:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'c') ADVANCE(476);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 461:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(796);
      if (lookahead == 'h') ADVANCE(506);
      if (lookahead == 'l') ADVANCE(556);
      if (lookahead == 'm') ADVANCE(464);
      if (lookahead == 'o') ADVANCE(518);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 462:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(811);
      if (lookahead == 'o') ADVANCE(584);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 463:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(823);
      if (lookahead == 'e') ADVANCE(517);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 464:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(850);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 465:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(865);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 466:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(862);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 467:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(558);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 468:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'd') ADVANCE(530);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 469:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(808);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 470:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(814);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 471:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(832);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 472:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(907);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 473:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(835);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 474:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(655);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 475:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(916);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 476:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(847);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 477:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(513);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 478:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(521);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 479:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(547);
      if (lookahead == 'o') ADVANCE(510);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 480:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(565);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 481:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(511);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 482:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(548);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 483:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(566);
      if (lookahead == 'h') ADVANCE(582);
      if (lookahead == 'o') ADVANCE(551);
      if (lookahead == 't') ADVANCE(450);
      if (lookahead == 'u') ADVANCE(453);
      if (lookahead == 'y') ADVANCE(563);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 484:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(550);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 485:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(472);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 486:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'e') ADVANCE(553);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 487:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'f') ADVANCE(643);
      if (lookahead == 'p') ADVANCE(459);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 488:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'f') ADVANCE(298);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 489:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'f') ADVANCE(488);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 490:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'f') ADVANCE(496);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 491:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'f') ADVANCE(531);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 492:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'g') ADVANCE(883);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 493:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'g') ADVANCE(874);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 494:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'h') ADVANCE(817);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 495:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'h') ADVANCE(528);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 496:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(493);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 497:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(460);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 498:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(452);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 499:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(526);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 500:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(514);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 501:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(567);
      if (lookahead == 'p') ADVANCE(449);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 502:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(562);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 503:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'i') ADVANCE(586);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 504:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'k') ADVANCE(507);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 505:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'k') ADVANCE(844);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 506:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'k') ADVANCE(467);
      if (lookahead == 'o') ADVANCE(497);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 507:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'k') ADVANCE(500);
      if (lookahead == 'l') ADVANCE(502);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 508:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'k') ADVANCE(543);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 509:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(802);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 510:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(838);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 511:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(877);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 512:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(898);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 513:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(539);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 514:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(512);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 515:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'l') ADVANCE(475);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 516:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(600);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 517:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(603);
      if (lookahead == 'n') ADVANCE(826);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 518:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(538);
      if (lookahead == 'n') ADVANCE(585);
      if (lookahead == 'p') ADVANCE(588);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 519:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(471);
      if (lookahead == 't') ADVANCE(515);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 520:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(542);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 521:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'm') ADVANCE(499);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 522:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(492);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 523:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(490);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 524:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(886);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 525:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(465);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 526:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(491);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 527:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'n') ADVANCE(466);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 528:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(640);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 529:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(646);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 530:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(587);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 531:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(895);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 532:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(457);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 533:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(520);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 534:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(489);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 535:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(523);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 536:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(580);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 537:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'o') ADVANCE(540);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 538:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(853);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 539:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(871);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 540:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(589);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 541:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(473);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 542:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(572);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 543:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'p') ADVANCE(451);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 544:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'q') ADVANCE(583);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 545:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(805);
      if (lookahead == 's') ADVANCE(508);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 546:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(652);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 547:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(790);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 548:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(544);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 549:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(498);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 550:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(590);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 551:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(568);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 552:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(569);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 553:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(573);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 554:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'r') ADVANCE(574);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 555:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(559);
      if (lookahead == 't') ADVANCE(579);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 556:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(658);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 557:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(504);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 558:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(505);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 559:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(532);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 560:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(474);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 561:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(570);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 562:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(575);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 563:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 's') ADVANCE(581);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 564:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(609);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 565:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(880);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 566:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(612);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 567:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(649);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 568:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(889);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 569:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(829);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 570:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(892);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 571:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(868);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 572:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(820);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 573:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(856);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 574:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(913);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 575:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(901);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 576:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(494);
      if (lookahead == 'u') ADVANCE(560);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 577:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(469);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 578:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(468);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 579:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(549);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 580:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(529);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 581:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 't') ADVANCE(478);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 582:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'u') ADVANCE(578);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 583:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'u') ADVANCE(484);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 584:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'v') ADVANCE(470);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 585:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'v') ADVANCE(486);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 586:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'v') ADVANCE(482);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 587:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'w') ADVANCE(524);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 588:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'y') ADVANCE(799);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 589:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'y') ADVANCE(904);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 590:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == 'y') ADVANCE(859);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 591:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(591);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(593);
      END_STATE();
    case 592:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(592);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 593:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 594:
      ACCEPT_TOKEN(aux_sym_comment_token1);
      if (eof) ADVANCE(294);
      ADVANCE_MAP(
        '"', 1207,
        '%', 616,
        ':', 918,
        '@', 296,
        'A', 409,
        'C', 319,
        'D', 304,
        'E', 313,
        'F', 314,
        'G', 389,
        'H', 335,
        'I', 345,
        'L', 302,
        'M', 320,
        'N', 338,
        'P', 303,
        'R', 321,
        'S', 339,
        'T', 305,
        'V', 337,
        'X', 316,
        'a', 555,
        'c', 461,
        'd', 446,
        'e', 455,
        'f', 456,
        'g', 536,
        'h', 477,
        'i', 487,
        'l', 444,
        'm', 462,
        'n', 480,
        'p', 445,
        'r', 463,
        's', 483,
        't', 447,
        'v', 479,
        'x', 458,
      );
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(301);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(592);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(593);
      END_STATE();
    case 595:
      ACCEPT_TOKEN(anon_sym_REM);
      END_STATE();
    case 596:
      ACCEPT_TOKEN(anon_sym_REM);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 597:
      ACCEPT_TOKEN(anon_sym_REM);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 598:
      ACCEPT_TOKEN(anon_sym_Rem);
      END_STATE();
    case 599:
      ACCEPT_TOKEN(anon_sym_Rem);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 600:
      ACCEPT_TOKEN(anon_sym_Rem);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 601:
      ACCEPT_TOKEN(anon_sym_rem);
      END_STATE();
    case 602:
      ACCEPT_TOKEN(anon_sym_rem);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 603:
      ACCEPT_TOKEN(anon_sym_rem);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 604:
      ACCEPT_TOKEN(anon_sym_SET);
      END_STATE();
    case 605:
      ACCEPT_TOKEN(anon_sym_SET);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 606:
      ACCEPT_TOKEN(anon_sym_SET);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 607:
      ACCEPT_TOKEN(anon_sym_Set);
      END_STATE();
    case 608:
      ACCEPT_TOKEN(anon_sym_Set);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 609:
      ACCEPT_TOKEN(anon_sym_Set);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 610:
      ACCEPT_TOKEN(anon_sym_set);
      END_STATE();
    case 611:
      ACCEPT_TOKEN(anon_sym_set);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 612:
      ACCEPT_TOKEN(anon_sym_set);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 613:
      ACCEPT_TOKEN(anon_sym_SLASHA);
      END_STATE();
    case 614:
      ACCEPT_TOKEN(anon_sym_EQ);
      END_STATE();
    case 615:
      ACCEPT_TOKEN(anon_sym_PERCENT);
      END_STATE();
    case 616:
      ACCEPT_TOKEN(anon_sym_PERCENT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 617:
      ACCEPT_TOKEN(anon_sym_ECHO);
      END_STATE();
    case 618:
      ACCEPT_TOKEN(anon_sym_ECHO);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 619:
      ACCEPT_TOKEN(anon_sym_ECHO);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 620:
      ACCEPT_TOKEN(anon_sym_IF);
      END_STATE();
    case 621:
      ACCEPT_TOKEN(anon_sym_IF);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 622:
      ACCEPT_TOKEN(anon_sym_IF);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 623:
      ACCEPT_TOKEN(anon_sym_GOTO);
      END_STATE();
    case 624:
      ACCEPT_TOKEN(anon_sym_GOTO);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 625:
      ACCEPT_TOKEN(anon_sym_GOTO);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 626:
      ACCEPT_TOKEN(anon_sym_EXIT);
      END_STATE();
    case 627:
      ACCEPT_TOKEN(anon_sym_EXIT);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 628:
      ACCEPT_TOKEN(anon_sym_EXIT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 629:
      ACCEPT_TOKEN(anon_sym_FOR);
      if (lookahead == 'M') ADVANCE(923);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 630:
      ACCEPT_TOKEN(anon_sym_FOR);
      if (lookahead == 'M') ADVANCE(306);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 631:
      ACCEPT_TOKEN(anon_sym_FOR);
      if (lookahead == 'M') ADVANCE(8);
      END_STATE();
    case 632:
      ACCEPT_TOKEN(anon_sym_PAUSE);
      END_STATE();
    case 633:
      ACCEPT_TOKEN(anon_sym_PAUSE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 634:
      ACCEPT_TOKEN(anon_sym_PAUSE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 635:
      ACCEPT_TOKEN(anon_sym_CLS);
      END_STATE();
    case 636:
      ACCEPT_TOKEN(anon_sym_CLS);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 637:
      ACCEPT_TOKEN(anon_sym_CLS);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 638:
      ACCEPT_TOKEN(anon_sym_echo);
      if (lookahead == ' ') ADVANCE(230);
      END_STATE();
    case 639:
      ACCEPT_TOKEN(anon_sym_echo);
      if (lookahead == ' ') ADVANCE(230);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 640:
      ACCEPT_TOKEN(anon_sym_echo);
      if (lookahead == ' ') ADVANCE(534);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 641:
      ACCEPT_TOKEN(anon_sym_if);
      END_STATE();
    case 642:
      ACCEPT_TOKEN(anon_sym_if);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 643:
      ACCEPT_TOKEN(anon_sym_if);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 644:
      ACCEPT_TOKEN(anon_sym_goto);
      END_STATE();
    case 645:
      ACCEPT_TOKEN(anon_sym_goto);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 646:
      ACCEPT_TOKEN(anon_sym_goto);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 647:
      ACCEPT_TOKEN(anon_sym_exit);
      END_STATE();
    case 648:
      ACCEPT_TOKEN(anon_sym_exit);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 649:
      ACCEPT_TOKEN(anon_sym_exit);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 650:
      ACCEPT_TOKEN(anon_sym_for);
      if (lookahead == 'm') ADVANCE(1065);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 651:
      ACCEPT_TOKEN(anon_sym_for);
      if (lookahead == 'm') ADVANCE(150);
      END_STATE();
    case 652:
      ACCEPT_TOKEN(anon_sym_for);
      if (lookahead == 'm') ADVANCE(448);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 653:
      ACCEPT_TOKEN(anon_sym_pause);
      END_STATE();
    case 654:
      ACCEPT_TOKEN(anon_sym_pause);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 655:
      ACCEPT_TOKEN(anon_sym_pause);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 656:
      ACCEPT_TOKEN(anon_sym_cls);
      END_STATE();
    case 657:
      ACCEPT_TOKEN(anon_sym_cls);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 658:
      ACCEPT_TOKEN(anon_sym_cls);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 659:
      ACCEPT_TOKEN(anon_sym_VER);
      END_STATE();
    case 660:
      ACCEPT_TOKEN(anon_sym_VER);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 661:
      ACCEPT_TOKEN(anon_sym_VER);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 662:
      ACCEPT_TOKEN(anon_sym_ASSOC);
      END_STATE();
    case 663:
      ACCEPT_TOKEN(anon_sym_ASSOC);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 664:
      ACCEPT_TOKEN(anon_sym_ASSOC);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 665:
      ACCEPT_TOKEN(anon_sym_CD);
      END_STATE();
    case 666:
      ACCEPT_TOKEN(anon_sym_CD);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 667:
      ACCEPT_TOKEN(anon_sym_CD);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 668:
      ACCEPT_TOKEN(anon_sym_COPY);
      END_STATE();
    case 669:
      ACCEPT_TOKEN(anon_sym_COPY);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 670:
      ACCEPT_TOKEN(anon_sym_COPY);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 671:
      ACCEPT_TOKEN(anon_sym_DEL);
      END_STATE();
    case 672:
      ACCEPT_TOKEN(anon_sym_DEL);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 673:
      ACCEPT_TOKEN(anon_sym_DEL);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 674:
      ACCEPT_TOKEN(anon_sym_DIR);
      END_STATE();
    case 675:
      ACCEPT_TOKEN(anon_sym_DIR);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 676:
      ACCEPT_TOKEN(anon_sym_DIR);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 677:
      ACCEPT_TOKEN(anon_sym_DATE);
      END_STATE();
    case 678:
      ACCEPT_TOKEN(anon_sym_DATE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 679:
      ACCEPT_TOKEN(anon_sym_DATE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 680:
      ACCEPT_TOKEN(anon_sym_MD);
      END_STATE();
    case 681:
      ACCEPT_TOKEN(anon_sym_MD);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 682:
      ACCEPT_TOKEN(anon_sym_MD);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 683:
      ACCEPT_TOKEN(anon_sym_MOVE);
      END_STATE();
    case 684:
      ACCEPT_TOKEN(anon_sym_MOVE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 685:
      ACCEPT_TOKEN(anon_sym_MOVE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 686:
      ACCEPT_TOKEN(anon_sym_PATH);
      END_STATE();
    case 687:
      ACCEPT_TOKEN(anon_sym_PATH);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 688:
      ACCEPT_TOKEN(anon_sym_PATH);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 689:
      ACCEPT_TOKEN(anon_sym_PROMPT);
      END_STATE();
    case 690:
      ACCEPT_TOKEN(anon_sym_PROMPT);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 691:
      ACCEPT_TOKEN(anon_sym_PROMPT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 692:
      ACCEPT_TOKEN(anon_sym_RD);
      END_STATE();
    case 693:
      ACCEPT_TOKEN(anon_sym_RD);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 694:
      ACCEPT_TOKEN(anon_sym_RD);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 695:
      ACCEPT_TOKEN(anon_sym_REN);
      END_STATE();
    case 696:
      ACCEPT_TOKEN(anon_sym_REN);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 697:
      ACCEPT_TOKEN(anon_sym_REN);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 698:
      ACCEPT_TOKEN(anon_sym_START);
      END_STATE();
    case 699:
      ACCEPT_TOKEN(anon_sym_START);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 700:
      ACCEPT_TOKEN(anon_sym_START);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 701:
      ACCEPT_TOKEN(anon_sym_TIME);
      END_STATE();
    case 702:
      ACCEPT_TOKEN(anon_sym_TIME);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 703:
      ACCEPT_TOKEN(anon_sym_TIME);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 704:
      ACCEPT_TOKEN(anon_sym_TYPE);
      END_STATE();
    case 705:
      ACCEPT_TOKEN(anon_sym_TYPE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 706:
      ACCEPT_TOKEN(anon_sym_TYPE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 707:
      ACCEPT_TOKEN(anon_sym_VOL);
      END_STATE();
    case 708:
      ACCEPT_TOKEN(anon_sym_VOL);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 709:
      ACCEPT_TOKEN(anon_sym_VOL);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 710:
      ACCEPT_TOKEN(anon_sym_ATTRIB);
      END_STATE();
    case 711:
      ACCEPT_TOKEN(anon_sym_ATTRIB);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 712:
      ACCEPT_TOKEN(anon_sym_ATTRIB);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 713:
      ACCEPT_TOKEN(anon_sym_CHKDSK);
      END_STATE();
    case 714:
      ACCEPT_TOKEN(anon_sym_CHKDSK);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 715:
      ACCEPT_TOKEN(anon_sym_CHKDSK);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 716:
      ACCEPT_TOKEN(anon_sym_CHOICE);
      END_STATE();
    case 717:
      ACCEPT_TOKEN(anon_sym_CHOICE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 718:
      ACCEPT_TOKEN(anon_sym_CHOICE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 719:
      ACCEPT_TOKEN(anon_sym_CMD);
      END_STATE();
    case 720:
      ACCEPT_TOKEN(anon_sym_CMD);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 721:
      ACCEPT_TOKEN(anon_sym_CMD);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 722:
      ACCEPT_TOKEN(anon_sym_COMP);
      END_STATE();
    case 723:
      ACCEPT_TOKEN(anon_sym_COMP);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 724:
      ACCEPT_TOKEN(anon_sym_COMP);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 725:
      ACCEPT_TOKEN(anon_sym_CONVERT);
      END_STATE();
    case 726:
      ACCEPT_TOKEN(anon_sym_CONVERT);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 727:
      ACCEPT_TOKEN(anon_sym_CONVERT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 728:
      ACCEPT_TOKEN(anon_sym_DRIVERQUERY);
      END_STATE();
    case 729:
      ACCEPT_TOKEN(anon_sym_DRIVERQUERY);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 730:
      ACCEPT_TOKEN(anon_sym_DRIVERQUERY);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 731:
      ACCEPT_TOKEN(anon_sym_EXPAND);
      END_STATE();
    case 732:
      ACCEPT_TOKEN(anon_sym_EXPAND);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 733:
      ACCEPT_TOKEN(anon_sym_EXPAND);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 734:
      ACCEPT_TOKEN(anon_sym_FIND);
      END_STATE();
    case 735:
      ACCEPT_TOKEN(anon_sym_FIND);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 736:
      ACCEPT_TOKEN(anon_sym_FIND);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 737:
      ACCEPT_TOKEN(anon_sym_FORMAT);
      END_STATE();
    case 738:
      ACCEPT_TOKEN(anon_sym_FORMAT);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 739:
      ACCEPT_TOKEN(anon_sym_FORMAT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 740:
      ACCEPT_TOKEN(anon_sym_HELP);
      END_STATE();
    case 741:
      ACCEPT_TOKEN(anon_sym_HELP);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 742:
      ACCEPT_TOKEN(anon_sym_HELP);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 743:
      ACCEPT_TOKEN(anon_sym_IPCONFIG);
      END_STATE();
    case 744:
      ACCEPT_TOKEN(anon_sym_IPCONFIG);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 745:
      ACCEPT_TOKEN(anon_sym_IPCONFIG);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 746:
      ACCEPT_TOKEN(anon_sym_LABEL);
      END_STATE();
    case 747:
      ACCEPT_TOKEN(anon_sym_LABEL);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 748:
      ACCEPT_TOKEN(anon_sym_LABEL);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 749:
      ACCEPT_TOKEN(anon_sym_NET);
      END_STATE();
    case 750:
      ACCEPT_TOKEN(anon_sym_NET);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 751:
      ACCEPT_TOKEN(anon_sym_NET);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 752:
      ACCEPT_TOKEN(anon_sym_PING);
      END_STATE();
    case 753:
      ACCEPT_TOKEN(anon_sym_PING);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 754:
      ACCEPT_TOKEN(anon_sym_PING);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 755:
      ACCEPT_TOKEN(anon_sym_SHUTDOWN);
      END_STATE();
    case 756:
      ACCEPT_TOKEN(anon_sym_SHUTDOWN);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 757:
      ACCEPT_TOKEN(anon_sym_SHUTDOWN);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 758:
      ACCEPT_TOKEN(anon_sym_SORT);
      END_STATE();
    case 759:
      ACCEPT_TOKEN(anon_sym_SORT);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 760:
      ACCEPT_TOKEN(anon_sym_SORT);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 761:
      ACCEPT_TOKEN(anon_sym_SUBST);
      END_STATE();
    case 762:
      ACCEPT_TOKEN(anon_sym_SUBST);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 763:
      ACCEPT_TOKEN(anon_sym_SUBST);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 764:
      ACCEPT_TOKEN(anon_sym_SYSTEMINFO);
      END_STATE();
    case 765:
      ACCEPT_TOKEN(anon_sym_SYSTEMINFO);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 766:
      ACCEPT_TOKEN(anon_sym_SYSTEMINFO);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 767:
      ACCEPT_TOKEN(anon_sym_TASKKILL);
      END_STATE();
    case 768:
      ACCEPT_TOKEN(anon_sym_TASKKILL);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 769:
      ACCEPT_TOKEN(anon_sym_TASKKILL);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 770:
      ACCEPT_TOKEN(anon_sym_TASKLIST);
      END_STATE();
    case 771:
      ACCEPT_TOKEN(anon_sym_TASKLIST);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 772:
      ACCEPT_TOKEN(anon_sym_TASKLIST);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 773:
      ACCEPT_TOKEN(anon_sym_XCOPY);
      END_STATE();
    case 774:
      ACCEPT_TOKEN(anon_sym_XCOPY);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 775:
      ACCEPT_TOKEN(anon_sym_XCOPY);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 776:
      ACCEPT_TOKEN(anon_sym_TREE);
      END_STATE();
    case 777:
      ACCEPT_TOKEN(anon_sym_TREE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 778:
      ACCEPT_TOKEN(anon_sym_TREE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 779:
      ACCEPT_TOKEN(anon_sym_FC);
      END_STATE();
    case 780:
      ACCEPT_TOKEN(anon_sym_FC);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 781:
      ACCEPT_TOKEN(anon_sym_FC);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 782:
      ACCEPT_TOKEN(anon_sym_DISKPART);
      END_STATE();
    case 783:
      ACCEPT_TOKEN(anon_sym_DISKPART);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 784:
      ACCEPT_TOKEN(anon_sym_DISKPART);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 785:
      ACCEPT_TOKEN(anon_sym_TITLE);
      END_STATE();
    case 786:
      ACCEPT_TOKEN(anon_sym_TITLE);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 787:
      ACCEPT_TOKEN(anon_sym_TITLE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 788:
      ACCEPT_TOKEN(anon_sym_ver);
      END_STATE();
    case 789:
      ACCEPT_TOKEN(anon_sym_ver);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 790:
      ACCEPT_TOKEN(anon_sym_ver);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 791:
      ACCEPT_TOKEN(anon_sym_assoc);
      END_STATE();
    case 792:
      ACCEPT_TOKEN(anon_sym_assoc);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 793:
      ACCEPT_TOKEN(anon_sym_assoc);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 794:
      ACCEPT_TOKEN(anon_sym_cd);
      END_STATE();
    case 795:
      ACCEPT_TOKEN(anon_sym_cd);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 796:
      ACCEPT_TOKEN(anon_sym_cd);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 797:
      ACCEPT_TOKEN(anon_sym_copy);
      END_STATE();
    case 798:
      ACCEPT_TOKEN(anon_sym_copy);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 799:
      ACCEPT_TOKEN(anon_sym_copy);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 800:
      ACCEPT_TOKEN(anon_sym_del);
      END_STATE();
    case 801:
      ACCEPT_TOKEN(anon_sym_del);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 802:
      ACCEPT_TOKEN(anon_sym_del);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 803:
      ACCEPT_TOKEN(anon_sym_dir);
      END_STATE();
    case 804:
      ACCEPT_TOKEN(anon_sym_dir);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 805:
      ACCEPT_TOKEN(anon_sym_dir);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 806:
      ACCEPT_TOKEN(anon_sym_date);
      END_STATE();
    case 807:
      ACCEPT_TOKEN(anon_sym_date);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 808:
      ACCEPT_TOKEN(anon_sym_date);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 809:
      ACCEPT_TOKEN(anon_sym_md);
      END_STATE();
    case 810:
      ACCEPT_TOKEN(anon_sym_md);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 811:
      ACCEPT_TOKEN(anon_sym_md);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 812:
      ACCEPT_TOKEN(anon_sym_move);
      END_STATE();
    case 813:
      ACCEPT_TOKEN(anon_sym_move);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 814:
      ACCEPT_TOKEN(anon_sym_move);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 815:
      ACCEPT_TOKEN(anon_sym_path);
      END_STATE();
    case 816:
      ACCEPT_TOKEN(anon_sym_path);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 817:
      ACCEPT_TOKEN(anon_sym_path);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 818:
      ACCEPT_TOKEN(anon_sym_prompt);
      END_STATE();
    case 819:
      ACCEPT_TOKEN(anon_sym_prompt);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 820:
      ACCEPT_TOKEN(anon_sym_prompt);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 821:
      ACCEPT_TOKEN(anon_sym_rd);
      END_STATE();
    case 822:
      ACCEPT_TOKEN(anon_sym_rd);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 823:
      ACCEPT_TOKEN(anon_sym_rd);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 824:
      ACCEPT_TOKEN(anon_sym_ren);
      END_STATE();
    case 825:
      ACCEPT_TOKEN(anon_sym_ren);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 826:
      ACCEPT_TOKEN(anon_sym_ren);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 827:
      ACCEPT_TOKEN(anon_sym_start);
      END_STATE();
    case 828:
      ACCEPT_TOKEN(anon_sym_start);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 829:
      ACCEPT_TOKEN(anon_sym_start);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 830:
      ACCEPT_TOKEN(anon_sym_time);
      END_STATE();
    case 831:
      ACCEPT_TOKEN(anon_sym_time);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 832:
      ACCEPT_TOKEN(anon_sym_time);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 833:
      ACCEPT_TOKEN(anon_sym_type);
      END_STATE();
    case 834:
      ACCEPT_TOKEN(anon_sym_type);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 835:
      ACCEPT_TOKEN(anon_sym_type);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 836:
      ACCEPT_TOKEN(anon_sym_vol);
      END_STATE();
    case 837:
      ACCEPT_TOKEN(anon_sym_vol);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 838:
      ACCEPT_TOKEN(anon_sym_vol);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 839:
      ACCEPT_TOKEN(anon_sym_attrib);
      END_STATE();
    case 840:
      ACCEPT_TOKEN(anon_sym_attrib);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 841:
      ACCEPT_TOKEN(anon_sym_attrib);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 842:
      ACCEPT_TOKEN(anon_sym_chkdsk);
      END_STATE();
    case 843:
      ACCEPT_TOKEN(anon_sym_chkdsk);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 844:
      ACCEPT_TOKEN(anon_sym_chkdsk);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 845:
      ACCEPT_TOKEN(anon_sym_choice);
      END_STATE();
    case 846:
      ACCEPT_TOKEN(anon_sym_choice);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 847:
      ACCEPT_TOKEN(anon_sym_choice);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 848:
      ACCEPT_TOKEN(anon_sym_cmd);
      END_STATE();
    case 849:
      ACCEPT_TOKEN(anon_sym_cmd);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 850:
      ACCEPT_TOKEN(anon_sym_cmd);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 851:
      ACCEPT_TOKEN(anon_sym_comp);
      END_STATE();
    case 852:
      ACCEPT_TOKEN(anon_sym_comp);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 853:
      ACCEPT_TOKEN(anon_sym_comp);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 854:
      ACCEPT_TOKEN(anon_sym_convert);
      END_STATE();
    case 855:
      ACCEPT_TOKEN(anon_sym_convert);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 856:
      ACCEPT_TOKEN(anon_sym_convert);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 857:
      ACCEPT_TOKEN(anon_sym_driverquery);
      END_STATE();
    case 858:
      ACCEPT_TOKEN(anon_sym_driverquery);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 859:
      ACCEPT_TOKEN(anon_sym_driverquery);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 860:
      ACCEPT_TOKEN(anon_sym_expand);
      END_STATE();
    case 861:
      ACCEPT_TOKEN(anon_sym_expand);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 862:
      ACCEPT_TOKEN(anon_sym_expand);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 863:
      ACCEPT_TOKEN(anon_sym_find);
      END_STATE();
    case 864:
      ACCEPT_TOKEN(anon_sym_find);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 865:
      ACCEPT_TOKEN(anon_sym_find);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 866:
      ACCEPT_TOKEN(anon_sym_format);
      END_STATE();
    case 867:
      ACCEPT_TOKEN(anon_sym_format);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 868:
      ACCEPT_TOKEN(anon_sym_format);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 869:
      ACCEPT_TOKEN(anon_sym_help);
      END_STATE();
    case 870:
      ACCEPT_TOKEN(anon_sym_help);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 871:
      ACCEPT_TOKEN(anon_sym_help);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 872:
      ACCEPT_TOKEN(anon_sym_ipconfig);
      END_STATE();
    case 873:
      ACCEPT_TOKEN(anon_sym_ipconfig);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 874:
      ACCEPT_TOKEN(anon_sym_ipconfig);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 875:
      ACCEPT_TOKEN(anon_sym_label);
      END_STATE();
    case 876:
      ACCEPT_TOKEN(anon_sym_label);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 877:
      ACCEPT_TOKEN(anon_sym_label);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 878:
      ACCEPT_TOKEN(anon_sym_net);
      END_STATE();
    case 879:
      ACCEPT_TOKEN(anon_sym_net);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 880:
      ACCEPT_TOKEN(anon_sym_net);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 881:
      ACCEPT_TOKEN(anon_sym_ping);
      END_STATE();
    case 882:
      ACCEPT_TOKEN(anon_sym_ping);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 883:
      ACCEPT_TOKEN(anon_sym_ping);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 884:
      ACCEPT_TOKEN(anon_sym_shutdown);
      END_STATE();
    case 885:
      ACCEPT_TOKEN(anon_sym_shutdown);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 886:
      ACCEPT_TOKEN(anon_sym_shutdown);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 887:
      ACCEPT_TOKEN(anon_sym_sort);
      END_STATE();
    case 888:
      ACCEPT_TOKEN(anon_sym_sort);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 889:
      ACCEPT_TOKEN(anon_sym_sort);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 890:
      ACCEPT_TOKEN(anon_sym_subst);
      END_STATE();
    case 891:
      ACCEPT_TOKEN(anon_sym_subst);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 892:
      ACCEPT_TOKEN(anon_sym_subst);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 893:
      ACCEPT_TOKEN(anon_sym_systeminfo);
      END_STATE();
    case 894:
      ACCEPT_TOKEN(anon_sym_systeminfo);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 895:
      ACCEPT_TOKEN(anon_sym_systeminfo);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 896:
      ACCEPT_TOKEN(anon_sym_taskkill);
      END_STATE();
    case 897:
      ACCEPT_TOKEN(anon_sym_taskkill);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 898:
      ACCEPT_TOKEN(anon_sym_taskkill);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 899:
      ACCEPT_TOKEN(anon_sym_tasklist);
      END_STATE();
    case 900:
      ACCEPT_TOKEN(anon_sym_tasklist);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 901:
      ACCEPT_TOKEN(anon_sym_tasklist);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 902:
      ACCEPT_TOKEN(anon_sym_xcopy);
      END_STATE();
    case 903:
      ACCEPT_TOKEN(anon_sym_xcopy);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 904:
      ACCEPT_TOKEN(anon_sym_xcopy);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 905:
      ACCEPT_TOKEN(anon_sym_tree);
      END_STATE();
    case 906:
      ACCEPT_TOKEN(anon_sym_tree);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 907:
      ACCEPT_TOKEN(anon_sym_tree);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 908:
      ACCEPT_TOKEN(anon_sym_fc);
      END_STATE();
    case 909:
      ACCEPT_TOKEN(anon_sym_fc);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 910:
      ACCEPT_TOKEN(anon_sym_fc);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 911:
      ACCEPT_TOKEN(anon_sym_diskpart);
      END_STATE();
    case 912:
      ACCEPT_TOKEN(anon_sym_diskpart);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 913:
      ACCEPT_TOKEN(anon_sym_diskpart);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 914:
      ACCEPT_TOKEN(anon_sym_title);
      END_STATE();
    case 915:
      ACCEPT_TOKEN(anon_sym_title);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 916:
      ACCEPT_TOKEN(anon_sym_title);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 917:
      ACCEPT_TOKEN(anon_sym_COLON);
      if (lookahead == ':') ADVANCE(299);
      END_STATE();
    case 918:
      ACCEPT_TOKEN(anon_sym_COLON);
      if (lookahead == ':') ADVANCE(300);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 919:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(929);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 920:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1046);
      if (lookahead == 'I') ADVANCE(994);
      if (lookahead == 'R') ADVANCE(1000);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 921:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1047);
      if (lookahead == 'E') ADVANCE(982);
      if (lookahead == 'I') ADVANCE(1016);
      if (lookahead == 'R') ADVANCE(976);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 922:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1028);
      if (lookahead == 'I') ADVANCE(991);
      if (lookahead == 'R') ADVANCE(960);
      if (lookahead == 'Y') ADVANCE(1012);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 923:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1041);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 924:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(999);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 925:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1023);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 926:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'A') ADVANCE(1025);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('B' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 927:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'B') ADVANCE(711);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 928:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'B') ADVANCE(1031);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 929:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'B') ADVANCE(957);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 930:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(968);
      if (lookahead == 'X') ADVANCE(973);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 931:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(780);
      if (lookahead == 'I') ADVANCE(998);
      if (lookahead == 'O') ADVANCE(1017);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 932:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(663);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 933:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(1008);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 934:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(1007);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 935:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'C') ADVANCE(951);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 936:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(666);
      if (lookahead == 'H') ADVANCE(979);
      if (lookahead == 'L') ADVANCE(1027);
      if (lookahead == 'M') ADVANCE(939);
      if (lookahead == 'O') ADVANCE(990);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 937:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(681);
      if (lookahead == 'O') ADVANCE(1054);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 938:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(693);
      if (lookahead == 'E') ADVANCE(989);
      if (lookahead == 'e') ADVANCE(1131);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 939:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(720);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 940:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(735);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 941:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(732);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 942:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(1029);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 943:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'D') ADVANCE(1003);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 944:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(678);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 945:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(684);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 946:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(702);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 947:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(777);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 948:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(705);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 949:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(633);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 950:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(786);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 951:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(717);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 952:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(986);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 953:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(993);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 954:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1018);
      if (lookahead == 'O') ADVANCE(983);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 955:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1035);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 956:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1036);
      if (lookahead == 'H') ADVANCE(1052);
      if (lookahead == 'O') ADVANCE(1022);
      if (lookahead == 'T') ADVANCE(925);
      if (lookahead == 'U') ADVANCE(928);
      if (lookahead == 'Y') ADVANCE(1034);
      if (lookahead == 'e') ADVANCE(1178);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 957:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(984);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 958:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1019);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 959:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1021);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 960:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(947);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 961:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'E') ADVANCE(1024);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 962:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'F') ADVANCE(621);
      if (lookahead == 'P') ADVANCE(934);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 963:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'F') ADVANCE(969);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 964:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'F') ADVANCE(1004);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 965:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'G') ADVANCE(753);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 966:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'G') ADVANCE(744);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 967:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'H') ADVANCE(687);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 968:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'H') ADVANCE(1001);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 969:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(966);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 970:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(935);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 971:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(927);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 972:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(997);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 973:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(1037);
      if (lookahead == 'P') ADVANCE(924);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 974:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(987);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 975:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(1033);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 976:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'I') ADVANCE(1056);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 977:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'K') ADVANCE(980);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 978:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'K') ADVANCE(714);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 979:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'K') ADVANCE(942);
      if (lookahead == 'O') ADVANCE(970);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 980:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'K') ADVANCE(974);
      if (lookahead == 'L') ADVANCE(975);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 981:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'K') ADVANCE(1014);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 982:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(672);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 983:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(708);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 984:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(747);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 985:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(768);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 986:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(1010);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 987:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(985);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 988:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'L') ADVANCE(950);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 989:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'M') ADVANCE(596);
      if (lookahead == 'N') ADVANCE(696);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 990:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'M') ADVANCE(1009);
      if (lookahead == 'N') ADVANCE(1055);
      if (lookahead == 'P') ADVANCE(1058);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 991:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'M') ADVANCE(946);
      if (lookahead == 'T') ADVANCE(988);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 992:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'M') ADVANCE(1013);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 993:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'M') ADVANCE(972);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 994:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(965);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 995:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(963);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 996:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(756);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 997:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(964);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 998:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(940);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 999:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'N') ADVANCE(941);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1000:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(992);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1001:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(618);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1002:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(624);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1003:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(1057);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1004:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(765);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1005:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(932);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1006:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(1050);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1007:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(995);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1008:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'O') ADVANCE(1011);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1009:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(723);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1010:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(741);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1011:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(1059);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1012:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(948);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1013:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(1042);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1014:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'P') ADVANCE(926);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1015:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'Q') ADVANCE(1053);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1016:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(675);
      if (lookahead == 'S') ADVANCE(981);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1017:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(629);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1018:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(660);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1019:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1015);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1020:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(971);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1021:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1060);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1022:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1038);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1023:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1039);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1024:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1043);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1025:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'R') ADVANCE(1044);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1026:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(1030);
      if (lookahead == 'T') ADVANCE(1049);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1027:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(636);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1028:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(977);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1029:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(978);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1030:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(1005);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1031:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(1040);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1032:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(949);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1033:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(1045);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1034:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'S') ADVANCE(1051);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1035:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(750);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1036:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(605);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1037:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(627);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1038:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(759);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1039:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(699);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1040:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(762);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1041:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(738);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1042:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(690);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1043:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(726);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1044:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(783);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1045:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(771);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1046:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(967);
      if (lookahead == 'U') ADVANCE(1032);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1047:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(944);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1048:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(943);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1049:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(1020);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1050:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(1002);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1051:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(953);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1052:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'U') ADVANCE(1048);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1053:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'U') ADVANCE(959);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1054:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'V') ADVANCE(945);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1055:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'V') ADVANCE(961);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1056:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'V') ADVANCE(958);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1057:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'W') ADVANCE(996);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1058:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'Y') ADVANCE(669);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1059:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'Y') ADVANCE(774);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1060:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'Y') ADVANCE(729);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1061:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1071);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1062:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1190);
      if (lookahead == 'i') ADVANCE(1137);
      if (lookahead == 'r') ADVANCE(1148);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1063:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1191);
      if (lookahead == 'e') ADVANCE(1124);
      if (lookahead == 'i') ADVANCE(1159);
      if (lookahead == 'r') ADVANCE(1118);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1064:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1171);
      if (lookahead == 'i') ADVANCE(1134);
      if (lookahead == 'r') ADVANCE(1102);
      if (lookahead == 'y') ADVANCE(1155);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1065:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1185);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1066:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1142);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1067:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1166);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1068:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(1168);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1069:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(840);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1070:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(1175);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1071:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(1098);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1072:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(1110);
      if (lookahead == 'x') ADVANCE(1116);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1073:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(909);
      if (lookahead == 'i') ADVANCE(1141);
      if (lookahead == 'o') ADVANCE(1160);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1074:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(792);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1075:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(1151);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1076:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(1149);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1077:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(1093);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1078:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(795);
      if (lookahead == 'h') ADVANCE(1121);
      if (lookahead == 'l') ADVANCE(1170);
      if (lookahead == 'm') ADVANCE(1081);
      if (lookahead == 'o') ADVANCE(1133);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1079:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(810);
      if (lookahead == 'o') ADVANCE(1198);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1080:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(822);
      if (lookahead == 'e') ADVANCE(1132);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1081:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(849);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1082:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(864);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1083:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(861);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1084:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(1172);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1085:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(1145);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1086:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(807);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1087:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(813);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1088:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(831);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1089:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(906);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1090:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(834);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1091:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(654);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1092:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(915);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1093:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(846);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1094:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1128);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1095:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1136);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1096:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1161);
      if (lookahead == 'o') ADVANCE(1125);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1097:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1179);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1098:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1126);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1099:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1162);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1100:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1180);
      if (lookahead == 'h') ADVANCE(1196);
      if (lookahead == 'o') ADVANCE(1165);
      if (lookahead == 't') ADVANCE(1067);
      if (lookahead == 'u') ADVANCE(1070);
      if (lookahead == 'y') ADVANCE(1177);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1101:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1164);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1102:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1089);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1103:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(1167);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1104:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(642);
      if (lookahead == 'p') ADVANCE(1076);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1105:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(1111);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1106:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(1146);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1107:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(882);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1108:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(873);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1109:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(816);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1110:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(1143);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1111:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1108);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1112:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1077);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1113:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1069);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1114:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1140);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1115:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1129);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1116:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1181);
      if (lookahead == 'p') ADVANCE(1066);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1117:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1176);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1118:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(1200);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1119:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(1122);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1120:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(843);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1121:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(1084);
      if (lookahead == 'o') ADVANCE(1112);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1122:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(1115);
      if (lookahead == 'l') ADVANCE(1117);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1123:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(1157);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1124:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(801);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1125:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(837);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1126:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(876);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1127:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(897);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1128:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(1153);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1129:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(1127);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1130:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(1092);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1131:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(599);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1132:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(602);
      if (lookahead == 'n') ADVANCE(825);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1133:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(1152);
      if (lookahead == 'n') ADVANCE(1199);
      if (lookahead == 'p') ADVANCE(1202);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1134:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(1088);
      if (lookahead == 't') ADVANCE(1130);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1135:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(1156);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1136:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(1114);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1137:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(1107);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1138:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(1105);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1139:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(885);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1140:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(1106);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1141:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(1082);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1142:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(1083);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1143:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(639);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1144:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(645);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1145:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1201);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1146:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(894);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1147:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1074);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1148:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1135);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1149:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1138);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1150:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1194);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1151:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(1154);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1152:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(852);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1153:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(870);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1154:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(1203);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1155:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(1090);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1156:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(1186);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1157:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(1068);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1158:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'q') ADVANCE(1197);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1159:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(804);
      if (lookahead == 's') ADVANCE(1123);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1160:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(650);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1161:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(789);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1162:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1158);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1163:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1113);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1164:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1204);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1165:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1182);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1166:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1183);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1167:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1187);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1168:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(1188);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1169:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1173);
      if (lookahead == 't') ADVANCE(1193);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1170:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(657);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1171:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1119);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1172:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1120);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1173:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1147);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1174:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1091);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1175:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1184);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1176:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1189);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1177:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(1195);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1178:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(608);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1179:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(879);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1180:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(611);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1181:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(648);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1182:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(888);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1183:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(828);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1184:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(891);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1185:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(867);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1186:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(819);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1187:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(855);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1188:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(912);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1189:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(900);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1190:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1109);
      if (lookahead == 'u') ADVANCE(1174);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1191:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1086);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1192:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1085);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1193:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1163);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1194:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1144);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1195:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(1095);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1196:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(1192);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1197:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(1101);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1198:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(1087);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1199:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(1103);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1200:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(1099);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1201:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'w') ADVANCE(1139);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1202:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'y') ADVANCE(798);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1203:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'y') ADVANCE(903);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1204:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'y') ADVANCE(858);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1205:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(1205);
      END_STATE();
    case 1206:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 1207:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(593);
      END_STATE();
    case 1208:
      ACCEPT_TOKEN(aux_sym_string_token1);
      END_STATE();
    case 1209:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(1209);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead) &&
          lookahead != '"') ADVANCE(1208);
      END_STATE();
    case 1210:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(1210);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 293},
  [2] = {.lex_state = 293},
  [3] = {.lex_state = 293},
  [4] = {.lex_state = 0},
  [5] = {.lex_state = 0},
  [6] = {.lex_state = 594},
  [7] = {.lex_state = 594},
  [8] = {.lex_state = 293},
  [9] = {.lex_state = 293},
  [10] = {.lex_state = 293},
  [11] = {.lex_state = 293},
  [12] = {.lex_state = 293},
  [13] = {.lex_state = 293},
  [14] = {.lex_state = 293},
  [15] = {.lex_state = 293},
  [16] = {.lex_state = 293},
  [17] = {.lex_state = 293},
  [18] = {.lex_state = 293},
  [19] = {.lex_state = 293},
  [20] = {.lex_state = 293},
  [21] = {.lex_state = 293},
  [22] = {.lex_state = 293},
  [23] = {.lex_state = 293},
  [24] = {.lex_state = 293},
  [25] = {.lex_state = 0},
  [26] = {.lex_state = 0},
  [27] = {.lex_state = 0},
  [28] = {.lex_state = 1},
  [29] = {.lex_state = 1},
  [30] = {.lex_state = 1},
  [31] = {.lex_state = 2},
  [32] = {.lex_state = 2},
  [33] = {.lex_state = 2},
  [34] = {.lex_state = 0},
  [35] = {.lex_state = 0},
  [36] = {.lex_state = 2},
  [37] = {.lex_state = 0},
  [38] = {.lex_state = 0},
  [39] = {.lex_state = 591},
  [40] = {.lex_state = 591},
  [41] = {.lex_state = 2},
  [42] = {.lex_state = 0},
  [43] = {.lex_state = 2},
  [44] = {.lex_state = 2},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_AT] = ACTIONS(1),
    [anon_sym_echooff] = ACTIONS(1),
    [anon_sym_COLON_COLON] = ACTIONS(1),
    [anon_sym_REM] = ACTIONS(1),
    [anon_sym_Rem] = ACTIONS(1),
    [anon_sym_rem] = ACTIONS(1),
    [anon_sym_SET] = ACTIONS(1),
    [anon_sym_Set] = ACTIONS(1),
    [anon_sym_set] = ACTIONS(1),
    [anon_sym_SLASHA] = ACTIONS(1),
    [anon_sym_EQ] = ACTIONS(1),
    [anon_sym_PERCENT] = ACTIONS(1),
    [anon_sym_ECHO] = ACTIONS(1),
    [anon_sym_IF] = ACTIONS(1),
    [anon_sym_GOTO] = ACTIONS(1),
    [anon_sym_EXIT] = ACTIONS(1),
    [anon_sym_FOR] = ACTIONS(1),
    [anon_sym_PAUSE] = ACTIONS(1),
    [anon_sym_CLS] = ACTIONS(1),
    [anon_sym_echo] = ACTIONS(1),
    [anon_sym_if] = ACTIONS(1),
    [anon_sym_goto] = ACTIONS(1),
    [anon_sym_exit] = ACTIONS(1),
    [anon_sym_for] = ACTIONS(1),
    [anon_sym_pause] = ACTIONS(1),
    [anon_sym_cls] = ACTIONS(1),
    [anon_sym_VER] = ACTIONS(1),
    [anon_sym_ASSOC] = ACTIONS(1),
    [anon_sym_CD] = ACTIONS(1),
    [anon_sym_COPY] = ACTIONS(1),
    [anon_sym_DEL] = ACTIONS(1),
    [anon_sym_DIR] = ACTIONS(1),
    [anon_sym_DATE] = ACTIONS(1),
    [anon_sym_MD] = ACTIONS(1),
    [anon_sym_MOVE] = ACTIONS(1),
    [anon_sym_PATH] = ACTIONS(1),
    [anon_sym_PROMPT] = ACTIONS(1),
    [anon_sym_RD] = ACTIONS(1),
    [anon_sym_REN] = ACTIONS(1),
    [anon_sym_START] = ACTIONS(1),
    [anon_sym_TIME] = ACTIONS(1),
    [anon_sym_TYPE] = ACTIONS(1),
    [anon_sym_VOL] = ACTIONS(1),
    [anon_sym_ATTRIB] = ACTIONS(1),
    [anon_sym_CHKDSK] = ACTIONS(1),
    [anon_sym_CHOICE] = ACTIONS(1),
    [anon_sym_CMD] = ACTIONS(1),
    [anon_sym_COMP] = ACTIONS(1),
    [anon_sym_CONVERT] = ACTIONS(1),
    [anon_sym_DRIVERQUERY] = ACTIONS(1),
    [anon_sym_EXPAND] = ACTIONS(1),
    [anon_sym_FIND] = ACTIONS(1),
    [anon_sym_FORMAT] = ACTIONS(1),
    [anon_sym_HELP] = ACTIONS(1),
    [anon_sym_IPCONFIG] = ACTIONS(1),
    [anon_sym_LABEL] = ACTIONS(1),
    [anon_sym_NET] = ACTIONS(1),
    [anon_sym_PING] = ACTIONS(1),
    [anon_sym_SHUTDOWN] = ACTIONS(1),
    [anon_sym_SORT] = ACTIONS(1),
    [anon_sym_SUBST] = ACTIONS(1),
    [anon_sym_SYSTEMINFO] = ACTIONS(1),
    [anon_sym_TASKKILL] = ACTIONS(1),
    [anon_sym_TASKLIST] = ACTIONS(1),
    [anon_sym_XCOPY] = ACTIONS(1),
    [anon_sym_TREE] = ACTIONS(1),
    [anon_sym_FC] = ACTIONS(1),
    [anon_sym_DISKPART] = ACTIONS(1),
    [anon_sym_TITLE] = ACTIONS(1),
    [anon_sym_ver] = ACTIONS(1),
    [anon_sym_assoc] = ACTIONS(1),
    [anon_sym_cd] = ACTIONS(1),
    [anon_sym_copy] = ACTIONS(1),
    [anon_sym_del] = ACTIONS(1),
    [anon_sym_dir] = ACTIONS(1),
    [anon_sym_date] = ACTIONS(1),
    [anon_sym_md] = ACTIONS(1),
    [anon_sym_move] = ACTIONS(1),
    [anon_sym_path] = ACTIONS(1),
    [anon_sym_prompt] = ACTIONS(1),
    [anon_sym_rd] = ACTIONS(1),
    [anon_sym_ren] = ACTIONS(1),
    [anon_sym_start] = ACTIONS(1),
    [anon_sym_time] = ACTIONS(1),
    [anon_sym_type] = ACTIONS(1),
    [anon_sym_vol] = ACTIONS(1),
    [anon_sym_attrib] = ACTIONS(1),
    [anon_sym_chkdsk] = ACTIONS(1),
    [anon_sym_choice] = ACTIONS(1),
    [anon_sym_cmd] = ACTIONS(1),
    [anon_sym_comp] = ACTIONS(1),
    [anon_sym_convert] = ACTIONS(1),
    [anon_sym_driverquery] = ACTIONS(1),
    [anon_sym_expand] = ACTIONS(1),
    [anon_sym_find] = ACTIONS(1),
    [anon_sym_format] = ACTIONS(1),
    [anon_sym_help] = ACTIONS(1),
    [anon_sym_ipconfig] = ACTIONS(1),
    [anon_sym_label] = ACTIONS(1),
    [anon_sym_net] = ACTIONS(1),
    [anon_sym_ping] = ACTIONS(1),
    [anon_sym_shutdown] = ACTIONS(1),
    [anon_sym_sort] = ACTIONS(1),
    [anon_sym_subst] = ACTIONS(1),
    [anon_sym_systeminfo] = ACTIONS(1),
    [anon_sym_taskkill] = ACTIONS(1),
    [anon_sym_tasklist] = ACTIONS(1),
    [anon_sym_xcopy] = ACTIONS(1),
    [anon_sym_tree] = ACTIONS(1),
    [anon_sym_fc] = ACTIONS(1),
    [anon_sym_diskpart] = ACTIONS(1),
    [anon_sym_title] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [sym_identifier] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [sym_number] = ACTIONS(1),
  },
  [1] = {
    [sym_program] = STATE(38),
    [sym_echooff] = STATE(2),
    [sym_comment] = STATE(2),
    [sym_variable_declaration] = STATE(2),
    [sym_variable_reference] = STATE(2),
    [sym_keyword] = STATE(2),
    [sym_function_definition] = STATE(2),
    [aux_sym_program_repeat1] = STATE(2),
    [ts_builtin_sym_end] = ACTIONS(3),
    [anon_sym_AT] = ACTIONS(5),
    [anon_sym_echooff] = ACTIONS(7),
    [anon_sym_COLON_COLON] = ACTIONS(9),
    [anon_sym_REM] = ACTIONS(11),
    [anon_sym_Rem] = ACTIONS(9),
    [anon_sym_rem] = ACTIONS(11),
    [anon_sym_SET] = ACTIONS(13),
    [anon_sym_Set] = ACTIONS(15),
    [anon_sym_set] = ACTIONS(13),
    [anon_sym_PERCENT] = ACTIONS(17),
    [anon_sym_ECHO] = ACTIONS(19),
    [anon_sym_IF] = ACTIONS(19),
    [anon_sym_GOTO] = ACTIONS(19),
    [anon_sym_EXIT] = ACTIONS(19),
    [anon_sym_FOR] = ACTIONS(21),
    [anon_sym_PAUSE] = ACTIONS(19),
    [anon_sym_CLS] = ACTIONS(19),
    [anon_sym_echo] = ACTIONS(21),
    [anon_sym_if] = ACTIONS(19),
    [anon_sym_goto] = ACTIONS(19),
    [anon_sym_exit] = ACTIONS(19),
    [anon_sym_for] = ACTIONS(21),
    [anon_sym_pause] = ACTIONS(19),
    [anon_sym_cls] = ACTIONS(19),
    [anon_sym_VER] = ACTIONS(19),
    [anon_sym_ASSOC] = ACTIONS(19),
    [anon_sym_CD] = ACTIONS(19),
    [anon_sym_COPY] = ACTIONS(19),
    [anon_sym_DEL] = ACTIONS(19),
    [anon_sym_DIR] = ACTIONS(19),
    [anon_sym_DATE] = ACTIONS(19),
    [anon_sym_MD] = ACTIONS(19),
    [anon_sym_MOVE] = ACTIONS(19),
    [anon_sym_PATH] = ACTIONS(19),
    [anon_sym_PROMPT] = ACTIONS(19),
    [anon_sym_RD] = ACTIONS(19),
    [anon_sym_REN] = ACTIONS(19),
    [anon_sym_START] = ACTIONS(19),
    [anon_sym_TIME] = ACTIONS(19),
    [anon_sym_TYPE] = ACTIONS(19),
    [anon_sym_VOL] = ACTIONS(19),
    [anon_sym_ATTRIB] = ACTIONS(19),
    [anon_sym_CHKDSK] = ACTIONS(19),
    [anon_sym_CHOICE] = ACTIONS(19),
    [anon_sym_CMD] = ACTIONS(19),
    [anon_sym_COMP] = ACTIONS(19),
    [anon_sym_CONVERT] = ACTIONS(19),
    [anon_sym_DRIVERQUERY] = ACTIONS(19),
    [anon_sym_EXPAND] = ACTIONS(19),
    [anon_sym_FIND] = ACTIONS(19),
    [anon_sym_FORMAT] = ACTIONS(19),
    [anon_sym_HELP] = ACTIONS(19),
    [anon_sym_IPCONFIG] = ACTIONS(19),
    [anon_sym_LABEL] = ACTIONS(19),
    [anon_sym_NET] = ACTIONS(19),
    [anon_sym_PING] = ACTIONS(19),
    [anon_sym_SHUTDOWN] = ACTIONS(19),
    [anon_sym_SORT] = ACTIONS(19),
    [anon_sym_SUBST] = ACTIONS(19),
    [anon_sym_SYSTEMINFO] = ACTIONS(19),
    [anon_sym_TASKKILL] = ACTIONS(19),
    [anon_sym_TASKLIST] = ACTIONS(19),
    [anon_sym_XCOPY] = ACTIONS(19),
    [anon_sym_TREE] = ACTIONS(19),
    [anon_sym_FC] = ACTIONS(19),
    [anon_sym_DISKPART] = ACTIONS(19),
    [anon_sym_TITLE] = ACTIONS(19),
    [anon_sym_ver] = ACTIONS(19),
    [anon_sym_assoc] = ACTIONS(19),
    [anon_sym_cd] = ACTIONS(19),
    [anon_sym_copy] = ACTIONS(19),
    [anon_sym_del] = ACTIONS(19),
    [anon_sym_dir] = ACTIONS(19),
    [anon_sym_date] = ACTIONS(19),
    [anon_sym_md] = ACTIONS(19),
    [anon_sym_move] = ACTIONS(19),
    [anon_sym_path] = ACTIONS(19),
    [anon_sym_prompt] = ACTIONS(19),
    [anon_sym_rd] = ACTIONS(19),
    [anon_sym_ren] = ACTIONS(19),
    [anon_sym_start] = ACTIONS(19),
    [anon_sym_time] = ACTIONS(19),
    [anon_sym_type] = ACTIONS(19),
    [anon_sym_vol] = ACTIONS(19),
    [anon_sym_attrib] = ACTIONS(19),
    [anon_sym_chkdsk] = ACTIONS(19),
    [anon_sym_choice] = ACTIONS(19),
    [anon_sym_cmd] = ACTIONS(19),
    [anon_sym_comp] = ACTIONS(19),
    [anon_sym_convert] = ACTIONS(19),
    [anon_sym_driverquery] = ACTIONS(19),
    [anon_sym_expand] = ACTIONS(19),
    [anon_sym_find] = ACTIONS(19),
    [anon_sym_format] = ACTIONS(19),
    [anon_sym_help] = ACTIONS(19),
    [anon_sym_ipconfig] = ACTIONS(19),
    [anon_sym_label] = ACTIONS(19),
    [anon_sym_net] = ACTIONS(19),
    [anon_sym_ping] = ACTIONS(19),
    [anon_sym_shutdown] = ACTIONS(19),
    [anon_sym_sort] = ACTIONS(19),
    [anon_sym_subst] = ACTIONS(19),
    [anon_sym_systeminfo] = ACTIONS(19),
    [anon_sym_taskkill] = ACTIONS(19),
    [anon_sym_tasklist] = ACTIONS(19),
    [anon_sym_xcopy] = ACTIONS(19),
    [anon_sym_tree] = ACTIONS(19),
    [anon_sym_fc] = ACTIONS(19),
    [anon_sym_diskpart] = ACTIONS(19),
    [anon_sym_title] = ACTIONS(19),
    [anon_sym_COLON] = ACTIONS(23),
  },
  [2] = {
    [sym_echooff] = STATE(3),
    [sym_comment] = STATE(3),
    [sym_variable_declaration] = STATE(3),
    [sym_variable_reference] = STATE(3),
    [sym_keyword] = STATE(3),
    [sym_function_definition] = STATE(3),
    [aux_sym_program_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(25),
    [anon_sym_AT] = ACTIONS(5),
    [anon_sym_echooff] = ACTIONS(7),
    [anon_sym_COLON_COLON] = ACTIONS(9),
    [anon_sym_REM] = ACTIONS(11),
    [anon_sym_Rem] = ACTIONS(9),
    [anon_sym_rem] = ACTIONS(11),
    [anon_sym_SET] = ACTIONS(13),
    [anon_sym_Set] = ACTIONS(15),
    [anon_sym_set] = ACTIONS(13),
    [anon_sym_PERCENT] = ACTIONS(17),
    [anon_sym_ECHO] = ACTIONS(19),
    [anon_sym_IF] = ACTIONS(19),
    [anon_sym_GOTO] = ACTIONS(19),
    [anon_sym_EXIT] = ACTIONS(19),
    [anon_sym_FOR] = ACTIONS(21),
    [anon_sym_PAUSE] = ACTIONS(19),
    [anon_sym_CLS] = ACTIONS(19),
    [anon_sym_echo] = ACTIONS(21),
    [anon_sym_if] = ACTIONS(19),
    [anon_sym_goto] = ACTIONS(19),
    [anon_sym_exit] = ACTIONS(19),
    [anon_sym_for] = ACTIONS(21),
    [anon_sym_pause] = ACTIONS(19),
    [anon_sym_cls] = ACTIONS(19),
    [anon_sym_VER] = ACTIONS(19),
    [anon_sym_ASSOC] = ACTIONS(19),
    [anon_sym_CD] = ACTIONS(19),
    [anon_sym_COPY] = ACTIONS(19),
    [anon_sym_DEL] = ACTIONS(19),
    [anon_sym_DIR] = ACTIONS(19),
    [anon_sym_DATE] = ACTIONS(19),
    [anon_sym_MD] = ACTIONS(19),
    [anon_sym_MOVE] = ACTIONS(19),
    [anon_sym_PATH] = ACTIONS(19),
    [anon_sym_PROMPT] = ACTIONS(19),
    [anon_sym_RD] = ACTIONS(19),
    [anon_sym_REN] = ACTIONS(19),
    [anon_sym_START] = ACTIONS(19),
    [anon_sym_TIME] = ACTIONS(19),
    [anon_sym_TYPE] = ACTIONS(19),
    [anon_sym_VOL] = ACTIONS(19),
    [anon_sym_ATTRIB] = ACTIONS(19),
    [anon_sym_CHKDSK] = ACTIONS(19),
    [anon_sym_CHOICE] = ACTIONS(19),
    [anon_sym_CMD] = ACTIONS(19),
    [anon_sym_COMP] = ACTIONS(19),
    [anon_sym_CONVERT] = ACTIONS(19),
    [anon_sym_DRIVERQUERY] = ACTIONS(19),
    [anon_sym_EXPAND] = ACTIONS(19),
    [anon_sym_FIND] = ACTIONS(19),
    [anon_sym_FORMAT] = ACTIONS(19),
    [anon_sym_HELP] = ACTIONS(19),
    [anon_sym_IPCONFIG] = ACTIONS(19),
    [anon_sym_LABEL] = ACTIONS(19),
    [anon_sym_NET] = ACTIONS(19),
    [anon_sym_PING] = ACTIONS(19),
    [anon_sym_SHUTDOWN] = ACTIONS(19),
    [anon_sym_SORT] = ACTIONS(19),
    [anon_sym_SUBST] = ACTIONS(19),
    [anon_sym_SYSTEMINFO] = ACTIONS(19),
    [anon_sym_TASKKILL] = ACTIONS(19),
    [anon_sym_TASKLIST] = ACTIONS(19),
    [anon_sym_XCOPY] = ACTIONS(19),
    [anon_sym_TREE] = ACTIONS(19),
    [anon_sym_FC] = ACTIONS(19),
    [anon_sym_DISKPART] = ACTIONS(19),
    [anon_sym_TITLE] = ACTIONS(19),
    [anon_sym_ver] = ACTIONS(19),
    [anon_sym_assoc] = ACTIONS(19),
    [anon_sym_cd] = ACTIONS(19),
    [anon_sym_copy] = ACTIONS(19),
    [anon_sym_del] = ACTIONS(19),
    [anon_sym_dir] = ACTIONS(19),
    [anon_sym_date] = ACTIONS(19),
    [anon_sym_md] = ACTIONS(19),
    [anon_sym_move] = ACTIONS(19),
    [anon_sym_path] = ACTIONS(19),
    [anon_sym_prompt] = ACTIONS(19),
    [anon_sym_rd] = ACTIONS(19),
    [anon_sym_ren] = ACTIONS(19),
    [anon_sym_start] = ACTIONS(19),
    [anon_sym_time] = ACTIONS(19),
    [anon_sym_type] = ACTIONS(19),
    [anon_sym_vol] = ACTIONS(19),
    [anon_sym_attrib] = ACTIONS(19),
    [anon_sym_chkdsk] = ACTIONS(19),
    [anon_sym_choice] = ACTIONS(19),
    [anon_sym_cmd] = ACTIONS(19),
    [anon_sym_comp] = ACTIONS(19),
    [anon_sym_convert] = ACTIONS(19),
    [anon_sym_driverquery] = ACTIONS(19),
    [anon_sym_expand] = ACTIONS(19),
    [anon_sym_find] = ACTIONS(19),
    [anon_sym_format] = ACTIONS(19),
    [anon_sym_help] = ACTIONS(19),
    [anon_sym_ipconfig] = ACTIONS(19),
    [anon_sym_label] = ACTIONS(19),
    [anon_sym_net] = ACTIONS(19),
    [anon_sym_ping] = ACTIONS(19),
    [anon_sym_shutdown] = ACTIONS(19),
    [anon_sym_sort] = ACTIONS(19),
    [anon_sym_subst] = ACTIONS(19),
    [anon_sym_systeminfo] = ACTIONS(19),
    [anon_sym_taskkill] = ACTIONS(19),
    [anon_sym_tasklist] = ACTIONS(19),
    [anon_sym_xcopy] = ACTIONS(19),
    [anon_sym_tree] = ACTIONS(19),
    [anon_sym_fc] = ACTIONS(19),
    [anon_sym_diskpart] = ACTIONS(19),
    [anon_sym_title] = ACTIONS(19),
    [anon_sym_COLON] = ACTIONS(23),
  },
  [3] = {
    [sym_echooff] = STATE(3),
    [sym_comment] = STATE(3),
    [sym_variable_declaration] = STATE(3),
    [sym_variable_reference] = STATE(3),
    [sym_keyword] = STATE(3),
    [sym_function_definition] = STATE(3),
    [aux_sym_program_repeat1] = STATE(3),
    [ts_builtin_sym_end] = ACTIONS(27),
    [anon_sym_AT] = ACTIONS(29),
    [anon_sym_echooff] = ACTIONS(32),
    [anon_sym_COLON_COLON] = ACTIONS(35),
    [anon_sym_REM] = ACTIONS(38),
    [anon_sym_Rem] = ACTIONS(35),
    [anon_sym_rem] = ACTIONS(38),
    [anon_sym_SET] = ACTIONS(41),
    [anon_sym_Set] = ACTIONS(44),
    [anon_sym_set] = ACTIONS(41),
    [anon_sym_PERCENT] = ACTIONS(47),
    [anon_sym_ECHO] = ACTIONS(50),
    [anon_sym_IF] = ACTIONS(50),
    [anon_sym_GOTO] = ACTIONS(50),
    [anon_sym_EXIT] = ACTIONS(50),
    [anon_sym_FOR] = ACTIONS(53),
    [anon_sym_PAUSE] = ACTIONS(50),
    [anon_sym_CLS] = ACTIONS(50),
    [anon_sym_echo] = ACTIONS(53),
    [anon_sym_if] = ACTIONS(50),
    [anon_sym_goto] = ACTIONS(50),
    [anon_sym_exit] = ACTIONS(50),
    [anon_sym_for] = ACTIONS(53),
    [anon_sym_pause] = ACTIONS(50),
    [anon_sym_cls] = ACTIONS(50),
    [anon_sym_VER] = ACTIONS(50),
    [anon_sym_ASSOC] = ACTIONS(50),
    [anon_sym_CD] = ACTIONS(50),
    [anon_sym_COPY] = ACTIONS(50),
    [anon_sym_DEL] = ACTIONS(50),
    [anon_sym_DIR] = ACTIONS(50),
    [anon_sym_DATE] = ACTIONS(50),
    [anon_sym_MD] = ACTIONS(50),
    [anon_sym_MOVE] = ACTIONS(50),
    [anon_sym_PATH] = ACTIONS(50),
    [anon_sym_PROMPT] = ACTIONS(50),
    [anon_sym_RD] = ACTIONS(50),
    [anon_sym_REN] = ACTIONS(50),
    [anon_sym_START] = ACTIONS(50),
    [anon_sym_TIME] = ACTIONS(50),
    [anon_sym_TYPE] = ACTIONS(50),
    [anon_sym_VOL] = ACTIONS(50),
    [anon_sym_ATTRIB] = ACTIONS(50),
    [anon_sym_CHKDSK] = ACTIONS(50),
    [anon_sym_CHOICE] = ACTIONS(50),
    [anon_sym_CMD] = ACTIONS(50),
    [anon_sym_COMP] = ACTIONS(50),
    [anon_sym_CONVERT] = ACTIONS(50),
    [anon_sym_DRIVERQUERY] = ACTIONS(50),
    [anon_sym_EXPAND] = ACTIONS(50),
    [anon_sym_FIND] = ACTIONS(50),
    [anon_sym_FORMAT] = ACTIONS(50),
    [anon_sym_HELP] = ACTIONS(50),
    [anon_sym_IPCONFIG] = ACTIONS(50),
    [anon_sym_LABEL] = ACTIONS(50),
    [anon_sym_NET] = ACTIONS(50),
    [anon_sym_PING] = ACTIONS(50),
    [anon_sym_SHUTDOWN] = ACTIONS(50),
    [anon_sym_SORT] = ACTIONS(50),
    [anon_sym_SUBST] = ACTIONS(50),
    [anon_sym_SYSTEMINFO] = ACTIONS(50),
    [anon_sym_TASKKILL] = ACTIONS(50),
    [anon_sym_TASKLIST] = ACTIONS(50),
    [anon_sym_XCOPY] = ACTIONS(50),
    [anon_sym_TREE] = ACTIONS(50),
    [anon_sym_FC] = ACTIONS(50),
    [anon_sym_DISKPART] = ACTIONS(50),
    [anon_sym_TITLE] = ACTIONS(50),
    [anon_sym_ver] = ACTIONS(50),
    [anon_sym_assoc] = ACTIONS(50),
    [anon_sym_cd] = ACTIONS(50),
    [anon_sym_copy] = ACTIONS(50),
    [anon_sym_del] = ACTIONS(50),
    [anon_sym_dir] = ACTIONS(50),
    [anon_sym_date] = ACTIONS(50),
    [anon_sym_md] = ACTIONS(50),
    [anon_sym_move] = ACTIONS(50),
    [anon_sym_path] = ACTIONS(50),
    [anon_sym_prompt] = ACTIONS(50),
    [anon_sym_rd] = ACTIONS(50),
    [anon_sym_ren] = ACTIONS(50),
    [anon_sym_start] = ACTIONS(50),
    [anon_sym_time] = ACTIONS(50),
    [anon_sym_type] = ACTIONS(50),
    [anon_sym_vol] = ACTIONS(50),
    [anon_sym_attrib] = ACTIONS(50),
    [anon_sym_chkdsk] = ACTIONS(50),
    [anon_sym_choice] = ACTIONS(50),
    [anon_sym_cmd] = ACTIONS(50),
    [anon_sym_comp] = ACTIONS(50),
    [anon_sym_convert] = ACTIONS(50),
    [anon_sym_driverquery] = ACTIONS(50),
    [anon_sym_expand] = ACTIONS(50),
    [anon_sym_find] = ACTIONS(50),
    [anon_sym_format] = ACTIONS(50),
    [anon_sym_help] = ACTIONS(50),
    [anon_sym_ipconfig] = ACTIONS(50),
    [anon_sym_label] = ACTIONS(50),
    [anon_sym_net] = ACTIONS(50),
    [anon_sym_ping] = ACTIONS(50),
    [anon_sym_shutdown] = ACTIONS(50),
    [anon_sym_sort] = ACTIONS(50),
    [anon_sym_subst] = ACTIONS(50),
    [anon_sym_systeminfo] = ACTIONS(50),
    [anon_sym_taskkill] = ACTIONS(50),
    [anon_sym_tasklist] = ACTIONS(50),
    [anon_sym_xcopy] = ACTIONS(50),
    [anon_sym_tree] = ACTIONS(50),
    [anon_sym_fc] = ACTIONS(50),
    [anon_sym_diskpart] = ACTIONS(50),
    [anon_sym_title] = ACTIONS(50),
    [anon_sym_COLON] = ACTIONS(56),
  },
  [4] = {
    [sym_string] = STATE(16),
    [ts_builtin_sym_end] = ACTIONS(59),
    [anon_sym_AT] = ACTIONS(59),
    [anon_sym_echooff] = ACTIONS(59),
    [anon_sym_COLON_COLON] = ACTIONS(59),
    [anon_sym_REM] = ACTIONS(61),
    [anon_sym_Rem] = ACTIONS(61),
    [anon_sym_rem] = ACTIONS(61),
    [anon_sym_SET] = ACTIONS(61),
    [anon_sym_Set] = ACTIONS(61),
    [anon_sym_set] = ACTIONS(61),
    [anon_sym_SLASHA] = ACTIONS(63),
    [anon_sym_PERCENT] = ACTIONS(59),
    [anon_sym_ECHO] = ACTIONS(61),
    [anon_sym_IF] = ACTIONS(61),
    [anon_sym_GOTO] = ACTIONS(61),
    [anon_sym_EXIT] = ACTIONS(61),
    [anon_sym_FOR] = ACTIONS(61),
    [anon_sym_PAUSE] = ACTIONS(61),
    [anon_sym_CLS] = ACTIONS(61),
    [anon_sym_echo] = ACTIONS(61),
    [anon_sym_if] = ACTIONS(61),
    [anon_sym_goto] = ACTIONS(61),
    [anon_sym_exit] = ACTIONS(61),
    [anon_sym_for] = ACTIONS(61),
    [anon_sym_pause] = ACTIONS(61),
    [anon_sym_cls] = ACTIONS(61),
    [anon_sym_VER] = ACTIONS(61),
    [anon_sym_ASSOC] = ACTIONS(61),
    [anon_sym_CD] = ACTIONS(61),
    [anon_sym_COPY] = ACTIONS(61),
    [anon_sym_DEL] = ACTIONS(61),
    [anon_sym_DIR] = ACTIONS(61),
    [anon_sym_DATE] = ACTIONS(61),
    [anon_sym_MD] = ACTIONS(61),
    [anon_sym_MOVE] = ACTIONS(61),
    [anon_sym_PATH] = ACTIONS(61),
    [anon_sym_PROMPT] = ACTIONS(61),
    [anon_sym_RD] = ACTIONS(61),
    [anon_sym_REN] = ACTIONS(61),
    [anon_sym_START] = ACTIONS(61),
    [anon_sym_TIME] = ACTIONS(61),
    [anon_sym_TYPE] = ACTIONS(61),
    [anon_sym_VOL] = ACTIONS(61),
    [anon_sym_ATTRIB] = ACTIONS(61),
    [anon_sym_CHKDSK] = ACTIONS(61),
    [anon_sym_CHOICE] = ACTIONS(61),
    [anon_sym_CMD] = ACTIONS(61),
    [anon_sym_COMP] = ACTIONS(61),
    [anon_sym_CONVERT] = ACTIONS(61),
    [anon_sym_DRIVERQUERY] = ACTIONS(61),
    [anon_sym_EXPAND] = ACTIONS(61),
    [anon_sym_FIND] = ACTIONS(61),
    [anon_sym_FORMAT] = ACTIONS(61),
    [anon_sym_HELP] = ACTIONS(61),
    [anon_sym_IPCONFIG] = ACTIONS(61),
    [anon_sym_LABEL] = ACTIONS(61),
    [anon_sym_NET] = ACTIONS(61),
    [anon_sym_PING] = ACTIONS(61),
    [anon_sym_SHUTDOWN] = ACTIONS(61),
    [anon_sym_SORT] = ACTIONS(61),
    [anon_sym_SUBST] = ACTIONS(61),
    [anon_sym_SYSTEMINFO] = ACTIONS(61),
    [anon_sym_TASKKILL] = ACTIONS(61),
    [anon_sym_TASKLIST] = ACTIONS(61),
    [anon_sym_XCOPY] = ACTIONS(61),
    [anon_sym_TREE] = ACTIONS(61),
    [anon_sym_FC] = ACTIONS(61),
    [anon_sym_DISKPART] = ACTIONS(61),
    [anon_sym_TITLE] = ACTIONS(61),
    [anon_sym_ver] = ACTIONS(61),
    [anon_sym_assoc] = ACTIONS(61),
    [anon_sym_cd] = ACTIONS(61),
    [anon_sym_copy] = ACTIONS(61),
    [anon_sym_del] = ACTIONS(61),
    [anon_sym_dir] = ACTIONS(61),
    [anon_sym_date] = ACTIONS(61),
    [anon_sym_md] = ACTIONS(61),
    [anon_sym_move] = ACTIONS(61),
    [anon_sym_path] = ACTIONS(61),
    [anon_sym_prompt] = ACTIONS(61),
    [anon_sym_rd] = ACTIONS(61),
    [anon_sym_ren] = ACTIONS(61),
    [anon_sym_start] = ACTIONS(61),
    [anon_sym_time] = ACTIONS(61),
    [anon_sym_type] = ACTIONS(61),
    [anon_sym_vol] = ACTIONS(61),
    [anon_sym_attrib] = ACTIONS(61),
    [anon_sym_chkdsk] = ACTIONS(61),
    [anon_sym_choice] = ACTIONS(61),
    [anon_sym_cmd] = ACTIONS(61),
    [anon_sym_comp] = ACTIONS(61),
    [anon_sym_convert] = ACTIONS(61),
    [anon_sym_driverquery] = ACTIONS(61),
    [anon_sym_expand] = ACTIONS(61),
    [anon_sym_find] = ACTIONS(61),
    [anon_sym_format] = ACTIONS(61),
    [anon_sym_help] = ACTIONS(61),
    [anon_sym_ipconfig] = ACTIONS(61),
    [anon_sym_label] = ACTIONS(61),
    [anon_sym_net] = ACTIONS(61),
    [anon_sym_ping] = ACTIONS(61),
    [anon_sym_shutdown] = ACTIONS(61),
    [anon_sym_sort] = ACTIONS(61),
    [anon_sym_subst] = ACTIONS(61),
    [anon_sym_systeminfo] = ACTIONS(61),
    [anon_sym_taskkill] = ACTIONS(61),
    [anon_sym_tasklist] = ACTIONS(61),
    [anon_sym_xcopy] = ACTIONS(61),
    [anon_sym_tree] = ACTIONS(61),
    [anon_sym_fc] = ACTIONS(61),
    [anon_sym_diskpart] = ACTIONS(61),
    [anon_sym_title] = ACTIONS(61),
    [anon_sym_COLON] = ACTIONS(61),
    [sym_identifier] = ACTIONS(65),
    [anon_sym_DQUOTE] = ACTIONS(67),
    [sym_number] = ACTIONS(69),
  },
  [5] = {
    [sym_string] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(71),
    [anon_sym_AT] = ACTIONS(71),
    [anon_sym_echooff] = ACTIONS(71),
    [anon_sym_COLON_COLON] = ACTIONS(71),
    [anon_sym_REM] = ACTIONS(73),
    [anon_sym_Rem] = ACTIONS(73),
    [anon_sym_rem] = ACTIONS(73),
    [anon_sym_SET] = ACTIONS(73),
    [anon_sym_Set] = ACTIONS(73),
    [anon_sym_set] = ACTIONS(73),
    [anon_sym_SLASHA] = ACTIONS(75),
    [anon_sym_PERCENT] = ACTIONS(71),
    [anon_sym_ECHO] = ACTIONS(73),
    [anon_sym_IF] = ACTIONS(73),
    [anon_sym_GOTO] = ACTIONS(73),
    [anon_sym_EXIT] = ACTIONS(73),
    [anon_sym_FOR] = ACTIONS(73),
    [anon_sym_PAUSE] = ACTIONS(73),
    [anon_sym_CLS] = ACTIONS(73),
    [anon_sym_echo] = ACTIONS(73),
    [anon_sym_if] = ACTIONS(73),
    [anon_sym_goto] = ACTIONS(73),
    [anon_sym_exit] = ACTIONS(73),
    [anon_sym_for] = ACTIONS(73),
    [anon_sym_pause] = ACTIONS(73),
    [anon_sym_cls] = ACTIONS(73),
    [anon_sym_VER] = ACTIONS(73),
    [anon_sym_ASSOC] = ACTIONS(73),
    [anon_sym_CD] = ACTIONS(73),
    [anon_sym_COPY] = ACTIONS(73),
    [anon_sym_DEL] = ACTIONS(73),
    [anon_sym_DIR] = ACTIONS(73),
    [anon_sym_DATE] = ACTIONS(73),
    [anon_sym_MD] = ACTIONS(73),
    [anon_sym_MOVE] = ACTIONS(73),
    [anon_sym_PATH] = ACTIONS(73),
    [anon_sym_PROMPT] = ACTIONS(73),
    [anon_sym_RD] = ACTIONS(73),
    [anon_sym_REN] = ACTIONS(73),
    [anon_sym_START] = ACTIONS(73),
    [anon_sym_TIME] = ACTIONS(73),
    [anon_sym_TYPE] = ACTIONS(73),
    [anon_sym_VOL] = ACTIONS(73),
    [anon_sym_ATTRIB] = ACTIONS(73),
    [anon_sym_CHKDSK] = ACTIONS(73),
    [anon_sym_CHOICE] = ACTIONS(73),
    [anon_sym_CMD] = ACTIONS(73),
    [anon_sym_COMP] = ACTIONS(73),
    [anon_sym_CONVERT] = ACTIONS(73),
    [anon_sym_DRIVERQUERY] = ACTIONS(73),
    [anon_sym_EXPAND] = ACTIONS(73),
    [anon_sym_FIND] = ACTIONS(73),
    [anon_sym_FORMAT] = ACTIONS(73),
    [anon_sym_HELP] = ACTIONS(73),
    [anon_sym_IPCONFIG] = ACTIONS(73),
    [anon_sym_LABEL] = ACTIONS(73),
    [anon_sym_NET] = ACTIONS(73),
    [anon_sym_PING] = ACTIONS(73),
    [anon_sym_SHUTDOWN] = ACTIONS(73),
    [anon_sym_SORT] = ACTIONS(73),
    [anon_sym_SUBST] = ACTIONS(73),
    [anon_sym_SYSTEMINFO] = ACTIONS(73),
    [anon_sym_TASKKILL] = ACTIONS(73),
    [anon_sym_TASKLIST] = ACTIONS(73),
    [anon_sym_XCOPY] = ACTIONS(73),
    [anon_sym_TREE] = ACTIONS(73),
    [anon_sym_FC] = ACTIONS(73),
    [anon_sym_DISKPART] = ACTIONS(73),
    [anon_sym_TITLE] = ACTIONS(73),
    [anon_sym_ver] = ACTIONS(73),
    [anon_sym_assoc] = ACTIONS(73),
    [anon_sym_cd] = ACTIONS(73),
    [anon_sym_copy] = ACTIONS(73),
    [anon_sym_del] = ACTIONS(73),
    [anon_sym_dir] = ACTIONS(73),
    [anon_sym_date] = ACTIONS(73),
    [anon_sym_md] = ACTIONS(73),
    [anon_sym_move] = ACTIONS(73),
    [anon_sym_path] = ACTIONS(73),
    [anon_sym_prompt] = ACTIONS(73),
    [anon_sym_rd] = ACTIONS(73),
    [anon_sym_ren] = ACTIONS(73),
    [anon_sym_start] = ACTIONS(73),
    [anon_sym_time] = ACTIONS(73),
    [anon_sym_type] = ACTIONS(73),
    [anon_sym_vol] = ACTIONS(73),
    [anon_sym_attrib] = ACTIONS(73),
    [anon_sym_chkdsk] = ACTIONS(73),
    [anon_sym_choice] = ACTIONS(73),
    [anon_sym_cmd] = ACTIONS(73),
    [anon_sym_comp] = ACTIONS(73),
    [anon_sym_convert] = ACTIONS(73),
    [anon_sym_driverquery] = ACTIONS(73),
    [anon_sym_expand] = ACTIONS(73),
    [anon_sym_find] = ACTIONS(73),
    [anon_sym_format] = ACTIONS(73),
    [anon_sym_help] = ACTIONS(73),
    [anon_sym_ipconfig] = ACTIONS(73),
    [anon_sym_label] = ACTIONS(73),
    [anon_sym_net] = ACTIONS(73),
    [anon_sym_ping] = ACTIONS(73),
    [anon_sym_shutdown] = ACTIONS(73),
    [anon_sym_sort] = ACTIONS(73),
    [anon_sym_subst] = ACTIONS(73),
    [anon_sym_systeminfo] = ACTIONS(73),
    [anon_sym_taskkill] = ACTIONS(73),
    [anon_sym_tasklist] = ACTIONS(73),
    [anon_sym_xcopy] = ACTIONS(73),
    [anon_sym_tree] = ACTIONS(73),
    [anon_sym_fc] = ACTIONS(73),
    [anon_sym_diskpart] = ACTIONS(73),
    [anon_sym_title] = ACTIONS(73),
    [anon_sym_COLON] = ACTIONS(73),
    [sym_identifier] = ACTIONS(77),
    [anon_sym_DQUOTE] = ACTIONS(67),
    [sym_number] = ACTIONS(79),
  },
  [6] = {
    [sym_string] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(71),
    [anon_sym_AT] = ACTIONS(73),
    [anon_sym_echooff] = ACTIONS(73),
    [anon_sym_COLON_COLON] = ACTIONS(73),
    [aux_sym_comment_token1] = ACTIONS(81),
    [anon_sym_REM] = ACTIONS(73),
    [anon_sym_Rem] = ACTIONS(73),
    [anon_sym_rem] = ACTIONS(73),
    [anon_sym_SET] = ACTIONS(73),
    [anon_sym_Set] = ACTIONS(73),
    [anon_sym_set] = ACTIONS(73),
    [anon_sym_PERCENT] = ACTIONS(73),
    [anon_sym_ECHO] = ACTIONS(73),
    [anon_sym_IF] = ACTIONS(73),
    [anon_sym_GOTO] = ACTIONS(73),
    [anon_sym_EXIT] = ACTIONS(73),
    [anon_sym_FOR] = ACTIONS(73),
    [anon_sym_PAUSE] = ACTIONS(73),
    [anon_sym_CLS] = ACTIONS(73),
    [anon_sym_echo] = ACTIONS(73),
    [anon_sym_if] = ACTIONS(73),
    [anon_sym_goto] = ACTIONS(73),
    [anon_sym_exit] = ACTIONS(73),
    [anon_sym_for] = ACTIONS(73),
    [anon_sym_pause] = ACTIONS(73),
    [anon_sym_cls] = ACTIONS(73),
    [anon_sym_VER] = ACTIONS(73),
    [anon_sym_ASSOC] = ACTIONS(73),
    [anon_sym_CD] = ACTIONS(73),
    [anon_sym_COPY] = ACTIONS(73),
    [anon_sym_DEL] = ACTIONS(73),
    [anon_sym_DIR] = ACTIONS(73),
    [anon_sym_DATE] = ACTIONS(73),
    [anon_sym_MD] = ACTIONS(73),
    [anon_sym_MOVE] = ACTIONS(73),
    [anon_sym_PATH] = ACTIONS(73),
    [anon_sym_PROMPT] = ACTIONS(73),
    [anon_sym_RD] = ACTIONS(73),
    [anon_sym_REN] = ACTIONS(73),
    [anon_sym_START] = ACTIONS(73),
    [anon_sym_TIME] = ACTIONS(73),
    [anon_sym_TYPE] = ACTIONS(73),
    [anon_sym_VOL] = ACTIONS(73),
    [anon_sym_ATTRIB] = ACTIONS(73),
    [anon_sym_CHKDSK] = ACTIONS(73),
    [anon_sym_CHOICE] = ACTIONS(73),
    [anon_sym_CMD] = ACTIONS(73),
    [anon_sym_COMP] = ACTIONS(73),
    [anon_sym_CONVERT] = ACTIONS(73),
    [anon_sym_DRIVERQUERY] = ACTIONS(73),
    [anon_sym_EXPAND] = ACTIONS(73),
    [anon_sym_FIND] = ACTIONS(73),
    [anon_sym_FORMAT] = ACTIONS(73),
    [anon_sym_HELP] = ACTIONS(73),
    [anon_sym_IPCONFIG] = ACTIONS(73),
    [anon_sym_LABEL] = ACTIONS(73),
    [anon_sym_NET] = ACTIONS(73),
    [anon_sym_PING] = ACTIONS(73),
    [anon_sym_SHUTDOWN] = ACTIONS(73),
    [anon_sym_SORT] = ACTIONS(73),
    [anon_sym_SUBST] = ACTIONS(73),
    [anon_sym_SYSTEMINFO] = ACTIONS(73),
    [anon_sym_TASKKILL] = ACTIONS(73),
    [anon_sym_TASKLIST] = ACTIONS(73),
    [anon_sym_XCOPY] = ACTIONS(73),
    [anon_sym_TREE] = ACTIONS(73),
    [anon_sym_FC] = ACTIONS(73),
    [anon_sym_DISKPART] = ACTIONS(73),
    [anon_sym_TITLE] = ACTIONS(73),
    [anon_sym_ver] = ACTIONS(73),
    [anon_sym_assoc] = ACTIONS(73),
    [anon_sym_cd] = ACTIONS(73),
    [anon_sym_copy] = ACTIONS(73),
    [anon_sym_del] = ACTIONS(73),
    [anon_sym_dir] = ACTIONS(73),
    [anon_sym_date] = ACTIONS(73),
    [anon_sym_md] = ACTIONS(73),
    [anon_sym_move] = ACTIONS(73),
    [anon_sym_path] = ACTIONS(73),
    [anon_sym_prompt] = ACTIONS(73),
    [anon_sym_rd] = ACTIONS(73),
    [anon_sym_ren] = ACTIONS(73),
    [anon_sym_start] = ACTIONS(73),
    [anon_sym_time] = ACTIONS(73),
    [anon_sym_type] = ACTIONS(73),
    [anon_sym_vol] = ACTIONS(73),
    [anon_sym_attrib] = ACTIONS(73),
    [anon_sym_chkdsk] = ACTIONS(73),
    [anon_sym_choice] = ACTIONS(73),
    [anon_sym_cmd] = ACTIONS(73),
    [anon_sym_comp] = ACTIONS(73),
    [anon_sym_convert] = ACTIONS(73),
    [anon_sym_driverquery] = ACTIONS(73),
    [anon_sym_expand] = ACTIONS(73),
    [anon_sym_find] = ACTIONS(73),
    [anon_sym_format] = ACTIONS(73),
    [anon_sym_help] = ACTIONS(73),
    [anon_sym_ipconfig] = ACTIONS(73),
    [anon_sym_label] = ACTIONS(73),
    [anon_sym_net] = ACTIONS(73),
    [anon_sym_ping] = ACTIONS(73),
    [anon_sym_shutdown] = ACTIONS(73),
    [anon_sym_sort] = ACTIONS(73),
    [anon_sym_subst] = ACTIONS(73),
    [anon_sym_systeminfo] = ACTIONS(73),
    [anon_sym_taskkill] = ACTIONS(73),
    [anon_sym_tasklist] = ACTIONS(73),
    [anon_sym_xcopy] = ACTIONS(73),
    [anon_sym_tree] = ACTIONS(73),
    [anon_sym_fc] = ACTIONS(73),
    [anon_sym_diskpart] = ACTIONS(73),
    [anon_sym_title] = ACTIONS(73),
    [anon_sym_COLON] = ACTIONS(73),
    [anon_sym_DQUOTE] = ACTIONS(83),
    [sym_number] = ACTIONS(85),
  },
  [7] = {
    [sym_string] = STATE(16),
    [ts_builtin_sym_end] = ACTIONS(59),
    [anon_sym_AT] = ACTIONS(61),
    [anon_sym_echooff] = ACTIONS(61),
    [anon_sym_COLON_COLON] = ACTIONS(61),
    [aux_sym_comment_token1] = ACTIONS(87),
    [anon_sym_REM] = ACTIONS(61),
    [anon_sym_Rem] = ACTIONS(61),
    [anon_sym_rem] = ACTIONS(61),
    [anon_sym_SET] = ACTIONS(61),
    [anon_sym_Set] = ACTIONS(61),
    [anon_sym_set] = ACTIONS(61),
    [anon_sym_PERCENT] = ACTIONS(61),
    [anon_sym_ECHO] = ACTIONS(61),
    [anon_sym_IF] = ACTIONS(61),
    [anon_sym_GOTO] = ACTIONS(61),
    [anon_sym_EXIT] = ACTIONS(61),
    [anon_sym_FOR] = ACTIONS(61),
    [anon_sym_PAUSE] = ACTIONS(61),
    [anon_sym_CLS] = ACTIONS(61),
    [anon_sym_echo] = ACTIONS(61),
    [anon_sym_if] = ACTIONS(61),
    [anon_sym_goto] = ACTIONS(61),
    [anon_sym_exit] = ACTIONS(61),
    [anon_sym_for] = ACTIONS(61),
    [anon_sym_pause] = ACTIONS(61),
    [anon_sym_cls] = ACTIONS(61),
    [anon_sym_VER] = ACTIONS(61),
    [anon_sym_ASSOC] = ACTIONS(61),
    [anon_sym_CD] = ACTIONS(61),
    [anon_sym_COPY] = ACTIONS(61),
    [anon_sym_DEL] = ACTIONS(61),
    [anon_sym_DIR] = ACTIONS(61),
    [anon_sym_DATE] = ACTIONS(61),
    [anon_sym_MD] = ACTIONS(61),
    [anon_sym_MOVE] = ACTIONS(61),
    [anon_sym_PATH] = ACTIONS(61),
    [anon_sym_PROMPT] = ACTIONS(61),
    [anon_sym_RD] = ACTIONS(61),
    [anon_sym_REN] = ACTIONS(61),
    [anon_sym_START] = ACTIONS(61),
    [anon_sym_TIME] = ACTIONS(61),
    [anon_sym_TYPE] = ACTIONS(61),
    [anon_sym_VOL] = ACTIONS(61),
    [anon_sym_ATTRIB] = ACTIONS(61),
    [anon_sym_CHKDSK] = ACTIONS(61),
    [anon_sym_CHOICE] = ACTIONS(61),
    [anon_sym_CMD] = ACTIONS(61),
    [anon_sym_COMP] = ACTIONS(61),
    [anon_sym_CONVERT] = ACTIONS(61),
    [anon_sym_DRIVERQUERY] = ACTIONS(61),
    [anon_sym_EXPAND] = ACTIONS(61),
    [anon_sym_FIND] = ACTIONS(61),
    [anon_sym_FORMAT] = ACTIONS(61),
    [anon_sym_HELP] = ACTIONS(61),
    [anon_sym_IPCONFIG] = ACTIONS(61),
    [anon_sym_LABEL] = ACTIONS(61),
    [anon_sym_NET] = ACTIONS(61),
    [anon_sym_PING] = ACTIONS(61),
    [anon_sym_SHUTDOWN] = ACTIONS(61),
    [anon_sym_SORT] = ACTIONS(61),
    [anon_sym_SUBST] = ACTIONS(61),
    [anon_sym_SYSTEMINFO] = ACTIONS(61),
    [anon_sym_TASKKILL] = ACTIONS(61),
    [anon_sym_TASKLIST] = ACTIONS(61),
    [anon_sym_XCOPY] = ACTIONS(61),
    [anon_sym_TREE] = ACTIONS(61),
    [anon_sym_FC] = ACTIONS(61),
    [anon_sym_DISKPART] = ACTIONS(61),
    [anon_sym_TITLE] = ACTIONS(61),
    [anon_sym_ver] = ACTIONS(61),
    [anon_sym_assoc] = ACTIONS(61),
    [anon_sym_cd] = ACTIONS(61),
    [anon_sym_copy] = ACTIONS(61),
    [anon_sym_del] = ACTIONS(61),
    [anon_sym_dir] = ACTIONS(61),
    [anon_sym_date] = ACTIONS(61),
    [anon_sym_md] = ACTIONS(61),
    [anon_sym_move] = ACTIONS(61),
    [anon_sym_path] = ACTIONS(61),
    [anon_sym_prompt] = ACTIONS(61),
    [anon_sym_rd] = ACTIONS(61),
    [anon_sym_ren] = ACTIONS(61),
    [anon_sym_start] = ACTIONS(61),
    [anon_sym_time] = ACTIONS(61),
    [anon_sym_type] = ACTIONS(61),
    [anon_sym_vol] = ACTIONS(61),
    [anon_sym_attrib] = ACTIONS(61),
    [anon_sym_chkdsk] = ACTIONS(61),
    [anon_sym_choice] = ACTIONS(61),
    [anon_sym_cmd] = ACTIONS(61),
    [anon_sym_comp] = ACTIONS(61),
    [anon_sym_convert] = ACTIONS(61),
    [anon_sym_driverquery] = ACTIONS(61),
    [anon_sym_expand] = ACTIONS(61),
    [anon_sym_find] = ACTIONS(61),
    [anon_sym_format] = ACTIONS(61),
    [anon_sym_help] = ACTIONS(61),
    [anon_sym_ipconfig] = ACTIONS(61),
    [anon_sym_label] = ACTIONS(61),
    [anon_sym_net] = ACTIONS(61),
    [anon_sym_ping] = ACTIONS(61),
    [anon_sym_shutdown] = ACTIONS(61),
    [anon_sym_sort] = ACTIONS(61),
    [anon_sym_subst] = ACTIONS(61),
    [anon_sym_systeminfo] = ACTIONS(61),
    [anon_sym_taskkill] = ACTIONS(61),
    [anon_sym_tasklist] = ACTIONS(61),
    [anon_sym_xcopy] = ACTIONS(61),
    [anon_sym_tree] = ACTIONS(61),
    [anon_sym_fc] = ACTIONS(61),
    [anon_sym_diskpart] = ACTIONS(61),
    [anon_sym_title] = ACTIONS(61),
    [anon_sym_COLON] = ACTIONS(61),
    [anon_sym_DQUOTE] = ACTIONS(83),
    [sym_number] = ACTIONS(89),
  },
  [8] = {
    [sym_string] = STATE(16),
    [ts_builtin_sym_end] = ACTIONS(59),
    [anon_sym_AT] = ACTIONS(59),
    [anon_sym_echooff] = ACTIONS(59),
    [anon_sym_COLON_COLON] = ACTIONS(59),
    [anon_sym_REM] = ACTIONS(59),
    [anon_sym_Rem] = ACTIONS(59),
    [anon_sym_rem] = ACTIONS(59),
    [anon_sym_SET] = ACTIONS(59),
    [anon_sym_Set] = ACTIONS(59),
    [anon_sym_set] = ACTIONS(59),
    [anon_sym_PERCENT] = ACTIONS(59),
    [anon_sym_ECHO] = ACTIONS(59),
    [anon_sym_IF] = ACTIONS(59),
    [anon_sym_GOTO] = ACTIONS(59),
    [anon_sym_EXIT] = ACTIONS(59),
    [anon_sym_FOR] = ACTIONS(61),
    [anon_sym_PAUSE] = ACTIONS(59),
    [anon_sym_CLS] = ACTIONS(59),
    [anon_sym_echo] = ACTIONS(61),
    [anon_sym_if] = ACTIONS(59),
    [anon_sym_goto] = ACTIONS(59),
    [anon_sym_exit] = ACTIONS(59),
    [anon_sym_for] = ACTIONS(61),
    [anon_sym_pause] = ACTIONS(59),
    [anon_sym_cls] = ACTIONS(59),
    [anon_sym_VER] = ACTIONS(59),
    [anon_sym_ASSOC] = ACTIONS(59),
    [anon_sym_CD] = ACTIONS(59),
    [anon_sym_COPY] = ACTIONS(59),
    [anon_sym_DEL] = ACTIONS(59),
    [anon_sym_DIR] = ACTIONS(59),
    [anon_sym_DATE] = ACTIONS(59),
    [anon_sym_MD] = ACTIONS(59),
    [anon_sym_MOVE] = ACTIONS(59),
    [anon_sym_PATH] = ACTIONS(59),
    [anon_sym_PROMPT] = ACTIONS(59),
    [anon_sym_RD] = ACTIONS(59),
    [anon_sym_REN] = ACTIONS(59),
    [anon_sym_START] = ACTIONS(59),
    [anon_sym_TIME] = ACTIONS(59),
    [anon_sym_TYPE] = ACTIONS(59),
    [anon_sym_VOL] = ACTIONS(59),
    [anon_sym_ATTRIB] = ACTIONS(59),
    [anon_sym_CHKDSK] = ACTIONS(59),
    [anon_sym_CHOICE] = ACTIONS(59),
    [anon_sym_CMD] = ACTIONS(59),
    [anon_sym_COMP] = ACTIONS(59),
    [anon_sym_CONVERT] = ACTIONS(59),
    [anon_sym_DRIVERQUERY] = ACTIONS(59),
    [anon_sym_EXPAND] = ACTIONS(59),
    [anon_sym_FIND] = ACTIONS(59),
    [anon_sym_FORMAT] = ACTIONS(59),
    [anon_sym_HELP] = ACTIONS(59),
    [anon_sym_IPCONFIG] = ACTIONS(59),
    [anon_sym_LABEL] = ACTIONS(59),
    [anon_sym_NET] = ACTIONS(59),
    [anon_sym_PING] = ACTIONS(59),
    [anon_sym_SHUTDOWN] = ACTIONS(59),
    [anon_sym_SORT] = ACTIONS(59),
    [anon_sym_SUBST] = ACTIONS(59),
    [anon_sym_SYSTEMINFO] = ACTIONS(59),
    [anon_sym_TASKKILL] = ACTIONS(59),
    [anon_sym_TASKLIST] = ACTIONS(59),
    [anon_sym_XCOPY] = ACTIONS(59),
    [anon_sym_TREE] = ACTIONS(59),
    [anon_sym_FC] = ACTIONS(59),
    [anon_sym_DISKPART] = ACTIONS(59),
    [anon_sym_TITLE] = ACTIONS(59),
    [anon_sym_ver] = ACTIONS(59),
    [anon_sym_assoc] = ACTIONS(59),
    [anon_sym_cd] = ACTIONS(59),
    [anon_sym_copy] = ACTIONS(59),
    [anon_sym_del] = ACTIONS(59),
    [anon_sym_dir] = ACTIONS(59),
    [anon_sym_date] = ACTIONS(59),
    [anon_sym_md] = ACTIONS(59),
    [anon_sym_move] = ACTIONS(59),
    [anon_sym_path] = ACTIONS(59),
    [anon_sym_prompt] = ACTIONS(59),
    [anon_sym_rd] = ACTIONS(59),
    [anon_sym_ren] = ACTIONS(59),
    [anon_sym_start] = ACTIONS(59),
    [anon_sym_time] = ACTIONS(59),
    [anon_sym_type] = ACTIONS(59),
    [anon_sym_vol] = ACTIONS(59),
    [anon_sym_attrib] = ACTIONS(59),
    [anon_sym_chkdsk] = ACTIONS(59),
    [anon_sym_choice] = ACTIONS(59),
    [anon_sym_cmd] = ACTIONS(59),
    [anon_sym_comp] = ACTIONS(59),
    [anon_sym_convert] = ACTIONS(59),
    [anon_sym_driverquery] = ACTIONS(59),
    [anon_sym_expand] = ACTIONS(59),
    [anon_sym_find] = ACTIONS(59),
    [anon_sym_format] = ACTIONS(59),
    [anon_sym_help] = ACTIONS(59),
    [anon_sym_ipconfig] = ACTIONS(59),
    [anon_sym_label] = ACTIONS(59),
    [anon_sym_net] = ACTIONS(59),
    [anon_sym_ping] = ACTIONS(59),
    [anon_sym_shutdown] = ACTIONS(59),
    [anon_sym_sort] = ACTIONS(59),
    [anon_sym_subst] = ACTIONS(59),
    [anon_sym_systeminfo] = ACTIONS(59),
    [anon_sym_taskkill] = ACTIONS(59),
    [anon_sym_tasklist] = ACTIONS(59),
    [anon_sym_xcopy] = ACTIONS(59),
    [anon_sym_tree] = ACTIONS(59),
    [anon_sym_fc] = ACTIONS(59),
    [anon_sym_diskpart] = ACTIONS(59),
    [anon_sym_title] = ACTIONS(59),
    [anon_sym_COLON] = ACTIONS(61),
    [anon_sym_DQUOTE] = ACTIONS(67),
    [sym_number] = ACTIONS(69),
  },
  [9] = {
    [sym_string] = STATE(14),
    [ts_builtin_sym_end] = ACTIONS(71),
    [anon_sym_AT] = ACTIONS(71),
    [anon_sym_echooff] = ACTIONS(71),
    [anon_sym_COLON_COLON] = ACTIONS(71),
    [anon_sym_REM] = ACTIONS(71),
    [anon_sym_Rem] = ACTIONS(71),
    [anon_sym_rem] = ACTIONS(71),
    [anon_sym_SET] = ACTIONS(71),
    [anon_sym_Set] = ACTIONS(71),
    [anon_sym_set] = ACTIONS(71),
    [anon_sym_PERCENT] = ACTIONS(71),
    [anon_sym_ECHO] = ACTIONS(71),
    [anon_sym_IF] = ACTIONS(71),
    [anon_sym_GOTO] = ACTIONS(71),
    [anon_sym_EXIT] = ACTIONS(71),
    [anon_sym_FOR] = ACTIONS(73),
    [anon_sym_PAUSE] = ACTIONS(71),
    [anon_sym_CLS] = ACTIONS(71),
    [anon_sym_echo] = ACTIONS(73),
    [anon_sym_if] = ACTIONS(71),
    [anon_sym_goto] = ACTIONS(71),
    [anon_sym_exit] = ACTIONS(71),
    [anon_sym_for] = ACTIONS(73),
    [anon_sym_pause] = ACTIONS(71),
    [anon_sym_cls] = ACTIONS(71),
    [anon_sym_VER] = ACTIONS(71),
    [anon_sym_ASSOC] = ACTIONS(71),
    [anon_sym_CD] = ACTIONS(71),
    [anon_sym_COPY] = ACTIONS(71),
    [anon_sym_DEL] = ACTIONS(71),
    [anon_sym_DIR] = ACTIONS(71),
    [anon_sym_DATE] = ACTIONS(71),
    [anon_sym_MD] = ACTIONS(71),
    [anon_sym_MOVE] = ACTIONS(71),
    [anon_sym_PATH] = ACTIONS(71),
    [anon_sym_PROMPT] = ACTIONS(71),
    [anon_sym_RD] = ACTIONS(71),
    [anon_sym_REN] = ACTIONS(71),
    [anon_sym_START] = ACTIONS(71),
    [anon_sym_TIME] = ACTIONS(71),
    [anon_sym_TYPE] = ACTIONS(71),
    [anon_sym_VOL] = ACTIONS(71),
    [anon_sym_ATTRIB] = ACTIONS(71),
    [anon_sym_CHKDSK] = ACTIONS(71),
    [anon_sym_CHOICE] = ACTIONS(71),
    [anon_sym_CMD] = ACTIONS(71),
    [anon_sym_COMP] = ACTIONS(71),
    [anon_sym_CONVERT] = ACTIONS(71),
    [anon_sym_DRIVERQUERY] = ACTIONS(71),
    [anon_sym_EXPAND] = ACTIONS(71),
    [anon_sym_FIND] = ACTIONS(71),
    [anon_sym_FORMAT] = ACTIONS(71),
    [anon_sym_HELP] = ACTIONS(71),
    [anon_sym_IPCONFIG] = ACTIONS(71),
    [anon_sym_LABEL] = ACTIONS(71),
    [anon_sym_NET] = ACTIONS(71),
    [anon_sym_PING] = ACTIONS(71),
    [anon_sym_SHUTDOWN] = ACTIONS(71),
    [anon_sym_SORT] = ACTIONS(71),
    [anon_sym_SUBST] = ACTIONS(71),
    [anon_sym_SYSTEMINFO] = ACTIONS(71),
    [anon_sym_TASKKILL] = ACTIONS(71),
    [anon_sym_TASKLIST] = ACTIONS(71),
    [anon_sym_XCOPY] = ACTIONS(71),
    [anon_sym_TREE] = ACTIONS(71),
    [anon_sym_FC] = ACTIONS(71),
    [anon_sym_DISKPART] = ACTIONS(71),
    [anon_sym_TITLE] = ACTIONS(71),
    [anon_sym_ver] = ACTIONS(71),
    [anon_sym_assoc] = ACTIONS(71),
    [anon_sym_cd] = ACTIONS(71),
    [anon_sym_copy] = ACTIONS(71),
    [anon_sym_del] = ACTIONS(71),
    [anon_sym_dir] = ACTIONS(71),
    [anon_sym_date] = ACTIONS(71),
    [anon_sym_md] = ACTIONS(71),
    [anon_sym_move] = ACTIONS(71),
    [anon_sym_path] = ACTIONS(71),
    [anon_sym_prompt] = ACTIONS(71),
    [anon_sym_rd] = ACTIONS(71),
    [anon_sym_ren] = ACTIONS(71),
    [anon_sym_start] = ACTIONS(71),
    [anon_sym_time] = ACTIONS(71),
    [anon_sym_type] = ACTIONS(71),
    [anon_sym_vol] = ACTIONS(71),
    [anon_sym_attrib] = ACTIONS(71),
    [anon_sym_chkdsk] = ACTIONS(71),
    [anon_sym_choice] = ACTIONS(71),
    [anon_sym_cmd] = ACTIONS(71),
    [anon_sym_comp] = ACTIONS(71),
    [anon_sym_convert] = ACTIONS(71),
    [anon_sym_driverquery] = ACTIONS(71),
    [anon_sym_expand] = ACTIONS(71),
    [anon_sym_find] = ACTIONS(71),
    [anon_sym_format] = ACTIONS(71),
    [anon_sym_help] = ACTIONS(71),
    [anon_sym_ipconfig] = ACTIONS(71),
    [anon_sym_label] = ACTIONS(71),
    [anon_sym_net] = ACTIONS(71),
    [anon_sym_ping] = ACTIONS(71),
    [anon_sym_shutdown] = ACTIONS(71),
    [anon_sym_sort] = ACTIONS(71),
    [anon_sym_subst] = ACTIONS(71),
    [anon_sym_systeminfo] = ACTIONS(71),
    [anon_sym_taskkill] = ACTIONS(71),
    [anon_sym_tasklist] = ACTIONS(71),
    [anon_sym_xcopy] = ACTIONS(71),
    [anon_sym_tree] = ACTIONS(71),
    [anon_sym_fc] = ACTIONS(71),
    [anon_sym_diskpart] = ACTIONS(71),
    [anon_sym_title] = ACTIONS(71),
    [anon_sym_COLON] = ACTIONS(73),
    [anon_sym_DQUOTE] = ACTIONS(67),
    [sym_number] = ACTIONS(79),
  },
  [10] = {
    [ts_builtin_sym_end] = ACTIONS(91),
    [anon_sym_AT] = ACTIONS(91),
    [anon_sym_echooff] = ACTIONS(91),
    [anon_sym_COLON_COLON] = ACTIONS(91),
    [anon_sym_REM] = ACTIONS(91),
    [anon_sym_Rem] = ACTIONS(91),
    [anon_sym_rem] = ACTIONS(91),
    [anon_sym_SET] = ACTIONS(91),
    [anon_sym_Set] = ACTIONS(91),
    [anon_sym_set] = ACTIONS(91),
    [anon_sym_PERCENT] = ACTIONS(91),
    [anon_sym_ECHO] = ACTIONS(91),
    [anon_sym_IF] = ACTIONS(91),
    [anon_sym_GOTO] = ACTIONS(91),
    [anon_sym_EXIT] = ACTIONS(91),
    [anon_sym_FOR] = ACTIONS(93),
    [anon_sym_PAUSE] = ACTIONS(91),
    [anon_sym_CLS] = ACTIONS(91),
    [anon_sym_echo] = ACTIONS(93),
    [anon_sym_if] = ACTIONS(91),
    [anon_sym_goto] = ACTIONS(91),
    [anon_sym_exit] = ACTIONS(91),
    [anon_sym_for] = ACTIONS(93),
    [anon_sym_pause] = ACTIONS(91),
    [anon_sym_cls] = ACTIONS(91),
    [anon_sym_VER] = ACTIONS(91),
    [anon_sym_ASSOC] = ACTIONS(91),
    [anon_sym_CD] = ACTIONS(91),
    [anon_sym_COPY] = ACTIONS(91),
    [anon_sym_DEL] = ACTIONS(91),
    [anon_sym_DIR] = ACTIONS(91),
    [anon_sym_DATE] = ACTIONS(91),
    [anon_sym_MD] = ACTIONS(91),
    [anon_sym_MOVE] = ACTIONS(91),
    [anon_sym_PATH] = ACTIONS(91),
    [anon_sym_PROMPT] = ACTIONS(91),
    [anon_sym_RD] = ACTIONS(91),
    [anon_sym_REN] = ACTIONS(91),
    [anon_sym_START] = ACTIONS(91),
    [anon_sym_TIME] = ACTIONS(91),
    [anon_sym_TYPE] = ACTIONS(91),
    [anon_sym_VOL] = ACTIONS(91),
    [anon_sym_ATTRIB] = ACTIONS(91),
    [anon_sym_CHKDSK] = ACTIONS(91),
    [anon_sym_CHOICE] = ACTIONS(91),
    [anon_sym_CMD] = ACTIONS(91),
    [anon_sym_COMP] = ACTIONS(91),
    [anon_sym_CONVERT] = ACTIONS(91),
    [anon_sym_DRIVERQUERY] = ACTIONS(91),
    [anon_sym_EXPAND] = ACTIONS(91),
    [anon_sym_FIND] = ACTIONS(91),
    [anon_sym_FORMAT] = ACTIONS(91),
    [anon_sym_HELP] = ACTIONS(91),
    [anon_sym_IPCONFIG] = ACTIONS(91),
    [anon_sym_LABEL] = ACTIONS(91),
    [anon_sym_NET] = ACTIONS(91),
    [anon_sym_PING] = ACTIONS(91),
    [anon_sym_SHUTDOWN] = ACTIONS(91),
    [anon_sym_SORT] = ACTIONS(91),
    [anon_sym_SUBST] = ACTIONS(91),
    [anon_sym_SYSTEMINFO] = ACTIONS(91),
    [anon_sym_TASKKILL] = ACTIONS(91),
    [anon_sym_TASKLIST] = ACTIONS(91),
    [anon_sym_XCOPY] = ACTIONS(91),
    [anon_sym_TREE] = ACTIONS(91),
    [anon_sym_FC] = ACTIONS(91),
    [anon_sym_DISKPART] = ACTIONS(91),
    [anon_sym_TITLE] = ACTIONS(91),
    [anon_sym_ver] = ACTIONS(91),
    [anon_sym_assoc] = ACTIONS(91),
    [anon_sym_cd] = ACTIONS(91),
    [anon_sym_copy] = ACTIONS(91),
    [anon_sym_del] = ACTIONS(91),
    [anon_sym_dir] = ACTIONS(91),
    [anon_sym_date] = ACTIONS(91),
    [anon_sym_md] = ACTIONS(91),
    [anon_sym_move] = ACTIONS(91),
    [anon_sym_path] = ACTIONS(91),
    [anon_sym_prompt] = ACTIONS(91),
    [anon_sym_rd] = ACTIONS(91),
    [anon_sym_ren] = ACTIONS(91),
    [anon_sym_start] = ACTIONS(91),
    [anon_sym_time] = ACTIONS(91),
    [anon_sym_type] = ACTIONS(91),
    [anon_sym_vol] = ACTIONS(91),
    [anon_sym_attrib] = ACTIONS(91),
    [anon_sym_chkdsk] = ACTIONS(91),
    [anon_sym_choice] = ACTIONS(91),
    [anon_sym_cmd] = ACTIONS(91),
    [anon_sym_comp] = ACTIONS(91),
    [anon_sym_convert] = ACTIONS(91),
    [anon_sym_driverquery] = ACTIONS(91),
    [anon_sym_expand] = ACTIONS(91),
    [anon_sym_find] = ACTIONS(91),
    [anon_sym_format] = ACTIONS(91),
    [anon_sym_help] = ACTIONS(91),
    [anon_sym_ipconfig] = ACTIONS(91),
    [anon_sym_label] = ACTIONS(91),
    [anon_sym_net] = ACTIONS(91),
    [anon_sym_ping] = ACTIONS(91),
    [anon_sym_shutdown] = ACTIONS(91),
    [anon_sym_sort] = ACTIONS(91),
    [anon_sym_subst] = ACTIONS(91),
    [anon_sym_systeminfo] = ACTIONS(91),
    [anon_sym_taskkill] = ACTIONS(91),
    [anon_sym_tasklist] = ACTIONS(91),
    [anon_sym_xcopy] = ACTIONS(91),
    [anon_sym_tree] = ACTIONS(91),
    [anon_sym_fc] = ACTIONS(91),
    [anon_sym_diskpart] = ACTIONS(91),
    [anon_sym_title] = ACTIONS(91),
    [anon_sym_COLON] = ACTIONS(93),
  },
  [11] = {
    [ts_builtin_sym_end] = ACTIONS(95),
    [anon_sym_AT] = ACTIONS(95),
    [anon_sym_echooff] = ACTIONS(95),
    [anon_sym_COLON_COLON] = ACTIONS(95),
    [anon_sym_REM] = ACTIONS(95),
    [anon_sym_Rem] = ACTIONS(95),
    [anon_sym_rem] = ACTIONS(95),
    [anon_sym_SET] = ACTIONS(95),
    [anon_sym_Set] = ACTIONS(95),
    [anon_sym_set] = ACTIONS(95),
    [anon_sym_PERCENT] = ACTIONS(95),
    [anon_sym_ECHO] = ACTIONS(95),
    [anon_sym_IF] = ACTIONS(95),
    [anon_sym_GOTO] = ACTIONS(95),
    [anon_sym_EXIT] = ACTIONS(95),
    [anon_sym_FOR] = ACTIONS(97),
    [anon_sym_PAUSE] = ACTIONS(95),
    [anon_sym_CLS] = ACTIONS(95),
    [anon_sym_echo] = ACTIONS(97),
    [anon_sym_if] = ACTIONS(95),
    [anon_sym_goto] = ACTIONS(95),
    [anon_sym_exit] = ACTIONS(95),
    [anon_sym_for] = ACTIONS(97),
    [anon_sym_pause] = ACTIONS(95),
    [anon_sym_cls] = ACTIONS(95),
    [anon_sym_VER] = ACTIONS(95),
    [anon_sym_ASSOC] = ACTIONS(95),
    [anon_sym_CD] = ACTIONS(95),
    [anon_sym_COPY] = ACTIONS(95),
    [anon_sym_DEL] = ACTIONS(95),
    [anon_sym_DIR] = ACTIONS(95),
    [anon_sym_DATE] = ACTIONS(95),
    [anon_sym_MD] = ACTIONS(95),
    [anon_sym_MOVE] = ACTIONS(95),
    [anon_sym_PATH] = ACTIONS(95),
    [anon_sym_PROMPT] = ACTIONS(95),
    [anon_sym_RD] = ACTIONS(95),
    [anon_sym_REN] = ACTIONS(95),
    [anon_sym_START] = ACTIONS(95),
    [anon_sym_TIME] = ACTIONS(95),
    [anon_sym_TYPE] = ACTIONS(95),
    [anon_sym_VOL] = ACTIONS(95),
    [anon_sym_ATTRIB] = ACTIONS(95),
    [anon_sym_CHKDSK] = ACTIONS(95),
    [anon_sym_CHOICE] = ACTIONS(95),
    [anon_sym_CMD] = ACTIONS(95),
    [anon_sym_COMP] = ACTIONS(95),
    [anon_sym_CONVERT] = ACTIONS(95),
    [anon_sym_DRIVERQUERY] = ACTIONS(95),
    [anon_sym_EXPAND] = ACTIONS(95),
    [anon_sym_FIND] = ACTIONS(95),
    [anon_sym_FORMAT] = ACTIONS(95),
    [anon_sym_HELP] = ACTIONS(95),
    [anon_sym_IPCONFIG] = ACTIONS(95),
    [anon_sym_LABEL] = ACTIONS(95),
    [anon_sym_NET] = ACTIONS(95),
    [anon_sym_PING] = ACTIONS(95),
    [anon_sym_SHUTDOWN] = ACTIONS(95),
    [anon_sym_SORT] = ACTIONS(95),
    [anon_sym_SUBST] = ACTIONS(95),
    [anon_sym_SYSTEMINFO] = ACTIONS(95),
    [anon_sym_TASKKILL] = ACTIONS(95),
    [anon_sym_TASKLIST] = ACTIONS(95),
    [anon_sym_XCOPY] = ACTIONS(95),
    [anon_sym_TREE] = ACTIONS(95),
    [anon_sym_FC] = ACTIONS(95),
    [anon_sym_DISKPART] = ACTIONS(95),
    [anon_sym_TITLE] = ACTIONS(95),
    [anon_sym_ver] = ACTIONS(95),
    [anon_sym_assoc] = ACTIONS(95),
    [anon_sym_cd] = ACTIONS(95),
    [anon_sym_copy] = ACTIONS(95),
    [anon_sym_del] = ACTIONS(95),
    [anon_sym_dir] = ACTIONS(95),
    [anon_sym_date] = ACTIONS(95),
    [anon_sym_md] = ACTIONS(95),
    [anon_sym_move] = ACTIONS(95),
    [anon_sym_path] = ACTIONS(95),
    [anon_sym_prompt] = ACTIONS(95),
    [anon_sym_rd] = ACTIONS(95),
    [anon_sym_ren] = ACTIONS(95),
    [anon_sym_start] = ACTIONS(95),
    [anon_sym_time] = ACTIONS(95),
    [anon_sym_type] = ACTIONS(95),
    [anon_sym_vol] = ACTIONS(95),
    [anon_sym_attrib] = ACTIONS(95),
    [anon_sym_chkdsk] = ACTIONS(95),
    [anon_sym_choice] = ACTIONS(95),
    [anon_sym_cmd] = ACTIONS(95),
    [anon_sym_comp] = ACTIONS(95),
    [anon_sym_convert] = ACTIONS(95),
    [anon_sym_driverquery] = ACTIONS(95),
    [anon_sym_expand] = ACTIONS(95),
    [anon_sym_find] = ACTIONS(95),
    [anon_sym_format] = ACTIONS(95),
    [anon_sym_help] = ACTIONS(95),
    [anon_sym_ipconfig] = ACTIONS(95),
    [anon_sym_label] = ACTIONS(95),
    [anon_sym_net] = ACTIONS(95),
    [anon_sym_ping] = ACTIONS(95),
    [anon_sym_shutdown] = ACTIONS(95),
    [anon_sym_sort] = ACTIONS(95),
    [anon_sym_subst] = ACTIONS(95),
    [anon_sym_systeminfo] = ACTIONS(95),
    [anon_sym_taskkill] = ACTIONS(95),
    [anon_sym_tasklist] = ACTIONS(95),
    [anon_sym_xcopy] = ACTIONS(95),
    [anon_sym_tree] = ACTIONS(95),
    [anon_sym_fc] = ACTIONS(95),
    [anon_sym_diskpart] = ACTIONS(95),
    [anon_sym_title] = ACTIONS(95),
    [anon_sym_COLON] = ACTIONS(97),
  },
  [12] = {
    [ts_builtin_sym_end] = ACTIONS(99),
    [anon_sym_AT] = ACTIONS(99),
    [anon_sym_echooff] = ACTIONS(99),
    [anon_sym_COLON_COLON] = ACTIONS(99),
    [anon_sym_REM] = ACTIONS(99),
    [anon_sym_Rem] = ACTIONS(99),
    [anon_sym_rem] = ACTIONS(99),
    [anon_sym_SET] = ACTIONS(99),
    [anon_sym_Set] = ACTIONS(99),
    [anon_sym_set] = ACTIONS(99),
    [anon_sym_PERCENT] = ACTIONS(99),
    [anon_sym_ECHO] = ACTIONS(99),
    [anon_sym_IF] = ACTIONS(99),
    [anon_sym_GOTO] = ACTIONS(99),
    [anon_sym_EXIT] = ACTIONS(99),
    [anon_sym_FOR] = ACTIONS(101),
    [anon_sym_PAUSE] = ACTIONS(99),
    [anon_sym_CLS] = ACTIONS(99),
    [anon_sym_echo] = ACTIONS(101),
    [anon_sym_if] = ACTIONS(99),
    [anon_sym_goto] = ACTIONS(99),
    [anon_sym_exit] = ACTIONS(99),
    [anon_sym_for] = ACTIONS(101),
    [anon_sym_pause] = ACTIONS(99),
    [anon_sym_cls] = ACTIONS(99),
    [anon_sym_VER] = ACTIONS(99),
    [anon_sym_ASSOC] = ACTIONS(99),
    [anon_sym_CD] = ACTIONS(99),
    [anon_sym_COPY] = ACTIONS(99),
    [anon_sym_DEL] = ACTIONS(99),
    [anon_sym_DIR] = ACTIONS(99),
    [anon_sym_DATE] = ACTIONS(99),
    [anon_sym_MD] = ACTIONS(99),
    [anon_sym_MOVE] = ACTIONS(99),
    [anon_sym_PATH] = ACTIONS(99),
    [anon_sym_PROMPT] = ACTIONS(99),
    [anon_sym_RD] = ACTIONS(99),
    [anon_sym_REN] = ACTIONS(99),
    [anon_sym_START] = ACTIONS(99),
    [anon_sym_TIME] = ACTIONS(99),
    [anon_sym_TYPE] = ACTIONS(99),
    [anon_sym_VOL] = ACTIONS(99),
    [anon_sym_ATTRIB] = ACTIONS(99),
    [anon_sym_CHKDSK] = ACTIONS(99),
    [anon_sym_CHOICE] = ACTIONS(99),
    [anon_sym_CMD] = ACTIONS(99),
    [anon_sym_COMP] = ACTIONS(99),
    [anon_sym_CONVERT] = ACTIONS(99),
    [anon_sym_DRIVERQUERY] = ACTIONS(99),
    [anon_sym_EXPAND] = ACTIONS(99),
    [anon_sym_FIND] = ACTIONS(99),
    [anon_sym_FORMAT] = ACTIONS(99),
    [anon_sym_HELP] = ACTIONS(99),
    [anon_sym_IPCONFIG] = ACTIONS(99),
    [anon_sym_LABEL] = ACTIONS(99),
    [anon_sym_NET] = ACTIONS(99),
    [anon_sym_PING] = ACTIONS(99),
    [anon_sym_SHUTDOWN] = ACTIONS(99),
    [anon_sym_SORT] = ACTIONS(99),
    [anon_sym_SUBST] = ACTIONS(99),
    [anon_sym_SYSTEMINFO] = ACTIONS(99),
    [anon_sym_TASKKILL] = ACTIONS(99),
    [anon_sym_TASKLIST] = ACTIONS(99),
    [anon_sym_XCOPY] = ACTIONS(99),
    [anon_sym_TREE] = ACTIONS(99),
    [anon_sym_FC] = ACTIONS(99),
    [anon_sym_DISKPART] = ACTIONS(99),
    [anon_sym_TITLE] = ACTIONS(99),
    [anon_sym_ver] = ACTIONS(99),
    [anon_sym_assoc] = ACTIONS(99),
    [anon_sym_cd] = ACTIONS(99),
    [anon_sym_copy] = ACTIONS(99),
    [anon_sym_del] = ACTIONS(99),
    [anon_sym_dir] = ACTIONS(99),
    [anon_sym_date] = ACTIONS(99),
    [anon_sym_md] = ACTIONS(99),
    [anon_sym_move] = ACTIONS(99),
    [anon_sym_path] = ACTIONS(99),
    [anon_sym_prompt] = ACTIONS(99),
    [anon_sym_rd] = ACTIONS(99),
    [anon_sym_ren] = ACTIONS(99),
    [anon_sym_start] = ACTIONS(99),
    [anon_sym_time] = ACTIONS(99),
    [anon_sym_type] = ACTIONS(99),
    [anon_sym_vol] = ACTIONS(99),
    [anon_sym_attrib] = ACTIONS(99),
    [anon_sym_chkdsk] = ACTIONS(99),
    [anon_sym_choice] = ACTIONS(99),
    [anon_sym_cmd] = ACTIONS(99),
    [anon_sym_comp] = ACTIONS(99),
    [anon_sym_convert] = ACTIONS(99),
    [anon_sym_driverquery] = ACTIONS(99),
    [anon_sym_expand] = ACTIONS(99),
    [anon_sym_find] = ACTIONS(99),
    [anon_sym_format] = ACTIONS(99),
    [anon_sym_help] = ACTIONS(99),
    [anon_sym_ipconfig] = ACTIONS(99),
    [anon_sym_label] = ACTIONS(99),
    [anon_sym_net] = ACTIONS(99),
    [anon_sym_ping] = ACTIONS(99),
    [anon_sym_shutdown] = ACTIONS(99),
    [anon_sym_sort] = ACTIONS(99),
    [anon_sym_subst] = ACTIONS(99),
    [anon_sym_systeminfo] = ACTIONS(99),
    [anon_sym_taskkill] = ACTIONS(99),
    [anon_sym_tasklist] = ACTIONS(99),
    [anon_sym_xcopy] = ACTIONS(99),
    [anon_sym_tree] = ACTIONS(99),
    [anon_sym_fc] = ACTIONS(99),
    [anon_sym_diskpart] = ACTIONS(99),
    [anon_sym_title] = ACTIONS(99),
    [anon_sym_COLON] = ACTIONS(101),
  },
  [13] = {
    [ts_builtin_sym_end] = ACTIONS(103),
    [anon_sym_AT] = ACTIONS(103),
    [anon_sym_echooff] = ACTIONS(103),
    [anon_sym_COLON_COLON] = ACTIONS(103),
    [anon_sym_REM] = ACTIONS(103),
    [anon_sym_Rem] = ACTIONS(103),
    [anon_sym_rem] = ACTIONS(103),
    [anon_sym_SET] = ACTIONS(103),
    [anon_sym_Set] = ACTIONS(103),
    [anon_sym_set] = ACTIONS(103),
    [anon_sym_PERCENT] = ACTIONS(103),
    [anon_sym_ECHO] = ACTIONS(103),
    [anon_sym_IF] = ACTIONS(103),
    [anon_sym_GOTO] = ACTIONS(103),
    [anon_sym_EXIT] = ACTIONS(103),
    [anon_sym_FOR] = ACTIONS(105),
    [anon_sym_PAUSE] = ACTIONS(103),
    [anon_sym_CLS] = ACTIONS(103),
    [anon_sym_echo] = ACTIONS(105),
    [anon_sym_if] = ACTIONS(103),
    [anon_sym_goto] = ACTIONS(103),
    [anon_sym_exit] = ACTIONS(103),
    [anon_sym_for] = ACTIONS(105),
    [anon_sym_pause] = ACTIONS(103),
    [anon_sym_cls] = ACTIONS(103),
    [anon_sym_VER] = ACTIONS(103),
    [anon_sym_ASSOC] = ACTIONS(103),
    [anon_sym_CD] = ACTIONS(103),
    [anon_sym_COPY] = ACTIONS(103),
    [anon_sym_DEL] = ACTIONS(103),
    [anon_sym_DIR] = ACTIONS(103),
    [anon_sym_DATE] = ACTIONS(103),
    [anon_sym_MD] = ACTIONS(103),
    [anon_sym_MOVE] = ACTIONS(103),
    [anon_sym_PATH] = ACTIONS(103),
    [anon_sym_PROMPT] = ACTIONS(103),
    [anon_sym_RD] = ACTIONS(103),
    [anon_sym_REN] = ACTIONS(103),
    [anon_sym_START] = ACTIONS(103),
    [anon_sym_TIME] = ACTIONS(103),
    [anon_sym_TYPE] = ACTIONS(103),
    [anon_sym_VOL] = ACTIONS(103),
    [anon_sym_ATTRIB] = ACTIONS(103),
    [anon_sym_CHKDSK] = ACTIONS(103),
    [anon_sym_CHOICE] = ACTIONS(103),
    [anon_sym_CMD] = ACTIONS(103),
    [anon_sym_COMP] = ACTIONS(103),
    [anon_sym_CONVERT] = ACTIONS(103),
    [anon_sym_DRIVERQUERY] = ACTIONS(103),
    [anon_sym_EXPAND] = ACTIONS(103),
    [anon_sym_FIND] = ACTIONS(103),
    [anon_sym_FORMAT] = ACTIONS(103),
    [anon_sym_HELP] = ACTIONS(103),
    [anon_sym_IPCONFIG] = ACTIONS(103),
    [anon_sym_LABEL] = ACTIONS(103),
    [anon_sym_NET] = ACTIONS(103),
    [anon_sym_PING] = ACTIONS(103),
    [anon_sym_SHUTDOWN] = ACTIONS(103),
    [anon_sym_SORT] = ACTIONS(103),
    [anon_sym_SUBST] = ACTIONS(103),
    [anon_sym_SYSTEMINFO] = ACTIONS(103),
    [anon_sym_TASKKILL] = ACTIONS(103),
    [anon_sym_TASKLIST] = ACTIONS(103),
    [anon_sym_XCOPY] = ACTIONS(103),
    [anon_sym_TREE] = ACTIONS(103),
    [anon_sym_FC] = ACTIONS(103),
    [anon_sym_DISKPART] = ACTIONS(103),
    [anon_sym_TITLE] = ACTIONS(103),
    [anon_sym_ver] = ACTIONS(103),
    [anon_sym_assoc] = ACTIONS(103),
    [anon_sym_cd] = ACTIONS(103),
    [anon_sym_copy] = ACTIONS(103),
    [anon_sym_del] = ACTIONS(103),
    [anon_sym_dir] = ACTIONS(103),
    [anon_sym_date] = ACTIONS(103),
    [anon_sym_md] = ACTIONS(103),
    [anon_sym_move] = ACTIONS(103),
    [anon_sym_path] = ACTIONS(103),
    [anon_sym_prompt] = ACTIONS(103),
    [anon_sym_rd] = ACTIONS(103),
    [anon_sym_ren] = ACTIONS(103),
    [anon_sym_start] = ACTIONS(103),
    [anon_sym_time] = ACTIONS(103),
    [anon_sym_type] = ACTIONS(103),
    [anon_sym_vol] = ACTIONS(103),
    [anon_sym_attrib] = ACTIONS(103),
    [anon_sym_chkdsk] = ACTIONS(103),
    [anon_sym_choice] = ACTIONS(103),
    [anon_sym_cmd] = ACTIONS(103),
    [anon_sym_comp] = ACTIONS(103),
    [anon_sym_convert] = ACTIONS(103),
    [anon_sym_driverquery] = ACTIONS(103),
    [anon_sym_expand] = ACTIONS(103),
    [anon_sym_find] = ACTIONS(103),
    [anon_sym_format] = ACTIONS(103),
    [anon_sym_help] = ACTIONS(103),
    [anon_sym_ipconfig] = ACTIONS(103),
    [anon_sym_label] = ACTIONS(103),
    [anon_sym_net] = ACTIONS(103),
    [anon_sym_ping] = ACTIONS(103),
    [anon_sym_shutdown] = ACTIONS(103),
    [anon_sym_sort] = ACTIONS(103),
    [anon_sym_subst] = ACTIONS(103),
    [anon_sym_systeminfo] = ACTIONS(103),
    [anon_sym_taskkill] = ACTIONS(103),
    [anon_sym_tasklist] = ACTIONS(103),
    [anon_sym_xcopy] = ACTIONS(103),
    [anon_sym_tree] = ACTIONS(103),
    [anon_sym_fc] = ACTIONS(103),
    [anon_sym_diskpart] = ACTIONS(103),
    [anon_sym_title] = ACTIONS(103),
    [anon_sym_COLON] = ACTIONS(105),
  },
  [14] = {
    [ts_builtin_sym_end] = ACTIONS(59),
    [anon_sym_AT] = ACTIONS(59),
    [anon_sym_echooff] = ACTIONS(59),
    [anon_sym_COLON_COLON] = ACTIONS(59),
    [anon_sym_REM] = ACTIONS(59),
    [anon_sym_Rem] = ACTIONS(59),
    [anon_sym_rem] = ACTIONS(59),
    [anon_sym_SET] = ACTIONS(59),
    [anon_sym_Set] = ACTIONS(59),
    [anon_sym_set] = ACTIONS(59),
    [anon_sym_PERCENT] = ACTIONS(59),
    [anon_sym_ECHO] = ACTIONS(59),
    [anon_sym_IF] = ACTIONS(59),
    [anon_sym_GOTO] = ACTIONS(59),
    [anon_sym_EXIT] = ACTIONS(59),
    [anon_sym_FOR] = ACTIONS(61),
    [anon_sym_PAUSE] = ACTIONS(59),
    [anon_sym_CLS] = ACTIONS(59),
    [anon_sym_echo] = ACTIONS(61),
    [anon_sym_if] = ACTIONS(59),
    [anon_sym_goto] = ACTIONS(59),
    [anon_sym_exit] = ACTIONS(59),
    [anon_sym_for] = ACTIONS(61),
    [anon_sym_pause] = ACTIONS(59),
    [anon_sym_cls] = ACTIONS(59),
    [anon_sym_VER] = ACTIONS(59),
    [anon_sym_ASSOC] = ACTIONS(59),
    [anon_sym_CD] = ACTIONS(59),
    [anon_sym_COPY] = ACTIONS(59),
    [anon_sym_DEL] = ACTIONS(59),
    [anon_sym_DIR] = ACTIONS(59),
    [anon_sym_DATE] = ACTIONS(59),
    [anon_sym_MD] = ACTIONS(59),
    [anon_sym_MOVE] = ACTIONS(59),
    [anon_sym_PATH] = ACTIONS(59),
    [anon_sym_PROMPT] = ACTIONS(59),
    [anon_sym_RD] = ACTIONS(59),
    [anon_sym_REN] = ACTIONS(59),
    [anon_sym_START] = ACTIONS(59),
    [anon_sym_TIME] = ACTIONS(59),
    [anon_sym_TYPE] = ACTIONS(59),
    [anon_sym_VOL] = ACTIONS(59),
    [anon_sym_ATTRIB] = ACTIONS(59),
    [anon_sym_CHKDSK] = ACTIONS(59),
    [anon_sym_CHOICE] = ACTIONS(59),
    [anon_sym_CMD] = ACTIONS(59),
    [anon_sym_COMP] = ACTIONS(59),
    [anon_sym_CONVERT] = ACTIONS(59),
    [anon_sym_DRIVERQUERY] = ACTIONS(59),
    [anon_sym_EXPAND] = ACTIONS(59),
    [anon_sym_FIND] = ACTIONS(59),
    [anon_sym_FORMAT] = ACTIONS(59),
    [anon_sym_HELP] = ACTIONS(59),
    [anon_sym_IPCONFIG] = ACTIONS(59),
    [anon_sym_LABEL] = ACTIONS(59),
    [anon_sym_NET] = ACTIONS(59),
    [anon_sym_PING] = ACTIONS(59),
    [anon_sym_SHUTDOWN] = ACTIONS(59),
    [anon_sym_SORT] = ACTIONS(59),
    [anon_sym_SUBST] = ACTIONS(59),
    [anon_sym_SYSTEMINFO] = ACTIONS(59),
    [anon_sym_TASKKILL] = ACTIONS(59),
    [anon_sym_TASKLIST] = ACTIONS(59),
    [anon_sym_XCOPY] = ACTIONS(59),
    [anon_sym_TREE] = ACTIONS(59),
    [anon_sym_FC] = ACTIONS(59),
    [anon_sym_DISKPART] = ACTIONS(59),
    [anon_sym_TITLE] = ACTIONS(59),
    [anon_sym_ver] = ACTIONS(59),
    [anon_sym_assoc] = ACTIONS(59),
    [anon_sym_cd] = ACTIONS(59),
    [anon_sym_copy] = ACTIONS(59),
    [anon_sym_del] = ACTIONS(59),
    [anon_sym_dir] = ACTIONS(59),
    [anon_sym_date] = ACTIONS(59),
    [anon_sym_md] = ACTIONS(59),
    [anon_sym_move] = ACTIONS(59),
    [anon_sym_path] = ACTIONS(59),
    [anon_sym_prompt] = ACTIONS(59),
    [anon_sym_rd] = ACTIONS(59),
    [anon_sym_ren] = ACTIONS(59),
    [anon_sym_start] = ACTIONS(59),
    [anon_sym_time] = ACTIONS(59),
    [anon_sym_type] = ACTIONS(59),
    [anon_sym_vol] = ACTIONS(59),
    [anon_sym_attrib] = ACTIONS(59),
    [anon_sym_chkdsk] = ACTIONS(59),
    [anon_sym_choice] = ACTIONS(59),
    [anon_sym_cmd] = ACTIONS(59),
    [anon_sym_comp] = ACTIONS(59),
    [anon_sym_convert] = ACTIONS(59),
    [anon_sym_driverquery] = ACTIONS(59),
    [anon_sym_expand] = ACTIONS(59),
    [anon_sym_find] = ACTIONS(59),
    [anon_sym_format] = ACTIONS(59),
    [anon_sym_help] = ACTIONS(59),
    [anon_sym_ipconfig] = ACTIONS(59),
    [anon_sym_label] = ACTIONS(59),
    [anon_sym_net] = ACTIONS(59),
    [anon_sym_ping] = ACTIONS(59),
    [anon_sym_shutdown] = ACTIONS(59),
    [anon_sym_sort] = ACTIONS(59),
    [anon_sym_subst] = ACTIONS(59),
    [anon_sym_systeminfo] = ACTIONS(59),
    [anon_sym_taskkill] = ACTIONS(59),
    [anon_sym_tasklist] = ACTIONS(59),
    [anon_sym_xcopy] = ACTIONS(59),
    [anon_sym_tree] = ACTIONS(59),
    [anon_sym_fc] = ACTIONS(59),
    [anon_sym_diskpart] = ACTIONS(59),
    [anon_sym_title] = ACTIONS(59),
    [anon_sym_COLON] = ACTIONS(61),
  },
  [15] = {
    [ts_builtin_sym_end] = ACTIONS(107),
    [anon_sym_AT] = ACTIONS(107),
    [anon_sym_echooff] = ACTIONS(107),
    [anon_sym_COLON_COLON] = ACTIONS(107),
    [anon_sym_REM] = ACTIONS(107),
    [anon_sym_Rem] = ACTIONS(107),
    [anon_sym_rem] = ACTIONS(107),
    [anon_sym_SET] = ACTIONS(107),
    [anon_sym_Set] = ACTIONS(107),
    [anon_sym_set] = ACTIONS(107),
    [anon_sym_PERCENT] = ACTIONS(107),
    [anon_sym_ECHO] = ACTIONS(107),
    [anon_sym_IF] = ACTIONS(107),
    [anon_sym_GOTO] = ACTIONS(107),
    [anon_sym_EXIT] = ACTIONS(107),
    [anon_sym_FOR] = ACTIONS(109),
    [anon_sym_PAUSE] = ACTIONS(107),
    [anon_sym_CLS] = ACTIONS(107),
    [anon_sym_echo] = ACTIONS(109),
    [anon_sym_if] = ACTIONS(107),
    [anon_sym_goto] = ACTIONS(107),
    [anon_sym_exit] = ACTIONS(107),
    [anon_sym_for] = ACTIONS(109),
    [anon_sym_pause] = ACTIONS(107),
    [anon_sym_cls] = ACTIONS(107),
    [anon_sym_VER] = ACTIONS(107),
    [anon_sym_ASSOC] = ACTIONS(107),
    [anon_sym_CD] = ACTIONS(107),
    [anon_sym_COPY] = ACTIONS(107),
    [anon_sym_DEL] = ACTIONS(107),
    [anon_sym_DIR] = ACTIONS(107),
    [anon_sym_DATE] = ACTIONS(107),
    [anon_sym_MD] = ACTIONS(107),
    [anon_sym_MOVE] = ACTIONS(107),
    [anon_sym_PATH] = ACTIONS(107),
    [anon_sym_PROMPT] = ACTIONS(107),
    [anon_sym_RD] = ACTIONS(107),
    [anon_sym_REN] = ACTIONS(107),
    [anon_sym_START] = ACTIONS(107),
    [anon_sym_TIME] = ACTIONS(107),
    [anon_sym_TYPE] = ACTIONS(107),
    [anon_sym_VOL] = ACTIONS(107),
    [anon_sym_ATTRIB] = ACTIONS(107),
    [anon_sym_CHKDSK] = ACTIONS(107),
    [anon_sym_CHOICE] = ACTIONS(107),
    [anon_sym_CMD] = ACTIONS(107),
    [anon_sym_COMP] = ACTIONS(107),
    [anon_sym_CONVERT] = ACTIONS(107),
    [anon_sym_DRIVERQUERY] = ACTIONS(107),
    [anon_sym_EXPAND] = ACTIONS(107),
    [anon_sym_FIND] = ACTIONS(107),
    [anon_sym_FORMAT] = ACTIONS(107),
    [anon_sym_HELP] = ACTIONS(107),
    [anon_sym_IPCONFIG] = ACTIONS(107),
    [anon_sym_LABEL] = ACTIONS(107),
    [anon_sym_NET] = ACTIONS(107),
    [anon_sym_PING] = ACTIONS(107),
    [anon_sym_SHUTDOWN] = ACTIONS(107),
    [anon_sym_SORT] = ACTIONS(107),
    [anon_sym_SUBST] = ACTIONS(107),
    [anon_sym_SYSTEMINFO] = ACTIONS(107),
    [anon_sym_TASKKILL] = ACTIONS(107),
    [anon_sym_TASKLIST] = ACTIONS(107),
    [anon_sym_XCOPY] = ACTIONS(107),
    [anon_sym_TREE] = ACTIONS(107),
    [anon_sym_FC] = ACTIONS(107),
    [anon_sym_DISKPART] = ACTIONS(107),
    [anon_sym_TITLE] = ACTIONS(107),
    [anon_sym_ver] = ACTIONS(107),
    [anon_sym_assoc] = ACTIONS(107),
    [anon_sym_cd] = ACTIONS(107),
    [anon_sym_copy] = ACTIONS(107),
    [anon_sym_del] = ACTIONS(107),
    [anon_sym_dir] = ACTIONS(107),
    [anon_sym_date] = ACTIONS(107),
    [anon_sym_md] = ACTIONS(107),
    [anon_sym_move] = ACTIONS(107),
    [anon_sym_path] = ACTIONS(107),
    [anon_sym_prompt] = ACTIONS(107),
    [anon_sym_rd] = ACTIONS(107),
    [anon_sym_ren] = ACTIONS(107),
    [anon_sym_start] = ACTIONS(107),
    [anon_sym_time] = ACTIONS(107),
    [anon_sym_type] = ACTIONS(107),
    [anon_sym_vol] = ACTIONS(107),
    [anon_sym_attrib] = ACTIONS(107),
    [anon_sym_chkdsk] = ACTIONS(107),
    [anon_sym_choice] = ACTIONS(107),
    [anon_sym_cmd] = ACTIONS(107),
    [anon_sym_comp] = ACTIONS(107),
    [anon_sym_convert] = ACTIONS(107),
    [anon_sym_driverquery] = ACTIONS(107),
    [anon_sym_expand] = ACTIONS(107),
    [anon_sym_find] = ACTIONS(107),
    [anon_sym_format] = ACTIONS(107),
    [anon_sym_help] = ACTIONS(107),
    [anon_sym_ipconfig] = ACTIONS(107),
    [anon_sym_label] = ACTIONS(107),
    [anon_sym_net] = ACTIONS(107),
    [anon_sym_ping] = ACTIONS(107),
    [anon_sym_shutdown] = ACTIONS(107),
    [anon_sym_sort] = ACTIONS(107),
    [anon_sym_subst] = ACTIONS(107),
    [anon_sym_systeminfo] = ACTIONS(107),
    [anon_sym_taskkill] = ACTIONS(107),
    [anon_sym_tasklist] = ACTIONS(107),
    [anon_sym_xcopy] = ACTIONS(107),
    [anon_sym_tree] = ACTIONS(107),
    [anon_sym_fc] = ACTIONS(107),
    [anon_sym_diskpart] = ACTIONS(107),
    [anon_sym_title] = ACTIONS(107),
    [anon_sym_COLON] = ACTIONS(109),
  },
  [16] = {
    [ts_builtin_sym_end] = ACTIONS(111),
    [anon_sym_AT] = ACTIONS(111),
    [anon_sym_echooff] = ACTIONS(111),
    [anon_sym_COLON_COLON] = ACTIONS(111),
    [anon_sym_REM] = ACTIONS(111),
    [anon_sym_Rem] = ACTIONS(111),
    [anon_sym_rem] = ACTIONS(111),
    [anon_sym_SET] = ACTIONS(111),
    [anon_sym_Set] = ACTIONS(111),
    [anon_sym_set] = ACTIONS(111),
    [anon_sym_PERCENT] = ACTIONS(111),
    [anon_sym_ECHO] = ACTIONS(111),
    [anon_sym_IF] = ACTIONS(111),
    [anon_sym_GOTO] = ACTIONS(111),
    [anon_sym_EXIT] = ACTIONS(111),
    [anon_sym_FOR] = ACTIONS(113),
    [anon_sym_PAUSE] = ACTIONS(111),
    [anon_sym_CLS] = ACTIONS(111),
    [anon_sym_echo] = ACTIONS(113),
    [anon_sym_if] = ACTIONS(111),
    [anon_sym_goto] = ACTIONS(111),
    [anon_sym_exit] = ACTIONS(111),
    [anon_sym_for] = ACTIONS(113),
    [anon_sym_pause] = ACTIONS(111),
    [anon_sym_cls] = ACTIONS(111),
    [anon_sym_VER] = ACTIONS(111),
    [anon_sym_ASSOC] = ACTIONS(111),
    [anon_sym_CD] = ACTIONS(111),
    [anon_sym_COPY] = ACTIONS(111),
    [anon_sym_DEL] = ACTIONS(111),
    [anon_sym_DIR] = ACTIONS(111),
    [anon_sym_DATE] = ACTIONS(111),
    [anon_sym_MD] = ACTIONS(111),
    [anon_sym_MOVE] = ACTIONS(111),
    [anon_sym_PATH] = ACTIONS(111),
    [anon_sym_PROMPT] = ACTIONS(111),
    [anon_sym_RD] = ACTIONS(111),
    [anon_sym_REN] = ACTIONS(111),
    [anon_sym_START] = ACTIONS(111),
    [anon_sym_TIME] = ACTIONS(111),
    [anon_sym_TYPE] = ACTIONS(111),
    [anon_sym_VOL] = ACTIONS(111),
    [anon_sym_ATTRIB] = ACTIONS(111),
    [anon_sym_CHKDSK] = ACTIONS(111),
    [anon_sym_CHOICE] = ACTIONS(111),
    [anon_sym_CMD] = ACTIONS(111),
    [anon_sym_COMP] = ACTIONS(111),
    [anon_sym_CONVERT] = ACTIONS(111),
    [anon_sym_DRIVERQUERY] = ACTIONS(111),
    [anon_sym_EXPAND] = ACTIONS(111),
    [anon_sym_FIND] = ACTIONS(111),
    [anon_sym_FORMAT] = ACTIONS(111),
    [anon_sym_HELP] = ACTIONS(111),
    [anon_sym_IPCONFIG] = ACTIONS(111),
    [anon_sym_LABEL] = ACTIONS(111),
    [anon_sym_NET] = ACTIONS(111),
    [anon_sym_PING] = ACTIONS(111),
    [anon_sym_SHUTDOWN] = ACTIONS(111),
    [anon_sym_SORT] = ACTIONS(111),
    [anon_sym_SUBST] = ACTIONS(111),
    [anon_sym_SYSTEMINFO] = ACTIONS(111),
    [anon_sym_TASKKILL] = ACTIONS(111),
    [anon_sym_TASKLIST] = ACTIONS(111),
    [anon_sym_XCOPY] = ACTIONS(111),
    [anon_sym_TREE] = ACTIONS(111),
    [anon_sym_FC] = ACTIONS(111),
    [anon_sym_DISKPART] = ACTIONS(111),
    [anon_sym_TITLE] = ACTIONS(111),
    [anon_sym_ver] = ACTIONS(111),
    [anon_sym_assoc] = ACTIONS(111),
    [anon_sym_cd] = ACTIONS(111),
    [anon_sym_copy] = ACTIONS(111),
    [anon_sym_del] = ACTIONS(111),
    [anon_sym_dir] = ACTIONS(111),
    [anon_sym_date] = ACTIONS(111),
    [anon_sym_md] = ACTIONS(111),
    [anon_sym_move] = ACTIONS(111),
    [anon_sym_path] = ACTIONS(111),
    [anon_sym_prompt] = ACTIONS(111),
    [anon_sym_rd] = ACTIONS(111),
    [anon_sym_ren] = ACTIONS(111),
    [anon_sym_start] = ACTIONS(111),
    [anon_sym_time] = ACTIONS(111),
    [anon_sym_type] = ACTIONS(111),
    [anon_sym_vol] = ACTIONS(111),
    [anon_sym_attrib] = ACTIONS(111),
    [anon_sym_chkdsk] = ACTIONS(111),
    [anon_sym_choice] = ACTIONS(111),
    [anon_sym_cmd] = ACTIONS(111),
    [anon_sym_comp] = ACTIONS(111),
    [anon_sym_convert] = ACTIONS(111),
    [anon_sym_driverquery] = ACTIONS(111),
    [anon_sym_expand] = ACTIONS(111),
    [anon_sym_find] = ACTIONS(111),
    [anon_sym_format] = ACTIONS(111),
    [anon_sym_help] = ACTIONS(111),
    [anon_sym_ipconfig] = ACTIONS(111),
    [anon_sym_label] = ACTIONS(111),
    [anon_sym_net] = ACTIONS(111),
    [anon_sym_ping] = ACTIONS(111),
    [anon_sym_shutdown] = ACTIONS(111),
    [anon_sym_sort] = ACTIONS(111),
    [anon_sym_subst] = ACTIONS(111),
    [anon_sym_systeminfo] = ACTIONS(111),
    [anon_sym_taskkill] = ACTIONS(111),
    [anon_sym_tasklist] = ACTIONS(111),
    [anon_sym_xcopy] = ACTIONS(111),
    [anon_sym_tree] = ACTIONS(111),
    [anon_sym_fc] = ACTIONS(111),
    [anon_sym_diskpart] = ACTIONS(111),
    [anon_sym_title] = ACTIONS(111),
    [anon_sym_COLON] = ACTIONS(113),
  },
  [17] = {
    [ts_builtin_sym_end] = ACTIONS(115),
    [anon_sym_AT] = ACTIONS(115),
    [anon_sym_echooff] = ACTIONS(115),
    [anon_sym_COLON_COLON] = ACTIONS(115),
    [anon_sym_REM] = ACTIONS(115),
    [anon_sym_Rem] = ACTIONS(115),
    [anon_sym_rem] = ACTIONS(115),
    [anon_sym_SET] = ACTIONS(115),
    [anon_sym_Set] = ACTIONS(115),
    [anon_sym_set] = ACTIONS(115),
    [anon_sym_PERCENT] = ACTIONS(115),
    [anon_sym_ECHO] = ACTIONS(115),
    [anon_sym_IF] = ACTIONS(115),
    [anon_sym_GOTO] = ACTIONS(115),
    [anon_sym_EXIT] = ACTIONS(115),
    [anon_sym_FOR] = ACTIONS(117),
    [anon_sym_PAUSE] = ACTIONS(115),
    [anon_sym_CLS] = ACTIONS(115),
    [anon_sym_echo] = ACTIONS(117),
    [anon_sym_if] = ACTIONS(115),
    [anon_sym_goto] = ACTIONS(115),
    [anon_sym_exit] = ACTIONS(115),
    [anon_sym_for] = ACTIONS(117),
    [anon_sym_pause] = ACTIONS(115),
    [anon_sym_cls] = ACTIONS(115),
    [anon_sym_VER] = ACTIONS(115),
    [anon_sym_ASSOC] = ACTIONS(115),
    [anon_sym_CD] = ACTIONS(115),
    [anon_sym_COPY] = ACTIONS(115),
    [anon_sym_DEL] = ACTIONS(115),
    [anon_sym_DIR] = ACTIONS(115),
    [anon_sym_DATE] = ACTIONS(115),
    [anon_sym_MD] = ACTIONS(115),
    [anon_sym_MOVE] = ACTIONS(115),
    [anon_sym_PATH] = ACTIONS(115),
    [anon_sym_PROMPT] = ACTIONS(115),
    [anon_sym_RD] = ACTIONS(115),
    [anon_sym_REN] = ACTIONS(115),
    [anon_sym_START] = ACTIONS(115),
    [anon_sym_TIME] = ACTIONS(115),
    [anon_sym_TYPE] = ACTIONS(115),
    [anon_sym_VOL] = ACTIONS(115),
    [anon_sym_ATTRIB] = ACTIONS(115),
    [anon_sym_CHKDSK] = ACTIONS(115),
    [anon_sym_CHOICE] = ACTIONS(115),
    [anon_sym_CMD] = ACTIONS(115),
    [anon_sym_COMP] = ACTIONS(115),
    [anon_sym_CONVERT] = ACTIONS(115),
    [anon_sym_DRIVERQUERY] = ACTIONS(115),
    [anon_sym_EXPAND] = ACTIONS(115),
    [anon_sym_FIND] = ACTIONS(115),
    [anon_sym_FORMAT] = ACTIONS(115),
    [anon_sym_HELP] = ACTIONS(115),
    [anon_sym_IPCONFIG] = ACTIONS(115),
    [anon_sym_LABEL] = ACTIONS(115),
    [anon_sym_NET] = ACTIONS(115),
    [anon_sym_PING] = ACTIONS(115),
    [anon_sym_SHUTDOWN] = ACTIONS(115),
    [anon_sym_SORT] = ACTIONS(115),
    [anon_sym_SUBST] = ACTIONS(115),
    [anon_sym_SYSTEMINFO] = ACTIONS(115),
    [anon_sym_TASKKILL] = ACTIONS(115),
    [anon_sym_TASKLIST] = ACTIONS(115),
    [anon_sym_XCOPY] = ACTIONS(115),
    [anon_sym_TREE] = ACTIONS(115),
    [anon_sym_FC] = ACTIONS(115),
    [anon_sym_DISKPART] = ACTIONS(115),
    [anon_sym_TITLE] = ACTIONS(115),
    [anon_sym_ver] = ACTIONS(115),
    [anon_sym_assoc] = ACTIONS(115),
    [anon_sym_cd] = ACTIONS(115),
    [anon_sym_copy] = ACTIONS(115),
    [anon_sym_del] = ACTIONS(115),
    [anon_sym_dir] = ACTIONS(115),
    [anon_sym_date] = ACTIONS(115),
    [anon_sym_md] = ACTIONS(115),
    [anon_sym_move] = ACTIONS(115),
    [anon_sym_path] = ACTIONS(115),
    [anon_sym_prompt] = ACTIONS(115),
    [anon_sym_rd] = ACTIONS(115),
    [anon_sym_ren] = ACTIONS(115),
    [anon_sym_start] = ACTIONS(115),
    [anon_sym_time] = ACTIONS(115),
    [anon_sym_type] = ACTIONS(115),
    [anon_sym_vol] = ACTIONS(115),
    [anon_sym_attrib] = ACTIONS(115),
    [anon_sym_chkdsk] = ACTIONS(115),
    [anon_sym_choice] = ACTIONS(115),
    [anon_sym_cmd] = ACTIONS(115),
    [anon_sym_comp] = ACTIONS(115),
    [anon_sym_convert] = ACTIONS(115),
    [anon_sym_driverquery] = ACTIONS(115),
    [anon_sym_expand] = ACTIONS(115),
    [anon_sym_find] = ACTIONS(115),
    [anon_sym_format] = ACTIONS(115),
    [anon_sym_help] = ACTIONS(115),
    [anon_sym_ipconfig] = ACTIONS(115),
    [anon_sym_label] = ACTIONS(115),
    [anon_sym_net] = ACTIONS(115),
    [anon_sym_ping] = ACTIONS(115),
    [anon_sym_shutdown] = ACTIONS(115),
    [anon_sym_sort] = ACTIONS(115),
    [anon_sym_subst] = ACTIONS(115),
    [anon_sym_systeminfo] = ACTIONS(115),
    [anon_sym_taskkill] = ACTIONS(115),
    [anon_sym_tasklist] = ACTIONS(115),
    [anon_sym_xcopy] = ACTIONS(115),
    [anon_sym_tree] = ACTIONS(115),
    [anon_sym_fc] = ACTIONS(115),
    [anon_sym_diskpart] = ACTIONS(115),
    [anon_sym_title] = ACTIONS(115),
    [anon_sym_COLON] = ACTIONS(117),
  },
  [18] = {
    [ts_builtin_sym_end] = ACTIONS(119),
    [anon_sym_AT] = ACTIONS(119),
    [anon_sym_echooff] = ACTIONS(119),
    [anon_sym_COLON_COLON] = ACTIONS(119),
    [anon_sym_REM] = ACTIONS(119),
    [anon_sym_Rem] = ACTIONS(119),
    [anon_sym_rem] = ACTIONS(119),
    [anon_sym_SET] = ACTIONS(119),
    [anon_sym_Set] = ACTIONS(119),
    [anon_sym_set] = ACTIONS(119),
    [anon_sym_PERCENT] = ACTIONS(119),
    [anon_sym_ECHO] = ACTIONS(119),
    [anon_sym_IF] = ACTIONS(119),
    [anon_sym_GOTO] = ACTIONS(119),
    [anon_sym_EXIT] = ACTIONS(119),
    [anon_sym_FOR] = ACTIONS(121),
    [anon_sym_PAUSE] = ACTIONS(119),
    [anon_sym_CLS] = ACTIONS(119),
    [anon_sym_echo] = ACTIONS(121),
    [anon_sym_if] = ACTIONS(119),
    [anon_sym_goto] = ACTIONS(119),
    [anon_sym_exit] = ACTIONS(119),
    [anon_sym_for] = ACTIONS(121),
    [anon_sym_pause] = ACTIONS(119),
    [anon_sym_cls] = ACTIONS(119),
    [anon_sym_VER] = ACTIONS(119),
    [anon_sym_ASSOC] = ACTIONS(119),
    [anon_sym_CD] = ACTIONS(119),
    [anon_sym_COPY] = ACTIONS(119),
    [anon_sym_DEL] = ACTIONS(119),
    [anon_sym_DIR] = ACTIONS(119),
    [anon_sym_DATE] = ACTIONS(119),
    [anon_sym_MD] = ACTIONS(119),
    [anon_sym_MOVE] = ACTIONS(119),
    [anon_sym_PATH] = ACTIONS(119),
    [anon_sym_PROMPT] = ACTIONS(119),
    [anon_sym_RD] = ACTIONS(119),
    [anon_sym_REN] = ACTIONS(119),
    [anon_sym_START] = ACTIONS(119),
    [anon_sym_TIME] = ACTIONS(119),
    [anon_sym_TYPE] = ACTIONS(119),
    [anon_sym_VOL] = ACTIONS(119),
    [anon_sym_ATTRIB] = ACTIONS(119),
    [anon_sym_CHKDSK] = ACTIONS(119),
    [anon_sym_CHOICE] = ACTIONS(119),
    [anon_sym_CMD] = ACTIONS(119),
    [anon_sym_COMP] = ACTIONS(119),
    [anon_sym_CONVERT] = ACTIONS(119),
    [anon_sym_DRIVERQUERY] = ACTIONS(119),
    [anon_sym_EXPAND] = ACTIONS(119),
    [anon_sym_FIND] = ACTIONS(119),
    [anon_sym_FORMAT] = ACTIONS(119),
    [anon_sym_HELP] = ACTIONS(119),
    [anon_sym_IPCONFIG] = ACTIONS(119),
    [anon_sym_LABEL] = ACTIONS(119),
    [anon_sym_NET] = ACTIONS(119),
    [anon_sym_PING] = ACTIONS(119),
    [anon_sym_SHUTDOWN] = ACTIONS(119),
    [anon_sym_SORT] = ACTIONS(119),
    [anon_sym_SUBST] = ACTIONS(119),
    [anon_sym_SYSTEMINFO] = ACTIONS(119),
    [anon_sym_TASKKILL] = ACTIONS(119),
    [anon_sym_TASKLIST] = ACTIONS(119),
    [anon_sym_XCOPY] = ACTIONS(119),
    [anon_sym_TREE] = ACTIONS(119),
    [anon_sym_FC] = ACTIONS(119),
    [anon_sym_DISKPART] = ACTIONS(119),
    [anon_sym_TITLE] = ACTIONS(119),
    [anon_sym_ver] = ACTIONS(119),
    [anon_sym_assoc] = ACTIONS(119),
    [anon_sym_cd] = ACTIONS(119),
    [anon_sym_copy] = ACTIONS(119),
    [anon_sym_del] = ACTIONS(119),
    [anon_sym_dir] = ACTIONS(119),
    [anon_sym_date] = ACTIONS(119),
    [anon_sym_md] = ACTIONS(119),
    [anon_sym_move] = ACTIONS(119),
    [anon_sym_path] = ACTIONS(119),
    [anon_sym_prompt] = ACTIONS(119),
    [anon_sym_rd] = ACTIONS(119),
    [anon_sym_ren] = ACTIONS(119),
    [anon_sym_start] = ACTIONS(119),
    [anon_sym_time] = ACTIONS(119),
    [anon_sym_type] = ACTIONS(119),
    [anon_sym_vol] = ACTIONS(119),
    [anon_sym_attrib] = ACTIONS(119),
    [anon_sym_chkdsk] = ACTIONS(119),
    [anon_sym_choice] = ACTIONS(119),
    [anon_sym_cmd] = ACTIONS(119),
    [anon_sym_comp] = ACTIONS(119),
    [anon_sym_convert] = ACTIONS(119),
    [anon_sym_driverquery] = ACTIONS(119),
    [anon_sym_expand] = ACTIONS(119),
    [anon_sym_find] = ACTIONS(119),
    [anon_sym_format] = ACTIONS(119),
    [anon_sym_help] = ACTIONS(119),
    [anon_sym_ipconfig] = ACTIONS(119),
    [anon_sym_label] = ACTIONS(119),
    [anon_sym_net] = ACTIONS(119),
    [anon_sym_ping] = ACTIONS(119),
    [anon_sym_shutdown] = ACTIONS(119),
    [anon_sym_sort] = ACTIONS(119),
    [anon_sym_subst] = ACTIONS(119),
    [anon_sym_systeminfo] = ACTIONS(119),
    [anon_sym_taskkill] = ACTIONS(119),
    [anon_sym_tasklist] = ACTIONS(119),
    [anon_sym_xcopy] = ACTIONS(119),
    [anon_sym_tree] = ACTIONS(119),
    [anon_sym_fc] = ACTIONS(119),
    [anon_sym_diskpart] = ACTIONS(119),
    [anon_sym_title] = ACTIONS(119),
    [anon_sym_COLON] = ACTIONS(121),
  },
  [19] = {
    [ts_builtin_sym_end] = ACTIONS(123),
    [anon_sym_AT] = ACTIONS(123),
    [anon_sym_echooff] = ACTIONS(123),
    [anon_sym_COLON_COLON] = ACTIONS(123),
    [anon_sym_REM] = ACTIONS(123),
    [anon_sym_Rem] = ACTIONS(123),
    [anon_sym_rem] = ACTIONS(123),
    [anon_sym_SET] = ACTIONS(123),
    [anon_sym_Set] = ACTIONS(123),
    [anon_sym_set] = ACTIONS(123),
    [anon_sym_PERCENT] = ACTIONS(123),
    [anon_sym_ECHO] = ACTIONS(123),
    [anon_sym_IF] = ACTIONS(123),
    [anon_sym_GOTO] = ACTIONS(123),
    [anon_sym_EXIT] = ACTIONS(123),
    [anon_sym_FOR] = ACTIONS(125),
    [anon_sym_PAUSE] = ACTIONS(123),
    [anon_sym_CLS] = ACTIONS(123),
    [anon_sym_echo] = ACTIONS(125),
    [anon_sym_if] = ACTIONS(123),
    [anon_sym_goto] = ACTIONS(123),
    [anon_sym_exit] = ACTIONS(123),
    [anon_sym_for] = ACTIONS(125),
    [anon_sym_pause] = ACTIONS(123),
    [anon_sym_cls] = ACTIONS(123),
    [anon_sym_VER] = ACTIONS(123),
    [anon_sym_ASSOC] = ACTIONS(123),
    [anon_sym_CD] = ACTIONS(123),
    [anon_sym_COPY] = ACTIONS(123),
    [anon_sym_DEL] = ACTIONS(123),
    [anon_sym_DIR] = ACTIONS(123),
    [anon_sym_DATE] = ACTIONS(123),
    [anon_sym_MD] = ACTIONS(123),
    [anon_sym_MOVE] = ACTIONS(123),
    [anon_sym_PATH] = ACTIONS(123),
    [anon_sym_PROMPT] = ACTIONS(123),
    [anon_sym_RD] = ACTIONS(123),
    [anon_sym_REN] = ACTIONS(123),
    [anon_sym_START] = ACTIONS(123),
    [anon_sym_TIME] = ACTIONS(123),
    [anon_sym_TYPE] = ACTIONS(123),
    [anon_sym_VOL] = ACTIONS(123),
    [anon_sym_ATTRIB] = ACTIONS(123),
    [anon_sym_CHKDSK] = ACTIONS(123),
    [anon_sym_CHOICE] = ACTIONS(123),
    [anon_sym_CMD] = ACTIONS(123),
    [anon_sym_COMP] = ACTIONS(123),
    [anon_sym_CONVERT] = ACTIONS(123),
    [anon_sym_DRIVERQUERY] = ACTIONS(123),
    [anon_sym_EXPAND] = ACTIONS(123),
    [anon_sym_FIND] = ACTIONS(123),
    [anon_sym_FORMAT] = ACTIONS(123),
    [anon_sym_HELP] = ACTIONS(123),
    [anon_sym_IPCONFIG] = ACTIONS(123),
    [anon_sym_LABEL] = ACTIONS(123),
    [anon_sym_NET] = ACTIONS(123),
    [anon_sym_PING] = ACTIONS(123),
    [anon_sym_SHUTDOWN] = ACTIONS(123),
    [anon_sym_SORT] = ACTIONS(123),
    [anon_sym_SUBST] = ACTIONS(123),
    [anon_sym_SYSTEMINFO] = ACTIONS(123),
    [anon_sym_TASKKILL] = ACTIONS(123),
    [anon_sym_TASKLIST] = ACTIONS(123),
    [anon_sym_XCOPY] = ACTIONS(123),
    [anon_sym_TREE] = ACTIONS(123),
    [anon_sym_FC] = ACTIONS(123),
    [anon_sym_DISKPART] = ACTIONS(123),
    [anon_sym_TITLE] = ACTIONS(123),
    [anon_sym_ver] = ACTIONS(123),
    [anon_sym_assoc] = ACTIONS(123),
    [anon_sym_cd] = ACTIONS(123),
    [anon_sym_copy] = ACTIONS(123),
    [anon_sym_del] = ACTIONS(123),
    [anon_sym_dir] = ACTIONS(123),
    [anon_sym_date] = ACTIONS(123),
    [anon_sym_md] = ACTIONS(123),
    [anon_sym_move] = ACTIONS(123),
    [anon_sym_path] = ACTIONS(123),
    [anon_sym_prompt] = ACTIONS(123),
    [anon_sym_rd] = ACTIONS(123),
    [anon_sym_ren] = ACTIONS(123),
    [anon_sym_start] = ACTIONS(123),
    [anon_sym_time] = ACTIONS(123),
    [anon_sym_type] = ACTIONS(123),
    [anon_sym_vol] = ACTIONS(123),
    [anon_sym_attrib] = ACTIONS(123),
    [anon_sym_chkdsk] = ACTIONS(123),
    [anon_sym_choice] = ACTIONS(123),
    [anon_sym_cmd] = ACTIONS(123),
    [anon_sym_comp] = ACTIONS(123),
    [anon_sym_convert] = ACTIONS(123),
    [anon_sym_driverquery] = ACTIONS(123),
    [anon_sym_expand] = ACTIONS(123),
    [anon_sym_find] = ACTIONS(123),
    [anon_sym_format] = ACTIONS(123),
    [anon_sym_help] = ACTIONS(123),
    [anon_sym_ipconfig] = ACTIONS(123),
    [anon_sym_label] = ACTIONS(123),
    [anon_sym_net] = ACTIONS(123),
    [anon_sym_ping] = ACTIONS(123),
    [anon_sym_shutdown] = ACTIONS(123),
    [anon_sym_sort] = ACTIONS(123),
    [anon_sym_subst] = ACTIONS(123),
    [anon_sym_systeminfo] = ACTIONS(123),
    [anon_sym_taskkill] = ACTIONS(123),
    [anon_sym_tasklist] = ACTIONS(123),
    [anon_sym_xcopy] = ACTIONS(123),
    [anon_sym_tree] = ACTIONS(123),
    [anon_sym_fc] = ACTIONS(123),
    [anon_sym_diskpart] = ACTIONS(123),
    [anon_sym_title] = ACTIONS(123),
    [anon_sym_COLON] = ACTIONS(125),
  },
  [20] = {
    [ts_builtin_sym_end] = ACTIONS(127),
    [anon_sym_AT] = ACTIONS(127),
    [anon_sym_echooff] = ACTIONS(127),
    [anon_sym_COLON_COLON] = ACTIONS(127),
    [anon_sym_REM] = ACTIONS(127),
    [anon_sym_Rem] = ACTIONS(127),
    [anon_sym_rem] = ACTIONS(127),
    [anon_sym_SET] = ACTIONS(127),
    [anon_sym_Set] = ACTIONS(127),
    [anon_sym_set] = ACTIONS(127),
    [anon_sym_PERCENT] = ACTIONS(127),
    [anon_sym_ECHO] = ACTIONS(127),
    [anon_sym_IF] = ACTIONS(127),
    [anon_sym_GOTO] = ACTIONS(127),
    [anon_sym_EXIT] = ACTIONS(127),
    [anon_sym_FOR] = ACTIONS(129),
    [anon_sym_PAUSE] = ACTIONS(127),
    [anon_sym_CLS] = ACTIONS(127),
    [anon_sym_echo] = ACTIONS(129),
    [anon_sym_if] = ACTIONS(127),
    [anon_sym_goto] = ACTIONS(127),
    [anon_sym_exit] = ACTIONS(127),
    [anon_sym_for] = ACTIONS(129),
    [anon_sym_pause] = ACTIONS(127),
    [anon_sym_cls] = ACTIONS(127),
    [anon_sym_VER] = ACTIONS(127),
    [anon_sym_ASSOC] = ACTIONS(127),
    [anon_sym_CD] = ACTIONS(127),
    [anon_sym_COPY] = ACTIONS(127),
    [anon_sym_DEL] = ACTIONS(127),
    [anon_sym_DIR] = ACTIONS(127),
    [anon_sym_DATE] = ACTIONS(127),
    [anon_sym_MD] = ACTIONS(127),
    [anon_sym_MOVE] = ACTIONS(127),
    [anon_sym_PATH] = ACTIONS(127),
    [anon_sym_PROMPT] = ACTIONS(127),
    [anon_sym_RD] = ACTIONS(127),
    [anon_sym_REN] = ACTIONS(127),
    [anon_sym_START] = ACTIONS(127),
    [anon_sym_TIME] = ACTIONS(127),
    [anon_sym_TYPE] = ACTIONS(127),
    [anon_sym_VOL] = ACTIONS(127),
    [anon_sym_ATTRIB] = ACTIONS(127),
    [anon_sym_CHKDSK] = ACTIONS(127),
    [anon_sym_CHOICE] = ACTIONS(127),
    [anon_sym_CMD] = ACTIONS(127),
    [anon_sym_COMP] = ACTIONS(127),
    [anon_sym_CONVERT] = ACTIONS(127),
    [anon_sym_DRIVERQUERY] = ACTIONS(127),
    [anon_sym_EXPAND] = ACTIONS(127),
    [anon_sym_FIND] = ACTIONS(127),
    [anon_sym_FORMAT] = ACTIONS(127),
    [anon_sym_HELP] = ACTIONS(127),
    [anon_sym_IPCONFIG] = ACTIONS(127),
    [anon_sym_LABEL] = ACTIONS(127),
    [anon_sym_NET] = ACTIONS(127),
    [anon_sym_PING] = ACTIONS(127),
    [anon_sym_SHUTDOWN] = ACTIONS(127),
    [anon_sym_SORT] = ACTIONS(127),
    [anon_sym_SUBST] = ACTIONS(127),
    [anon_sym_SYSTEMINFO] = ACTIONS(127),
    [anon_sym_TASKKILL] = ACTIONS(127),
    [anon_sym_TASKLIST] = ACTIONS(127),
    [anon_sym_XCOPY] = ACTIONS(127),
    [anon_sym_TREE] = ACTIONS(127),
    [anon_sym_FC] = ACTIONS(127),
    [anon_sym_DISKPART] = ACTIONS(127),
    [anon_sym_TITLE] = ACTIONS(127),
    [anon_sym_ver] = ACTIONS(127),
    [anon_sym_assoc] = ACTIONS(127),
    [anon_sym_cd] = ACTIONS(127),
    [anon_sym_copy] = ACTIONS(127),
    [anon_sym_del] = ACTIONS(127),
    [anon_sym_dir] = ACTIONS(127),
    [anon_sym_date] = ACTIONS(127),
    [anon_sym_md] = ACTIONS(127),
    [anon_sym_move] = ACTIONS(127),
    [anon_sym_path] = ACTIONS(127),
    [anon_sym_prompt] = ACTIONS(127),
    [anon_sym_rd] = ACTIONS(127),
    [anon_sym_ren] = ACTIONS(127),
    [anon_sym_start] = ACTIONS(127),
    [anon_sym_time] = ACTIONS(127),
    [anon_sym_type] = ACTIONS(127),
    [anon_sym_vol] = ACTIONS(127),
    [anon_sym_attrib] = ACTIONS(127),
    [anon_sym_chkdsk] = ACTIONS(127),
    [anon_sym_choice] = ACTIONS(127),
    [anon_sym_cmd] = ACTIONS(127),
    [anon_sym_comp] = ACTIONS(127),
    [anon_sym_convert] = ACTIONS(127),
    [anon_sym_driverquery] = ACTIONS(127),
    [anon_sym_expand] = ACTIONS(127),
    [anon_sym_find] = ACTIONS(127),
    [anon_sym_format] = ACTIONS(127),
    [anon_sym_help] = ACTIONS(127),
    [anon_sym_ipconfig] = ACTIONS(127),
    [anon_sym_label] = ACTIONS(127),
    [anon_sym_net] = ACTIONS(127),
    [anon_sym_ping] = ACTIONS(127),
    [anon_sym_shutdown] = ACTIONS(127),
    [anon_sym_sort] = ACTIONS(127),
    [anon_sym_subst] = ACTIONS(127),
    [anon_sym_systeminfo] = ACTIONS(127),
    [anon_sym_taskkill] = ACTIONS(127),
    [anon_sym_tasklist] = ACTIONS(127),
    [anon_sym_xcopy] = ACTIONS(127),
    [anon_sym_tree] = ACTIONS(127),
    [anon_sym_fc] = ACTIONS(127),
    [anon_sym_diskpart] = ACTIONS(127),
    [anon_sym_title] = ACTIONS(127),
    [anon_sym_COLON] = ACTIONS(129),
  },
  [21] = {
    [ts_builtin_sym_end] = ACTIONS(131),
    [anon_sym_AT] = ACTIONS(131),
    [anon_sym_echooff] = ACTIONS(131),
    [anon_sym_COLON_COLON] = ACTIONS(131),
    [anon_sym_REM] = ACTIONS(131),
    [anon_sym_Rem] = ACTIONS(131),
    [anon_sym_rem] = ACTIONS(131),
    [anon_sym_SET] = ACTIONS(131),
    [anon_sym_Set] = ACTIONS(131),
    [anon_sym_set] = ACTIONS(131),
    [anon_sym_PERCENT] = ACTIONS(131),
    [anon_sym_ECHO] = ACTIONS(131),
    [anon_sym_IF] = ACTIONS(131),
    [anon_sym_GOTO] = ACTIONS(131),
    [anon_sym_EXIT] = ACTIONS(131),
    [anon_sym_FOR] = ACTIONS(133),
    [anon_sym_PAUSE] = ACTIONS(131),
    [anon_sym_CLS] = ACTIONS(131),
    [anon_sym_echo] = ACTIONS(133),
    [anon_sym_if] = ACTIONS(131),
    [anon_sym_goto] = ACTIONS(131),
    [anon_sym_exit] = ACTIONS(131),
    [anon_sym_for] = ACTIONS(133),
    [anon_sym_pause] = ACTIONS(131),
    [anon_sym_cls] = ACTIONS(131),
    [anon_sym_VER] = ACTIONS(131),
    [anon_sym_ASSOC] = ACTIONS(131),
    [anon_sym_CD] = ACTIONS(131),
    [anon_sym_COPY] = ACTIONS(131),
    [anon_sym_DEL] = ACTIONS(131),
    [anon_sym_DIR] = ACTIONS(131),
    [anon_sym_DATE] = ACTIONS(131),
    [anon_sym_MD] = ACTIONS(131),
    [anon_sym_MOVE] = ACTIONS(131),
    [anon_sym_PATH] = ACTIONS(131),
    [anon_sym_PROMPT] = ACTIONS(131),
    [anon_sym_RD] = ACTIONS(131),
    [anon_sym_REN] = ACTIONS(131),
    [anon_sym_START] = ACTIONS(131),
    [anon_sym_TIME] = ACTIONS(131),
    [anon_sym_TYPE] = ACTIONS(131),
    [anon_sym_VOL] = ACTIONS(131),
    [anon_sym_ATTRIB] = ACTIONS(131),
    [anon_sym_CHKDSK] = ACTIONS(131),
    [anon_sym_CHOICE] = ACTIONS(131),
    [anon_sym_CMD] = ACTIONS(131),
    [anon_sym_COMP] = ACTIONS(131),
    [anon_sym_CONVERT] = ACTIONS(131),
    [anon_sym_DRIVERQUERY] = ACTIONS(131),
    [anon_sym_EXPAND] = ACTIONS(131),
    [anon_sym_FIND] = ACTIONS(131),
    [anon_sym_FORMAT] = ACTIONS(131),
    [anon_sym_HELP] = ACTIONS(131),
    [anon_sym_IPCONFIG] = ACTIONS(131),
    [anon_sym_LABEL] = ACTIONS(131),
    [anon_sym_NET] = ACTIONS(131),
    [anon_sym_PING] = ACTIONS(131),
    [anon_sym_SHUTDOWN] = ACTIONS(131),
    [anon_sym_SORT] = ACTIONS(131),
    [anon_sym_SUBST] = ACTIONS(131),
    [anon_sym_SYSTEMINFO] = ACTIONS(131),
    [anon_sym_TASKKILL] = ACTIONS(131),
    [anon_sym_TASKLIST] = ACTIONS(131),
    [anon_sym_XCOPY] = ACTIONS(131),
    [anon_sym_TREE] = ACTIONS(131),
    [anon_sym_FC] = ACTIONS(131),
    [anon_sym_DISKPART] = ACTIONS(131),
    [anon_sym_TITLE] = ACTIONS(131),
    [anon_sym_ver] = ACTIONS(131),
    [anon_sym_assoc] = ACTIONS(131),
    [anon_sym_cd] = ACTIONS(131),
    [anon_sym_copy] = ACTIONS(131),
    [anon_sym_del] = ACTIONS(131),
    [anon_sym_dir] = ACTIONS(131),
    [anon_sym_date] = ACTIONS(131),
    [anon_sym_md] = ACTIONS(131),
    [anon_sym_move] = ACTIONS(131),
    [anon_sym_path] = ACTIONS(131),
    [anon_sym_prompt] = ACTIONS(131),
    [anon_sym_rd] = ACTIONS(131),
    [anon_sym_ren] = ACTIONS(131),
    [anon_sym_start] = ACTIONS(131),
    [anon_sym_time] = ACTIONS(131),
    [anon_sym_type] = ACTIONS(131),
    [anon_sym_vol] = ACTIONS(131),
    [anon_sym_attrib] = ACTIONS(131),
    [anon_sym_chkdsk] = ACTIONS(131),
    [anon_sym_choice] = ACTIONS(131),
    [anon_sym_cmd] = ACTIONS(131),
    [anon_sym_comp] = ACTIONS(131),
    [anon_sym_convert] = ACTIONS(131),
    [anon_sym_driverquery] = ACTIONS(131),
    [anon_sym_expand] = ACTIONS(131),
    [anon_sym_find] = ACTIONS(131),
    [anon_sym_format] = ACTIONS(131),
    [anon_sym_help] = ACTIONS(131),
    [anon_sym_ipconfig] = ACTIONS(131),
    [anon_sym_label] = ACTIONS(131),
    [anon_sym_net] = ACTIONS(131),
    [anon_sym_ping] = ACTIONS(131),
    [anon_sym_shutdown] = ACTIONS(131),
    [anon_sym_sort] = ACTIONS(131),
    [anon_sym_subst] = ACTIONS(131),
    [anon_sym_systeminfo] = ACTIONS(131),
    [anon_sym_taskkill] = ACTIONS(131),
    [anon_sym_tasklist] = ACTIONS(131),
    [anon_sym_xcopy] = ACTIONS(131),
    [anon_sym_tree] = ACTIONS(131),
    [anon_sym_fc] = ACTIONS(131),
    [anon_sym_diskpart] = ACTIONS(131),
    [anon_sym_title] = ACTIONS(131),
    [anon_sym_COLON] = ACTIONS(133),
  },
  [22] = {
    [ts_builtin_sym_end] = ACTIONS(135),
    [anon_sym_AT] = ACTIONS(135),
    [anon_sym_echooff] = ACTIONS(135),
    [anon_sym_COLON_COLON] = ACTIONS(135),
    [anon_sym_REM] = ACTIONS(135),
    [anon_sym_Rem] = ACTIONS(135),
    [anon_sym_rem] = ACTIONS(135),
    [anon_sym_SET] = ACTIONS(135),
    [anon_sym_Set] = ACTIONS(135),
    [anon_sym_set] = ACTIONS(135),
    [anon_sym_PERCENT] = ACTIONS(135),
    [anon_sym_ECHO] = ACTIONS(135),
    [anon_sym_IF] = ACTIONS(135),
    [anon_sym_GOTO] = ACTIONS(135),
    [anon_sym_EXIT] = ACTIONS(135),
    [anon_sym_FOR] = ACTIONS(137),
    [anon_sym_PAUSE] = ACTIONS(135),
    [anon_sym_CLS] = ACTIONS(135),
    [anon_sym_echo] = ACTIONS(137),
    [anon_sym_if] = ACTIONS(135),
    [anon_sym_goto] = ACTIONS(135),
    [anon_sym_exit] = ACTIONS(135),
    [anon_sym_for] = ACTIONS(137),
    [anon_sym_pause] = ACTIONS(135),
    [anon_sym_cls] = ACTIONS(135),
    [anon_sym_VER] = ACTIONS(135),
    [anon_sym_ASSOC] = ACTIONS(135),
    [anon_sym_CD] = ACTIONS(135),
    [anon_sym_COPY] = ACTIONS(135),
    [anon_sym_DEL] = ACTIONS(135),
    [anon_sym_DIR] = ACTIONS(135),
    [anon_sym_DATE] = ACTIONS(135),
    [anon_sym_MD] = ACTIONS(135),
    [anon_sym_MOVE] = ACTIONS(135),
    [anon_sym_PATH] = ACTIONS(135),
    [anon_sym_PROMPT] = ACTIONS(135),
    [anon_sym_RD] = ACTIONS(135),
    [anon_sym_REN] = ACTIONS(135),
    [anon_sym_START] = ACTIONS(135),
    [anon_sym_TIME] = ACTIONS(135),
    [anon_sym_TYPE] = ACTIONS(135),
    [anon_sym_VOL] = ACTIONS(135),
    [anon_sym_ATTRIB] = ACTIONS(135),
    [anon_sym_CHKDSK] = ACTIONS(135),
    [anon_sym_CHOICE] = ACTIONS(135),
    [anon_sym_CMD] = ACTIONS(135),
    [anon_sym_COMP] = ACTIONS(135),
    [anon_sym_CONVERT] = ACTIONS(135),
    [anon_sym_DRIVERQUERY] = ACTIONS(135),
    [anon_sym_EXPAND] = ACTIONS(135),
    [anon_sym_FIND] = ACTIONS(135),
    [anon_sym_FORMAT] = ACTIONS(135),
    [anon_sym_HELP] = ACTIONS(135),
    [anon_sym_IPCONFIG] = ACTIONS(135),
    [anon_sym_LABEL] = ACTIONS(135),
    [anon_sym_NET] = ACTIONS(135),
    [anon_sym_PING] = ACTIONS(135),
    [anon_sym_SHUTDOWN] = ACTIONS(135),
    [anon_sym_SORT] = ACTIONS(135),
    [anon_sym_SUBST] = ACTIONS(135),
    [anon_sym_SYSTEMINFO] = ACTIONS(135),
    [anon_sym_TASKKILL] = ACTIONS(135),
    [anon_sym_TASKLIST] = ACTIONS(135),
    [anon_sym_XCOPY] = ACTIONS(135),
    [anon_sym_TREE] = ACTIONS(135),
    [anon_sym_FC] = ACTIONS(135),
    [anon_sym_DISKPART] = ACTIONS(135),
    [anon_sym_TITLE] = ACTIONS(135),
    [anon_sym_ver] = ACTIONS(135),
    [anon_sym_assoc] = ACTIONS(135),
    [anon_sym_cd] = ACTIONS(135),
    [anon_sym_copy] = ACTIONS(135),
    [anon_sym_del] = ACTIONS(135),
    [anon_sym_dir] = ACTIONS(135),
    [anon_sym_date] = ACTIONS(135),
    [anon_sym_md] = ACTIONS(135),
    [anon_sym_move] = ACTIONS(135),
    [anon_sym_path] = ACTIONS(135),
    [anon_sym_prompt] = ACTIONS(135),
    [anon_sym_rd] = ACTIONS(135),
    [anon_sym_ren] = ACTIONS(135),
    [anon_sym_start] = ACTIONS(135),
    [anon_sym_time] = ACTIONS(135),
    [anon_sym_type] = ACTIONS(135),
    [anon_sym_vol] = ACTIONS(135),
    [anon_sym_attrib] = ACTIONS(135),
    [anon_sym_chkdsk] = ACTIONS(135),
    [anon_sym_choice] = ACTIONS(135),
    [anon_sym_cmd] = ACTIONS(135),
    [anon_sym_comp] = ACTIONS(135),
    [anon_sym_convert] = ACTIONS(135),
    [anon_sym_driverquery] = ACTIONS(135),
    [anon_sym_expand] = ACTIONS(135),
    [anon_sym_find] = ACTIONS(135),
    [anon_sym_format] = ACTIONS(135),
    [anon_sym_help] = ACTIONS(135),
    [anon_sym_ipconfig] = ACTIONS(135),
    [anon_sym_label] = ACTIONS(135),
    [anon_sym_net] = ACTIONS(135),
    [anon_sym_ping] = ACTIONS(135),
    [anon_sym_shutdown] = ACTIONS(135),
    [anon_sym_sort] = ACTIONS(135),
    [anon_sym_subst] = ACTIONS(135),
    [anon_sym_systeminfo] = ACTIONS(135),
    [anon_sym_taskkill] = ACTIONS(135),
    [anon_sym_tasklist] = ACTIONS(135),
    [anon_sym_xcopy] = ACTIONS(135),
    [anon_sym_tree] = ACTIONS(135),
    [anon_sym_fc] = ACTIONS(135),
    [anon_sym_diskpart] = ACTIONS(135),
    [anon_sym_title] = ACTIONS(135),
    [anon_sym_COLON] = ACTIONS(137),
  },
  [23] = {
    [ts_builtin_sym_end] = ACTIONS(139),
    [anon_sym_AT] = ACTIONS(139),
    [anon_sym_echooff] = ACTIONS(139),
    [anon_sym_COLON_COLON] = ACTIONS(139),
    [anon_sym_REM] = ACTIONS(139),
    [anon_sym_Rem] = ACTIONS(139),
    [anon_sym_rem] = ACTIONS(139),
    [anon_sym_SET] = ACTIONS(139),
    [anon_sym_Set] = ACTIONS(139),
    [anon_sym_set] = ACTIONS(139),
    [anon_sym_PERCENT] = ACTIONS(139),
    [anon_sym_ECHO] = ACTIONS(139),
    [anon_sym_IF] = ACTIONS(139),
    [anon_sym_GOTO] = ACTIONS(139),
    [anon_sym_EXIT] = ACTIONS(139),
    [anon_sym_FOR] = ACTIONS(141),
    [anon_sym_PAUSE] = ACTIONS(139),
    [anon_sym_CLS] = ACTIONS(139),
    [anon_sym_echo] = ACTIONS(141),
    [anon_sym_if] = ACTIONS(139),
    [anon_sym_goto] = ACTIONS(139),
    [anon_sym_exit] = ACTIONS(139),
    [anon_sym_for] = ACTIONS(141),
    [anon_sym_pause] = ACTIONS(139),
    [anon_sym_cls] = ACTIONS(139),
    [anon_sym_VER] = ACTIONS(139),
    [anon_sym_ASSOC] = ACTIONS(139),
    [anon_sym_CD] = ACTIONS(139),
    [anon_sym_COPY] = ACTIONS(139),
    [anon_sym_DEL] = ACTIONS(139),
    [anon_sym_DIR] = ACTIONS(139),
    [anon_sym_DATE] = ACTIONS(139),
    [anon_sym_MD] = ACTIONS(139),
    [anon_sym_MOVE] = ACTIONS(139),
    [anon_sym_PATH] = ACTIONS(139),
    [anon_sym_PROMPT] = ACTIONS(139),
    [anon_sym_RD] = ACTIONS(139),
    [anon_sym_REN] = ACTIONS(139),
    [anon_sym_START] = ACTIONS(139),
    [anon_sym_TIME] = ACTIONS(139),
    [anon_sym_TYPE] = ACTIONS(139),
    [anon_sym_VOL] = ACTIONS(139),
    [anon_sym_ATTRIB] = ACTIONS(139),
    [anon_sym_CHKDSK] = ACTIONS(139),
    [anon_sym_CHOICE] = ACTIONS(139),
    [anon_sym_CMD] = ACTIONS(139),
    [anon_sym_COMP] = ACTIONS(139),
    [anon_sym_CONVERT] = ACTIONS(139),
    [anon_sym_DRIVERQUERY] = ACTIONS(139),
    [anon_sym_EXPAND] = ACTIONS(139),
    [anon_sym_FIND] = ACTIONS(139),
    [anon_sym_FORMAT] = ACTIONS(139),
    [anon_sym_HELP] = ACTIONS(139),
    [anon_sym_IPCONFIG] = ACTIONS(139),
    [anon_sym_LABEL] = ACTIONS(139),
    [anon_sym_NET] = ACTIONS(139),
    [anon_sym_PING] = ACTIONS(139),
    [anon_sym_SHUTDOWN] = ACTIONS(139),
    [anon_sym_SORT] = ACTIONS(139),
    [anon_sym_SUBST] = ACTIONS(139),
    [anon_sym_SYSTEMINFO] = ACTIONS(139),
    [anon_sym_TASKKILL] = ACTIONS(139),
    [anon_sym_TASKLIST] = ACTIONS(139),
    [anon_sym_XCOPY] = ACTIONS(139),
    [anon_sym_TREE] = ACTIONS(139),
    [anon_sym_FC] = ACTIONS(139),
    [anon_sym_DISKPART] = ACTIONS(139),
    [anon_sym_TITLE] = ACTIONS(139),
    [anon_sym_ver] = ACTIONS(139),
    [anon_sym_assoc] = ACTIONS(139),
    [anon_sym_cd] = ACTIONS(139),
    [anon_sym_copy] = ACTIONS(139),
    [anon_sym_del] = ACTIONS(139),
    [anon_sym_dir] = ACTIONS(139),
    [anon_sym_date] = ACTIONS(139),
    [anon_sym_md] = ACTIONS(139),
    [anon_sym_move] = ACTIONS(139),
    [anon_sym_path] = ACTIONS(139),
    [anon_sym_prompt] = ACTIONS(139),
    [anon_sym_rd] = ACTIONS(139),
    [anon_sym_ren] = ACTIONS(139),
    [anon_sym_start] = ACTIONS(139),
    [anon_sym_time] = ACTIONS(139),
    [anon_sym_type] = ACTIONS(139),
    [anon_sym_vol] = ACTIONS(139),
    [anon_sym_attrib] = ACTIONS(139),
    [anon_sym_chkdsk] = ACTIONS(139),
    [anon_sym_choice] = ACTIONS(139),
    [anon_sym_cmd] = ACTIONS(139),
    [anon_sym_comp] = ACTIONS(139),
    [anon_sym_convert] = ACTIONS(139),
    [anon_sym_driverquery] = ACTIONS(139),
    [anon_sym_expand] = ACTIONS(139),
    [anon_sym_find] = ACTIONS(139),
    [anon_sym_format] = ACTIONS(139),
    [anon_sym_help] = ACTIONS(139),
    [anon_sym_ipconfig] = ACTIONS(139),
    [anon_sym_label] = ACTIONS(139),
    [anon_sym_net] = ACTIONS(139),
    [anon_sym_ping] = ACTIONS(139),
    [anon_sym_shutdown] = ACTIONS(139),
    [anon_sym_sort] = ACTIONS(139),
    [anon_sym_subst] = ACTIONS(139),
    [anon_sym_systeminfo] = ACTIONS(139),
    [anon_sym_taskkill] = ACTIONS(139),
    [anon_sym_tasklist] = ACTIONS(139),
    [anon_sym_xcopy] = ACTIONS(139),
    [anon_sym_tree] = ACTIONS(139),
    [anon_sym_fc] = ACTIONS(139),
    [anon_sym_diskpart] = ACTIONS(139),
    [anon_sym_title] = ACTIONS(139),
    [anon_sym_COLON] = ACTIONS(141),
  },
  [24] = {
    [anon_sym_echooff] = ACTIONS(143),
    [anon_sym_COLON_COLON] = ACTIONS(145),
    [anon_sym_REM] = ACTIONS(147),
    [anon_sym_Rem] = ACTIONS(145),
    [anon_sym_rem] = ACTIONS(147),
    [anon_sym_SET] = ACTIONS(149),
    [anon_sym_Set] = ACTIONS(151),
    [anon_sym_set] = ACTIONS(149),
    [anon_sym_ECHO] = ACTIONS(153),
    [anon_sym_IF] = ACTIONS(153),
    [anon_sym_GOTO] = ACTIONS(153),
    [anon_sym_EXIT] = ACTIONS(153),
    [anon_sym_FOR] = ACTIONS(155),
    [anon_sym_PAUSE] = ACTIONS(153),
    [anon_sym_CLS] = ACTIONS(153),
    [anon_sym_echo] = ACTIONS(155),
    [anon_sym_if] = ACTIONS(153),
    [anon_sym_goto] = ACTIONS(153),
    [anon_sym_exit] = ACTIONS(153),
    [anon_sym_for] = ACTIONS(155),
    [anon_sym_pause] = ACTIONS(153),
    [anon_sym_cls] = ACTIONS(153),
    [anon_sym_VER] = ACTIONS(153),
    [anon_sym_ASSOC] = ACTIONS(153),
    [anon_sym_CD] = ACTIONS(153),
    [anon_sym_COPY] = ACTIONS(153),
    [anon_sym_DEL] = ACTIONS(153),
    [anon_sym_DIR] = ACTIONS(153),
    [anon_sym_DATE] = ACTIONS(153),
    [anon_sym_MD] = ACTIONS(153),
    [anon_sym_MOVE] = ACTIONS(153),
    [anon_sym_PATH] = ACTIONS(153),
    [anon_sym_PROMPT] = ACTIONS(153),
    [anon_sym_RD] = ACTIONS(153),
    [anon_sym_REN] = ACTIONS(153),
    [anon_sym_START] = ACTIONS(153),
    [anon_sym_TIME] = ACTIONS(153),
    [anon_sym_TYPE] = ACTIONS(153),
    [anon_sym_VOL] = ACTIONS(153),
    [anon_sym_ATTRIB] = ACTIONS(153),
    [anon_sym_CHKDSK] = ACTIONS(153),
    [anon_sym_CHOICE] = ACTIONS(153),
    [anon_sym_CMD] = ACTIONS(153),
    [anon_sym_COMP] = ACTIONS(153),
    [anon_sym_CONVERT] = ACTIONS(153),
    [anon_sym_DRIVERQUERY] = ACTIONS(153),
    [anon_sym_EXPAND] = ACTIONS(153),
    [anon_sym_FIND] = ACTIONS(153),
    [anon_sym_FORMAT] = ACTIONS(153),
    [anon_sym_HELP] = ACTIONS(153),
    [anon_sym_IPCONFIG] = ACTIONS(153),
    [anon_sym_LABEL] = ACTIONS(153),
    [anon_sym_NET] = ACTIONS(153),
    [anon_sym_PING] = ACTIONS(153),
    [anon_sym_SHUTDOWN] = ACTIONS(153),
    [anon_sym_SORT] = ACTIONS(153),
    [anon_sym_SUBST] = ACTIONS(153),
    [anon_sym_SYSTEMINFO] = ACTIONS(153),
    [anon_sym_TASKKILL] = ACTIONS(153),
    [anon_sym_TASKLIST] = ACTIONS(153),
    [anon_sym_XCOPY] = ACTIONS(153),
    [anon_sym_TREE] = ACTIONS(153),
    [anon_sym_FC] = ACTIONS(153),
    [anon_sym_DISKPART] = ACTIONS(153),
    [anon_sym_TITLE] = ACTIONS(153),
    [anon_sym_ver] = ACTIONS(153),
    [anon_sym_assoc] = ACTIONS(153),
    [anon_sym_cd] = ACTIONS(153),
    [anon_sym_copy] = ACTIONS(153),
    [anon_sym_del] = ACTIONS(153),
    [anon_sym_dir] = ACTIONS(153),
    [anon_sym_date] = ACTIONS(153),
    [anon_sym_md] = ACTIONS(153),
    [anon_sym_move] = ACTIONS(153),
    [anon_sym_path] = ACTIONS(153),
    [anon_sym_prompt] = ACTIONS(153),
    [anon_sym_rd] = ACTIONS(153),
    [anon_sym_ren] = ACTIONS(153),
    [anon_sym_start] = ACTIONS(153),
    [anon_sym_time] = ACTIONS(153),
    [anon_sym_type] = ACTIONS(153),
    [anon_sym_vol] = ACTIONS(153),
    [anon_sym_attrib] = ACTIONS(153),
    [anon_sym_chkdsk] = ACTIONS(153),
    [anon_sym_choice] = ACTIONS(153),
    [anon_sym_cmd] = ACTIONS(153),
    [anon_sym_comp] = ACTIONS(153),
    [anon_sym_convert] = ACTIONS(153),
    [anon_sym_driverquery] = ACTIONS(153),
    [anon_sym_expand] = ACTIONS(153),
    [anon_sym_find] = ACTIONS(153),
    [anon_sym_format] = ACTIONS(153),
    [anon_sym_help] = ACTIONS(153),
    [anon_sym_ipconfig] = ACTIONS(153),
    [anon_sym_label] = ACTIONS(153),
    [anon_sym_net] = ACTIONS(153),
    [anon_sym_ping] = ACTIONS(153),
    [anon_sym_shutdown] = ACTIONS(153),
    [anon_sym_sort] = ACTIONS(153),
    [anon_sym_subst] = ACTIONS(153),
    [anon_sym_systeminfo] = ACTIONS(153),
    [anon_sym_taskkill] = ACTIONS(153),
    [anon_sym_tasklist] = ACTIONS(153),
    [anon_sym_xcopy] = ACTIONS(153),
    [anon_sym_tree] = ACTIONS(153),
    [anon_sym_fc] = ACTIONS(153),
    [anon_sym_diskpart] = ACTIONS(153),
    [anon_sym_title] = ACTIONS(153),
    [anon_sym_COLON] = ACTIONS(157),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 4,
    ACTIONS(17), 1,
      anon_sym_PERCENT,
    ACTIONS(67), 1,
      anon_sym_DQUOTE,
    ACTIONS(159), 1,
      sym_number,
    STATE(22), 2,
      sym_variable_reference,
      sym_string,
  [14] = 4,
    ACTIONS(17), 1,
      anon_sym_PERCENT,
    ACTIONS(67), 1,
      anon_sym_DQUOTE,
    ACTIONS(161), 1,
      sym_number,
    STATE(19), 2,
      sym_variable_reference,
      sym_string,
  [28] = 4,
    ACTIONS(17), 1,
      anon_sym_PERCENT,
    ACTIONS(67), 1,
      anon_sym_DQUOTE,
    ACTIONS(163), 1,
      sym_number,
    STATE(23), 2,
      sym_variable_reference,
      sym_string,
  [42] = 3,
    ACTIONS(165), 1,
      anon_sym_DQUOTE,
    ACTIONS(167), 1,
      aux_sym_string_token1,
    STATE(30), 1,
      aux_sym_string_repeat1,
  [52] = 3,
    ACTIONS(169), 1,
      anon_sym_DQUOTE,
    ACTIONS(171), 1,
      aux_sym_string_token1,
    STATE(29), 1,
      aux_sym_string_repeat1,
  [62] = 3,
    ACTIONS(174), 1,
      anon_sym_DQUOTE,
    ACTIONS(176), 1,
      aux_sym_string_token1,
    STATE(29), 1,
      aux_sym_string_repeat1,
  [72] = 2,
    ACTIONS(75), 1,
      anon_sym_SLASHA,
    ACTIONS(178), 1,
      sym_identifier,
  [79] = 2,
    ACTIONS(63), 1,
      anon_sym_SLASHA,
    ACTIONS(180), 1,
      sym_identifier,
  [86] = 1,
    ACTIONS(182), 1,
      sym_identifier,
  [90] = 1,
    ACTIONS(184), 1,
      anon_sym_EQ,
  [94] = 1,
    ACTIONS(186), 1,
      anon_sym_PERCENT,
  [98] = 1,
    ACTIONS(188), 1,
      sym_identifier,
  [102] = 1,
    ACTIONS(190), 1,
      anon_sym_EQ,
  [106] = 1,
    ACTIONS(192), 1,
      ts_builtin_sym_end,
  [110] = 1,
    ACTIONS(194), 1,
      aux_sym_comment_token1,
  [114] = 1,
    ACTIONS(196), 1,
      aux_sym_comment_token1,
  [118] = 1,
    ACTIONS(198), 1,
      sym_identifier,
  [122] = 1,
    ACTIONS(200), 1,
      anon_sym_EQ,
  [126] = 1,
    ACTIONS(202), 1,
      sym_identifier,
  [130] = 1,
    ACTIONS(180), 1,
      sym_identifier,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(25)] = 0,
  [SMALL_STATE(26)] = 14,
  [SMALL_STATE(27)] = 28,
  [SMALL_STATE(28)] = 42,
  [SMALL_STATE(29)] = 52,
  [SMALL_STATE(30)] = 62,
  [SMALL_STATE(31)] = 72,
  [SMALL_STATE(32)] = 79,
  [SMALL_STATE(33)] = 86,
  [SMALL_STATE(34)] = 90,
  [SMALL_STATE(35)] = 94,
  [SMALL_STATE(36)] = 98,
  [SMALL_STATE(37)] = 102,
  [SMALL_STATE(38)] = 106,
  [SMALL_STATE(39)] = 110,
  [SMALL_STATE(40)] = 114,
  [SMALL_STATE(41)] = 118,
  [SMALL_STATE(42)] = 122,
  [SMALL_STATE(43)] = 126,
  [SMALL_STATE(44)] = 130,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_program, 0, 0, 0),
  [5] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [21] = {.entry = {.count = 1, .reusable = false}}, SHIFT(9),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(33),
  [25] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_program, 1, 0, 0),
  [27] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0),
  [29] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(24),
  [32] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(13),
  [35] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(39),
  [38] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(6),
  [41] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(5),
  [44] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(31),
  [47] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(43),
  [50] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(9),
  [53] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(9),
  [56] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_program_repeat1, 2, 0, 0), SHIFT_REPEAT(33),
  [59] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_keyword, 2, 0, 0),
  [61] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_keyword, 2, 0, 0),
  [63] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [65] = {.entry = {.count = 1, .reusable = false}}, SHIFT(42),
  [67] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [69] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [71] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_keyword, 1, 0, 0),
  [73] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_keyword, 1, 0, 0),
  [75] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [77] = {.entry = {.count = 1, .reusable = false}}, SHIFT(34),
  [79] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [81] = {.entry = {.count = 1, .reusable = false}}, SHIFT(10),
  [83] = {.entry = {.count = 1, .reusable = false}}, SHIFT(28),
  [85] = {.entry = {.count = 1, .reusable = false}}, SHIFT(14),
  [87] = {.entry = {.count = 1, .reusable = false}}, SHIFT(15),
  [89] = {.entry = {.count = 1, .reusable = false}}, SHIFT(16),
  [91] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_comment, 2, 0, 0),
  [93] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_comment, 2, 0, 0),
  [95] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_definition, 2, 0, 1),
  [97] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_definition, 2, 0, 1),
  [99] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_echooff, 2, 0, 0),
  [101] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_echooff, 2, 0, 0),
  [103] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_echooff, 1, 0, 0),
  [105] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_echooff, 1, 0, 0),
  [107] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_comment, 3, 0, 0),
  [109] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_comment, 3, 0, 0),
  [111] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_keyword, 3, 0, 0),
  [113] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_keyword, 3, 0, 0),
  [115] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_definition, 3, 0, 2),
  [117] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_definition, 3, 0, 2),
  [119] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 2, 0, 0),
  [121] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string, 2, 0, 0),
  [123] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_variable_declaration, 4, 0, 0),
  [125] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_variable_declaration, 4, 0, 0),
  [127] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 3, 0, 0),
  [129] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_string, 3, 0, 0),
  [131] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_variable_reference, 3, 0, 3),
  [133] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_variable_reference, 3, 0, 3),
  [135] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_variable_declaration, 5, 0, 0),
  [137] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_variable_declaration, 5, 0, 0),
  [139] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_variable_declaration, 6, 0, 0),
  [141] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_variable_declaration, 6, 0, 0),
  [143] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [145] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [147] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [149] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [151] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [153] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [155] = {.entry = {.count = 1, .reusable = false}}, SHIFT(8),
  [157] = {.entry = {.count = 1, .reusable = false}}, SHIFT(36),
  [159] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [163] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [165] = {.entry = {.count = 1, .reusable = false}}, SHIFT(18),
  [167] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [169] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0),
  [171] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(29),
  [174] = {.entry = {.count = 1, .reusable = false}}, SHIFT(20),
  [176] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [178] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [180] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [182] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [184] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [186] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [188] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [190] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [192] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [194] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [196] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [198] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [200] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [202] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
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

TS_PUBLIC const TSLanguage *tree_sitter_batch(void) {
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
