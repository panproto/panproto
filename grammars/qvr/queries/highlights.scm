; QVR syntax highlighting queries.
;
; Consumed by tree-sitter–driven highlighters: nvim-treesitter,
; Emacs treesit (Emacs 29+), Helix, Zed, and anything else that
; reads `queries/highlights.scm` from a tree-sitter grammar
; directory. The Pygments lexer at quivers.dsl.pygments_lexer
; walks the same tree-sitter parse via a Python token-mapping
; table and produces the same colourings; both surfaces share
; the grammar as the single source of truth.

; ---------------------------------------------------------------------------
; keywords
; ---------------------------------------------------------------------------

; Module-level declaration / statement keywords.
[
  "algebra"
  "semigroupoid"
  "bilinear_form"
  "composition_rule"
  "contraction"
  "category"
  "rule"
  "wiring"
  "schema"
  "object"
  "let"
  "export"
  "output"
  "where"
  "type"
  "space"
  "kernel"
  "discretize"
  "embed"
  "program"
  "alias"
  "bundle"
  "observe"
  "marginalize"
  "in"
  "for"
  "over"
  "iid"
  "via"
  "return"
  "latent"
  "observed"
] @keyword

; Effect-signature keywords (the `! Sample, Score` row after a
; program signature, and the `over M` modifier).
[
  "Pure"
  "Sample"
  "Score"
  "Marginal"
] @keyword.modifier

; Deduction-block keywords.
[
  "deduction"
  "atoms"
  "semiring"
  "start"
  "depth"
  "lexicon"
  "from"
  "with"
  "axioms"
  "learnable"
  "compressor"
] @keyword

; Structural-compression block keywords (signature / encoder /
; decoder / loss). Sort kinds (`object` / `index` / `data`) are
; coloured as type keywords; encoder-body shapes (`recurrent`,
; `attention`, `message_passing` slots) as control keywords.
[
  "signature"
  "encoder"
  "decoder"
  "loss"
] @keyword.declaration

[
  "sorts"
  "constructors"
  "binders"
  "vertex_kinds"
  "edge_kinds"
  "binds"
  "dim"
  "vocab"
  "weight"
  "on"
  "of"
  "chart"
  "iterations"
  "readout"
  "init"
  "message"
  "update"
  "var_init"
  "as"
  "recurrent"
  "attention"
  "structure"
  "primitive"
  "factor"
  "binder_select"
  "body"
  "recursive"
] @keyword

(sort_kind) @type.qualifier

; Built-in morphism / let-expression / object-constructor functions.
[
  "identity"
  "fan"
  "repeat"
  "stack"
  "scan"
  "parser"
  "ccg"
  "lambek"
  "chart_fold"
  "parse"
  "curry_right"
  "curry_left"
  "FreeResiduated"
  "FreeMonoid"
] @function.builtin

(let_call func: _ @function.builtin)

; ---------------------------------------------------------------------------
; operators
; ---------------------------------------------------------------------------

[
  "|->"
] @operator.body-arrow

[
  "|-"
  "⊢"
] @operator.sequent

[
  "->"
  "=>"
  ">>>"
  ">>"
  "<<"
  ">=>"
  "<-"
  "--"
  "~"
  "@"
  "*"
  "+"
  "/"
  "\\"
  "-"
  "="
  ":"
  "."
] @operator

; The effect-signature marker on a program declaration.
"!" @operator.special

; ---------------------------------------------------------------------------
; declarations and identifiers
; ---------------------------------------------------------------------------

(algebra_decl name: (identifier) @constant)
(category_decl names: (identifier) @type)
(object_decl   name: (identifier) @type)
(rule_decl     name: (identifier) @function)
(rule_decl     variables: (identifier) @variable.parameter)
(schema_decl   name: (identifier) @function)
(schema_parameter names: (identifier) @variable.parameter)
(morphism_decl name: (identifier) @function)
(let_decl      name: (identifier) @function)
(kernel_decl name: (identifier) @function)
(discretize_decl name: (identifier) @function)
(embed_decl    name: (identifier) @function)
(program_decl  name: (identifier) @function)
(space_decl    name: (identifier) @type)
(type_alias_decl name: (identifier) @type)
(enum_set_literal elements: (identifier) @constant)
(free_residuated_expr generators: (identifier) @type)
(free_monoid_expr generators: (identifier) @type)

(space_constructor       constructor: (identifier) @type.builtin)
(space_constructor_bare  constructor: (identifier) @type.builtin)

(kernel_decl family: (identifier) @type)

