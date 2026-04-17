# FHIR as a document case study

[FHIR](https://www.hl7.org/fhir/) [@fhir] (Fast Healthcare Interoperability Resources) is an HL7 standard for exchanging healthcare records. The columnar-storage and record-shredding ideas that underlie [Apache Parquet](https://parquet.apache.org/) [@parquet], which appears elsewhere in Part IV, originate in @melnik2010dremel. Its schemas are JSON or XML documents with deeply nested objects, elaborate cardinality constraints, terminology bindings, and conformance profiles that layer on top of a base resource to narrow the constraints further. FHIR stresses panproto's framework in a way relational schemas do not: nesting is arbitrary in depth, constraints are frequent, and the versioning cadence (R4, R4B, R5, R6) is both regular and non-backward-compatible across some resources.

This chapter follows FHIR through panproto's representation and points at the places where the translation is exact and the places where panproto accepts looseness rather than demand fidelity to every corner of the specification. The code is in [`panproto_protocols::domain::fhir`](https://docs.rs/panproto-protocols/latest/panproto_protocols/domain/fhir/).

## What a FHIR resource looks like

FHIR organises healthcare data into *resources*, each a typed record whose shape is defined by the specification. A `Patient` resource has fields for name, birth date, gender, active status, and identifiers; a `Observation` resource has fields for the observed value, the subject, the time of observation, and the code that classifies what was measured. Fields can be scalars, nested objects, arrays, or references to other resources through a structured URL. Every field has a declared cardinality (min and max occurrences) and, where applicable, a binding to a terminology (a code set from SNOMED CT, LOINC, or another standard vocabulary).

A FHIR *profile* further constrains a base resource. A US Core Patient profile, for example, is a Patient with additional constraints: the identifier field must be non-empty, the name field must include at least one family name, the birth date must be present. Profiles are themselves FHIR documents in a language called StructureDefinition, and a profiled resource is one that satisfies both the base resource's constraints and the profile's additional ones.

## Translation to a GAT

The base FHIR resource types become sorts in panproto's FHIR theory: one sort per resource type (`Patient`, `Observation`, `Procedure`, and the rest), plus one sort per nested complex type (`HumanName`, `Address`, `Period`, and so on), plus sorts for the primitive types FHIR defines (`string`, `boolean`, `dateTime`, `code`, `uri`). Operations are the field accessors: for a Patient resource with a `name` field of type `HumanName`, the theory has an operation $\mathsf{name} : \mathsf{Patient} \to \mathsf{Array}(\mathsf{HumanName})$, since a patient may have multiple names.

Cardinality constraints become equations. A field with minimum cardinality 1 produces an equation demanding the array be non-empty; a field with maximum cardinality 1 produces an equation demanding the array have at most one element (in practice, panproto uses a scalar sort rather than a singleton-array sort in the latter case). Terminology bindings become equations that require a code value to belong to a given value set, checked by [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) against a value-set lookup table the schema carries alongside.

Profiles become theory morphisms. A US Core Patient profile is a theory morphism from the base FHIR theory into a narrower theory whose equations include the extra constraints. Applying the morphism is the $\Delta_f$-pullback: an instance under the narrower profile is an instance under the base with additional guarantees. Going the other way requires a $\Pi_f$-style filter that keeps only the base-resource instances satisfying the profile's constraints.

## Cross-resource references

FHIR references are structured identifiers of the form `ResourceType/id`, sometimes with a version anchor. A reference field in a FHIR resource has a known set of target resource types. Panproto represents reference fields as operations whose codomain is a sum sort over the allowed target types, with the sum-sort tag carrying the resource type of the actual referent.

Referential integrity is not enforced at schema-build time by FHIR itself; a reference may point at a resource that does not exist in the local dataset. Panproto mirrors this: the sum-sort tag records the *claimed* target type, and the equations do not require the target to be present. A developer who wants referential integrity at migration time applies a separate consistency check, built on top of [the instance functor](../core/schemas-as-instances.md).

## Where the translation is imperfect

FHIR extensions are open-ended. Every resource in FHIR may carry extension fields whose shape is not declared in the base resource's specification. Panproto represents extensions as an opaque JSON-valued sort that preserves them across migrations without decomposition. A migration that needs to transform extension contents must do so through [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/)'s field-transform mechanism applied to the extension sort directly.

FHIRPath invariants (expressions attached to resources) form a rich constraint language that panproto's equations cannot faithfully encode in full generality. Panproto handles the invariants that map cleanly onto [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) (presence checks, cardinality constraints, simple relational comparisons) and flags the rest as unchecked. A schema under the FHIR protocol may pass panproto's validator while failing a complex FHIRPath invariant the specification requires; for production use, a FHIR-specific validator should run alongside panproto's.

Resource versioning is per-resource. Some resources (`Patient`, `Observation`) have evolved in backward-compatible ways across R4, R4B, R5, and R6; others have had breaking changes. Panproto represents each version's schema as a distinct schema under the FHIR protocol, and migrations between versions go through the standard pipeline. The engine does not itself determine which version pairs are backward-compatible; the developer marks this explicitly in the migration declaration.

## Closing

The next chapter, [Tree-sitter and full-AST parsing](./tree-sitter.md), closes Part IV with the auto-derivation mechanism: how panproto produces protocols from [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars for the 248 programming languages that do not have hand-written protocol definitions.
