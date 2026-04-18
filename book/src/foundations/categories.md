# Categories

Most of the rest of this book can be read as commentary on a single definition, and the present chapter gives it. The definition is short enough to write down in half a page. Becoming comfortable enough with it to reason by it takes longer, and the chapter is paced for that second requirement rather than the first: the formal statement arrives after the middle, once three worked settings have built up enough familiarity that the definition lands as the name for something already seen.

The concept originates with @eilenbergmaclane1945general, who introduced it to organise the comparison of algebraic structures in topology. It has since become the standard vocabulary for any situation where *things* and *directed operations between things* are studied together, and has been picked up in turn by logic, algebra, programming language theory, and the theory of databases. In this book the things are schemas and the operations are the migrations that carry data from one schema to another. By the end of the chapter the words *thing*, *directed operation*, and *structure* will have been pinned down precisely enough to be usable in every later chapter.

A reader who has met categories before can skim the next two sections and pick up at the category of panproto schemas. A reader who has not can expect the chapter to take about half an hour of careful reading; it is the one chapter in the book whose contents everything else depends on, and it repays a slow first pass.

## Composition, already familiar

The primitive operation of a category is composition: given two operations whose ends fit, a third operation is obtained that does the work of both in succession. Before writing this down as a definition we will look at three settings in which the operation is already familiar, so that the eventual definition lands as a name for something we can already do rather than as a new imposition.

A single running example will carry the chapter. A team maintains a dataset of people, each person carrying a name and an email address. After a time, the team decides to add a phone field. Later they rename `email` to `contact_email` to match a sibling field `contact_phone`. Each of the two decisions is a migration against the previous schema, and the effect of running them in order is itself a migration, one that transforms the original records in a single step. We will keep this picture in view as we move between settings.

### Function composition

The first place a programmer has already met composition is in the wiring together of functions. Given a function that trims whitespace from a string, and a second function that parses a trimmed string into a date, the operation of trimming a raw string and then parsing the result is itself a function from strings to dates. Haskell writes its two components as

```haskell
trim  :: String -> String
parse :: String -> Date
```

and writes the composite directly, using a period:

```haskell
parse . trim :: String -> Date
```

The period is pronounced "after": `parse . trim` is `parse` after `trim`, and applied to a string `s` it computes `parse (trim s)`. Rust supplies no operator of this shape, and the composite has to be written out as a new function by hand:

```rust
fn parse_trimmed(s: String) -> Date {
    parse(trim(s))
}
```

Whether we write `parse . trim`, `parse(trim(s))`, or `parse_trimmed`, we are pointing at the same object: a function whose input is a string and whose output is a date. The operator is a matter of convenience; the object it builds is the point.

One subtlety is worth flagging in advance. The order of composition runs right to left: the rightmost function is the one that runs first, which agrees with ordinary function application (`parse(trim(s))`) and with the standard mathematical notation for composition of morphisms. It disagrees with the left-to-right convention of Unix pipes and of F#'s `>>` operator, where the leftmost operation runs first. A reader calibrated to pipes will want to recalibrate here; we will use the right-to-left convention throughout and flag it on its first few uses.

### Command composition in a shell

The second setting is one any Unix user already has in hand. A shell pipeline chains commands, each consuming the output of the previous one:

```
lsof | grep Chrome | wc -l
```

Each command is a process that reads from standard input and writes to standard output. The vertical bar connects the output of one to the input of the next, and the pipeline as a whole is a process that consumes whatever the leftmost command consumes and produces whatever the rightmost command produces. Chain three commands and the result is a single "command" you could bind to a new name; chain four and the same holds. The bar is doing what the period in Haskell does: given two operations whose ends fit, it produces one new operation of the same kind.

The pipeline example is the one this chapter will have to give up on. It motivates the definition well enough, but processes have side effects and buffering behaviour that do not match the rigidity of a mathematical morphism, and pipelines do not in fact form a category in the strict sense. We will come back to this and admit it at the end of the chapter. Recognising the pattern without believing the example supports it fully is a useful habit: category theory tolerates motivating analogies and rejects them as instances in the same breath, and the reader will encounter the move often.

### Migration composition

The third setting is the subject of the book. A panproto schema is an object of a specific category, and a migration is a morphism between two schemas of that category. Given the two migrations of the running example — a first migration adding the `phone` field, a second renaming the email field — their composition is itself a migration, carrying a record of the original schema through both transformations in a single step.

Write the three schemas as $S_0$, $S_1$, and $S_2$ and the two migrations as $m_{01} : S_0 \to S_1$ and $m_{12} : S_1 \to S_2$. The composition $m_{12} \circ m_{01} : S_0 \to S_2$ is the migration that does both things at once, and the engine in [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) treats it as a first-class value of the same Rust type as its two components. What [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/) returns is not a script that runs the two in sequence but a single migration, compiled once, which can be applied to an instance of $S_0$ to yield an instance of $S_2$ directly.

