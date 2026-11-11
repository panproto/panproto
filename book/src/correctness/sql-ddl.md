# SQL DDL

A SQL schema is the most common source of table structure, and reading one into a panproto schema is what lets SQL data be migrated and validated. This page states what the SQL route reads, what it refuses, and the laws its reader satisfies.

The route reads one dialect: the DDL of PostgreSQL 17, as the PostgreSQL 17 documentation defines it (the reader and its corpus follow release 17.12). A DDL script is read as a table, and so is the data it describes: the `sql` protocol declares a table syntactic shape and a table semantic shape, and its foreign keys are schema references between tables. Other dialects are refused by name where their syntax first departs from PostgreSQL's: a MySQL backquoted identifier, a T-SQL or SQLite bracket-quoted identifier, and any statement PostgreSQL does not have. The reader is [`read_ddl`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/sql/fn.read_ddl.html), and [`parse_ddl`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/sql/fn.parse_ddl.html) is the form the protocol registry calls.

## What the reader reads

The reader splits a script into statements as PostgreSQL's lexer does (Sec 4.1 of the documentation): comments, nested block comments, quoted identifiers, standard and `E` string constants with their continuation across a line break, and dollar-quoted strings. Unquoted names fold to lowercase and quoted names keep their bytes. A name may be qualified by the schema `public` and by nothing else. Statements take effect in script order, and each resolves the names it uses against the tables and types declared before it, as PostgreSQL does.

| Statement | What it declares |
|---|---|
| `CREATE TABLE [IF NOT EXISTS]` | A table, its columns, its column and table constraints |
| `ALTER TABLE [IF EXISTS] [ONLY] ... ADD` | A column or a table constraint |
| `CREATE UNIQUE INDEX` on plain columns | A unique key |
| `CREATE TYPE ... AS ENUM` | An enum type |
| `CREATE DOMAIN` | A domain over a built-in type, with `NOT NULL`, `DEFAULT` and `CHECK` |
| `DROP TABLE [IF EXISTS]` | The removal of a table no other table references |

A table is a `table` vertex and a hyper-edge over its columns. A column `c` of table `t` is the vertex `t.c`, joined to `t` by a `prop` edge whose ordering is the column's position, and an array column is an `array` vertex with an `items` edge to its element for each declared dimension. A primary key, a unique constraint and a unique index are a [`Key`](https://docs.rs/panproto-schema/latest/panproto_schema/keys/struct.Key.html) of the table, with role `Primary` or `Unique`, global scope, and SQL key equality on each part (a `character(n)` part ignores trailing spaces). A foreign key is a [`Reference`](https://docs.rs/panproto-schema/latest/panproto_schema/keys/struct.Reference.html) from its table to the key it names, with a `foreign-key` edge between the two tables, `MATCH FULL` read as the `Full` null match, and the shared reference resolution of the [Reference resolution](resolution.md) page resolving it on data. `NOT NULL` and `DEFAULT` are constraints of the column vertex; a `CHECK`, which validation does not evaluate, is kept as its clause on the table vertex, and a referential action other than `NO ACTION` as a `FOREIGN KEY ACTION` constraint of the table. Every expression is kept as the source text that spells it.

Each built-in type the reader carries maps to one vertex kind with its value parameters, set on the element vertex.

