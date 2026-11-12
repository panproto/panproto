# Theory DSL

A theory document declares sorts, operations and equations, or a morphism between theories, or a bundle of both, as a JSON, YAML or Nickel value. This page states what the compiler does with such a value: which values the document grammar admits, how a theory's parents and imports are elaborated into one presentation, how a morphism document is compiled and checked, and where in the source text each refusal points. The meaning of each construct is set out in the [Theory DSL semantics](../explanation/semantics/theory-dsl.md) page; the laws below state that the compiler computes exactly that meaning on a stated finite scope, against an independent reference, `panproto_reference::pages::theory_dsl`.

The reference starts from a document value, the reference crate's own JSON value; Nickel evaluation and YAML reading produce that value and are outside the laws. It checks signatures and morphisms with the reference of the [Theories](theories.md) page, which that page proves against `panproto-gat`, and transcribes everything else from the semantics page. It answers "outside the domain" for a document using a dependent sort, a sort kind other than `structural`, a closed sort, an implicit input, a directed equation, a conflict policy, a `case`, `let` or hole term, a sort applied to arguments, or a body other than a theory, a morphism or a bundle of theories and morphisms; none of the documents of the scopes below does.

The scopes are built over a library of two theories. `L` has sorts `V` and `E`, operations `s, t : E -> V` and `r : V -> E`, and the equation `sr : s(r(v)) = v`; `K` has sorts `P` and `Q`, operations `a, b : Q -> P` and `c : P -> Q`, and `ac : a(c(p)) = p`. The name `M` resolves to nothing.

## Theory documents follow the document grammar {#L-theory-dsl-1}

### Statement

For a JSON text $d$ of the grammar scope, write $G(d)$ for `check_document` applied to the reference's reading of $d$. When $G(d)$ accepts, `read_source(d, Json)` returns a document. When $G(d)$ refuses with a path $p$ and a problem $q$, `read_source` refuses with `TheoryDslError::Spanned` around `TheoryDslError::Shape` with path $p$ and the problem $q$, and its span is the reference's location of $p$ in $d$.

### Outside the domain

None within the scope: the grammar is total on JSON values. Duplicate member names are read as the last occurrence by both.

### Definitions used

$G$ is the grammar of the semantics page, written as field tables: an object is checked for its least unknown member name, then field by field in table order, a missing required field refused at the object; a map's members are checked in name order and an array's elements in order; the body is selected by the one body key present. The scope takes eight base documents, one per body kind, which together use every field of the grammar, and replaces each of their values in turn by each of eight values (`null`, `true`, `1`, `"x"`, `[]`, `{}`, the value wrapped in an array, the value wrapped in an object under an unknown member): $153 \cdot 8 = 1{,}224$ documents over the 153 values of the eight bases. To these it adds each base document with one object member removed (122 documents) and with an unknown member added to one object (35 documents), $1{,}381$ in all.

### Proof

1. Production's `read_document` and the reference's `check_document` check the same field tables in the same order, and neither depends on the order of members in the source: both refuse the least unknown member name and visit a map's members by name.
2. A body key is present in both exactly when the member is, and both drop an `instance` document's `class` member before counting bodies, so `NoBody` and `SeveralBodies` agree, with the keys in grammar order.
3. Each leaf check (string, optional string, boolean, choice, tagged record, theory reference) refuses the same JSON types, so the first refused value and its problem agree.
4. A value the grammar admits deserializes into the serde types, so production returns a document exactly when the reference accepts; the differential checks this, the refusal, its path and its span on every document of the scope.

### Executable counterpart

- Law ID: `theory-dsl.document-grammar`
- Reference: `panproto_reference::pages::theory_dsl::check_document`
- Enumerator: `panproto_reference::enumerate::theory_dsl::shape_documents; bound: documents=1381`
- Exhaustive differential test: `the_grammar_refuses_as_the_reference_on_every_mutated_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=the_grammar_refuses_as_the_reference_on_every_mutated_document)'`
- Reference test: `the_document_grammar_refuses_in_field_table_order; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=the_document_grammar_refuses_in_field_table_order)'`
- File: `crates/panproto-theory-dsl/src/shape.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`