### The pattern, named

All three settings present a class of *objects* (types, streams, schemas) together with a class of *directed operations* (functions, pipes, migrations) between them. Any two operations whose ends meet compose to yield a third in the same class. A category is the mathematical structure that captures exactly this pattern, and the next section writes down what that means.

A reader new to the idiom may reasonably ask why the pattern deserves a name of its own when function composition is already familiar to every programmer. The answer takes more than this chapter to spell out in full, but the short version is that once the structure is named, every result in the category-theoretic literature becomes available to every instance of the structure. The identity law we are about to meet is one any programmer would verify unconsciously for their favourite kind of arrow; what the category definition buys us is that verifying it once, at the level of the abstract structure, hands us a tool kit — functors, natural transformations, colimits, adjunctions — that applies the moment the structure is recognised. The rest of Part I is that tool kit.

## The definition

A **category** $\mathcal{C}$ consists of four pieces of data subject to two axioms.

The data: a class $\mathrm{Ob}(\mathcal{C})$ of **objects**; for each ordered pair of objects $A, B$ a set $\mathcal{C}(A, B)$ of **morphisms** from $A$ to $B$, with an individual morphism $f \in \mathcal{C}(A, B)$ written $f : A \to B$; for each triple of objects $A, B, C$ a **composition** operation sending a pair $(g, f)$ with $f : A \to B$ and $g : B \to C$ to a morphism $g \circ f : A \to C$; and for each object $A$ a distinguished **identity morphism** $\mathrm{id}_A : A \to A$.

The axioms require composition to be associative,

$$h \circ (g \circ f) \;=\; (h \circ g) \circ f$$

for any three composable morphisms, and the identity morphism to behave as a two-sided unit for composition,

$$f \circ \mathrm{id}_A \;=\; f \;=\; \mathrm{id}_B \circ f$$

for every $f : A \to B$.

The definition will appear in every chapter that follows. It rewards a second reading now.

### Reading the definition

What is notable about the four pieces of data is what they do not say. Nothing in the definition fixes what an object *is*. An object of a category is whatever the category has chosen to call one, and in different categories an object may be a Haskell type, a panproto schema, a topological space, or a formal proposition; the definition does not pretend to decide between these. The same looseness applies to morphisms: the definition requires only that between any two objects a *set* of morphisms exist, not that they be functions or continuous maps or any other specific kind of arrow. (The word for a category that respects the set-not-class restriction is **locally small**. Every category in this book is locally small, and we will not say so again.)

A morphism from $A$ to $B$ is also called an **arrow**, and the two words are used interchangeably; one says "arrow" while drawing a picture and "morphism" while writing an equation. The set $\mathcal{C}(A, B)$ is called the **hom-set** from $A$ to $B$, a piece of vocabulary inherited from categories whose morphisms preserve algebraic structure and were therefore called homomorphisms. Some authors write $\mathrm{Hom}_\mathcal{C}(A, B)$ or $\mathrm{Hom}(A, B)$ when the ambient category is obvious. We use $\mathcal{C}(A, B)$: it is less to write and no less precise.

### Associativity

Associativity says that a chain of three or more composable morphisms has a well-defined composite that does not depend on how we group adjacent pairs. Given

$$A \xrightarrow{f} B \xrightarrow{g} C \xrightarrow{h} D,$$

there are two ways to collapse the chain into a single morphism from $A$ to $D$: compose the first two first and get $h \circ (g \circ f)$, or compose the last two first and get $(h \circ g) \circ f$. The axiom says the two morphisms are equal, which is what lets us write $h \circ g \circ f$ without parentheses and without committing to an associativity convention.

Any programmer who has ever chained three functions together has relied on associativity without thinking about it: `parse . trim . normalise` is unambiguous because function composition is associative, and if it were not, Haskell would have to choose an associativity and the programmer would have to remember the choice. In every category we work with, composition is associative for a reason particular to the category — for functions, two functions agreeing on every input are equal, and both groupings produce the same value on every input — and once we know this is so, the reasoning from that point on is independent of why: we say "composition is associative" and treat the fact as available.

### The identity law

The identity law says that composing a morphism with an identity on either side leaves it alone:

$$f \circ \mathrm{id}_A \;=\; f \;=\; \mathrm{id}_B \circ f.$$

As a picture, the law is the commutativity of the square

$$
\begin{CD}
A @>{\mathrm{id}_A}>> A \\
@V{f}VV @VV{f}V \\
B @>>{\mathrm{id}_B}> B
\end{CD}
$$

