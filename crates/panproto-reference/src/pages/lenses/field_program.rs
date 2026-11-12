//! The value-level steps of the lens DSL at the instance level.
//!
//! An `apply_expr`, a `compute_field`, the default an `add_field` writes and
//! its optional computation, and a directed equation each attach a program
//! of field steps to a vertex. Get runs the program on the fields of every
//! node anchored there. Put follows the baseline-and-pullback rule: it
//! rebuilds the source fields from the recorded ones and the view, takes an
//! edited output back through the inverses of the steps that wrote it,
//! replays the program and refuses an edit the replay does not reproduce.
//! The step maps are functions on the reference's values: the expression
//! language that production evaluates them with is the trusted boundary,
//! checked on its own correctness pages.

use std::collections::{BTreeMap, BTreeSet};

use super::coercion::ValueMap;
use crate::model::{Graph, Node};
use crate::restrict::{
    LayerRefusal, PredicateRefusal, PutFieldRefusal, ValueLayer, ValueLayerSemantics,
};
use crate::value::{Presence, Value};

/// One step of a field program.
#[derive(Clone, Debug)]
pub enum FieldStep {
    /// Rewrite the field `key` in place by `forward` when the node carries
    /// it; an edit is taken back by `inverse`, when there is one.
    Rewrite {
        /// The rewritten field.
        key: &'static str,
        /// The map applied at get.
        forward: ValueMap,
        /// The map taking an edited value back.
        inverse: Option<ValueMap>,
    },
    /// Write `forward` of the field `source` under `target`; a node without
    /// `source` is refused. An edit of `target` is taken back by `inverse`
    /// to `source` only when the view no longer shows `source`.
    Compute {
        /// The written field.
        target: &'static str,
        /// The field read.
        source: &'static str,
        /// The map applied at get.
        forward: ValueMap,
        /// The map taking an edited value back.
        inverse: Option<ValueMap>,
    },
    /// Write `value` under `key` when the node does not carry it.
    Default {
        /// The written field.
        key: &'static str,
        /// The written value.
        value: Value,
    },
}

impl FieldStep {
    /// The field this step writes.
    const fn written(&self) -> &'static str {
        match self {
            Self::Rewrite { key, .. } | Self::Default { key, .. } => key,
            Self::Compute { target, .. } => target,
        }
    }

    /// Whether a refused replay of an edit of the written field is an edit
    /// of a derived cell rather than of a transformed one.
    const fn derives(&self) -> bool {
        matches!(self, Self::Compute { .. } | Self::Default { .. })
    }
}

/// The value layer of a field program at a set of vertices.
#[derive(Clone, Debug)]
pub struct FieldProgram {
    /// Name of the program, recorded in the layer's canonical descriptor.
    pub name: &'static str,
    /// The vertices whose nodes the program runs on.
    pub vertices: Vec<String>,
    /// The steps, in order.
    pub steps: Vec<FieldStep>,
}

/// A run of a program: the fields it produced and the steps that wrote.
struct Run {
    fields: BTreeMap<String, Value>,
    executed: Vec<usize>,
}

impl FieldProgram {
    /// Definition: The fields a program may write at each of its vertices,
    /// in step order, the declared outputs of its migration.
    ///
    /// Book anchors: `L-lenses-27`.
    #[must_use]
    pub fn output_fields(&self) -> Vec<String> {
        self.steps
            .iter()
            .map(|step| step.written().to_owned())
            .collect()
    }

    fn runs_at(&self, anchor: &str) -> bool {
        !self.steps.is_empty() && self.vertices.iter().any(|vertex| vertex == anchor)
    }

