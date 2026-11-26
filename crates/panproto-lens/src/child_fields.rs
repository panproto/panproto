//! Fields of a tree node that a child of the node holds.
//!
//! A record read from JSON holds each field as a child of the record's node.
//! An `ApplyExpr` on such a field reads the child's value, and get leaves the
//! result on the child, or among the node's own fields when the child holds
//! no scalar. Either way the field is one **coordinate** of the node, shown
//! by the node's own field of that name when it has one and by the child
//! otherwise, and a serialized view, which presents both under one key, may
//! hold on the child what get left on the node.
//!
//! [`record`] says which source values of such children get records, and
//! [`solve`] how put finds the source value of each.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    ChildField, ChildFieldResult, Complement, FieldTransform, Node, NodeRun, PutDomainError,
    RestrictError, RestrictPlan, TreeRuns, ViewPosition, WInstance,
};
use panproto_schema::Edge;

use crate::Lens;
use crate::asymmetric;
use crate::error::LensError;

/// The column an edge binds its child under.
fn column(edge: &Edge) -> &str {
    edge.name.as_deref().unwrap_or(&edge.tgt)
}

/// The arcs of a tree, by parent and by child.
struct Arcs<'a> {
    below: HashMap<u32, Vec<(u32, &'a Edge)>>,
    into: HashMap<u32, (u32, &'a Edge)>,
}

impl<'a> Arcs<'a> {
    fn new(tree: &'a WInstance) -> Self {
        let mut below: HashMap<u32, Vec<(u32, &Edge)>> = HashMap::new();
        let mut into = HashMap::new();
        for (parent, child, edge) in &tree.arcs {
            below.entry(*parent).or_default().push((*child, edge));
            into.entry(*child).or_insert((*parent, edge));
        }
        Self { below, into }
    }

    fn below(&self, parent: u32) -> &[(u32, &'a Edge)] {
        self.below.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// Whether a serialization presents the field `key` of `parent` and the
    /// child `child` under one key.
    fn collapses(&self, parent: u32, key: &str, child: u32) -> bool {
        self.into
            .get(&child)
            .is_some_and(|(above, edge)| *above == parent && column(edge) == key)
    }
}

/// The value a childless node presents: its own value, null for one that is
/// null or absent, and the record (or empty list) of its fields otherwise.
fn presented(node: &Node) -> Value {
    match &node.value {
        Some(FieldPresence::Present(value)) => value.clone(),
        Some(FieldPresence::Null | FieldPresence::Absent) => Value::Null,
        None if node.is_list() => Value::List(Vec::new()),
        None => Value::Unknown(node.extra_fields.clone()),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

/// The values a view shows in the fields of `parent` that its children hold,
/// by name: those an inverse of a rewrite is evaluated under. A field that
/// shows null, or a child that has children, binds nothing.
fn shown_fields(view: &WInstance, arcs: &Arcs<'_>, parent: u32) -> HashMap<String, Value> {
    let own = view.nodes.get(&parent).map(|node| &node.extra_fields);
    let mut shown = HashMap::new();
    for (child, edge) in arcs.below(parent) {
        let Some(node) = view.nodes.get(child) else {
            continue;
        };
        let name = column(edge);
        let value = match own.and_then(|fields| fields.get(name)) {
            Some(value) => value.clone(),
            None if node.value.is_none()
                && (node.extra_fields.is_empty() || !arcs.below(*child).is_empty()) =>
            {
                continue;
            }
            None => presented(node),
        };
        if value != Value::Null {
            shown.insert(name.to_owned(), value);
        }
    }
    shown
}

/// `shown`, the value of `key`, carried back through the inverses of
/// `rewrites` in reverse order, each evaluated with `key` bound to the value
/// so far beside `fields`, with whether every inverse evaluated. A rewrite
/// without an inverse, or whose inverse does not evaluate, passes the value
/// on.
fn carried_back(
    rewrites: &[&FieldTransform],
    key: &str,
    shown: &Value,
    mut fields: HashMap<String, Value>,
) -> (Value, bool) {
    let mut value = shown.clone();
    let mut evaluated = true;
    for rewrite in rewrites.iter().rev() {
        let (FieldTransform::ApplyExpr {
            inverse: Some(inverse),
            ..
        }
        | FieldTransform::ComputeField {
            inverse: Some(inverse),
            ..
        }) = rewrite
        else {
            continue;
        };
        fields.insert(key.to_owned(), value.clone());
        let env = panproto_inst::build_env_from_extra_fields(&fields);
        match panproto_expr::eval(inverse, &env, &panproto_expr::EvalConfig::default()) {
            Ok(result) => value = panproto_inst::expr_literal_to_value(&result),
            Err(_) => evaluated = false,
        }
    }
    (value, evaluated)
}

/// Record the source content of each child of `fields` that put could not
/// give back without it: the source value, and the source fields of a child
/// that holds no value.
///
/// `fields` is what get's run of the transforms made of the fields an
/// `ApplyExpr` ran on. The content is recorded when
///
/// * the view's child differs from the source's;
/// * get leaves a value for the key among the parent's own fields, where a
///   serialized view loses the child; or
/// * the value the view shows, carried back through the inverses that ran,
///   is another value, or an inverse does not evaluate.
///
/// Values are compared by canonical bytes and presences exactly. In every
/// other case the value the view shows is the source value, and put's first
/// candidate for it is that value. A child with a discriminator and no value
/// is not recorded: the record of its fields does not determine it.
pub fn record(
    source: &WInstance,
    view: &WInstance,
    fields: &[ChildField<'_>],
    original_values: &mut HashMap<u32, Option<FieldPresence>>,
    original_extra_fields: &mut HashMap<u32, HashMap<String, Value>>,
) {
    if fields.is_empty() {
        return;
    }
    let arcs = Arcs::new(view);
    for field in fields {
        let (Some(before), Some(after)) =
            (source.nodes.get(&field.child), view.nodes.get(&field.child))
        else {
            continue;
        };
        if before.value.is_none() && before.discriminator.is_some() {
            continue;
        }
        let rewrites = match &field.result {
            ChildFieldResult::Written(_) => field.rewrites.as_slice(),
            ChildFieldResult::OnParent(_)
                if arcs.collapses(field.parent, &field.key, field.child) =>
            {
                &[]
            }
            ChildFieldResult::OnParent(_) | ChildFieldResult::Passed => &[],
        };
        let shadowed = matches!(field.result, ChildFieldResult::OnParent(_))
            && arcs.collapses(field.parent, &field.key, field.child);
        let determined = !shadowed
            && asymmetric::same_canonical_presence(before.value.as_ref(), after.value.as_ref())
            && asymmetric::same_canonical_fields(&before.extra_fields, &after.extra_fields)
            && (rewrites.is_empty() || {
                let shown = presented(after);
                let beside = shown_fields(view, &arcs, field.parent);
                let (value, evaluated) = carried_back(rewrites, &field.key, &shown, beside);
                evaluated && same(&value, &shown)
            });
        if determined {
            continue;
        }
        original_values.insert(field.child, before.value.clone());
        if before.value.is_none() {
            original_extra_fields.insert(field.child, before.extra_fields.clone());
        }
    }
}

/// `view` with each field that a rewrite of `lens` may change and that the
/// view holds on a child alone held where `again` holds it, when both show
/// the same value there.
///
/// A serialization presents a node's own field and the child of that name
/// under one key, so this changes no serialized view.
pub fn held_as(lens: &Lens, view: &WInstance, again: &WInstance) -> WInstance {
    let mut keys_of: HashMap<&Name, BTreeSet<String>> = HashMap::new();
    for (anchor, program) in &lens.compiled.field_transforms {
        let image = lens.compiled.vertex_remap.get(anchor).unwrap_or(anchor);
        rewritable_keys(program, keys_of.entry(image).or_default());
    }
    let arcs = Arcs::new(view);
    let mut held = view.clone();
    for (id, node) in &view.nodes {
        let (Some(keys), Some(again_node)) = (keys_of.get(&node.anchor), again.nodes.get(id))
        else {
            continue;
        };
        for key in keys {
            if node.extra_fields.contains_key(key) {
                continue;
            }
            let mut named = arcs
                .below(*id)
                .iter()
                .filter(|(_, edge)| column(edge) == key);
            let (Some((child, _)), None) = (named.next(), named.next()) else {
                continue;
            };
            let (Some(shown), Some(again_child)) = (view.nodes.get(child), again.nodes.get(child))
            else {
                continue;
            };
            let left = again_node.extra_fields.get(key);
            let value = left.map_or_else(|| presented(again_child), Clone::clone);
            if !arcs.below(*child).is_empty() || !asymmetric::value_equiv(&presented(shown), &value)
            {
                continue;
            }
            if let Some(node) = held.nodes.get_mut(child) {
                Content::of(again_child).put_on(node);
            }
            if let (Some(left), Some(node)) = (left, held.nodes.get_mut(id)) {
                node.extra_fields.insert(key.clone(), left.clone());
            }
        }
    }
    held
}

/// What a child holds in place of a value: put restores these and nothing
/// else of the node.
#[derive(Clone, Debug)]
struct Content {
    value: Option<FieldPresence>,
    fields: HashMap<String, Value>,
}

impl Content {
    fn of(node: &Node) -> Self {
        Self {
            value: node.value.clone(),
            fields: node.extra_fields.clone(),
        }
    }

    /// The content that presents `value` on a child that holds a record as
    /// its fields exactly when `fielded` says so.
    fn presenting(value: Value, fielded: bool) -> Self {
        match value {
            Value::Null => Self {
                value: Some(FieldPresence::Null),
                fields: HashMap::new(),
            },
            Value::Unknown(fields) if fielded => Self {
                value: None,
                fields,
            },
            value => Self {
                value: Some(FieldPresence::Present(value)),
                fields: HashMap::new(),
            },
        }
    }

    fn same(&self, other: &Self) -> bool {
        asymmetric::same_canonical_presence(self.value.as_ref(), other.value.as_ref())
            && asymmetric::same_canonical_fields(&self.fields, &other.fields)
    }

    fn put_on(&self, node: &mut Node) {
        node.value.clone_from(&self.value);
        node.extra_fields.clone_from(&self.fields);
    }
}

/// A field of a node that one child of the node holds in the view and that
/// the node's transforms may rewrite.
struct Coordinate {
    parent: u32,
    key: String,
    child: u32,
    /// Whether a serialization presents the parent's own field of that name
    /// and the child under one key.
    collapses: bool,
    /// Whether the child's own transforms write its value.
    own: bool,
    /// Whether the child is one the source held.
    held: bool,
    /// The value the view shows.
    shown: Value,
    /// The child's content in the view.
    on_child: Content,
    /// The recorded source content of a child the view has not moved.
    recorded: Option<Content>,
    /// How many candidates the coordinate can have.
    candidates_at_most: usize,
}

/// How many operations of `program`, in any branch, with an inverse write
/// `key`.
fn inverses_of(program: &[FieldTransform], key: &str) -> usize {
    program
        .iter()
        .map(|transform| match transform {
            FieldTransform::ApplyExpr {
                key: written,
                inverse: Some(_),
                ..
            }
            | FieldTransform::ComputeField {
                target_key: written,
                inverse: Some(_),
                ..
            } => usize::from(written == key),
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                inverses_of(std::slice::from_ref(inner), key)
            }
            FieldTransform::Case { branches } => branches
                .iter()
                .map(|branch| inverses_of(&branch.transforms, key))
                .sum(),
            FieldTransform::ApplyExpr { inverse: None, .. }
            | FieldTransform::ComputeField { inverse: None, .. }
            | FieldTransform::PathTransform { .. }
            | FieldTransform::RenameField { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::AddField { .. }
            | FieldTransform::KeepFields { .. }
            | FieldTransform::MapReferences { .. } => 0,
        })
        .sum()
}

/// The keys the `ApplyExpr` operations of `program` rewrite, in any branch.
fn rewritable_keys(program: &[FieldTransform], keys: &mut BTreeSet<String>) {
    for transform in program {
        match transform {
            FieldTransform::ApplyExpr { key, .. } if key != "__value__" => {
                keys.insert(key.clone());
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                rewritable_keys(std::slice::from_ref(inner), keys);
            }
            FieldTransform::Case { branches } => {
                for branch in branches {
                    rewritable_keys(&branch.transforms, keys);
                }
            }
            FieldTransform::ApplyExpr { .. }
            | FieldTransform::PathTransform { .. }
            | FieldTransform::RenameField { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::AddField { .. }
            | FieldTransform::KeepFields { .. }
            | FieldTransform::ComputeField { .. }
            | FieldTransform::MapReferences { .. } => {}
        }
    }
}

/// The recorded source content of `child`, when get recorded one and the
/// view still holds the child where get put it.
fn recorded_content(
    complement: &Complement,
    arcs: &Arcs<'_>,
    parent: u32,
    child: u32,
    node: &Node,
) -> Option<Content> {
    let value = complement.original_values.get(&child)?;
    if complement.original_parent.get(&child) != Some(&parent) {
        return None;
    }
    if let Some(position) = complement.view_positions.get(&child) {
        let (above, label) = arcs.into.get(&child)?;
        let unmoved = matches!(
            position,
            ViewPosition::Child { parent, edge, .. } if parent == above && edge == *label
        );
        if !unmoved {
            return None;
        }
    }
    let fields = match complement.original_extra_fields.get(&child) {
        Some(fields) => fields.clone(),
        None if value.is_some() && node.value.is_none() => HashMap::new(),
        None => node.extra_fields.clone(),
    };
    Some(Content {
        value: value.clone(),
        fields,
    })
}

/// The coordinates of `view`, whose source structure is `structure`, in
/// ascending parent and key.
///
/// A field that the view's parent holds itself is none: the child is then a
/// coordinate of its own, restored as the view shows it.
fn coordinates(
    view: &WInstance,
    complement: &Complement,
    plan: &RestrictPlan,
    structure: &WInstance,
    arcs: &Arcs<'_>,
) -> Vec<Coordinate> {
    let mut keys_of: HashMap<&Name, BTreeSet<String>> = HashMap::new();
    let mut parents: Vec<u32> = Vec::new();
    for (&id, node) in &structure.nodes {
        if !view.nodes.contains_key(&id) || complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let keys = keys_of.entry(&node.anchor).or_insert_with(|| {
            let mut keys = BTreeSet::new();
            rewritable_keys(plan.program(&node.anchor), &mut keys);
            keys
        });
        if !keys.is_empty() {
            parents.push(id);
        }
    }
    if parents.is_empty() {
        return Vec::new();
    }
    parents.sort_unstable();
    let source_arcs = Arcs::new(structure);
    let mut found = Vec::new();
    for parent in parents {
        let mut by_column: BTreeMap<&str, Option<u32>> = BTreeMap::new();
        for (child, edge) in source_arcs.below(parent) {
            if *child != parent {
                by_column
                    .entry(column(edge))
                    .and_modify(|only| *only = None)
                    .or_insert(Some(*child));
            }
        }
        for key in &keys_of[&structure.nodes[&parent].anchor] {
            let Some(Some(child)) = by_column.get(key.as_str()).copied() else {
                continue;
            };
            let Some(node) = view.nodes.get(&child) else {
                continue;
            };
            if complement.synthesized_nodes.contains(&child) || !arcs.below(child).is_empty() {
                continue;
            }
            let collapses = arcs.collapses(parent, key, child);
            if collapses && view.nodes[&parent].extra_fields.contains_key(key) {
                continue;
            }
            found.push(Coordinate {
                parent,
                key: key.clone(),
                child,
                collapses,
                own: structure.nodes.get(&child).is_some_and(|node| {
                    plan.program(&node.anchor)
                        .iter()
                        .any(asymmetric::transforms_node_value)
                }),
                held: complement.original_parent.contains_key(&child),
                shown: presented(node),
                on_child: Content::of(node),
                recorded: recorded_content(complement, arcs, parent, child, node),
                candidates_at_most: inverses_of(
                    plan.program(&structure.nodes[&parent].anchor),
                    key,
                ) + 2,
            });
        }
    }
    found
}

/// What get of a tried source makes of a coordinate.
struct Outcome<'p> {
    /// The value it shows, when its transforms evaluate.
    value: Option<Value>,
    /// Whether the value is written onto the child.
    written: bool,
    /// Whether the value is left among the parent's fields.
    on_parent: bool,
    /// The operations that made the value out of the child's.
    rewrites: Vec<&'p FieldTransform>,
    /// Every operation with an inverse that ran on the key.
    inverted: Vec<&'p FieldTransform>,
    /// The child as the rule for its own transforms restores it.
    restored: Content,
}

/// A source put tries for some of the coordinates, with what get makes of
/// them.
struct Attempt<'p> {
    source: WInstance,
    /// The run of the transforms of each view node in the baseline.
    traces: HashMap<u32, asymmetric::TransformTrace>,
    /// What get makes of each coordinate tried, in the order tried.
    outcomes: Vec<Outcome<'p>>,
    /// The first refusal of a transform, if there is one.
    error: Option<RestrictError>,
}

/// An attempt with the contents the coordinates tried hold, in the order
/// tried.
struct Tried<'p> {
    contents: Vec<Content>,
    attempt: Attempt<'p>,
    /// Whether a value is reproduced only by one of equal canonical bytes,
    /// as in the source that decides which coordinates are edited; by an
    /// equivalent one otherwise.
    exactly: bool,
}

/// What get's runs make of `coordinate`: `run` is the run of its parent's
/// transforms, one of `runs`, and `restored` the child as the rule for its
/// own transforms restores it.
fn outcome<'p>(
    coordinate: &Coordinate,
    run: &NodeRun<'p>,
    runs: &TreeRuns<'_, 'p>,
    child: &Node,
    restored: Content,
) -> Outcome<'p> {
    let field = run
        .child_fields
        .iter()
        .find(|field| field.key == coordinate.key && field.child == coordinate.child);
    let (value, written, rewrites) = match field {
        Some(ChildField {
            result: ChildFieldResult::Written(value),
            rewrites,
            ..
        }) => (Some(value.clone()), true, rewrites.clone()),
        Some(ChildField {
            result: ChildFieldResult::OnParent(value),
            rewrites,
            ..
        }) if coordinate.collapses => (Some(value.clone()), false, rewrites.clone()),
        _ => (
            run.shown
                .get(&coordinate.key)
                .filter(|_| coordinate.collapses)
                .cloned(),
            false,
            Vec::new(),
        ),
    };
    let on_parent = value.is_some() && !written;
    let evaluated = run.error.is_none() && !run.failing.contains(&coordinate.key);
    let value = match value {
        _ if !evaluated => None,
        Some(value) => Some(value),
        None => {
            let own = runs.run(coordinate.child);
            own.error.is_none().then(|| {
                let mut child = child.clone();
                child.value = own.value;
                child.extra_fields = own.shown;
                presented(&child)
            })
        }
    };
    Outcome {
        value,
        written,
        on_parent,
        rewrites,
        inverted: field
            .map(|field| field.inverted.clone())
            .unwrap_or_default(),
        restored,
    }
}

fn same_contents(a: &[Content], b: &[Content]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same(b))
}

/// The trees an attempt runs in: the source structure and the view, whole
/// or cut down to the subtree of one parent, with the coordinates they hold.
struct World<'w> {
    structure: Cow<'w, WInstance>,
    view: Cow<'w, WInstance>,
    /// The view nodes whose transforms an attempt runs, ascending.
    ids: Cow<'w, [u32]>,
    /// The children of the coordinates the world holds.
    children: Cow<'w, BTreeSet<u32>>,
    /// The coordinates the world holds, with the contents they hold where
    /// an attempt does not try them.
    held: Vec<(usize, Content)>,
}

