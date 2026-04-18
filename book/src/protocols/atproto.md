# ATProto lexicons

The protocol underlying [Bluesky](https://bsky.app/) and the wider AT network is called [ATProto](https://atproto.com/), and the schemas under it are written in a language called [Lexicon](https://atproto.com/specs/lexicon): JSON documents declaring record types, their fields, and the constraints the fields must satisfy. Reading the Lexicon specification and implementing a protocol for it in panproto was an interesting exercise because Lexicon leaves rather a lot to convention. Where most serialisation formats have formal rules for schema evolution, Lexicon has a specification that is less formal than those rules would like it to be and more formal than a casual reader might expect. The translation panproto performs has to absorb the informality into explicit equations, and the chapter is about where the translation is clean and where it is not.

The code lives in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/); the mathematical background lives in [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) and [Algebraic and generalised algebraic theories](../foundations/gats.md).

## What a lexicon is

A Lexicon is a JSON document with a tree of definitions. Every record type declared in a lexicon carries a name — a reverse-DNS identifier like `app.bsky.feed.post` — along with a list of fields with types and constraints, and a set of permitted query operations. The primitive field types are strings (optionally constrained by length or regular expression), integers, booleans, content identifiers, blob references, and nested objects. Arrays hold lists of a given type, and unions are tagged alternatives among several record types.

An ATProto schema, as panproto sees it, is a population of records conforming to one or more lexicons. A record names the lexicon it conforms to, and its field values are required to satisfy the constraints that lexicon imposes. The data model is tree-shaped — records may contain sub-records through nesting — and cross-record references go through content identifiers rather than in-line pointers.

## Translation to a GAT

Panproto registers a theory for ATProto with one sort for each primitive type (`String`, `Int`, `Bool`, `Cid`, `Blob`), one sort for each array type (`Array(T)` per element sort $T$), and one sort for each lexicon record type present in the loaded lexicons. Operations translate the field accessors: for a record type $R$ with a field `text : String`, the theory has an operation $\mathsf{text} : R \to \mathsf{String}$. Constraints on field values — maximum length, regular-expression pattern, minimum array size — are encoded as equations in the theory, stated in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) over the record's fields.

A loaded lexicon becomes a schema under this theory. The sorts and operations are fixed by the theory; the lexicon's specific choices — which fields exist, which constraints apply, which alternatives a union admits — are the schema's choices, represented as equations the schema enforces over instances.

## What the translation preserves

Record structure transfers field by field. Every field in the lexicon becomes an operation in the schema, with the same name and a corresponding target sort. The JSON form

```json
{"type": "object", "properties": {"text": {"type": "string"}}}
```

produces an operation $\mathsf{text}$ from the containing record sort to $\mathsf{String}$, and the schema carries the operation as one of its interpretations.

Scalar constraints become equations. A `maxLength` constraint on a string field produces an equation restricting the length of the string operation's value; a regex pattern produces an equation demanding the string match the pattern. Both equations are evaluated in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) against the field's value at instance-build time, and an instance that violates an equation is rejected by the validator.

For union types, a lexicon union over record types $R_1, \ldots, R_n$ produces a sum sort whose discriminator is the record-type identifier at each occurrence in the schema.

## Where the translation is imperfect

The translation is not always clean. Three places where panproto accepts looseness rather than forcing the schema into a stricter form than the Lexicon spec supports are worth naming.

Cross-lexicon references via CIDs are not enforceable at schema-build time. A field whose value is the CID of a record in a different collection carries no schema-level guarantee that the referenced record exists; Lexicon itself treats CIDs as opaque strings with no referential-integrity constraint beyond optional format checks. Panproto faithfully mirrors this: the sort $\mathsf{Cid}$ has no cross-sort equations in the derived theory. A developer who wants referential integrity has to impose it as a separate constraint, checked at migration time or at version-control commit time.

The `unknown` type, which Lexicon permits as a placeholder for fields whose shape is not yet fixed, maps to a sort whose interpretation is opaque. Instances of that sort carry their serialised JSON through the engine without decomposition. Panproto does not complain about this, but operations depending on the shape of an `unknown` value will not have a theory-level specification to check against.

Certain Lexicon constraints — referential ones over CIDs, some more elaborate validation rules — exceed what `panproto-expr`'s equations can faithfully represent. Panproto handles the constraints that map cleanly onto its expression language and flags the rest as unchecked. A schema under the ATProto protocol may therefore pass panproto's validator while failing a Lexicon constraint a dedicated Lexicon validator would catch; in production use, both validators should run.

## A concrete example

The `app.bsky.feed.post` lexicon declares a record with text, a creation timestamp, and optional language and reply references. Its translation is a schema with five sorts — the post record itself, plus `String`, `DateTime`, and the CID-based reference types — and operations for each field. The equations encode the constraints the lexicon JSON states: `text` is at most 300 characters, `createdAt` is an ISO 8601 timestamp, `langs` (when present) is a non-empty array of BCP 47 tags.

The parser registered in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/) reads the lexicon JSON, constructs a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) populated with the sorts, operations, and equations above, and hands it to the engine. The emitter does the inverse: given a schema, it regenerates the JSON form that would round-trip back to the same schema.

## Migration across lexicon versions

Lexicons evolve. A field can be added, a constraint can be tightened or loosened, a union can gain or lose an alternative. Each such change is a theory morphism between the two lexicon versions' schemas, in the sense of [Theory morphisms and instance migration](../core/morphisms-and-migration.md). Panproto's migration engine handles lexicon-to-lexicon migrations through the same pipeline it uses for any protocol, with no ATProto-specific logic: the engine reads the difference between the two lexicon JSON documents, constructs the theory morphism, applies it through the restrict/lift pipeline, and produces a migration whose lift function carries records from the old lexicon to the new one.

## Further reading

@atproto is the authoritative specification, with the Lexicon spec specifically at https://atproto.com/specs/lexicon. The [Bluesky](https://bsky.app/) application is the largest current deployment and a useful source of example lexicons.

## Closing

The next chapter turns to Apache Avro, whose schema-evolution rules are specified formally enough to map onto panproto's migration primitives without the informalities Lexicon forces.
