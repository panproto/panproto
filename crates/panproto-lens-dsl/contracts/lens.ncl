# panproto lens DSL: Nickel contract library
#
# Import this from your .ncl lens files:
#   let L = import "panproto/lens.ncl" in
#   { ... } | L.Lens

# -- Coercion classification --
# Use as: coercion = L.iso, coercion = L.projection, etc.
let iso = "iso" in
let retraction = "retraction" in
let projection = "projection" in
let opaque = "opaque" in

let Coercion = std.contract.from_predicate (fun x =>
  x == "iso" || x == "retraction" || x == "projection" || x == "opaque"
) in

# -- Step contracts --

let RenameSpec = {
  old | String,
  new | String,
} in

let AddFieldSpec = {
  name | String,
  kind | String,
  default | optional,
  expr | String | optional,
} in

let ApplyExprSpec = {
  field | String,
  expr | String,
  inverse | String | optional,
  coercion | Coercion | optional,
} in

let ComputeFieldSpec = {
  target | String,
  expr | String,
  inverse | String | optional,
  coercion | Coercion | optional,
} in

let HoistSpec = {
  parent | String,
  intermediate | String,
  child | String,
} in

let NestSpec = {
  parent | String,
  child | String,
  intermediate | String,
  intermediate_kind | String,
  edge_kind | String,
} in

let PullbackSpec = {
  name | String,
  domain | String,
  codomain | String,
  sort_map | { _ : String } | default = {},
  op_map | { _ : String } | default = {},
} in

let CoerceSortSpec = {
  sort | String,
  target_kind | String,
  expr | String,
  inverse | String | optional,
  coercion | Coercion,
} in

let MergeSortsSpec = {
  sort_a | String,
  sort_b | String,
  merged | String,
  expr | String,
} in

let AddSortSpec = {
  name | String,
  kind | String,
  default | optional,
} in

let AddOpSpec = {
  name | String,
  src | String,
  tgt | String,
  kind | String,
} in

let EquationSpec = {
  name | String,
  lhs | String,
  rhs | String,
} in

# A step is a record with exactly one of these keys set.
# We use an open contract: each field is optional, and users
# provide exactly one per step object.
let Step = {
  remove_field | String | optional,
  rename_field | RenameSpec | optional,
  add_field | AddFieldSpec | optional,
  apply_expr | ApplyExprSpec | optional,
  compute_field | ComputeFieldSpec | optional,
  hoist_field | HoistSpec | optional,
  nest_field | NestSpec | optional,
  scoped | { focus | String, inner | Array Dyn } | optional,
  pullback | PullbackSpec | optional,
  coerce_sort | CoerceSortSpec | optional,
  merge_sorts | MergeSortsSpec | optional,
  add_sort | AddSortSpec | optional,
  drop_sort | String | optional,
  rename_sort | RenameSpec | optional,
  add_op | AddOpSpec | optional,
  drop_op | String | optional,
  rename_op | RenameSpec | optional,
  add_equation | EquationSpec | optional,
  drop_equation | String | optional,
} in

# -- Rule contracts --

let FeaturePattern = {
  name | String | optional,
  type_id | String | optional,
} in

let Replacement = {
  name | optional,
  rename_attrs | { _ : String } | optional,
  add_attrs | { _ : Dyn } | optional,
  drop_attrs | Array String | optional,
  keep_attrs | Array String | optional,
  map_attr_value | { _ : Dyn } | optional,
} in

let Rule = {
  match | FeaturePattern,
  replace | Replacement | optional,
} in

# -- Compose contracts --

let ComposeMode = std.contract.from_predicate (fun x =>
  x == "vertical" || x == "horizontal"
) in

let LensRef =
  std.contract.from_predicate (fun x =>
    std.is_record x
    && (std.record.has_field "ref" x || std.record.has_field "inline" x)
  )
in

let ComposeSpec = {
  mode | ComposeMode,
  lenses | Array LensRef,
} in

# -- Auto contracts --

let AutoSpec = {
  quality_threshold | Number | optional,
  enable_overlap | Bool | optional,
  max_search_depth | Number | optional,
} in

# -- Passthrough --

let Passthrough = std.contract.from_predicate (fun x =>
  x == "keep" || x == "drop"
) in

# -- The lens document contract --

let Lens = {
  id | String,
  description | String | optional,
  source | String,
  target | String,

  steps | Array Step | optional,
  rules | Array Rule | optional,
  compose | ComposeSpec | optional,
  auto | AutoSpec | optional,

  passthrough | Passthrough | optional,
  invertible | Bool | optional,

  extensions | { _ : Dyn } | optional,
} in

# =====================================================================
# Combinator functions
# =====================================================================

# Remove a field (drop a sort).
let remove = fun field => { remove_field = field } in

