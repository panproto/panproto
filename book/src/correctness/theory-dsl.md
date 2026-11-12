# Theory DSL

A theory document declares sorts, operations and equations, a morphism between theories, a composition of theories by colimit steps, a protocol naming its schema and instance theories, or a bundle of these, as a JSON, YAML or Nickel value. This page states what the compiler does with such a value: which values the document grammar admits, how a theory's parents and imports are elaborated into one presentation, how a morphism document is compiled and checked, how a composition document composes its steps, how a protocol document resolves and compiles its theory references, and where in the source text each refusal points. The meaning of each construct is set out in the [Theory DSL semantics](../explanation/semantics/theory-dsl.md) page; the laws below state that the compiler computes exactly that meaning on a stated finite scope, against an independent reference, `panproto_reference::pages::theory_dsl`.

The reference starts from a document value, the reference crate's own JSON value; Nickel evaluation and YAML reading produce that value and are outside the laws. It checks signatures and morphisms with the reference of the [Theories](theories.md) page, which that page proves against `panproto-gat`, and transcribes everything else from the semantics page. Sorts with parameters, sort expressions applied to terms, operations with implicit inputs and equations over dependent sorts are in the domain: the reference infers implicit inputs and types equations with the Theories reference's `implicit_inputs_inferable` and `typecheck_equation`. It answers "outside the domain" for a document using a sort kind other than `structural`, a closed sort, a directed equation, a conflict policy, a `case`, `let` or hole term, a composition step with identified symbols, a shared sort with parameters, or sides a colimit step cannot label, a protocol's inline composed theory, or a class, instance or inductive body; none of the documents of the scopes below does.

The scopes are built over a library of three theories. `L` has sorts `V` and `E`, operations `s, t : E -> V` and `r : V -> E`, and the equation `sr : s(r(v)) = v`; `K` has sorts `P` and `Q`, operations `a, b : Q -> P` and `c : P -> Q`, and `ac : a(c(p)) = p`. `C` is the theory of categories with implicit objects: sorts `Ob` and `Hom(a : Ob, b : Ob)`, operations `id(x : Ob) : Hom(x, x)` and `comp({a b c : Ob}, g : Hom(b, c), f : Hom(a, b)) : Hom(a, c)`, and the equations `idl : comp(id(b), f) = f` and `idr : comp(f, id(a)) = f`. `N` shares `V`, `E` and `t : E -> V` with `L` and adds the sort `F` and `h : F -> V`. The name `M` resolves to nothing.

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

For a theory or bundle document $d$ of the theory, dependent and bundle scopes, write $C(d)$ for `compile_document` of the reference over the library. When $C(d)$ accepts with theories $T$, `compile_source(d, Json)` with the library as resolver accepts with theories that are $T$ declaration for declaration, in order. When $C(d)$ refuses at path $p$ with refusal $\rho$, production refuses at $p$ with the same refusal: the same grammar problem, a term parse error, `TheoryNotFound`, `UnknownExposed` or `Duplicate` with the same name, `AmbiguousOpInputs`, a signature refusal whose first defect is the reference's, `NonInferrableImplicit` for the same operation, or an equation typing refusal.

### Outside the domain

The constructions listed at the head of the page. Equation typing refusals are compared by path, which names the equation, and not by the checker's message.

### Definitions used