*Figure 1.1: the identity law. The two paths from $A$ in the upper-left to $B$ in the lower-right yield $f \circ \mathrm{id}_A$ across the top and $\mathrm{id}_B \circ f$ down the left; the axiom requires both to equal $f$.*

The identity morphism plays the role a zero plays in addition or a one in multiplication: the thing a composition leaves untouched. The identity on a type is the function that returns its argument; the identity on a panproto schema is the migration that leaves every record exactly where it was; the identity on a topological space is the map sending every point to itself. A fair question is why the law needs to be stated at all, when every example one meets has an obviously neutral identity. The answer is that the definition of a category does not hand us the identity morphism: we have to supply one when constructing a category, and the law is the check that what we have supplied actually plays the role.

### Isomorphisms

A morphism $f : A \to B$ is an **isomorphism** when there is a morphism $g : B \to A$ satisfying both

$$g \circ f \;=\; \mathrm{id}_A \qquad \text{and} \qquad f \circ g \;=\; \mathrm{id}_B.$$

The morphism $g$, when it exists, is uniquely determined by $f$; we call it the **inverse** of $f$ and write $f^{-1}$. Two objects $A$ and $B$ are **isomorphic**, written $A \cong B$, when some isomorphism between them exists.

Isomorphism is the category's own notion of sameness. Two isomorphic objects are indistinguishable from within the category: any construction expressible in its morphisms will treat them identically. In the category of sets, isomorphism coincides with the existence of a bijection. In the category of Haskell types, two types are isomorphic if they have the same inhabitants up to relabelling. In the category of panproto schemas, two schemas are isomorphic when a migration goes each way and the two composites are identities — they are different names for the same structural content. Recognising which morphisms of a category are isomorphisms, and which are not, is usually the single most informative thing one can say about what the category is. The isomorphism question will reappear in every subsequent chapter, and it is the starting point of [Bidirectional lenses](../core/lenses.md), where the failure of most schema migrations to be invertible forces a weaker and more useful notion.

## The three examples, checked

We now have a definition and three informal examples. Whether the examples really are instances of the definition is a question the definition was written to make answerable, and we now answer it for each.

### Haskell types and functions

The category $\mathbf{Hask}$ has Haskell types as its objects and Haskell functions as its morphisms. Composition is the period from the standard library:

```haskell
(.) :: (b -> c) -> (a -> b) -> (a -> c)
(f . g) x = f (g x)
```

The identity on any type `a` is the polymorphic function

```haskell
id :: a -> a
id x = x
```

which simply returns its argument.

Associativity holds because Haskell functions are equal when they agree on every argument, and both groupings of a three-function chain agree on every argument: for every $x$,

$$((h \circ g) \circ f)(x) \;=\; h(g(f(x))) \;=\; (h \circ (g \circ f))(x).$$

The identity law holds by the same reasoning, since $(f \circ \mathrm{id})(x) = f(\mathrm{id}(x)) = f(x) = \mathrm{id}(f(x)) = (\mathrm{id} \circ f)(x)$.

A parenthetical for readers who know some Haskell: $\mathbf{Hask}$ as stated here is an idealisation. Real Haskell admits non-termination, exceptions, and the polymorphic bottom value `undefined`; taking these into account gives a more delicate structure sometimes written $\mathbf{Hask}_\bot$. The chapters that use $\mathbf{Hask}$ use the idealised form and the subtlety does not bear on any argument we will make. @barrwells1990category and @riehl2017category both discuss the fuller structure for a reader who wants it.

### Panproto schemas and migrations

Fix a panproto protocol $P$. The category $\mathbf{Sch}_P$ has the schemas under $P$ as its objects and migrations between them as its morphisms. The Rust representation of an object is a value of the [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) type from [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/); a morphism is a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value from [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). Composition is [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/), and the identity migration on a schema is the migration whose lift function returns its input untouched.

The claim that $\mathbf{Sch}_P$ is a category is therefore a claim about how the crate is arranged. Associativity is checked by the test suite: compose three migrations, compute the composite either way, and compare the lifted records on every input in a sampled state space. The identity law is checked similarly. Both properties hold by construction — `compose` was written so that they would — but the tests are what catch a bug in `compose` before it propagates.

Back to the running example. The team's $S_0 \xrightarrow{m_{01}} S_1 \xrightarrow{m_{12}} S_2$ composition is a morphism in $\mathbf{Sch}_P$ whose lift carries an $S_0$-record into the corresponding $S_2$-record. In Rust:

```rust
let m_01 = /* add-phone migration */;
let m_12 = /* rename-email migration */;
let m_02 = panproto_mig::compose(&m_12, &m_01)?;
```

