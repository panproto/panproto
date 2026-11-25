//! Fields of a record that its children hold.
//!
//! A record holds a field either itself or through a child. A rewrite of a
//! field reads the field the record holds itself, else the child's value,
//! and get leaves its result on the child when the child holds a scalar and
//! among the record's own fields otherwise. The definitions here are those
//! of `L-lenses-5` for one record whose source holds no field of its own,
//! whose names no lens renames, whose children have no transforms of their
//! own, and whose program inverts computations only of fields it can also
//! rewrite. Expressions are functions, so no evaluator is modelled.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::value::{Presence, Value};

/// Values by name.
pub type Bindings = BTreeMap<String, Value>;

/// A partial function of the values it reads.
pub type Function = Rc<dyn Fn(&Bindings) -> Option<Value>>;

/// A partial predicate of the values it reads.
pub type Predicate = Rc<dyn Fn(&Bindings) -> Option<bool>>;

/// One operation of a record's program.
#[derive(Clone)]
pub enum Operation {
    /// Rewrite a field from its own value, the only value `to` reads.
    Rewrite {
        /// The field.
        key: String,
        /// The result.
        to: Function,
        /// The inverse, over the values the view shows.
        from: Option<Function>,
    },
    /// Compute a field from the record's values.
    Compute {
        /// The field.
        key: String,
        /// The result.
        to: Function,
        /// The inverse of a computation that reads its own field alone.
        from: Option<Function>,
    },
    /// Move a field the record holds itself.
    Rename {
        /// The field.
        from: String,
        /// Its new name.
        to: String,
    },
    /// Drop a field the record holds itself.
    Drop(String),
    /// Add a field the record does not hold itself.
    Add(String, Value),
    /// Drop every field the record holds itself but these.
    Keep(BTreeSet<String>),
    /// Run the operations of the first branch whose predicate holds.
    Case(Vec<(Predicate, Vec<Self>)>),
}

/// What a child holds: a value, or, having none, a record as its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// A value, null or nothing.
    Value(Presence),
    /// The fields of a record.
    Fields(Bindings),
}

impl Content {
    /// Definition: the value a child shows.
    #[must_use]
    pub fn shown(&self) -> Value {
        match self {
            Self::Value(Presence::Present(value)) => value.clone(),
            Self::Value(Presence::Null | Presence::Absent) => Value::Null,
            Self::Fields(fields) => Value::Unknown(fields.clone()),
        }
    }

    const fn is_scalar(&self) -> bool {
        matches!(self, Self::Value(Presence::Present(_)))
    }
}

/// A record: the fields it holds itself and those its children hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// The fields the record holds itself.
    pub own: Bindings,
    /// The fields its children hold.
    pub children: BTreeMap<String, Content>,
}

impl Record {
    /// Definition: the value the record shows for `key`: its own field, else
    /// its child's.
    #[must_use]
    pub fn shown(&self, key: &str) -> Option<Value> {
        self.own
            .get(key)
            .cloned()
            .or_else(|| self.children.get(key).map(Content::shown))
    }

    /// Definition: what a serialization of the record and a reader of it give
    /// back: a field the record holds itself and a child of that name are one
    /// field, which the child then holds, a null as null and a record as
    /// fields.
    #[must_use]
    pub fn serialized(&self) -> Self {
        let mut read = self.clone();
        for (key, content) in &mut read.children {
            let shown = read.own.remove(key).unwrap_or_else(|| content.shown());
            *content = match shown {
                Value::Null => Content::Value(Presence::Null),
                Value::Unknown(fields) => Content::Fields(fields),
                value => Content::Value(Presence::Present(value)),
            };
        }
        read
    }

    /// Definition: the fields two records show agree.
    #[must_use]
    pub fn presents(&self, other: &Self) -> bool {
        let keys = |record: &Self| -> BTreeSet<String> {
            record
                .own
                .keys()
                .chain(record.children.keys())
                .cloned()
                .collect()
        };
        keys(self) == keys(other)
            && keys(self)
                .iter()
                .all(|key| self.shown(key) == other.shown(key))
    }
}

/// How the value under a key was made of the value of the child it names.
#[derive(Clone, Default)]
struct Made {
    /// The operations, with their inverses.
    by: Vec<Option<Function>>,
    /// Whether the value is still the child's to hold.
    on_child: bool,
}

