# Integrate with language models

panproto separates *perception* (turning input into a schema-instance) from *reasoning* (transforming and verifying that instance). A small open-source language model is well suited for the perception layer: parsing freeform input into a structured schema. panproto handles the reasoning layer, where every transformation is verified.

## Prerequisites

A language model accessible via API or local inference. A target schema for the structured output.

## The task

### Use the LM to fill an instance

Prompt the model with the target schema (read it from disk and serialize as JSON) and the freeform input. The model returns a candidate instance.

```sh
cat schemas/user.json
# pipe the schema and the freeform input to your model
# capture the model's structured output as candidate.json
```

### Validate the schema and parse the candidate

`schema validate` checks a *schema* against a protocol's GAT, not an instance:

```sh
schema validate --protocol atproto schemas/user.json
```

To check that the model's candidate **instance** conforms to the schema, parse it through the language SDK and let the parse fail loudly. For example, with the Python binding:

```python
import panproto

proto = panproto.get_builtin_protocol("atproto")
schema = ... # load the target schema
reg = panproto.IoRegistry()
inst = reg.parse("atproto", schema, candidate_bytes)
```

A `panproto.IoError` (or `panproto.ParseError`) means the model produced an instance that does not conform. Retry with the error message in the prompt.

### Apply only verified transformations

Once you have a valid instance, every subsequent transformation is a panproto migration: deterministic, type-checked, lens-law-verified. The LM does not get to invent new fields after validation.

## Verification

The validation pass against the target schema is the perception-layer check. The migration's existence check (`schema check`) and lens-law tests are the reasoning-layer check. Both must pass for the pipeline to succeed.

## Common mistakes

- Skipping the validation pass. LM output that *looks* well-formed is often subtly off (extra fields, wrong types, omitted required structure). Validate before reasoning.
- Letting the LM produce migrations directly. Migrations are mathematical objects with verifiable properties; LM-authored migrations skip the verification.

## See also

- [Define a schema](./define-schema/index.md).
- [Build a migration](./build-migration.md).
- [Query instances](./query-instances.md).
