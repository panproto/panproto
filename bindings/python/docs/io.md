# I/O

The `IoRegistry` wraps 76 protocol codecs for parsing raw input bytes into schema-conforming instances and emitting instances back to protocol-specific formats.

## Creating a registry

```python
import panproto

io = panproto.IoRegistry()
print(len(io))                # 76
print(io.list_protocols())    # ["graphql", "openapi", "sql", ...]
```

## Parsing

```python
instance = io.parse("json", schema, b'{"name": "alice"}')
```

The `parse` method takes a protocol name, a `Schema`, and raw input bytes, and returns an `Instance` (W-type).

## Emitting

```python
output_bytes = io.emit("json", schema, instance)
```

## Supported protocols

| Category | Protocols |
|----------|-----------|
| Annotation | brat, conllu, naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf, decomp, ucca, fovea, bead, web_annotation, amr, concrete, nif |
| API | graphql, openapi, asyncapi, jsonapi, raml |
| Config | cloudformation, ansible, k8s_crd, hcl |
| Data Schema | json_schema, yaml_schema, toml_schema, cddl, bson, csv_table, ini_schema |
| Data Science | dataframe, parquet, arrow |
| Database | mongodb, dynamodb, cassandra, neo4j, sql, redis |
| Domain | geojson, fhir, rss_atom, vcard_ical, swift_mt, edi_x12 |
| Serialization | protobuf, avro, thrift, capnproto, flatbuffers, asn1, bond, msgpack_schema |
| Type System | typescript, python, rust_serde, java, go_struct, kotlin, csharp, swift |
| Web/Document | atproto, jsx, vue, svelte, css, html, markdown, xml_xsd, docx, odf |

## Instances

An `Instance` is a W-type (tree-shaped data conforming to a schema):

```python
instance = panproto.Instance.from_json(schema, "root_vertex_id", '{"key": "value"}')
print(instance.node_count)
print(instance.to_json())
errors = instance.validate()   # list[str], empty if valid
d = instance.to_dict()         # raw W-type structure as a Python dict
```
