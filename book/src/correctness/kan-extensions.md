# Kan extensions

This page proves the laws of panproto's three Kan extensions along a migration on tables, graphs, and trees. Delta pulls an instance back, Sigma pushes it forward by a quotient, and Pi pushes it forward by a limit. The page also proves the two adjunctions $\Sigma \dashv \Delta \dashv \Pi$ with their hom-set bijections, the squares that make the graph and tree forms conjugates of the table form, and the two constructions every one of them reads first: the classification of a schema into tables, relations and columns, and the entity functor a migration states between two classified schemas.

The construction itself is explained in the [Explanation](../explanation/index.md) part; we use the notation of [Shared notation](../explanation/semantics/shared-notation.md) and fix the objects below once, so that each block can name them.

#### The objects of this page

A **classified schema** $R$ has a set of tables, a set of relations (edges between two tables, each with a source leg and a target leg), for each table $t$ a set of columns $\mathrm{col}(t)$ (each the name of one attribute arrow from $t$ to a value vertex), the value vertices, and the inner edges (edges leaving a value vertex). Its **entity category** $\mathcal{E}(R)$ has one object per table and one per relation and, besides identities, exactly the two legs $\mathrm{src}_e : e \to \mathrm{src}(e)$ and $\mathrm{tgt}_e : e \to \mathrm{tgt}(e)$ of each relation $e$; no arrow leaves a table except its identity, so two legs never compose.

An **instance** $I$ of $R$ is a finite set of rows, each with a numeric id, a table $t$ and one cell for every column of $t$ and no other, together with a finite set of pairs $(e, a, b)$, $e$ a relation, $a$ a row at $\mathrm{src}(e)$ and $b$ a row at $\mathrm{tgt}(e)$. A cell is a value; a **labelled null** may stand at the top of a cell and nowhere below it. Two values are equal when their canonical bytes are equal. An instance that meets these conditions **conforms** to $R$; the first violation, in ascending row order and within a row in ascending column order, a pair counted at its source row after that row's own violation, is the refusal `NonConforming` with that row, column and reason (`TableOutside`, `MissingCell`, `StrayCell`, `NestedLabeledNull`, `RelationOutside`).

A **homomorphism** $h : I \to J$ of instances of one schema is a map of rows that keeps each row's table, sends each pair to a pair, and has a null map under which every cell of a row equals the cell of its image; a labelled null is sent to the value it first meets in ascending row and column order, and every later occurrence must agree. Two homomorphisms are the same when their row maps are equal and they send every labelled null of the domain to equal values.

