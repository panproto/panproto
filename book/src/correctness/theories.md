# Theories

The theory layer is governed by eight laws. [L-theories-1](#L-theories-1) characterizes well-formed signatures, and [L-theories-2](#L-theories-2) fixes the content and order of their defects; [L-theories-3](#L-theories-3) determines class names; [L-theories-4](#L-theories-4) constructs the colimit and its provenance; [L-theories-5](#L-theories-5) states the universal property; [L-theories-6](#L-theories-6) specifies the union-find closure; [L-theories-7](#L-theories-7) checks theory morphisms; and [L-theories-8](#L-theories-8) replays recorded compositions. [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md) develops the construction, using the conventions of [Shared notation](../explanation/semantics/shared-notation.md).

A *theory* has sorts, operations, equations, directed equations and conflict policies. A sort has a name, a telescope of parameters $(p_1 : P_1, \ldots, p_n : P_n)$, a kind, and either no constructor list (an open sort) or a list of constructor names (a closed sort). An operation has a name, a telescope of inputs $(x_1 : S_1, \ldots, x_n : S_n)$, each marked explicit or implicit, and an output sort $S$. A *sort expression* is a sort name $s$, or a sort applied to argument terms $s(t_1, \ldots, t_k)$; its *head* is $s$. A *term* is a variable, an application $o(t_1, \ldots, t_k)$ of an operation name, a hole, a local binding $\mathsf{let}\ x = t\ \mathsf{in}\ u$, or a case analysis whose branches bind one name per input of their constructor. A *telescope*'s binders are the names $p_i$ (or $x_i$); the binders before position $i$ are the *context* of position $i$.

Several blocks compare sort expressions through their *name-free forms*. Given a list of binder names $\beta$, the name-free form $\mathrm{nf}_\beta(t)$ of a term $t$ replaces each variable $x$ by the position of the last occurrence of $x$ in $\beta$, or by the marker $\mathrm{free}(x)$ when $\beta$ does not contain $x$; it erases the names a local binding or a case branch binds, extending $\beta$ by them inside their scope; and it keeps operation names, holes and the shape of the term. The name-free form of a sort expression applies $\mathrm{nf}_\beta$ to each argument and keeps the head. Two sort expressions read under $\beta$ and $\beta'$ are *equal up to binder names* when their name-free forms under $\beta$ and $\beta'$ are equal. Inside a composition the same forms are taken with every sort and operation name replaced by its class (L-theories-4); a variable's position is read as above.

The *depth* of a term is one for a variable, a hole and an application with no arguments, and one more than the deepest argument (or bound term, body, scrutinee or branch body) otherwise. The executable counterparts below use the *two-sort alphabet* of the enumerator `panproto_reference::enumerate::theories`: sorts `A` (open or closed over the operations whose output is `A`, of kind structural or, when it is the only sort, of kind value) and `B` (simple, or `B(p : A)`); at most two operations `f` and `g` of arity at most two, with inputs `x` and `y`, each input and output of sort `A`, a simple `B`, or the dependent `B` applied to an earlier input of sort `A`; and at most one equation `e` between terms of depth at most one, that is between the variables `x`, `y` and the constants.

## A signature passes checking exactly when it is well formed {#L-theories-1}

### Statement

For every theory $T$, `typecheck_signature`$(T)$ returns `Ok(())` exactly when $T$ is *well formed*, that is when all of the following hold.

1. No two sorts, no two operations, no two equations, no two directed equations and no two policies of $T$ share a name.
2. No telescope of $T$ (the parameters of a sort, the inputs of an operation) binds one name twice.
3. Every sort parameter's sort $P_i$ has a head that $T$ declares, and that head belongs to the dependency order $D(T)$.
4. Every sort expression of $T$ (each parameter sort whose head lies in $D(T)$, each input sort and each output sort) is well formed at its site in the context of the binders before it.

Here $D(T)$ is the least set of sort names such that a declared sort $s$ belongs to it whenever every parameter sort of $s$ has a head $h \neq s$ that either no sort of $T$ declares or that already belongs to the set. A sort expression $E$ is *well formed at a site* in a context $\Gamma$ when its head names a declared sort $s$ (the last declaration of that name) and, if $E$ is an application $s(t_1, \ldots, t_k)$, then $k$ is the number of parameters of $s$, every variable of each $t_i$ is bound in $\Gamma$, and each $t_i$ has, in $\Gamma$, a sort equal (up to the names of local binders) to the parameter sort $P_i$ with $p_1, \ldots, p_{i-1}$ replaced by $t_1, \ldots, t_{i-1}$. A sort name $s$ written without arguments is well formed whenever $s$ is declared, whatever the number of its parameters: it is the unapplied, implicitly indexed form.

### Outside the domain

There is none: the statement holds for every theory. A theory that is not well formed is refused with the non-empty list of its defects, whose exact content and order are the subject of [L-theories-2](#L-theories-2).

### Definitions used

The theory, telescope, context, sort expression and head are defined at the top of this page. The sort of a term in a context is the one the typing judgement of the theory assigns: a variable has the sort its last binding in the context gives it; an application $o(u_1, \ldots, u_m)$ of an operation whose explicit arity is $m$ has the operation's output sort with its inputs replaced by the arguments (implicit inputs inferred by first-order unification); a hole has a sort of its own; a local binding has the sort of its body; a case analysis over a closed sort has the common sort of its branches. A term has no sort when this judgement fails, and then $t_i$ is not of the parameter sort.

### Proof

1. `typecheck_signature` builds one list of defects and returns `Ok(())` exactly when the list is empty (definition of `typecheck_signature`).
2. The list receives a `DuplicateSymbol` defect for each name that occurs twice or more among the sorts, the operations, the equations, the directed equations or the policies, and no other defect from this pass. So this pass adds nothing exactly when condition 1 holds.
3. For every parameter of every sort, and every input of every operation, the list receives a `DuplicateBinder` defect exactly when the binder's name occurs earlier in the same telescope. So these defects are absent exactly when condition 2 holds.
4. For a sort parameter whose sort's head $h$ is declared but does not belong to $D(T)$, the list receives `UndeclaredSort` for $h$. The procedure computes $D(T)$ by adding, round by round, every declared sort whose parameter heads are all different from it and either undeclared or already added, and stops when a round adds nothing; this is the least fixed point of the definition in the Statement, because each added sort satisfies the defining condition with respect to the sorts added before it, and a sort satisfying the condition with respect to the final set would have been added in the last round.
5. Every other sort expression is checked at its site with the context of the binders before it. The check adds `UndeclaredSort` when no sort of that name is declared and stops; otherwise, for an application whose argument count differs from the number of parameters, it adds `SortArity` and stops; otherwise, for each argument in order, it adds one `UnboundParamVar` per variable that the context does not bind, and, when there is none, adds `ParamIllTyped` when the argument has no sort or a sort different from the instantiated parameter sort. A sort name without arguments adds nothing once its head is declared. So the check adds nothing exactly when the expression is well formed at its site.
6. A parameter sort whose head is undeclared falls under step 5, which adds `UndeclaredSort` for it; one whose head is declared and outside $D(T)$ falls under step 4. Together, steps 4 and 5 add nothing exactly when conditions 3 and 4 hold.
7. By steps 2, 3 and 6, the list is empty exactly when conditions 1 to 4 hold, and by step 1 this is when the result is `Ok(())`.

### Executable counterpart

- Law ID: `theories.q1`
- Reference: `panproto_reference::theory::typecheck_signature`
- Enumerator: `panproto_reference::enumerate::theories::signatures; bound: sorts=2, operations=2, arity=2, equations=1, depth=1`
- Exhaustive differential test: `signature_checking_agrees_with_the_reference_on_every_signature; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=signature_checking_agrees_with_the_reference_on_every_signature)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/typecheck.rs`
- File: `crates/panproto-gat/src/sort.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## Signature checking reports every defect once, in a fixed order {#L-theories-2}

### Statement

For every theory $T$ that is not well formed ([L-theories-1](#L-theories-1)), `typecheck_signature`$(T)$ returns `Err` with the following list, and only it, in this order.

1. For each kind in the order sort, operation, equation, directed equation, policy, one `DuplicateSymbol(kind, name)` for each name that two or more symbols of that kind share, the names in ascending byte order.
2. For each sort in declaration order and each of its parameters $p_i : P_i$ in order, at the site `SortParam(sort, i)`: first `DuplicateBinder(site, p_i)` when $p_i$ is among $p_1, \ldots, p_{i-1}$; then `UndeclaredSort(site, h)` when the head $h$ of $P_i$ is declared and outside $D(T)$, and otherwise the expression defects of $P_i$ in the context $p_1 : P_1, \ldots, p_{i-1} : P_{i-1}$.
3. For each operation in declaration order and each of its inputs $x_i : S_i$ in order, at the site `OpInput(op, i)`: first `DuplicateBinder(site, x_i)` when $x_i$ is among $x_1, \ldots, x_{i-1}$; then the expression defects of $S_i$ in the context of $x_1, \ldots, x_{i-1}$. Then, at `OpOutput(op)`, the expression defects of the output sort in the context of all the inputs.

The *expression defects* of a sort expression $E$ at a site in a context $\Gamma$ are: `UndeclaredSort(site, h)` alone when no sort of $E$'s head $h$ is declared; nothing more when $E$ is a sort name; `SortArity(site, s, n, k)` alone when $E = s(t_1, \ldots, t_k)$ and the last declaration of $s$ has $n \neq k$ parameters; and otherwise, for each argument $t_i$ in order, one `UnboundParamVar(site, v)` for each variable $v$ of $t_i$ that $\Gamma$ does not bind, in ascending byte order of $v$, or, when there is none, `ParamIllTyped(site, s, i, expected, found)` when $t_i$ has no sort in $\Gamma$ (with `found` reporting the failure) or a sort that differs from the parameter sort $P_i$ with $p_1, \ldots, p_{i-1}$ replaced by $t_1, \ldots, t_{i-1}$ (`expected` and `found` being those two sorts as written).

### Outside the domain

For a well-formed theory the result is `Ok(())` ([L-theories-1](#L-theories-1)); there is no list. No other input is outside the domain.

### Definitions used

$D(T)$, well-formedness at a site, the sort of a term and the context are those of [L-theories-1](#L-theories-1). The sites `SortParam(sort, i)`, `OpInput(op, i)` and `OpOutput(op)` name the parameter, input and output of the named declaration. The byte order of names is the lexicographic order of their UTF-8 bytes.

### Proof

1. `typecheck_signature` first computes the duplicate-name defects, then walks the sorts in declaration order, then the operations in declaration order, appending to one list, and returns the list when it is not empty (definition). So the list is the concatenation of the three parts of the Statement in that order, provided each part is as stated.
2. Part 1. For each kind in the stated order, the procedure scans the names of that kind and collects, into a set ordered by bytes, every name seen before; it then appends one `DuplicateSymbol` per collected name in that order. A name shared by $m \geq 2$ symbols is collected (at its second occurrence) and, being in a set, appears once.
3. Part 2. For the parameter $i$ of a sort the procedure first tests whether $p_i$ is among the earlier parameter names and appends `DuplicateBinder` if so; it then appends `UndeclaredSort` when the head is declared and outside $D(T)$, and otherwise runs the expression check with the earlier parameters as context. These are the stated defects in the stated order.
4. Part 3. For the input $i$ of an operation the procedure appends `DuplicateBinder` when $x_i$ is among the earlier input names, then runs the expression check with the earlier inputs as context; after the inputs it runs the expression check on the output with all inputs as context.
5. The expression check, at a site with a context, looks up the last declaration of the head. Without one it appends `UndeclaredSort` and returns. A sort name returns with nothing more. An application with an argument count different from the declared parameter count appends `SortArity` with the declared and given counts and returns. Otherwise, for each argument in order it collects the argument's variables that the context does not bind, as a set ordered by bytes; when the set is not empty it appends one `UnboundParamVar` per member and goes to the next argument; when it is empty it computes the instantiated parameter sort and the argument's sort and appends `ParamIllTyped` when inference fails or the two sorts differ. These are the expression defects as defined.
6. By steps 2 to 5 each part is as stated, and by step 1 the list is their concatenation. Each defect is appended at most once at its site, so each is reported once.

### Executable counterpart

- Law ID: `theories.q2`
- Reference: `panproto_reference::theory::typecheck_signature`
- Enumerator: `panproto_reference::enumerate::theories::raw_signatures; bound: sorts=2, operations=1, arity=2, binders=2`
- Exhaustive differential test: `signature_checking_agrees_with_the_reference_on_every_signature; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=signature_checking_agrees_with_the_reference_on_every_signature)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/typecheck.rs`
- File: `crates/panproto-gat/src/sort.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## Classes are named by their majority name, and freshened names never collide {#L-theories-3}

### Statement

Let $D$ be a diagram and $O$ composition options such that `compose`$(D, O)$ returns a composition $C$, and fix one symbol kind. For a class $K$ of that kind (a sort or operation class, or an equation, directed-equation or policy class), write $\mathrm{cand}(K)$ for its *candidate*: the name that occurs most often among the names of $K$'s members, ties going to the least such name by bytes. Write $N(K)$ for the name $C$ gives $K$. Then:

1. $\mathrm{cand}(K)$ depends only on the multiset of $K$'s member names.
2. Under `Amalgamate`, $N(K) = \mathrm{cand}(K)$ for every sort and operation class $K$.
3. Under `ExplicitIdentity` with `Reject`, $N(K) = \mathrm{cand}(K)$ for every class $K$, and no two classes of one kind share a candidate.
4. Under `ExplicitIdentity` with `Freshen`, the names $N(K)$ of the classes of one kind are pairwise distinct and differ from every reserved name; a class whose candidate no other class of its kind shares and that is not reserved keeps $N(K) = \mathrm{cand}(K)$ unless that candidate is later taken by a qualified name, and every other class gets $\ell.\mathrm{cand}(K)$, with $\ell$ the least node label among its members, possibly followed by suffixes $\#k$.
5. `report.freshened` lists, for exactly the classes $K$ with $N(K) \neq \mathrm{cand}(K)$, the members of $K$, $\mathrm{cand}(K)$ and $N(K)$.

The reserved names are those of the generated equations of [L-theories-4](#L-theories-4), and only equation classes have them.

### Outside the domain

When `compose` refuses, there is no composition to name. The refusals that belong to naming are: under `ExplicitIdentity` with `Reject`, `NameClash(kind, name, classes)` when two or more classes of one kind share the candidate `name`, or one class's candidate is a reserved name, `classes` being every such class (and the reserved name's holder as a one-member class), ascending; and under `Amalgamate`, `NameConflict(kind, name, first, second)` when an equation class's candidate is a reserved name, `second` being the holder. The remaining refusals are those of [L-theories-4](#L-theories-4).

### Definitions used

The classes, the node labels and the generated equations are those of [L-theories-4](#L-theories-4). The suffixing rule is: while some name is shared by two or more classes of one kind, or equals a reserved name, every class holding such a name $n$ is renamed $n\#k$, where $k$ is the class's position, counted from one, among the classes holding $n$ in ascending order of their sorted member lists. A *pass* is one application of this rule to all such names at once.

### Proof

1. The candidate is computed from the count of each member name and the byte order of names, both functions of the multiset of names; this gives part 1.
2. Under `Amalgamate`, the composition joins every two same-kind symbols of different nodes that share a name, so the members of distinct sort or operation classes have disjoint name sets. The candidate of a class is one of its members' names, so two distinct classes cannot share a candidate, no clash arises, and $N(K) = \mathrm{cand}(K)$; this gives part 2.
3. Under `ExplicitIdentity` with `Reject`, naming refuses with `NameClash` as soon as a candidate is shared or reserved. Since $C$ was returned, no candidate is shared or reserved, no class is renamed, and $N(K) = \mathrm{cand}(K)$; this gives part 3.
4. Under `ExplicitIdentity` with `Freshen`, naming first renames every class whose candidate is shared or reserved to $\ell.\mathrm{cand}(K)$ and leaves the others with their candidates, then applies the suffixing rule until no name is shared or reserved. When it stops, the names of one kind are pairwise distinct and none is reserved, by its stopping condition. It remains to show that it stops.
5. Each pass renames a class $K$ holding $n$ to $n\#k$, which extends $K$'s name, so every class's name only grows by appended components $\#k$ (with $k$ a decimal numeral, which contains no `#`).
6. Two classes that hold one name $n$ in some pass both take part in that pass and are renamed to $n\#a$ and $n\#b$ with $a \neq b$. From then on their names are $n\#a\alpha$ and $n\#b\beta$, with $\alpha$ and $\beta$ sequences of components. Both start with $n$, and the rest of each splits uniquely at its `#` characters into components, the first being $a$ and $b$ respectively; so the two names differ in every later pass, and the two classes share a name in at most one pass.
7. A class holding a reserved name $r$ in some pass is renamed to a longer name, and by step 5 never again holds $r$. So each class holds each reserved name in at most one pass.
8. A pass happens only when some name is shared by two classes or equals a reserved name. By steps 6 and 7, the number of passes is at most the number of pairs of classes of the kind plus the number of classes times the number of reserved names, a finite number. So naming stops, and part 4 follows from step 4.
9. Every renaming in steps 3 and 4 records one freshening with the class's members, its candidate and its final name exactly when the final name differs from the candidate, and no other entry is recorded; this gives part 5.

### Executable counterpart

- Law ID: `theories.q3`
- Reference: `panproto_reference::theory::compose`
- Enumerator: `panproto_reference::enumerate::theories::bare_signatures; bound: sorts=2, operations=1, legs=2, options=4`
- Exhaustive differential test: `composition_agrees_with_the_reference_on_every_span; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_agrees_with_the_reference_on_every_span)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/compose.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## The colimit identifies exactly what the diagram relates, with total provenance and a commuting cocone {#L-theories-4}

### Statement

Let $D$ be a diagram with nodes $n_1, \ldots, n_m$ (labels and theories $T_{n_i}$) and edges $e_1, \ldots, e_q$ (each a morphism from the theory of its source node to that of its target node), and let $O$ be composition options. Suppose `compose`$(D, O)$ returns a composition $C$ with result theory $R$. Then:

1. The sort classes and the operation classes of $C$ are exactly the equivalence classes of the relation $\sim$ defined below, and each class $K$ has the name $N(K)$ of [L-theories-3](#L-theories-3).
2. $R$ declares one sort for each sort class with a declared member, namely the declaration of the class's least declared member with every sort and operation name replaced by its class's name, and one operation for each operation class, built in the same way from its least member. The sort classes with no declared member are the *free names* of $C$; $R$ does not declare them.
3. $R$'s equations are one representative per equation class (the least member's translated equation, named by [L-theories-3](#L-theories-3)) together with every generated equation; its directed equations and policies are one representative per class. Equations are classified by their translated sides with local binders compared by position and free variables held fixed by name, in the stated orientation; directed equations also by their implementation, inverse, value kinds and classification; policies by value kind and strategy. Under `ExplicitIdentity` an element joins the class of equal content; under `Amalgamate` it joins the class of equal content that holds its name.
4. The provenance is total and exact: every sort symbol (declared or used without declaration), operation, equation, directed equation and policy of every node has exactly one image, the name of its class; `added_eqs` maps each generated equation's name to its edge's position $k$ in the canonical edge order and the operation it identifies.
5. For each node $n_i$, `inclusions`$[i]$ sends every sort symbol and every operation of $n_i$ to its class's name.
6. The cocone commutes: for every edge $e : s \to t$, every sort assignment $a \mapsto b$ of $e$ between symbols of $s$ and $t$ gives $s.a$ and $t.b$ one image, and every operation assignment $o \mapsto o'$ by name gives $s.o$ and $t.o'$ one image; for every assignment of $o$ to a term, $R$ holds the generated equation that equates $N([s.o])$ applied to $o$'s explicit inputs with that term in result names.
7. Every inclusion is a theory morphism: `check_morphism`$(T_{n_i}, R, \mathrm{inclusions}[i])$ returns `Ok(())`.
8. The report lists, sorted by check and subject, `NodeWellFormed`, `InclusionIsMorphism` and `EquationImage` for each node, `LegIsMorphism` and `CoconeCommutes` for each edge, and `ClassSignaturesAgree`, `UniversalIdentity`, `ResultWellTyped` and `NameClashFree` for the diagram. `CoconeCommutes` for an edge is `PassedByEquation` with the least name of the equations that edge generated, and `Passed` when it generated none. `InclusionIsMorphism` for a node, and `UniversalIdentity`, are `Inapplicable` with reason `OpenTheory` listing the free names involved (ascending) when there are any, and `Passed` otherwise. Every other record is `Passed`. The status is `Failed` when a record failed, else `Incomplete` when one is inapplicable or incomplete, else `Passed`; `name_clash_free` holds exactly when every two same-kind, same-name symbols of different nodes are related by $\sim_E$ and every two same-name equations (directed equations, policies) of different nodes have one image.

The relation $\sim_E$ on symbols of one kind is the least equivalence relation containing: $s.a \sim_E t.b$ for every edge $e : s \to t$ and sort assignment $a \mapsto b$ of $e$ such that $a$ is a sort symbol of $s$ and $b$ one of $t$; $s.o \sim_E t.o'$ for every operation assignment $o \mapsto o'$ by name such that $o$ and $o'$ are operations of $s$ and $t$; and, for every sort name $u$ some node uses without declaring, $n.u \sim_E d$ for every such use $n.u$ and every declared sort $d$ named $u$, or, when no node declares $u$, $n.u \sim_E n'.u$ for every two such uses. The relation $\sim$ is $\sim_E$ under `ExplicitIdentity`, and under `Amalgamate` the least equivalence relation containing $\sim_E$ and every pair of same-kind, same-name symbols of different nodes.

### Outside the domain

`compose` refuses, with the first of these in this order.

1. `InvalidNodeLabel(label)` for the first node, in diagram order, whose label is empty or contains `.` or `#`; `DuplicateNodeLabel(label)` for the first label met twice (checked node by node, the label test first).
2. `UnknownNodeLabel(label)` for the first edge, in order, whose source or (then) target label names no node.
3. `InvalidNode(label, defects)` for the first node whose signature defects ([L-theories-2](#L-theories-2)) include any other than `UndeclaredSort` of a name the node does not declare; `defects` are those.
4. `InvalidLeg { edge, detail }` for the first edge that is not a theory morphism ([L-theories-7](#L-theories-7)), with that check's refusal, or whose morphism sends a sort name its source uses without declaring to a name its target neither declares nor uses (`ImplicitSortImageUnknown`).
5. Under `ExplicitIdentity`, `NameClash(Sort, u, classes)` for the first sort name $u$ (ascending) used without declaration whose declarations lie in two or more classes of $\sim_E$ without the uses, `classes` being those classes.
6. For the first sort class, then the first operation class, in ascending order of member lists, whose members disagree: with `first` its least declared member and `second` the least member that disagrees with it, `NameConflict(kind, name of first, first, second)` under `Amalgamate` when `first` and `second` are not related by $\sim_E$, and otherwise `SortSignatureMismatch(first, second, detail)` (the first difference in the order arity, parameter sort by position, kind, closure, a used but undeclared member counting as arity zero) for sorts, and for operations `QuotientNotRepresentable { first, second, NeedsSortEquation }` when the two signatures agree in their inputs' and output's head classes and implicit flags, and `OpSignatureMismatch(first, second)` otherwise.
7. The naming refusals of [L-theories-3](#L-theories-3) for sort and operation classes.
8. `IllTypedTransport { source, .. }` for the first translated equation, in the order: node equations (nodes in diagram order), generated equations (in canonical edge order, operations ascending, `source` being the operation), directed equations (nodes in diagram order), that does not typecheck against $R$'s sorts and operations.
9. Under `Amalgamate`, `NameConflict(kind, name, first, second)` for the first equation (directed equation, policy), in ascending source order, whose name a class of other content holds, `first` being that class's least member; then the naming refusals of [L-theories-3](#L-theories-3) for these classes.
10. The refusals of [L-theories-5](#L-theories-5).

### Definitions used

A diagram, its labels and composition options are as in `TheoryDiagram` and `ComposeOptions`. A *symbol* is a pair of a node label and a name; the *sort symbols* of a node are its declared sorts and the sort names its signatures use without declaring. Two members of a sort class *disagree* when their signatures differ once every sort and operation name is replaced by its class: arity, the name-free form of each parameter sort, kind, and the set of constructor classes; two members of an operation class disagree when their name-free signatures with implicit flags differ in the same sense. The *canonical edge order* sorts the edges by source label, target label and the canonical bytes of the morphism. A *generated equation* is the equation $N([s.o])(x_1, \ldots, x_r) = t'$ named `{s}.{o}~{k}`, for each edge $e : s \to t$ at position $k$ of the canonical edge order and each operation $o$ that $e$ assigns to a term $t$, the $x_i$ being $o$'s explicit inputs and $t'$ being $t$ with each operation renamed to its class's name in $t$'s node. The *least* member of a class is the least in the order of (label, name). Theory morphisms and `check_morphism` are those of [L-theories-7](#L-theories-7); the union-find closure is that of [L-theories-6](#L-theories-6).

### Proof

1. The composition creates one element per sort symbol (declared or used without declaration) and per operation of every node, then joins, for each edge in order, every sort assignment and every operation assignment by name whose two symbols exist, then joins the uses of each undeclared name as the definition of $\sim_E$ says. Each join merges the two current classes, so after these joins the classes are those of the least equivalence relation containing the joined pairs ([L-theories-6](#L-theories-6), whose argument holds for any finite list of joins). These are the classes of $\sim_E$.
2. Under `Amalgamate` it then joins every two same-kind, same-name symbols of different nodes, and by the same argument the classes become those of $\sim$; under `ExplicitIdentity` it joins nothing more and $\sim = \sim_E$. Naming gives each class $N(K)$ ([L-theories-3](#L-theories-3)). This proves part 1.
3. The result's sorts are built class by class from the least declared member's declaration, every sort and operation name replaced by the class name of its node's symbol, and a class with no declared member contributes no sort and is recorded as free; its operations are built in the same way from each class's least member. This proves part 2.
4. Every node equation, translated into result names, is classified as the Statement says, and each class contributes its least member's translation under its name; every generated equation is added under its own name and is not classified with the node equations. Directed equations and policies are classified in the same way. This proves part 3.
5. Each sort symbol and operation lies in exactly one class (step 1), and the composition records each symbol's class name; each node equation, directed equation and policy lies in exactly one class of step 4 and is recorded with that class's name; each generated equation is recorded in `added_eqs` with its position and operation. Nothing else is recorded. This proves part 4.
6. `inclusions`$[i]$ is built from the provenance entries of $n_i$'s sort symbols and operations, which by step 5 are their class names. This proves part 5.
7. For an edge $e : s \to t$ and a sort assignment $a \mapsto b$ between existing symbols, step 1 joined $s.a$ and $t.b$, so they lie in one class and have one image; the same holds for operation assignments by name. For an assignment of $o$ to a term, step 4 added the generated equation. This proves part 6.
8. Part 7. Fix $n_i$ and write $\iota$ for its inclusion. Every declared sort $a$ of $n_i$ goes to the sort $R$ declares for its class (part 2), which exists since $a$ is a declared member. The composition passed the class checks of refusal 6, so $a$ and the least declared member of its class agree in arity, kind, constructor classes and the name-free form of every parameter sort after class translation; $\iota$'s action on $a$'s parameter sorts is exactly that translation, and $R$'s declaration is the least member's, translated. So arity, kind, parameter sorts up to binder names and the constructor image set all match, and the sort checks of [L-theories-7](#L-theories-7) pass. Every operation $o$ goes by name to its class's operation, whose declaration is the least member's translated, and the class checks make $o$'s translated signature equal to it up to binder names; every sort head of $o$'s signature is a sort symbol of $n_i$, so $\iota$ maps it. So the operation checks pass. Every equation of $n_i$ translates under $\iota$ to its translated sides, which, with local binders compared by position and free variables held fixed, are the sides of its class's representative in $R$ (step 4), in the same orientation. Thus the syntactic preservation test succeeds; the same holds for directed equations. Hence `check_morphism` returns `Ok(())`.
9. Part 8. The composition records the listed checks with the stated outcomes, sorts them, aggregates the status by the stated rule, and computes `name_clash_free` from $\sim_E$ (the classes before the `Amalgamate` joins) and from the provenance of equations, directed equations and policies, as the Statement says.

### Executable counterpart

- Law ID: `theories.q4`
- Reference: `panproto_reference::theory::compose`
- Enumerator: `panproto_reference::enumerate::theories::{bare_signatures,class_operation_scope,class_sort_scope,sort_mismatch_spans}`
- Bound: sorts=2, operations=1, legs=2, options=4, plus four sort-mismatch connector spans.
- Class-agreement scopes: 2,809 syntactically unique, well-formed operation signatures give 7,890,481 ordered pairs; 254 syntactically unique, well-formed sort declarations give 64,516 ordered pairs. A dependent input or parameter may mention only an earlier $X$ binder, whereas an output may mention the full telescope. The scopes cross both implicit flags, structural and value kinds, open and closed declarations, and the bare and empty-application spellings of $X$.
- Scoped witnesses: twelve operation signatures and ten sort declarations add arity three, nested applications, holes under an unused local binding, and shadowing. Alpha-equivalent and capture-distinguishing twins cover nested local bindings and case branches, including repeated names that resolve to the last local binder. Duplicate telescope binders remain excluded because [L-theories-1](#L-theories-1) rejects them.
- Public-composition coverage: the ordered-pair sweep sends every pair through the production and reference composition functions, counts successful composition, `NeedsSortEquation`, operation-signature refusal and all four sort verdicts, and requires every class to occur. Both engines expose each class-agreement verdict at the public composition boundary.
- Exhaustive differential test: `composition_agrees_with_the_reference_on_every_span; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_agrees_with_the_reference_on_every_span)'`
- Exhaustive differential test: `composition_agrees_on_every_class_signature_pair; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_agrees_on_every_class_signature_pair)'`
- Exhaustive differential test: `composition_reports_every_sort_mismatch_detail_like_the_reference; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_reports_every_sort_mismatch_detail_like_the_reference)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/compose.rs`
- File: `crates/panproto-gat/src/quotient.rs`
- File: `crates/panproto-gat/src/check_status.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## A composition's result is a well-formed, well-typed theory {#L-theories-5}

### Statement

Let $D$ be a diagram and $O$ composition options such that `compose`$(D, O)$ returns a composition $C$ with result theory $R$ and free names $F$ ([L-theories-4](#L-theories-4)). Then:

1. every defect `typecheck_signature`$(R)$ reports is `UndeclaredSort(site, u)` with $u \in F$; in particular, when $F$ is empty, $R$ is well formed ([L-theories-1](#L-theories-1));
2. every equation and every directed equation of $R$ is well typed in $R$: its two sides have one sort under the sorts its variables receive from their positions as operation arguments;
3. $R$ passes the remaining checks of the theory body: every implicit input of every operation occurs in the sort of an explicit input or in the output sort, and every closed sort's constructors are operations of that sort, every other operation of that sort being defined by an equation whose left side applies it to distinct variables;
4. $R$'s name is `O.result_name`, its `extends` is empty, and its sorts, operations, equations, directed equations and policies are each in ascending name order.

### Outside the domain

`compose` refuses with `IllTypedResult(defects)` when $R$'s signature has a defect other than `UndeclaredSort` of a free name, `defects` being those; and with `IllTypedResult([])` when the signature passes but a check of part 3 fails. These come after every refusal of [L-theories-4](#L-theories-4). A translated equation that is not well typed is refused earlier, with `IllTypedTransport` ([L-theories-4](#L-theories-4), refusal 8).

### Definitions used

Well-formedness and the defects are those of [L-theories-1](#L-theories-1) and [L-theories-2](#L-theories-2); free names, the classes and the generated equations are those of [L-theories-4](#L-theories-4). An equation is *well typed* in a theory when, after each free variable receives the sort required at the operation-argument positions where it occurs (equal sorts at all of them, with implicit inputs solved by first-order unification), both sides have a sort and the two sorts are equal.

### Proof

1. After the result is assembled, the composition runs `typecheck_signature` on $R$, discards the `UndeclaredSort` defects whose sort is a free name, and refuses with `IllTypedResult` if any defect remains. Since $C$ was returned, none remained; this gives part 1, and when $F$ is empty there is no defect at all, so $R$ is well formed by [L-theories-1](#L-theories-1).
2. Before classifying equations, the composition checks every translated node equation, every generated equation and every translated directed equation (as the equation between its two sides) for well-typedness against $R$'s sorts and operations, and refuses with `IllTypedTransport` at the first that fails. Since $C$ was returned, all of them are well typed. Each equation of $R$ is either a class representative, which is one of the translated node equations, or a generated equation; each directed equation of $R$ is a class representative, one of the translated directed equations. Well-typedness depends only on the sorts and operations of $R$, which do not change after this check. This gives part 2.
3. Last, the composition runs the remaining theory-body checks on $R$ and refuses with `IllTypedResult([])` if one fails. Since $C$ was returned, all passed; this gives part 3.
4. The composition names $R$ by `O.result_name`, leaves `extends` empty, sorts the sorts and operations by name, and the classification yields equations, directed equations and policies in ascending name order, the generated equations being added and the whole list sorted again. This gives part 4.

### Executable counterpart

- Law ID: `theories.q5`
- Reference: `panproto_reference::theory::compose`
- Enumerator: `panproto_reference::enumerate::theories::signatures; bound: nodes=2, sorts=2, operations=1, equations=1, options=4`
- Exhaustive differential test: `composition_agrees_with_the_reference_on_every_discrete_pair; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_agrees_with_the_reference_on_every_discrete_pair)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/compose.rs`
- File: `crates/panproto-gat/src/typecheck.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## The union-find closure labels each element by the least member of its class {#L-theories-6}

### Statement

For every size $n$ and every finite list $I = ((l_1, r_1), \ldots, (l_m, r_m))$ of pairs of naturals, `quotient`$(n, I)$ returns `None` when some $l_j$ or $r_j$ is at least $n$, and otherwise `Some`$(\lambda)$ where, for every $i < n$, $\lambda(i) = \min\{\, j < n : j \equiv_I i \,\}$ and $\equiv_I$ is the least equivalence relation on $\{0, \ldots, n-1\}$ containing every pair of $I$.

### Outside the domain

There is none: the statement covers every size and list, the out-of-range case included.

### Definitions used

An equivalence relation is reflexive, symmetric and transitive; the least one containing a set of pairs is the intersection of all that contain it, and its classes partition $\{0, \ldots, n-1\}$.

### Proof

1. If some pair of $I$ has a member at least $n$, `quotient` returns `None` when it reaches the first such pair, before it returns anything else (definition). Assume from here that every member of every pair is less than $n$.
2. Write $I_k$ for the first $k$ pairs of $I$ and $\lambda_k$ for the labels after the procedure has processed them; $\lambda_0(i) = i$. We show by induction on $k$ the invariant: $\lambda_k(i) = \lambda_k(i')$ exactly when $i \equiv_{I_k} i'$, and $\lambda_k(i)$ is the least member of $i$'s $\equiv_{I_k}$ class.
3. For $k = 0$, $\equiv_{I_0}$ is equality and $\lambda_0$ is the identity, so both claims hold.
4. Assume the invariant for $k$ and let $(l, r)$ be pair $k+1$, with $a = \lambda_k(l)$, $b = \lambda_k(r)$ and $c = \min(a, b)$. The procedure sets to $c$ every label equal to $a$ or $b$ and leaves every other label unchanged.
5. The least equivalence containing $I_{k+1}$ has as classes those of $\equiv_{I_k}$, except that the classes of $l$ and $r$ are merged (they may coincide). By the invariant, the labels equal to $a$ are exactly the members of $l$'s class and those equal to $b$ exactly the members of $r$'s class, so after step 4 the members of the merged class share the label $c$, every other class keeps its own label, and distinct classes still have distinct labels (an unchanged class's label is neither $a$ nor $b$, so it is not $c$). This proves the first claim for $k+1$.
6. By the invariant, $a$ and $b$ are the least members of the classes of $l$ and $r$, so $c = \min(a, b)$ is the least member of their union; the other classes keep their least members as labels. This proves the second claim for $k+1$.
7. By induction the invariant holds for $k = m$, and $\equiv_{I_m} = \equiv_I$, so $\lambda_m(i)$ is the least member of $i$'s class, as stated.

### Executable counterpart

- Law ID: `theories.union-find-closure`
- Reference: `panproto_reference::theory::quotient`
- Enumerator: `panproto_reference::enumerate::theories::identifications; bound: size=4, pairs=3`
- Exhaustive differential test: `quotient_closure_agrees_with_the_reference_on_every_identification_list; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=quotient_closure_agrees_with_the_reference_on_every_identification_list)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/quotient.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## A morphism check accepts exactly the theory morphisms, and reports the first failure {#L-theories-7}

### Statement

Let $S$ and $T$ be theories and $m$ a pair of maps, $m_{\mathrm{sort}}$ from sort names to sort names and $m_{\mathrm{op}}$ from operation names to images (an operation name, or a term). Write $\hat m$ for the *whole action* of $m$ on terms of $S$: it renames an application of an operation sent to an operation, replaces an application of an operation $o$ sent to a term $t$ by $t$ with $o$'s inputs replaced, without capture, by the translated arguments, replaces each case branch's constructor by its constructor image (keeping it when there is none), and keeps variables, holes and binders. Write $\hat F$ for its action on sort expressions: the head goes through $m_{\mathrm{sort}}$ (kept when unmapped) and each argument through $\hat m$. Then `check_morphism`$(S, T, m)$ returns `Ok(())` exactly when $m$ is a *theory morphism* from $S$ to $T$:

1. for every sort $s$ of $S$, in declaration order: $m_{\mathrm{sort}}(s)$ is defined and names a sort $s'$ of $T$ (its last declaration) with as many parameters as $s$ and the same kind; for each parameter $i$, $\hat F(P_i)$ read under $s$'s parameters before $i$ equals $s'$'s parameter sort $i$ read under $s'$'s parameters before $i$, up to binder names; and if $s$ is closed, the set of constructor images of $s$'s constructors equals the set of $s'$'s constructors (empty when $s'$ is open);
2. for every operation $o : (x_1 : S_1, \ldots, x_n : S_n) \to S_0$ of $S$, in declaration order: $m_{\mathrm{op}}(o)$ is defined; $m_{\mathrm{sort}}$ is defined on the head of every $S_i$ and of $S_0$; and either $m_{\mathrm{op}}(o)$ names an operation $o'$ of $T$ with $n$ inputs whose input sorts and output sort equal $\hat F(S_1), \ldots, \hat F(S_n), \hat F(S_0)$ up to binder names (each read under the inputs before it, the output under all of them), or $m_{\mathrm{op}}(o)$ is a term $t$ every variable of which is one of the $x_i$, which has a sort in $T$ under the context $x_1 : \hat F(S_1), \ldots, x_n : \hat F(S_n)$, and whose sort equals $\hat F(S_0)$ up to the names of local binders;
3. for every equation $l = r$ of $S$, in order, either some equation of $T$ equals $\hat m(l) = \hat m(r)$ or $\hat m(r) = \hat m(l)$ with local binders compared by position and free variables held fixed by name, or the two sides are shown equal in $T$ within 512 rewrite steps for each side: first by the directed equations of $T$ alone, then by those and every equation of $T$ in both orientations;
4. for every directed equation $l \to r$ of $S$, in order, some directed equation of $T$ equals $\hat m(l) \to \hat m(r)$ with local binders compared by position and free variables held fixed by name.

When $m$ is not a theory morphism, `check_morphism` returns the refusal of the first failed condition in the order of the clauses above, each clause's conditions in the order written: `MissingSortMapping`, `SortNotFound`, `SortArityMismatch`, `SortKindMismatch`, `SortParamMismatch` (the first parameter), `ClosureMismatch`; `MissingOpMapping`, `MissingSortMapping` (the first unmapped head, inputs before output), then `OpNotFound`, `OpTypeMismatch` (arity, then the first input, then the output) for an operation image, or `TermAssignmentUnboundVar` (the first unbound variable in order of occurrence), `TermAssignmentIllTyped`, `TermAssignmentSortMismatch` for a term image; `EquationNotPreserved` when rewriting reaches normal forms that differ, `EquationPreservationUnknown` when it exhausts its budget; `DirectedEquationNotPreserved`.

### Outside the domain

There is none: every pair of theories and every pair of maps is in the domain, and every refusal is listed in the Statement. The equation clause is a bounded decision: an equation derivable in $T$ only beyond the step budget, or only by a rewrite sequence the procedure does not follow, is refused with `EquationPreservationUnknown` or `EquationNotPreserved`; the check never accepts an equation it has not shown preserved.

### Definitions used

Equality up to binder names, name-free forms and the sort of a term are defined at the top of this page and in [L-theories-1](#L-theories-1). The *constructor image* of a constructor $c$ of $S$ is $k$ when $m_{\mathrm{op}}(c)$ is the operation $k$, $c$ itself when $m_{\mathrm{op}}(c)$ is undefined, and $k$ when $m_{\mathrm{op}}(c)$ is the term $k(x_1, \ldots, x_n)$ whose $i$-th argument is a variable denoting $c$'s $i$-th input (the last input of that name); otherwise $c$ has none, and the set compared contains that term, written out, in its place. Two equations agree up to local binder names when their joint name-free form is identical: a `Let` binder or case-branch binder is read by position, while a free variable retains its name. Thus `x = x` and `y = y` are distinct equations. Rewriting a term with a list of rules applies, at each node after its children are normal, the first rule whose left side matches, counting each application as one step, and gives up when the budget is spent or the term grows more than 64 levels deeper than every rule side and the starting term.

### Proof

1. `check_morphism` runs the sort pass, then the operation pass, then the equation pass, then the directed-equation pass, and returns the first refusal any of them returns, or `Ok(())` when none does (definition).
2. The sort pass visits the sorts of $S$ in declaration order and, for each, refuses with `MissingSortMapping` when $m_{\mathrm{sort}}$ is undefined, `SortNotFound` when $T$ declares no sort of the image's name, `SortArityMismatch` and `SortKindMismatch` when arity or kind differ, `SortParamMismatch` at the first parameter whose translated name-free form differs from the target's, and `ClosureMismatch` when a closed sort's constructor image set (constructors without an image represented by their rendered term) differs from the target's constructor set. So it returns nothing exactly when clause 1 holds, and otherwise the first failure in the stated order.
3. The operation pass visits the operations in declaration order and, for each, refuses with `MissingOpMapping` when $m_{\mathrm{op}}$ is undefined, then with `MissingSortMapping` at the first head of an input or of the output on which $m_{\mathrm{sort}}$ is undefined; it translates the input and output sorts by $\hat F$. For an operation image it refuses with `OpNotFound` when $T$ declares no operation of that name, with `OpTypeMismatch` when the input counts differ, then at the first input, then at the output, whose name-free forms differ. For a term image it refuses with `TermAssignmentUnboundVar` at the first variable in order of occurrence that is not an input, `TermAssignmentIllTyped` when the term has no sort in $T$ under the translated context, and `TermAssignmentSortMismatch` when its sort differs from $\hat F(S_0)$ in name-free form under the inputs (the same binder list on both sides, so up to local binder names). So it returns nothing exactly when clause 2 holds.
4. The equation pass translates each equation's sides by $\hat m$ and accepts it when some equation of $T$ matches it, in either orientation, with local binders compared by position and free variables held fixed. Otherwise it accepts when the two sides agree under that same joint comparison, or when their normal forms under the directed equations agree, or when their normal forms under the directed equations and every equation in both orientations agree; it refuses with `EquationPreservationUnknown` when a normalization exhausts its budget before deciding, and with `EquationNotPreserved` when both normalizations end and differ. So it returns nothing exactly when clause 3 holds.
5. The directed-equation pass accepts a directed equation exactly when $T$ holds one equal to its image under the same treatment of local binders and free variables, in the same orientation, and refuses with `DirectedEquationNotPreserved` otherwise; this is clause 4.
6. By steps 1 to 5, `check_morphism` returns `Ok(())` exactly when clauses 1 to 4 hold, and otherwise the first failure in the stated order.

### Executable counterpart

- Law ID: `theories.morphism-checking`
- Reference: `panproto_reference::theory::check_morphism`
- Enumerator: `panproto_reference::enumerate::theories::{morphisms,whole_action_scope,constructor_scope}; bound: two-sort signatures plus 6 domains x 6 codomains x 7,200 whole-action legs, and 3 constructor domains x 4 constructor codomains x 16 legs`
- Exhaustive differential test: `morphism_checking_agrees_with_the_reference_on_every_assignment; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=morphism_checking_agrees_with_the_reference_on_every_assignment)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/morphism.rs`
- File: `crates/panproto-gat/src/sort.rs`
- File: `crates/panproto-gat/src/typecheck.rs`
- File: `crates/panproto-reference/src/pages/theories.rs` (`check_morphism` and `apply_morphism`)
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.

## Replaying a recipe composes each step from its recorded data {#L-theories-8}

### Statement

Let $\Sigma$ be a recipe with result name $\rho$ and steps $\sigma_0, \ldots, \sigma_{k-1}$, and $M_0$ a registry of named theories. Define the registries $M_{j+1}$ and the names $\nu_{j+1}$ by replaying $\sigma_j$ against $M_j$:

1. `Base(n)`: $n$ is a key of $M_j$; $M_{j+1} = M_j$ and $\nu_{j+1} = n$.
2. `Colimit { left, right, shared_sorts, shared_ops, mode }`: `left` and `right` are keys of $M_j$; the interface $I$ holds, in order, the declaration in $M_j(\mathit{left})$ of each shared sort and then of each shared operation, each of which both $M_j(\mathit{left})$ and $M_j(\mathit{right})$ declare; the composition of the by-name span $\mathit{left} \leftarrow I \rightarrow \mathit{right}$, with the interface labeled `step_j_interface` and the two sides labeled by their names, under `mode`, `Reject`, result name `step_j` and rewrite budget 512, returns a composition; $M_{j+1}$ adds its theory under `step_j`, and $\nu_{j+1}$ = `step_j`.
3. `Diagram { nodes, edges, mode, clash, rewrite_budget, inline }`: each node name resolves to the inline theory of that name or, failing that, to $M_j$'s; the diagram of those nodes, labeled by their names, and of one edge per recorded edge (its maps as recorded) composes under the recorded options with result name `step_j`; $M_{j+1}$ adds its theory under `step_j`, and $\nu_{j+1}$ = `step_j`.

Then `recompose`$(\Sigma, M_0)$ returns $M_k(\nu_k)$ renamed $\rho$ when $k \geq 1$ and every step replays.

### Outside the domain

`recompose` refuses at the first step that does not replay, in step order: `TheoryNotFound(n)` for a base step whose name, a colimit step whose `left` (then `right`), or a diagram step whose first node, is neither inline nor registered; `MissingShared(n)` for the first shared sort, then the first shared operation, that one side does not declare; and `Composition(e)` when the step's composition refuses with `e` ([L-theories-4](#L-theories-4)). An empty recipe refuses with `TheoryNotFound`, since it has no last step.

### Definitions used

The by-name span, its legs (the identity on the interface's sort and operation names, and on the sort names the interface uses without declaring), compositions, modes and clash policies are those of [L-theories-4](#L-theories-4) and [L-theories-3](#L-theories-3).

### Proof

1. `recompose` keeps a registry, initially $M_0$, and for each step in order replays it with the name `step_j`: a base step checks that its name is registered and records it as the last name; a colimit or diagram step computes a theory, adds it under `step_j` and records `step_j` as the last name (definition). By induction on $j$, the registry and last name after step $j$ are $M_{j+1}$ and $\nu_{j+1}$, provided each step replays, and the first step that does not replay ends the replay with its refusal.
2. A colimit step looks up `left`, then `right`, refusing with `TheoryNotFound` for the first missing; builds the interface from each shared sort, then each shared operation, as `left` declares it, refusing with `MissingShared` for the first one either side lacks; builds the by-name span with the stated labels; and composes it with the stated options, refusing with `Composition` when the composition refuses. This is clause 2 and its refusals.
3. A diagram step resolves each node name against the inline theories first and the registry second, refusing with `TheoryNotFound` for the first unresolved name; builds each edge's morphism from its recorded maps; and composes with the recorded mode, clash policy and budget, refusing with `Composition` when the composition refuses. This is clause 3 and its refusals.
4. After the last step, `recompose` looks up the last recorded name, which exists by step 1 when $k \geq 1$, and returns that theory with its name replaced by $\rho$; with no step there is no last name and it refuses with `TheoryNotFound`.

### Executable counterpart

- Law ID: `theories.recompose-replay`
- Reference: `panproto_reference::theory::recompose`
- Enumerator: `panproto_reference::enumerate::theories::recipes; bound: sorts=2, operations=1, steps=1, options=4`
- Exhaustive differential test: `recipe_replay_agrees_with_the_reference_on_every_recipe; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=recipe_replay_agrees_with_the_reference_on_every_recipe)'`
- Property differential test: `randomized_theory_differential; command: cargo nextest run -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=randomized_theory_differential)'`
- File: `crates/panproto-gat/src/composition.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`
- File: `crates/panproto-reference/src/enumerate/theories.rs`

### Mutation equivalences

None.

### Audit record

- Author: lane author
- Auditor: independent verifier
- Date: 2026-10-15
- Commit: 0000000
- Verdict: pass
- Defects: None.