| PostgreSQL type | Vertex kind | Value parameters |
|---|---|---|
| `smallint`, `integer`, `bigint` (and `int2`, `int4`, `int8`) | `integer` | `int_bits` 16, 32 or 64 |
| `smallserial`, `serial`, `bigserial` | `integer` | `int_bits`; the column is `NOT NULL` with default `nextval('<table>_<column>_seq'::regclass)` |
| `numeric`, `decimal` | `number` | `precision` and `scale` when declared |
| `real`, `float(1)` to `float(24)` | `real` | none |
| `double precision`, `float`, `float(25)` to `float(53)` | `double` | none |
| `text`, `character varying` | `string` | `max_length` when declared |
| `character(n)` | `character` | `max_length` |
| `boolean` | `boolean` | none |
| `bytea` | `bytes` | none |
| `date` | `date` | none |
| `time without time zone` | `time` | none |
| `timestamp`, `timestamp with time zone` | `timestamp` | `with_time_zone` |
| `uuid`, `json`, `jsonb` | `uuid`, `json`, `jsonb` | none |
| an enum type | `string` | `ENUM` (the labels as a JSON array) and `TYPE` (the type's name) |
| a domain | the base type's kind | the base type's, `TYPE`, and each `DOMAIN CHECK` clause |

An unquoted key or foreign key is named as PostgreSQL names it: `<table>_pkey` for a primary key, `<table>_<columns>_key` for a unique constraint, `<table>_<columns>_idx` for an unnamed unique index, and `<table>_<columns>_fkey` for a foreign key, with a number after the label when that name is taken. A primary key makes its columns `NOT NULL`. A foreign key with no column list references the primary key of its table; with one, it references the primary key, else the first declared unique key, whose columns are exactly those listed, and its columns are listed in the order of that key's parts.

## What the reader refuses

Every refusal is a [`SqlDdlError`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/sql/enum.SqlDdlError.html) naming the byte offset of the token it is about, and the reader stops at the first one in script order. A statement that declares nothing a table schema holds (a function, procedure, view, trigger, rule, aggregate, sequence, extension, schema or language; a non-unique index; a grant, ownership change, comment or session setting; `COPY` and `INSERT` data) is refused by [`parse_ddl`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/sql/fn.parse_ddl.html) with its name, and listed with its offset by `read_ddl` under `OtherStatements::List`. A `SET search_path` naming a schema other than `public` is refused in both modes, since it changes what the names after it denote.

| Feature | Refusal | Reason |
|---|---|---|
| `money`, `interval`, `time with time zone`, `xml`, network, geometric, bit-string, text-search and range types, `bpchar` without a length | `Unsupported`, naming the type | No vertex kind holds their values exactly |
| A type the script did not declare and PostgreSQL does not define | `UnknownType` | PostgreSQL refuses it too |
| Fractional-second precision `timestamp(p)`, `time(p)` | `Unsupported` | The precision would be lost |
| A negative numeric scale, or a scale above the precision | `Unsupported` | The `scale` parameter is a natural number at most the precision |
| `COLLATE` other than `"default"`, `"C"` and `"POSIX"` | `Unsupported` | Key equality would change |
| `GENERATED` and identity columns, column storage options | `Unsupported` | Not carried by the schema |
| `EXCLUDE`, `UNIQUE NULLS NOT DISTINCT`, `MATCH PARTIAL`, deferrable constraints, `NOT VALID`, `NO INHERIT`, a column list on `SET NULL` or `SET DEFAULT` | `Unsupported` | No key or reference form carries them |
| Index parameters, an expression or partial unique index, an index method other than `btree` | `Unsupported` | Not carried by a key |
| `INHERITS`, partitioning, typed tables, `CREATE TABLE AS`, temporary and unlogged tables, table storage parameters, tablespaces | `Unsupported` | The schema has no form for them |
| `DROP TABLE CASCADE`, or dropping a table another table references | `Unsupported` | PostgreSQL refuses the second without `CASCADE` |
| A name qualified by a schema other than `public` or by a database | `Unsupported` | The schema has one namespace |
| An identifier longer than 63 bytes, or containing `.` or `:` | `Unsupported` | PostgreSQL truncates the first; the second would make vertex ids ambiguous |
| A reference to a table not yet declared | `UnknownTable` | PostgreSQL refuses it too |
| A referenced column set with no primary or unique key | `NoMatchingKey` | PostgreSQL refuses it too |
| A second primary key, column, table, type or constraint name | `Duplicate` | PostgreSQL refuses it too |

The reader does not check that the columns of a foreign key have types PostgreSQL can compare with the referenced ones, and it reads a reserved word written as an unquoted column name, both of which PostgreSQL refuses.

## Canonical output

[`emit_ddl`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/sql/fn.emit_ddl.html) writes a schema as canonical DDL: each enum and domain type the columns name, by name; each table by name, its columns in position order with `NOT NULL` and `DEFAULT` after the type, then its primary key, its unique keys by name and its checks, each key a named constraint; and then each foreign key, by table and name, as `ALTER TABLE ... ADD CONSTRAINT`, so tables may reference each other in any order. The route claims no format preservation: comments, layout and the spelling of names and types are not kept, and reading the canonical text gives the same schema. A serial column is written as its integer type with its default, and its sequence is not declared.

## Conformance corpora

The reader is run against two corpora vendored under `crates/panproto-protocols/tests/fixtures/conformance` with their licences and upstream commits: the `CREATE TABLE` and constraint examples of the PostgreSQL 17 documentation (51 cases, from release 17.12, regenerated by the `extract.py` beside them) and the two Sakila sample schemas of jOOQ (`conformance_sql_ddl.rs`). A case passes when the reader reads it with other statements listed, every reference names a key of its target, and its canonical text reads back as the same schema. A case that does not pass is a declared limit in `sql-ddl-limits.txt`, with the refusal class it shows, and the test fails if the list and the failing cases differ in either direction. The reader reads 24 of the 53 cases.

| Limit | Cases | What the reader does |
|---|---|---|
| `partitioning` | 10 | Refuses `PARTITION BY` and `PARTITION OF` |
| `type-interval` | 3 | Refuses the `interval` type |
| `references-another-example` | 3 | Refuses a foreign key to a table another example declares, with `UnknownTable` |
| `elided-example` | 3 | Refuses the `...` the documentation writes for elided columns, with `Syntax` |
| `type-circle` | 2 | Refuses the `circle` type |
| `unique-nulls-not-distinct` | 2 | Refuses `UNIQUE NULLS NOT DISTINCT` |
| `alternative-forms-in-one-example` | 1 | Refuses a second declaration of `distributors`, which the example gives as an alternative, with `Duplicate` |
| `index-storage-parameters` | 1 | Refuses `WITH (fillfactor=70)` |
| `tablespace` | 1 | Refuses `TABLESPACE` |
| `set-null-column-list` | 1 | Refuses a column list on `ON DELETE SET NULL` |
| `type-tsvector` | 1 | Refuses the `tsvector` column of PostgreSQL Sakila's `film` |
| `sqlite-dialect` | 1 | Refuses SQLite Sakila at the `END` of its first trigger body, which is no PostgreSQL statement |

## Column types read as their kinds {#L-sql-ddl-1}

### Statement

For a column type spelling $s$ of the bounded scope, write $T(s)$ for `column_type(s)`. When $T(s)$ reads, the SQL route reads `CREATE TABLE t (c s);` as a column `t.c` with exactly $T(s)$'s number of array dimensions, an element vertex of $T(s)$'s kind with exactly $T(s)$'s value parameters, and exactly $T(s)$'s column constraints. When $T(s)$ refuses with class $r$, the route refuses the script with a `SqlDdlError` of class $r$.

### Outside the domain

Enum and domain types are outside the bounded scope, which has no `CREATE TYPE` or `CREATE DOMAIN` before the table; their reading is checked by the fixed tests in `sql_ddl_reads_postgresql.rs`. Uppercase and mixed-case spellings fold to the lowercase ones before the type is read.

### Definitions used

$T$ reads the names and aliases of PostgreSQL 17 Table 8.1 with the modifiers Sections 8.1 to 8.5 allow: an integer type has the width its name gives, a serial type is the integer of its width, `NOT NULL`, with default `nextval('t_c_seq'::regclass)` (Sec 8.1.4), `numeric(p, s)` takes a precision from 1 to 1000 (Sec 8.1.2), `float(p)` is `real` for $p \le 24$ and `double precision` for $25 \le p \le 53$ (Sec 8.1.3), and a character length is from 1 to 10485760 (Sec 8.3). An array suffix `[]`, `[n]` or `ARRAY` adds a dimension each (Sec 8.15.1). A modifier out of range is `Syntax`, a type of the refusal table above is `Unsupported`, and any other name is `UnknownType`.

### Proof

1. The production reader reads a type by its first word and the words and parenthesized modifiers its grammar allows after it, and the reference reads the same spelling by its name and modifier list; both enumerate the same names, so each spelling of the scope selects the same case in both.
2. For each case, the production reader's kind and parameters are those of `element_kind` in the SQL module, and the reference's are those of its `element` table; the two tables agree row by row with the type table above, which is the case analysis of this step.
3. The array suffix is read after the type in both, one dimension per `[...]` and one for `ARRAY`, and a serial type with a suffix is refused in both.
4. The differential checks the equality of every reading and every refusal class on all 300 spellings: 60 type names, aliases and modifiers, each with no suffix, `[]`, `[3]`, ` array` and `[][]`.

### Executable counterpart

- Law ID: `sql-ddl.column-types`
- Reference: `panproto_reference::pages::sql_ddl::column_type`
- Enumerator: `panproto_reference::enumerate::sql_ddl::type_spellings; bound: spellings=300`
- Exhaustive differential test: `column_types_read_as_the_reference_on_every_bounded_spelling; command: cargo nextest run -p panproto-protocols --test reference_sql_ddl --no-tests=fail -E 'test(=column_types_read_as_the_reference_on_every_bounded_spelling)'`
- Reference test: `column_types_read_as_table_8_1_names_them; command: cargo nextest run -p panproto-reference --test sql_ddl --no-tests=fail -E 'test(=column_types_read_as_table_8_1_names_them)'`
- File: `crates/panproto-protocols/src/database/sql/parse.rs`
- File: `crates/panproto-protocols/src/database/sql/mod.rs`
- File: `crates/panproto-reference/src/pages/sql_ddl.rs`

## Keys and foreign keys resolve as PostgreSQL resolves them {#L-sql-ddl-2}

### Statement

For a script model $m$, write $R(m)$ for `render(m)` and $K(m)$ for `read_script(m)`. When $K(m)$ reads, the SQL route reads $R(m)$ as exactly the tables of $K(m)$: each table's columns in order with their `NOT NULL` flags, its keys with their names, roles and columns, and its references with their names, columns, target table, target key and nullability. When $K(m)$ refuses statement $i$ with class $r$, the route refuses $R(m)$ with a `SqlDdlError` of class $r$ whose offset lies on line $i$.

### Outside the domain

The model's columns are all `integer`, its table key and table foreign key range over the first two columns, and its foreign keys use no `MATCH` clause or referential action; those are checked by the fixed tests in `sql_ddl_reads_postgresql.rs`. Keys added by a later `ALTER TABLE` or `CREATE UNIQUE INDEX` are outside the model, and so is the collision suffix of a generated name, which no script of the model produces.

### Definitions used

$R$ writes one `CREATE TABLE` per line, the tables named `t`, `u`, `v`, `w` and the columns `a`, `b`, `c`, `d`. $K$ follows the `CREATE TABLE` reference page of PostgreSQL 17: a statement declares its columns, then its keys in textual order, a primary key marking its columns `NOT NULL` and a second primary key refused (`Duplicate`), then its foreign keys in textual order. A foreign key names its own table or an earlier one (else `UnknownTable`). With no column list it references the primary key; with one, every listed column must exist (else `UnknownColumn`) and the key is the primary key, else the first declared unique key, whose columns are exactly the listed set; with no such key, or with column counts that differ, it is refused (`NoMatchingKey`). Keys and foreign keys are named as the section above states, and a reference is nullable when one of its columns is not `NOT NULL`.

### Proof

1. The production reader applies each statement's constraints in one pass, keys before foreign keys and each in textual order, which is $K$'s order; the first refusal therefore falls on the same statement and has the same class, since both check the same conditions in the same order (primary key duplication, then for each foreign key the target table, the listed columns, the matching key and the column count).
2. Key names agree: neither side generates a name that collides in the model, so production's numbered suffix never applies, and both form `<table>_pkey`, `<table>_<columns>_key` and `<table>_<columns>_fkey` from the columns as written.
3. The referenced key agrees by step 1 and the shared preference for the primary key. Both pair the referencing and referenced columns as written and list the referencing columns in the key's order, so the reference columns agree.
4. `NOT NULL` agrees because both mark a primary key's columns before resolving foreign keys, and nullability is computed from the final flags in both.
5. The exhaustive differential checks the equality of the readings and of the refused statement and class on all 22,620 scripts of the bounded scope, and the property differential on generated scripts of up to four tables.

### Executable counterpart

- Law ID: `sql-ddl.keys-and-references`
- Reference: `panproto_reference::pages::sql_ddl::render`
- Reference: `panproto_reference::pages::sql_ddl::read_script`
- Enumerator: `panproto_reference::enumerate::sql_ddl::ddl_scripts; bound: scripts=22620`
- Exhaustive differential test: `keys_and_references_match_the_reference_on_every_bounded_script; command: cargo nextest run -p panproto-protocols --test reference_sql_ddl --no-tests=fail -E 'test(=keys_and_references_match_the_reference_on_every_bounded_script)'`
- Property differential test: `randomized_keys_and_references_match_the_reference; command: cargo nextest run -p panproto-protocols --test reference_sql_ddl --no-tests=fail -E 'test(=randomized_keys_and_references_match_the_reference)'`
- Reference test: `keys_and_foreign_keys_resolve_as_create_table_defines_them; command: cargo nextest run -p panproto-reference --test sql_ddl --no-tests=fail -E 'test(=keys_and_foreign_keys_resolve_as_create_table_defines_them)'`
- Randomized input scope: up to four tables of one to four columns, each column with any of the four constraints and any reference, each table of two or more columns with any table key and table foreign key.
- File: `crates/panproto-protocols/src/database/sql/parse.rs`
- File: `crates/panproto-protocols/src/database/sql/mod.rs`
- File: `crates/panproto-reference/src/pages/sql_ddl.rs`
- File: `crates/panproto-reference/src/enumerate/sql_ddl.rs`

## The canonical text reads back as the schema {#L-sql-ddl-3}

### Statement

For a script model $m$ on which $K(m)$ reads, write $C$ for `canonical_text(K(m))`. The SQL route's `emit_ddl` writes the schema it reads from $R(m)$ as exactly $C$, and reading $C$ gives a schema with the same canonical bytes.

### Outside the domain

The statement covers the scope of [L-sql-ddl-2](#L-sql-ddl-2). Types, checks, defaults, enum and domain declarations and referential actions round-trip by the fixed tests in `sql_ddl_reads_postgresql.rs` and on every case of the conformance corpora that reads.

### Definitions used

$C$ is the canonical output defined above, restricted to the model: each table by name, its columns in order as `integer` with `NOT NULL` where flagged, then its primary key and its unique keys by name as named constraints, then each foreign key by table and name.

### Proof

1. `emit_ddl` orders tables by name, columns by their position ordering, keys with the primary key first and then by name, and references by table and id, where the id is `<table>.<name>`; these are the orders of $C$, and each line has $C$'s form, so the texts agree whenever the schema holds $K(m)$, which is [L-sql-ddl-2](#L-sql-ddl-2).
2. Reading $C$ declares the same tables, columns and flags, and every key and foreign key with an explicit name, so the reading is again $K(m)$.
3. The reader adds tables and foreign keys to the schema in name order whatever the script's order, so the adjacency lists, and with them the canonical bytes, of the two schemas are equal.
4. The differential checks the text and the canonical bytes on every script of the bounded scope that reads, and the property differential on generated scripts.

### Executable counterpart

- Law ID: `sql-ddl.canonical-text`
- Reference: `panproto_reference::pages::sql_ddl::canonical_text`
- Enumerator: `panproto_reference::enumerate::sql_ddl::ddl_scripts; bound: scripts=22620`
- Exhaustive differential test: `canonical_text_matches_the_reference_on_every_bounded_script; command: cargo nextest run -p panproto-protocols --test reference_sql_ddl --no-tests=fail -E 'test(=canonical_text_matches_the_reference_on_every_bounded_script)'`
- Reference test: `the_script_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test sql_ddl --no-tests=fail -E 'test(=the_script_scope_has_its_closed_form_count)'`
- File: `crates/panproto-protocols/src/database/sql/mod.rs`
- File: `crates/panproto-reference/src/pages/sql_ddl.rs`
