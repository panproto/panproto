# Kan extensions

This page proves the laws of panproto's three Kan extensions along a migration on tables, graphs, and trees. Delta pulls an instance back, Sigma pushes it forward by a quotient, and Pi pushes it forward by a limit. The page also proves the two adjunctions $\Sigma \dashv \Delta \dashv \Pi$ with their hom-set bijections, the squares that make the graph and tree forms conjugates of the table form, and the two constructions every one of them reads first: the classification of a schema into tables, relations and columns, and the entity functor a migration states between two classified schemas.

The construction itself is explained in the [Explanation](../explanation/index.md) part; we use the notation of [Shared notation](../explanation/semantics/shared-notation.md) and fix the objects below once, so that each block can name them.

#### The objects of this page

A **classified schema** $R$ has a set of tables, a set of relations (edges between two tables, each with a source leg and a target leg), for each table $t$ a set of columns $\mathrm{col}(t)$ (each the name of one attribute arrow from $t$ to a value vertex), the value vertices, and the inner edges (edges leaving a value vertex). Its **entity category** $\mathcal{E}(R)$ has one object per table and one per relation and, besides identities, exactly the two legs $\mathrm{src}_e : e \to \mathrm{src}(e)$ and $\mathrm{tgt}_e : e \to \mathrm{tgt}(e)$ of each relation $e$; no arrow leaves a table except its identity, so two legs never compose.

An **instance** $I$ of $R$ is a finite set of rows, each with a numeric id, a table $t$ and one cell for every column of $t$ and no other, together with a finite set of pairs $(e, a, b)$, $e$ a relation, $a$ a row at $\mathrm{src}(e)$ and $b$ a row at $\mathrm{tgt}(e)$. A cell is a value; a **labelled null** may stand at the top of a cell and nowhere below it. Two values are equal when their canonical bytes are equal. An instance that meets these conditions **conforms** to $R$. Otherwise its first violation is the refusal `NonConforming` with a row, a column (none for a table or pair violation) and a reason, found as follows. The rows are read in ascending id; a row at a vertex that is no table of $R$ is `TableOutside`; otherwise the least column that is either a column of its table without a cell (`MissingCell`) or a cell outside its table's columns (`StrayCell`) is the violation; otherwise the least column whose cell holds a labelled null below its top level is `NestedLabeledNull`. A pair is charged to its source row: a pair at an edge that is no relation of $R$ is `RelationOutside`, and a pair whose two rows are not rows of its relation's source and target tables is `IncoherentPair`; among the violating pairs the least source row comes first, a tie going to the least relation. A row's own violation comes before a pair charged to the same or a later row. A production instance cannot hold an incoherent pair, since inserting a pair checks that its rows lie at its relation's endpoint tables, so `IncoherentPair` is a refusal of the reference alone.

A **homomorphism** $h : I \to J$ of instances of one schema is a map of rows that keeps each row's table, sends each pair to a pair, and has a null map under which every cell of a row equals the cell of its image; a labelled null is sent to the value it first meets in ascending row and column order, and every later occurrence must agree. Two homomorphisms are the same when their row maps are equal and they send every labelled null of the domain to equal values. A homomorphism is thus determined by its row map: every labelled null of a conforming domain stands at the top of some cell $(r, \alpha)$, and each homomorphism with that row map sends it to the cell of $h(r)$ at $\alpha$. A map offered as a homomorphism $I \to J$ that is none is refused with `NotANaturalMap` naming the least row of $I$ that has no image, is sent to a row outside its table, has cells that are not exactly its image's cells under the map's null map (a labelled null the map does not list standing for itself), or is the source of a pair not sent to a pair. A pair is thus charged to its source row and competes with the other conditions by that row, so a pair violation at a row precedes a cell violation at a later one.

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

A $J$ that does not conform to $T$ is refused with `NonConforming` naming its first violation. When a later copy would need an id above `u32::MAX`, Delta refuses with `RowIdsExhausted` and returns nothing. Delta on a map $h$ from $J$ to $J'$ refuses first as Delta refuses on $J$, then as it refuses on $J'$, and then, when $h$ is no homomorphism $J \to J'$, with `NotANaturalMap` naming the row defined at the top of this page.

### Definitions used

Classified schema, instance, conformance, functor, kept part and homomorphism, as defined at the top of this page.

### Proof