/// What one search runs over: the trees, the coordinates of a part, the
/// first source tried, and which of the sources it tries it admits.
struct Scope<'s, 'a> {
    world: &'s World<'s>,
    part: &'s [usize],
    base: &'s Tried<'a>,
    admits: &'s dyn Fn(&Tried<'a>) -> bool,
}

/// Classes of slots, a slot being a field of an element, joined by the
/// operations that read or write them together.
#[derive(Default)]
struct Classes {
    slot: HashMap<(u32, String), usize>,
    above: Vec<usize>,
}

impl Classes {
    fn index(&mut self, element: u32, field: &str) -> usize {
        let key = (element, field.to_owned());
        if let Some(&at) = self.slot.get(&key) {
            return at;
        }
        let at = self.above.len();
        self.above.push(at);
        self.slot.insert(key, at);
        at
    }

    fn root(&mut self, mut at: usize) -> usize {
        while self.above[at] != at {
            self.above[at] = self.above[self.above[at]];
            at = self.above[at];
        }
        at
    }

    fn join(&mut self, slots: impl IntoIterator<Item = usize>) {
        let mut slots = slots.into_iter();
        let Some(first) = slots.next() else {
            return;
        };
        let first = self.root(first);
        for slot in slots {
            let slot = self.root(slot);
            self.above[slot] = first;
        }
    }
}

