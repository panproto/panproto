"""Write the INSERT examples of the PostgreSQL documentation as data cases.

Usage: python3 -I extract_dml.py <dml.sgml> <output dir>

The section `dml-insert` (6.1 Inserting Data) opens with the CREATE TABLE
of the table its examples insert into. Each later <programlisting> of the
section, with markup removed and entities decoded, is written after that
CREATE TABLE to <output dir>/dml-insert-NN.sql, numbered in document order.
"""
import html, pathlib, re, sys
src, out = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-8").read()
start = re.search(r'<sect1 id="dml-insert"', text).start()
body = text[start:text.index("</sect1>", start)]
listings = [
    html.unescape(re.sub(r"<[^>]+>", "", b)).strip("\n")
    for b in re.findall(r"<programlisting>(.*?)</programlisting>", body, re.S)
]
create, examples = listings[0], listings[1:]
assert create.startswith("CREATE TABLE")
pathlib.Path(out).mkdir(parents=True, exist_ok=True)
for n, example in enumerate(examples, 1):
    open(f"{out}/dml-insert-{n:02d}.sql", "w", encoding="utf-8").write(f"{create}\n{example}\n")
print(len(examples))
