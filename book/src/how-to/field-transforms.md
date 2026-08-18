# Apply field transforms

A field transform computes a target value that a vertex map alone cannot provide. Each transform is a value-level program in the [expression language](../reference/expression-language.md).

## Prerequisites

A migration mapping between two schemas. The expression language reference for available builtins.

## The task

### What the mapping file can and cannot carry

The mapping JSON consumed by `schema check` is a serialized [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.Migration.html), and what it states is where things go rather than how their values are computed:

```json
{
  "vertex_map": {
    "user": "user",
    "user:first": "user:given_name"
  },
  "edge_map": [],
  "hyper_edge_map": {},
  "label_map": [],
  "resolver": [],
  "hyper_resolver": []
}
```

`Migration` carries an `expr_resolvers` field keyed by the `(src_vertex, tgt_vertex)` pair a resolver bridges. Its values are serialized [`Expr`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.Expr.html) syntax trees rather than expression source, so a JSON string in that position fails to deserialize and `schema check` rejects the file before examining the mapping. The field is carried rather than consumed: [`compose`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.compose.html) composes two migrations' resolvers and the version-control layer hashes them, while `compile` does not copy them onto the [`CompiledMigration`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/struct.CompiledMigration.html) that lift and restrict read. Nothing applies an expression resolver to a record.

Value-level transforms that do run reach the engine as [`FieldTransform`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/enum.FieldTransform.html) entries on the compiled migration, which is the route the next section takes. Their backward direction comes from the lens and protolens layer rather than from the mapping file: pair an `ApplyExpr` field transform with its inverse on the corresponding `Protolens` step, or annotate a coercion on the schema and let the migration compiler emit both directions.

### Split one field into two

A `name` field that becomes `firstName` and `lastName` is the correspondence a vertex map cannot state. A vertex map is a function, so `user.name` has at most one image, and two target fields need two. panproto's morphism class is narrower still. It is the length-1 fragment, where an edge goes to an edge and never to a path or a pair, as the chapter on [migrations as morphisms](../explanation/migrations-as-morphisms.md) sets out. There is no schema-level object here for the search to find, so the correspondence lives at the value level instead.

The mechanism is [`FieldTransform::ComputeField`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/enum.FieldTransform.html), attached to the *parent* vertex rather than to either field, one entry per target key. Its environment binds the parent's `extra_fields` together with the scalar values of the parent's immediate children, so `name` resolves as a plain variable even though `user.name` is a schema-defined child vertex and not an extra field. That environment is built over the *source* instance, which is why the expression still reads `name` after `user.name` has been left out of `surviving_verts`: the vertex has no home in the target and does not survive, and the transform sees it anyway.

