# Glossary

A glossary of defined terms used in more than one chapter, with back-links to the chapter that introduces each.

**arrow.** Synonym for *morphism*. Used more often when drawing diagrams. [Categories](../foundations/categories.md).

**asymmetric lens.** A lens whose source is larger than its view, with a `put` function that takes the old source as an argument to preserve data outside the view. [Bidirectional lenses](../core/lenses.md).

**auto-migration.** A migration inferred automatically by panproto-vcs from the diff between two schema versions, without an explicit user-written declaration. [Data versioning](../vcs/data-versioning.md).

**BibTeX.** The citation-entry format used by `references.bib`. [Notation reference](./notation-table.md).

**blob.** A content-addressed byte sequence. Used in both git's and panproto-vcs's object models. [What git already versions and what it does not](../vcs/git-background.md).

**category.** The mathematical structure at the centre of the book: a class of objects, hom-sets of morphisms between them, a composition operation, and identity morphisms, subject to associativity and identity laws. [Categories](../foundations/categories.md).

**cocone.** A natural transformation from a diagram $D : J \to \mathcal{C}$ to a constant functor. The universal cocone under $D$ is the colimit of $D$. [Colimits and pushouts](../foundations/colimits.md).

**colimit.** A universal cocone under a diagram. Coproducts, pushouts, and coequalizers are specific cases. [Colimits and pushouts](../foundations/colimits.md).

**complement.** For a lens, the part of the source that `put` preserves while modifying the view. Made explicit in Cambria-style lenses. [Bidirectional lenses](../core/lenses.md).

**composition.** The primitive operation of a category: given two morphisms whose ends meet, a third morphism from the start of the first to the end of the second. [Categories](../foundations/categories.md).

**contextual category.** The categorical structure of a generalised algebraic theory, equipped with the machinery needed to interpret dependent sorts. [Algebraic and generalised algebraic theories](../foundations/gats.md).

**coproduct.** The universal cocone under a two-object discrete diagram. In $\mathbf{Set}$ it is the disjoint union. [Universal properties](../foundations/universal-properties.md).

**diagram.** A functor from a small shape category into a target category. [Colimits and pushouts](../foundations/colimits.md).

**functor.** A structure-preserving mapping between categories, sending objects to objects and morphisms to morphisms while respecting composition and identity. [Functors and natural transformations](../foundations/functors.md).

**GAT.** Generalised algebraic theory, in the sense of @cartmell1986generalised. The expressive theory formalism that panproto uses to specify protocols. [Algebraic and generalised algebraic theories](../foundations/gats.md).

**Hask.** The category of Haskell types and functions between them, under the idealisation that every function is total. [Categories](../foundations/categories.md).

**hom-set.** The set of morphisms between two fixed objects of a category, written $\mathcal{C}(A, B)$ or $\mathrm{Hom}_\mathcal{C}(A, B)$. [Categories](../foundations/categories.md).

**identity.** The morphism $\mathrm{id}_A : A \to A$ required to exist for every object of a category and to act as a two-sided unit for composition. Also the identity function in a programming language, the `cat` command with no arguments in Unix, and the identity migration in panproto. [Categories](../foundations/categories.md).

**instance.** A record set under a panproto schema, representing the data that lives under the schema. [Protocols as theories, schemas as instances](../core/schemas-as-instances.md).

**isomorphism.** A morphism $f : A \to B$ with a two-sided inverse $g : B \to A$. Objects with an isomorphism between them are isomorphic, written $A \cong B$. [Categories](../foundations/categories.md).

**lens.** A pair of functions `get` and `put` between two data structures that together behave like a disciplined two-way translation, subject to the round-trip laws. [Bidirectional lenses](../core/lenses.md).

**Lexicon.** The schema language of ATProto, consisting of JSON documents that declare record types and their constraints. [ATProto lexicons](../protocols/atproto.md).

**lift.** The operation that applies a compiled migration to an instance, producing an instance under the target schema. [The restrict/lift pipeline](../core/restrict-lift.md).

**migration.** A morphism of models in the category $\mathrm{Mod}(P)$ for a protocol $P$, packaged with a pushforward choice at each extension site. [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

**model.** A structure-preserving functor from the syntactic contextual category of a theory into a target contextual category. A panproto schema is a model of its protocol's theory. [Algebraic and generalised algebraic theories](../foundations/gats.md).

**morphism.** An arrow in a category. Synonym for *arrow* when the context is formal. [Categories](../foundations/categories.md).

**natural transformation.** A morphism between functors that respects the categorical structure on both sides; a collection of component morphisms indexed by objects of the source category, satisfying the naturality square. [Functors and natural transformations](../foundations/functors.md).

**product.** The universal cone over a two-object discrete diagram. In $\mathbf{Set}$ it is the Cartesian product. [Universal properties](../foundations/universal-properties.md).

**protocol.** A generalised algebraic theory together with a parser, an emitter, and a registered entry in the protocols registry. [Defining a protocol](../protocols/defining.md).

**protolens.** A schema-indexed family of lenses, of the form $\Pi(S : \mathrm{Schema}).\, \mathrm{Lens}(F(S), G(S))$. [Protolenses](../core/protolenses.md).

**pullback functor.** The functor $\Delta_f : \mathrm{Mod}(T_2) \to \mathrm{Mod}(T_1)$ induced by a theory morphism $f : T_1 \to T_2$, obtained by reading a model through $f$. [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

**pushforward functor.** One of the two adjoints of the pullback, $\Sigma_f$ (left) or $\Pi_f$ (right), going from $\mathrm{Mod}(T_1)$ to $\mathrm{Mod}(T_2)$. [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

**pushout.** The colimit of a span, the universal cocone under a three-object diagram with two arrows from a common source. [Colimits and pushouts](../foundations/colimits.md).

**schema.** A panproto schema, in the sense of this book: a model of a registered protocol's generalised algebraic theory. Includes protocol schemas (ATProto, Avro, and so on), relational schemas, and programming-language grammars derived from tree-sitter. [Protocols as theories, schemas as instances](../core/schemas-as-instances.md).

**serialize / deserialize.** The operations that turn a Rust value into bytes and back. In panproto, handled through [serde](https://serde.rs/).

**symmetric lens.** A lens between two structures neither of which is smaller than the other, with a shared complement between them. [Bidirectional lenses](../core/lenses.md).

**theory morphism.** A structure-preserving mapping between generalised algebraic theories, equivalent to a functor between the contextual categories they generate. [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

**tree-sitter.** The incremental parsing library used by panproto to auto-derive protocols for programming languages. [Tree-sitter and full-AST parsing](../protocols/tree-sitter.md).

**WASM boundary.** The opaque-handle, MessagePack-serialised boundary between panproto's Rust core and its non-Rust clients. [The WebAssembly boundary](../sdks/wasm-boundary.md).
