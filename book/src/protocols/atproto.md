# ATProto lexicons

[ATProto](https://atproto.com/) is the protocol underlying [Bluesky](https://bsky.app/) and the wider AT network. It defines its schemas in a language called [Lexicon](https://atproto.com/specs/lexicon): JSON documents that declare record types, their fields, and the constraints the fields must satisfy. This chapter walks through how panproto represents a Lexicon as a GAT, what the representation preserves exactly, and where the translation has to absorb Lexicon's informal conventions into explicit equations.

The chapter is a case study rather than a full reference. For the code, see [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/); for the Lexicon specification, see @atproto. The mathematical background is in [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) and [Algebraic and generalised algebraic theories](../foundations/gats.md).

## What a lexicon is

A Lexicon is a JSON document with a tree of definitions. Every record type declared in a lexicon has a name (a reverse-DNS identifier like `app.bsky.feed.post`), a list of fields with types and constraints, and a set of permitted query operations. The primitive field types include strings (optionally with length and pattern constraints), integers, booleans, CIDs (content identifiers), blob references, and nested objects. Lexicons also support arrays (lists of a given type) and unions (tagged alternatives among several record types).

An ATProto schema, in panproto's sense, is a population of records conforming to one or more lexicons. A record in the population has a lexicon identifier naming the type it conforms to, plus field values that satisfy the constraints the lexicon imposes. The data model is tree-shaped: records may contain sub-records through nesting, and cross-record references go through CIDs.

## Translation to a GAT

The theory panproto registers for ATProto has one sort for each primitive type (`String`, `Int`, `Bool`, `Cid`, `Blob`), one sort for each distinct array type (a sort `Array(T)` for each element sort $T$), and one sort for each distinct lexicon record type present in the registered lexicons. Operations translate the field accessors: for a record type $R$ with a field `text : String`, the theory has an operation $\mathsf{text} : R \to \mathsf{String}$. Constraints on field values (maximum length, regex pattern, minimum array size) are encoded as equations in the theory, with the equations expressed in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) terms over the record fields.

A lexicon loaded into panproto therefore becomes a *schema* under this theory: the sorts and operations are fixed by the theory, and the lexicon's specific choices (which fields exist, which constraints apply, which alternatives a union admits) are represented as equations the schema enforces over instances.

## What the translation preserves

Three things transfer cleanly.

Record structure transfers by field-by-field match. Every field in the lexicon becomes an operation in the schema, with the same name and a corresponding target sort. The JSON form `{"type": "object", "properties": {"text": {"type": "string"}}}` produces an operation $\mathsf{text}$ from the containing record sort to $\mathsf{String}$.

Scalar constraints transfer through equations. A `maxLength` constraint on a string field produces an equation that restricts the length of the string operation's value; a regex pattern produces an equation demanding the string match the pattern. Both equations are evaluated in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) against the field's value at instance-build time.

Union types transfer through disjoint sums of sorts. A lexicon union over record types $R_1, \ldots, R_n$ produces a sum sort whose discriminator is the record-type identifier at each union occurrence.

## Where the translation is imperfect

Several aspects of Lexicon do not translate as cleanly.

Cross-lexicon references through CIDs are not enforceable at schema-build time. A field whose value is the CID of a record in a different collection carries no schema-level guarantee that the referenced record actually exists; Lexicon treats CIDs as opaque strings with no referential-integrity constraint beyond optional format checks. Panproto faithfully mirrors this: the theory has a sort $\mathsf{Cid}$ with no cross-sort equations. A developer who wants referential integrity must impose it as a separate constraint checked at [migration time](../core/morphisms-and-migration.md) or at [version-control commit time](../vcs/objects-and-dag.md).

The `unknown` type, which Lexicon permits as a placeholder for fields whose shape is unknown at schema-design time, maps to a sort whose interpretation is opaque. Instances of that sort carry their serialised JSON through the engine without decomposition. Panproto does not complain about this, but operations depending on the shape of an `unknown` value will not have a theory-level specification to check against.

## A concrete example

Consider the `app.bsky.feed.post` lexicon, whose JSON form declares a record with text, creation timestamp, and optional language and reply references. Its translation as a panproto schema has five sorts (the post record itself, plus `String` and `DateTime` and the CID-based reference types) and operations for each field. The equations encode the constraints stated in the lexicon JSON: that `text` is at most 300 characters, that `createdAt` is an ISO 8601 timestamp, that `langs` (when present) is a non-empty array of BCP 47 tags.

A schema loaded from this lexicon is consumed by the ATProto parser registered in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/). The parser reads the lexicon JSON, constructs a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value populated with the sorts, operations, and equations above, and hands it to the engine. The emitter performs the inverse: given a schema, it generates the JSON form that would round-trip back to the same schema.

## Migration across lexicon versions

ATProto lexicons evolve. A field can be added, a constraint can be tightened or loosened, a union can gain or lose an alternative. Each such change is a theory morphism between the two lexicon versions' schemas, in the sense of [Theory morphisms and instance migration](../core/morphisms-and-migration.md). Panproto's migration engine handles Lexicon-to-Lexicon migrations through the same pipeline it uses for any protocol, with no ATProto-specific logic: the engine reads the difference between the two lexicon JSON documents, constructs the theory morphism, applies it through [the restrict/lift pipeline](../core/restrict-lift.md), and produces a migration whose lift function moves records from the old lexicon to the new one.

## Closing

The next chapter, [Apache Avro](./avro.md), works through a serialisation-format protocol whose schema evolution rules are well-specified, and maps those rules onto the migration primitives developed in Part II. The comparison with ATProto is instructive: Avro formalises what Lexicon leaves informal.