# Rename a field's JSON property key.
let rename = fun old new => { rename_field = { old = old, new = new } } in

# Add a field with a default value.
let add = fun name kind default => {
  add_field = { name = name, kind = kind, default = default }
} in

# Add a field with a computed expression.
let add_computed = fun name kind default expr => {
  add_field = { name = name, kind = kind, default = default, expr = expr }
} in

# Apply an expression to an existing field.
let apply = fun field expr => {
  apply_expr = { field = field, expr = expr }
} in

# Apply an expression with inverse for round-tripping.
let apply_invertible = fun field expr inverse => {
  apply_expr = { field = field, expr = expr, inverse = inverse, coercion = iso }
} in

# Compute a new field from an expression.
let compute = fun target expr => {
  compute_field = { target = target, expr = expr }
} in

# Compute a new field with an inverse expression.
let compute_invertible = fun target expr inverse => {
  compute_field = { target = target, expr = expr, inverse = inverse, coercion = iso }
} in

# Hoist a nested field up one level.
let hoist = fun parent intermediate child => {
  hoist_field = { parent = parent, intermediate = intermediate, child = child }
} in

# Nest a direct child under a new intermediate vertex.
let nest = fun parent child intermediate intermediate_kind edge_kind => {
  nest_field = {
    parent = parent,
    child = child,
    intermediate = intermediate,
    intermediate_kind = intermediate_kind,
    edge_kind = edge_kind,
  }
} in

# Apply inner steps to each element of an array.
let map_items = fun focus inner => {
  scoped = { focus = focus, inner = inner }
} in

# Pullback along a theory morphism.
let pullback = fun name domain codomain sort_map op_map => {
  pullback = {
    name = name,
    domain = domain,
    codomain = codomain,
    sort_map = sort_map,
    op_map = op_map,
  }
} in

# Coerce a sort's value kind.
let coerce = fun sort target_kind expr coercion => {
  coerce_sort = {
    sort = sort,
    target_kind = target_kind,
    expr = expr,
    coercion = coercion,
  }
} in

# Coerce with an inverse.
let coerce_invertible = fun sort target_kind expr inverse => {
  coerce_sort = {
    sort = sort,
    target_kind = target_kind,
    expr = expr,
    inverse = inverse,
    coercion = iso,
  }
} in

# Merge two sorts into one.
let merge = fun sort_a sort_b merged expr => {
  merge_sorts = {
    sort_a = sort_a,
    sort_b = sort_b,
    merged = merged,
    expr = expr,
  }
} in

# Add a sort to the theory.
let add_sort = fun name kind default => {
  add_sort = { name = name, kind = kind, default = default }
} in

# Drop a sort from the theory.
let drop_sort = fun name => { drop_sort = name } in

# Rename a sort.
let rename_sort = fun old new => { rename_sort = { old = old, new = new } } in

# Add an operation to the theory.
let add_op = fun name src tgt kind => {
  add_op = { name = name, src = src, tgt = tgt, kind = kind }
} in

# Drop an operation from the theory.
let drop_op = fun name => { drop_op = name } in

# Rename an operation.
let rename_op = fun old new => { rename_op = { old = old, new = new } } in

# Add an equation.
let add_equation = fun name lhs rhs => {
  add_equation = { name = name, lhs = lhs, rhs = rhs }
} in

# Drop an equation.
let drop_equation = fun name => { drop_equation = name } in

# -- Template helpers --

# Generate counter fields (integer fields with default 0).
let counter_fields = fun fields =>
  fields |> std.array.map (fun f => add f "integer" 0)
in

# Generate string fields with empty defaults.
let string_fields = fun fields =>
  fields |> std.array.map (fun f => add f "string" "")
in

# Build a rule mapping one name to another.
let map_name = fun from to => {
  match = { name = from },
  replace = { name = to },
} in

# Build a rule that drops a matched feature.
let drop_feature = fun name => {
  match = { name = name },
} in

# =====================================================================
# Exports
# =====================================================================

{
  # Contracts
  Lens,
  Step,
  Rule,
  FeaturePattern,
  Replacement,
  ComposeSpec,
  AutoSpec,
  Coercion,

  # Coercion values
  iso,
  retraction,
  projection,
  opaque,

  # Step combinators
  remove,
  rename,
  add,
  add_computed,
  apply,
  apply_invertible,
  compute,
  compute_invertible,
  hoist,
  nest,
  map_items,
  pullback,
  coerce,
  coerce_invertible,
  merge,
  add_sort,
  drop_sort,
  rename_sort,
  add_op,
  drop_op,
  rename_op,
  add_equation,
  drop_equation,

  # Template helpers
  counter_fields,
  string_fields,
  map_name,
  drop_feature,
}