$C$ resolves the parents in `extends` order, then each import in order, checking its exposed names and renaming its declarations by $\rho$ (a sort's parameters and an operation's input and output sorts included, with the operations applied in their arguments); includes each declaration once; parses the spec's sorts with their parameters, then its operations, then its equations; resolves the spec's own sort heads, and the operation heads in terms and in sort arguments, with the order of the semantics page (the spec's and its parents' names, then qualified names, then bare names of imports without an alias or exposing the name); checks the signature with the Theories reference, taking the path of the first defect from the declaration it concerns; refuses the first operation, in declaration order, with an implicit input that occurs in no explicit input's sort and not in the output sort, at that operation; and types each equation with the Theories reference's equation typing, in which an equation's variables take their sorts from the positions they fill, an argument that is an application constrains the implicit inputs of the operation it is passed to, and only implicit inputs are unknowns. A bundle elaborates its theories in listed order, each seeing the bundle's earlier theories before the library. The theory scope is the product of 3 `extends` lists (none, `L`, `M`), 6 import lists (none, `L`, `L` aliased, `L` aliased with exposed names, `L` exposing an undeclared name, `M`), 4 sort lists, no operation or one of 15, and no equation or one of 5: $3 \cdot 6 \cdot 4 \cdot 16 \cdot 6 = 6{,}912$ documents. The dependent scope is the product of 2 `extends` lists (none, `C`), 4 import lists (none, `C`, `C` aliased `D`, `C` aliased `D` exposing `Hom`), 8 sort lists (none; `Ob` and `Hom`; those and `Sq(x : Ob, f : Hom(x, x))`; a sort over unbound variables; a parameter of qualified sort `D.Hom(x, x)`; a repeated binder; an unparsable parameter sort; a parameter sort applied to too many arguments), no operation or one of 11 (among them operations with inferable and non-inferable implicit inputs, a sort of the wrong arity, an unbound variable, an argument of the wrong sort, a qualified sort, an operation applied in a sort argument and an empty argument), and no equation or one of 6 (identity and associativity laws, `comp(id(a), id(b)) = id(a)`, which identifies two fixed variables, and `comp(f(x), id(y)) = f(x)`): $2 \cdot 4 \cdot 8 \cdot 12 \cdot 7 = 5{,}376$ documents. The bundle scope is every sequence of at most three of four theory specs: $1 + 4 + 16 + 64 = 85$ documents.

### Proof

1. Both resolve parents before imports and imports in order, and refuse the first unresolved name at its `extends` entry or its `imports` entry's `from`, and the first undeclared exposed name at its `expose` entry.
2. Both rename an import's declarations by $\rho$ and include a declaration only when no equal one is present, so the union agrees declaration for declaration.
3. Both parse the spec's sorts, then operations (output first, then the `input` or `inputs` sorts, refusing both together), then equations (left side first), so the first term parse refusal and `AmbiguousOpInputs` agree in kind and path.
4. Name resolution consults the same four tiers in the same order, so the elaborated signature is the same; the Theories page proves the signature check against the reference, and both take a duplicate's path from its second declaration and an input's from `input` when the shorthand declared it.
5. A sort expression is a symbol or a symbol applied to terms, and both parse it with the term grammar for its arguments, refusing an empty argument; `S()` is the sort `S`. Production renames a sort expression by its sort map at the head and its operation map in the arguments, which is the reference's renaming.
6. Production checks the theory body after the signature: every implicit input in declaration order, then closed sorts (none in the domain), then each equation in order, which is the reference's order, and locates an implicit-input refusal at its operation and an equation refusal at the equation.
7. The production equation checker infers each variable's sort from the input positions it fills, also unifying an application argument's output sort with the input sort at its position, and solves an application's implicit inputs binding only its own fresh parameters; the Theories reference's equation typing states the same judgement, and it refuses a conflict, an unknown operation, a wrong arity, an argument of another sort or two sides of different sorts. Both type the included equations before the spec's own.
8. The exhaustive differentials check the agreement on the 6,912 theory documents, the 5,376 dependent documents and the 85 bundles, and the property differentials on theory and dependent theory documents with up to four operations and four equations.

### Executable counterpart

- Law ID: `theory-dsl.elaboration`
- Reference: `panproto_reference::pages::theory_dsl::elaborate`
- Enumerator: `panproto_reference::enumerate::theory_dsl::theory_documents; bound: documents=6912`
- Enumerator: `panproto_reference::enumerate::theory_dsl::dependent_documents; bound: documents=5376`
- Enumerator: `panproto_reference::enumerate::theory_dsl::bundle_documents; bound: documents=85`
- Exhaustive differential test: `theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `dependent_theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=dependent_theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `bundles_compile_their_theories_in_listed_order_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=bundles_compile_their_theories_in_listed_order_as_the_reference)'`
- Property differential test: `longer_theory_documents_compile_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=longer_theory_documents_compile_as_the_reference)'`
- Property differential test: `longer_dependent_theory_documents_compile_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=longer_dependent_theory_documents_compile_as_the_reference)'`
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

