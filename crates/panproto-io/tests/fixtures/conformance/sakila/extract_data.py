"""Write the bounded subset of the Sakila data that the data conformance
case reads.

Usage: python3 -I extract_data.py <postgres-sakila-schema.sql> <postgres-sakila-insert-data.sql> <output.sql>

The output is, in order: the pg_dump blocks of the schema file that declare
the tables language, country, city, actor and category (each block runs
from its `-- Name: X; Type: T; ...` header to the next header), with their
sequences, primary keys and the foreign key of city to country; then the
`SET` statements that open the data file; then, for each of those tables,
the data file's lines from `ALTER TABLE t DISABLE TRIGGER ALL;` to
`ALTER TABLE t ENABLE TRIGGER ALL;`, in the data file's order. Nothing is
edited.
"""
import re, sys
schema_path, data_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
TABLES = ["language", "country", "city", "actor", "category"]
WANTED = set()
for t in TABLES:
    WANTED |= {(t, "TABLE"), (f"{t}_{t}_id_seq", "SEQUENCE"), (f"{t}_pkey", "CONSTRAINT")}
WANTED.add(("city_country_id_fkey", "FK CONSTRAINT"))

schema = open(schema_path, encoding="utf-8").read()
header = re.compile(r"^--\n-- Name: ([^;]+); Type: ([^;]+);.*\n--\n", re.M)
marks = list(header.finditer(schema))
blocks = []
for i, m in enumerate(marks):
    end = marks[i + 1].start() if i + 1 < len(marks) else len(schema)
    if (m.group(1), m.group(2)) in WANTED:
        blocks.append(schema[m.start():end])
assert len(blocks) == len(WANTED), (len(blocks), len(WANTED))

data = open(data_path, encoding="utf-8").read().split("\n")
opening = []
for line in data:
    if line.startswith("SET "):
        opening.append(line)
    elif line.startswith("SELECT"):
        break
sections = []
for t in sorted(TABLES, key=lambda t: data.index(f"ALTER TABLE {t} DISABLE TRIGGER ALL;")):
    start = data.index(f"ALTER TABLE {t} DISABLE TRIGGER ALL;")
    end = data.index(f"ALTER TABLE {t} ENABLE TRIGGER ALL;", start)
    sections.append("\n".join(data[start:end + 1]))

with open(out_path, "w", encoding="utf-8") as out:
    out.write("".join(blocks))
    out.write("\n".join(opening) + "\n\n")
    out.write("\n\n".join(sections) + "\n")
