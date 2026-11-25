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
/// so far beside `fields`. A rewrite without an inverse passes the value on.
/// Nothing when an inverse does not evaluate.
fn carried_back(
    rewrites: &[&FieldTransform],
    key: &str,
    shown: &Value,
    mut fields: HashMap<String, Value>,
) -> Option<Value> {
    let mut value = shown.clone();
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
        fields.insert(key.to_owned(), value);
        let env = panproto_inst::build_env_from_extra_fields(&fields);
        let result =
            panproto_expr::eval(inverse, &env, &panproto_expr::EvalConfig::default()).ok()?;
        value = panproto_inst::expr_literal_to_value(&result);
    }
    Some(value)
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
pub(crate) fn record(
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
                carried_back(rewrites, &field.key, &shown, beside)
                    .is_some_and(|value| same(&value, &shown))
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
pub(crate) fn held_as(lens: &Lens, view: &WInstance, again: &WInstance) -> WInstance {
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
            _ => {}
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
    /// The child as the rule for its own transforms restores it.
    restored: Content,
}

/// A source put tries, with what get makes of its coordinates.
struct Attempt<'p> {
    source: WInstance,
    /// The run of the transforms of each view node in the baseline.
    traces: HashMap<u32, asymmetric::TransformTrace>,
    outcomes: Vec<Outcome<'p>>,
    /// The first refusal of a transform, if there is one.
    error: Option<RestrictError>,
}

/// An attempt with the contents its coordinates hold.
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
        restored,
    }
}

/// The search for the source of a view.
pub(crate) struct Search<'a> {
    lens: &'a Lens,
    view: &'a WInstance,
    complement: &'a Complement,
    plan: &'a RestrictPlan,
    /// The arcs of the view.
    arcs: Arcs<'a>,
    /// The source with its recorded fields, each coordinate's child as the
    /// view holds it.
    structure: WInstance,
    coordinates: Vec<Coordinate>,
}

impl<'a> Search<'a> {
    /// The search for the source of `view`.
    ///
    /// # Errors
    ///
    /// A structural refusal of the restoration.
    pub(crate) fn new(
        lens: &'a Lens,
        view: &'a WInstance,
        complement: &'a Complement,
        plan: &'a RestrictPlan,
    ) -> Result<Self, LensError> {
        let structure = asymmetric::restored_structure(lens, view, complement)?;
        let arcs = Arcs::new(view);
        let coordinates = coordinates(view, complement, plan, &structure, &arcs);
        Ok(Self {
            lens,
            view,
            complement,
            plan,
            arcs,
            structure,
            coordinates,
        })
    }

