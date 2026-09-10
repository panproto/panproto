//! One resource policy for every public surface.
//!
//! Several subsystems already impose useful local bounds: the parser's
//! walk depth, CST extraction depth, expression evaluation steps,
//! morphism search budget, model-check assignment counts. Each is
//! sound on its own, and together they were not a policy: what an
//! input was allowed to cost depended on which door it came through,
//! so the same document could be refused through the CLI and accepted
//! through the C ABI.
//!
//! Two properties make this a policy rather than another local bound.
//!
//! A [`Budget`] is *shared*, not per-call. A nested operation draws
//! from the same allowance as the operation containing it, so a caller
//! cannot be charged once for a walk and again for each subwalk, and a
//! document cannot escape a bound by being processed in pieces.
//!
//! A failure names *which* resource ran out and *what* the bound was.
//! "Too deep" without a number tells a caller nothing about what to
//! pass instead.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The resources a bounded operation can exhaust.
///
/// Named separately so a failure says which allowance ran out, and so
/// a caller raising one bound does not have to guess which.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Bytes of input accepted.
    InputBytes,
    /// Documents in one bundle.
    BundleEntries,
    /// Vertices, edges and nodes in a decoded schema or instance.
    GraphElements,
    /// Bytes of metadata attached to a schema or instance.
    MetadataBytes,
    /// Levels of nesting descended.
    Depth,
    /// Steps of evaluation or search performed.
    Steps,
    /// Bytes of output produced.
    OutputBytes,
}

impl Resource {
    /// The name used in diagnostics and in the configuration field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InputBytes => "input bytes",
            Self::BundleEntries => "bundle entries",
            Self::GraphElements => "graph elements",
            Self::MetadataBytes => "metadata bytes",
            Self::Depth => "depth",
            Self::Steps => "steps",
            Self::OutputBytes => "output bytes",
        }
    }
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A bounded resource ran out.
///
/// Carries both halves a caller needs: which allowance was exhausted,
/// and what it was set to, so raising it is a matter of reading the
/// error rather than of guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{resource} limit exceeded: the configured bound is {limit}")]
pub struct LimitExceeded {
    /// Which allowance ran out.
    pub resource: Resource,
    /// The bound that was configured for it.
    pub limit: u64,
}

/// What a single operation is allowed to consume.
///
/// Every field is a maximum. `0` means unbounded, which is a
/// deliberate choice a Rust caller can make and which no FFI or
/// command-line entry point selects by default: unbounded behaviour
/// should be asked for, never inherited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Bytes of input accepted in one operation.
    pub input_bytes: u64,
    /// Documents in one bundle.
    pub bundle_entries: u64,
    /// Vertices, edges and nodes in a decoded schema or instance.
    pub graph_elements: u64,
    /// Bytes of metadata attached to a schema or instance.
    pub metadata_bytes: u64,
    /// Levels of nesting descended.
    pub depth: u64,
    /// Steps of evaluation or search performed.
    pub steps: u64,
    /// Bytes of output produced.
    pub output_bytes: u64,
}

impl ResourceLimits {
    /// The defaults every untrusted public surface uses.
    ///
    /// Chosen to admit any document a person would plausibly author
    /// while refusing one built to exhaust a machine. The depth bound
    /// matches the walk and extraction depths that already existed, so
    /// this changes no behaviour those already governed.
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            input_bytes: 64 * 1024 * 1024,
            bundle_entries: 4_096,
            graph_elements: 1_000_000,
            metadata_bytes: 16 * 1024 * 1024,
            depth: 128,
            steps: 10_000_000,
            output_bytes: 64 * 1024 * 1024,
        }
    }

    /// No bound on anything.
    ///
    /// For a Rust caller processing input it produced itself. Reaching
    /// for this at a boundary that accepts input from elsewhere is what
    /// this type exists to prevent.
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            input_bytes: 0,
            bundle_entries: 0,
            graph_elements: 0,
            metadata_bytes: 0,
            depth: 0,
            steps: 0,
            output_bytes: 0,
        }
    }

    /// The bound configured for `resource`.
    #[must_use]
    pub const fn get(&self, resource: Resource) -> u64 {
        match resource {
            Resource::InputBytes => self.input_bytes,
            Resource::BundleEntries => self.bundle_entries,
            Resource::GraphElements => self.graph_elements,
            Resource::MetadataBytes => self.metadata_bytes,
            Resource::Depth => self.depth,
            Resource::Steps => self.steps,
            Resource::OutputBytes => self.output_bytes,
        }
    }

    /// Start a budget against these limits.
    #[must_use]
    pub fn budget(self) -> Budget {
        Budget::new(self)
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::defaults()
    }
}

