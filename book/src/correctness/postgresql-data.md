# PostgreSQL data

A schema read from DDL is only half of a database; the rows are the other half. This page states how the PostgreSQL route reads the rows of a data script, what each column type's values become, what it refuses, how foreign keys are resolved to the rows they name, and the laws its reader satisfies. The DDL the rows are read against is the subject of the [PostgreSQL DDL](postgresql-ddl.md) page.

The route reads PostgreSQL 17 data scripts, as the PostgreSQL 17 documentation defines them (the reader and its corpus follow release 17.12): the `INSERT ... VALUES` statements that `pg_dump --inserts` and `pg_dump --column-inserts` write, and those a person writes by hand, after or without the DDL. A data script's syntax is read as a table: the rows are an [`FInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/struct.FInstance.html) of the schema's tables. Its interpreted data is a graph: the `postgresql` protocol declares a table syntactic shape and a graph semantic shape, the graph being the table's graph form with one `foreign-key` edge from each row to the row its foreign key names. The reader is [`read_dump`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.read_dump.html) in `panproto-io` (feature `tree-sitter`), with [`read_data`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.read_data.html) for a data-only script read against a schema given separately. The registry's table codec for the protocol reads either kind of script and writes the canonical one, and its resolution route resolves the foreign keys.

## What the reader reads

The script is parsed with the same tree-sitter grammar as the DDL, generated from PostgreSQL's `gram.y`, and is read in script order. The DDL statements are read as the DDL page states. Each `INSERT INTO table [(columns)] VALUES (...), (...)` adds its rows to a table that a statement before it declared; without a column list the values fill the columns in position order. A value is a constant of the grammar: `NULL`, `TRUE`, `FALSE`, an integer or float constant with at most one sign, or a string constant (a standard string, an `E` string or a dollar-quoted string, with its continuation segments). The constant is read under its column's type by the rules of the next table, which are those of the type's input function (Chapter 8), restricted to the forms whose value the instance model holds exactly. A column that a statement leaves out, or gives `DEFAULT`, and every column under `DEFAULT VALUES`, is `NULL` when it declares no `DEFAULT`. `SET standard_conforming_strings` changes how the standard strings after it are read (with it off, a backslash starts an escape, as in an `E` string), and `SET client_encoding` must name UTF-8. `ALTER TABLE ... ENABLE TRIGGER` and `DISABLE TRIGGER`, which a data-only dump writes around its rows, are read past with the other statements that declare nothing a schema holds.

