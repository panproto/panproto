"""Runs the Python snippets of docs/io.md and docs/migrations.md that this
change touches, against a small atproto schema pair."""

import json

import panproto

protocol = panproto.get_builtin_protocol("atproto")
src_protocol = tgt_protocol = protocol


def build(nsid, max_length=None):
    b = protocol.schema()
    b.vertex("r", "record", nsid)
    b.vertex("r:body", "object")
    b.vertex("r:body.text", "string")
    b.edge("r", "r:body", "record-schema")
    b.edge("r:body", "r:body.text", "prop", "text")
    if max_length is not None:
        b.constraint("r:body.text", "maxLength", max_length)
    return b.build()


src_schema, tgt_schema = build("local.doc.src"), build("local.doc.tgt", "3")
mb = panproto.MigrationBuilder()
for v in ("r", "r:body", "r:body.text"):
    mb.map_vertex(v, v)
compiled = panproto.compile_migration(mb.build(), src_schema, tgt_schema)
instance = panproto.Instance.from_json(src_schema, "r:body", json.dumps({"text": "hey"}))
long = panproto.Instance.from_json(src_schema, "r:body", json.dumps({"text": "hello"}))

# docs/io.md, Validation
report = instance.validate(protocol)  # ValidationReport
if not report.passed:
    for message in report.messages:
        print(message)
print("io.md:", report, report.status, len(report.coverage), report.plan[:12], report.digest()[:12])

# docs/migrations.md, Lifting instances
lifted = compiled.lift(instance, protocol)
lifted = compiled.lift(instance, src_protocol, tgt_protocol)
print("lift:", lifted.to_json().replace("\n", ""))
try:
    compiled.lift(long, protocol)
except panproto.MigrationError as err:
    if err.report is not None:
        print("lift error:", err.side, err.report.messages)

# docs/migrations.md, Get
view, complement = compiled.get(instance)
restored = compiled.put(view, complement)
print("get/put:", type(complement).__name__, restored.to_json() == instance.to_json())

# docs/migrations.md, Coverage checking
instances = [instance, long]
report = panproto.check_coverage(compiled, instances, src_schema, tgt_schema, protocol)
report2 = panproto.check_coverage(
    compiled, instances, src_schema, tgt_schema, src_protocol, tgt_protocol
)
assert report == report2
print("coverage:", report["total_records"], report["successful"], list(report["failed"][0]["reason"]))
