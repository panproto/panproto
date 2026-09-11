# tidal_mini

Tree-sitter grammar for the contents of a [TidalCycles mini-notation](https://tidalcycles.org/docs/reference/mini_notation/) string. It does not parse the surrounding Haskell or the string delimiters.

## Features

| Syntax | Tree-sitter rule |
|---|---|
| Whitespace-separated events and unsigned decimal numbers | `_dot_group` |
| `name:N` sample selection | `event` |
| Rest `~` and elongation marker `_` | `rest`, `elongation` |
| Subdivision with `[ ]` | `group` |
| Alternation with `< >` | `alternation` |
| Polymetric groups with `{ }` and optional `%N` | `polymetric` |
| Parallel or random branches separated by `,` or `\|` | `_pattern_list` |
| Top-level grouping shorthand `.` | `_pattern` |
| `*N`, `/N`, `@N`, `!N`, `?N`, and `%N` suffixes | the corresponding suffix rule |
| `(beats,steps)` and `(beats,steps,offset)` | `euclid_suffix` |

The grammar permits either `,` or `|` separator in the same container. The parser records syntax only and does not enforce the distinct Tidal semantics of superposition and random choice.

## Usage

Enable the `lang-tidal_mini` Cargo feature to compile this grammar into
`panproto-grammars`. Python users can install the same grammar through the
`panproto-grammars-music` companion package.

## Limitations

Identifiers match `[A-Za-z][A-Za-z0-9_]*`, and numbers are unsigned integers or decimals. Thus this grammar is not a parser for every value or sample name Tidal may accept. A rest is a standalone step and cannot carry suffixes. Semantic constraints, such as valid probability ranges or Euclidean parameters, are not checked.

## Development

`grammar.js` is the source. `src/parser.c`, `src/grammar.json`, and `src/node-types.json` are generated tree-sitter artifacts consumed by `panproto-grammars`. The corpus at `test/corpus/spec_examples.txt` contains 22 cases. The duplicate path `test/test/corpus/spec_examples.txt` currently contains the same file.

```sh
cd grammars/tidal_mini
tree-sitter generate
tree-sitter test
```

## License

[MIT](LICENSE)
