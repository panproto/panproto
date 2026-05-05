# panproto-grammars-devops

A panproto companion package shipping tree-sitter grammars for
devops languages: Dockerfile, Terraform, HCL, Nix, Bash, YAML, TOML, Make, CMake.

## Install

```bash
pip install panproto-grammars-devops
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
`crates/panproto-grammars-devops/` and
`bindings/python-grammars-devops/`.

## License

MIT.
