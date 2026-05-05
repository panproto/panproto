# tidal_mini

A tree-sitter grammar for the [TidalCycles] mini-notation: the *island grammar* embedded inside the string argument to `s`, `n`, `note`, and other pattern-producing functions in a Haskell-host Tidal program.

This is one of the few grammars in `panproto-grammars` authored in-repo against a documented language specification rather than vendored from an upstream `tree-sitter-tidal-mini` package; no such package exists. The companion grammar in `grammars/strudel_mini/` is its sibling for the JavaScript-host port.

## Source spec

Every construct in this grammar is grounded in the official mini-notation reference at <https://tidalcycles.org/docs/reference/mini_notation>. The grammar's per-rule comments cite the documented example each rule was derived from. No syntax that does not appear in that page has been added.

## Coverage

| Construct | Spec example | Grammar rule |
|---|---|---|
| Rest | `"~ hh"` | `rest` (literal `~`) |
| Step repetition | `"bd*2 sd"` | `repeat_suffix` |
| Step division | `"bd/2"` | `divide_suffix` |
| Replication | `"bd!3 sd"` | `replicate_suffix` |
| Elongation marker | `"bd _ _ ~ sd _"` | `elongation` |
| Elongation suffix | `"superpiano@3 superpiano"` | `elongate_suffix` |
| Probabilistic removal | `"bd? sd"`, `"hh?0.8"` | `probability_suffix` |
| Sample selection | `"arpy:1 arpy:2"` | `event` (`name:sample`) |
| Numeric ratio | `"bd*4%2"` | `ratio_suffix` |
| Random choice | `"[bd\|hh\|cp]"` | `\|` separator in `_pattern_list` |
| Group brackets | `"[bd sd] hh"` | `group` |
| Alternation | `"bd <sd hh cp>"` | `alternation` |
| Polymetric | `"{bd hh}%8"` | `polymetric` |
| Euclidean | `"bd(3,8)"`, `"bd(3,8,1)"` | `euclid_suffix` |
| Superposition | `"[bd*2,hh*3]"` | `,` separator in `_pattern_list` |
| Top-level dot | `"bd*3 . hh*4 cp"` | `.` between `_dot_group`s |
| Nested subdivision | `"[bd [hh [cp sn:2] hh]]"` | recursive `group` |

22 corpus tests under `test/corpus/spec_examples.txt`, each named after the spec construct it exercises. All 22 pass with the latest `tree-sitter generate`.

## Usage

`grammars/tidal_mini/` contains both the source `grammar.js` and the generated `src/parser.c` / `src/grammar.json` / `src/node-types.json`, so panproto's `panproto-grammars` build picks the grammar up directly via the `lang-tidal_mini` feature. Local iteration:

```bash
cd grammars/tidal_mini
tree-sitter generate
tree-sitter test
```

The grammar reaches Python through the `panproto-grammars-music` companion pack: `pip install panproto-grammars-music` adds `tidal_mini` to `panproto.AstParserRegistry()`.

[TidalCycles]: https://tidalcycles.org/