impl Content {
    /// A digest of the content, equal for contents that are the same up to
    /// the order a map of fields is visited in.
    fn digest(&self, into: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        format!("{:?}", self.value).hash(into);
        let mut fields: Vec<(&String, String)> = self
            .fields
            .iter()
            .map(|(key, value)| (key, format!("{value:?}")))
            .collect();
        fields.sort_unstable();
        fields.hash(into);
    }
}

/// A digest of `contents`, with which the sources a search has tried are
/// found again.
fn digest(contents: &[Content]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for content in contents {
        content.digest(&mut hasher);
    }
    hasher.finish()
}

/// The search for the source of a view.
pub struct Search<'a> {
    lens: &'a Lens,
    view: &'a WInstance,
    complement: &'a Complement,
    plan: &'a RestrictPlan,
    /// The arcs of the view.
    arcs: Arcs<'a>,
    /// The source with its recorded fields, each coordinate's child as the
    /// view holds it.
    structure: WInstance,
    /// The arcs of the source structure by parent, as indices.
    structure_below: HashMap<u32, Vec<usize>>,
    /// The names of each node's children in the source structure.
    columns: HashMap<u32, BTreeSet<String>>,
    coordinates: Vec<Coordinate>,
    /// The view nodes put restores, ascending.
    ids: Vec<u32>,
    /// The children of the coordinates.
    children: BTreeSet<u32>,
    /// How many node runs the attempts have made.
    #[cfg(test)]
    runs: std::cell::Cell<usize>,
}

