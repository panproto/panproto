; QVR syntax highlighting queries.
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

; ---------------------------------------------------------------------------
; keywords (derived from grammar literals)
; ---------------------------------------------------------------------------

[
  "as"
  "atoms"
  "attention"
  "binary"
  "binder_select"
  "binders"
  "binds"
  "body"
  "bundle"
  "categories"
  "category"
  "ccg"
  "change_base"
  "composition"
  "constructors"
  "contraction"
  "curry_left"
  "curry_right"
  "dagger"
  "decoder"
  "deduction"
  "define"
  "depth"
  "dim"
  "edge_kinds"
  "effect_depth"
  "encoder"
  "export"
  "factor"
  "freeze"
  "from"
  "in"
  "init"
  "iterations"
  "lambek"
  "let"
  "lex"
  "lexicon"
  "loss"
  "marginalize"
  "max_length"
  "message"
  "morphism"
  "observe"
  "op"
  "ops"
  "primitive"
  "program"
  "readout"
  "recurrent"
  "recursive"
  "return"
  "rule"
  "rules"
  "sample"
  "schema"
  "score"
  "signature"
  "sorts"
  "start"
  "structure"
  "terminal"
  "trace"
  "unary"
  "update"
  "var_init"
  "vertex_kinds"
  "where"
] @keyword

; Sort kinds in structural-compression signatures.
[
  "data"
  "index"
  "object"
] @type.qualifier

; ---------------------------------------------------------------------------
; builtin types (constructor / param-kind heads)
; ---------------------------------------------------------------------------

[
  "Ball"
  "CholeskyFactor"
  "Correlation"
  "Covariance"
  "Diagonal"
  "FinSet"
  "LowerTriangular"
  "Mor"
  "Nat"
  "Object"
  "Orthogonal"
  "Real"
  "Simplex"
  "Space"
  "Sphere"
  "Stiefel"
] @type.builtin

; ---------------------------------------------------------------------------
; builtin functions (combinators, intrinsics)
; ---------------------------------------------------------------------------

[
  "FreeMonoid"
  "FreeResiduated"
  "cap"
  "chart_fold"
  "cup"
  "fan"
  "from_data"
  "identity"
  "parser"
  "repeat"
  "scan"
  "stack"
] @function.builtin

; ---------------------------------------------------------------------------
; operators
; ---------------------------------------------------------------------------

[
  "*"
  "+"
  "-"
  "--"
  "->"
  "."
  "/"
  ":"
  "<-"
  "<<"
  "="
  ">>"
  ">>>"
  "@"
  "\\"
  "|-"
  "|->"
  "~"
  "⊢"
] @operator

; ---------------------------------------------------------------------------
; declarations and identifiers
; ---------------------------------------------------------------------------

(category_decl    names: (identifier) @type)
(object_decl      names: (identifier) @type)
(rule_decl        name: (identifier) @function)
(rule_decl        variables: (identifier) @variable.parameter)
(schema_decl      name: (identifier) @function)
(schema_parameter names: (identifier) @variable.parameter)
(morphism_decl    names: (identifier) @function)
(define_decl      name: (identifier) @function)
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
(lexicon_entry    words: (string) @string)

; Structural-compression declarations.
(signature_decl   name: (identifier) @type)
(signature_decl   params: (identifier) @type.parameter)
(sort_decl        name: (identifier) @type)
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
(encoder_op_rule  op: (identifier) @function)
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
