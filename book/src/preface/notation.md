# A note on notation

This chapter documents the conventions the book uses for citations, code, mathematics, cross-references, and figures. Readers comfortable with academic technical writing will find few surprises; the chapter exists so that a reader who is uncertain what a particular glyph or link type is meant to do can look it up here rather than in the chapter where the glyph first appears.

## Citations

Every academic source the book names carries an inline citation in Chicago author-date style, rendered automatically by the [mdbook-bib](https://github.com/francisco-perez-sorrosal/mdbook-bib) preprocessor. The three citation forms, in Pandoc notation, are:

- `@key` produces a textual citation ("Mac Lane (1998)") and is used when the cited author is the grammatical subject of the sentence.
- `[@key]` produces a parenthetical citation ("(Mac Lane 1998)") and is used when the citation follows a claim.
- `[-@key]` produces a year-only citation ("(1998)") and is used after a name already appears in the sentence.

Multiple citations combine inside a single bracket: `[@key1; @key2]`. The complete bibliography appears at the end of the book, with full metadata and links. Every entry has been read before being cited.

## Cross-references

References to other chapters appear as hyperlinks to the chapter's title: for example, [Categories](../foundations/categories.md). Chapter numbers are not used in prose, since the book's table of contents is by title rather than by number. References to specific sections within a chapter use anchor links: the reader may follow the link directly to the section named.

References to specific panproto code appear as hyperlinks to the corresponding [docs.rs](https://docs.rs/) page. Every first mention of a panproto module, type, or function in a chapter is linked; subsequent mentions appear as plain monospace.

## Code blocks

Code appears in fenced blocks with the language tagged. Haskell and Rust are used most often. Haskell carries mathematical definitions whose type syntax is close to the standard mathematical notation for morphisms; Rust carries concrete panproto implementation. Where relevant, a code block is followed by an italic caption numbered `Listing N.M`, where `N` is the chapter's position and `M` is the listing's position within the chapter.

The caption identifies what the code shows and notes any conventions the reader may not have encountered. A reader who wants a fuller account of panproto's Rust API should consult the [docs.rs documentation for the relevant crate](https://docs.rs/panproto-core/latest/panproto_core/).

## Mathematics

Inline mathematics uses `$…$`. Displayed mathematics appears on its own line between `$$…$$` delimiters, rendered by [mdbook-katex](https://github.com/lzanini/mdbook-katex). A display equation is preceded by a sentence that names the symbols appearing in it; a reader who has lost track of a symbol's meaning can scan back to the introducing sentence.

Standard categorical symbols are used throughout: $\mathcal{C}, \mathcal{D}$ for categories, $f : A \to B$ for a morphism, $g \circ f$ for composition, $\mathrm{id}_A$ for the identity on $A$, $\mathcal{C}(A, B)$ for the hom-set from $A$ to $B$, $A \cong B$ for isomorphism. Panproto-specific conventions are collected in the [notation reference appendix](../appendices/notation-table.md).

Commutative diagrams are rendered through KaTeX's `\begin{CD}…\end{CD}` environment for simple squares and triangles, through [mermaid](https://mermaid.js.org/) graphs for informal "picture of a category" sketches, and through committed SVG (exported from [quiver](https://q.uiver.app/)) for diagrams that require diagonals or pasting-diagram structure beyond what KaTeX supports.

## External links

Every external tool, library, language, or specification carries a hyperlink to its canonical home page on first mention per chapter. Later mentions in the same chapter appear as plain prose. The convention follows academic practice for Web resources and matches how the [docs.rs](https://docs.rs/) code links work.

## What the book assumes

The book assumes the reader can program. It does not assume a specific language background: examples use Haskell for mathematical brevity and Rust for implementation, with each new language construct glossed on first appearance. The book does not assume any prior category theory, set theory beyond the informal level of everyday programming, or familiarity with panproto itself.

A reader who wants a warm-up in category theory before the foundations chapters begin may skim the first few posts of Bartosz Milewski's [Category Theory for Programmers](https://bartoszmilewski.com/2014/10/28/category-theory-for-programmers-the-preface/), from which several of the book's pedagogical conventions are adapted.

## Closing

[What panproto is](./what-panproto-is.md), the next chapter, frames panproto against the tools a working developer is likely to have used already and is the chapter to read before [Categories](../foundations/categories.md).
