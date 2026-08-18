# panproto lens DSL: Nickel contract library
#
# Import this from your .ncl lens files:
#   let L = import "panproto/lens.ncl" in
#   { ... } | L.Lens

{
  # -- Coercion classification --

  iso = "iso",
  retraction = "retraction",
  projection = "projection",
  opaque = "opaque",

  Coercion = std.contract.from_predicate (fun x =>
    x == "iso" || x == "retraction" || x == "projection" || x == "opaque"
  ),

  # -- Step contracts --

  RenameSpec = {
    old | String,
    new | String,
  },

  AddFieldSpec = {
    name | String,
    kind | String,
    fallback | optional,
    expr | String | optional,
  },

  ApplyExprSpec = {
    field | String,
    expr | String,
    inverse | String | optional,
    coercion | Coercion | optional,
  },

  ComputeFieldSpec = {
    target | String,
    expr | String,
    inverse | String | optional,
    coercion | Coercion | optional,
  },

  HoistSpec = {
    parent | String,
    intermediate | String,
    child | String,
  },

  NestSpec = {
    parent | String,
    child | String,
    intermediate | String,
    intermediate_kind | String,
    edge_kind | String,
    old_edge_name | String | optional,
    parent_to_intermediate | String | optional,
    intermediate_to_child | String | optional,
  },

  PullbackSpec = {
    name | String,
    domain | String,
    codomain | String,
    sort_map | { _ : String } | optional,
    op_map | { _ : String } | optional,
  },

  CoerceSortSpec = {
    sort | String,
    source_kind | String | optional,
    target_kind | String,
    expr | String,
    inverse | String | optional,
    coercion | Coercion,
  },

  MergeSortsSpec = {
    sort_a | String,
    sort_b | String,
    merged | String,
    expr | String,
  },

  AddSortSpec = {
    name | String,
    kind | String,
    fallback | optional,
  },

  AddOpSpec = {
    name | String,
    src | String,
    tgt | String,
    kind | String,
  },

  EquationSpec = {
    name | String,
    lhs | String,
    rhs | String,
  },

  Step = {
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
  },

  # -- Rule contracts --

  FeaturePattern = {
    name | String | optional,
    type_id | String | optional,
  },

  Replacement = {
    name | optional,
    rename_attrs | { _ : String } | optional,
    add_attrs | { _ : Dyn } | optional,
    drop_attrs | Array String | optional,
    keep_attrs | Array String | optional,
    map_attr_value | { _ : Dyn } | optional,
  },

  Rule = {
    pattern | FeaturePattern,
    replace | Replacement | optional,
  },

  # -- Compose contracts --

  ComposeMode = std.contract.from_predicate (fun x =>
    x == "vertical" || x == "horizontal"
  ),

  LensRef =
    std.contract.from_predicate (fun x =>
      std.is_record x
      && (std.record.has_field "ref" x || std.record.has_field "inline" x)
    ),

  ComposeSpec = {
    mode | ComposeMode,
    lenses | Array LensRef,
  },

  # -- Auto contracts --

  PreferenceKind = std.contract.from_predicate (fun x =>
    x == "same_edge_name" || x == "similar_name" || x == "same_kind"
  ),

  PreferencePredicate = {
    kind | PreferenceKind,
    threshold | Number | optional,
  },

  ConstraintType = std.contract.from_predicate (fun x =>
    x == "scope" || x == "exclude_targets" || x == "exclude_sources" || x == "prefer"
  ),

  Constraint = {
    type | ConstraintType,
    under | String | optional,
    targets | String | optional,
    vertices | Array String | optional,
    predicate | PreferencePredicate | optional,
    weight | Number | optional,
  },

  HintSpec = {
    anchors | { _ : String } | optional,
    constraints | Array Constraint | optional,
  },

  AutoSpec = {
    quality_threshold | Number | optional,
    enable_overlap | Bool | optional,
    max_results | Number | optional,
    hints | HintSpec | optional,
  },

  # -- Passthrough --

  Passthrough = std.contract.from_predicate (fun x =>
    x == "keep" || x == "drop"
  ),

  # -- The lens document contract --

  Lens = {
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
  },

  # =====================================================================
  # Combinator functions
  #
  # Parameter names use a _ prefix to avoid colliding with the record
  # field names they populate (Nickel's record scope shadows enclosing
  # bindings when the names coincide, causing infinite recursion).
  # =====================================================================

  remove = fun _field => { remove_field = _field },

  rename = fun _old _new => { rename_field = { old = _old, new = _new } },

  add = fun _name _kind _fallback => {
    add_field = { name = _name, kind = _kind, fallback = _fallback }
  },

  add_computed = fun _name _kind _fallback _expr => {
    add_field = { name = _name, kind = _kind, fallback = _fallback, expr = _expr }
  },

  apply = fun _field _expr => {
    apply_expr = { field = _field, expr = _expr }
  },

  apply_invertible = fun _field _expr _inverse => {
    apply_expr = { field = _field, expr = _expr, inverse = _inverse, coercion = "iso" }
  },

  compute = fun _target _expr => {
    compute_field = { target = _target, expr = _expr }
  },

  compute_invertible = fun _target _expr _inverse => {
    compute_field = { target = _target, expr = _expr, inverse = _inverse, coercion = "iso" }
  },

  hoist = fun _parent _intermediate _child => {
    hoist_field = { parent = _parent, intermediate = _intermediate, child = _child }
  },

  nest = fun _parent _child _intermediate _intermediate_kind _edge_kind => {
    nest_field = {
      parent = _parent,
      child = _child,
      intermediate = _intermediate,
      intermediate_kind = _intermediate_kind,
      edge_kind = _edge_kind,
    }
  },

  map_items = fun _focus _inner => {
    scoped = { focus = _focus, inner = _inner }
  },

  pullback = fun _name _domain _codomain _sort_map _op_map => {
    pullback = {
      name = _name,
      domain = _domain,
      codomain = _codomain,
      sort_map = _sort_map,
      op_map = _op_map,
    }
  },

  coerce = fun _sort _target_kind _expr _coercion => {
    coerce_sort = {
      sort = _sort,
      target_kind = _target_kind,
      expr = _expr,
      coercion = _coercion,
    }
  },

  coerce_invertible = fun _sort _target_kind _expr _inverse => {
    coerce_sort = {
      sort = _sort,
      target_kind = _target_kind,
      expr = _expr,
      inverse = _inverse,
      coercion = "iso",
    }
  },

  merge = fun _sort_a _sort_b _merged _expr => {
    merge_sorts = {
      sort_a = _sort_a,
      sort_b = _sort_b,
      merged = _merged,
      expr = _expr,
    }
  },

  add_sort = fun _name _kind _fallback => {
    add_sort = { name = _name, kind = _kind, fallback = _fallback }
  },

  drop_sort = fun _name => { drop_sort = _name },

  rename_sort = fun _old _new => { rename_sort = { old = _old, new = _new } },

  add_op = fun _name _src _tgt _kind => {
    add_op = { name = _name, src = _src, tgt = _tgt, kind = _kind }
  },

  drop_op = fun _name => { drop_op = _name },

  rename_op = fun _old _new => { rename_op = { old = _old, new = _new } },

  add_equation = fun _name _lhs _rhs => {
    add_equation = { name = _name, lhs = _lhs, rhs = _rhs }
  },

  drop_equation = fun _name => { drop_equation = _name },

  # -- Hint combinators --

  anchor = fun _src _tgt => { "%{_src}" = _tgt },

  scope = fun _under _targets => {
    type = "scope",
    under = _under,
    targets = _targets,
  },

  exclude_targets = fun _vertices => {
    type = "exclude_targets",
    vertices = _vertices,
  },

  exclude_sources = fun _vertices => {
    type = "exclude_sources",
    vertices = _vertices,
  },

  prefer_same_edge_name = fun _weight => {
    type = "prefer",
    predicate = { kind = "same_edge_name" },
    weight = _weight,
  },

  prefer_similar_name = fun _threshold _weight => {
    type = "prefer",
    predicate = { kind = "similar_name", threshold = _threshold },
    weight = _weight,
  },

  # -- Template helpers --

  counter_fields = fun _fields =>
    _fields |> std.array.map (fun _f => { add_field = { name = _f, kind = "integer", fallback = 0 } }),

  string_fields = fun _fields =>
    _fields |> std.array.map (fun _f => { add_field = { name = _f, kind = "string", fallback = "" } }),

  map_name = fun _from _to => {
    pattern = { name = _from },
    replace = { name = _to },
  },

  drop_feature = fun _name => {
    pattern = { name = _name },
  },
}
