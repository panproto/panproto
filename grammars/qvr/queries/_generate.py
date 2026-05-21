"""Regenerate :file:`highlights.scm` from the current grammar.

The QVR tree-sitter grammar is the single source of truth for surface
syntax. This script reads :file:`grammars/qvr/src/grammar.json`, walks
each rule's terminal strings, and emits a deterministic
:file:`highlights.scm` consumed by every tree-sitter-driven
highlighter (nvim-treesitter, Helix, Zed, Emacs treesit, ...).

Run after any edit to :file:`grammars/qvr/grammar.js`:

.. code-block:: shell

    cd grammars/qvr
    tree-sitter generate          # produces src/grammar.json
    python queries/_generate.py   # rewrites queries/highlights.scm

The hand-curated parts (declaration node patterns binding identifier
roles like ``@type`` / ``@function``) live inline below; the literal
keyword / operator / builtin lists at the top are derived.
"""

from __future__ import annotations

import sys
from pathlib import Path

THIS_DIR = Path(__file__).resolve().parent
REPO_ROOT = THIS_DIR.parents[2]
sys.path.insert(0, str(REPO_ROOT / "src"))

from quivers.dsl._grammar_introspection import (  # noqa: E402
    BUILTIN_FUNCTIONS,
    BUILTIN_TYPES,
    COMPOSITION_LEVELS,
    KEYWORDS,
    OPERATORS,
    SORT_KINDS,
)


HEADER = """; QVR syntax highlighting queries.
;
; AUTO-GENERATED from grammars/qvr/src/grammar.json by
; grammars/qvr/queries/_generate.py. Do not edit by hand; rerun the
; generator after any grammar change. The literal keyword / builtin
; / operator lists below are derived from the grammar; the structural
; node-pattern rules (binding @type / @function / @variable roles to
; specific decl fields) are emitted from a fixed template inside the
; generator.
;
; Consumed by tree-sitter-driven highlighters: nvim-treesitter, Helix,
; Zed, Emacs treesit, and the in-tree Pygments lexer / REPL highlighter
; (which walk the same grammar through a shared mapping table).
"""

NODE_PATTERNS = """\
; ---------------------------------------------------------------------------
; declarations and identifiers
; ---------------------------------------------------------------------------

(category_decl    names: (identifier) @type)
(object_decl      name: (identifier) @type)
(rule_decl        name: (identifier) @function)
(rule_decl        variables: (identifier) @variable.parameter)
(schema_decl      name: (identifier) @function)
(schema_parameter names: (identifier) @variable.parameter)
(morphism_decl    name: (identifier) @function)
(let_decl         name: (identifier) @function)
(program_decl     name: (identifier) @function)
(bundle_decl      name: (identifier) @function)
(contraction_decl name: (identifier) @function)
(contraction_input name: (identifier) @function)
(composition_decl name: (identifier) @function)
(composition_rule_entry key: (identifier) @function)
(composition_rule_entry params: (identifier) @variable.parameter)
(enum_set_literal elements: (identifier) @constant)
(free_residuated_expr generators: (identifier) @type)
(free_monoid_expr generators: (identifier) @type)

; Constructor heads on object expressions.
(discrete_constructor constructor: _ @type.builtin)
(continuous_constructor constructor: _ @type.builtin)

; Object atoms in expression position.
(object_atom (identifier) @type)
(object_effect_apply effect: (identifier) @type)

; Latent / kernel morphism families (initializer).
(morphism_init_family family: (identifier) @type)

; Deduction blocks.
(deduction_decl   name: (identifier) @function)
(deduction_atoms  atoms: (identifier) @constant)
(deduction_rule   name: (identifier) @function)
(deduction_lexicon_from_file path: (string) @string)
(lexicon_entry    word: (string) @string)

; Structural-compression declarations.
(signature_decl   name: (identifier) @type)
(signature_decl   params: (identifier) @type.parameter)
(sort_decl        name: (identifier) @type)
(sort_decl        dim: (integer) @number)
(constructor_decl name: (identifier) @constructor)
(constructor_decl domain: (identifier) @type)
(constructor_decl codomain: (identifier) @type)
(binder_decl      name: (identifier) @constructor)
(binder_decl      codomain: (identifier) @type)
(binder_var_decl  var: (identifier) @variable.parameter)
(binder_var_decl  sort: (identifier) @type)
(binder_arg_decl  arg: (identifier) @variable.parameter)
(binder_arg_decl  sort: (identifier) @type)
(vertex_kind_decl name: (identifier) @type)
(edge_kind_decl   name: (identifier) @type)
(edge_kind_decl   src: (identifier) @type)
(edge_kind_decl   tgt: (identifier) @type)
(encoder_decl     name: (identifier) @function)
(encoder_decl     signature: (identifier) @type)
(decoder_decl     name: (identifier) @function)
(decoder_decl     signature: (identifier) @type)
(loss_decl        name: (identifier) @function)

; Pragmas.
(pragma_outer) @attribute
(pragma_inner) @attribute
(pragma_entry key: (identifier) @attribute)

; Identifier roles in expressions.
(expr_ident (identifier) @variable)
(let_var    (identifier) @variable)

; Sort-kind tokens highlight as type qualifiers.
(sort_kind) @type.qualifier

; ---------------------------------------------------------------------------
; literals
; ---------------------------------------------------------------------------

(integer)       @number
(float)         @number
(signed_number) @number
(string)        @string
(line_comment)  @comment
(block_comment) @comment.block
(doc_comment)   @comment.documentation
"""


def _quote_list(items: list[str]) -> str:
    """Format a sorted Scheme string-array for a ``@keyword`` capture."""
    rendered = "\n".join(f'  "{item}"' for item in sorted(items))
    return f"[\n{rendered}\n]"


def render() -> str:
    keyword_lines = [
        "; ---------------------------------------------------------------------------",
        "; keywords (derived from grammar literals)",
        "; ---------------------------------------------------------------------------",
        "",
        _quote_list(sorted(KEYWORDS - COMPOSITION_LEVELS - SORT_KINDS)) + " @keyword",
        "",
        "; Composition levels.",
        _quote_list(sorted(COMPOSITION_LEVELS)) + " @keyword.modifier",
        "",
        "; Sort kinds in structural-compression signatures.",
        _quote_list(sorted(SORT_KINDS)) + " @type.qualifier",
        "",
        "; Effect tokens carried by option-block values; not grammar literals.",
        '"!" @operator.special',
        "",
        "; ---------------------------------------------------------------------------",
        "; builtin types (constructor / param-kind heads)",
        "; ---------------------------------------------------------------------------",
        "",
        _quote_list(sorted(BUILTIN_TYPES)) + " @type.builtin",
        "",
        "; ---------------------------------------------------------------------------",
        "; builtin functions (combinators, intrinsics)",
        "; ---------------------------------------------------------------------------",
        "",
        _quote_list(sorted(BUILTIN_FUNCTIONS)) + " @function.builtin",
        "",
        "; ---------------------------------------------------------------------------",
        "; operators",
        "; ---------------------------------------------------------------------------",
        "",
        _quote_list(sorted(OPERATORS)) + " @operator",
    ]
    return "\n".join([HEADER, *keyword_lines, "", NODE_PATTERNS])


def main() -> int:
    out = THIS_DIR / "highlights.scm"
    out.write_text(render(), encoding="utf-8")
    mirror = (
        REPO_ROOT
        / "editors"
        / "zed-extension-qvr"
        / "languages"
        / "qvr"
        / "highlights.scm"
    )
    if mirror.is_file():
        mirror.write_text(render(), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