/// The state of a run: the record's own fields, how the values under its
/// keys were made of children's values, and the keys rewritten so far.
#[derive(Default)]
struct State {
    own: Bindings,
    made: BTreeMap<String, Made>,
    rewritten: BTreeSet<String>,
    /// The rewrites that did not evaluate, where the run passes over them.
    failing: Option<BTreeSet<String>>,
}

fn bindings(own: &Bindings, children: &BTreeMap<String, Content>) -> Bindings {
    let mut all: Bindings = children
        .iter()
        .map(|(key, content)| (key.clone(), content.shown()))
        .collect();
    all.extend(own.clone());
    all
}

/// Run `program` on a record whose children are `children`. `None` when an
/// operation does not evaluate, unless it is a rewrite of a value a child
/// holds and the run passes over it.
fn execute(
    program: &[Operation],
    children: &BTreeMap<String, Content>,
    state: &mut State,
) -> Option<()> {
    for operation in program {
        match operation {
            Operation::Rewrite { key, to, from } => {
                let held = state.own.get(key).cloned();
                let from_child = held.is_none();
                let Some(value) = held.or_else(|| children.get(key).map(Content::shown)) else {
                    continue;
                };
                let result = to(&BTreeMap::from([(key.clone(), value.clone())]));
                let result = match (result, &mut state.failing) {
                    (Some(result), _) => result,
                    (None, Some(failing)) if from_child || state.made.contains_key(key) => {
                        failing.insert(key.clone());
                        value
                    }
                    (None, _) => return None,
                };
                state.own.insert(key.clone(), result);
                state.rewritten.insert(key.clone());
                if from_child {
                    let made = Made {
                        by: vec![from.clone()],
                        on_child: true,
                    };
                    state.made.insert(key.clone(), made);
                } else if let Some(made) = state.made.get_mut(key) {
                    made.by.push(from.clone());
                }
            }
            Operation::Compute { key, to, from } => {
                let from_child = !state.own.contains_key(key) && children.contains_key(key);
                let result = to(&bindings(&state.own, children))?;
                state.own.insert(key.clone(), result);
                let mut made = state.made.remove(key).unwrap_or_default();
                if from.is_some() && (from_child || !made.by.is_empty()) {
                    made.by.push(from.clone());
                    made.on_child = false;
                    state.made.insert(key.clone(), made);
                }
            }
            Operation::Rename { from, to } => {
                if let Some(value) = state.own.remove(from) {
                    state.made.remove(from);
                    state.made.remove(to);
                    state.own.insert(to.clone(), value);
                }
            }
            Operation::Drop(key) => {
                state.own.remove(key);
                state.made.remove(key);
            }
            Operation::Add(key, value) => {
                if !state.own.contains_key(key) {
                    state.own.insert(key.clone(), value.clone());
                    state.made.remove(key);
                }
            }
            Operation::Keep(keys) => {
                state.own.retain(|key, _| keys.contains(key));
                state.made.retain(|key, _| keys.contains(key));
            }
            Operation::Case(branches) => {
                for (predicate, operations) in branches {
                    if predicate(&bindings(&state.own, children))? {
                        execute(operations, children, state)?;
                        break;
                    }
                }
            }
        }
    }
    Some(())
}

/// Where get leaves what a run makes of a child's field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Left {
    OnChild,
    OnRecord,
    Nowhere,
}

/// What a run makes of a child's field.
struct Outcome {
    left: Left,
    /// The value get shows, when the rewrites evaluate.
    value: Option<Value>,
    /// The operations that made it of the child's value.
    by: Vec<Option<Function>>,
}

/// The view of a run that has ended, and what it made of each child's field.
fn finish(
    children: &BTreeMap<String, Content>,
    state: &State,
    evaluated: bool,
) -> (Record, BTreeMap<String, Outcome>) {
    let mut view = Record {
        own: state.own.clone(),
        children: children.clone(),
    };
    let mut outcomes = BTreeMap::new();
    for (key, content) in children {
        let made = state.made.get(key);
        let on_child = made.is_some_and(|made| made.on_child) && content.is_scalar();
        let left = match view.own.get(key).cloned() {
            None => Left::Nowhere,
            Some(value) if on_child => {
                view.own.remove(key);
                let written = Content::Value(Presence::Present(value));
                view.children.insert(key.clone(), written);
                Left::OnChild
            }
            Some(_) => Left::OnRecord,
        };
        let evaluates = evaluated
            && state
                .failing
                .as_ref()
                .is_none_or(|failing| !failing.contains(key));
        let by = if left == Left::Nowhere {
            Vec::new()
        } else {
            made.map(|made| made.by.clone()).unwrap_or_default()
        };
        outcomes.insert(
            key.clone(),
            Outcome {
                left,
                value: view.shown(key).filter(|_| evaluates),
                by,
            },
        );
    }
    (view, outcomes)
}

