# Panproto Theory DSL: Nickel contract library.
#
# Provides typed contracts and combinator functions for defining
# GAT theories, morphisms, compositions, and protocols in Nickel.
#
# Usage:
#   let { Theory, Morphism, Sort, Op, simple, unary, eq, colimit, .. }
#     = import "panproto/theory.ncl" in
#   { id = "dev.my-domain.my-theory", description = "...", ... }
#
# Parameter names use a _ prefix to avoid colliding with the record
# field names they populate (Nickel's record scope shadows enclosing
# bindings when the names coincide, causing infinite recursion).

{
  # ═══════════════════════════════════════════════════════════════════
  # Sort kind constructors
  # ═══════════════════════════════════════════════════════════════════

  Structural = { type = "structural" },
  Val = fun _kind => { type = "val", value_kind = _kind },
  Coercion = fun _from _to _class => {
    type = "coercion", from = _from, to = _to, class = _class
  },
  Merger = fun _kind => { type = "merger", value_kind = _kind },

  # ═══════════════════════════════════════════════════════════════════
  # Contracts
  # ═══════════════════════════════════════════════════════════════════

  ParamSpec = {
    name | String,
    sort | String,
    implicit | Bool | optional,
  },

  SortKindSpec = {
    type | String,
    ..
  },

  SortSpec = {
    name | String,
    params | Array ParamSpec | optional,
    kind | optional,
  },

  OpSpec = {
    name | String,
    input | String | optional,
    inputs | Array ParamSpec | optional,
    output | String,
  },

  EquationSpec = {
    name | String,
    lhs | String,
    rhs | String,
  },

  DirectedEqSpec = {
    name | String,
    lhs | String,
    rhs | String,
    impl_expr | String,
    inverse | String | optional,
    source_kind | String | optional,
    target_kind | String | optional,
    coercion_class | String | optional,
  },

  StrategySpec = {
    type | String,
    ..
  },

  PolicySpec = {
    name | String,
    value_kind | String,
    strategy | StrategySpec,
  },

  EdgeRuleSpec = {
    edge_kind | String,
    src_kind | String,
    tgt_kind | String,
  },

  ColimitStepSpec = {
    left | String,
    right | String,
    shared_sorts | Array String,
    shared_ops | Array String | optional,
  },

  # ═══════════════════════════════════════════════════════════════════
  # Document contracts
  # ═══════════════════════════════════════════════════════════════════

  Theory = {
    id | String,
    description | String,
    theory | String,
    extends | Array String | optional,
    sorts | Array SortSpec | optional,
    ops | Array OpSpec | optional,
    equations | Array EquationSpec | optional,
    directed_equations | Array DirectedEqSpec | optional,
    policies | Array PolicySpec | optional,
  },

  Morphism = {
    id | String,
    description | String,
    morphism | String,
    domain | String,
    codomain | String,
    sort_map | { _ : String },
    op_map | { _ : String },
  },

  Composition = {
    id | String,
    description | String,
    compose | {
      result | String,
      bases | Array String | optional,
      steps | Array ColimitStepSpec,
    },
  },

  Protocol = {
    id | String,
    description | String,
    protocol | String,
    schema_theory | String | { theory | String, .. } | { result | String, .. },
    instance_theory | String | { theory | String, .. } | { result | String, .. },
    edge_rules | Array EdgeRuleSpec | optional,
  },

  Bundle = {
    id | String,
    description | String,
    bundle | String,
    theories | Array { theory | String, .. } | optional,
    morphisms | Array { morphism | String, .. } | optional,
    compositions | Array { result | String, .. } | optional,
    protocols | Array { protocol | String, .. } | optional,
  },

  Class = {
    id | String,
    description | String,
    class | String,
    params | Array String,
    signatures | Array OpSpec,
    axioms | Array EquationSpec | optional,
  },

  Instance = {
    id | String,
    description | String,
    instance | String,
    class | String,
    target | String,
    bindings | { _ : String },
  },

  # ═══════════════════════════════════════════════════════════════════
  # Sort combinators
  # ═══════════════════════════════════════════════════════════════════

  simple = fun _name => { name = _name },
  dependent = fun _name _params => { name = _name, params = _params },
  val_sort = fun _name _kind => { name = _name, kind = Val _kind },
  coercion_sort = fun _name _from _to _class => {
    name = _name, kind = Coercion _from _to _class
  },
  merger_sort = fun _name _kind => { name = _name, kind = Merger _kind },
  param = fun _name _sort => { name = _name, sort = _sort },

  # ═══════════════════════════════════════════════════════════════════
  # Operation combinators
  # ═══════════════════════════════════════════════════════════════════

  unary = fun _name _input _output => {
    name = _name, input = _input, output = _output,
  },

  binary = fun _name _in1 _in2 _output => {
    name = _name,
    inputs = [{ name = _in1.0, sort = _in1.1 }, { name = _in2.0, sort = _in2.1 }],
    output = _output,
  },

  nullary = fun _name _output => {
    name = _name, inputs = [], output = _output,
  },

  nary = fun _name _inputs _output => {
    name = _name, inputs = _inputs, output = _output,
  },

  # ═══════════════════════════════════════════════════════════════════
  # Equation combinators
  # ═══════════════════════════════════════════════════════════════════

  eq = fun _name _lhs _rhs => {
    name = _name, lhs = _lhs, rhs = _rhs,
  },

  directed_eq = fun _name _lhs _rhs _impl_expr => {
    name = _name, lhs = _lhs, rhs = _rhs, impl_expr = _impl_expr,
  },

  directed_eq_invertible = fun _name _lhs _rhs _impl_expr _inverse => {
    name = _name, lhs = _lhs, rhs = _rhs,
    impl_expr = _impl_expr, inverse = _inverse,
  },

  # ═══════════════════════════════════════════════════════════════════
  # Policy combinators
  # ═══════════════════════════════════════════════════════════════════

  keep_left = fun _name _value_kind => {
    name = _name, value_kind = _value_kind,
    strategy = { type = "keep_left" },
  },

  keep_right = fun _name _value_kind => {
    name = _name, value_kind = _value_kind,
    strategy = { type = "keep_right" },
  },

  fail_on_conflict = fun _name _value_kind => {
    name = _name, value_kind = _value_kind,
    strategy = { type = "fail" },
  },

  custom_policy = fun _name _value_kind _expr => {
    name = _name, value_kind = _value_kind,
    strategy = { type = "custom", expr = _expr },
  },

  # ═══════════════════════════════════════════════════════════════════
  # Composition combinators
  # ═══════════════════════════════════════════════════════════════════

  colimit = fun _left _right _shared_sorts => {
    left = _left, right = _right,
    shared_sorts = _shared_sorts, shared_ops = [],
  },

  colimit_with_ops = fun _left _right _shared_sorts _shared_ops => {
    left = _left, right = _right,
    shared_sorts = _shared_sorts, shared_ops = _shared_ops,
  },

  # ═══════════════════════════════════════════════════════════════════
  # Edge rule combinators
  # ═══════════════════════════════════════════════════════════════════

  edge_rule = fun _edge_kind _src_kind _tgt_kind => {
    edge_kind = _edge_kind, src_kind = _src_kind, tgt_kind = _tgt_kind,
  },
}
