# Panproto Theory DSL: Nickel contract library.
#
# Provides typed contracts and combinator functions for defining
# GAT theories, morphisms, compositions, and protocols in Nickel.
#
# Usage:
#   let { Theory, Morphism, Sort, Op, simple, unary, eq, colimit, .. }
#     = import "panproto/theory.ncl" in
#   { id = "dev.my-domain.my-theory", description = "...", ... }

# ═══════════════════════════════════════════════════════════════════
# Sort kind constructors
# ═══════════════════════════════════════════════════════════════════

let Structural = { type = "structural" } in
let Val = fun kind => { type = "val", value_kind = kind } in
let Coercion = fun from to class => {
  type = "coercion", from = from, to = to, class = class
} in
let Merger = fun kind => { type = "merger", value_kind = kind } in

# ═══════════════════════════════════════════════════════════════════
# Contracts
# ═══════════════════════════════════════════════════════════════════

let ParamSpec = {
  name | String,
  sort | String,
  implicit | Bool | optional,
} in

let SortKindSpec = {
  type | String,
  ..
} in

let SortSpec = {
  name | String,
  params | Array ParamSpec | default = [],
  kind | default = Structural,
} in

let OpSpec = {
  name | String,
  input | String | optional,
  inputs | Array ParamSpec | optional,
  output | String,
} in

let EquationSpec = {
  name | String,
  lhs | String,
  rhs | String,
} in

let DirectedEqSpec = {
  name | String,
  lhs | String,
  rhs | String,
  impl_expr | String,
  inverse | String | optional,
  source_kind | String | optional,
  target_kind | String | optional,
  coercion_class | String | default = "iso",
} in

let StrategySpec = {
  type | String,
  ..
} in

let PolicySpec = {
  name | String,
  value_kind | String,
  strategy | StrategySpec,
} in

let EdgeRuleSpec = {
  edge_kind | String,
  src_kind | String,
  tgt_kind | String,
} in

let ColimitStepSpec = {
  left | String,
  right | String,
  shared_sorts | Array String,
  shared_ops | Array String | default = [],
} in

# ═══════════════════════════════════════════════════════════════════
# Document contracts
# ═══════════════════════════════════════════════════════════════════

let Theory = {
  id | String,
  description | String,
  theory | String,
  extends | Array String | default = [],
  sorts | Array SortSpec | default = [],
  ops | Array OpSpec | default = [],
  equations | Array EquationSpec | default = [],
  directed_equations | Array DirectedEqSpec | default = [],
  policies | Array PolicySpec | default = [],
} in

let Morphism = {
  id | String,
  description | String,
  morphism | String,
  domain | String,
  codomain | String,
  sort_map | { _ : String },
  op_map | { _ : String },
} in

let Composition = {
  id | String,
  description | String,
  compose | {
    result | String,
    bases | Array String | default = [],
    steps | Array ColimitStepSpec,
  },
} in

let Protocol = {
  id | String,
  description | String,
  protocol | String,
  schema_theory | String | { theory | String, .. } | { result | String, .. },
  instance_theory | String | { theory | String, .. } | { result | String, .. },
  edge_rules | Array EdgeRuleSpec | default = [],
} in

let Bundle = {
  id | String,
  description | String,
  bundle | String,
  theories | Array { theory | String, .. } | default = [],
  morphisms | Array { morphism | String, .. } | default = [],
  compositions | Array { result | String, .. } | default = [],
  protocols | Array { protocol | String, .. } | default = [],
} in

# ═══════════════════════════════════════════════════════════════════
# Sort combinators
# ═══════════════════════════════════════════════════════════════════

let simple = fun name => { name = name } in
let dependent = fun name params => { name = name, params = params } in
let val_sort = fun name kind => { name = name, kind = Val kind } in
let coercion_sort = fun name from to class => {
  name = name, kind = Coercion from to class
} in
let merger_sort = fun name kind => { name = name, kind = Merger kind } in
let param = fun name sort => { name = name, sort = sort } in

# ═══════════════════════════════════════════════════════════════════
# Operation combinators
# ═══════════════════════════════════════════════════════════════════

let unary = fun name input output => {
  name = name, input = input, output = output,
} in

let binary = fun name in1 in2 output => {
  name = name,
  inputs = [param in1.0 in1.1, param in2.0 in2.1],
  output = output,
} in

let nullary = fun name output => {
  name = name, inputs = [], output = output,
} in

let nary = fun name inputs output => {
  name = name, inputs = inputs, output = output,
} in

# ═══════════════════════════════════════════════════════════════════
# Equation combinators
# ═══════════════════════════════════════════════════════════════════

let eq = fun name lhs rhs => {
  name = name, lhs = lhs, rhs = rhs,
} in

let directed_eq = fun name lhs rhs impl_expr => {
  name = name, lhs = lhs, rhs = rhs, impl_expr = impl_expr,
} in

let directed_eq_invertible = fun name lhs rhs impl_expr inverse => {
  name = name, lhs = lhs, rhs = rhs,
  impl_expr = impl_expr, inverse = inverse,
} in

# ═══════════════════════════════════════════════════════════════════
# Policy combinators
# ═══════════════════════════════════════════════════════════════════

let keep_left = fun name value_kind => {
  name = name, value_kind = value_kind,
  strategy = { type = "keep_left" },
} in

let keep_right = fun name value_kind => {
  name = name, value_kind = value_kind,
  strategy = { type = "keep_right" },
} in

let fail_on_conflict = fun name value_kind => {
  name = name, value_kind = value_kind,
  strategy = { type = "fail" },
} in

let custom_policy = fun name value_kind expr => {
  name = name, value_kind = value_kind,
  strategy = { type = "custom", expr = expr },
} in

# ═══════════════════════════════════════════════════════════════════
# Composition combinators
# ═══════════════════════════════════════════════════════════════════

let colimit = fun left right shared_sorts => {
  left = left, right = right,
  shared_sorts = shared_sorts, shared_ops = [],
} in

let colimit_with_ops = fun left right shared_sorts shared_ops => {
  left = left, right = right,
  shared_sorts = shared_sorts, shared_ops = shared_ops,
} in

# ═══════════════════════════════════════════════════════════════════
# Edge rule combinators
# ═══════════════════════════════════════════════════════════════════

let edge_rule = fun edge_kind src_kind tgt_kind => {
  edge_kind = edge_kind, src_kind = src_kind, tgt_kind = tgt_kind,
} in

# ═══════════════════════════════════════════════════════════════════
# Exports
# ═══════════════════════════════════════════════════════════════════

{
  # Sort kind constructors
  Structural,
  Val,
  Coercion,
  Merger,

  # Contracts
  ParamSpec,
  SortKindSpec,
  SortSpec,
  OpSpec,
  EquationSpec,
  DirectedEqSpec,
  StrategySpec,
  PolicySpec,
  EdgeRuleSpec,
  ColimitStepSpec,

  # Document contracts
  Theory,
  Morphism,
  Composition,
  Protocol,
  Bundle,

  # Sort combinators
  simple,
  dependent,
  val_sort,
  coercion_sort,
  merger_sort,
  param,

  # Operation combinators
  unary,
  binary,
  nullary,
  nary,

  # Equation combinators
  eq,
  directed_eq,
  directed_eq_invertible,

  # Policy combinators
  keep_left,
  keep_right,
  fail_on_conflict,
  custom_policy,

  # Composition combinators
  colimit,
  colimit_with_ops,

  # Edge rule combinators
  edge_rule,
}