Rows are numbered from zero, by table name and then in the order the script inserts them, so the instance does not depend on how the script interleaves its tables. `NULL` is [`Value::Null`](https://docs.rs/panproto-inst/latest/panproto_inst/enum.Value.html), distinct from the empty string.

| Column type | Input forms read | Value |
|---|---|---|
| `smallint`, `integer`, `bigint` | Optional white space and sign, decimal digits or a `0x`, `0o`, `0b` prefix and its digits, `_` between two digits; in the type's range | `Int` |
| `numeric` | Decimal digits with an optional point and exponent (`_` between digits), a radix-prefixed integer, `NaN`, `Infinity`, `inf` with an optional sign, any case | `Decimal` with PostgreSQL's display scale: the digits after the point less the exponent, never below zero, so `1e2` is `100` and `1.50` keeps its scale; a zero has no sign |
| `numeric(p, s)` | As `numeric`, at most `s` digits after the point and `p - s` before it, no infinity | `Decimal` padded to scale `s` |
| `real`, `double precision` | A decimal numeral that the type holds exactly or whose value has it as its shortest text (the shared coercion), or `NaN`, `Infinity`, `inf` with an optional sign; an unquoted zero constant is positive, since it is a `numeric` first | `Float` |
| `text`, `character varying(n)` | Any text without NUL; a text longer than `n` characters is cut when the excess is spaces | `Str` |
| `character(n)` | As `character varying(n)`, then padded to `n` with spaces | `Str` |
| `boolean` | `TRUE`, `FALSE`, and `boolin`'s spellings: a prefix of `true`, `false`, `yes`, `no`, two or more letters of `on` or `off`, `1`, `0`, white space trimmed, any case | `Bool` |
| `bytea` | The hex format `\x` and hex digit pairs, white space between pairs; the escape format, `\\` and `\` with three octal digits | `Bytes` |
| `date`, `time`, `timestamp` | The ISO 8601 forms of the shared temporal reader, with at most six fraction digits and no offset; a date or timestamp may be `infinity` or `-infinity` | `Temporal`, kept as written |
| `timestamp with time zone` | As `timestamp`, with an offset | `Temporal`, its offset kept as written |
| `uuid` | 32 hex digits, optionally in braces, a hyphen allowed after any group of four | `Str`, the canonical lowercase form |
| `json`, `jsonb` | A JSON text | `Str`, the text as written |
| an enum type | One of its labels, exactly | `Str` |
| an array of any of these | `array_in`'s form, `{...}` nested once per declared dimension, elements quoted with `"` or unquoted and trimmed, unquoted `NULL` a null element, sub-arrays of one level of one length; or the empty array `{}` | `List` |

## What the reader refuses

Every refusal is a [`SqlDataError`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/enum.SqlDataError.html) naming the byte offset of the token it is about: a `Script` refusal, which is the DDL reader's [`SqlDdlError`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/postgresql/enum.SqlDdlError.html), or a `Value` refusal naming the table, the column and a [`ValueRefusal`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/enum.ValueRefusal.html). A script whose DDL is refused is refused at the DDL's first refusal; otherwise the first refused statement or value in script order is reported.

| Feature | Refusal | Reason |
|---|---|---|
| An expression other than a constant: an operator, a function call, a cast (`'1'::integer`), a typed constant (`DATE '...'`), `ARRAY[...]`, a bit-string constant, a `U&'...'` string (which the grammar reads as an operator expression) | `Unsupported`, naming it | No expression is evaluated |
| A `DEFAULT` (left out, given, or `DEFAULT VALUES`) of a column that declares one, a serial column's included | `Unsupported` | The default expression is not evaluated |
| `INSERT ... SELECT`, `WITH`, `ON CONFLICT`, `RETURNING`, `OVERRIDING ... VALUE` | `Unsupported` | They depend on the database's state |
| `UPDATE`, `DELETE`, `TRUNCATE`, `MERGE` | `Unsupported` | They change rows already read |
| `DROP TABLE` of a table holding rows, `ALTER TABLE ... ADD COLUMN` to one | `Unsupported` | They change rows already read |
| An `INSERT` into a table no statement before it declared, a target column the table does not declare, or named twice | `UnknownTable`, `UnknownColumn`, `Duplicate` | PostgreSQL refuses them too |
| More values than target columns, or fewer than the columns listed | `Syntax` | PostgreSQL refuses them too |
| A string constant whose escapes decode to NUL or to bytes that are not UTF-8 | `Syntax` | PostgreSQL refuses them too |
| A backslash before a quote in a standard string while `standard_conforming_strings` is off | `Unsupported` | The grammar tokenizes the string as the setting on does |
| A `client_encoding` other than UTF-8, a `standard_conforming_strings` value other than on or off | `Unsupported` | The text would be read differently |
| `COPY ... FROM stdin` and its data lines | `Syntax`, in the data | See the declared limit below |
| A value out of its type's range, a `numeric(p, s)` value with too many digits, an infinity in `numeric(p, s)` | `Value` (`Overflow`, `ScaleExceeded`, `PrecisionExceeded`, `NonFinite`) | PostgreSQL refuses the range and the infinity, and rounds the scale; nothing is rounded here |
| A float numeral that the type would round, an integer constant with a fraction, a time with more than six fraction digits, a leap second | `Value` (`PrecisionExceeded` or `Rounded`) | PostgreSQL would round the value |
| A `timestamp with time zone` without an offset, a `timestamp`, `time` or `date` with one | `Value` (`OffsetRequired`, `OffsetForbidden`) | PostgreSQL would read the first in the session's time zone and ignore the second |
| A date or time in a form other than ISO 8601 (`January 8, 1999`, `epoch`, `now`, BC dates) | `Value` (`InvalidTemporal`) | Its value depends on `DateStyle` or the clock, or is not carried |
| A text longer than its column, other than by spaces | `Value` (`TooLong`) | PostgreSQL refuses it too |
| A text that is no label of its enum, no JSON text, no form of its type | `Value` (`NotALabel`, `Syntax`) | PostgreSQL refuses them too |
| An array with explicit bounds (`[0:1]={...}`), or whose dimensions differ from the column's | `Value` (`Unsupported`, `Dimensions`) | Bounds are not carried; PostgreSQL accepts any dimensions, the schema declares one |
| A numeric constant for a non-numeric column, a boolean constant for a non-boolean one | `Value` (`Unsupported`) | The assignment cast is not evaluated |

The reader does not compute `jsonb`'s normal form (keys sorted, the last of duplicate keys kept): a `jsonb` value is the text as written, so two spellings of one `jsonb` value are two values. Domain `CHECK` constraints and table `CHECK` constraints are not evaluated, as on the DDL page.

### Declared limit: `COPY ... FROM stdin`

`pg_dump` without `--inserts` writes each table's rows as a `COPY table (columns) FROM stdin;` statement followed by tab-separated data lines and a line `\.`. The grammar has no token for those lines: it parses the statement and then fails inside the data, so the script is refused with `Syntax` at the first data line (the test `copy_from_stdin_data_is_refused_inside_the_data` pins it). Reading them needs a grammar change in [gmr/tree-sitter-postgres](https://github.com/gmr/tree-sitter-postgres): a new external scanner token `COPY_DATA`, scanned after `DOLLAR_QUOTED_STRING` in `postgres/src/scanner.c`, that takes every byte after the newline ending a `COPY ... FROM STDIN` statement up to and including the line `\.`; `$.copy_data` in the grammar's `externals`; and an optional `$.copy_data` after a `CopyStmt` whose source is `STDIN`, made in upstream's `script/codegen.js`, which generates `grammar.js` from `gram.y`. Regenerating the parser after that change needs `tree-sitter generate` on the whole PostgreSQL grammar, which upstream reports needs more than 12.5 GB of memory; the vendored parser has not been regenerated. Until it is, convert such a dump with `pg_dump --inserts`, or load the rows as a table directory.

## Keys and foreign keys

The reader checks no key: primary-key, unique and `NOT NULL` violations are reported by the shared validation ([`validate_table`](https://docs.rs/panproto-inst/latest/panproto_inst/validation/fn.validate_table.html) under the `postgresql` protocol), whose key check compares each key's parts with SQL key equality, a `character(n)` part ignoring trailing spaces. Foreign keys are resolved by the shared reference resolution of the [Reference resolution](resolution.md) page: [`resolve_foreign_keys`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.resolve_foreign_keys.html), or the registry's resolution route for the protocol, compiles the `TableToGraph` lens of [`ForeignKeyResolution`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/struct.ForeignKeyResolution.html) over the schema and adds one `foreign-key` edge from each row to the row its key values name. A composite foreign key compares every part. Under `MATCH SIMPLE` a key with any `NULL` part references nothing; under `MATCH FULL` a key with every part `NULL` references nothing and one with some but not all is refused. A key value that no row holds is a dangling reference and one that more than one row holds (a primary-key violation of the target) an ambiguous one: the run refuses either with a typed `ResolveError`, or, when lenient, records it in the complement as an unresolved reference with its reason. A deferrable foreign key is refused by the DDL reader, and a labelled null, which a script cannot write, is refused by the writer.

## Canonical output

[`write_data`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.write_data.html) writes an instance's rows as one `INSERT` statement per table holding rows, tables in name order, every column listed in position order and the rows in id order: `NULL`, `TRUE` and `FALSE`, an integer or a finite `numeric` unquoted, and every other value as a standard string constant holding its output text (a float by its shortest text, `bytea` in the hex format, a temporal as written, an array in `array_out`'s form). Each written value is read back before it is written, and a value that would not read back as itself is refused, as are a table or column the schema does not declare, a row without a cell for a column, foreign-key pairs held in the instance (a script states foreign keys only through column values) and a labelled null. [`write_dump`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.write_dump.html) writes the schema's types and tables, then the rows, then the foreign keys, so the script loads into PostgreSQL whatever order its tables reference each other in. The route claims no format preservation: reading the canonical script gives the same schema and the same instance, its rows numbered as the reader numbers them.

## Conformance corpora

The reader is run against three corpora vendored under `crates/panproto-io/tests/fixtures/conformance` with their licences and upstream commits (`conformance_postgresql_data.rs`): the `INSERT` examples of section 6.1 of the PostgreSQL 17 documentation, each after the section's `CREATE TABLE` (6 cases, regenerated by `extract_dml.py`); the tutorial script `src/tutorial/basics.source` of the PostgreSQL source; and a bounded subset of the jOOQ Sakila sample data, the 931 rows of the tables `language`, `country`, `city`, `actor` and `category` after their DDL (112,823 bytes of the 7,963,146-byte data file, regenerated by `extract_data.py`; it sets `standard_conforming_strings` off, disables triggers around each table and writes `character(20)` names that are padded). A case passes when it reads, its foreign keys resolve, the shared validation reports nothing, and its canonical script reads back as the same schema and instance and writes the same text. A case that does not pass is a declared limit in `postgresql-data-limits.txt`. The reader reads 6 of the 8 cases; on the Sakila subset it reads 931 rows and resolves the 600 foreign keys of `city`.

| Limit | Cases | What the reader does |
|---|---|---|
| `insert-select` | 1 | Refuses `INSERT ... SELECT` |
| `type-point` | 1 | Refuses the tutorial's `point` column, as the DDL reader does |

## Input texts read as their values {#L-postgresql-data-1}

### Statement

For a column type $t$ and an input text $s$ of the bounded scope, write $V(t, s)$ for `input_value(t, s)`. The PostgreSQL route reads `CREATE TABLE t (c t); INSERT INTO t VALUES ('s');` (with each quote of $s$ doubled) as one row whose cell `c` is the value $V(t, s)$ writes, or, when $V(t, s)$ refuses with class $r$, refuses the script with a `Value` refusal of class $r$.

### Outside the domain

Floats, temporals, enums, JSON and arrays are outside the bounded scope; their reading is checked by the fixed tests of `postgresql_data_reads_insert_dumps.rs`, the float rule by the shared coercion's own laws on the [Values](values.md) page. Unquoted constants are checked by [L-postgresql-data-2](#L-postgresql-data-2) and the fixed tests.

### Definitions used

$V$ reads the types `smallint`, `integer`, `bigint`, `numeric`, `numeric(5,2)`, `boolean`, `bytea`, `varchar(3)`, `character(3)`, `text` and `uuid` by the rules of the type table above, written from Chapter 8 of the PostgreSQL 17 documentation and the input functions it names: integer ranges of 16, 32 and 64 bits; `numeric`'s display scale and exponent bound of 1000; `boolin`'s prefixes; `byteain`'s two formats; the length rule of Sec 8.3; `uuid_in`'s hyphen groups. Its values are canonical strings (`int:42`, `numeric:1.50`, `bool:true`, `bytes:00ff`, `text:abc`).

### Proof

1. The production reader decodes the string constant's body, then reads it with the input rule of its column's type in the `values` module; the reference applies its rule for the same type to the same text. Both rules check the same conditions in the same order: white space, sign, radix prefix and digits (with the placement of `_`), then the range or the scale and precision, then the length.
2. For `numeric`, both compute the digits and the display scale from the mantissa and the exponent without floating point, so the values agree digit for digit; production's `DecimalValue` with exponent $-$scale is written by the test in positional notation and compared as a string.
3. The differential checks the equality of every value and every refusal class on all 704 cases: 11 types, each with 64 input texts chosen to cross every boundary of the rules.

### Executable counterpart

- Law ID: `postgresql-data.values`
- Reference: `panproto_reference::pages::postgresql_data::input_value`
- Enumerator: `panproto_reference::enumerate::postgresql_data::value_cases; bound: cases=704`
- Exhaustive differential test: `input_texts_read_as_the_reference_on_every_bounded_case; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_postgresql_data --no-tests=fail -E 'test(=input_texts_read_as_the_reference_on_every_bounded_case)'`
- Reference test: `input_forms_read_as_the_type_documentation_gives_them; command: cargo nextest run -p panproto-reference --test postgresql_data --no-tests=fail -E 'test(=input_forms_read_as_the_type_documentation_gives_them)'`
- File: `crates/panproto-io/src/postgresql/values.rs`
- File: `crates/panproto-io/src/postgresql/data.rs`
- File: `crates/panproto-reference/src/pages/postgresql_data.rs`

## An INSERT adds the rows the INSERT page defines {#L-postgresql-data-2}

### Statement

For a script model $m$, write $R(m)$ for `render(m)` and $K(m)$ for `read_script(m)`. When $K(m)$ reads, the PostgreSQL route reads $R(m)$, after the model's DDL, as exactly the rows of $K(m)$: table `a`'s rows numbered from zero in script order, then table `b`'s, each cell the value $K(m)$ gives or `NULL`. When $K(m)$ refuses with class $r$, the route refuses $R(m)$ with a refusal of class $r$.

### Outside the domain

The model's tables are `a (x integer, y text, z integer DEFAULT 0)` and `b (x integer)`, its statements are one-row `INSERT ... VALUES` with or without a column list, and its constants are `NULL`, `DEFAULT`, the integer $-1$ and two strings; multi-row statements, the other constants and the statements the reader refuses by name are checked by the fixed tests.

### Definitions used

$K$ follows the `INSERT` reference page of PostgreSQL 17: the target columns are the listed ones, or all in order; a row with more values than targets, or fewer than the listed targets, is refused (`Syntax`); each value is read under its column's type in target order (`Value` when its type does not read it, an integer constant for the text column included); and a column left out or given `DEFAULT` takes its default, which is `NULL` for `x` and `y` and refused for `z` (`Unsupported`), since its `DEFAULT 0` is not evaluated. The rows are numbered by table name, then in script order.

### Proof

1. The production reader reads each statement's constants first, then each row: the count of values against the targets, the values in target order under their columns' types, and last the columns left to a default. $K$ checks the same conditions in the same order, so the first refusal has the same class.
2. A row of a statement that reads holds, for each column, the value of the constant targeting it or `NULL`, in both.
3. Production gathers rows by table and numbers them by table name, then in insertion order; $K$ returns `a`'s rows before `b`'s, each in script order. The test checks that production's ids are those positions.
4. The exhaustive differential checks the equality of rows and refusal classes on all 2,060 scripts of the bounded scope (835 single statements and the 1,225 ordered pairs of the 35 one-cell statements, 228 of the scripts reading), and the property differential on generated scripts of up to seven statements.

### Executable counterpart

- Law ID: `postgresql-data.rows`
- Reference: `panproto_reference::pages::postgresql_data::render`
- Reference: `panproto_reference::pages::postgresql_data::read_script`
- Enumerator: `panproto_reference::enumerate::postgresql_data::scripts; bound: scripts=2060`
- Exhaustive differential test: `rows_match_the_reference_on_every_bounded_script; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_postgresql_data --no-tests=fail -E 'test(=rows_match_the_reference_on_every_bounded_script)'`
- Property differential test: `randomized_scripts_read_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_postgresql_data --no-tests=fail -E 'test(=randomized_scripts_read_as_the_reference)'`
- Reference test: `an_insert_adds_rows_as_the_insert_page_defines_them; command: cargo nextest run -p panproto-reference --test postgresql_data --no-tests=fail -E 'test(=an_insert_adds_rows_as_the_insert_page_defines_them)'`
- Randomized input scope: one to seven statements, each any statement of the bounded scope.
- File: `crates/panproto-io/src/postgresql/data.rs`
- File: `crates/panproto-reference/src/pages/postgresql_data.rs`
- File: `crates/panproto-reference/src/enumerate/postgresql_data.rs`

## The canonical text reads back as the rows {#L-postgresql-data-3}

### Statement

For a script model $m$ on which $K(m)$ reads, write $C$ for `canonical_text(K(m))`. The route's `write_data` writes the instance it reads from $R(m)$ as exactly $C$, and reading $C$ after the model's DDL gives the same instance.

### Outside the domain

The statement covers the scope of [L-postgresql-data-2](#L-postgresql-data-2). Every other column type, and `write_dump`'s DDL and foreign keys around the rows, round-trip by the fixed tests and on every case of the conformance corpora that reads.

### Definitions used

$C$ is the canonical output defined above, restricted to the model: for `a` and then `b`, when it holds rows, `INSERT INTO t (columns) VALUES` with every column in position order, then one line per row, an integer unquoted, a text quoted with each quote doubled, `NULL` for a null.

### Proof

1. `write_data` orders tables by name and rows by id, lists every column in position order and writes each value as $C$ does: an integer by its decimal text, a text as a standard string, a null as `NULL`. By [L-postgresql-data-2](#L-postgresql-data-2) the instance is $K(m)$, so the texts agree.
2. Every value written is read back before it is written, so reading $C$ gives each cell's value again; the rows of each table appear in id order and tables in name order, which is the reader's numbering, so the ids agree too.
3. The differential checks the text and the reread instance on all 228 scripts of the bounded scope that read.

### Executable counterpart

- Law ID: `postgresql-data.canonical-text`
- Reference: `panproto_reference::pages::postgresql_data::canonical_text`
- Enumerator: `panproto_reference::enumerate::postgresql_data::scripts; bound: scripts=2060`
- Exhaustive differential test: `canonical_text_matches_the_reference_on_every_bounded_script; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_postgresql_data --no-tests=fail -E 'test(=canonical_text_matches_the_reference_on_every_bounded_script)'`
- Reference test: `the_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test postgresql_data --no-tests=fail -E 'test(=the_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/postgresql/data.rs`
- File: `crates/panproto-reference/src/pages/postgresql_data.rs`