## Every refusal of JSON or YAML text points at the refused value {#L-theory-dsl-4}

### Statement

For every document $d$ of the scopes of laws 2, 3, 5 and 6 that the reference refuses at path $p$, the production refusal of `read_source` or `compile_source` on $d$ is `TheoryDslError::Spanned` with path $p$ and span $(o, \ell)$, where $(o, \ell)$ is the reference's `locate(d, p)`: the byte offset and length of the value at $p$ in $d$, from its first byte to its last, the last occurrence when a member is named twice. The same document read as YAML text, which JSON text also is, is refused by the same function at the same path with span $(o, c)$, where $c$ is the byte length of the character at $o$: the first character of the refused value.

### Outside the domain

Nickel sources: a Nickel evaluation error keeps Nickel's own span. A JSON syntax error is located at the byte where the JSON reader stopped, and a YAML syntax error at the character where the YAML reader stopped, which the reference does not model. The extent of a refused value in YAML text: `yaml_serde`, the YAML reader, reports a position only through an error, as the start of the node it was reading when the error was raised, and reports no end position for any node; the event parser beneath it, which does carry end positions, is exposed only through an `unsafe` C-style interface that the workspace's `unsafe_code = "deny"` lint excludes. A YAML refusal is therefore located by the first character of the refused value and not by its whole range.

### Definitions used

`locate` reads $d$ as a tree of values with their byte ranges and follows $p$, taking the last member of a name.

### Proof

1. Every refusal names a path (laws 1, 2, 3, 5 and 6 state which), and `compile_source` and `read_source` wrap a refusal whose path names a value of $d$ in `Spanned` with the range `locate_json` computes for JSON text, and with the first character `locate_yaml` finds for YAML text.
2. `locate_json` and `locate` read the same JSON grammar, start a value at its first non-blank byte and end it after its last byte, and both keep the last occurrence of a member. `locate_yaml` reads the text with the YAML reader, follows $p$ to the last occurrence of each member, and refuses the node it reaches; the reader reports the start of that node, which in JSON text is the node's first non-blank byte, the start `locate` gives.
3. Every path the laws name is the path of a value present in $d$, except that of a missing field, which is the path of the object lacking it; so a span exists for every refusal, and the differentials compare it with `locate` on every refused document of those scopes and of the grammar scope.

### Executable counterpart

- Law ID: `theory-dsl.refusal-location`
- Reference: `panproto_reference::pages::theory_dsl::locate`
- Reference: `panproto_reference::pages::theory_dsl::compile_document`
- Exhaustive differential test: `theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `dependent_theory_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=dependent_theory_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `the_grammar_refuses_as_the_reference_on_every_mutated_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=the_grammar_refuses_as_the_reference_on_every_mutated_document)'`
- Exhaustive differential test: `protocol_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=protocol_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `composition_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=composition_documents_compile_as_the_reference_on_every_bounded_document)'`
- Reference test: `locate_finds_the_last_occurrence_of_a_member; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=locate_finds_the_last_occurrence_of_a_member)'`
- File: `crates/panproto-dsl-eval/src/path.rs`
- File: `crates/panproto-theory-dsl/src/compile.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`

## A protocol document compiles to the protocol it names {#L-theory-dsl-5}

### Statement

For a protocol or bundle document $d$ of the protocol and protocol bundle scopes, when the reference's `compile_document` accepts with protocols $P$ and theories $T$, `compile_source(d, Json)` accepts with $P$ and $T$: each protocol has the reference's name, schema theory name, instance theory name and edge rules, one source kind and one target kind per rule, every feature flag off, no vertex kinds, constraint sorts or composition recipes; and the compiled theories are the bundle's theories and the protocols' inline theories. When the reference refuses at path $p$, production refuses at $p$ with the same refusal.

