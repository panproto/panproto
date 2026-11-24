# Lens DSL

A lens document describes a bidirectional transformation as a JSON, YAML or Nickel value: a list of steps, a list of rewrite rules, a composition of other lenses, or a symmetric pair of step lists. This page states what the schema-free compiler does with such a value: which values the document grammar admits, how each body compiles to an ordered sequence of stages, how parts compose vertically and horizontally, and where in the source text each refusal points. The meaning of each construct is set out in the [Lens DSL semantics](../explanation/semantics/lens-dsl.md) page; the laws below state that the compiler computes exactly that meaning on a stated finite scope, against an independent reference, `panproto_reference::pages::lens_dsl`.

The reference starts from a document value, the reference crate's own JSON value; Nickel evaluation and YAML reading produce that value and are outside the laws. It is transcribed from the semantics page and shares no code with production. Two judgements belong to other components and are parameters of the reference: whether a string is an expression of the [expression language](../explanation/semantics/expression-language.md), and whether a declared coercion class survives its sampled round-trip laws. The differentials pass production's parser and sample check, so the laws hold relative to those two judgements. The reference answers "outside the domain" for one construction only, a reference to a document the grammar refuses, which production cannot express since it resolves references against documents it has already read; no document of the scopes below makes one.

A compiled lens is compared at the level of its protolenses. Each stage's chain is compared protolens by protolens: a fused chain or a horizontal composite is read as the sequence of its elementary transforms, a scoped protolens as its focus with its own sequence, and each elementary transform is compared field by field together with the residue its complement keeps (the default of an added sort, the target of a dropped edge, the class of a coercion). Each stage's value-level transforms are compared by anchor, an expression by the expression it parses to. What a compiled lens does to an instance, its instantiation at a schema and its `get` and `put`, is not compared on this page: the [Lenses](lenses.md) page proves instantiation, `get` and `put` against its reference for every elementary protolens kind the step alphabet compiles to without a value-level transform ([L-lenses-21](lenses.md#L-lenses-21)), while protolenses carrying a value-level transform, fused chains of several kinds and scoped protolenses have no instance-level reference yet.

Every scope compiles at the body vertex `body`, and a composition resolves its references against a library of eleven documents: a step list, a horizontal composition of two parts, a vertical composition of a step and a reference to that horizontal one, a one-part horizontal composition, a symmetric body, a document refused for its expression, two documents referencing each other, an `auto` body, a step list with a directed equation, and a document the grammar refuses, which only the reference's own tests name.

## Lens documents follow the document grammar {#L-lens-dsl-1}

### Statement

For a JSON text $d$ of the grammar scope, write $G(d)$ for `check_document` applied to the reference's reading of $d$. When $G(d)$ accepts, `read_source(d, Json)` returns a document. When $G(d)$ refuses with a path $p$ and a problem $q$, `read_source` refuses with `LensDslError::Spanned` around `LensDslError::Shape` with path $p$ and the problem $q$, and its span is the reference's location of $p$ in $d$.

### Outside the domain

None within the scope: the grammar is total on JSON values. Duplicate member names are read as the last occurrence by both.

### Definitions used

$G$ is the grammar of the semantics page written as field tables. The root must be an object carrying exactly one body key (refused with `NoBody` or `SeveralBodies`, the keys in grammar order, at the root); its table is the header (`id`, `description`, `source`, `target`) followed by the body's own table. An object is checked for its least unknown member name, then field by field in table order, a missing required field refused at the object; a map's members are checked in name order and an array's elements in order. A variant object (a step, a lens reference) is refused at its least member naming no variant, then with `NoVariant` or `SeveralVariants` at the object. A tagged object (an attribute operation, a hint constraint, a preference predicate) is refused for a missing tag at the object and for an unknown tag at the tag. A natural number is a whole number written without fraction or exponent and below $2^{32}$. The scope takes six base documents, one per body, which together use every member of the grammar and every step variant, and replaces each of their values in turn by each of nine values (`null`, `true`, `1`, `1.5`, `"x"`, `[]`, `{}`, the value wrapped in an array, the value wrapped in an object under an unknown member): $232 \cdot 9 = 2{,}088$ documents over the 232 values of the six bases. To these it adds each base with one object member removed (187 documents) and with an unknown member added to one object (78 documents), $2{,}353$ in all.

### Proof

1. Production's `read_document` and the reference's `check_document` check the same field tables in the same order, and neither depends on the order of members in the source: both refuse the least unknown member name by byte order and visit a map's members by name.
2. Both count the body keys present before reading any member, so `NoBody` and `SeveralBodies` agree, with the keys in grammar order.
3. Each leaf check (string, boolean, number, natural number, choice, string or object, nullable value) refuses the same JSON values, and both treat a variant object and a tagged object in the order the definitions state, so the first refused value and its problem agree.
4. A value the grammar admits deserializes into the serde types, so production returns a document exactly when the reference accepts; the differential checks this, the refusal, its path and its span on every document of the scope.

### Executable counterpart

- Law ID: `lens-dsl.document-grammar`
- Reference: `panproto_reference::pages::lens_dsl::check_document`
- Enumerator: `panproto_reference::enumerate::lens_dsl::shape_documents; bound: documents=2353`
- Exhaustive differential test: `the_grammar_refuses_as_the_reference_on_every_mutated_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=the_grammar_refuses_as_the_reference_on_every_mutated_document)'`
- Reference test: `the_lens_grammar_refuses_in_field_table_order; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=the_lens_grammar_refuses_in_field_table_order)'`
- File: `crates/panproto-lens-dsl/src/shape.rs`
- File: `crates/panproto-dsl-eval/src/shape.rs`
- File: `crates/panproto-reference/src/pages/lens_dsl.rs`

## A step list compiles to one stage per step {#L-lens-dsl-2}

### Statement

For a document $d$ of the steps scope or the symmetric scope, write $C(d)$ for `compile_document` of the reference. When $C(d)$ accepts with stages $\sigma_1, \ldots, \sigma_n$, `compile_source(d, Json)` accepts with $n$ stages, the $i$-th of which has the chain and the value-level transforms of $\sigma_i$ in the sense stated at the head of the page; its flat `chain` is the concatenation of the stages' chains, its flat `field_transforms` gathers their transforms by anchor in stage order, and a symmetric body's two legs are the reference's. When $C(d)$ refuses at path $p$ with refusal $\rho$, production refuses at $p$ with the refusal of the same class: `ExprParse`, `TermParse`, `DefaultValue`, `CoercionNotHonest`, `ValueStepInSymmetricLeg` or `NotInvertible`.

### Outside the domain

`auto` and `from_diff` bodies are refused by the schema-free compiler with `AutoRequiresSchemas` at the body key; their generation from schemas is not compared. `LensDslError::Fuse` cannot arise from a document: every elementary protolens has the identity as its source, a horizontal composite's source is $\mathrm{Id} \circ \mathrm{Id}$, which is the identity, so adjacent protolenses always compose (step 4 below). A refused expression is compared by its class and path, not by the parser's message.

### Definitions used

A step compiles at the body vertex $b$ as the semantics page states: `remove_field n` drops the sort `b.n`; `rename_field` relabels the edge from $b$ to `b.old`; `add_field` adds the sort `b.n` with its default and a `prop` edge labelled `n`, and a projection computing `n` when it has an `expr`; `apply_expr` and `compute_field` are value-level transforms at $b$, with class `projection` when none is declared; `hoist_field` adds the operation `child : parent -> child` and drops the intermediate; `nest_field` adds the intermediate without a default, the two edges through it (labelled by the intermediate and the child when no label is given) and drops the direct edge; `scoped` compiles each inner step at the focus and maps each of its stages' fused chains over the focus; every other step is its one elementary protolens. A step's checks run in member order: the default (refused when it holds a whole number past `i64` or contradicts its declared kind, `null` always admitted), then `expr`, then `inverse`, then the coercion's sampled laws; an equation's left side before its right. A term is `ident` or `ident(term, ..., term)` with white space between tokens, so `f(x,)` is refused. Directed equations become one protolens each of a final stage, checked `impl`, `inverse`, `lhs`, `rhs` and then their sampled laws. `invertible: true` is refused when a protolens of a stage or of a symmetric body's right leg, or a value-level transform, is lossy: a protolens is lossless when it renames a sort, an operation or an edge, adds or drops an equation, pulls back, coerces with class `iso` or is a directed equation with an inverse, and a scoped protolens, a fusion or a horizontal composite is lossless when every part is; a value-level transform is lossless when its class is `iso`. A symmetric body compiles each leg at its focus (the body vertex when absent or empty) and refuses a leg step that carries a value-level transform. The steps scope is every list of at most two steps of a 36-step alphabet (every step kind, and each place a step is refused), with `invertible` absent or `true`, and with no directed equation or one of four (an honest lossless one, an honest lossy one, one whose left side is no term, a dishonest one): $(1 + 36 + 36^2) \cdot 2 \cdot 5 = 13{,}330$ documents. The symmetric scope is every pair of 8 legs with 3 focus values and 2 `invertible` values: $8^2 \cdot 3 \cdot 2 = 384$ documents.

### Proof

1. Both compile the steps in order and stop at the first refusal; within a step both check its members in the order the definitions state, so the first refusal and its path agree.
2. Each step kind yields the same elementary protolenses in the same order: the reference's table and production's step compiler map each variant to the same constructors with the same arguments, and the complement each constructor keeps is the residue the reference records.
3. A step yields one stage, except `scoped`, which yields one per inner stage, and both give a `scoped` step with no inner step no stage; the flat summaries are projections of the stages in both.
4. A fused chain's source is its first protolens's source. Every elementary protolens has the identity as its source, a scoped one too, and the horizontal composite of protolenses with identity sources has source $\mathrm{Id} \circ \mathrm{Id} = \mathrm{Id}$, which production writes as the identity; so the composability predicate holds between any two protolenses a document compiles to, and fusion never refuses.
5. Losslessness is decided by the same recursion over complements in both, and both visit the stages and then the right leg, so `NotInvertible` is returned on the same documents.
6. The exhaustive differentials check the agreement on all 13,330 step documents and 384 symmetric documents, and the property differential on step lists of up to eight steps.

### Executable counterpart

- Law ID: `lens-dsl.step-compilation`
- Reference: `panproto_reference::pages::lens_dsl::step_stages`
- Reference: `panproto_reference::pages::lens_dsl::compile_document`
- Enumerator: `panproto_reference::enumerate::lens_dsl::step_documents; bound: documents=13330`
- Enumerator: `panproto_reference::enumerate::lens_dsl::symmetric_documents; bound: documents=384`
- Exhaustive differential test: `step_lists_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=step_lists_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `symmetric_bodies_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=symmetric_bodies_compile_as_the_reference_on_every_bounded_document)'`
- Property differential test: `longer_step_lists_compile_as_the_reference; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=longer_step_lists_compile_as_the_reference)'`
- Reference test: `a_step_list_compiles_to_one_stage_per_step; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=a_step_list_compiles_to_one_stage_per_step)'`
- File: `crates/panproto-lens-dsl/src/steps.rs`
- File: `crates/panproto-lens-dsl/src/compile.rs`
- File: `crates/panproto-lens/src/protolens.rs`
- File: `crates/panproto-reference/src/pages/lens_dsl.rs`

## Rules expand to steps in a fixed order {#L-lens-dsl-3}

### Statement

For a document $d$ of the rules scope, when the reference's compilation accepts with stages $\sigma_1, \ldots, \sigma_n$, `compile_source(d, Json)` accepts with the same stages in the sense of [L-lens-dsl-2](#L-lens-dsl-2); when it refuses at $p$, production refuses at $p$ with `RuleCompile`, `DefaultValue` or `NotInvertible`.

### Outside the domain

A `map_attr_value` operator that names no operation, or whose `value` has the wrong type, is refused by the grammar ([L-lens-dsl-1](#L-lens-dsl-1)), so `RuleCompile` arises from a malformed template only.

### Definitions used

A rule expands, in this order, to a `rename_sort` from its pattern name to a literal new name that differs from it (a projection computing `name` for a template), one `rename_field` per `rename_attrs` entry, one `remove_field` per `drop_attrs` entry, one `add_field` per `add_attrs` entry and one `apply_expr` per `map_attr_value` entry, each map in key order; a rule without a replacement, or with `replace: null`, is a `drop_sort` of its pattern name. An `add_attrs` value's kind is `integer` for a whole number `i64` holds, `number` for any other number, `boolean`, `string` (also for `null`), `array` or `object`. A template is literal text with interpolations `{v}`, each rendered with `int_to_str`; one that is never closed or names no variable is refused at `replace/name`, and a refusal of an expanded step is located at the rule member it came from. After every rule come a stage keeping every replacing rule's `keep_attrs`, and under `passthrough: drop` a stage keeping the name each replacing rule keeps (its literal new name, else its pattern name), each only when non-empty. The scope is every list of at most two of 12 rules (every replacement member, every attribute operator, a malformed and an empty template, a default past `i64`), with 3 `passthrough` values and 2 `invertible` values: $(1 + 12 + 12^2) \cdot 3 \cdot 2 = 942$ documents.

### Proof

1. Both expand a rule completely before compiling its steps, so a template refusal precedes any refusal of the rule's other members, and both compile the expanded steps by [L-lens-dsl-2](#L-lens-dsl-2).
2. Both iterate the attribute maps in key order, so the stages agree in order; the expression an attribute operator denotes is the same in both, an operand kept as the whole number or double it is.
3. Both build the keep stages from the replacing rules in rule order and add them only when non-empty.
4. The exhaustive differential checks the agreement on all 942 documents.

### Executable counterpart

- Law ID: `lens-dsl.rule-compilation`
- Reference: `panproto_reference::pages::lens_dsl::rules_stages`
- Enumerator: `panproto_reference::enumerate::lens_dsl::rule_documents; bound: documents=942`
- Exhaustive differential test: `rules_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=rules_compile_as_the_reference_on_every_bounded_document)'`
- Reference test: `rule_maps_expand_in_key_order; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=rule_maps_expand_in_key_order)'`
- File: `crates/panproto-lens-dsl/src/rules.rs`
- File: `crates/panproto-reference/src/pages/lens_dsl.rs`

## Vertical composition is concatenation {#L-lens-dsl-4}

### Statement

For a document $d$ of the composition scope in mode `vertical`, when the reference accepts with stages $\sigma$, production (`compile_with_refs` over the library) accepts with $\sigma$; when it refuses at $p$, production refuses at $p$ with the same class: a part's own refusal, `UnresolvedRef` or `SymmetricRefInDirectionalCompose` naming the same document, `RefCycle` with the same chain, `InRef` carrying the referenced document's refusal at the same path of that document, or `NotInvertible`. Moreover, for step lists $a$, $b$, $c$ of at most one step each, $\mathsf{compile}(a \mathbin{+\!\!+} b) = \mathsf{compile}(a) \mathbin{;} \mathsf{compile}(b)$, the composition $[a, b]$ compiles to the same stages, $[\mathsf{ref}(a ; b), c]$, $[a, \mathsf{ref}(b ; c)]$ and $[a, b, c]$ compile to the same stages, and empty parts and the empty composition contribute nothing, whenever the parts compile.

### Outside the domain

A reference to a document the grammar refuses, which the reference answers as outside its domain and production cannot express; the scope makes none.

### Definitions used

A part is an inline step list, compiled at the body vertex by [L-lens-dsl-2](#L-lens-dsl-2), or a reference resolved transitively: a reference to a document open along the chain of references (the composing document included) is refused with `RefCycle` carrying that chain and the reopened identifier, an absent one with `UnresolvedRef`, a referenced document's own refusal is `InRef`, and a referenced symmetric body is refused. Vertical composition is $\mathsf{vertical}(\pi_1, \ldots, \pi_k) = \pi_1 \mathbin{+\!\!+} \cdots \mathbin{+\!\!+} \pi_k$ on the parts' stage sequences. The composition scope is every list of at most three of 16 parts (five inline lists and references to nine library documents, to a missing document and to the composing document itself), in 2 modes, with 2 `invertible` values: $(1 + 16 + 16^2 + 16^3) \cdot 2 \cdot 2 = 17{,}476$ documents. The law tests take the 37 lists of at most one step of the step alphabet: 1,369 pairs, of which 676 have both parts compiling, and 50,653 triples, of which 17,576 do.

### Proof

1. Both compile the parts in order, a reference by compiling the referenced document with the same body vertex and the chain extended by its identifier, and stop at the first refusal, located at the reference or under the inline part.
2. Vertical composition concatenates the parts' stages in both, so it is a monoid homomorphism from step lists under concatenation: $\mathsf{compile}(a \mathbin{+\!\!+} b) = \mathsf{compile}(a) \mathbin{+\!\!+} \mathsf{compile}(b)$ by [L-lens-dsl-2](#L-lens-dsl-2) step 3, associativity follows from associativity of concatenation, and the empty list of stages is its unit.
3. The exhaustive differential checks the agreement with the reference on the 17,476 composition documents (of both modes), the law tests check the homomorphism, associativity and unit on every pair and triple above, and the property differential checks compositions of up to five parts.

### Executable counterpart

- Law ID: `lens-dsl.vertical-composition`
- Reference: `panproto_reference::pages::lens_dsl::vertical`
- Enumerator: `panproto_reference::enumerate::lens_dsl::compose_documents; bound: documents=17476`
- Exhaustive differential test: `compositions_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=compositions_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `vertical_composition_is_concatenation_on_every_pair_of_short_step_lists; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=vertical_composition_is_concatenation_on_every_pair_of_short_step_lists)'`
- Exhaustive differential test: `vertical_composition_is_associative_with_the_empty_composition_as_identity; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=vertical_composition_is_associative_with_the_empty_composition_as_identity)'`
- Property differential test: `longer_compositions_compile_as_the_reference; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=longer_compositions_compile_as_the_reference)'`
- Reference test: `vertical_composition_concatenates_and_resolves_references_transitively; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=vertical_composition_concatenates_and_resolves_references_transitively)'`
- File: `crates/panproto-lens-dsl/src/compose.rs`
- File: `crates/panproto-lens-dsl/src/resolver.rs`
- File: `crates/panproto-reference/src/pages/lens_dsl.rs`

## Horizontal composition folds the fused parts {#L-lens-dsl-5}

### Statement

For a document $d$ of the composition scope in mode `horizontal`, production agrees with the reference as in [L-lens-dsl-4](#L-lens-dsl-4), with `EmptyHorizontalPart` at `compose/lenses/i` for the first part whose chain is empty. Moreover, for single steps $a$, $b$, $c$ whose chains are non-empty, $[\mathsf{ref}(a * b), c]$, $[a, \mathsf{ref}(b * c)]$ and $[a, b, c]$ compile to protolenses with the identity as source and the same elementary transforms and residue in the same order.

### Outside the domain

`Fuse` cannot arise, as [L-lens-dsl-2](#L-lens-dsl-2) states. Two horizontal composites with different bracketings are different protolenses; associativity holds for their sequences of elementary transforms and residue, which is what the law states.

### Definitions used

$\mathsf{horizontal}(\pi_1, \ldots, \pi_k)$ is no stage when $k = 0$; otherwise each part's chain is fused (a single protolens is itself), the fused protolenses are folded from the left with $\eta * \theta : F \circ F' \Rightarrow G \circ G'$, and the result is one stage carrying every part's value-level transforms gathered by anchor in part order. The parts are compiled first, all of them, so a part's own refusal precedes an empty part's. The associativity test takes the 36 single steps of the step alphabet: 46,656 triples, of which 10,648 have every part compiling to a non-empty chain.

### Proof

1. Both compile every part before composing, then refuse the first part whose chain is empty, so the refusals agree in class and path.
2. Both fuse each chain and fold from the left, so the stage's protolens is the reference's, read as its sequence of elementary transforms (the head of the page).
3. The fold's elementary sequence is the concatenation of the parts' sequences, since $\eta * \theta$ composes the targets in order and fusion concatenates; concatenation is associative, and every source is the identity by [L-lens-dsl-2](#L-lens-dsl-2) step 4, so the three bracketings agree.
4. The exhaustive differential checks the agreement with the reference on the 17,476 composition documents, the law test checks associativity on every triple above, and the property differential checks compositions of up to five parts.

### Executable counterpart

- Law ID: `lens-dsl.horizontal-composition`
- Reference: `panproto_reference::pages::lens_dsl::horizontal`
- Enumerator: `panproto_reference::enumerate::lens_dsl::compose_documents; bound: documents=17476`
- Exhaustive differential test: `compositions_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=compositions_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `horizontal_composition_is_associative_up_to_its_elementary_structure; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=horizontal_composition_is_associative_up_to_its_elementary_structure)'`
- Property differential test: `longer_compositions_compile_as_the_reference; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=longer_compositions_compile_as_the_reference)'`
- Reference test: `horizontal_composition_folds_fused_parts_from_the_left; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=horizontal_composition_folds_fused_parts_from_the_left)'`
- File: `crates/panproto-lens-dsl/src/compose.rs`
- File: `crates/panproto-lens/src/protolens.rs`
- File: `crates/panproto-reference/src/pages/lens_dsl.rs`

## Every refusal of JSON or YAML text points at the refused value {#L-lens-dsl-6}

### Statement

For every document $d$ of the five scopes that the reference refuses at path $p$, the production refusal of `read_source` or `compile_source` on $d$ is `LensDslError::Spanned` with path $p$ and span $(o, \ell)$, where $(o, \ell)$ is the reference's `locate(d, p)`: the byte offset and length of the value at $p$ in $d$, from its first byte to its last, the last occurrence when a member is named twice. Such a document compiled with `compile_source` from YAML text, which JSON text also is, is refused at the same path with span $(o, c)$, where $c$ is the byte length of the character at $o$: the first character of the refused value. For a document of the composition scope, compiled with `compile_with_refs`, the refusal names $p$ and `locate_json` finds the same range.

### Outside the domain

Nickel sources: a Nickel evaluation error keeps Nickel's own span. A JSON syntax error is located at the byte where the JSON reader stopped, and a YAML syntax error at the character where the YAML reader stopped, which the reference does not model. The extent of a refused value in YAML text: `yaml_serde`, the YAML reader, reports a position only through an error, as the start of the node it was reading when the error was raised, and reports no end position for any node; the event parser beneath it, which does carry end positions, is exposed only through an `unsafe` C-style interface that the workspace's `unsafe_code = "deny"` lint excludes. A YAML refusal is therefore located by the first character of the refused value and not by its whole range.

### Definitions used

`locate` reads $d$ as a tree of values with their byte ranges and follows $p$, taking the last member of a name; it is shared with the [Theory DSL](theory-dsl.md#L-theory-dsl-4) page. The paths are those laws 1 to 5 state: a step's member (`expr`, `inverse`, `default`, `lhs`, `rhs`), a coercion's step, a rule member, a reference at `compose/lenses/i/ref`, a horizontal part at `compose/lenses/i`, a symmetric leg step, a directed equation, `invertible`, or the body key of an `auto` or `from_diff` body.

### Proof

1. Every refusal names a path (laws 1 to 5 state which), and `compile_source` and `read_source` wrap a refusal whose path names a value of $d$ in `Spanned` with the range `locate_json` computes for JSON text, and with the first character `locate_yaml` finds for YAML text.
2. `locate_json` and `locate` read the same JSON grammar, start a value at its first non-blank byte and end it after its last byte, and both keep the last occurrence of a member. `locate_yaml` reads the text with the YAML reader, follows $p$ to the last occurrence of each member, and refuses the node it reaches; the reader reports the start of that node, which in JSON text is the node's first non-blank byte, the start `locate` gives.
3. Every path the laws name is the path of a value present in $d$, except that of a missing field, which is the path of the object lacking it; so a span exists for every refusal, and the differentials compare it with `locate` on every refused document of the five scopes.

### Executable counterpart

- Law ID: `lens-dsl.refusal-location`
- Reference: `panproto_reference::dsl_path::locate`
- Reference: `panproto_reference::pages::lens_dsl::compile_document`
- Exhaustive differential test: `step_lists_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=step_lists_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `compositions_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=compositions_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `the_grammar_refuses_as_the_reference_on_every_mutated_document; command: cargo nextest run -p panproto-lens-dsl --test reference_lens_dsl --no-tests=fail -E 'test(=the_grammar_refuses_as_the_reference_on_every_mutated_document)'`
- Reference test: `refusals_are_located_at_the_member_they_concern; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=refusals_are_located_at_the_member_they_concern)'`
- Reference test: `locate_finds_the_last_occurrence_of_a_member_in_a_lens_document; command: cargo nextest run -p panproto-reference --test lens_dsl --no-tests=fail -E 'test(=locate_finds_the_last_occurrence_of_a_member_in_a_lens_document)'`
- File: `crates/panproto-dsl-eval/src/path.rs`
- File: `crates/panproto-lens-dsl/src/compile.rs`
- File: `crates/panproto-reference/src/dsl_path.rs`
