# I/O

The `IoRegistry` wraps 50 protocol codecs for parsing raw input bytes into schema-conforming instances and emitting instances back to protocol-specific formats.

## Creating a registry

```python
import panproto

io = panproto.IoRegistry()
print(len(io))                # 50
print(io.list_protocols())    # ["amr", "ansible", "arrow", ...]
```

## Parsing

```python
instance = io.parse("openapi", schema, b'{"name": "alice"}')
```

The `parse` method takes a protocol name, a `Schema`, and raw input bytes, and returns an `Instance` (W-type).

## Emitting

```python
output_bytes = io.emit("openapi", schema, instance)
```

## Supported protocols

| Category | Protocols |
|----------|-----------|
| Annotation | brat, conllu, naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf, decomp, ucca, fovea, bead, web_annotation, amr, concrete, nif |
| API | openapi, asyncapi, jsonapi, raml |
| Config | cloudformation, ansible, k8s_crd |
| Data Schema | cddl, bson |
| Data Science | dataframe, parquet, arrow |
| Database | mongodb, dynamodb, cassandra, neo4j, redis |
| Domain | geojson, fhir, rss_atom, vcard_ical, swift_mt, edi_x12 |
| Serialization | avro, flatbuffers, asn1, bond, msgpack_schema |
| Web/Document | atproto, docx, odf |

## Instances

An `Instance` is a W-type (tree-shaped data conforming to a schema):

```python
instance = panproto.Instance.from_json(schema, "root_vertex_id", '{"key": "value"}')
print(instance.node_count)
print(instance.to_json())
d = instance.to_dict()         # raw W-type structure as a Python dict
```

### Validation

`validate` checks the instance against its schema under a protocol. The schema
says which vertices and edges exist; the protocol says what its vertex kinds and
constraint sorts mean, so validation needs both:

```python
protocol = panproto.get_builtin_protocol("atproto")
report = instance.validate(protocol)   # ValidationReport
if not report.passed:
    for message in report.messages:
        print(message)
```

A `ValidationReport` has a `status` of `"passed"`, `"failed"` or `"incomplete"`.
A required diagnostic makes it `"failed"`; otherwise a required check that could
not be decided makes it `"incomplete"`. Advisory entries never change the status.
The structured entries are `diagnostics` (each failed check instance, as a dict
naming the check, its subject and the violation), `incomplete` (each check
instance not decided, with the reason) and `coverage` (one `(check, counts)`
pair per compiled check). `messages` renders the entries that decide the status,
`plan` is the digest of the validation plan, `digest()` is the report's own
identity, and `to_dict()` returns the whole report.