; Latent morphism prior: `latent W ... ~ Family(args) ...`.
; Colour the prior's family name like a kernel's family name.
(morphism_prior family: (identifier) @type)

; Axis-role clauses: `over <axes> [iid over <axes>]`.  Axes are
; identifiers that name dom/cod factors; the reserved tokens `dom`
; and `cod` are shortcuts.  Color the axis names so they stand out
; from generic identifiers in the surrounding distribution clause.
(axis_role_clause over: (identifier) @variable.parameter)
(axis_role_clause iid_over: (identifier) @variable.parameter)
(axis_tuple axis: (identifier) @variable.parameter)

; Deduction-block heads colour the bound name as a function /
; type per the surface convention (deductions are values that
; produce a chart; atoms / lexicon entries are constants).
(deduction_decl name: (identifier) @function)
(deduction_atoms atoms: (identifier) @constant)
(deduction_semiring semiring: (identifier) @constant)
(deduction_start start: (identifier) @constant)
(deduction_axioms source: (identifier) @function)
(deduction_signature signature: (identifier) @type)
(deduction_encoder_attach encoder: (identifier) @function)
(lexicon_entry word: (string) @string)
(lexicon_entry learnable: (learnable_marker) @keyword)

; Structural-compression declarations: every header binds a
; type-like name; sort / constructor / binder / vertex_kind /
; edge_kind names colour as types; encoder / decoder / loss
; names colour as functions.
(signature_decl name: (identifier) @type)
(signature_decl params: (identifier) @type.parameter)
(sort_decl name: (identifier) @type)
(sort_decl dim: (integer) @number)
(constructor_decl name: (identifier) @constructor)
(constructor_decl domain: (identifier) @type)
(constructor_decl codomain: (identifier) @type)
(binder_decl name: (identifier) @constructor)
(binder_decl codomain: (identifier) @type)
(binder_var_decl var: (identifier) @variable.parameter)
(binder_var_decl sort: (identifier) @type)
(binder_var_decl annot: (identifier) @variable.parameter)
(binder_var_decl annot_sort: (identifier) @type)
(binder_arg_decl arg: (identifier) @variable.parameter)
(binder_arg_decl sort: (identifier) @type)
(vertex_kind_decl name: (identifier) @type)
(edge_kind_decl name: (identifier) @type)
(edge_kind_decl src: (identifier) @type)
(edge_kind_decl tgt: (identifier) @type)

(encoder_decl name: (identifier) @function)
(encoder_decl signature: (identifier) @type)
(encoder_op_rule op: (identifier) @constructor)
(encoder_op_rule args: (identifier) @variable.parameter)
(encoder_op_rule state: (identifier) @variable.parameter)
(encoder_op_rule prefix: (identifier) @variable.parameter)
(encoder_init_rule kind: (identifier) @type)
(encoder_init_rule arg: (identifier) @variable.parameter)
(encoder_message_rule edge_kind: (identifier) @type)
(encoder_message_rule src: (identifier) @variable.parameter)
(encoder_message_rule tgt: (identifier) @variable.parameter)
(encoder_update_rule vertex_kind: (identifier) @type)
(encoder_update_rule self: (identifier) @variable.parameter)
(encoder_update_rule msgs: (identifier) @variable.parameter)
(encoder_var_init var_sort: (identifier) @type)
(encoder_var_init annot_sort: (identifier) @type)
(encoder_var_init ty: (identifier) @variable.parameter)
(encoder_dim sort: (identifier) @type)

(decoder_decl name: (identifier) @function)
(decoder_decl signature: (identifier) @type)
(decoder_structure arg: (identifier) @variable.parameter)
(decoder_primitive arg: (identifier) @variable.parameter)
(decoder_factor arg: (identifier) @variable.parameter)
(decoder_binder_select arg: (identifier) @variable.parameter)
(decoder_dim sort: (identifier) @type)

(loss_decl name: (identifier) @function)
(loss_attachment target: (identifier) @function)
(loss_attachment rule_name: (identifier) @function)
(loss_attachment deduction: (identifier) @function)
(loss_attachment chart_of: (identifier) @function)
(loss_attachment_kind) @keyword

; identifiers in patterns / expressions
(type_atom   (identifier) @type)
(type_effect_apply effect: (identifier) @type)
(space_atom  (identifier) @type)
(expr_ident  (identifier) @variable)
(let_var     (identifier) @variable)

; ---------------------------------------------------------------------------
; literals
; ---------------------------------------------------------------------------

(integer)       @number
(float)         @number
(signed_number) @number
(string)        @string
(line_comment)  @comment
(doc_comment)   @comment.documentation