    /// Try the source in which the coordinates hold `contents`.
    fn attempt(&self, contents: &[Content]) -> Attempt<'a> {
        let mut baseline = self.structure.clone();
        self.place(contents, &mut baseline);
        let mut source = baseline.clone();
        let mut traces = HashMap::new();
        let mut error = None;
        let runs = TreeRuns::new(&baseline, self.plan);
        let mut ids: Vec<u32> = self.view.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if self.complement.synthesized_nodes.contains(&id) {
                continue;
            }
            let run = runs.run(id);
            error = error.or(run.error);
            let trace = asymmetric::TransformTrace {
                operations: run.operations,
                fields: run.fields,
                value: run.value,
            };
            if let Some(node) = source.nodes.get_mut(&id) {
                asymmetric::restore_traced_fields(
                    node,
                    &self.view.nodes[&id],
                    &trace,
                    self.complement.original_extra_fields.contains_key(&id)
                        || self.complement.identity.is_some(),
                );
            }
            traces.insert(id, trace);
        }
        let children: BTreeSet<u32> = self.coordinates.iter().map(|c| c.child).collect();
        asymmetric::invert_computations_on_child_scalars(
            self.view,
            &traces,
            &children,
            &mut source.nodes,
        );
        let restored: Vec<Content> = self
            .coordinates
            .iter()
            .map(|coordinate| Content::of(&source.nodes[&coordinate.child]))
            .collect();
        self.place(contents, &mut source);
        let mut outcomes = Vec::with_capacity(self.coordinates.len());
        let runs = TreeRuns::new(&source, self.plan);
        let mut of_parent: Option<(u32, NodeRun<'a>)> = None;
        for (coordinate, restored) in self.coordinates.iter().zip(restored) {
            let run = match of_parent.take() {
                Some((parent, run)) if parent == coordinate.parent => run,
                _ => runs.run(coordinate.parent),
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

    /// Place `contents` on the children of the coordinates in `tree`.
    fn place(&self, contents: &[Content], tree: &mut WInstance) {
        for (coordinate, content) in self.coordinates.iter().zip(contents) {
            if let Some(node) = tree.nodes.get_mut(&coordinate.child) {
                content.put_on(node);
            }
        }
    }

    /// Whether get of the tried source shows what the view shows at
    /// coordinate `at`.
    ///
    /// A field left among the parent's fields does not determine the child
    /// it shadows when the child is the source's, nothing of it is recorded,
    /// and the field was not made of the child's value or the child holds a
    /// record as its fields, which may have had a discriminator besides.
    fn reproduced(&self, at: usize, tried: &Tried<'_>) -> bool {
        let coordinate = &self.coordinates[at];
        let outcome = &tried.attempt.outcomes[at];
        let undetermined = outcome.on_parent
            && coordinate.held
            && coordinate.recorded.is_none()
            && (outcome.rewrites.is_empty() || tried.contents[at].value.is_none());
        !undetermined
            && outcome.value.as_ref().is_some_and(|value| {
                same(value, &coordinate.shown)
                    || (!tried.exactly && asymmetric::value_equiv(value, &coordinate.shown))
            })
    }

    /// The first candidate for coordinate `at` after `attempt`: the value
    /// the view shows, carried back through the inverses of the rewrites
    /// that ran, and that value itself where an inverse does not evaluate.
    fn first_candidate(&self, at: usize, attempt: &Attempt<'_>) -> Content {
        let coordinate = &self.coordinates[at];
        let outcome = &attempt.outcomes[at];
        if coordinate.own && !outcome.written {
            return outcome.restored.clone();
        }
        if outcome.rewrites.is_empty() {
            return coordinate.on_child.clone();
        }
        let beside = shown_fields(self.view, &self.arcs, coordinate.parent);
        let carried = carried_back(
            &outcome.rewrites,
            &coordinate.key,
            &coordinate.shown,
            beside,
        );
        match carried {
            Some(value) if !same(&value, &coordinate.shown) => {
                let fielded = coordinate
                    .recorded
                    .as_ref()
                    .map_or(coordinate.on_child.value.is_none(), |recorded| {
                        recorded.value.is_none()
                    });
                Content::presenting(value, fielded)
            }
            _ => coordinate.on_child.clone(),
        }
    }

    /// The put-domain violations of `attempt`, in discovery order.
    fn violations(&self, tried: &Tried<'a>) -> Vec<PutDomainError> {
        let mut replay = Replay::default();
        for (at, coordinate) in self.coordinates.iter().enumerate() {
            let reproduced = self.reproduced(at, tried);
            if coordinate.collapses {
                replay
                    .on_children
                    .entry(coordinate.parent)
                    .or_default()
                    .insert(coordinate.key.clone(), reproduced);
            }
            if !reproduced && !tried.attempt.outcomes[at].on_parent {
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

    /// The coordinates get of the tried source does not reproduce.
    fn missed(&self, tried: &Tried<'a>) -> BTreeSet<usize> {
        (0..self.coordinates.len())
            .filter(|&at| !self.reproduced(at, tried))
            .collect()
    }

    /// The source of the view.
    ///
    /// The first source tried holds each coordinate at its recorded content,
    /// or at what the view shows where there is no record. A coordinate is
    /// edited when it has no record or get of that source does not reproduce
    /// it. With none edited, that source is the only one tried.
    ///
    /// Otherwise every edited coordinate takes its first candidate. Then, in
    /// rounds, each edited coordinate that get of the last source does not
    /// reproduce takes its first candidate under the rewrites that ran in
    /// that round, or the shown value when it holds that candidate already;
    /// when get reproduces them all and the source is refused for another
    /// reason, they all do. A search ends with the first source admitted,
    /// when a round repeats one of the last two sources, or after two rounds
    /// for each edited coordinate and two more. Coordinates that a tried
    /// source did not reproduce and that were not counted as edited are then
    /// counted so, and the search starts again, until there are none.
    ///
    /// # Errors
    ///
    /// The violations of the first source in which the edited coordinates
    /// hold their first candidates, or of the first source tried where the
    /// transforms do not evaluate on that one.
    fn source(self) -> Result<WInstance, Vec<PutDomainError>> {
        let contents: Vec<Content> = self
            .coordinates
            .iter()
            .map(|c| c.recorded.as_ref().unwrap_or(&c.on_child).clone())
            .collect();
        let mut base = Tried {
            attempt: self.attempt(&contents),
            contents,
            exactly: true,
        };
        if let Some(error) = base.attempt.error.take() {
            return Err(asymmetric::put_refusal_from_restrict(
                self.lens, self.view, error,
            ));
        }
        let missed = self.missed(&base);
        let mut edited: BTreeSet<usize> = (0..self.coordinates.len())
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
        let mut refusal = None;
        loop {
            let mut others = BTreeSet::new();
            match self.search(&base, &edited, &mut others, &mut refusal) {
                Some(source) => return Ok(source),
                None if others.is_empty() => break,
                None => edited.extend(others),
            }
        }
        Err(refusal.unwrap_or_else(|| self.violations(&base)))
    }

    /// One search over the candidates of `edited`, from `base`: the source
    /// admitted, if there is one. `others` takes the coordinates outside
    /// `edited` that a tried source did not reproduce, and `refusal` the
    /// violations of the first source of the first search, where its
    /// transforms evaluate.
    fn search(
        &self,
        base: &Tried<'a>,
        edited: &BTreeSet<usize>,
        others: &mut BTreeSet<usize>,
        refusal: &mut Option<Vec<PutDomainError>>,
    ) -> Option<WInstance> {
        let first: Vec<Content> = self.contents(base, |at| {
            edited
                .contains(&at)
                .then(|| self.first_candidate(at, &base.attempt))
        });
        let mut last = self.tried(first);
        let mut before: Option<Vec<Content>> = None;
        for _ in 0..2 * edited.len() + 2 {
            let missed = self.missed(&last);
            if missed.is_empty() && last.attempt.error.is_none() {
                let found = self.violations(&last);
                if found.is_empty() {
                    return Some(last.attempt.source);
                }
                refusal.get_or_insert(found);
            } else if refusal.is_none() {
                let evaluates = last.attempt.error.is_none()
                    && last.attempt.outcomes.iter().all(|o| o.value.is_some());
                *refusal = Some(self.violations(if evaluates { &last } else { base }));
            }
            others.extend(missed.difference(edited));
            let mut moving: BTreeSet<usize> = missed.intersection(edited).copied().collect();
            if moving.is_empty() {
                moving.clone_from(edited);
            }
            let next = self.contents(&last, |at| {
                moving.contains(&at).then(|| {
                    let first = self.first_candidate(at, &last.attempt);
                    if last.contents[at].same(&first) {
                        self.coordinates[at].on_child.clone()
                    } else {
                        first
                    }
                })
            });
            let repeats = |contents: &[Content]| contents.iter().zip(&next).all(|(a, b)| a.same(b));
            if repeats(&last.contents) || before.as_deref().is_some_and(repeats) {
                return None;
            }
            before = Some(std::mem::take(&mut last.contents));
            last = self.tried(next);
        }
        None
    }

    /// The contents of `tried` with those `change` gives.
    fn contents(
        &self,
        tried: &Tried<'_>,
        change: impl Fn(usize) -> Option<Content>,
    ) -> Vec<Content> {
        tried
            .contents
            .iter()
            .enumerate()
            .map(|(at, content)| change(at).unwrap_or_else(|| content.clone()))
            .collect()
    }

    fn tried(&self, contents: Vec<Content>) -> Tried<'a> {
        Tried {
            attempt: self.attempt(&contents),
            contents,
            exactly: false,
        }
    }
}

/// What the replay of a tried source gives at the coordinates of a view.
#[derive(Default)]
pub(crate) struct Replay {
    /// For each parent, the keys a serialized view may hold on a child, and
    /// whether get shows there what the view shows.
    pub(crate) on_children: HashMap<u32, BTreeMap<String, bool>>,
    /// The children that show what get does not give them.
    pub(crate) refused: BTreeSet<u32>,
}

/// The source of `view`, or every reason its values have none.
///
/// # Errors
///
/// The violations, in discovery order.
pub(crate) fn solve(
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