## A theory body elaborates its parents, imports and own declarations {#L-theory-dsl-2}

### Statement

For a theory or bundle document $d$ of the theory and bundle scopes, write $C(d)$ for `compile_document` of the reference over the library. When $C(d)$ accepts with theories $T$, `compile_source(d, Json)` with the library as resolver accepts with theories that are $T$ declaration for declaration, in order. When $C(d)$ refuses at path $p$ with refusal $\rho$, production refuses at $p$ with the same refusal: the same grammar problem, a term parse error, `TheoryNotFound`, `UnknownExposed` or `Duplicate` with the same name, `AmbiguousOpInputs`, a signature refusal whose first defect is the reference's, or an equation typing refusal.

### Outside the domain

The constructions listed at the head of the page. Equation typing refusals are compared by path, which names the equation, and not by the checker's message.

### Definitions used

$C$ resolves the parents in `extends` order, then each import in order, checking its exposed names and renaming its declarations by $\rho$; includes each declaration once; resolves the spec's own sort and operation heads with the order of the semantics page (the spec's and its parents' names, then qualified names, then bare names of imports without an alias or exposing the name); checks the signature with the Theories reference, taking the path of the first defect from the declaration it concerns; and types each equation first order. A bundle elaborates its theories in listed order, each seeing the bundle's earlier theories before the library. The theory scope is the product of 3 `extends` lists (none, `L`, `M`), 6 import lists (none, `L`, `L` aliased, `L` aliased with exposed names, `L` exposing an undeclared name, `M`), 4 sort lists, no operation or one of 15, and no equation or one of 5: $3 \cdot 6 \cdot 4 \cdot 16 \cdot 6 = 6{,}912$ documents. The bundle scope is every sequence of at most three of four theory specs: $1 + 4 + 16 + 64 = 85$ documents.

### Proof

1. Both resolve parents before imports and imports in order, and refuse the first unresolved name at its `extends` entry or its `imports` entry's `from`, and the first undeclared exposed name at its `expose` entry.
2. Both rename an import's declarations by $\rho$ and include a declaration only when no equal one is present, so the union agrees declaration for declaration.
3. Both parse the spec's sorts, then operations (output first, then the `input` or `inputs` sorts, refusing both together), then equations (left side first), so the first term parse refusal and `AmbiguousOpInputs` agree in kind and path.
4. Name resolution consults the same four tiers in the same order, so the elaborated signature is the same; the Theories page proves the signature check against the reference, and both take a duplicate's path from its second declaration and an input's from `input` when the shorthand declared it.
5. On a first-order signature, the production equation checker infers each variable's sort from the input positions it fills and refuses a conflict, an unknown operation, a wrong arity, an argument of another sort or two sides of different sorts, which is the reference's typing; both type the included equations before the spec's own.
6. The exhaustive differentials check the agreement on the 6,912 theory documents and 85 bundles, and the property differential on theory documents with up to four operations and four equations.

### Executable counterpart

- Law ID: `theory-dsl.elaboration`
- Reference: `panproto_reference::pages::theory_dsl::elaborate`
- Enumerator: `panproto_reference::enumerate::theory_dsl::theory_documents; bound: documents=6912`
- Enumerator: `panproto_reference::enumerate::theory_dsl::bundle_documents; bound: documents=85`
- Exhaustive differential test: `theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `bundles_compile_their_theories_in_listed_order_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=bundles_compile_their_theories_in_listed_order_as_the_reference)'`
- Property differential test: `longer_theory_documents_compile_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=longer_theory_documents_compile_as_the_reference)'`
- Reference test: `imports_rename_and_parents_take_precedence_over_bare_imported_names; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=imports_rename_and_parents_take_precedence_over_bare_imported_names)'`
- File: `crates/panproto-theory-dsl/src/compile_theory.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`

## A morphism document compiles to the checked morphism it states {#L-theory-dsl-3}

### Statement

