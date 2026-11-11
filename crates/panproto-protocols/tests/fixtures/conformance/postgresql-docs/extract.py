"""Extract the DDL examples of one section of a PostgreSQL SGML source.

Usage: python3 -I extract.py <file.sgml> <section id> <output dir> <prefix>

Every <programlisting> of the section whose text, with markup removed
and entities decoded, starts with CREATE TABLE or ALTER TABLE is written,
in document order, to <output dir>/<prefix>-NN.sql.
"""
import re, sys, html, pathlib
src, section, out, prefix = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
text = open(src).read()
# The section: from the element whose id is `section` to its closing tag.
m = re.search(r'<(sect1|sect2|refsect1)\s+id="%s"' % re.escape(section), text)
tag = m.group(1)
start = m.start()
depth = 0
pos = start
for t in re.finditer(r'<(/?)%s\b' % tag, text[start:]):
    depth += -1 if t.group(1) else 1
    if depth == 0:
        end = start + t.end()
        break
body = text[start:end]
blocks = re.findall(r'<programlisting>(.*?)</programlisting>', body, re.S)
n = 0
pathlib.Path(out).mkdir(parents=True, exist_ok=True)
for b in blocks:
    b = re.sub(r'<[^>]+>', '', b)
    b = html.unescape(b).strip('\n')
    if not re.match(r'\s*(CREATE|ALTER) TABLE', b):
        continue
    n += 1
    open(f'{out}/{prefix}-{n:02d}.sql', 'w').write(b + '\n')
print(n)