/// Definition: the values a view shows in the fields its children hold,
/// other than null and the empty record.
fn shown_fields(view: &Record) -> Bindings {
    view.children
        .keys()
        .filter_map(|key| Some((key.clone(), view.shown(key)?)))
        .filter(|(key, value)| match value {
            Value::Null => false,
            Value::Unknown(fields) => !fields.is_empty() || view.own.contains_key(key),
            _ => true,
        })
        .collect()
}

/// Definition: the shown value of `key` carried back: through the inverses
/// of the operations `by`, last first, one without an inverse passing it on.
/// `None` when an inverse does not evaluate.
fn carried_back(by: &[Option<Function>], key: &str, view: &Record) -> Option<Value> {
    let mut fields = shown_fields(view);
    let mut value = view.shown(key)?;
    for inverse in by.iter().rev().flatten() {
        fields.insert(key.to_owned(), value);
        value = inverse(&fields)?;
    }
    Some(value)
}

/// What get keeps of a source besides its view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildComplement {
    /// The keys of the source's children.
    pub held: BTreeSet<String>,
    /// The source content of the children that put cannot give back without
    /// it.
    pub recorded: BTreeMap<String, Content>,
}

/// Definition: get.
///
/// The program runs on the source. A child's field whose value was made of
/// the child's is left on the child when the child holds a scalar, and any
/// other result among the record's own fields. The source content of a child
/// is recorded when
///
/// 1. the view's child holds another content;
/// 2. a rewrite ran on the field and its result is left among the record's
///    own fields; or
/// 3. a rewrite ran on the field, and the value the view shows there,
///    carried back through the inverses of the operations that made it, is
///    another value or is not defined.
///
/// `None` outside the domain: the source holds a field of its own, or an
/// operation does not evaluate.
///
/// Book anchors: `L-lenses-5`.
#[must_use]
pub fn child_get(program: &[Operation], source: &Record) -> Option<(Record, ChildComplement)> {
    if !source.own.is_empty() {
        return None;
    }
    let mut state = State::default();
    execute(program, &source.children, &mut state)?;
    let (view, outcomes) = finish(&source.children, &state, true);
    let mut complement = ChildComplement::default();
    for (key, content) in &source.children {
        complement.held.insert(key.clone());
        let outcome = &outcomes[key];
        let another = view.children[key] != *content;
        let ran = state.rewritten.contains(key);
        let shadowed = ran && outcome.left == Left::OnRecord;
        let lost = ran
            && outcome.left == Left::OnChild
            && carried_back(&outcome.by, key, &view) != view.shown(key);
        if another || shadowed || lost {
            complement.recorded.insert(key.clone(), content.clone());
        }
    }
    Some((view, complement))
}

/// The keys a program can rewrite, in any branch.
fn rewritable(program: &[Operation]) -> BTreeSet<String> {
    program
        .iter()
        .flat_map(|operation| match operation {
            Operation::Rewrite { key, .. } => BTreeSet::from([key.clone()]),
            Operation::Case(branches) => branches
                .iter()
                .flat_map(|(_, operations)| rewritable(operations))
                .collect(),
            _ => BTreeSet::new(),
        })
        .collect()
}

/// The contents the coordinates of a view hold in a source put tries.
pub type Assignment = BTreeMap<String, Content>;

/// A put: a program, the view, the complement and the coordinates.
struct Put<'a> {
    program: &'a [Operation],
    view: &'a Record,
    complement: &'a ChildComplement,
    coordinates: BTreeSet<String>,
}

/// A source put tries, and what get makes of it.
struct Try {
    source: Record,
    outcomes: BTreeMap<String, Outcome>,
    /// Whether its program evaluates, rewrites of children's values apart.
    evaluated: bool,
    /// The view get shows of it, when its program evaluates.
    again: Option<Record>,
}