/// An allowance being drawn down by one operation and everything
/// nested inside it.
///
/// Cloning shares the same counters rather than copying them, which is
/// the property that makes limits compose: an operation that calls
/// another passes a clone, and the two draw from one pool. A budget
/// that reset per subsystem would bound each step and nothing overall,
/// which is what having several unrelated local limits already
/// achieved.
#[derive(Clone, Debug)]
pub struct Budget {
    limits: ResourceLimits,
    consumed: Arc<Consumed>,
}

#[derive(Debug, Default)]
struct Consumed {
    input_bytes: AtomicU64,
    bundle_entries: AtomicU64,
    graph_elements: AtomicU64,
    metadata_bytes: AtomicU64,
    steps: AtomicU64,
    output_bytes: AtomicU64,
}

impl Budget {
    /// A fresh budget against `limits`.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            consumed: Arc::new(Consumed::default()),
        }
    }

    /// A fresh budget against [`ResourceLimits::defaults`].
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(ResourceLimits::defaults())
    }

    /// The limits this budget draws against.
    #[must_use]
    pub const fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Charge `amount` against `resource`.
    ///
    /// # Errors
    ///
    /// Returns [`LimitExceeded`] naming `resource` and its bound when
    /// the charge would take the total past it. The amount is still
    /// recorded, so a caller that ignores one failure and continues
    /// does not find the next charge succeeding.
    pub fn charge(&self, resource: Resource, amount: u64) -> Result<(), LimitExceeded> {
        let limit = self.limits.get(resource);
        let Some(counter) = self.counter(resource) else {
            // Depth is not cumulative: it rises and falls with the
            // walk, so it is checked against a level rather than a
            // running total. See `enter`.
            return self.check_depth(amount);
        };
        let total = counter.fetch_add(amount, Ordering::Relaxed) + amount;
        if limit != 0 && total > limit {
            return Err(LimitExceeded { resource, limit });
        }
        Ok(())
    }

    /// Check that `level` is within the depth bound.
    ///
    /// # Errors
    ///
    /// Returns [`LimitExceeded`] for [`Resource::Depth`] when `level`
    /// is past the bound.
    pub fn enter(&self, level: u64) -> Result<(), LimitExceeded> {
        self.check_depth(level)
    }

    fn check_depth(&self, level: u64) -> Result<(), LimitExceeded> {
        let limit = self.limits.depth;
        if limit != 0 && level > limit {
            return Err(LimitExceeded {
                resource: Resource::Depth,
                limit,
            });
        }
        Ok(())
    }

    /// How much of `resource` has been charged so far.
    #[must_use]
    pub fn consumed(&self, resource: Resource) -> u64 {
        self.counter(resource)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }

    fn counter(&self, resource: Resource) -> Option<&AtomicU64> {
        match resource {
            Resource::InputBytes => Some(&self.consumed.input_bytes),
            Resource::BundleEntries => Some(&self.consumed.bundle_entries),
            Resource::GraphElements => Some(&self.consumed.graph_elements),
            Resource::MetadataBytes => Some(&self.consumed.metadata_bytes),
            Resource::Steps => Some(&self.consumed.steps),
            Resource::OutputBytes => Some(&self.consumed.output_bytes),
            Resource::Depth => None,
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::with_defaults()
    }
}