### Outside the domain

A protocol's inline composed theory, which the reference does not model; [Protocol bodies](../explanation/semantics/theory-dsl.md#protocol-bodies) on the semantics page states their meaning. The edge rules' kinds are not checked against any theory, so no refusal concerns them.

### Definitions used

The reference resolves the schema theory reference, then the instance theory reference: a name must be one the resolver knows (in a bundle, the bundle's theories, then the library), and is refused with `TheoryNotFound` at the member otherwise; an inline theory is elaborated by law 2 with the same resolver, its refusals located under the member. The inline theories of a document's protocols are recorded after its last protocol, schema before instance, protocol by protocol: a name recorded already is kept when it names the same theory and refused with `Duplicate` otherwise, at the root of a protocol document or at `/protocols` of a bundle; a bundle naming one protocol twice is refused at the second's `protocol` member. The protocol scope is the product of 6 schema references (`L`, `M`, an inline theory `S`, an inline `S` extending `L`, an ill-formed inline `S`, an inline theory named `L`), 5 instance references (`K`, `M`, the first inline `S` again, an inline `I` with an aliased import, an inline `I` importing `M`) and 3 edge-rule lists: $6 \cdot 5 \cdot 3 = 90$ documents. The protocol bundle scope lists the theory `A` and each sequence of at most three of four protocols (one naming `A`, one whose inline theory is `A` again, one whose inline theory is another `A`, one naming the theory its own schema reference builds): $1 + 4 + 16 + 64 = 85$ documents.

### Proof

1. Production resolves the schema reference before the instance reference and locates every refusal of either under its member, as the reference does; a named reference consults the same theories in the same order, so `TheoryNotFound` agrees in name and path.
2. An inline theory compiles with `compile_theory_with_resolver`, which law 2 proves against `elaborate`, with the resolver the reference uses.
3. Production records the inline theories after the last protocol, in the reference's order, keeping a name whose two theories have the same canonical bytes; on the theories of the scopes, equal canonical bytes is the reference's equality of theories.
4. The protocol production builds copies the names and the edge rules and sets nothing else, which is the protocol the reference's conversion denotes.
5. The exhaustive differentials check the agreement, the refusal, its path and its span, on all 90 protocol documents and 85 protocol bundles.

### Executable counterpart

- Law ID: `theory-dsl.protocol-documents`
- Reference: `panproto_reference::pages::theory_dsl::compile_protocol`
- Enumerator: `panproto_reference::enumerate::theory_dsl::protocol_documents; bound: documents=90`
- Enumerator: `panproto_reference::enumerate::theory_dsl::protocol_bundle_documents; bound: documents=85`
- Exhaustive differential test: `protocol_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=protocol_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `bundled_protocols_record_their_inline_theories_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=bundled_protocols_record_their_inline_theories_as_the_reference)'`
- Reference test: `protocol_references_resolve_compile_and_record_inline_theories; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=protocol_references_resolve_compile_and_record_inline_theories)'`
- File: `crates/panproto-theory-dsl/src/compile_protocol.rs`
- File: `crates/panproto-theory-dsl/src/compile.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`

## A composition document composes its colimit steps in order {#L-theory-dsl-6}

### Statement

For a composition or bundle document $d$ of the composition and composition bundle scopes, when the reference's `compile_document` accepts with theories $T$ and recipes $Q$, `compile_source(d, Json)` accepts with theories of the same canonical bytes as $T$ and with the recipes $Q$, step for step; when the reference refuses at path $p$, production refuses at $p$ with `TheoryNotFound` or `Duplicate` of the same name, or with `ColimitFailed`.

### Outside the domain