For a morphism document $d$ of the morphism scope, when the reference's `compile_morphism` accepts with morphism $m$, `compile_source(d, Json)` accepts with $m$; when it refuses at path $p$, production refuses at $p$ with `TheoryNotFound` of the same name, `UnknownMapKey` of the same map and key, or `MorphismCheck` whose checker error is the same variant as the reference's.

### Outside the domain

Operation images that are terms, which the document grammar does not express.

### Definitions used

The reference resolves the domain, then the codomain; refuses the least sort-map key, then the least operation-map key, that names no domain symbol; and checks the morphism with the Theories reference, locating a refusal at the map lacking an entry, at the entry of the first domain symbol (in declaration order) whose image is refused, or at the spec for an equation not preserved. The scope is the product of 4 domain and codomain pairs over `L`, `K` and `M`, 3 images of each of `V` and `E` (none, `P`, `Q`), an extra sort-map entry `Z` or none, and 4 images of each of `s`, `t` and `r` (none, `a`, `b`, `c`): $4 \cdot 9 \cdot 2 \cdot 64 = 4{,}608$ documents.

### Proof

1. Both resolve in the same order and test the map keys in the same order against the domain's declarations.
2. The Theories page proves the morphism check against the reference, including which refusal comes first: sorts in declaration order, then operations, then equations.
3. Production locates the refusal by the same rule as the reference, reading the domain symbol from the error or, for a missing target, from the first domain symbol mapped to it, which is the symbol the check refuses first.
4. The exhaustive differential checks the agreement on all 4,608 documents.

### Executable counterpart

- Law ID: `theory-dsl.morphism-documents`
- Reference: `panproto_reference::pages::theory_dsl::compile_morphism`
- Enumerator: `panproto_reference::enumerate::theory_dsl::morphism_documents; bound: documents=4608`
- Exhaustive differential test: `morphism_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=morphism_documents_compile_as_the_reference_on_every_bounded_document)'`
- Reference test: `morphism_refusals_are_located_at_the_entry_they_concern; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=morphism_refusals_are_located_at_the_entry_they_concern)'`
- File: `crates/panproto-theory-dsl/src/compile_morphism.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`

## Every refusal of JSON text points at the refused value {#L-theory-dsl-4}

### Statement

For every document $d$ of the four scopes that the reference refuses at path $p$, the production refusal of `read_source` or `compile_source` on $d$ is `TheoryDslError::Spanned` with path $p$ and span $(o, \ell)$, where $(o, \ell)$ is the reference's `locate(d, p)`: the byte offset and length of the value at $p$ in $d$, from its first byte to its last, the last occurrence when a member is named twice.

### Outside the domain

YAML and Nickel sources: a YAML refusal carries its path without a span, and a Nickel evaluation error keeps Nickel's own span. A JSON syntax error is located at the byte where the JSON reader stopped, which the reference does not model.

### Definitions used

`locate` reads $d$ as a tree of values with their byte ranges and follows $p$, taking the last member of a name.

### Proof

1. Every refusal names a path (laws 1 to 3 state which), and `compile_source` and `read_source` wrap a refusal whose path names a value of $d$ in `Spanned` with the range `locate_json` computes.
2. `locate_json` and `locate` read the same JSON grammar, start a value at its first non-blank byte and end it after its last byte, and both keep the last occurrence of a member.
3. Every path the laws name is the path of a value present in $d$, except that of a missing field, which is the path of the object lacking it; so a span exists for every refusal, and the differentials compare it with `locate` on every refused document of the four scopes.

### Executable counterpart

- Law ID: `theory-dsl.refusal-location`
- Reference: `panproto_reference::pages::theory_dsl::locate`
- Reference: `panproto_reference::pages::theory_dsl::compile_document`
- Exhaustive differential test: `theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `the_grammar_refuses_as_the_reference_on_every_mutated_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=the_grammar_refuses_as_the_reference_on_every_mutated_document)'`
- Reference test: `locate_finds_the_last_occurrence_of_a_member; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=locate_finds_the_last_occurrence_of_a_member)'`
- File: `crates/panproto-theory-dsl/src/path.rs`
- File: `crates/panproto-theory-dsl/src/compile.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`