impl<'a> Search<'a> {
    /// The search for the source of `view`.
    ///
    /// # Errors
    ///
    /// A structural refusal of the restoration.
    pub fn new(
        lens: &'a Lens,
        view: &'a WInstance,
        complement: &'a Complement,
        plan: &'a RestrictPlan,
    ) -> Result<Self, LensError> {
        let structure = asymmetric::restored_structure(lens, view, complement)?;
        let arcs = Arcs::new(view);
        let mut columns: HashMap<u32, BTreeSet<String>> = HashMap::new();
        let mut structure_below: HashMap<u32, Vec<usize>> = HashMap::new();
        for (at, (parent, _, edge)) in structure.arcs.iter().enumerate() {
            columns
                .entry(*parent)
                .or_default()
                .insert(column(edge).to_owned());
            structure_below.entry(*parent).or_default().push(at);
        }
        let coordinates = coordinates(view, complement, plan, &structure, &arcs);
        let mut ids: Vec<u32> = view
            .nodes
            .keys()
            .copied()
            .filter(|id| !complement.synthesized_nodes.contains(id))
            .collect();
        ids.sort_unstable();
        let children = coordinates.iter().map(|c| c.child).collect();
        Ok(Self {
            lens,
            view,
            complement,
            plan,
            arcs,
            structure,
            structure_below,
            columns,
            coordinates,
            ids,
            children,
            #[cfg(test)]
            runs: std::cell::Cell::new(0),
        })
    }

