# Apache Avro: schema evolution as migration

[Apache Avro](https://avro.apache.org/) [@avrospec] is a serialisation format with a schema language specified precisely enough to include formal rules for *schema evolution*: the rules by which a reader using one version of a schema can correctly consume data written under an older or newer version. This chapter walks through those rules and maps them onto the migration primitives developed in [Theory morphisms and instance migration](../core/morphisms-and-migration.md). Where Lexicon (in [the previous chapter](./atproto.md)) leaves schema evolution informal, Avro specifies it, which makes the translation into panproto's framework sharper.

The Rust code is in [`panproto_protocols::serialization::avro`](https://docs.rs/panproto-protocols/latest/panproto_protocols/serialization/avro/).

## What Avro specifies

An Avro schema is a JSON document declaring records, enums, arrays, maps, unions, and primitive types (null, boolean, int, long, float, double, bytes, string). A record is a list of named fields, each typed; field types can be primitives, references to other record types, or compositional types built from the above.

Avro's schema evolution specification distinguishes three cases. *Backward compatibility*: a new-version reader can read old-version data (the new schema is a safe extension). *Forward compatibility*: an old-version reader can read new-version data (the new schema is a safe restriction relative to the old). *Full compatibility*: both directions hold.

The specification includes concrete rules for when each case applies. Adding a field with a default value is backward-compatible. Removing a field with a default value is forward-compatible. Renaming a field requires the new name to be recorded as an alias; readers look up fields by alias when the primary name does not match. Changing a field's type is permitted only among compatible type pairs (int to long is fine in one direction; double to float is not).

## Translation to migration

The rules above are not an informal convention; they are a specification panproto can translate directly. Each case becomes a specific kind of migration in the sense of [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

Adding a field with a default value is a theory morphism $f : T_{\text{old}} \to T_{\text{new}}$ whose image extends the source with a new operation, together with a $\Sigma_f$-style pushforward that supplies the default value for every instance. The migration compiles through [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) like any other; the default becomes the lift function's constant output for the added field.

Removing a field with a default value is the inverse: a theory morphism $f : T_{\text{old}} \to T_{\text{new}}$ whose image drops an operation. On the instance side this is a $\Delta_f$-pullback, which the lift implements as a projection that forgets the removed field. The default value is used only on the other side of the migration, when code written against the *new* schema encounters *old* data; the old data still carries the field, and the new schema's absent-field interpretation uses the default.

Renaming a field is a theory morphism that maps the old operation name to the new operation name, with every other part of the theory unchanged. The lift function applies the rename on every record. Avro's aliases are what record the old name so that readers using the new schema can still find the field in old data; panproto's migration compiler treats alias lists as the *symmetric* form of this morphism, where both the old name and the new name resolve to the same underlying operation.

Changing a field type among compatible primitives is a theory morphism whose image alters the target sort of an operation. Avro's rules about which type pairs are compatible correspond to panproto's existence-checking stage ([The restrict/lift pipeline](../core/restrict-lift.md)): a theory morphism that maps int to long succeeds at existence checking, since every int value embeds in the long codomain; a morphism that maps double to float fails, since the inverse embedding is not total. Panproto's diagnostics for such failures reproduce Avro's compatibility categories at the term level.

## Unions

Avro unions are tagged alternatives among several types, with an explicit resolution order for reader-side disambiguation. A union $[A, B, C]$ in Avro becomes a panproto sum sort with three injection operations. The resolution order becomes part of the schema-level equations: the sort's elimination form takes a union value and dispatches on the tag, and the tag's ordering is recorded as a priority a reader applies when two types admit the same JSON representation.

Schema evolution on unions follows the general pattern. Adding a new alternative is a theory morphism extending the disjunction; removing an alternative requires the removed branch to be unreachable in the existing data, which panproto's existence checker verifies by examining the source instance against the reduced sum.

## What Avro leaves to the engine

Two things Avro's specification leaves informal become explicit in panproto's representation.

Avro's "full compatibility" is a conjunction: both backward and forward compatibility hold. Panproto expresses this as a pair of theory morphisms, one in each direction, which together form an isomorphism at the schema level up to the translation of optional defaults. A migration certified full-compatibility in Avro is the round-trippable lens of [Bidirectional lenses](../core/lenses.md) applied to the relevant pair of schemas.

Avro's default-value semantics are stated at the field level. Panproto elevates them to the equation level: a field with a default value becomes an operation whose theory carries an equation saying *when the field is absent in an instance, the operation returns the default*. This equation is evaluated by [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) at record-read time and is part of the schema's validator, not of the lift function alone.

## Closing

The next chapter, [A relational case study](./relational.md), works through a different protocol family: relational database schemas, where the category-theoretic content is larger (the relational model is closer to Spivak's original categorical-database treatment) and the migration primitives are denser.
