//! Lens paths: the free category on atomic lens stages.
//!
//! A path is a non-empty sequence of atomic stages, or the distinguished
//! identity at a schema; `then` concatenates, dropping a distinguished
//! identity on either side, and never fuses. Get folds the stages left to
//! right; put folds right to left, each stage putting against the view the
//! stage after it produced; the put domain of a path is that of the first
//! failing stage in put order. Fusion is an optimisation that must agree
//! with this fold, so the reference never fuses.

/// A path of stages of type `L`, by value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LensPath<L> {
    /// The distinguished identity at a schema, named.
    Identity(String),
    /// Atomic stages in order (non-empty).
    Stages(Vec<L>),
}

impl<L: Clone> LensPath<L> {
    /// A path of one stage.
    #[must_use]
    pub fn single(l: L) -> Self {
        Self::Stages(vec![l])
    }

    /// Concatenation: a distinguished identity is a two-sided unit.
    #[must_use]
    pub fn then(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Identity(_), p) | (p, Self::Identity(_)) => p.clone(),
            (Self::Stages(a), Self::Stages(b)) => {
                Self::Stages(a.iter().chain(b).cloned().collect())
            }
        }
    }

    /// The atomic stages (none for an identity).
    #[must_use]
    pub fn stages(&self) -> &[L] {
        match self {
            Self::Identity(_) => &[],
            Self::Stages(s) => s,
        }
    }
}

/// One stage's get: stage index and view to the next view and its
/// complement.
pub type StageGet<'a, I, C, E> = &'a dyn Fn(usize, &I) -> Result<(I, C), E>;

/// Get through a path: each stage's get on the previous view; returns the
/// last view and every stage's complement in order.
///
/// # Errors
///
/// The first stage error with its stage index.
pub fn get_path<I: Clone, C, E>(
    stages: usize,
    get: StageGet<'_, I, C, E>,
    source: &I,
) -> Result<(I, Vec<C>), (usize, E)> {
    let mut view = source.clone();
    let mut cs = Vec::with_capacity(stages);
    for k in 0..stages {
        let (v, c) = get(k, &view).map_err(|e| (k, e))?;
        view = v;
        cs.push(c);
    }
    Ok((view, cs))
}

/// Put through a path: from the last stage to the first, each stage puts
/// the view against its complement, giving the view of the stage before.
///
/// # Errors
///
/// The first failing stage in put order (the rightmost failing stage whose
/// right neighbours all succeeded), with its index.
pub fn put_path<I: Clone, C, E>(
    put: &dyn Fn(usize, &I, &C) -> Result<I, E>,
    view: &I,
    complements: &[C],
) -> Result<I, (usize, E)> {
    let mut cur = view.clone();
    for k in (0..complements.len()).rev() {
        cur = put(k, &cur, &complements[k]).map_err(|e| (k, e))?;
    }
    Ok(cur)
}
