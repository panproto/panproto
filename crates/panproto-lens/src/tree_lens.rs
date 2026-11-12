//! Tree lenses over trees held as segments.
//!
//! Whole-state tree get and put are [`crate::get`] and [`crate::put`]; this
//! module runs tree get over a tree laid out as the segments of a partition,
//! as a checkpointed run of [`TreeRestrict`].

use panproto_inst::RestrictPlan;
use panproto_inst::identity::{ComplementShape, Digest};
use panproto_inst::segment::{
    Budget, RunSpec, SegmentManifest, SegmentSource, SegmentStore, SegmentedRestrictError,
    SegmentedRun, TreeRestrict, run_segmented,
};

use crate::{Lens, LensError};

/// Forward tree-lens direction over a tree held as segments, as a
/// checkpointed run.
///
/// The plan of `lens` is compiled once, as whole-state tree get compiles
/// it, and run as [`TreeRestrict`] over the tree the manifest `manifest`
/// seals, whose trusted digest is `input`, reading its segments from
/// `source` and committing every task to `store` under `budget`. The run's
/// lens identity and view schema are those of the tree identity of `lens`
/// ([`Lens::identity`] at [`ComplementShape::WType`]); it stops once
/// `cancel_after_tasks` tasks are done, and a later call against the same
/// store resumes it. A complete run emits each segment's view and tree
/// complement segment, read back with
/// [`read_emitted_segment`](panproto_inst::segment::read_emitted_segment)
/// and [`TreeComplementSegment::from_canonical_bytes`](panproto_inst::segment::TreeComplementSegment::from_canonical_bytes).
/// Glued, with the synthesized elements renamed by key as whole-state get
/// numbers them, the views are whole-state tree get's view, and the
/// complements its complement in the graph form of the source's embedding.
///
/// A lens whose restriction carries an expression is declined by the run
/// with a global residual before any segment is read; budgets, refusals and
/// store failures are reported in the run's report.
///
/// # Errors
///
/// Returns the plan compilation error.
pub fn get_segmented<Src: SegmentSource, St: SegmentStore>(
    lens: &Lens,
    (manifest, input): (&SegmentManifest, Digest),
    source: &Src,
    budget: Budget,
    store: &mut St,
    cancel_after_tasks: Option<u32>,
) -> Result<SegmentedRun<SegmentedRestrictError>, LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    let identity = lens.identity(ComplementShape::WType);
    Ok(run_segmented(
        &TreeRestrict { plan: &plan },
        manifest,
        source,
        RunSpec {
            lens: identity.lens,
            input,
            output_schema: identity.target_schema,
            cancel_after_tasks,
            reuse: None,
        },
        budget,
        store,
        &mut |_| {},
    ))
}
