# PostgreSQL documentation DDL examples

The cases are the DDL examples of the PostgreSQL 17 documentation, taken
from the `postgres/postgres` repository at tag `REL_17_12` (commit
`702b3dc86d35bcc08c8e23cffa3c4d9fe9d32c85`):

| Prefix | Source file | Section |
|---|---|---|
| `ddl-constraints` | `doc/src/sgml/ddl.sgml` | `ddl-constraints` (5.5 Constraints) |
| `create-table-examples` | `doc/src/sgml/ref/create_table.sgml` | `sql-createtable-examples` (CREATE TABLE, Examples) |

`extract.py` writes them: each program listing of the section that starts
with `CREATE TABLE` or `ALTER TABLE`, markup removed and entities decoded,
is one case, numbered in document order:

```sh
python3 -I extract.py ddl.sgml ddl-constraints cases ddl-constraints
python3 -I extract.py create_table.sgml sql-createtable-examples cases create-table-examples
```

Licence: the PostgreSQL Licence, in `COPYRIGHT` (the repository's
`COPYRIGHT` file at the same commit, unchanged), which grants permission to
copy and distribute provided the copyright notice and its two paragraphs
appear in all copies.

## Data cases

`data-cases/` holds the data examples, from the same commit:

| Case | Source |
|---|---|
| `dml-insert-NN.sql` | `doc/src/sgml/dml.sgml`, section `dml-insert` (6.1 Inserting Data) |
| `tutorial-basics.sql` | `src/tutorial/basics.source`, unedited |

`extract_dml.py` writes the `dml-insert` cases: the section's `CREATE
TABLE` listing followed by each later listing of the section, one case
per listing, numbered in document order:

```sh
python3 -I extract_dml.py dml.sgml data-cases
```
