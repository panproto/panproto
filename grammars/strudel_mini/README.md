# strudel_mini

A tree-sitter grammar for [Strudel]'s mini-notation: the JavaScript-host port of TidalCycles mini-notation.

This is one of the few grammars in `panproto-grammars` authored in-repo against a documented language specification rather than vendored from an upstream `tree-sitter-strudel-mini` package; no such package exists. The companion grammar in `grammars/tidal_mini/` is its sibling for the Haskell-host original.

## Source spec

Authored from the official Strudel mini-notation reference at <https://strudel.cc/learn/mini-notation/>. Strudel's mini-notation is a port of Tidal's; the documented differences from Tidal — verified against the page — are encoded in the grammar:

- `-` is accepted as an alternative spelling of the rest token, in addition to `~`.
- The Tidal-specific `_` elongation marker is *not* in the Strudel docs (only the `@` suffix is).
- The Tidal-specific `{}` polymetric brackets are *not* in the Strudel docs.
- The Tidal-specific `%N` numeric ratio suffix is *not* in the Strudel docs.
- The Tidal-specific top-level `.` grouping shorthand is *not* in the Strudel docs.
- Strudel allows `|` and `,` at the top level of the pattern (per the example `"[g3,b3,e4] | [a3,c3,e4]"`); Tidal restricts those to inside containers.

Every construct in this grammar is grounded in a documented example from the Strudel page; per-rule comments cite the section it came from.

## Coverage

| Construct | Spec example | Grammar rule |
|---|---|---|
| Whitespace-separated events | `note("c e g b")` | `_pattern` |
| Step repetition (group) | `note("[e5 b4 d5 c5]*2")` | `repeat_suffix` on `group` |
| Step division (group) | `note("[e5 b4 d5 c5]/2")` | `divide_suffix` on `group` |
| Time subdivision `[ ]` | `note("e5 [b4 c5] d5")` | `group` |
| Alternation `< >` | `note("<e5 b4 d5 c5>")` | `alternation` |
| Polyphony `,` | `note("[g3,b3,e4]")` | `,` separator in `_pattern_list` |
| Elongation `@` | `note("<[g3,b3,e4]@2 [a3,c3,e4]>")` | `elongate_suffix` |
| Replication `!` | `note("<[g3,b3,e4]!2 [a3,c3,e4]>")` | `replicate_suffix` |
| Probability `?` | `note("[g3,b3,e4]*8?")`, `?0.1` | `probability_suffix` |
| Random selection `\|` | `note("[g3,b3,e4] \| [a3,c3,e4]")` | top-level `_pattern_list` |
| Euclidean | `s("bd(3,8,0)")` | `euclid_suffix` |
| Rest `~` or `-` | `note("[b4 [~ c5] d5 e5]")` | `rest` (choice of `~` and `-`) |

14 corpus tests under `test/corpus/spec_examples.txt`. All 14 pass.

## Usage

`grammars/strudel_mini/` contains both the source `grammar.js` and the generated `src/parser.c` / `src/grammar.json` / `src/node-types.json`, so panproto's `panproto-grammars` build picks the grammar up directly via the `lang-strudel_mini` feature. Local iteration:

```bash
cd grammars/strudel_mini
tree-sitter generate
tree-sitter test
```

The grammar reaches Python through the `panproto-grammars-music` companion pack: `pip install panproto-grammars-music` adds `strudel_mini` to `panproto.AstParserRegistry()`.

[Strudel]: https://strudel.cc/
