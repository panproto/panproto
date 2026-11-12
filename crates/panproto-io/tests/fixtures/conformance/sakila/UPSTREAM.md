# Sakila sample database schemas and data

Fetched from `github.com/jOOQ/sakila` at commit
`aed53ce65404eac184f4134f34239a08c464df77`.

| File | Source path |
|---|---|
| `sakila-postgres.sql` | `postgres-sakila-db/postgres-sakila-schema.sql` |
| `sakila-sqlite.sql` | `sqlite-sakila-db/sqlite-sakila-schema.sql` |
| `sakila-data-subset.sql` | a bounded subset of `postgres-sakila-db/postgres-sakila-schema.sql` and `postgres-sakila-db/postgres-sakila-insert-data.sql` |

The insert-data file is 7,963,146 bytes (sha256
`5abfb27721f56443dcd67b0fe4313c66c2ceca5e552fc1e31b2fb289157ff2e9`), too
large to vendor whole. `extract_data.py` writes the subset, 112,823
bytes: the schema file's pg_dump blocks for the tables `language`,
`country`, `city`, `actor` and `category` (each table, its sequence, its
primary key, and the foreign key of `city` to `country`), the `SET`
statements that open the data file, and the data file's 931 rows of those
five tables, every line unedited:

```sh
python3 -I extract_data.py sakila-postgres.sql postgres-sakila-insert-data.sql sakila-data-subset.sql
```

Licence: BSD 2-Clause, in `LICENSE` (the repository's `LICENSE` file at
the same commit).