impl<'a> Put<'a> {
    fn new(program: &'a [Operation], view: &'a Record, complement: &'a ChildComplement) -> Self {
        let coordinates = rewritable(program)
            .into_iter()
            .filter(|key| view.children.contains_key(key) && !view.own.contains_key(key))
            .collect();
        Self {
            program,
            view,
            complement,
            coordinates,
        }
    }

    /// The source in which the coordinates hold `assignment`.
    fn attempt(&self, assignment: &Assignment) -> Try {
        let mut children = self.view.children.clone();
        children.extend(assignment.clone());
        let mut state = State {
            failing: Some(BTreeSet::new()),
            ..State::default()
        };
        let evaluated = execute(self.program, &children, &mut state).is_some();
        let (again, outcomes) = finish(&children, &state, evaluated);
        let refused = state.failing.as_ref().is_some_and(|f| !f.is_empty());
        Try {
            source: Record {
                own: Bindings::new(),
                children,
            },
            outcomes,
            evaluated,
            again: (evaluated && !refused).then_some(again),
        }
    }

    /// Definition: get of the tried source shows at `key` what the view
    /// shows.
    ///
    /// A field left among the record's own fields does not determine the
    /// child it shadows when the child is the source's, nothing of it is
    /// recorded, and the field was not made of the child's value or the child
    /// holds a record, which may have had a discriminator besides.
    fn reproduces(&self, tried: &Try, assignment: &Assignment, key: &str) -> bool {
        let outcome = &tried.outcomes[key];
        let undetermined = outcome.left == Left::OnRecord
            && self.complement.held.contains(key)
            && !self.complement.recorded.contains_key(key)
            && (outcome.by.is_empty() || matches!(assignment[key], Content::Fields(_)));
        !undetermined && outcome.value.is_some() && outcome.value == self.view.shown(key)
    }

    fn missed(&self, tried: &Try, assignment: &Assignment) -> BTreeSet<String> {
        self.coordinates
            .iter()
            .filter(|key| !self.reproduces(tried, assignment, key))
            .cloned()
            .collect()
    }

    /// Definition: put admits the tried source: get shows the view of it,
    /// where a field that a child alone holds in the view may be one that
    /// get leaves among the record's own fields.
    fn admits(&self, tried: &Try, assignment: &Assignment) -> bool {
        let Some(again) = &tried.again else {
            return false;
        };
        if !self.missed(tried, assignment).is_empty() {
            return false;
        }
        self.view
            .own
            .iter()
            .all(|(key, value)| again.own.get(key) == Some(value))
            && again
                .own
                .keys()
                .all(|key| self.view.own.contains_key(key) || self.coordinates.contains(key))
            && self.view.children.iter().all(|(key, content)| {
                self.coordinates.contains(key) || again.children.get(key) == Some(content)
            })
    }

    fn on_child(&self, key: &str) -> Content {
        self.view.children[key].clone()
    }

    /// Definition: the first candidate for `key` after a tried source: the
    /// shown value carried back through the inverses of the operations that
    /// made its value there, and the shown value where that is not defined.
    fn first(&self, tried: &Try, key: &str) -> Content {
        let on_child = self.on_child(key);
        match carried_back(&tried.outcomes[key].by, key, self.view) {
            Some(value) if Some(&value) != self.view.shown(key).as_ref() => {
                let fielded = matches!(
                    self.complement.recorded.get(key).unwrap_or(&on_child),
                    Content::Fields(_)
                );
                match value {
                    Value::Null => Content::Value(Presence::Null),
                    Value::Unknown(fields) if fielded => Content::Fields(fields),
                    value => Content::Value(Presence::Present(value)),
                }
            }
            _ => on_child,
        }
    }

    /// Definition: each coordinate at its recorded content, else as the view
    /// holds it.
    fn base(&self) -> Assignment {
        self.coordinates
            .iter()
            .map(|key| {
                let recorded = self.complement.recorded.get(key).cloned();
                (key.clone(), recorded.unwrap_or_else(|| self.on_child(key)))
            })
            .collect()
    }

