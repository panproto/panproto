# Explanation

Explanation pages cover *why* the system is the way it is. They are essays, not recipes. Each one is built in two tiers:

- An **In plain terms** opener (3-5 sentences) that names the concept and gives the working-developer analogue. You can read the entire quadrant at this level and come away with a working mental model.
- A formal section that names the underlying construction and points into the reference and semantics pages where it is precisely defined.

The pages here, in increasing order of formality:

| Page | Tier |
|---|---|
| [What panproto solves](./what-panproto-solves.md) | Plain |
| [Schemas as theories](./schemas-as-theories.md) | Plain, with one formal section |
| [Migrations as morphisms](./migrations-as-morphisms.md) | Plain, with one formal section |
| [Lenses and round-trip laws](./lenses-roundtrip.md) | Plain, with the three laws stated formally |
| [Composing protocols by colimit](./protocol-colimits.md) | Plain, with one formal section |
| [Schema version control semantics](./vcs-semantics.md) | Plain, with one formal section |
| [What panproto verifies](./what-is-verified.md) | Catalogue of mechanically-checked properties |
| [Architecture](./architecture.md) | Crate dependency graph and the layering that holds the system together |

The [denotational semantics](./semantics/index.md) cluster is separate and load-bearing. It is the place where the implementation is pinned to a precise mathematical specification: the expression language, the lens DSL, the theory DSL, the pushout-based merge, and protolens composition. Each of those pages still opens with a plain-terms section, but the body is dense.
