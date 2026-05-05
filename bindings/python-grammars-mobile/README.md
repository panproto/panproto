# panproto-grammars-mobile

A panproto companion package shipping tree-sitter grammars for
mobile languages: Swift, Kotlin, Dart, Java, Objective-C.

## Install

```bash
pip install panproto-grammars-mobile
```

The package declares an entry point under `panproto.grammars`.
panproto's `AstParserRegistry` factory picks it up automatically;
there is nothing to import from this package directly.

## Use

```python
import panproto

reg = panproto.AstParserRegistry()
# parse one of the grammars this pack adds:
# schema = reg.parse_with_protocol("typescript", b"...", "main.ts")
```

See the panproto repository for the full list of available grammar
packs and the source for this one at
`crates/panproto-grammars-mobile/` and
`bindings/python-grammars-mobile/`.

## License

MIT.