    /// Definition: one search over the candidates of `edited`, giving the
    /// source it ends with and the coordinates outside `edited` that a tried
    /// source missed.
    fn search(
        &self,
        base: &Assignment,
        edited: &BTreeSet<String>,
    ) -> Result<Record, BTreeSet<String>> {
        let tried = self.attempt(base);
        let mut last = base.clone();
        for key in edited {
            last.insert(key.clone(), self.first(&tried, key));
        }
        let mut before: Option<Assignment> = None;
        let mut others = BTreeSet::new();
        for _ in 0..2 * edited.len() + 2 {
            let tried = self.attempt(&last);
            if self.admits(&tried, &last) {
                return Ok(tried.source);
            }
            let missed = self.missed(&tried, &last);
            others.extend(missed.difference(edited).cloned());
            let mut moving: BTreeSet<String> = missed.intersection(edited).cloned().collect();
            if moving.is_empty() {
                moving.clone_from(edited);
            }
            let mut next = last.clone();
            for key in moving {
                let first = self.first(&tried, &key);
                let content = if last[&key] == first {
                    self.on_child(&key)
                } else {
                    first
                };
                next.insert(key, content);
            }
            if next == last || Some(&next) == before.as_ref() {
                break;
            }
            before = Some(std::mem::replace(&mut last, next));
        }
        Err(others)
    }

    /// Every assignment of `candidates`.
    fn assignments(candidates: &BTreeMap<String, Vec<Content>>) -> Vec<Assignment> {
        candidates
            .iter()
            .fold(vec![Assignment::new()], |partial, (key, contents)| {
                partial
                    .iter()
                    .flat_map(|assignment| {
                        contents.iter().map(|content| {
                            let mut longer = assignment.clone();
                            longer.insert(key.clone(), content.clone());
                            longer
                        })
                    })
                    .collect()
            })
    }
}

/// Definition: put of a view of a source with the complement get gave.
///
/// The coordinates are the fields the program can rewrite that the view holds
/// on a child alone. The first source tried holds each at its recorded
/// content, else at what the view shows; a coordinate is edited when it has
/// no record or get of that source misses it. With none edited that source
/// is the only one tried. Otherwise the search of [`Put::search`] runs over
/// the candidates of the edited coordinates, and is repeated with the missed
/// ones counted as edited until there are no others.
///
/// `None` when put refuses the view.
///
/// Book anchors: `L-lenses-5`.
#[must_use]
pub fn child_put(
    program: &[Operation],
    view: &Record,
    complement: &ChildComplement,
) -> Option<Record> {
    let put = Put::new(program, view, complement);
    let base = put.base();
    let tried = put.attempt(&base);
    if !tried.evaluated {
        return None;
    }
    let missed = put.missed(&tried, &base);
    let mut edited: BTreeSet<String> = put
        .coordinates
        .iter()
        .filter(|key| !complement.recorded.contains_key(*key) || missed.contains(*key))
        .cloned()
        .collect();
    if edited.is_empty() {
        return put.admits(&tried, &base).then_some(tried.source);
    }
    loop {
        match put.search(&base, &edited) {
            Ok(source) => return Some(source),
            Err(others) if others.is_empty() => return None,
            Err(others) => edited.extend(others),
        }
    }
}

/// Definition: every source admitted by brute force.
///
/// The candidates of a coordinate are its recorded content, its shown value
/// and every value it is carried back to under the operations that ran in
/// some combination of candidates; they are collected until no combination
/// gives another. The sources are those of the combinations put admits, in
/// the order of the candidates.
///
/// Book anchors: `L-lenses-5`.
#[must_use]
pub fn child_put_solutions(
    program: &[Operation],
    view: &Record,
    complement: &ChildComplement,
) -> Vec<Record> {
    let put = Put::new(program, view, complement);
    let mut candidates: BTreeMap<String, Vec<Content>> = put
        .coordinates
        .iter()
        .map(|key| {
            let mut contents: Vec<Content> =
                complement.recorded.get(key).cloned().into_iter().collect();
            let on_child = put.on_child(key);
            if !contents.contains(&on_child) {
                contents.push(on_child);
            }
            (key.clone(), contents)
        })
        .collect();
    loop {
        let before: usize = candidates.values().map(Vec::len).sum();
        for assignment in Put::assignments(&candidates) {
            let tried = put.attempt(&assignment);
            for (key, contents) in &mut candidates {
                let first = put.first(&tried, key);
                if !contents.contains(&first) {
                    contents.push(first);
                }
            }
        }
        if candidates.values().map(Vec::len).sum::<usize>() == before {
            break;
        }
    }
    let mut sources: Vec<Record> = Vec::new();
    for assignment in Put::assignments(&candidates) {
        let tried = put.attempt(&assignment);
        if put.admits(&tried, &assignment) && !sources.contains(&tried.source) {
            sources.push(tried.source);
        }
    }
    sources
}
