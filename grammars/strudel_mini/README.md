# strudel_mini

Tree-sitter grammar for the contents of a [Strudel mini-notation](https://strudel.cc/learn/mini-notation/) string. It does not parse the surrounding JavaScript or the string delimiters.

## Implemented syntax

| Syntax | Tree-sitter rule |
|---|---|
| Whitespace-separated events and unsigned decimal numbers | `_pattern` |
| `name:N` sample selection | `event` |
| Rests written as `~` or `-` | `rest` |
| Subdivision with `[ ]` | `group` |
| Alternation with `< >` | `alternation` |
| Parallel or random branches separated by `,` or `\|` | `_pattern_list` |
| `*N`, `/N`, `@N`, `!N`, and `?N` suffixes | the corresponding suffix rule |
| `(beats,steps)` and `(beats,steps,offset)` | `euclid_suffix` |

The grammar permits `,` and `|` at the top level as well as inside brackets. It permits either separator in the same list. The parser records syntax only and does not assign distinct semantics to the two separators.

## Limits

This is a subset of the current Strudel notation. Identifiers match `[A-Za-z][A-Za-z0-9_]*`, and numbers are unsigned integers or decimals. The grammar does not accept pitch spellings containing `#`. It also omits `_` elongation, even though the current Strudel reference includes `_` in its review example. Tidal-specific `{ }`, `%N`, and top-level `.` forms are not implemented.

These limits describe the checked-in `grammar.js`. The README does not claim conformance with every construct accepted by Strudel.

## Files and tests

`grammar.js` is the source. `src/parser.c`, `src/grammar.json`, and `src/node-types.json` are generated tree-sitter artifacts consumed by `panproto-grammars`. The corpus at `test/corpus/spec_examples.txt` contains 14 cases. The duplicate path `test/test/corpus/spec_examples.txt` currently contains the same file.

```bash
cd grammars/strudel_mini
tree-sitter generate
tree-sitter test
```

The `lang-strudel_mini` feature enables this grammar. The Python `panproto-grammars-music` wheel enables the same feature through `group-music`.

## License

[MIT](LICENSE)