A **functor** $F : S' \to T$ between classified schemas is a map of the kept tables of $S$ to tables of $T$, of the kept relations to relations of $T$ with the endpoints the table map gives, and of each kept column $(s, \alpha)$ to a column $F(s, \alpha)$ of $F(s)$; the kept part $S'$ holds the kept tables with their kept columns and the kept relations. On $\mathcal{E}(S')$ it acts by the table and relation maps and sends each leg of $e$ to the same leg of $F(e)$; the legs compose correctly because a kept relation's image runs between the images of its endpoints. $F$ is **surjective on attributes** when every column of every table $d$ of $T$ is $F(s, \alpha)$ for some kept column of some table $s$ with $F(s) = d$. The **fibre** of $d$ is the ascending list of kept tables $s$ with $F(s) = d$.

The **graph form** of an instance has a node per row, one value subtree per cell (a leaf holding the value, or a record or list subtree that folds to it), an attribute edge from the row to its cell, and an edge per pair. The **encoding** $E$ reads an encodable graph back as an instance and a layout recording every id, position, decoration and subtree; the **decoding** $D$ lays an instance out as a graph through a layout. The **tree form** of a fan-free graph with exactly one node no edge reaches is the tree read from that node.

#### The finite scopes

Every law below has an exhaustive differential test over a stated scope and a property differential test. The **bounded family** has ten migrations: (i) two tables merged with disjoint columns; (ii) two tables merged with a shared column; (iii) a column rename; (iv) a target column outside the image; (v) a target table outside the image; (vi) a relation carried to a loop across a merge of tables; (vii) a partial self relation; (viii) two columns of one table sent to one column over the alphabet $\{0, 1, n_0, n_1\}$ of two integers and two labelled nulls; (ix) that column merge over the four float bit patterns $+0$, $-0$ and the two quiet NaNs; and (x) an identity carrying a relation. Its objects are every labelled instance of every source and target with at most three rows per table and six in all, every subset of the possible pairs of each relation, every cell drawn from the case's alphabet: 248,552 labelled objects, whose per-case counts are recomputed by a separate product formula. Pi is run at the product bounds 4 and 81.

The 3,918,714,467 ordered source-target pairs of the bounded family are covered through a quotient. Two labelled objects of one case and side are in one **hom profile** when one is carried onto the other by a permutation of the rows within each table together with a permutation of the case's labelled-null alphabet. Each profile is represented by the image of least canonical bytes under these permutations, and its size is counted independently as the index of the representative's stabilizer in the permutation group. The bounded family has 18,811 profiles and 19,120,495 ordered representative pairs, whose class sizes multiply out to exactly the 3,918,714,467 labelled pairs. The two blocks on the hom-set bijections prove that the quotient is exact for every observable they check.

The **graph scope** is the encoding spec (a table with one structured column, a table without columns and one relation, every cell one of seven forms: a constant, both null forms, a record subtree, a list subtree, a leaf holding the list the subtree folds to, and a labelled null) with every graph of at most four table nodes in four presentations, the eighteen single-site mutations of every presentation with at most two table nodes at eight sites each, five hom families, and ten graph migrations: four merges, three column cases and three relation merges. The **generated cases** are functors and instances drawn from a seed: up to four tables per side with up to three columns each, up to four relations, tables merged or dropped, columns merged, dropped or missed, at most three rows per table and six in all, and integer, float and labelled-null cells.

## Delta is precomposition {#L-kan-extensions-1}

### Statement

For every functor $F : S' \to T$ between classified schemas and every instance $J$ of $T$, $\Delta_F J$ is the instance of $S'$ with, for each kept table $s$ and each row $r$ of $J$ at $F(s)$, one copy row $r_s$ at $s$ whose cell at each kept column $\alpha$ of $s$ is $J(r)$ at $F(s, \alpha)$, and, for each kept relation $e$ and each pair $(F(e), a, b)$ of $J$, the pair $(e, a_{\mathrm{src}(e)}, b_{\mathrm{tgt}(e)})$. Copies are made in ascending $(s, r)$ order; the first copy of a row keeps the row's id and every later copy takes the next id above every id of $J$. The provenance records each copy with its table and the row it copies. On a homomorphism $h : J \to J'$, $\Delta_F h$ sends $r_s$ to $h(r)_s$. Delta never creates a labelled null.

### Outside the domain

A $J$ that does not conform to $T$ is refused with `NonConforming` naming its first violation. When a later copy would need an id above `u32::MAX`, Delta refuses with `RowIdsExhausted` and returns nothing. Delta on a map that is no homomorphism $J \to J'$ is refused.

### Definitions used

Classified schema, instance, conformance, functor, kept part and homomorphism, as defined at the top of this page.

### Proof

1. Each copy $r_s$ lies at the kept table $s$ and holds exactly the cells at the kept columns of $s$, because each $F(s, \alpha)$ is a column of $F(s)$ and $r$ conforms to $T$, so $J(r)$ has a cell at it. A cell is copied unchanged, so no labelled null is nested below a cell's top level. Each pair $(e, a_{\mathrm{src}(e)}, b_{\mathrm{tgt}(e)})$ joins rows at the endpoints of $e$ because $F(e)$ runs from $F(\mathrm{src}(e))$ to $F(\mathrm{tgt}(e))$, so $a$ lies at $F(\mathrm{src}(e))$ and has a copy at $\mathrm{src}(e)$, and likewise $b$. Thus $\Delta_F J$ conforms to $S'$.
2. The ids are pairwise distinct: the first copy of each row keeps that row's id, which is distinct from every other row's, and every later copy takes a fresh id above every id of $J$, each handed out once. When none is left the operation refuses before returning, so no partial instance escapes.
3. $\Delta_F J$ is $J \circ F$ on $\mathcal{E}(S')$ with the copies named by the provenance: at a table $s$ the rows of $J \circ F$ are the rows of $J$ at $F(s)$, which the copies $r_s$ enumerate bijectively; at a relation $e$ they are the pairs of $J$ at $F(e)$, which the pairs of step 1 enumerate bijectively, and the legs agree because each copied pair joins the copies of its endpoints.
4. For a homomorphism $h : J \to J'$, the map $r_s \mapsto h(r)_s$ keeps tables; a copied cell $J(r)(F(s,\alpha))$ equals $J'(h(r))(F(s,\alpha))$ under the null map of $h$ because $h$ preserves every cell of $r$; a copied pair goes to the copy of the image pair, which $h$ preserves. So $\Delta_F h$ is a homomorphism whose null map is forced by its cells. The identity of $J$ gives $r_s \mapsto r_s$, and for $h' \circ h$ the copy $r_s$ goes to $h'(h(r))_s$ either way, so $\Delta_F$ is a functor.

### Executable counterpart

- Law ID: `kan.delta`
- Reference: `panproto_reference::rel::delta`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/migration_functor.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Sigma is the quotient of the disjoint union {#L-kan-extensions-2}

### Statement

For every functor $F : S' \to T$ and every instance $I$ of $S$, $\Sigma_F I$ is computed from the restriction $I'$ of $I$ to the kept part: each row $r$ of $I'$ at $s$ keeps its id at $F(s)$; the pairs are the images $(F(e), a, b)$ of the pairs $(e, a, b)$ of $I'$, as a set. Each output cell $(r, \beta)$ is equated with $I'(r)(\alpha)$ for every kept column $\alpha$ of $r$'s table with $F(s, \alpha) = \beta$, in ascending $(r, \beta, \alpha)$ order; an output cell with no such column holds a fresh labelled null, numbered from one above the largest null of $I$ (zero when it has none) in ascending $(r, \beta)$ order. The equations generate classes over cells, labelled nulls and constants; a class is represented by its constant, else by its least labelled null. The provenance records each fresh null with its cell, and each null of $I'$ whose class another value represents with that value. On a homomorphism $h : I \to I'$ of instances of $S'$, $\Sigma_F h$ sends each row to $h$ of it, with the null map its cells force.

On the graph form, the rows of graph Sigma at a target table are the disjoint union of the rows at the tables of its fibre, and its relation edges are the separated reflection $L_{\mathrm{sep}}$ of the pointwise left Kan extension of the input's entity presheaf: two elements of one relation with equal endpoints are identified, and nothing else is. The total extensions of a table and of a graph agree with Sigma on their domain: the table extension is $\Sigma_F$ exactly, the graph extension is the presheaf left Kan extension before $L_{\mathrm{sep}}$.

### Outside the domain

A non-conforming $I$ is refused with `NonConforming`. When the equations join two classes holding different constants, Sigma refuses with `SigmaConstantClash` naming the output row and column of the equation that first joins them and the two constants; no instance satisfies the equations. When a fresh null would need an id above `u64::MAX`, Sigma refuses with `NullIdsExhausted`. Outside their domain the total extensions refuse an element without an image (`UnmappedAnchor`, `UnmappedEdge`), a value the target declares no column for or does not admit (`Validation`), and a clash.

### Definitions used

Instance, conformance, functor, kept part, entity category and homomorphism, as defined at the top of this page. The **pointwise left Kan extension** of a functor $P : \mathcal{E}(S') \to \mathbf{Set}$ along $F$ has at each object $d$ of $\mathcal{E}(T)$ the colimit over the comma category $(F \downarrow d)$: the disjoint union of the sets $P(c)$ indexed by the pairs $(c, f : F(c) \to d)$, quotiented by joining $x$ at $(c, f)$ with $P(u)(x)$ at $(c', f')$ for each arrow $u : c \to c'$ with $f' \circ F(u) = f$. Its **separated reflection** $L_{\mathrm{sep}}$ identifies two elements of one relation object whose two legs agree.

### Proof

1. The rows. Let $d$ be a table of $T$. The arrows of $\mathcal{E}(T)$ into $d$ are its identity and the legs of the relations with an endpoint at $d$, so the comma objects are $(s, \mathrm{id}_d)$ for each kept table $s$ of the fibre of $d$ and $(e, \ell)$ for each kept relation $e$ and leg $\ell$ of $F(e)$ ending at $d$. Each element of $P(e)$ at $(e, \ell)$ is joined, along the leg $u$ of $e$ with $F(u) = \ell$, with its endpoint at $(s, \mathrm{id}_d)$. No arrow of $\mathcal{E}(S')$ leaves a table except its identity, so no two rows are joined. Thus the colimit at $d$ is the disjoint union of the rows of the fibre tables, which is the row set of $\Sigma_F I$ with each row keeping its id.
2. The pairs. At a relation $\rho$ of $T$ the only arrows into $\rho$ are its identity, so the comma objects are the kept relations $e$ with $F(e) = \rho$ and the colimit is the disjoint union of their pairs, each leg sending a pair to the class of its endpoint. $L_{\mathrm{sep}}$ identifies two of these elements exactly when their endpoints agree, which leaves the set of image pairs $(F(e), a, b)$.
3. The cells. A cell of the output is an unknown of the value side; the equations of the statement are the conditions under which the copy of $r$ in $\Delta_F \Sigma_F I$ agrees with $r$, which is what makes the unit a homomorphism (block [Sigma is left adjoint to Delta](#L-kan-extensions-4), step 1). Closing them under transitivity gives the classes, and a homomorphism out of $\Sigma_F I$ must send every member of a class to one value. A class with two distinct constants thus admits no value, which is the refusal; otherwise the least choice is the class's constant, else a labelled null shared by the class, and the class's least labelled null is chosen so that a null of $I$ keeps its id. A cell with no preimage column is constrained by nothing, so it is a fresh unknown.
4. Conformance. Each output row lies at a table of $T$ and holds exactly the columns of that table, because each column $\beta$ either has a preimage column of the row's table, whose cell exists, or receives a fresh null. A representative is a constant or a labelled null at the top of a cell; a constant of $I'$ conforms, so it has no nested null. Each image pair joins rows at the endpoints of $F(e)$. Fresh nulls are distinct from every null of $I$ because they are numbered above them, and distinct from each other because each is handed out once.
5. Functoriality. For $h : I \to I'$, the row map $r \mapsto h(r)$ keeps tables after $F$ and sends pairs to pairs. A cell of $\Sigma_F I$ is the representative of a class whose members are cells of $I$ and nulls; $h$ sends each member to a member of the corresponding class of $\Sigma_F I'$, a fresh null of $(r, \beta)$ to the cell of $(h(r), \beta)$, so the cells agree under the forced null map. Identities and composites are kept because the row maps compose.
6. The graph form. By steps 1 and 2, graph Sigma (decode after table Sigma after encode) has at each target table the rows of the fibre and at each relation the image pairs, which is $L_{\mathrm{sep}}$ of the presheaf left Kan extension of the graph's entity presheaf; parallel edges of the input that land on one pair are identified, and rows never are.
7. The extensions. On the domain where every element of the input has an image and every target column of a realized table is an image, the table extension computes the rows, pairs and cells of steps 1 to 3 with no fresh null, so it equals $\Sigma_F$; the graph extension relabels each node and edge without identifying edges, which is the presheaf left Kan extension before $L_{\mathrm{sep}}$.

### Executable counterpart

- Law ID: `kan.sigma`
- Reference: `panproto_reference::rel::sigma`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/migration_functor.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Pi is the limit of compatible families {#L-kan-extensions-3}

### Statement

For every functor $F : S' \to T$ surjective on attributes, every instance $I$ of $S$ and every bound $b$, $\Pi_F I$ is computed from the restriction $I'$ of $I$ to the kept part: at each table $d$ of $T$ with fibre $s_1 < \dots < s_k$, one row per tuple $(r_1, \dots, r_k)$ of rows of $I'$ at $s_1, \dots, s_k$ whose cells at the preimages of each column $\beta$ of $d$ are all equal, that common value being its cell at $\beta$ (a table outside the image has the one empty tuple); at each relation $\rho$ of $T$, the pair $(u, v)$ of rows over $\mathrm{src}(\rho)$ and $\mathrm{tgt}(\rho)$ whenever, for every kept relation $x$ with $F(x) = \rho$, the components of $u$ at $\mathrm{src}(x)$ and of $v$ at $\mathrm{tgt}(x)$ form a pair of $I'$ at $x$ (every pair when no relation is carried to $\rho$). A row of a table whose fibre is one table keeps the id of the row it projects to; every other row takes the next id above every id of $I$, the image tables first by the least table of their fibre, tuples ascending, then the tables outside the image by name. The provenance records each row with its table and tuple. On a homomorphism $h$, $\Pi_F h$ sends the row of a tuple to the row of the tuple of its images. Pi creates no labelled null.

### Outside the domain

When $F$ is not surjective on attributes, Pi refuses with `AttributeOutsideImage` naming the least table and column outside the image; no right adjoint exists there, because a column outside the image would have to range over every value. A non-conforming $I$ is refused with `NonConforming`. Before enumerating a product of two or more fibre tables, or the pair product of a relation to which nothing is carried, whose size exceeds $b$, Pi refuses with `ProductSizeExceeded` naming the vertex, the exact product and $b$. When a fresh id would pass `u32::MAX`, it refuses with `RowIdsExhausted`.

### Definitions used

Instance, conformance, functor, surjective on attributes, fibre and homomorphism, as defined at the top of this page.

### Proof

1. At a table $d$ the comma category $(d \downarrow F)$ of $\mathcal{E}$ holds the kept tables of the fibre, with no arrow between two of them, and the kept relations whose image has an endpoint at $d$, each with its legs. A compatible family picks one row at each fibre table and, at each such relation, the pair its legs force; on the entity part it is a tuple of fibre rows. The attribute part adds that the value $\Pi$ assigns to a column $\beta$ of $d$ is the value of every preimage column, so a tuple is admitted exactly when those values agree, and that value is the cell. A table outside the image has an empty fibre, whose one family is the empty tuple; it has no columns because $F$ is surjective on attributes and every column of $d$ is an image only if some fibre table exists.
2. At a relation $\rho$ the families are pairs of families over its endpoints, compatible with each kept relation carried to $\rho$; this is the stated condition, and with nothing carried every pair is compatible.
3. The result conforms to $T$: each row holds exactly the columns of its table (step 1 assigns each column one value), and each pair joins rows at the endpoints of $\rho$. Its cells are cells of $I'$, which conform, so no labelled null is nested and none is created.
4. The bound is checked on the product sizes before they are enumerated, so a refusal leaves no partial output, and below the bound the enumeration is complete. The ids are distinct by the argument of [Delta is precomposition](#L-kan-extensions-1), step 2.
5. For a homomorphism $h : I \to I'$, the images of the components of an admitted tuple agree at every column under the null map of $h$, so they form an admitted tuple of $\Pi_F I'$, and image pairs satisfy the compatibility with carried relations because $h$ preserves pairs. The map keeps tables and is identity- and composite-preserving because it acts componentwise.

### Executable counterpart

- Law ID: `kan.pi`
- Reference: `panproto_reference::rel::pi`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/pi.rs`
- File: `crates/panproto-inst/src/migration_functor.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Sigma is left adjoint to Delta {#L-kan-extensions-4}

### Statement

For every functor $F : S' \to T$, every instance $I$ of $S'$ that Sigma accepts and every instance $J$ of $T$, the unit $\eta_I : I \to \Delta_F \Sigma_F I$ sends each row $r$ at $s$ to its copy $r_s$, with each labelled null of $I$ sent to the representative of its class; the counit $\varepsilon_J : \Sigma_F \Delta_F J \to J$ sends each copy $r_s$ to $r$, with each fresh null of a cell $(r_s, \beta)$ sent to $J(r)(\beta)$. Both are homomorphisms, and the triangle identities $\Delta_F \varepsilon_J \circ \eta_{\Delta_F J} = \mathrm{id}_{\Delta_F J}$ and $\varepsilon_{\Sigma_F I} \circ \Sigma_F \eta_I = \mathrm{id}_{\Sigma_F I}$ hold, compared as homomorphisms. When Sigma clashes on $I$, $\mathrm{Hom}(I, \Delta_F J)$ is empty for every $J$.

### Outside the domain

The unit refuses as $\Sigma_F$ refuses on $I$ ([Sigma is the quotient of the disjoint union](#L-kan-extensions-2)) and as $\Delta_F$ refuses on $\Sigma_F I$; the counit refuses as $\Delta_F$ refuses on $J$ ([Delta is precomposition](#L-kan-extensions-1)) and as Sigma refuses on $\Delta_F J$. Neither returns a partial map.

### Definitions used

Instance, homomorphism and functor, as defined at the top of this page; $\Sigma_F$ and $\Delta_F$ with their provenance, from the two blocks the statement links.

### Proof

1. The unit is a homomorphism. The copy $r_s$ lies at $s$, the table of $r$. Its cell at a kept column $\alpha$ is $\Sigma_F I (r)(F(s, \alpha))$, the representative of the class of the output cell $(r, F(s, \alpha))$, which by the equations of Sigma contains $I(r)(\alpha)$; so $I(r)(\alpha)$, read through the null map that sends each null to the representative of its class, equals the copied cell. A pair $(e, a, b)$ of $I$ goes to $(e, a_{\mathrm{src}(e)}, b_{\mathrm{tgt}(e)})$, the copy of the image pair $(F(e), a, b)$ of $\Sigma_F I$.
2. The counit is a homomorphism. $\Sigma_F \Delta_F J$ keeps the ids of the copies, the copy $r_s$ lying at $F(s)$, the table of $r$. Its cell $(r_s, \beta)$ is, when $\beta$ has preimage columns, the representative of the class of the copied cells $J(r)(\beta)$, all equal, so the class holds no two constants and its value is $J(r)(\beta)$ or a null standing for it; with no preimage it is a fresh null, sent to $J(r)(\beta)$. A pair $(F(e), a_{\mathrm{src}(e)}, b_{\mathrm{tgt}(e)})$ goes to $(F(e), a, b)$, a pair of $J$.
3. The triangle identities. $\Delta_F \varepsilon_J \circ \eta_{\Delta_F J}$ sends $r_s$ to the copy at $s$ of $\varepsilon_J(r_s) = r$, which is $r_s$, and every null of $\Delta_F J$ is a null of $J$ that both maps fix; so it is the identity. $\varepsilon_{\Sigma_F I} \circ \Sigma_F \eta_I$ sends a row $r$ of $\Sigma_F I$ to the copy $r_s$ in $\Sigma_F \Delta_F \Sigma_F I$ and then back to $r$; a fresh null of $\Sigma_F I$ goes to the fresh null of the same cell and back to it. So it is the identity.
4. The clash. If Sigma clashes on $I$, two constants $c \ne c'$ lie in one class. A homomorphism $g : I \to \Delta_F J$ would send each cell of $I$ to the copied cell of $J$, and each output cell $(r, \beta)$ of a class to one cell of $J$, namely $J(g(r))(\beta)$; following the equations that join $c$ and $c'$, the values $c$ and $c'$ would both equal one value of $J$, which is impossible because they differ. Thus no such $g$ exists.

### Executable counterpart

- Law ID: `kan.sigma-delta-adjunction`
- Reference: `panproto_reference::rel::sigma`
- Reference: `panproto_reference::rel::delta`
- Reference: `panproto_reference::rel::homs`
- Reference: `panproto_reference::rel::row_bijection`
- Reference: `panproto_reference::rel::sigma_unit`
- Reference: `panproto_reference::rel::sigma_counit`
- Reference: `panproto_reference::rel::sigma_transpose_forward`
- Reference: `panproto_reference::rel::sigma_transpose_inverse`
- Enumerator: `panproto_reference::enumerate::kan_extensions::hom_profile_classes; bound: rows_per_table=3, rows=6, profiles=18811, pairs=19120495`
- Exhaustive differential test: `o14_hom_profile_pairs_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=o14_hom_profile_pairs_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Delta is left adjoint to Pi {#L-kan-extensions-5}

### Statement

For every functor $F : S' \to T$ surjective on attributes, every bound $b$, every instance $J$ of $T$ and every instance $I$ of $S'$, the unit $\mu_J : J \to \Pi_F \Delta_F J$ sends each row $r$ at $d$ to the row whose tuple holds the copy $r_s$ at each table $s$ of the fibre of $d$ (the one row when $d$ is outside the image), and the counit $\kappa_I : \Delta_F \Pi_F I \to I$ sends the copy at $s$ of a row with tuple $t$ to the component of $t$ at $s$; both have the identity null map and are homomorphisms, and the triangle identities $\kappa_{\Delta_F J} \circ \Delta_F \mu_J = \mathrm{id}_{\Delta_F J}$ and $\Pi_F \kappa_I \circ \mu_{\Pi_F I} = \mathrm{id}_{\Pi_F I}$ hold wherever every Pi they read is within $b$.

### Outside the domain

When $F$ is not surjective on attributes both refuse with `AttributeOutsideImage`. The unit refuses with `ProductSizeExceeded` when $\Pi_F \Delta_F J$ exceeds $b$, the counit when $\Pi_F I$ does; each also refuses as Delta and Pi refuse on its inputs ([Delta is precomposition](#L-kan-extensions-1), [Pi is the limit of compatible families](#L-kan-extensions-3)).

### Definitions used

Instance, homomorphism, functor, surjective on attributes and fibre, as defined at the top of this page; $\Delta_F$ and $\Pi_F$ with their provenance, from the blocks the statement links.

### Proof

1. The unit is a homomorphism. The tuple of copies of $r$ is admitted by $\Pi_F \Delta_F J$: the copy $r_s$ holds at each kept column $\alpha$ the cell $J(r)(F(s, \alpha))$, so at every preimage of a column $\beta$ of $d$ the copies hold $J(r)(\beta)$, all equal; the admitted row's cell at $\beta$ is $J(r)(\beta)$, and surjectivity on attributes makes every column of $d$ such a $\beta$. A pair $(\rho, u, v)$ of $J$ goes to the pair of the tuples of copies, which is compatible with every relation $x$ carried to $\rho$ because $\Delta_F J$ holds $(x, u_{\mathrm{src}(x)}, v_{\mathrm{tgt}(x)})$.
2. The counit is a homomorphism. The copy at $s$ of a row with tuple $t$ holds at each kept column $\alpha$ of $s$ the row's cell at $F(s, \alpha)$, which by admission equals the cell of the component $t_s$ at $\alpha$. A pair of $\Delta_F \Pi_F I$ at a kept relation $x$ is the copy of a pair $(F(x), u, v)$ of $\Pi_F I$, whose components at the endpoints of $x$ form a pair of $I$ at $x$ by compatibility.
3. The triangle identities. $\kappa_{\Delta_F J} \circ \Delta_F \mu_J$ sends a copy $r_s$ to the copy at $s$ of the tuple of copies of $r$, then to its component at $s$, which is $r_s$. $\Pi_F \kappa_I \circ \mu_{\Pi_F I}$ sends a row $w$ with tuple $t$ to the tuple of copies of $w$, then to the tuple of their components, which is $t$, the row $w$ itself. Both null maps are identities.

### Executable counterpart

- Law ID: `kan.delta-pi-adjunction`
- Reference: `panproto_reference::rel::delta`
- Reference: `panproto_reference::rel::pi`
- Reference: `panproto_reference::rel::homs`
- Reference: `panproto_reference::rel::row_bijection`
- Reference: `panproto_reference::rel::pi_unit`
- Reference: `panproto_reference::rel::pi_counit`
- Reference: `panproto_reference::rel::pi_transpose_forward`
- Reference: `panproto_reference::rel::pi_transpose_inverse`
- Enumerator: `panproto_reference::enumerate::kan_extensions::hom_profile_classes; bound: rows_per_table=3, rows=6, profiles=18811, pairs=19120495`
- Exhaustive differential test: `o14_hom_profile_pairs_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=o14_hom_profile_pairs_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/pi.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The Sigma and Delta hom-sets are in bijection {#L-kan-extensions-6}

### Statement

For every functor $F : S' \to T$, every instance $I$ of $S'$ that Sigma accepts and every instance $J$ of $T$, the forward transpose $\Phi(h) = \Delta_F h \circ \eta_I$, which sends a row $r$ at $s$ to the copy $h(r)_s$, and the inverse transpose $\Phi^{-1}(k) = \varepsilon_J \circ \Sigma_F k$, which sends a row $r$ of $\Sigma_F I$ to the row of $J$ that the copy $k(r)$ copies, are mutually inverse bijections $\mathrm{Hom}(\Sigma_F I, J) \cong \mathrm{Hom}(I, \Delta_F J)$, natural in $I$ and $J$: for homomorphisms $a : I' \to I$ and $c : J \to J'$, $\Phi(c \circ h \circ \Sigma_F a) = \Delta_F c \circ \Phi(h) \circ a$, and likewise for $\Phi^{-1}$.

### Outside the domain

The transposes refuse as the unit and counit of [Sigma is left adjoint to Delta](#L-kan-extensions-4) refuse, and a supplied map that is no homomorphism between the stated instances is refused (`NotANaturalMap`, naming the first offending row). When Sigma clashes on $I$ both hom-sets are empty.

### Definitions used

Instance, homomorphism, functor and hom profile, as defined at the top of this page; the unit and counit of [Sigma is left adjoint to Delta](#L-kan-extensions-4).

### Proof

1. $\Phi$ and $\Phi^{-1}$ land in the stated hom-sets: each is a composite of homomorphisms ([Delta is precomposition](#L-kan-extensions-1), step 4; [Sigma is the quotient of the disjoint union](#L-kan-extensions-2), step 5; [Sigma is left adjoint to Delta](#L-kan-extensions-4), steps 1 and 2).
2. $\Phi^{-1} \circ \Phi = \mathrm{id}$. For $h : \Sigma_F I \to J$, $\Phi^{-1}(\Phi(h)) = \varepsilon_J \circ \Sigma_F \Delta_F h \circ \Sigma_F \eta_I = h \circ \varepsilon_{\Sigma_F I} \circ \Sigma_F \eta_I$ by naturality of $\varepsilon$ (it sends a copy to the row it copies, which commutes with applying $h$ to rows and cells), and this is $h$ by the second triangle identity. $\Phi \circ \Phi^{-1} = \mathrm{id}$ by the symmetric argument with naturality of $\eta$ and the first triangle identity. Thus both are bijections.
3. Naturality. $\Phi(c \circ h \circ \Sigma_F a) = \Delta_F c \circ \Delta_F h \circ \Delta_F \Sigma_F a \circ \eta_{I'} = \Delta_F c \circ \Delta_F h \circ \eta_I \circ a$ by functoriality of $\Delta_F$ and naturality of $\eta$; the inverse transpose is natural by the dual computation.
4. The hom-profile quotient is exact. Let $\alpha : I \to \bar{I}$ and $\gamma : J \to \bar{J}$ be the profile transports onto the representatives (row permutations within tables, with a permutation of the case's labelled nulls).

   (i) Each labelled object has exactly one profile, because the representative is the image of least canonical bytes over a finite group, and its class size is the index of its stabilizer.

   (ii) Conjugation $h \mapsto \gamma \circ h \circ \Sigma_F \alpha^{-1}$ is a bijection $\mathrm{Hom}(\Sigma_F I, J) \to \mathrm{Hom}(\Sigma_F \bar{I}, \bar{J})$ with inverse the conjugation by $\alpha$ and $\gamma^{-1}$, and $k \mapsto \Delta_F \gamma \circ k \circ \alpha^{-1}$ is a bijection of the other hom-set. The transport $\Sigma_F \alpha$ sends rows by $\alpha$, merged nulls by their representatives, and fresh nulls by their cells, for Sigma's equations, classes and fresh numbering are carried by $\alpha$ slot for slot.

   (iii) By step 3 these bijections commute with $\Phi$ and $\Phi^{-1}$.

   (iv) They carry identities, composites and naturality squares to identities, composites and naturality squares, because conjugation is a functor.

   (v) The production and reference operations are equivariant under the same transports, since both compute the same exact outputs on every labelled object (block [Sigma is the quotient of the disjoint union](#L-kan-extensions-2)) and their transposes are the formulas of the statement, which are written in the transported data. The one exception is the first-witness payload of `SigmaConstantClash`, which names the first clashing equation in numeric row order, so its row and constant pair need not correspond under a row permutation. For that refusal the comparison between two members of one profile checks only that both refuse with `SigmaConstantClash` and that each returned witness belongs to the clash set the transport carries; on every labelled object the payload is compared exactly.

   Executing every representative pair and weighting it by the product of its two class sizes thus checks every one of the 3,918,714,467 labelled pairs.

### Executable counterpart

- Law ID: `kan.sigma-delta-hom-bijection`
- Reference: `panproto_reference::rel::homs`
- Reference: `panproto_reference::rel::row_bijection`
- Reference: `panproto_reference::rel::sigma_unit`
- Reference: `panproto_reference::rel::sigma_counit`
- Reference: `panproto_reference::rel::sigma_transpose_forward`
- Reference: `panproto_reference::rel::sigma_transpose_inverse`
- Enumerator: `panproto_reference::enumerate::kan_extensions::hom_profile_classes; bound: rows_per_table=3, rows=6, profiles=18811, pairs=19120495`
- Exhaustive differential test: `o14_hom_profile_pairs_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=o14_hom_profile_pairs_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The Delta and Pi hom-sets are in bijection {#L-kan-extensions-7}

### Statement

For every functor $F : S' \to T$ surjective on attributes, every bound $b$, every instance $J$ of $T$ and every instance $I$ of $S'$ with $\Pi_F I$ within $b$, the forward transpose $\Psi(g)$, which sends a row $r$ at $d$ to the row of $\Pi_F I$ whose tuple holds $g(r_s)$ at each table $s$ of the fibre of $d$, and the inverse transpose $\Psi^{-1}(k) = \kappa_I \circ \Delta_F k$, which sends a copy $r_s$ to the component at $s$ of the tuple of $k(r)$, are mutually inverse bijections $\mathrm{Hom}(\Delta_F J, I) \cong \mathrm{Hom}(J, \Pi_F I)$, natural in $J$ and $I$. Wherever $\Pi_F \Delta_F J$ is within $b$ too, $\Psi(g) = \Pi_F g \circ \mu_J$.

### Outside the domain

When $F$ is not surjective on attributes the transposes refuse with `AttributeOutsideImage`; when $\Pi_F I$ exceeds $b$, with `ProductSizeExceeded`; a supplied map that is no homomorphism is refused with `NotANaturalMap`. The transposes need $\Pi_F I$ only: they are defined when $\Pi_F \Delta_F J$ exceeds $b$, where only the unit and its component formula are undefined.

### Definitions used

Instance, homomorphism, functor, surjective on attributes, fibre and hom profile, as defined at the top of this page; the unit and counit of [Delta is left adjoint to Pi](#L-kan-extensions-5).

### Proof

1. $\Psi(g)$ is a homomorphism. The tuple $(g(r_s))_s$ is admitted: $g$ sends the copy $r_s$, whose cell at $\alpha$ is $J(r)(F(s,\alpha))$, to a row with an equal cell, so at every preimage of a column $\beta$ of $d$ the images hold $J(r)(\beta)$ under the null map of $g$, and the admitted row holds it at $\beta$. Pairs go to compatible pairs because $g$ preserves the copied pairs. $\Psi^{-1}(k)$ is a composite of homomorphisms.
2. Mutual inverses. $\Psi^{-1}(\Psi(g))$ sends $r_s$ to the component at $s$ of the tuple $(g(r_{s'}))_{s'}$, which is $g(r_s)$. $\Psi(\Psi^{-1}(k))$ sends $r$ to the row whose tuple holds, at each $s$, the component at $s$ of the tuple of $k(r)$; that tuple is the tuple of $k(r)$, and a row of $\Pi_F I$ is determined by its table and tuple, so the row is $k(r)$. The null maps agree because each transpose sends a null to where the given map sends it.
3. The component formula. When the unit exists, $\Pi_F g \circ \mu_J$ sends $r$ to the row of the tuple $(g(r_s))_s$, which is $\Psi(g)(r)$.
4. Naturality in $J$ and $I$ follows from the formulas by the computation of [The Sigma and Delta hom-sets are in bijection](#L-kan-extensions-6), step 3, with $\Psi$ and $\Pi_F$ in place of $\Phi$ and $\Sigma_F$; both sides of each square send a row to the row of the tuple of the transported components.
5. The hom-profile quotient is exact by the argument of [The Sigma and Delta hom-sets are in bijection](#L-kan-extensions-6), step 4, clauses (i) to (v), with the transports of $\Delta_F$ (a copy $(s, r)$ to $(s, \alpha(r))$) and of $\Pi_F$ (a tuple to the tuple of its transported components) in place of Sigma's; Delta and Pi have no first-witness refusal on the bounded family, so every transported payload is compared exactly.

### Executable counterpart

- Law ID: `kan.delta-pi-hom-bijection`
- Reference: `panproto_reference::rel::homs`
- Reference: `panproto_reference::rel::row_bijection`
- Reference: `panproto_reference::rel::pi_unit`
- Reference: `panproto_reference::rel::pi_counit`
- Reference: `panproto_reference::rel::pi_transpose_forward`
- Reference: `panproto_reference::rel::pi_transpose_inverse`
- Enumerator: `panproto_reference::enumerate::kan_extensions::hom_profile_classes; bound: rows_per_table=3, rows=6, profiles=18811, pairs=19120495`
- Exhaustive differential test: `o14_hom_profile_pairs_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=o14_hom_profile_pairs_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/pi.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Graph Kan extensions are conjugates of the table ones {#L-kan-extensions-8}

### Statement

For a classified schema $X$, a graph is **encodable** over $X$ when it passes five checks in order. The first failure is `NotEncodable` with the least offender of the first failing pass: (1) nodes ascending, no node is a fan and every anchor is a table or value vertex of $X$; (2) edges ascending, every label is a relation, attribute arrow or inner edge of $X$ joining nodes anchored at its endpoints; (3) no two edges share label and endpoints; (4) table nodes ascending and their columns ascending, each table node has exactly one edge along each column; (5) value nodes ascending, each has exactly one in-edge, following the least in-edge reaches a table node, a leaf holds a present or null value, a node with out-edges holds none and its out-edges form a record (no positions, distinct column names) or a list (distinct positions), and no labelled null lies below the top of a cell.

On encodable graphs the encoding $E_X$ is a table and a layout, $D_X(E_X G) = G$, and for every conforming table $T$ and every layout $L$ the table of $E_X(D_X(T, L))$ is $T$. A homomorphism of encodable graphs is a map of table nodes whose encodings form a table homomorphism; $E$ is a bijection on these hom-sets that keeps identities and composites.

For a functor $F$, graph Delta, Sigma and Pi are $D \circ X_F \circ E$ for the table operation $X_F$ of [Delta is precomposition](#L-kan-extensions-1), [Sigma is the quotient of the disjoint union](#L-kan-extensions-2) and [Pi is the limit of compatible families](#L-kan-extensions-3). Sigma and Pi encode over the whole source and restrict to the kept part $\rho$, which keeps the nodes at kept tables and value vertices and the edges at kept arrows between them, so $E(\rho G)$ is the restriction of $E(G)$. The output layout is chosen from the table provenance: a row node keeps its decorations when it has one source row; the contributing input cells of an output cell are the cell Delta copies, the cells of Pi's tuple at the preimages of its column, or, for Sigma, the input cells of the same row at the kept columns the functor sends to its column; each output cell takes the subtree of the least contributing input cell in `(table, row, column)` order that, moved along the functor's value remaps onto its own image column, hangs from the output cell's value vertex and still folds to it, else a record-free leaf; selected ids are kept at their first occurrence when unclaimed and every other node and edge is fresh above every input and layout id. Their units, counits and transposes are those of the table adjunctions read through $E$, and both graph adjunctions hold on the encodable graphs over the kept part and over the target.

Graph provenance names, for every output node and edge, every input node and edge that contributes to it: a row its source rows, a cell's nodes and edges every node and edge of the input cells the table provenance names for that cell, and a relation edge the input edges of its contributing pairs.

### Outside the domain

A schema-taking form refuses with `RelSchema` when either schema has no classification (the source first), then with `Functor` when the migration states no functor; every form then refuses with `NotEncodable` for an input outside the encoding domain, with `Adjunction` carrying the table operation's refusal, and with `NodeIdsExhausted`, `EdgeIdsExhausted` or `EdgePositionsExhausted` when decoding has no id or position left. The checks run in that order and no partial graph or provenance is returned. A graph homomorphism check refuses with the domain's encoding refusal, then the codomain's, then the table homomorphism refusal.

### Definitions used

Classified schema, instance, functor, kept part, graph form, encoding and decoding, as defined at the top of this page; Delta, Sigma, Pi and their adjunctions, from the blocks the statement links.

### Proof

1. $D_X(E_X G) = G$. The layout of $E_X G$ records every node's id and decorations, every cell's subtree with its ids and positions, and every relation edge's id and position. $D_X$ lays one table node per row with the recorded decorations, the recorded subtree for every cell (it folds to the cell, because the cell was read from it), and one edge per pair with the recorded id; no recorded id collides, because they are the ids of one graph. So every node and edge of $G$ is rebuilt with its id, data and position.
2. The table of $E_X(D_X(T, L))$ is $T$. $D_X$ lays a table node per row with the row's id, a subtree per cell folding to the cell (the selected subtree only when it folds to the cell by canonical bytes, else a leaf holding the cell), and one edge per pair; the result passes the five checks because each cell has one rooted, unshared subtree and the table conforms. Reading it back gives the rows, the cells (the folds) and the pairs of $T$.
3. Full faithfulness. A map of table nodes is a graph homomorphism exactly when its encoding is a table homomorphism, by definition; distinct node maps encode to distinct row maps; so $E$ is a bijection on hom-sets, and it keeps identities and composites because composition is composition of the node and null maps.
4. The restriction. $\rho G$ keeps exactly the rows at kept tables, their cells at kept columns with their subtrees, and the pairs at kept relations, which is the restriction of $E(G)$ to the kept part; Sigma and Pi read only the kept part, so encoding the whole source and restricting gives the same table as encoding $\rho G$ over the kept part.
5. The adjunctions transport. For $G$ encodable over the kept part and $H$ over the target, $\mathrm{Hom}(D \Sigma E G, H) \cong \mathrm{Hom}(\Sigma E G, E H) \cong \mathrm{Hom}(E G, \Delta E H) \cong \mathrm{Hom}(G, D \Delta E H)$: the outer bijections are step 3 together with steps 1 and 2 (the table of $E D$ is the table), the middle one is [The Sigma and Delta hom-sets are in bijection](#L-kan-extensions-6). Each step is natural because $E$ keeps composites, so the composite is natural and its value at an identity is the transported unit. Delta and Pi transport the same way with [The Delta and Pi hom-sets are in bijection](#L-kan-extensions-7).
6. Layout, ids and provenance. The layout rules choose a representative of the output's isomorphism class, which the laws do not observe; they are deterministic because every choice is a least element in a fixed order, and the ids are distinct because kept ids are admitted once and outside the row ids and fresh ids lie above every input and layout id. The provenance attributes to each output node and edge exactly the input objects the table provenance names for its row, cell or pair, on the nodes and edges actually decoded for that cell.

### Executable counterpart

- Law ID: `kan.graph-conjugation-square`
- Reference: `panproto_reference::encode::decode_table`
- Reference: `panproto_reference::encode::encode_graph`
- Reference: `panproto_reference::encode::g_delta`
- Reference: `panproto_reference::encode::g_pi`
- Reference: `panproto_reference::encode::g_sigma`
- Reference: `panproto_reference::migration::graph_square`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/graph_kan.rs`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Table Kan extensions are the encodings of the graph ones {#L-kan-extensions-9}

### Statement

For every functor $F$ and every conforming instance $T$, the table of $E(g_X(D(T)))$ is $X_F(T)$ for $X$ each of Delta, Sigma and Pi, with $D(T)$ the record-free decoding: the table route and the graph route agree on every observable of the table, and a refusal of one is the same refusal of the other.

### Outside the domain

A refusal of the table operation is the graph route's `Adjunction` refusal with the same payload; a table that does not conform is refused by both with the same `NonConforming` reason.

### Definitions used

Instance, conformance, encoding and decoding, as defined at the top of this page; the graph Kan extensions of [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).

### Proof

1. $g_X(D T) = D(X_F(E(D T)), L)$ for the layout $L$ the graph operation selects, by the definition in [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).
2. The table of $E(D T)$ is $T$ by step 2 of that block, so $g_X(D T) = D(X_F T, L)$.
3. The table of $E(D(X_F T, L))$ is $X_F T$, again by step 2, because $X_F T$ conforms ([Delta is precomposition](#L-kan-extensions-1), step 1; [Sigma is the quotient of the disjoint union](#L-kan-extensions-2), step 4; [Pi is the limit of compatible families](#L-kan-extensions-3), step 3).
4. When $X_F$ refuses on $T$, the graph route reaches the table operation (the record-free decoding of a conforming table is encodable, step 2) and returns its refusal wrapped in `Adjunction`, unchanged.

### Executable counterpart

- Law ID: `kan.table-conjugation-square`
- Reference: `panproto_reference::migration::table_square`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/graph_kan.rs`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/pi.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Tree Kan extensions are the graph ones read back {#L-kan-extensions-10}

### Statement

For every functor $F$ and every fan-free tree $X$, tree Sigma and tree Pi of $X$ are graph Sigma and graph Pi (restricted forms) of $X$'s graph form, read back as a tree from the one node no edge reaches, and tree Delta is graph Delta read back the same way. On the tree-Kan domain, where $F$ is injective on tables, relations and columns and every target table, relation and column is an image, the graph form of each production tree function (the extension for Sigma, the tree Pi and the tree pullback for Delta) is identical to the graph route's output.

### Outside the domain

Tree Delta refuses a functor that sends two source vertices (kept tables, or the value vertices of kept columns) to one target with `NonInjectiveVertexMap`, naming the first such pair in ascending source order and their image, and then one that sends two source edges to one with `NonInjectiveEdgeMap` naming the image; tree Pi refuses a merging vertex map with `NonInjectiveVertexMap` naming that same first pair and their image (the production refusal names the image and its whole fibre, whose two least members are that pair). A graph refusal is returned unchanged. A result without exactly one root is refused with `RootCount` and the number of roots, and one with a root but no tree form with `NotATree`.

### Definitions used

Functor, graph form and tree form, as defined at the top of this page; the graph Kan extensions of [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).

### Proof

1. The tree routes are defined as composites of the tree's graph form, a graph Kan extension and the read-back; the merging-map refusals come first because a pullback or product along a merging vertex map duplicates a node into several anchors, which no tree holds.
2. The read-back is the tree form of the output when it has one root and the parent arcs reach it from every node; otherwise its refusal counts the roots or names the failing node, which the statement lists.
3. On the tree-Kan domain both table functors are reindexings along an isomorphism of schemas: Sigma's equations have one preimage per column, so no cell is equated with another and no fresh null arises, and each target table and relation has one source, so rows and pairs are relabelled one for one; Pi's fibres have one table each, so every tuple is one row keeping its id. The encoding commutes with the tree's embedding as a graph, and the layout rules select every unique original subtree and edge with their ids ([Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8), step 6). The tree functions relabel each node and arc along the same isomorphism and keep ids, so their graph forms are identical to the graph route's output.

### Executable counterpart

- Law ID: `kan.tree-conjugation-square`
- Reference: `panproto_reference::encode::t_delta`
- Reference: `panproto_reference::encode::t_pi`
- Reference: `panproto_reference::encode::t_sigma`
- Reference: `panproto_reference::migration::tree_square`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/graph_kan.rs`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/pi.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## A migration states one entity functor {#L-kan-extensions-11}

### Statement

For classified schemas $S$ and $T$ and a migration, the entity functor keeps and maps, in this order: (1) each source table that the vertex remap maps, or that survives and is no remap image, to its remap image or itself, which must be a table of $T$; (2) each relation between kept tables that the edge remap maps, or that survives and is no remap image, to its remap image or to the edge of its kind and name between the endpoint images, which must be a relation of $T$ between those images; (3) each attribute arrow of a kept table that the edge remap maps, to a target attribute arrow from the image table (ending at the value vertex's remap when it has one), and each surviving one that is no image to the target arrow from the image table with its kind and name ending at its value vertex's image, an unmentioned column being dropped; (4a) every edge remap is checked not to cross between relations and attribute arrows, and the remaps of inner edges and value vertices are recorded; and, for the Kan extensions, (4b) each inner-edge remap must keep the value structure: its image is an inner edge of $T$ with the edge's kind and name, and the kind of each endpoint of the image is the kind of the remap image of the corresponding endpoint (an endpoint without a remap keeping its own kind).

### Outside the domain

Each step reports its least offender: `VertexImageNotATable` with the table and its image (1); `IncoherentEdgeImage` with the relation and its image (2, and 4a for a relation sent to an attribute arrow); `IncoherentAttributeImage` with the arrow and its image, or none for a surviving arrow without one (3, and 4a for an attribute arrow sent to a relation); `ValueStructureRemap` with the inner edge (4b). A schema without a classification is refused by [Classification](#L-kan-extensions-12) first.

### Definitions used

Classified schema, kept part and functor, as defined at the top of this page.

### Proof

1. The steps produce a functor $F : S' \to T$: step 1 gives a map of kept tables into tables of $T$; step 2 keeps a relation only when its image runs between the images of its endpoints, so the legs of each kept relation go to the legs of its image and $\mathcal{E}(S') \to \mathcal{E}(T)$ preserves domains, codomains and the trivial composites; step 3 gives each kept column a column of its table's image.
2. Each refusal names a datum that would break this: an image that is no table leaves the object map undefined; an image relation that does not join the endpoint images breaks a leg; an attribute image that is no column of the image table leaves the cell map undefined; a remap across the divide would send an entity object to a value or the reverse.
3. Step 4b accepts exactly the remaps that rename vertices inside values without changing a value's shape: a value records its fields by edge name and kind and its leaves by kind, so a remap that keeps names, kinds and endpoint kinds sends each value to a value of the same form.
4. The order of the checks and the least offender within each are fixed, so the refusal is a function of the input.

### Executable counterpart

- Law ID: `kan.entity-functor`
- Reference: `panproto_reference::rel::entity_functor`
- Enumerator: `panproto_reference::enumerate::kan_extensions::entity_migrations; bound: schemas=6, schema_pairs=36`
- Exhaustive differential test: `entity_functor_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=entity_functor_agrees_with_the_reference)'`
- Property differential test: `randomized_classification_and_entity_functor_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_classification_and_entity_functor_differential)'`
- File: `crates/panproto-inst/src/migration_functor.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Classification {#L-kan-extensions-12}

### Statement

For a schema and a protocol with value kinds $V$ and non-value kinds $N$, the strict classification has as tables the declared tables (the vertices holding a key and the holders and targets of references) and the vertices of a kind in $N$; as value vertices the vertices reachable along edges from the undeclared vertices of a kind in $V$, the walk stopping at tables; as relations the edges between two tables; as columns of a table the edges from it to a value vertex, each named by its name, else its target; and as inner edges the edges leaving a value vertex.

### Outside the domain

The first failing check is the refusal: `AmbiguousKind` for the least kind in both $V$ and $N$; `UnclassifiedKind` for the least undeclared vertex whose kind is in neither; `ValueReachesTable` for the least edge from a value vertex to a table; `DuplicateColumn` for the least table, and column, that two attribute arrows name.

### Definitions used

Classified schema, as defined at the top of this page.

### Proof

1. Every vertex is classified exactly once: a declared vertex is a table; an undeclared one has a kind in $V$ or in $N$ (otherwise the second refusal), not both (otherwise the first); a kind in $N$ makes it a table, a kind in $V$ makes it a value vertex, and the closure adds only vertices that are not tables.
2. An edge from a value vertex into a table would put a table inside a cell, which the third refusal excludes; so every edge out of a value vertex stays among value vertices and is an inner edge, every edge between tables is a relation, and every edge from a table to a value vertex is an attribute arrow.
3. Two attribute arrows of one table with one column name would give a row two cells of one name, which the fourth refusal excludes, so each table's columns are well defined.
4. The checks run in the stated order with the least offender of each, so the refusal is a function of the input.

### Executable counterpart

- Law ID: `kan.relation-classification`
- Reference: `panproto_reference::rel::classify`
- Enumerator: `panproto_reference::enumerate::kan_extensions::classification_schemas; bound: vertices=3, kinds=4, edges=2, schemas=23552`
- Exhaustive differential test: `classification_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=classification_agrees_with_the_reference)'`
- Property differential test: `randomized_classification_and_entity_functor_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_classification_and_entity_functor_differential)'`
- File: `crates/panproto-inst/src/rel_schema.rs`
- File: `crates/panproto-reference/src/pages/kan_extensions.rs`
- File: `crates/panproto-reference/src/enumerate/kan_extensions.rs`

### Mutation equivalences

None.

### Audit record

- Author: Kan-extension proof author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.