A step with `identify` entries, a step sharing a sort with parameters, and a step whose sides cannot label a diagram node (a name with `.` or `#`, a theory composed with itself, the step's interface label): production composes these as diagrams whose shared theories carry the identified symbols and records a `Diagram` step, which the reference does not model. `ColimitFailed` is compared by kind and path, not by its message.

### Definitions used

The reference resolves each base in order, refusing an unknown one at its `bases` entry. For each step in order it resolves the left and then the right side, a theory the resolver knows or the result `step_k` of an earlier step, refusing an unknown side at the step's `left` or `right`; records the step as `CompositionStep::Colimit` with its sides, shared sorts and shared operations and its mode (`amalgamate` when absent); and takes as the step's theory the Theories reference's `recompose` of the recipe so far, refusing at the step when the replay is refused. A composition without steps is refused at `steps`. The result is the last step's theory named by `result`; the recipe's version is 1 exactly when a step is in `explicit_identity` mode. A bundle compiles its compositions after its theories, in listed order, each seeing the bundle's theories and earlier compositions before the library, and refuses a result named as a theory or an earlier result at its `result` member. The composition scope is the product of 5 side pairs (`L` and `N` either way round, `K` and `N`, and pairs naming `M`), 5 shared-sort lists (none, `V`, `V` and `E`, `F`, `Z`), 3 shared-operation lists (none, `t`, `h`), 3 modes (absent, `amalgamate`, `explicit_identity`), 3 base lists (none, `L`, `M`) and 3 second steps (none, `step_0` with `K`, `step_1` with `K`): $5 \cdot 5 \cdot 3 \cdot 3 \cdot 3 \cdot 3 = 2{,}025$ documents, of which the reference accepts 132. The composition bundle scope lists the theory `A` and each sequence of at most three of four compositions (`A` with `L` into `R`, the same again, a result named `A`, `R` with `K` into `S`): $1 + 4 + 16 + 64 = 85$ documents.

### Proof

1. Production resolves the bases and then each step's sides in the reference's order, a name `step_k` only among earlier results, and locates an unknown base or side at the same member.
2. On a step in the domain, production records `CompositionStep::Colimit` with the step's sides, shared sorts and operations and mode, as the reference does, and composes it as `pushout_by_name` (amalgamate) or `compose` over the by-name span (explicit identity), which is the step `panproto_gat::recompose` replays; the Theories page proves that replay against the reference's `recompose` (L-theories-8), and the differential checks the theory's canonical bytes on every accepted document.
3. Each refusal of the step's composition is `ColimitFailed` at the step, and the reference's replay refuses the same steps; a composition without steps is refused at `steps` by both.
4. Production names the result `result`, records the recipe under it with version 1 exactly when a step is in `explicit_identity` mode, and compiles a bundle's compositions in the reference's order and with its duplicate refusal.
5. The exhaustive differentials check the agreement, the refusal, its path and its span (and the YAML span of law 4) on all 2,025 composition documents and 85 composition bundles.

### Executable counterpart

- Law ID: `theory-dsl.composition-documents`
- Reference: `panproto_reference::pages::theory_dsl::compile_composition`
- Enumerator: `panproto_reference::enumerate::theory_dsl::composition_documents; bound: documents=2025`
- Enumerator: `panproto_reference::enumerate::theory_dsl::composition_bundle_documents; bound: documents=85`
- Exhaustive differential test: `composition_documents_compile_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=composition_documents_compile_as_the_reference_on_every_bounded_document)'`
- Exhaustive differential test: `bundled_compositions_compile_in_listed_order_as_the_reference; command: cargo nextest run -p panproto-theory-dsl --test reference_theory_dsl --no-tests=fail -E 'test(=bundled_compositions_compile_in_listed_order_as_the_reference)'`
- Reference test: `compositions_replay_their_colimit_steps_and_locate_refusals; command: cargo nextest run -p panproto-reference --test theory_dsl --no-tests=fail -E 'test(=compositions_replay_their_colimit_steps_and_locate_refusals)'`
- File: `crates/panproto-theory-dsl/src/compile_compose.rs`
- File: `crates/panproto-theory-dsl/src/compile.rs`
- File: `crates/panproto-reference/src/pages/theory_dsl.rs`