The arguments are in right-to-left order, matching the mathematical convention. What `compose` returns is a single `Migration` value whose lift, applied to any $S_0$-record, yields the corresponding $S_2$-record; running the two migrations in sequence by hand would produce the same records at twice the cost. Every chapter of Part II is in one way or another about operations on $\mathbf{Sch}_P$.

### Unix pipelines, and why the example gives up

The pipeline example of §1 is not, strictly speaking, a category. Three obstacles stand in the way.

A process has side effects. `make clean | grep foo` does not merely transform bytes: it can delete files. Two pipelines that produce identical byte sequences can differ in what they write to disk, what signals they send, what they log, so there is no equivalence on processes under which composition is well-defined and identity neutral.

A process has buffering behaviour. `cat file | head -n 1` may kill `cat` with SIGPIPE before it has read the file in full, depending on buffer sizes. Reordering commutative-looking operations can change what runs and what does not, so the composition operation is not cleanly a function of the bytes in and bytes out.

A process imitates the identity only up to an equivalence that ignores timing and buffer boundaries. The idealised `cat` of the motivating example is the identity on byte streams; the actual executable introduces a process that reads, buffers, and writes, none of which a mathematical identity does.

The pipeline example therefore motivates the definition as illustration rather than as instance. Composition-as-primitive is already present in the working vocabulary of anyone who has used a shell, which is what we wanted from the example; that the analogy does not survive close inspection is a pattern we will meet often in category theory and is a habit worth cultivating. When we are told that something forms a category, the first response is to check; the second is to say, even when it does not, what structure it does have and what the failure tells us.

## The category of schemas

The category that will carry the rest of the book is $\mathbf{Sch}_P$, the schemas under a fixed protocol $P$.

A **schema**, in this book, means a model of a generalised algebraic theory in the sense of [Algebraic and generalised algebraic theories](./gats.md). The class this covers is much wider than the word "schema" might suggest to a database engineer. A JSON Schema document is a schema; an ATProto lexicon is a schema; an Apache Avro record definition is a schema; a SQL DDL file is a schema; a Rust source file, parsed through a tree-sitter grammar, is a schema; a FHIR resource profile is a schema. The claim developed in [GATs](./gats.md) and vindicated case by case in Part IV is that all of them are instances of the same mathematical object, and that migrations between them can therefore be handled uniformly by a single engine rather than per-format by many.

A **migration** from $S$ to $T$ is a structure-preserving map between the two models, in a sense [Theory morphisms and instance migration](../core/morphisms-and-migration.md) will make precise. For the present chapter we need only the categorical shape: whatever a migration is, it is the kind of operation the definition of $\mathbf{Sch}_P$'s morphisms demands.

Every construction of Part II takes place inside $\mathbf{Sch}_P$. Protocol composition is a colimit in a related category, handled in [Colimits and pushouts](./colimits.md). Functorial data migration, covered in [Morphisms and migration](../core/morphisms-and-migration.md), is a triple of functors between $\mathbf{Sch}_P$ and $\mathbf{Sch}_Q$ induced by a protocol morphism $P \to Q$. Bidirectional lenses enrich the morphisms of $\mathbf{Sch}_P$ with a controlled kind of inverse, and are developed in [Lenses](../core/lenses.md). Schematic version control uses pushouts in $\mathbf{Sch}_P$ to merge branches, which is the subject of [Merge as pushout](../vcs/merge-as-pushout.md). None of these constructions makes sense unless $\mathbf{Sch}_P$ is, in fact, a category; the claim that it is was the subject of this chapter.

## Further reading

The categorical literature is large and uneven in register. The reading list below is short and ordered from most accessible to most demanding.

@lawvereschanuel2009conceptual, *Conceptual Mathematics*, is the gentlest book-length introduction, written for a reader who has not studied university mathematics. @awodey2010category, *Category Theory*, is a careful graduate introduction example-rich enough to serve alongside this book's foundations chapters. @leinster2014basic, *Basic Category Theory*, is shorter, more opinionated, and freely available on arXiv; it is our recommendation to a reader who finds the register of this chapter too slow. @riehl2017category, *Category Theory in Context*, is the best single reference for anyone continuing past the basics. @maclane1998categories, *Categories for the Working Mathematician*, is the canonical reference text, demanding but authoritative.

For category theory with programming examples as motivation, @barrwells1990category, *Category Theory for Computing Science*, and @rydeheardburstall1988computational, *Computational Category Theory*, are the two classical sources; the latter gives the same material as executable ML code. @milewski2014essence, the blog series underlying most of this book's pedagogical moves, is the closest contemporary work in register and is recommended for a reader who wants an alternative pass through the same ideas.

## Closing

The next chapter introduces **functors**, the structure-preserving maps between categories and the setting in which most of Part I's later constructions are stated.