```rust
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use panproto_core::expr::{BuiltinOp, Expr, Literal};
use panproto_core::gat::{CoercionClass, Name};
use panproto_core::inst::{CompiledMigration, FieldTransform, parse_json, to_json, wtype_restrict};
use panproto_core::schema::{Protocol, SchemaBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = Protocol {
        name: "demo".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Protocol::default()
    };

    let old = SchemaBuilder::new(&protocol)
        .vertex("user", "object", None::<&str>)?
        .vertex("user.name", "string", None::<&str>)?
        .edge("user", "user.name", "prop", Some("name"))?
        .entry("user")
        .build()?;

    let new = SchemaBuilder::new(&protocol)
        .vertex("user", "object", None::<&str>)?
        .vertex("user.firstName", "string", None::<&str>)?
        .vertex("user.lastName", "string", None::<&str>)?
        .edge("user", "user.firstName", "prop", Some("firstName"))?
        .edge("user", "user.lastName", "prop", Some("lastName"))?
        .entry("user")
        .build()?;

    // `(split name " ")`, the subexpression both computed fields read.
    let parts = || {
        Expr::Builtin(
            BuiltinOp::Split,
            vec![
                Expr::Var(Arc::from("name")),
                Expr::Lit(Literal::Str(" ".into())),
            ],
        )
    };

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("user"),
        vec![
            FieldTransform::ComputeField {
                target_key: "firstName".into(),
                expr: Expr::Builtin(BuiltinOp::Head, vec![parts()]),
                inverse: None,
                coercion_class: CoercionClass::Opaque,
            },
            FieldTransform::ComputeField {
                target_key: "lastName".into(),
                expr: Expr::Builtin(
                    BuiltinOp::Join,
                    vec![
                        Expr::Builtin(BuiltinOp::Tail, vec![parts()]),
                        Expr::Lit(Literal::Str(" ".into())),
                    ],
                ),
                inverse: None,
                coercion_class: CoercionClass::Opaque,
            },
        ],
    );

    // `user.name` is deliberately absent: it has no image in the target.
    let mut surviving_verts = HashSet::new();
    surviving_verts.insert(Name::from("user"));

    let migration = CompiledMigration {
        surviving_verts,
        field_transforms,
        ..CompiledMigration::default()
    };

    let record = serde_json::json!({ "name": "Ada Byron King" });
    let instance = parse_json(&old, "user", &record)?;
    let migrated = wtype_restrict(&instance, &old, &new, &migration)?;
    let json = to_json(&new, &migrated);

    assert_eq!(json["firstName"], "Ada");
    assert_eq!(json["lastName"], "Byron King");

    // The computed values live on the parent, so the migrated instance is a
    // single node with no arcs: no child was materialized for either field.
    assert_eq!(migrated.node_count(), 1);
    assert!(migrated.arcs.is_empty());

    // One token splits into one part, so `lastName` comes out empty and
    // `concat(firstName, " ", lastName)` does not read back the input.
    let single = parse_json(&old, "user", &serde_json::json!({ "name": "Cher" }))?;
    let migrated = wtype_restrict(&single, &old, &new, &migration)?;
    let json = to_json(&new, &migrated);

    assert_eq!(json["firstName"], "Cher");
    assert_eq!(json["lastName"], "");

    Ok(())
}
```

*Listing 1: One source field becoming two target fields, at the value level.*

The listing builds its `CompiledMigration` by hand, which is not how one usually arrives. The supported route from a `Migration` to a `CompiledMigration` is [`compile`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.compile.html), which routes the value-level work it does know about through `op_term_assignments`, the coercions declared on the schema, and leaves `field_transforms` empty. So the two halves of this page do not join up: the mapping file cannot state the split, and the listing cannot be reached from the mapping file. Constructing the compiled form directly is what a caller does today to attach a transform, and `..CompiledMigration::default()` is the spelling to use so that a field added later does not break the call.

Notice the two assertions on the shape of the result. `ComputeField` writes into the parent's `extra_fields`, and `to_json` serializes `extra_fields` after children so that a computed key shadows any child of the same name. The target schema declares `user.firstName` and `user.lastName` as vertices; the migrated instance carries them as keys on the parent object and materializes no child node for either. For a consumer reading JSON, which is the usual case, that is the shape it wanted. For a consumer walking the instance tree looking for a child under the `firstName` edge, it is not there.

### Why the class is `Opaque`