    /// Run the program on `fields`, refusing at the first step whose map is
    /// undefined or whose read field is absent, naming the written field.
    fn run(&self, fields: &BTreeMap<String, Value>) -> Result<Run, &'static str> {
        let mut fields = fields.clone();
        let mut executed = Vec::new();
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                FieldStep::Rewrite { key, forward, .. } => {
                    if let Some(value) = fields.get(*key) {
                        let image = forward(value).ok_or(*key)?;
                        fields.insert((*key).to_owned(), image);
                        executed.push(index);
                    }
                }
                FieldStep::Compute {
                    target,
                    source,
                    forward,
                    ..
                } => {
                    let image = fields.get(*source).and_then(forward).ok_or(*target)?;
                    fields.insert((*target).to_owned(), image);
                    executed.push(index);
                }
                FieldStep::Default { key, value } => {
                    if !fields.contains_key(*key) {
                        fields.insert((*key).to_owned(), value.clone());
                        executed.push(index);
                    }
                }
            }
        }
        Ok(Run { fields, executed })
    }

    /// Whether some step of `steps` writes `key`.
    fn touches<'a>(&self, mut steps: impl Iterator<Item = &'a usize>, key: &str) -> bool {
        steps.any(|index| self.steps[*index].written() == key)
    }

    /// The baseline source fields of a view node: the recorded source fields
    /// with every view field no step writes, or the view's fields when get
    /// recorded none.
    fn baseline(
        &self,
        shown: &BTreeMap<String, Value>,
        recorded: Option<&BTreeMap<String, Value>>,
    ) -> BTreeMap<String, Value> {
        let all: Vec<usize> = (0..self.steps.len()).collect();
        recorded.map_or_else(
            || shown.clone(),
            |recorded| {
                let mut fields = recorded.clone();
                for (key, value) in shown {
                    if !self.touches(all.iter(), key) {
                        fields.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
                fields
            },
        )
    }

    /// Pull the view's edited outputs back through the inverses of the
    /// steps the baseline run executed, latest first.
    fn pull_back(
        &self,
        shown: &BTreeMap<String, Value>,
        baseline: BTreeMap<String, Value>,
        run: &Run,
    ) -> BTreeMap<String, Value> {
        let mut changed: BTreeSet<String> = run
            .fields
            .keys()
            .chain(shown.keys())
            .filter(|key| run.fields.get(*key) != shown.get(*key))
            .cloned()
            .collect();
        // A field no executed step writes is the view's, present or absent.
        let mut fields = baseline;
        fields.retain(|key, _| self.touches(run.executed.iter(), key) || shown.contains_key(key));
        for (key, value) in shown {
            if !self.touches(run.executed.iter(), key) {
                fields.insert(key.clone(), value.clone());
            }
        }
        let mut working = shown.clone();
        for index in run.executed.iter().rev() {
            match &self.steps[*index] {
                FieldStep::Rewrite {
                    key,
                    inverse: Some(inverse),
                    ..
                } if changed.contains(*key)
                    && working.contains_key(*key)
                    && fields.contains_key(*key) =>
                {
                    if let Some(source) = inverse(&working[*key]) {
                        fields.insert((*key).to_owned(), source.clone());
                        working.insert((*key).to_owned(), source);
                    }
                }
                FieldStep::Compute {
                    target,
                    source,
                    inverse: Some(inverse),
                    ..
                } if changed.contains(*target)
                    && working.contains_key(*target)
                    && (!working.contains_key(*source) || source == target)
                    && fields.contains_key(*source) =>
                {
                    if let Some(value) = inverse(&working[*target]) {
                        fields.insert((*source).to_owned(), value.clone());
                        working.insert((*source).to_owned(), value);
                        changed.remove(*target);
                        changed.insert((*source).to_owned());
                    }
                }
                _ => {}
            }
        }
        fields
    }

    /// The replay check: the fields the baseline run or the replay wrote,
    /// each judged by the last step that wrote it, against the view; the
    /// first field the replay does not reproduce is refused.
    fn replay_refusal(
        &self,
        shown: &BTreeMap<String, Value>,
        baseline: &Run,
        replay: &Run,
    ) -> Option<PutFieldRefusal> {
        if replay.fields == *shown {
            return None;
        }
        let mut seen = BTreeSet::new();
        baseline
            .executed
            .iter()
            .chain(&replay.executed)
            .rev()
            .map(|index| &self.steps[*index])
            .filter(|step| seen.insert(step.written()))
            .find(|step| replay.fields.get(step.written()) != shown.get(step.written()))
            .map(|step| {
                let field = step.written().to_owned();
                if step.derives() {
                    PutFieldRefusal::DerivedCellEdited { field }
                } else {
                    PutFieldRefusal::TransformNotInvertible { field }
                }
            })
    }
}

impl ValueLayer for FieldProgram {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/field-program",
            version: 1,
            canonical_payload: self.name.as_bytes().to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    /// Definition: Get runs the program on the fields of a node at one of
    /// its vertices; a step whose read field is absent or whose map is
    /// undefined refuses, naming the field it writes.
    ///
    /// Book anchors: `L-lenses-27`.
    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        if !self.runs_at(&node.anchor) {
            return Ok((node.fields.clone(), node.value.clone()));
        }
        let run = self.run(&node.fields).map_err(|field| LayerRefusal {
            kind: "field_transform_failed".to_owned(),
            detail: field.to_owned(),
        })?;
        Ok((run.fields, node.value.clone()))
    }

    /// Definition: Put restores a node's fields by the baseline-and-pullback
    /// rule.
    ///
    /// The baseline is the recorded source fields together with every view
    /// field no step writes, or the view's fields when get recorded none.
    /// A field the baseline run does not write takes its presence and value
    /// from the view. An output the view changed is taken back through the
    /// inverse of the step that wrote it, latest step first: a rewrite's
    /// inverse to its own field, a computation's to the field it read when
    /// the view no longer shows that field. The program is replayed on the
    /// result; a written field the replay does not reproduce is refused, as
    /// `DerivedCellEdited` when a computation or a default wrote it last and
    /// as `TransformNotInvertible` otherwise. A replay that refuses is left
    /// to the get of the restored source. The node's own value is the
    /// view's.
    ///
    /// Book anchors: `L-lenses-27`.
    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        _: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        if !self.runs_at(&view.anchor) {
            return Ok((
                original_fields
                    .cloned()
                    .unwrap_or_else(|| view.fields.clone()),
                view.value.clone(),
            ));
        }
        let baseline = self.baseline(&view.fields, original_fields);
        let Ok(run) = self.run(&baseline) else {
            return Ok((baseline, view.value.clone()));
        };
        let restored = self.pull_back(&view.fields, baseline, &run);
        let Ok(replay) = self.run(&restored) else {
            return Ok((restored, view.value.clone()));
        };
        self.replay_refusal(&view.fields, &run, &replay)
            .map_or_else(|| Ok((restored, view.value.clone())), Err)
    }
}