    /// The coordinates split into parts searched apart.
    ///
    /// A slot is a field of an element of the source structure, a
    /// coordinate's slot its key at its parent. The fields of an element are
    /// those it and its children hold and those its program may write. An
    /// operation of an element's program, or its survival predicate, joins
    /// the slots of the fields its names bind
    /// ([`panproto_inst::FieldFootprint::fields_read`]) with those it may write,
    /// where it reads anything; one that reads `self`, or removes unnamed
    /// fields depending on a field, joins every field of the element. A field
    /// read that a structural child holds joins every coordinate below that
    /// child, whose values the child's binding is made of. An operation that
    /// reads the instance through a graph builtin sees the fields of
    /// elements, not their values: it joins the coordinates whose children
    /// hold a record as fields, and those of parents holding fields of their
    /// own, which their restoration writes. Every coordinate is in one part
    /// where the source schema declares a uniqueness key, which compares
    /// elements. The parts are the classes of the coordinates' slots, each
    /// ascending, in the order of their first coordinates.
    fn partition(&self) -> Vec<Vec<usize>> {
        let count = self.coordinates.len();
        if count == 0 {
            return Vec::new();
        }
        let keyed = self.lens.src_schema.declarations.keys.values().any(|keys| {
            keys.iter().any(|key| {
                matches!(
                    key.role,
                    panproto_schema::KeyRole::Primary
                        | panproto_schema::KeyRole::Unique
                        | panproto_schema::KeyRole::Identifier
                )
            })
        });
        if keyed {
            return vec![(0..count).collect()];
        }
        let mut classes = Classes::default();
        let slots: Vec<usize> = self
            .coordinates
            .iter()
            .map(|coordinate| classes.index(coordinate.parent, &coordinate.key))
            .collect();
        let visible: Vec<usize> = (0..count)
            .filter(|&at| {
                let coordinate = &self.coordinates[at];
                let fielded = coordinate
                    .recorded
                    .as_ref()
                    .unwrap_or(&coordinate.on_child)
                    .value
                    .is_none();
                let holding = |tree: &WInstance| {
                    tree.nodes
                        .get(&coordinate.parent)
                        .is_some_and(|node| !node.extra_fields.is_empty())
                };
                fielded || holding(&self.structure) || holding(self.view)
            })
            .map(|at| slots[at])
            .collect();
        let mut ids: Vec<u32> = self.structure.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node = &self.structure.nodes[&id];
            let program = self.plan.program(&node.anchor);
            let predicate = self.lens.compiled.conditional_survival.get(&node.anchor);
            if program.is_empty() && predicate.is_none() {
                continue;
            }
            let mut columns: BTreeMap<String, Vec<u32>> = BTreeMap::new();
            for &at in self.structure_below.get(&id).map_or(&[][..], Vec::as_slice) {
                let (_, child, edge) = &self.structure.arcs[at];
                if *child != id {
                    columns
                        .entry(column(edge).to_owned())
                        .or_default()
                        .push(*child);
                }
            }
            let footprints: Vec<panproto_inst::FieldFootprint> = program
                .iter()
                .map(FieldTransform::footprint)
                .chain(predicate.map(panproto_inst::FieldFootprint::of_predicate))
                .collect();
            let written: BTreeSet<&String> = footprints.iter().flat_map(|f| &f.writes).collect();
            let present = |name: &str| {
                node.extra_fields.contains_key(name)
                    || columns.contains_key(name)
                    || written.contains(&name.to_owned())
            };
            for footprint in &footprints {
                let everything = footprint.reads_element
                    || (footprint.writes_others && !footprint.reads.is_empty());
                let mut fields: BTreeSet<String> = if everything {
                    node.extra_fields
                        .keys()
                        .chain(columns.keys())
                        .cloned()
                        .collect()
                } else {
                    footprint.fields_read(&present)
                };
                if fields.is_empty() && !footprint.reads_instance {
                    continue;
                }
                let read = fields.clone();
                fields.extend(footprint.writes.iter().cloned());
                let mut joined: Vec<usize> = fields
                    .iter()
                    .map(|field| classes.index(id, field))
                    .collect();
                for field in &read {
                    for &child in columns.get(field).map_or(&[][..], Vec::as_slice) {
                        if self.structure_below.contains_key(&child) {
                            joined.extend(self.coordinates_below(child, &slots));
                        }
                    }
                }
                if footprint.reads_instance {
                    joined.extend(visible.iter().copied());
                }
                classes.join(joined);
            }
        }
        let mut parts: Vec<Vec<usize>> = Vec::new();
        let mut part_of: HashMap<usize, usize> = HashMap::new();
        for (at, &slot) in slots.iter().enumerate() {
            let root = classes.root(slot);
            let part = *part_of.entry(root).or_insert_with(|| {
                parts.push(Vec::new());
                parts.len() - 1
            });
            parts[part].push(at);
        }
        parts
    }

    /// The slots of the coordinates whose children are below `node` in the
    /// source structure.
    fn coordinates_below(&self, node: u32, slots: &[usize]) -> Vec<usize> {
        let mut below = BTreeSet::from([node]);
        let mut pending = vec![node];
        while let Some(id) = pending.pop() {
            for &at in self.structure_below.get(&id).map_or(&[][..], Vec::as_slice) {
                let child = self.structure.arcs[at].1;
                if below.insert(child) {
                    pending.push(child);
                }
            }
        }
        self.coordinates
            .iter()
            .zip(slots)
            .filter(|(coordinate, _)| below.contains(&coordinate.child))
            .map(|(_, &slot)| slot)
            .collect()
    }

    /// The coordinates searched together, each part ascending.
    #[cfg(test)]
    fn parts(&self) -> Vec<Vec<usize>> {
        self.partition()
    }

    /// The whole source structure and view.
    fn whole(&self) -> World<'_> {
        World {
            structure: Cow::Borrowed(&self.structure),
            view: Cow::Borrowed(self.view),
            ids: Cow::Borrowed(&self.ids),
            children: Cow::Borrowed(&self.children),
            held: Vec::new(),
        }
    }

    /// The subtrees of `parent` in the source structure and the view, their
    /// coordinates holding `contents`: none where the two do not hold the
    /// same view nodes below `parent`.
    fn world_below(&self, parent: u32, contents: &[Content]) -> Option<World<'_>> {
        let mut structure_ids = BTreeSet::from([parent]);
        let mut structure_arcs = Vec::new();
        let mut pending = vec![parent];
        while let Some(id) = pending.pop() {
            for &at in self.structure_below.get(&id).map_or(&[][..], Vec::as_slice) {
                let arc = &self.structure.arcs[at];
                structure_arcs.push(arc.clone());
                if structure_ids.insert(arc.1) {
                    pending.push(arc.1);
                }
            }
        }
        let mut view_ids = BTreeSet::from([parent]);
        let mut view_arcs = Vec::new();
        let mut pending = vec![parent];
        while let Some(id) = pending.pop() {
            for (child, edge) in self.arcs.below(id) {
                view_arcs.push((id, *child, (*edge).clone()));
                if view_ids.insert(*child) {
                    pending.push(*child);
                }
            }
        }
        let restored = |id: &&u32| !self.complement.synthesized_nodes.contains(*id);
        let ids: Vec<u32> = view_ids.iter().filter(restored).copied().collect();
        let held_in_view = structure_ids
            .iter()
            .filter(restored)
            .filter(|id| self.view.nodes.contains_key(*id));
        if !held_in_view.eq(ids.iter()) {
            return None;
        }
        let nodes_of = |tree: &WInstance, ids: &BTreeSet<u32>| -> HashMap<u32, Node> {
            ids.iter()
                .filter_map(|id| tree.nodes.get(id).map(|node| (*id, node.clone())))
                .collect()
        };
        let mut structure = WInstance::new(
            nodes_of(&self.structure, &structure_ids),
            structure_arcs,
            Vec::new(),
            parent,
            self.structure.nodes.get(&parent)?.anchor.clone(),
        );
        let view = WInstance::new(
            nodes_of(self.view, &view_ids),
            view_arcs,
            Vec::new(),
            parent,
            self.view.nodes.get(&parent)?.anchor.clone(),
        );
        let held: Vec<(usize, Content)> = (0..self.coordinates.len())
            .filter(|&at| structure_ids.contains(&self.coordinates[at].child))
            .map(|at| (at, contents[at].clone()))
            .collect();
        for (at, content) in &held {
            if let Some(node) = structure.nodes.get_mut(&self.coordinates[*at].child) {
                content.put_on(node);
            }
        }
        let children = held
            .iter()
            .map(|(at, _)| self.coordinates[*at].child)
            .collect();
        Some(World {
            structure: Cow::Owned(structure),
            view: Cow::Owned(view),
            ids: Cow::Owned(ids),
            children: Cow::Owned(children),
            held,
        })
    }

    /// Try the source of `world` in which the coordinates of `part` hold
    /// `contents`.
    fn attempt(&self, world: &World<'_>, part: &[usize], contents: &[Content]) -> Attempt<'a> {
        let mut baseline = world.structure.as_ref().clone();
        self.place(part, contents, &mut baseline);
        let mut source = baseline.clone();
        let mut traces = HashMap::new();
        let mut error = None;
        let runs = TreeRuns::new(&baseline, self.plan);
        for &id in world.ids.iter() {
            let run = runs.run(id);
            #[cfg(test)]
            self.runs.set(self.runs.get() + 1);
            error = error.or(run.error);
            let trace = asymmetric::TransformTrace {
                operations: run.operations,
                fields: run.fields,
                value: run.value,
                columns: self.columns.get(&id).cloned().unwrap_or_default(),
            };
            if let Some(node) = source.nodes.get_mut(&id) {
                asymmetric::restore_traced_fields(
                    node,
                    &world.view.nodes[&id],
                    &trace,
                    self.complement.original_extra_fields.contains_key(&id)
                        || self.complement.identity.is_some(),
                );
            }
            traces.insert(id, trace);
        }
        asymmetric::invert_computations_on_child_scalars(
            &world.view,
            &traces,
            &world.children,
            &mut source.nodes,
        );
        let restored: Vec<Content> = part
            .iter()
            .map(|&at| Content::of(&source.nodes[&self.coordinates[at].child]))
            .collect();
        for (at, content) in &world.held {
            if let Some(node) = source.nodes.get_mut(&self.coordinates[*at].child) {
                content.put_on(node);
            }
        }
        self.place(part, contents, &mut source);
        let mut outcomes = Vec::with_capacity(part.len());
        let runs = TreeRuns::new(&source, self.plan);
        let mut of_parent: Option<(u32, NodeRun<'a>)> = None;
        for (&at, restored) in part.iter().zip(restored) {
            let coordinate = &self.coordinates[at];
            let run = match of_parent.take() {
                Some((parent, run)) if parent == coordinate.parent => run,
                _ => {
                    #[cfg(test)]
                    self.runs.set(self.runs.get() + 1);
                    runs.run(coordinate.parent)
                }
            };
            let child = &source.nodes[&coordinate.child];
            outcomes.push(outcome(coordinate, &run, &runs, child, restored));
            of_parent = Some((coordinate.parent, run));
        }
        drop(runs);
        Attempt {
            source,
            traces,
            outcomes,
            error,
        }
    }

    /// Place `contents` on the children of the coordinates of `part` in
    /// `tree`.
    fn place(&self, part: &[usize], contents: &[Content], tree: &mut WInstance) {
        for (&at, content) in part.iter().zip(contents) {
            if let Some(node) = tree.nodes.get_mut(&self.coordinates[at].child) {
                content.put_on(node);
            }
        }
    }

    /// Whether get of a tried source, holding `content` at coordinate `at`,
    /// shows there what the view shows, `outcome` being what it makes of
    /// the coordinate.
    ///
    /// A field left among the parent's fields does not determine the child
    /// it shadows when the child is the source's, nothing of it is recorded,
    /// and the field was not made of the child's value or the child holds a
    /// record as its fields, which may have had a discriminator besides.
    fn reproduced(
        &self,
        at: usize,
        outcome: &Outcome<'_>,
        content: &Content,
        exactly: bool,
    ) -> bool {
        let coordinate = &self.coordinates[at];
        let undetermined = outcome.on_parent
            && coordinate.held
            && coordinate.recorded.is_none()
            && (outcome.rewrites.is_empty() || content.value.is_none());
        !undetermined
            && outcome.value.as_ref().is_some_and(|value| {
                same(value, &coordinate.shown)
                    || (!exactly && asymmetric::value_equiv(value, &coordinate.shown))
            })
    }

    /// The candidates for coordinate `at` after an attempt that made
    /// `outcome` of it, in order:
    ///
    /// 1. the value the view shows, carried back through the inverses of the
    ///    rewrites that ran;
    /// 2. the content the view holds;
    /// 3. the value the view shows carried back through the inverse of one
    ///    operation that ran on the key alone, the last first.
    ///
    /// A child whose own transforms write its value, and whose value the
    /// parent did not write, takes the value their rule restores in place of
    /// the first. Candidates equal to an earlier one are left out.
    fn candidates(&self, at: usize, outcome: &Outcome<'_>) -> Vec<Content> {
        let coordinate = &self.coordinates[at];
        let mut candidates: Vec<Content> = Vec::new();
        let mut offer = |content: Content| {
            if !candidates.iter().any(|known| known.same(&content)) {
                candidates.push(content);
            }
        };
        if coordinate.own && !outcome.written {
            offer(outcome.restored.clone());
            offer(coordinate.on_child.clone());
            return candidates;
        }
        let fielded = coordinate.recorded.as_ref().map_or_else(
            || coordinate.on_child.value.is_none(),
            |recorded| recorded.value.is_none(),
        );
        let content = |value: Value| {
            if same(&value, &coordinate.shown) {
                coordinate.on_child.clone()
            } else {
                Content::presenting(value, fielded)
            }
        };
        let beside = shown_fields(self.view, &self.arcs, coordinate.parent);
        let (carried, _) = carried_back(
            &outcome.rewrites,
            &coordinate.key,
            &coordinate.shown,
            beside.clone(),
        );
        offer(content(carried));
        offer(coordinate.on_child.clone());
        for rewrite in outcome.inverted.iter().rev() {
            let (alone, evaluated) = carried_back(
                std::slice::from_ref(rewrite),
                &coordinate.key,
                &coordinate.shown,
                beside.clone(),
            );
            if evaluated {
                offer(content(alone));
            }
        }
        candidates
    }

    /// The candidate after `holding` among those of coordinate `at`: the
    /// first when it holds none of them or the last.
    fn next_candidate(&self, at: usize, outcome: &Outcome<'_>, holding: &Content) -> Content {
        let candidates = self.candidates(at, outcome);
        let next = candidates
            .iter()
            .position(|candidate| candidate.same(holding))
            .map_or(0, |held| held + 1);
        candidates
            .get(next)
            .or_else(|| candidates.first())
            .cloned()
            .unwrap_or_else(|| self.coordinates[at].on_child.clone())
    }

    /// The first candidate of each coordinate of `edited` after `base`,
    /// with the contents of `base` elsewhere in `part`.
    fn first_candidates(
        &self,
        part: &[usize],
        base: &Tried<'_>,
        edited: &BTreeSet<usize>,
    ) -> Vec<Content> {
        part.iter()
            .map(|&at| {
                if edited.contains(&at) {
                    self.candidates(at, &base.attempt.outcomes[at])[0].clone()
                } else {
                    base.contents[at].clone()
                }
            })
            .collect()
    }

    /// The put-domain violations of a source tried for every coordinate.
    fn violations(&self, tried: &Tried<'a>) -> Vec<PutDomainError> {
        let mut replay = Replay::default();
        for (at, coordinate) in self.coordinates.iter().enumerate() {
            let outcome = &tried.attempt.outcomes[at];
            let reproduced = self.reproduced(at, outcome, &tried.contents[at], tried.exactly);
            if coordinate.collapses {
                replay
                    .on_children
                    .entry(coordinate.parent)
                    .or_default()
                    .insert(coordinate.key.clone(), reproduced);
            }
            if !reproduced && !outcome.on_parent {
                replay.refused.insert(coordinate.child);
            }
        }
        asymmetric::value_violations(
            self.lens,
            self.view,
            self.complement,
            self.plan,
            &tried.attempt.source,
            &tried.attempt.traces,
            &replay,
        )
    }

    /// The coordinates of `part` that get of the tried source does not
    /// reproduce.
    fn missed(&self, part: &[usize], tried: &Tried<'a>) -> BTreeSet<usize> {
        part.iter()
            .zip(&tried.attempt.outcomes)
            .zip(&tried.contents)
            .filter(|((at, outcome), content)| {
                !self.reproduced(**at, outcome, content, tried.exactly)
            })
            .map(|((at, _), _)| *at)
            .collect()
    }

    /// The refusal of the view: the violations of the source in which the
    /// edited coordinates hold their first candidates, where its transforms
    /// evaluate, and of `base` otherwise.
    fn refusal(&self, base: &Tried<'a>, edited: &BTreeSet<usize>) -> Vec<PutDomainError> {
        let all: Vec<usize> = (0..self.coordinates.len()).collect();
        let first = self.tried(
            &self.whole(),
            &all,
            self.first_candidates(&all, base, edited),
        );
        let evaluates = first.attempt.error.is_none()
            && first.attempt.outcomes.iter().all(|o| o.value.is_some());
        let found = self.violations(if evaluates { &first } else { base });
        if !found.is_empty() {
            return found;
        }
        // Neither source names a violation of the clauses: name the first
        // coordinate the first of them does not reproduce.
        let missed = self.missed(&all, &first);
        let at = missed.first().copied().unwrap_or(0);
        vec![PutDomainError::TransformNotInvertible {
            elem: panproto_inst::ViewRef::Node(self.coordinates[at].child),
            field: self.coordinates[at].key.clone(),
        }]
    }

    /// The source of the view.
    ///
    /// The first source tried holds each coordinate at its recorded content,
    /// or at what the view shows where there is no record. A coordinate is
    /// edited when it has no record or get of that source does not reproduce
    /// it. With none edited, that source is the only one tried.
    ///
    /// Otherwise, with more than one part ([`Self::partition`]), each part
    /// with an edited coordinate is searched alone ([`Self::part_source`]),
    /// in the subtree of its parent where it has one parent, the other
    /// coordinates holding their first contents, and a source is accepted
    /// when the part's coordinates are reproduced. The source in which every
    /// part holds what its search accepted is tried whole, and returned when
    /// it is admitted. Where a part accepts nothing, where the parts cannot
    /// be cut apart, or where the whole source is not admitted, and with one
    /// part, the coordinates are searched together, and the view is refused
    /// when that search admits nothing. The parts decide no refusal.
    ///
    /// # Errors
    ///
    /// The violations of the first source in which the edited coordinates
    /// hold their first candidates, or of the first source tried where the
    /// transforms do not evaluate on that one.
    fn source(&self) -> Result<WInstance, Vec<PutDomainError>> {
        let all: Vec<usize> = (0..self.coordinates.len()).collect();
        let contents: Vec<Content> = self
            .coordinates
            .iter()
            .map(|c| c.recorded.as_ref().unwrap_or(&c.on_child).clone())
            .collect();
        let mut base = Tried {
            attempt: self.attempt(&self.whole(), &all, &contents),
            contents,
            exactly: true,
        };
        if let Some(error) = base.attempt.error.take() {
            return Err(asymmetric::put_refusal_from_restrict(
                self.lens, self.view, error,
            ));
        }
        let missed = self.missed(&all, &base);
        let edited: BTreeSet<usize> = all
            .iter()
            .copied()
            .filter(|at| self.coordinates[*at].recorded.is_none() || missed.contains(at))
            .collect();
        if edited.is_empty() {
            let found = self.violations(&base);
            return if found.is_empty() {
                Ok(base.attempt.source)
            } else {
                Err(found)
            };
        }
        let parts = self.partition();
        if parts.len() > 1
            && let Some(found) = self.source_by_parts(&parts, &base, &edited)
        {
            return Ok(found);
        }
        let admits = |tried: &Tried<'a>| self.violations(tried).is_empty();
        let whole = self.whole();
        let scope = Scope {
            world: &whole,
            part: &all,
            base: &base,
            admits: &admits,
        };
        match self.part_source(&scope, &edited) {
            Some(found) => Ok(found.attempt.source),
            None => Err(self.refusal(&base, &edited)),
        }
    }

    /// The source of the view found part by part and admitted whole, or
    /// none.
    fn source_by_parts(
        &self,
        parts: &[Vec<usize>],
        base: &Tried<'a>,
        edited: &BTreeSet<usize>,
    ) -> Option<WInstance> {
        let all: Vec<usize> = (0..self.coordinates.len()).collect();
        let whole = self.whole();
        let mut worlds: HashMap<u32, World<'_>> = HashMap::new();
        let mut contents = base.contents.clone();
        let accepts = |_: &Tried<'a>| true;
        for part in parts {
            let edited_here: BTreeSet<usize> = part
                .iter()
                .copied()
                .filter(|at| edited.contains(at))
                .collect();
            if edited_here.is_empty() {
                continue;
            }
            let parent = self.coordinates[part[0]].parent;
            let alone = part.iter().all(|&at| self.coordinates[at].parent == parent);
            let world = if alone {
                match worlds.entry(parent) {
                    std::collections::hash_map::Entry::Occupied(entry) => &*entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        &*entry.insert(self.world_below(parent, &base.contents)?)
                    }
                }
            } else {
                &whole
            };
            let scope = Scope {
                world,
                part,
                base,
                admits: &accepts,
            };
            let found = self.part_source(&scope, &edited_here)?;
            for (&at, content) in part.iter().zip(found.contents) {
                contents[at] = content;
            }
        }
        let tried = self.tried(&whole, &all, contents);
        let admitted = tried.attempt.error.is_none()
            && self.missed(&all, &tried).is_empty()
            && self.violations(&tried).is_empty();
        admitted.then_some(tried.attempt.source)
    }

    /// The source of the coordinates of the part of `scope` that it admits,
    /// found with the coordinates of `edited` counted as edited.
    ///
    /// Every edited coordinate takes its first candidate
    /// ([`Self::candidates`]). Then, in rounds, each edited coordinate that
    /// get of the last source does not reproduce takes its candidate under
    /// the rewrites that ran in that round after the one it holds (the first
    /// after the last); when get reproduces them all and the source is not
    /// admitted, they all do. A search ends with the first source admitted,
    /// when a round repeats one of the last two sources, or after
    /// `(m + 2) e + 2` sources, for `e` edited coordinates and at most `m`
    /// operations with an inverse writing one key. A search that admits
    /// nothing is followed by one that moves one coordinate a round.
    /// Coordinates that a tried source did not reproduce and that were not
    /// counted as edited are then counted so, and the searches start again,
    /// until there are none; then, once, with every coordinate of the part
    /// counted as edited.
    fn part_source(&self, scope: &Scope<'_, 'a>, edited: &BTreeSet<usize>) -> Option<Tried<'a>> {
        let mut edited = edited.clone();
        loop {
            let mut others = BTreeSet::new();
            for one_at_a_time in [false, true] {
                let found = self.search(scope, &edited, one_at_a_time, &mut others);
                if found.is_some() {
                    return found;
                }
            }
            if others.is_empty() {
                // Nothing was missed and nothing admitted: the coordinates
                // whose values the view shows as get gives them are tried
                // too, once.
                if edited.len() == scope.part.len() {
                    return None;
                }
                others.extend(scope.part.iter().copied());
            }
            edited.extend(others);
        }
    }

    /// One search over the candidates of `edited`, from the first source of
    /// `scope`: the source admitted, if there is one. With `one_at_a_time`, a
    /// round moves only the first coordinate that would move, in the order
    /// of the coordinates (ascending parent and key), and the search may take
    /// `e + 1` times as many rounds. The search ends when a round would try
    /// a source it has tried: the next source is a function of the last, so
    /// it would try only sources it has tried. `others` takes the
    /// coordinates outside `edited` that a tried source did not reproduce.
    fn search(
        &self,
        scope: &Scope<'_, 'a>,
        edited: &BTreeSet<usize>,
        one_at_a_time: bool,
        others: &mut BTreeSet<usize>,
    ) -> Option<Tried<'a>> {
        let Scope {
            world,
            part,
            base,
            admits,
        } = *scope;
        let mut last = self.tried(world, part, self.first_candidates(part, base, edited));
        let mut seen: HashMap<u64, Vec<Vec<Content>>> = HashMap::new();
        seen.entry(digest(&last.contents))
            .or_default()
            .push(last.contents.clone());
        let widest = edited
            .iter()
            .map(|&at| self.coordinates[at].candidates_at_most)
            .max()
            .unwrap_or(2);
        let rounds = widest * edited.len() * if one_at_a_time { edited.len() + 1 } else { 1 } + 2;
        for _ in 0..rounds {
            let missed = self.missed(part, &last);
            if missed.is_empty() && last.attempt.error.is_none() && admits(&last) {
                return Some(last);
            }
            others.extend(missed.difference(edited));
            let mut moving: BTreeSet<usize> = missed.intersection(edited).copied().collect();
            if moving.is_empty() {
                moving.clone_from(edited);
            }
            if one_at_a_time {
                moving = moving.into_iter().take(1).collect();
            }
            let next: Vec<Content> = part
                .iter()
                .enumerate()
                .map(|(index, &at)| {
                    if moving.contains(&at) {
                        self.next_candidate(
                            at,
                            &last.attempt.outcomes[index],
                            &last.contents[index],
                        )
                    } else {
                        last.contents[index].clone()
                    }
                })
                .collect();
            let tried_before = seen.entry(digest(&next)).or_default();
            if tried_before
                .iter()
                .any(|contents| same_contents(contents, &next))
            {
                return None;
            }
            tried_before.push(next.clone());
            last = self.tried(world, part, next);
        }
        None
    }

    fn tried(&self, world: &World<'_>, part: &[usize], contents: Vec<Content>) -> Tried<'a> {
        Tried {
            attempt: self.attempt(world, part, &contents),
            contents,
            exactly: false,
        }
    }
}

/// What the replay of a tried source gives at the coordinates of a view.
#[derive(Default)]
pub struct Replay {
    /// For each parent, the keys a serialized view may hold on a child, and
    /// whether get shows there what the view shows.
    pub on_children: HashMap<u32, BTreeMap<String, bool>>,
    /// The children that show what get does not give them.
    pub refused: BTreeSet<u32>,
}

/// The source of `view`, or every reason its values have none.
///
/// # Errors
///
/// The violations, in discovery order.
pub fn solve(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    plan: &RestrictPlan,
) -> Result<WInstance, Vec<PutDomainError>> {
    match Search::new(lens, view, complement, plan) {
        Ok(search) => search.source(),
        Err(LensError::PutDomain(error)) => Err(vec![*error]),
        Err(LensError::Restrict(error)) => {
            Err(asymmetric::put_refusal_from_restrict(lens, view, error))
        }
        Err(error) => Err(vec![PutDomainError::SurvivalPredicateRefused {
            elem: panproto_inst::ViewRef::Node(view.root),
            refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                message: error.to_string(),
            },
        }]),
    }
}

#[cfg(test)]
mod tests;