The second half of the listing is the reason. "Cher" splits into a single part, `lastName` comes out empty, and `concat(firstName, " ", lastName)` reads back `"Cher "` rather than `"Cher"`. Splitting on whitespace and joining back is not the identity on strings. A [`CoercionClass`](https://docs.rs/panproto-gat/latest/panproto_gat/sort/enum.CoercionClass.html) is a claim about the round trip and not about the forward direction, so a computation that runs perfectly well forward is still `Opaque` when nothing recovers the input from the output. Where an inverse does exist, supply it in `inverse` and classify `Iso`. Where the value is deterministically re-derivable from data that survives, `Projection` is the class, and the AT-URI decomposition (`repo` into its DID, collection and record key) is the case that fits it: the source field is still there, so the computed parts are derived rather than lost.

### Write expressions that cannot fail

"Take the second token" is the reading most people write first, and it aborts the migration rather than yielding an empty field:

```text
index(split(name, " "), 1)
   ->  field transform on `lastName` failed to evaluate:
       index out of bounds: 1 for list of length 1
```

An expression that can fail on one record takes down the whole restrict for that record rather than just the one field, and `RestrictError::FieldTransformFailed` names the target key that raised. The listing uses `head` and `tail` for that reason. `split` in the expression language is Rust's `str::split`, which yields at least one part for any string whatsoever, so `head` and `tail` cannot hit the empty-list case they would otherwise error on.

Where a guard is genuinely needed, `length` reports how many parts a list has and an `if` on it keeps the index in range. `default` will not serve: it substitutes for a null, and an index past the end raises rather than evaluating to one. Note also that `len` is the *string* builtin and `length` the list one, which is the pairing to check first when an expression type-errors on a `split`.

### From the SDKs

The TypeScript and Python SDKs do not yet expose per-field transforms on the `MigrationBuilder`. To compose a migration with a value-level rewrite, build a `ProtolensChain` directly (`combinators::rename_field`, `elementary::apply_expr`, ...) and call `compile_migration` on it:

```python
import panproto

# `panproto.rename_field`, `add_field`, `remove_field`, `hoist_field`,
# and `pipeline` each return a `ProtolensChain`. Serialize and apply via
# `chain.to_json()` and the lens APIs.
chain = panproto.rename_field("user", "full_name", "full_name", "name")
print(chain.to_json())
```

Neither SDK reaches the `field_transforms` route either. `panproto.compile_migration(migration, src_schema, tgt_schema)` compiles a mapping into the form lift consumes, and what it can carry at the value level is what `compile` can carry: coercions declared on the schema, not per-field expressions supplied in the mapping.

## Verification

```sh
schema check --src schemas/v1.json --tgt schemas/v2.json --mapping migration.json --typecheck
```

`check --typecheck` ensures the transforms type-check against the source and target schemas. Property tests in CI then verify the lens laws on sampled data.

Type checking says nothing about what a computed field costs in the backward direction. The [`Complement`](https://docs.rs/panproto-inst/latest/panproto_inst/complement/struct.Complement.html) a `get` produces records `original_extra_fields`, the node's fields as they stood *before* the transforms ran, so it never contains the computed key. `put` restores from that snapshot, and that is what makes `GetPut` hold: the computed field is discarded rather than inverted, and the source comes back as it was.

The same mechanism explains why an edit to a computed field in the view does not survive the round trip. Backward propagation reads the view's value at the forward target key and writes an inverted value only when the transform carries a usable `inverse`; with none, there is nothing to write and the snapshot stands. A lens is an isomorphism only when every transform in it classifies `Iso`, so a lens carrying this split reports `is_isomorphism() == false`, and `obstruction_to_isomorphism` names the reason.

Read `firstName` and `lastName` as a view of `name` rather than as fields in their own right. Edit `name` and the split follows. Edit the split and the edit is dropped.

## Common mistakes

- Expecting `expr_resolvers` in a mapping file to move data. It deserializes only as an `Expr` syntax tree, and nothing on the lift path reads it whatever it holds. Attach a `FieldTransform` to the compiled migration, or declare a coercion on the schema and let `compile` emit both directions.
- Attaching a `ComputeField` to the field's own vertex. The expression environment is the fiber over a node, so a transform that reads sibling fields belongs on the parent. A transform on `user.name` sees `user.name`'s own children, which for a string vertex is nothing.
- Classifying a lossy computation `Iso` because the forward direction is correct. The class is a claim about the round trip. An `Iso` that does not round-trip turns a lens-law failure into a silent data change.
- Indexing into a split without checking its length. The failure is per-record, and it takes the whole record's migration down with it.
- Using IO or random functions in the expression. The language is bounded-pure; non-deterministic builtins are not exposed.
- Letting the budget exceed. Long string operations on large records can hit the step budget. Expressions that hit the budget raise `ExprError::StepLimitExceeded` at runtime.

## See also

- [Reference: expression language](../reference/expression-language.md) for builtins and types.
- [Build a migration](./build-migration.md) for the surrounding workflow.
- [Find a span between two schemas](./spans.md) for what the search can and cannot state at the schema level.
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md) for why `backward` matters.