1. Each copy $r_s$ lies at the kept table $s$ and holds exactly the cells at the kept columns of $s$, because each $F(s, \alpha)$ is a column of $F(s)$ and $r$ conforms to $T$, so $J(r)$ has a cell at it. A cell is copied unchanged, so no labelled null is nested below a cell's top level. Each pair $(e, a_{\mathrm{src}(e)}, b_{\mathrm{tgt}(e)})$ joins rows at the endpoints of $e$ because $F(e)$ runs from $F(\mathrm{src}(e))$ to $F(\mathrm{tgt}(e))$, so $a$ lies at $F(\mathrm{src}(e))$ and has a copy at $\mathrm{src}(e)$, and likewise $b$. Thus $\Delta_F J$ conforms to $S'$.
2. The ids are pairwise distinct: the first copy of each row keeps that row's id, which is distinct from every other row's, and every later copy takes a fresh id above every id of $J$, each handed out once. When none is left the operation refuses before returning, so no partial instance escapes.
3. $\Delta_F J$ is $J \circ F$ on $\mathcal{E}(S')$ with the copies named by the provenance: at a table $s$ the rows of $J \circ F$ are the rows of $J$ at $F(s)$, which the copies $r_s$ enumerate bijectively; at a relation $e$ they are the pairs of $J$ at $F(e)$, which the pairs of step 1 enumerate bijectively, and the legs agree because each copied pair joins the copies of its endpoints.
4. For a homomorphism $h : J \to J'$, the map $r_s \mapsto h(r)_s$ keeps tables; a copied cell $J(r)(F(s,\alpha))$ equals $J'(h(r))(F(s,\alpha))$ under the null map of $h$ because $h$ preserves every cell of $r$; a copied pair goes to the copy of the image pair, which $h$ preserves. So $\Delta_F h$ is a homomorphism whose null map is forced by its cells. The identity of $J$ gives $r_s \mapsto r_s$, and for $h' \circ h$ the copy $r_s$ goes to $h'(h(r))_s$ either way, so $\Delta_F$ is a functor.
5. The refusals on a map. $\Delta_F h$ is formed from $\Delta_F J$ and $\Delta_F J'$ and then from $h$, so it refuses as Delta refuses on $J$, then on $J'$, and only then checks $h$, naming the row of the definition. The test `maps_and_their_refusals_agree_with_the_reference` executes this order on every map, homomorphism or not, between the objects of the bounded family with at most one row per table and two in all, with two refusing objects per side.

### Executable counterpart

- Law ID: `kan.delta`
- Reference: `panproto_reference::rel::delta`
- Reference: `panproto_reference::rel::delta_on_hom`
- Enumerator: `panproto_reference::enumerate::kan_extensions::instances; bound: rows_per_table=3, rows=6, objects=248552`
- Exhaustive differential test: `bounded_family_kan_objects_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=bounded_family_kan_objects_agree_with_the_reference)'`
- Property differential test: `randomized_kan_hom_bijection_differential; command: cargo nextest run -p panproto-inst --test reference_kan --no-tests=fail -E 'test(=randomized_kan_hom_bijection_differential)'`
- File: `crates/panproto-inst/src/adjunction.rs`
- File: `crates/panproto-inst/src/table_restrict.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-inst/src/graph_kan.rs`
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

For every functor $F : S' \to T$ and every instance $I$ of $S$, $\Sigma_F I$ is computed from the restriction $I'$ of $I$ to the kept part: each row $r$ of $I'$ at $s$ keeps its id at $F(s)$; the pairs are the images $(F(e), a, b)$ of the pairs $(e, a, b)$ of $I'$, as a set. Each output cell $(r, \beta)$ is equated with $I'(r)(\alpha)$ for every kept column $\alpha$ of $r$'s table with $F(s, \alpha) = \beta$, in ascending $(r, \beta, \alpha)$ order; an output cell with no such column holds a fresh labelled null, numbered from one above the largest null of $I$ (zero when it has none) in ascending $(r, \beta)$ order. The equations generate classes over cells, labelled nulls and constants; a class is represented by its constant, else by its least labelled null. The provenance records each fresh null with its cell, and each null of $I'$ whose class another value represents with that value. On a homomorphism $h : I \to I'$ of instances of $S'$ such that Sigma accepts both $I$ and $I'$, $\Sigma_F h$ sends each row to $h$ of it, with the null map its cells force.

On the graph form, the rows of graph Sigma at a target table are the disjoint union of the rows at the tables of its fibre, and its relation edges are the separated reflection $L_{\mathrm{sep}}$ of the pointwise left Kan extension of the input's entity presheaf: two elements of one relation with equal endpoints are identified, and nothing else is.

The two total extensions defined below have different domains. The **table extension** of an instance of $S$ equals $\Sigma_F$ of it exactly on the instances whose every row, cell and pair lies at a kept table, column or relation, whose every cell is a value the kind of its source column admits, which give a row to no target table with a column outside the image, and on which Sigma does not clash. The **graph extension** of a graph over $S$ equals, on its rows and relation edges, the presheaf left Kan extension of its entity presheaf before $L_{\mathrm{sep}}$, on the graphs with no node or edge outside the kept part.

### Outside the domain

A non-conforming $I$ is refused with `NonConforming`. Sigma then processes every equation before it draws any fresh null. When an equation joins two classes holding different constants, Sigma refuses with `SigmaConstantClash` naming the first such equation in ascending $(r, \beta, \alpha)$ order: its row $r$, its column $\beta$, the constant of the class that already holds the cells of $r$ at the preimage columns of $\beta$ before $\alpha$ (`left`), and the constant of the class of $I'(r)(\alpha)$ (`right`); no instance satisfies the equations. Only when no equation clashes are the fresh nulls numbered, and Sigma refuses with `NullIdsExhausted` when one would need an id above `u64::MAX`. On a map $h$ from $I$ to $I'$, $\Sigma_F$ refuses as Sigma refuses on $I$, then as it refuses on $I'$, and then with `NotANaturalMap` when $h$ is no homomorphism.

The table extension refuses outside its domain: an element without an image with `UnmappedAnchor` or `UnmappedEdge`, a cell its column's kind does not admit or a target table that receives a row while one of its columns is outside the image with `Validation`, and a clash with `SigmaConstantClash`. The graph extension refuses a graph with a node or edge outside the kept part with `UnmappedAnchor` or `UnmappedEdge`; on the graphs of the graph scope it refuses nothing else, since it neither solves equations nor makes cells.

### Definitions used

Instance, conformance, functor, kept part, entity category and homomorphism, as defined at the top of this page. The **pointwise left Kan extension** of a functor $P : \mathcal{E}(S') \to \mathbf{Set}$ along $F$ has at each object $d$ of $\mathcal{E}(T)$ the colimit over the comma category $(F \downarrow d)$: the disjoint union of the sets $P(c)$ indexed by the pairs $(c, f : F(c) \to d)$, quotiented by joining $x$ at $(c, f)$ with $P(u)(x)$ at $(c', f')$ for each arrow $u : c \to c'$ with $f' \circ F(u) = f$. Its **separated reflection** $L_{\mathrm{sep}}$ identifies two elements of one relation object whose two legs agree. The **entity presheaf** of a graph over $S$ sends each kept table to its nodes there and each kept relation to its edges there, each leg to the endpoint map. The **table extension** of an instance of $S$ (production `functor_extend`) moves each row to the image of its table keeping its id, each pair to the image of its relation as a set, and each cell along the column map, solving the equations of Sigma and making a fresh null at each column outside the image; it validates its input and output against their schemas. The **graph extension** of a graph over $S$ (production `graph_extend`) moves each node to the image of its anchor and each edge to the image of its label, keeping every node id, edge id and position and identifying nothing.

### Proof

1. The rows. Let $d$ be a table of $T$. The arrows of $\mathcal{E}(T)$ into $d$ are its identity and the legs of the relations with an endpoint at $d$, so the comma objects are $(s, \mathrm{id}_d)$ for each kept table $s$ of the fibre of $d$ and $(e, \ell)$ for each kept relation $e$ and leg $\ell$ of $F(e)$ ending at $d$. Each element of $P(e)$ at $(e, \ell)$ is joined, along the leg $u$ of $e$ with $F(u) = \ell$, with its endpoint at $(s, \mathrm{id}_d)$. No arrow of $\mathcal{E}(S')$ leaves a table except its identity, so no two rows are joined. Thus the colimit at $d$ is the disjoint union of the rows of the fibre tables, which is the row set of $\Sigma_F I$ with each row keeping its id.
2. The pairs. At a relation $\rho$ of $T$ the only arrows into $\rho$ are its identity, so the comma objects are the kept relations $e$ with $F(e) = \rho$ and the colimit is the disjoint union of their pairs, each leg sending a pair to the class of its endpoint. $L_{\mathrm{sep}}$ identifies two of these elements exactly when their endpoints agree, which leaves the set of image pairs $(F(e), a, b)$.
3. The cells. A cell of the output is an unknown of the value side; the equations of the statement are the conditions under which the copy of $r$ in $\Delta_F \Sigma_F I$ agrees with $r$, which is what makes the unit a homomorphism (block [Sigma is left adjoint to Delta](#L-kan-extensions-4), step 1). Closing them under transitivity gives the classes, and a homomorphism out of $\Sigma_F I$ must send every member of a class to one value. A class with two distinct constants thus admits no value, which is the refusal; otherwise the least choice is the class's constant, else a labelled null shared by the class, and the class's least labelled null is chosen so that a null of $I$ keeps its id. A cell with no preimage column is constrained by nothing, so it is a fresh unknown. The equations are processed in a fixed order before any fresh null is drawn, so the refusal is a function of the input, and a clash, which leaves no instance at all, is reported whatever ids remain.
4. Conformance. Each output row lies at a table of $T$ and holds exactly the columns of that table, because each column $\beta$ either has a preimage column of the row's table, whose cell exists, or receives a fresh null. A representative is a constant or a labelled null at the top of a cell; a constant of $I'$ conforms, so it has no nested null. Each image pair joins rows at the endpoints of $F(e)$. Fresh nulls are distinct from every null of $I$ because they are numbered above them, and distinct from each other because each is handed out once.
5. Functoriality. For $h : I \to I'$, the row map $r \mapsto h(r)$ keeps tables after $F$ and sends pairs to pairs. A cell of $\Sigma_F I$ is the representative of a class whose members are cells of $I$ and nulls; $h$ sends each member to a member of the corresponding class of $\Sigma_F I'$, a fresh null of $(r, \beta)$ to the cell of $(h(r), \beta)$, so the cells agree under the forced null map. Identities and composites are kept because the row maps compose. $\Sigma_F h$ is formed from $\Sigma_F I$ and $\Sigma_F I'$ before $h$ is checked, which is the stated refusal order on maps.
6. The graph form. By steps 1 and 2, graph Sigma (decode after table Sigma after encode) has at each target table the rows of the fibre and at each relation the image pairs, which is $L_{\mathrm{sep}}$ of the presheaf left Kan extension of the graph's entity presheaf; parallel edges of the input that land on one pair are identified, and rows never are. The test `g_sigma_is_the_separated_reflection_of_the_reference_presheaf_sigma` executes this on every whole-source graph of the graph scope's ten migrations: the reference builds the entity category and presheaf (`rel::entity_category`, `rel::entity_instance`), the pointwise colimit (`rel::presheaf_left_kan`) and its reflection (`rel::separated_pairs`), and each of the six production graph Sigma entry points must equal the reference graph Sigma and be isomorphic, on rows and relation edges, to that reflection.
7. The extensions. By the definitions above, on its domain the table extension keeps each row's id at the image of its table (step 1), takes the image pairs as a set (step 2) and solves the equations of step 3; as every element lies in the kept part, restricting to it changes nothing, and as every column of a target table that receives a row is an image, no fresh null arises, so the result is $\Sigma_F$ of the input, while the domain's conditions are exactly the absence of the three refusals. The graph extension, on a graph inside the kept part, sends the nodes at the fibre of a target table to that table one for one, which is the colimit of step 1, and the edges at the relations carried to $\rho$ to $\rho$ one for one with their endpoints, which is the disjoint union of step 2 before $L_{\mathrm{sep}}$, because it identifies nothing. The test `an_extend_agrees_with_the_reference_sigma_and_its_presheaf` executes both on every whole-source graph of the graph scope's ten migrations: the table extension of the graph's encoding must equal `rel::sigma` exactly and be isomorphic on rows and pairs to `rel::separated_pairs`, the graph extension must be isomorphic on rows and relation edges to `rel::unseparated_pairs`, and each must refuse exactly where its domain is left, with the variants stated.

### Executable counterpart

- Law ID: `kan.sigma`
- Reference: `panproto_reference::rel::sigma`
- Reference: `panproto_reference::rel::sigma_on_hom`
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

For every functor $F : S' \to T$ surjective on attributes, every instance $I$ of $S$ and every bound $b$, $\Pi_F I$ is computed from the restriction $I'$ of $I$ to the kept part: at each table $d$ of $T$ with fibre $s_1 < \dots < s_k$, one row per tuple $(r_1, \dots, r_k)$ of rows of $I'$ at $s_1, \dots, s_k$ whose cells at the preimages of each column $\beta$ of $d$ are all equal, that common value being its cell at $\beta$ (a table outside the image has the one empty tuple); at each relation $\rho$ of $T$, the pair $(u, v)$ of rows over $\mathrm{src}(\rho)$ and $\mathrm{tgt}(\rho)$ whenever, for every kept relation $x$ with $F(x) = \rho$, the components of $u$ at $\mathrm{src}(x)$ and of $v$ at $\mathrm{tgt}(x)$ form a pair of $I'$ at $x$ (every pair when no relation is carried to $\rho$). A row of a table whose fibre is one table keeps the id of the row it projects to; every other row takes the next id above every id of $I$, the image tables first by the least table of their fibre, tuples ascending, then the tables outside the image by name. The provenance records each row with its table and tuple. On a homomorphism $h : I \to I'$ with both $\Pi_F I$ and $\Pi_F I'$ within $b$, $\Pi_F h$ sends the row of a tuple to the row of the tuple of its images. Pi creates no labelled null.

### Outside the domain

When $F$ is not surjective on attributes, Pi refuses with `AttributeOutsideImage` naming the least table and column outside the image; no right adjoint exists there, because a column outside the image would have to range over every value. A non-conforming $I$ is refused next, with `NonConforming`. Pi then checks every product against $b$ before it hands out any id: first, at each table of $T$ in ascending order whose fibre has two or more tables, the product of the numbers of rows of $I'$ at the fibre tables; then, at each relation of $T$ in ascending order to which no kept relation is carried, the product of the numbers of rows Pi admits at its source and target tables. The first product above $b$ is refused with `ProductSizeExceeded` naming the table (for a relation, its source table), the exact product and $b$. Only then are rows numbered, and Pi refuses with `RowIdsExhausted` when a fresh id would pass `u32::MAX`. On a map $h$ from $I$ to $I'$, $\Pi_F$ refuses as Pi refuses on $I$, then as it refuses on $I'$, and then with `NotANaturalMap` when $h$ is no homomorphism.

### Definitions used

Instance, conformance, functor, surjective on attributes, fibre and homomorphism, as defined at the top of this page.

### Proof

1. At a table $d$ the comma category $(d \downarrow F)$ of $\mathcal{E}$ has as objects the pairs $(c, f : d \to F(c))$. The only arrow of $\mathcal{E}(T)$ leaving a table is its identity, so $f = \mathrm{id}_d$ and $F(c) = d$, which makes $c$ a kept table of the fibre (a relation's image is a relation, never $d$). The objects are thus one $(s, \mathrm{id}_d)$ per fibre table $s$, and an arrow $(s, \mathrm{id}_d) \to (s', \mathrm{id}_d)$ is an arrow $u : s \to s'$ of $\mathcal{E}(S')$ with $F(u) = \mathrm{id}_d$, which only the identities are, so there is no arrow between two of them. A compatible family is then one row at each fibre table with no condition linking them; on the entity part it is a tuple of fibre rows. The attribute part adds that the value $\Pi$ assigns to a column $\beta$ of $d$ is the value of every preimage column, so a tuple is admitted exactly when those values agree, and that value is the cell. A table outside the image has an empty fibre, whose one family is the empty tuple; it has no columns because $F$ is surjective on attributes and every column of $d$ is an image only if some fibre table exists.
2. At a relation $\rho$ the objects of $(\rho \downarrow F)$ are $(x, \mathrm{id}_\rho)$ for each kept relation $x$ with $F(x) = \rho$, $(s, \mathrm{src}_\rho)$ for each table $s$ of the fibre of $\mathrm{src}(\rho)$ and $(s', \mathrm{tgt}_\rho)$ for each table $s'$ of the fibre of $\mathrm{tgt}(\rho)$, the last two kinds distinct even when $\rho$ is a loop; the only arrows besides identities are the legs of each $x$, from $(x, \mathrm{id}_\rho)$ to $(\mathrm{src}(x), \mathrm{src}_\rho)$ and to $(\mathrm{tgt}(x), \mathrm{tgt}_\rho)$. A family is thus a tuple $u$ over the source fibre, a tuple $v$ over the target fibre and, at each $x$, a pair of $I'$ whose legs are $u$ at $\mathrm{src}(x)$ and $v$ at $\mathrm{tgt}(x)$; this is the stated condition, and with nothing carried every pair of tuples is a family.
3. The result conforms to $T$: each row holds exactly the columns of its table (step 1 assigns each column one value), and each pair joins rows at the endpoints of $\rho$. Its cells are cells of $I'$, which conform, so no labelled null is nested and none is created.
4. Every product is checked against $b$ before any row is numbered, in the fixed order of the statement (a relation's product reads the numbers of rows Pi admits at its endpoint tables, which are counted first), so a refusal leaves no partial output, `ProductSizeExceeded` comes before any `RowIdsExhausted`, and below the bound the enumeration is complete. The ids are distinct by the argument of [Delta is precomposition](#L-kan-extensions-1), step 2.
5. For a homomorphism $h : I \to I'$ with both limits within $b$, the images of the components of an admitted tuple agree at every column under the null map of $h$, so they form an admitted tuple of $\Pi_F I'$, and image pairs satisfy the compatibility with carried relations because $h$ preserves pairs. The map keeps tables and is identity- and composite-preserving because it acts componentwise. $\Pi_F h$ is formed from $\Pi_F I$ and $\Pi_F I'$ before $h$ is checked, which is the stated refusal order on maps.

### Executable counterpart

- Law ID: `kan.pi`
- Reference: `panproto_reference::rel::pi`
- Reference: `panproto_reference::rel::pi_on_hom`
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

When $F$ is not surjective on attributes both refuse with `AttributeOutsideImage`, before either reads its input. The unit refuses with `ProductSizeExceeded` when $\Pi_F \Delta_F J$ exceeds $b$, the counit when $\Pi_F I$ does; each also refuses as Delta and Pi refuse on its inputs ([Delta is precomposition](#L-kan-extensions-1), [Pi is the limit of compatible families](#L-kan-extensions-3)).

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

For every functor $F : S' \to T$, every instance $I$ of $S'$ that Sigma accepts and every instance $J$ of $T$, the forward transpose $\Phi(h) = \Delta_F h \circ \eta_I$, which sends a row $r$ at $s$ to the copy $h(r)_s$, and the inverse transpose $\Phi^{-1}(k) = \varepsilon_J \circ \Sigma_F k$, which sends a row $r$ of $\Sigma_F I$ to the row of $J$ that the copy $k(r)$ copies, are mutually inverse bijections $\mathrm{Hom}(\Sigma_F I, J) \cong \mathrm{Hom}(I, \Delta_F J)$, natural in $I$ and $J$: for homomorphisms $a : I' \to I$, with Sigma accepting $I'$ too, and $c : J \to J'$, $\Phi(c \circ h \circ \Sigma_F a) = \Delta_F c \circ \Phi(h) \circ a$ and $\Phi^{-1}(\Delta_F c \circ k \circ a) = c \circ \Phi^{-1}(k) \circ \Sigma_F a$. The unit and counit are natural: $\Delta_F \Sigma_F a \circ \eta_{I'} = \eta_I \circ a$ and $c \circ \varepsilon_J = \varepsilon_{J'} \circ \Sigma_F \Delta_F c$.

### Outside the domain

The forward transpose $\Phi(h)$ refuses as Sigma refuses on $I$, then as Delta refuses on $\Sigma_F I$, then as Delta refuses on $J$, and then with `NotANaturalMap` when $h$ is no homomorphism $\Sigma_F I \to J$. The inverse transpose $\Phi^{-1}(k)$ refuses as Sigma refuses on $I$, then as Delta refuses on $J$, then as Sigma refuses on $\Delta_F J$, and then with `NotANaturalMap` when $k$ is no homomorphism $I \to \Delta_F J$. In both, `NotANaturalMap` names the least offending row of the supplied map's domain, as defined at the top of this page, and neither transpose returns a partial map. When Sigma clashes on $I$ there is no $\Sigma_F I$, so $\mathrm{Hom}(\Sigma_F I, J)$ is not defined and both transposes refuse with the clash, while $\mathrm{Hom}(I, \Delta_F J)$ is empty for every $J$ ([Sigma is left adjoint to Delta](#L-kan-extensions-4), step 4).

### Definitions used

Instance, homomorphism (with the fact that a homomorphism is determined by its row map), functor and hom profile, as defined at the top of this page; $\Sigma_F$ and $\Delta_F$ on homomorphisms, from [Sigma is the quotient of the disjoint union](#L-kan-extensions-2) and [Delta is precomposition](#L-kan-extensions-1); the unit and counit of [Sigma is left adjoint to Delta](#L-kan-extensions-4).

### Proof

1. $\Phi$ and $\Phi^{-1}$ land in the stated hom-sets: each is a composite of homomorphisms ([Delta is precomposition](#L-kan-extensions-1), step 4; [Sigma is the quotient of the disjoint union](#L-kan-extensions-2), step 5; [Sigma is left adjoint to Delta](#L-kan-extensions-4), steps 1 and 2).
2. Naturality of $\eta$. Both $\Delta_F \Sigma_F a \circ \eta_{I'}$ and $\eta_I \circ a$ are homomorphisms $I' \to \Delta_F \Sigma_F I$, so it suffices that their row maps agree. A row $r$ of $I'$ at $s$ goes under the first to $r_s$ and then, as $\Sigma_F a$ sends each row to $a$ of it and $\Delta_F$ sends a copy $x_s$ to $(\Sigma_F a)(x)_s$, to $a(r)_s$; under the second it goes to $a(r)$, which lies at $s$ because $a$ keeps tables, and then to $a(r)_s$.
3. Naturality of $\varepsilon$. Both $c \circ \varepsilon_J$ and $\varepsilon_{J'} \circ \Sigma_F \Delta_F c$ are homomorphisms $\Sigma_F \Delta_F J \to J'$. A row of $\Sigma_F \Delta_F J$ is a copy $r_s$ under its own id; the first sends it to $r$ and then to $c(r)$, the second to $(\Delta_F c)(r_s) = c(r)_s$ and then to the row it copies, $c(r)$. The row maps agree, so the homomorphisms do.
4. $\Phi^{-1} \circ \Phi = \mathrm{id}$. For $h : \Sigma_F I \to J$,

   $$\Phi^{-1}(\Phi(h)) = \varepsilon_J \circ \Sigma_F \Delta_F h \circ \Sigma_F \eta_I = h \circ \varepsilon_{\Sigma_F I} \circ \Sigma_F \eta_I = h,$$

   by functoriality of $\Sigma_F$, naturality of $\varepsilon$ at $h$ (step 3) and the second triangle identity.
5. $\Phi \circ \Phi^{-1} = \mathrm{id}$. For $k : I \to \Delta_F J$,

   $$\Phi(\Phi^{-1}(k)) = \Delta_F \varepsilon_J \circ \Delta_F \Sigma_F k \circ \eta_I = \Delta_F \varepsilon_J \circ \eta_{\Delta_F J} \circ k = k,$$

   by functoriality of $\Delta_F$, naturality of $\eta$ at $k$ (step 2) and the first triangle identity. Thus both transposes are bijections.
6. Naturality of the transposes. By functoriality of $\Delta_F$ and naturality of $\eta$ at $a$,

   $$\Phi(c \circ h \circ \Sigma_F a) = \Delta_F c \circ \Delta_F h \circ \Delta_F \Sigma_F a \circ \eta_{I'} = \Delta_F c \circ \Delta_F h \circ \eta_I \circ a = \Delta_F c \circ \Phi(h) \circ a.$$

   Dually, by functoriality of $\Sigma_F$ and naturality of $\varepsilon$ at $c$,

   $$\Phi^{-1}(\Delta_F c \circ k \circ a) = \varepsilon_{J'} \circ \Sigma_F \Delta_F c \circ \Sigma_F k \circ \Sigma_F a = c \circ \varepsilon_J \circ \Sigma_F k \circ \Sigma_F a = c \circ \Phi^{-1}(k) \circ \Sigma_F a.$$

7. The refusal order is the order in which the transposes take their ingredients: the forward transpose forms $\Sigma_F I$, the unit's $\Delta_F \Sigma_F I$ and $\Delta_F J$ before it checks the supplied map, and the inverse transpose forms $\Sigma_F I$, $\Delta_F J$ and the counit's $\Sigma_F \Delta_F J$ before it checks the supplied map. The tests `transposes_and_the_pi_unit_refuse_in_the_stated_order`, `sigma_reports_a_clash_whatever_null_ids_remain` and `maps_and_their_refusals_agree_with_the_reference` execute this order, the last on every map, homomorphism or not, between the objects of the bounded family with at most one row per table and two in all, with two refusing objects per side.
8. The hom-profile quotient is exact. Let $\alpha : I \to \bar{I}$ and $\gamma : J \to \bar{J}$ be the profile transports onto the representatives (row permutations within tables, with a permutation of the case's labelled nulls).

   (i) Each labelled object has exactly one profile, because the representative is the image of least canonical bytes over a finite group, and its class size is the index of its stabilizer.

   (ii) Conjugation $h \mapsto \gamma \circ h \circ \Sigma_F \alpha^{-1}$ is a bijection $\mathrm{Hom}(\Sigma_F I, J) \to \mathrm{Hom}(\Sigma_F \bar{I}, \bar{J})$ with inverse the conjugation by $\alpha$ and $\gamma^{-1}$, and $k \mapsto \Delta_F \gamma \circ k \circ \alpha^{-1}$ is a bijection of the other hom-set. The transport $\Sigma_F \alpha$ sends rows by $\alpha$, merged nulls by their representatives, and fresh nulls by their cells, for Sigma's equations, classes and fresh numbering are carried by $\alpha$ slot for slot.

   (iii) By step 6, with $a$ and $c$ the transports (which are isomorphisms), these bijections commute with $\Phi$ and $\Phi^{-1}$.

   (iv) They carry identities, composites and naturality squares to identities, composites and naturality squares, because conjugation is a functor.

   (v) Production computes the reference's transposes on every labelled member, not only on the representatives. The test `every_labelled_object_has_the_reference_units_and_counits` runs production's units and counits of both adjunctions (Pi at both bounds), with the Sigma, Delta and Pi results and provenance they are built from, on all 248,552 labelled objects and requires the reference's results exactly, each refusal by its typed reason; the transposes, their composites and naturality are executed on the representative pairs only. A transpose of a member pair is fixed by that object-level data and the supplied map. Production computes $\Phi(h)$ as the unit followed by $\Delta_F h$, whose row map sends $r_s$ to the copy of $h(r)$ at $s$ that the Delta provenance names and whose null map is $h$'s on the nulls of $\Delta_F \Sigma_F I$; it computes $\Phi^{-1}(k)$ as $\Sigma_F k$ followed by the counit, where $\Sigma_F k$ sends each row to $k$ of it, each fresh null to the cell its provenance names, and each other null to the representative of $k$'s image of it. Each formula reads rows and nulls only through the object-level provenance and the supplied map, so it commutes with renaming rows within tables and labelled nulls. On a member, production's transposes therefore equal the reference's, whose transports onto the representative pair are the executed transposes by (ii) and (iii).

   The comparison between two members of one profile excludes literal equality for exactly two first-witness payloads, which the algorithms stage in numeric traversal order, and compares complete structural feature sets instead. Let $\tau$ be the bijection the row transport induces on semantic keys: $\mathrm{Row}(r)$ to $\mathrm{Row}(\alpha(r))$, $\mathrm{Cell}(r, c)$ to $\mathrm{Cell}(\alpha(r), c)$, $\mathrm{Attribute}(r, c)$ to $\mathrm{Attribute}(\alpha(r), c)$ and $\mathrm{Relation}(e, r, r')$ to $\mathrm{Relation}(e, \alpha(r), \alpha(r'))$; numeric row, node and edge ids are no semantic keys. First, for `SigmaConstantClash`, each member independently builds the incidence graph whose vertices are the output slots $(\mathrm{Row}(r), \beta)$, the used labelled nulls and the canonical bytes of the constants, with an edge from a slot to each source-cell value mapped into it; its clash-feature set holds $(\mathrm{Row}(r), \beta, \{c, c'\})$ for every slot and every unordered pair of distinct constants in that slot's component. Transporting the complete set of one member through $\tau$ must give the complete set of the other; each returned `SigmaConstantClash` with its constant pair taken unordered must be a member of its own side's set; and both must return exactly the `SigmaConstantClash` variant. Second, for the input graph-to-tree conversion of the bounded leaf graphs, each member independently computes the complete set of semantic node keys with at least two distinct parents and, when there is one root and no such node, the complete set of semantic node keys with no parent chain reaching that root; $\tau$ must carry each complete set to the other member's; a returned `SecondParent` or `Unreached` must be of the same family on both members, with the semantic key of its node in its own side's set; every `RootCount` count must be equal; and the nine other input `NotATree` families (`MissingRoot`, `DanglingEdge`, `PositionedEdge`, `ReservedArc`, `ArcIntoFan`, `RootHasParent`, `FanLabel`, `FanData` and `Fan`) must not occur. Every other payload is compared exactly between members, and on every labelled object the reference and production payloads, these two included, are compared exactly. The member comparisons are executed by the test `o14_every_labelled_member_has_checked_operation_transports` of the reference crate.

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

For every functor $F : S' \to T$ surjective on attributes, every bound $b$, every instance $J$ of $T$ and every instance $I$ of $S'$ with $\Pi_F I$ within $b$, the forward transpose $\Psi(g)$, which sends a row $r$ at $d$ to the row of $\Pi_F I$ whose tuple holds $g(r_s)$ at each table $s$ of the fibre of $d$, and the inverse transpose $\Psi^{-1}(k) = \kappa_I \circ \Delta_F k$, which sends a copy $r_s$ to the component at $s$ of the tuple of $k(r)$, are mutually inverse bijections $\mathrm{Hom}(\Delta_F J, I) \cong \mathrm{Hom}(J, \Pi_F I)$. They are natural in $J$: for a homomorphism $a : J' \to J$, $\Psi(g \circ \Delta_F a) = \Psi(g) \circ a$. They are natural in $I$ wherever the second limit is within $b$ too: for a homomorphism $c : I \to I'$ with $\Pi_F I'$ within $b$, $\Psi(c \circ g) = \Pi_F c \circ \Psi(g)$. Wherever $\Pi_F \Delta_F J$ is within $b$ too, $\Psi(g) = \Pi_F g \circ \mu_J$.

### Outside the domain

The forward transpose $\Psi(g)$ checks the supplied map before it forms Pi: it refuses as Delta refuses on $J$, then with `NotANaturalMap` when $g$ is no homomorphism $\Delta_F J \to I$, and only then as Pi refuses on $I$ (`AttributeOutsideImage` when $F$ is not surjective on attributes, `NonConforming`, `ProductSizeExceeded` when $\Pi_F I$ exceeds $b$, `RowIdsExhausted`, in the order of [Pi is the limit of compatible families](#L-kan-extensions-3)). The inverse transpose $\Psi^{-1}(k)$ refuses as Pi refuses on $I$, then as Delta refuses on $J$, then as Delta refuses on $\Pi_F I$, and then with `NotANaturalMap` when $k$ is no homomorphism $J \to \Pi_F I$. `NotANaturalMap` names the row defined at the top of this page. The transposes need $\Pi_F I$ only: they are defined when $\Pi_F \Delta_F J$ exceeds $b$, where only the unit and its component formula are undefined.

### Definitions used

Instance, homomorphism (with the fact that a homomorphism is determined by its row map), functor, surjective on attributes, fibre and hom profile, as defined at the top of this page; $\Delta_F$ and $\Pi_F$ on homomorphisms, from [Delta is precomposition](#L-kan-extensions-1) and [Pi is the limit of compatible families](#L-kan-extensions-3); the unit and counit of [Delta is left adjoint to Pi](#L-kan-extensions-5).

### Proof

1. $\Psi(g)$ is a homomorphism. The tuple $(g(r_s))_s$ is admitted: $g$ sends the copy $r_s$, whose cell at $\alpha$ is $J(r)(F(s,\alpha))$, to a row with an equal cell, so at every preimage of a column $\beta$ of $d$ the images hold $J(r)(\beta)$ under the null map of $g$, and the admitted row holds it at $\beta$. Pairs go to compatible pairs because $g$ preserves the copied pairs. $\Psi^{-1}(k)$ is a composite of homomorphisms.
2. Mutual inverses. $\Psi^{-1}(\Psi(g))$ sends $r_s$ to the component at $s$ of the tuple $(g(r_{s'}))_{s'}$, which is $g(r_s)$. $\Psi(\Psi^{-1}(k))$ sends $r$ to the row whose tuple holds, at each $s$, the component at $s$ of the tuple of $k(r)$; that tuple is the tuple of $k(r)$, and a row of $\Pi_F I$ is determined by its table and tuple, so the row is $k(r)$. The null maps agree because each transpose sends a null to where the given map sends it.
3. The component formula. When the unit exists, $\Pi_F g \circ \mu_J$ sends $r$ to the row of the tuple $(g(r_s))_s$, which is $\Psi(g)(r)$.
4. Naturality, from the tuple formula alone, so that only the limits named in the statement are needed. Each side of each equation is a homomorphism, so it suffices that the row maps agree. For $a : J' \to J$, $\Psi(g \circ \Delta_F a)$ sends a row $r$ of $J'$ at $d$ to the row of the tuple $(g((\Delta_F a)(r_s)))_s = (g(a(r)_s))_s$, which is $\Psi(g)(a(r))$, as $a(r)$ lies at $d$. For $c : I \to I'$, $\Psi(c \circ g)$ sends $r$ to the row of the tuple $(c(g(r_s)))_s$, and $\Pi_F c$ sends the row of the tuple $(g(r_s))_s$, which is $\Psi(g)(r)$, to the row of the tuple of its components' images, the same row.
5. The refusal order is the order in which the transposes take their ingredients: the forward transpose forms $\Delta_F J$ and checks the supplied map against it and $I$ before it forms $\Pi_F I$, and the inverse transpose forms $\Pi_F I$, $\Delta_F J$ and the counit's $\Delta_F \Pi_F I$ before it checks the supplied map. The tests `pi_transpose_checks_the_map_before_the_product_bound`, `transposes_and_the_pi_unit_refuse_in_the_stated_order` and `maps_and_their_refusals_agree_with_the_reference` execute this order.
6. The hom-profile quotient is exact by the argument of [The Sigma and Delta hom-sets are in bijection](#L-kan-extensions-6), step 8, clauses (i) to (v), with the transports of $\Delta_F$ (a copy $(s, r)$ to $(s, \alpha(r))$) and of $\Pi_F$ (a tuple to the tuple of its transported components) in place of Sigma's, and step 4 in place of that block's step 6. In clause (v), production computes $\Psi(g)$ by the tuple formula of the statement, reading the copies from the Delta provenance and the rows from the Pi provenance, with $g$'s null map on the nulls of $J$, and $\Psi^{-1}(k)$ as $\Delta_F k$ followed by the counit; both read rows and nulls only through the object-level provenance and the supplied map. Delta and Pi have no first-witness refusal on the bounded family, so every transported payload is compared exactly.

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

For a classified schema $X$, a graph is **encodable** over $X$ when it passes five checks in order. The first failure is `NotEncodable` with the least offender of the first failing pass, each pass checking its offender's conditions in the order listed. (1) Nodes ascending: a node of the fan shape or with an out-edge of a fan kind is `FanNode`, and a node anchored at neither a table nor a value vertex of $X$ is `AnchorOutsideSide`. (2) Edges ascending: an edge whose source or target is no node is `DanglingEdge` (a refusal of the reference alone, since a production graph rejects such an edge when it is built), one whose label is no relation, attribute arrow or inner edge of $X$ is `EdgeOutsideSide`, and one whose endpoints are not anchored at its label's ends is `IncoherentEndpoints`. (3) Two or more edges sharing label and endpoints are `ParallelEdges`, naming the group, ascending, whose least edge is least. (4) Table nodes ascending and, within one, its columns ascending: a column along which the node has no edge is `CellAsExtraField` when the node holds a field of that name and `MissingCell` otherwise, and one along which it has several edges is `RepeatedCell` with those edges. (5) Value nodes ascending: a node with several in-edges is `SharedValue`; one from which following the least in-edge reaches no table node is `UnrootedValue`; a leaf holding neither a present nor a null value is `MissingValue`; a node with out-edges that holds a value, or whose out-edges form neither a record (no positions, distinct column names) nor a list (all positioned, distinct positions), is `InnerValueShape`; and a node holding a labelled null below its top level, or a labelled null reached along an inner edge, is `NestedLabeledNull`.

On encodable graphs the encoding $E_X$ is a table and a layout, and $D_X(E_X G) = G$; for every conforming table $T$ and every layout $L$ on which the decoding succeeds (it refuses when ids or positions run out), the table of $E_X(D_X(T, L))$ is $T$. A homomorphism of encodable graphs is a map of table nodes whose encodings form a table homomorphism; $E$ is a bijection on these hom-sets that keeps identities and composites.

For a functor $F$, graph Delta, Sigma and Pi are $D \circ X_F \circ E$ for the table operation $X_F$ of [Delta is precomposition](#L-kan-extensions-1), [Sigma is the quotient of the disjoint union](#L-kan-extensions-2) and [Pi is the limit of compatible families](#L-kan-extensions-3). Sigma and Pi encode over the whole source and restrict to the kept part, written $G|_{S'}$, which keeps the nodes at kept tables and value vertices and the edges at kept arrows between them, so $E(G|_{S'})$ is the restriction of $E(G)$. The output layout is chosen from the table provenance. A row node keeps its decorations when it has one source row. The contributing input cells of an output cell are the cell Delta copies, the cells of Pi's tuple at the preimages of its column, or, for Sigma, the input cells of the same row at the kept columns the functor sends to its column. Each output cell takes the subtree of the least contributing input cell in `(table, row, column)` order that, once moved onto the output cell's own arrow, hangs from that arrow's value vertex and still folds to the output cell, else a record-free leaf; for Sigma and Pi a subtree moves forward along the functor (each inner edge to its remap, else to the inner edge of its kind and name from its moved source), and for Delta it moves back onto the source column of the copy (each inner edge to its one preimage inner edge, the move failing when there is none or several). The contributing input pairs of an output pair are, for Sigma, the input pairs $(e, a, b)$ with $F(e)$ its relation; for Delta, the pair the copied pair copies; and for Pi, the input pairs $(x, u_{\mathrm{src}(x)}, v_{\mathrm{tgt}(x)})$ of the kept relations $x$ carried to its relation, $u$ and $v$ its rows' tuples. An output relation edge takes the least recorded position of its contributing pairs' edges, and it takes the recorded id of its contributing pair's edge only when it has exactly one contributing pair and the kept relation $e$ involved (the output's own relation for Delta, the contributing pair's for Sigma and Pi) is the only kept relation sent to $F(e)$ and both endpoint tables of $F(e)$ have one-table fibres. Selected ids are kept at their first occurrence when unclaimed, and every other node and edge is fresh above every input and layout id. An edge with no recorded source (the attribute edge of a fresh leaf, or a relation edge with no contributing pair) is created: at a node with an out-edge that carries a recorded position, the created out-edges take the next positions above the largest such, in ascending edge id, and elsewhere they take none. Their units, counits and transposes are those of the table adjunctions read through $E$, and both graph adjunctions hold on the encodable graphs over the kept part and over the target.

Graph provenance names, for every output node and edge, every input node and edge that contributes to it: a row its source rows, a cell's nodes and edges every node and edge of its contributing input cells as defined above, and a relation edge the input edges of its contributing pairs.

### Outside the domain

A schema-taking form refuses with `RelSchema` when either schema has no classification (the source first), then with `Functor` when the migration states no functor; every form then refuses with `NotEncodable` for an input outside the encoding domain, with `Adjunction` carrying the table operation's refusal, and with `NodeIdsExhausted`, `EdgeIdsExhausted` or `EdgePositionsExhausted` when decoding has no id or position left. The checks run in that order and no partial graph or provenance is returned. A graph homomorphism check refuses with the domain's encoding refusal, then the codomain's, then the table homomorphism refusal.

### Definitions used

Classified schema, instance, functor, kept part, graph form, encoding and decoding, as defined at the top of this page; Delta, Sigma, Pi and their adjunctions, from the blocks the statement links.

### Proof

1. $D_X(E_X G) = G$. The layout of $E_X G$ records every node's id and decorations, every cell's subtree with its ids and positions, and every relation edge's id and position. $D_X$ lays one table node per row with the recorded decorations, the recorded subtree for every cell (it folds to the cell, because the cell was read from it), and one edge per pair with the recorded id; no recorded id collides, because they are the ids of one graph. So every node and edge of $G$ is rebuilt with its id, data and position.
2. The table of $E_X(D_X(T, L))$ is $T$ wherever $D_X(T, L)$ succeeds. $D_X$ lays a table node per row with the row's id, a subtree per cell folding to the cell (the selected subtree only when it folds to the cell by canonical bytes, else a leaf holding the cell), and one edge per pair; the result passes the five checks because each cell has one rooted, unshared subtree and the table conforms. Reading it back gives the rows, the cells (the folds) and the pairs of $T$.
3. Full faithfulness. A map of table nodes is a graph homomorphism exactly when its encoding is a table homomorphism, by definition; distinct node maps encode to distinct row maps; so $E$ is a bijection on hom-sets, and it keeps identities and composites because composition is composition of the node and null maps.
4. The restriction. $G|_{S'}$ keeps exactly the rows at kept tables, their cells at kept columns with their subtrees, and the pairs at kept relations, which is the restriction of $E(G)$ to the kept part; Sigma and Pi read only the kept part, so encoding the whole source and restricting gives the same table as encoding $G|_{S'}$ over the kept part.
5. The adjunctions transport. For $G$ encodable over the kept part and $H$ over the target, $\mathrm{Hom}(D \Sigma E G, H) \cong \mathrm{Hom}(\Sigma E G, E H) \cong \mathrm{Hom}(E G, \Delta E H) \cong \mathrm{Hom}(G, D \Delta E H)$: the outer bijections are step 3 together with steps 1 and 2 (the table of $E D$ is the table), the middle one is [The Sigma and Delta hom-sets are in bijection](#L-kan-extensions-6). Each step is natural because $E$ keeps composites, so the composite is natural and its value at an identity is the transported unit. Delta and Pi transport the same way with [The Delta and Pi hom-sets are in bijection](#L-kan-extensions-7).
6. Layout, ids and provenance. The layout rules choose a representative of the output's isomorphism class, which the laws do not observe; they are deterministic because every choice is a least element, a least position or a unique preimage in a fixed order, and the ids are distinct because kept ids are admitted once and outside the row ids and fresh ids lie above every input and layout id. A relation edge's recorded id is subject to the same first-occurrence rule as every other selected id, and created positions lie above every carried position at their node, so they collide with none. The provenance attributes to each row node its source rows, to each node and edge of a cell every node and edge of that cell's contributing input cells, read on the nodes and edges actually decoded for the cell, and to each relation edge the input edges of its contributing pairs.
7. Execution on the graph scope. The test `encoding_accepts_exactly_the_five_pass_domain` and the tests `o7_encoding_agrees_with_the_reference` and `o7_encoding_refusals_agree_with_the_reference` execute the five passes, their refusals and their payload order; `o7_decoding_agrees_with_the_reference_on_every_table_and_layout` executes steps 1 and 2, record and list subtrees included; `o7_graph_kan_homs_agree_with_the_reference` executes step 3 and the order of the graph homomorphism refusals; and `o7_graph_adjunctions_agree_with_the_reference` executes step 5 on the graph adjunctions.

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

For every functor $F$ and every conforming instance $T$ whose record-free decoding $D(T)$ succeeds, and for $X$ each of Delta, Sigma and Pi: when $X_F$ refuses on $T$, the graph route $g_X(D(T))$ refuses with the same refusal; when $X_F$ accepts $T$ and the graph route's decoding of $X_F(T)$ succeeds, the table of $E(g_X(D(T)))$ is $X_F(T)$, so the two routes agree on every observable of the table.

### Outside the domain

A refusal of the table operation is the graph route's `Adjunction` refusal with the same payload; a table that does not conform is refused by both with the same `NonConforming` reason. The graph route alone refuses with `NodeIdsExhausted`, `EdgeIdsExhausted` or `EdgePositionsExhausted` when the decoding of $X_F(T)$ has no id or position left, which the table operation, holding no value nodes or edges, never meets: along the merge of $a\{x\}$ and $b\{x\}$ into $m\{x\}$, one row of id `u32::MAX - 2` decodes and table Delta accepts it with copies at `u32::MAX - 2` and `u32::MAX - 1`, while graph Delta needs two more nodes for their leaves and refuses with `NodeIdsExhausted`.

### Definitions used

Instance, conformance, encoding and decoding, as defined at the top of this page; the graph Kan extensions of [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).

### Proof

1. $g_X(D T) = D(X_F(E(D T)), L)$ for the layout $L$ the graph operation selects, by the definition in [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).
2. The table of $E(D T)$ is $T$ by step 2 of that block, so $g_X(D T) = D(X_F T, L)$.
3. Suppose $X_F$ accepts $T$ and $D(X_F T, L)$ succeeds. $X_F T$ conforms ([Delta is precomposition](#L-kan-extensions-1), step 1; [Sigma is the quotient of the disjoint union](#L-kan-extensions-2), step 4; [Pi is the limit of compatible families](#L-kan-extensions-3), step 3), and step 2 of that block applies to every conforming table and layout on which the decoding succeeds, which is the case here; so the table of $E(D(X_F T, L))$ is $X_F T$.
4. When $X_F$ refuses on $T$, the graph route reaches the table operation (the record-free decoding of a conforming table is encodable, step 2) and returns its refusal wrapped in `Adjunction`, unchanged, before it decodes anything; so a refusal of the table operation is never masked by an exhausted decoding.
5. When $X_F$ accepts $T$, the graph route's only further step is the decoding of $X_F T$, so its only refusals beyond those of step 4 are the decoding's `NodeIdsExhausted`, `EdgeIdsExhausted` and `EdgePositionsExhausted`. The decoding allocates value nodes and edges that the table has no counterpart of, so it can exhaust the ids where the table operation does not, as the instance under Outside the domain shows; the test `the_graph_route_alone_refuses_when_decoding_runs_out_of_ids` executes it on both sides.

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

For every functor $F$ and every fan-free tree $X$, tree Sigma and tree Pi of $X$ are graph Sigma and graph Pi (restricted forms) of $X$'s graph form, read back as a tree, and tree Delta is graph Delta read back the same way. The **read-back** of a graph checks, in this order and each time at its least offender: a fan node a tree cannot carry; an edge carrying a position; a node with incoming edges from two different nodes, or two edges from one node to one node; a cycle; and, among several roots, a fan spanning two of their trees. A graph that passes them is a tree when exactly one of its nodes, fan nodes set aside, has no incoming edge, and is read from that root; with no such node (the empty graph) or several, it has no tree form.

The **tree-Kan domain** holds the functors that are injective on tables, relations and columns, on the value vertices of the kept part (each sent to its vertex remap, else to itself) and on its inner edges, and whose image contains every target table, relation, column, value vertex and inner edge. The production tree functions relabel without building anything: tree Sigma (the extension) sends each node's anchor to its image and each arc's edge to its image; tree Pi, defined only for a vertex-injective map, does the same; tree Delta (the pullback) sends each node's anchor and each arc's edge back to its unique preimage. Each keeps every node id, the order of the arcs, the fans and the root. On the tree-Kan domain the graph form of each production tree function is identical to the graph route's output.

### Outside the domain

Tree Delta refuses a functor that sends two source vertices (kept tables, or the value vertices of kept columns, each sent to the value vertex of its image column) to one target with `NonInjectiveVertexMap`, naming the first collision met scanning the source vertices ascending: the least source vertex that shares its image with a lesser one, the least such lesser one, and their image, so that colliding pairs are ordered by their second source, then their first. It then refuses one that sends two source edges (kept relations, or the attribute arrows of kept columns) to one with `NonInjectiveEdgeMap`, naming the image (production names its endpoints). Tree Pi refuses a functor that sends two of the kept tables and the value vertices of the kept part (each value vertex sent to its vertex remap, else to itself) to one target with `NonInjectiveVertexMap`, naming the first collision in the same order; the production refusal names the image and its whole fibre, ascending, whose two least members are that pair. Tree Sigma refuses no merging map. A graph refusal is returned unchanged. A graph-route output that fails a read-back check is refused with `NotATree`, and one that passes them with other than exactly one root is refused with `RootCount` and the number of roots.

### Definitions used

Functor, kept part, graph form and tree form, as defined at the top of this page; the read-back, the tree-Kan domain and the production tree functions, as defined in the statement; the graph Kan extensions and their layout rules, from [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8).

### Proof

1. The tree routes are defined as composites of the tree's graph form, a graph Kan extension and the read-back; the merging-map refusals come first because a pullback or product along a merging vertex map duplicates a node into several anchors, which no tree holds. Scanning the source vertices ascending meets a collision at its second source, and the first lesser source with that image is the least one, which fixes the named pair.
2. The read-back returns the tree form of the output exactly when every check passes and one root remains; each refusal names the first failing check or counts the roots, so it is a function of the output. The test `bounded_family_kan_objects_agree_with_the_reference` executes both read-backs on every input conversion and every route output of the bounded family. Its independent structural oracle computes the root count before attempting a rooted read-back and requires the production graph to have that exact count. With one root, the bounded leaf graphs can fail only because a semantic node has at least two distinct parents or because it has no parent chain to the root. The oracle computes the complete node-key set for each family; production must refuse in the corresponding `MultipleParents` or `Cycle` family, and the reference's returned `SecondParent` or `Unreached` node must belong to the complete set. The nine other reference `NotATree` families cannot occur in this scope and are rejected rather than grouped into a broad outcome bucket. Thus the comparison fixes both the semantic family and every available payload, while allowing the two implementations to name the same family in their respective public vocabularies.
3. On the tree-Kan domain the table functors are reindexings along an isomorphism of entity categories. Sigma's equations have one preimage column per column, so no cell is equated with another and no fresh null arises, and each target table and relation has one source, so rows keep their ids and pairs are relabelled one for one; Pi's fibres have one table each, so every tuple is one row keeping its id, and no table lies outside the image. By the layout rules of [Graph Kan extensions are conjugates of the table ones](#L-kan-extensions-8), every row node keeps its decorations (one source row); every cell has one contributing input cell, whose subtree moves along the bijective maps of value vertices and inner edges and still folds to the same value, so it is selected with its ids; and every relation edge has exactly one contributing pair at a relation that is the only one sent to its image, between tables with one-table fibres, so it keeps its id and its position. Nothing is created and no id is fresh, so the decoded graph is the input graph form with every anchor and label relabelled along the isomorphism and every id and position kept. The production tree functions, by their definitions in the statement, perform the same relabelling and keep every id, so the graph form of each is identical to the graph route's output. The test `bounded_family_kan_objects_agree_with_the_reference` executes this square on every source tree (Sigma, and Pi at both bounds) and every target tree (Delta) of the bounded family whose functor is in the tree-Kan domain; the bounded family's schemas give each column its own value vertex and have no inner edges, so injectivity and surjectivity on tables, relations and columns decide the domain there.

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
- File: `crates/panproto-inst/src/ginstance/tree_shape.rs`
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

For classified schemas $S$ and $T$ and a migration, the entity functor keeps and maps, in this order: (1) each source table that the vertex remap maps, or that survives and is no remap image, to its remap image or itself, which must be a table of $T$; (2) each relation between kept tables that the edge remap maps, or that survives and is no remap image, to its remap image or to the edge of its kind and name between the endpoint images, which must be a relation of $T$ between those images; (3) each attribute arrow of a kept table that the edge remap maps, to a target attribute arrow from the image table (ending at the value vertex's remap when it has one), and each surviving one that is no image to the target arrow from the image table with its kind and name ending at its value vertex's image, an unmentioned column being dropped; (4a) every edge remap is checked not to cross between relations and attribute arrows, and the remaps of inner edges and value vertices are recorded; and, for the Kan extensions, (4b) each inner-edge remap must keep the value structure: its image is an inner edge of $T$ with the edge's kind and name, and the schema kind of each endpoint of the image is the schema kind of the remap image of the corresponding endpoint, whether that remap image is a value vertex or a table (an endpoint without a remap keeping its own kind).

### Outside the domain

Each step reports its least offender: `VertexImageNotATable` with the table and its image (1); `IncoherentEdgeImage` with the relation and its image (2, and 4a for a relation sent to an attribute arrow); `IncoherentAttributeImage` with the arrow and its image, or none for a surviving arrow without one (3, and 4a for an attribute arrow sent to a relation), where in step 3 the least offender is the least attribute arrow of every kept table in edge order (source, target, kind, name), not the least column name; `ValueStructureRemap` with the inner edge (4b). A schema without a classification is refused by [Classification](#L-kan-extensions-12) first.

### Definitions used

Classified schema, kept part and functor, as defined at the top of this page. A **migration** from $S$ to $T$ is read here as four finite data, and the entity functor reads nothing else: a **vertex remap** sending some source vertices to target vertices, an **edge remap** sending some source edges to target edges, a set of **surviving vertices** and a set of **surviving edges**. A vertex or edge *survives* when it lies in the surviving set, it *is a remap image* when some entry of the remap sends something to it, and its *remap image* is the target of its own remap entry, else itself. The value-vertex and inner-edge remaps of step (4a) are the entries of the two remaps whose sources are value vertices and inner edges of $S$.

### Proof

1. The steps produce a functor $F : S' \to T$: step 1 gives a map of kept tables into tables of $T$; step 2 keeps a relation only when its image runs between the images of its endpoints, so the legs of each kept relation go to the legs of its image and $\mathcal{E}(S') \to \mathcal{E}(T)$ preserves domains, codomains and the trivial composites; step 3 gives each kept column a column of its table's image.
2. Each refusal names a datum that would break this: an image that is no table leaves the object map undefined; an image relation that does not join the endpoint images breaks a leg; an attribute image that is no column of the image table leaves the cell map undefined; a remap across the divide would send an entity object to a value or the reverse.
3. Step 4b accepts exactly the remaps that rename vertices inside values without changing a value's shape: a value records its fields by edge name and kind and its leaves by kind, so a remap that keeps names, kinds and endpoint kinds sends each value to a value of the same form. The kinds compared are schema kinds, read the same way on both sides of the comparison, so a remap image that is a table is compared by its own kind like any other vertex. The test `entity_functor_orders_attribute_arrows_and_compares_schema_kinds` executes this case and the edge order of step 3, which the entity scope cannot separate from column-name order.
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

The first failing check is the refusal: `AmbiguousKind` for the least kind in both $V$ and $N$; `UnclassifiedKind` for the least undeclared vertex whose kind is in neither; `ValueReachesTable` for the least edge from a value vertex to a table; `DuplicateColumn` for the least pair $(t, c)$, in lexicographic order, such that two attribute arrows of the table $t$ have the column name $c$.

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
