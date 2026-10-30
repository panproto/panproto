//! Complete finite scopes for the reference-resolution page.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::Tree;
use crate::resolution::{IndexedSource, IndexedView, SingleStep, UriDirective};

/// The three targets of the finite S8 state-law carrier.
pub const INDEXED_TARGETS: [&str; 3] = ["a", "b", "c"];

/// Every reference trace of length at most three.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 3)
}

/// Labelled two-node rooted trees with no fans.
#[must_use]
pub fn trees() -> Vec<Tree> {
    super::trees(&super::TreeScope {
        nodes: 2,
        anchors: &["a"],
        values: &[],
        arc_names: &["e"],
        hyper_edges: &[],
        max_fans: 0,
    })
}

/// Every source subset of `{a,b,c}`, in bit-vector order.
///
/// The carrier has exactly `2^3 = 8` elements.
#[must_use]
pub fn indexed_sources() -> Vec<IndexedSource> {
    (0u8..8)
        .map(|bits| {
            INDEXED_TARGETS
                .iter()
                .enumerate()
                .filter(|(index, _)| bits & (1 << index) != 0)
                .map(|(_, target)| (*target).to_owned())
                .collect()
        })
        .collect()
}

/// The deterministic Single-Step Basis at `view`.
///
/// Additions of absent targets come first in target order. Then, for each
/// carrier in id order, deletion precedes retargeting to every other target
/// in target order. These are exactly the add/delete/retarget witnesses used
/// by the S8 finite model; no candidate is deduplicated after application.
#[must_use]
pub fn single_step_basis(view: &IndexedView) -> Vec<SingleStep> {
    let named: BTreeSet<&str> = view.values().map(String::as_str).collect();
    let mut steps: Vec<SingleStep> = INDEXED_TARGETS
        .iter()
        .filter(|target| !named.contains(**target))
        .map(|target| SingleStep::Add((*target).to_owned()))
        .collect();
    for (id, target) in view {
        steps.push(SingleStep::Delete(*id));
        steps.extend(
            INDEXED_TARGETS
                .iter()
                .filter(|other| **other != target)
                .map(|other| SingleStep::Retarget {
                    id: *id,
                    target: (*other).to_owned(),
                }),
        );
    }
    steps
}

/// Every injective renaming of `ids` into `0..pool`, in lexicographic order.
#[must_use]
pub fn injective_renamings(ids: &[u32], pool: u32) -> Vec<BTreeMap<u32, u32>> {
    ids.iter().fold(vec![BTreeMap::new()], |partial, id| {
        partial
            .into_iter()
            .flat_map(|renaming| {
                let used: BTreeSet<u32> = renaming.values().copied().collect();
                (0..pool)
                    .filter(move |image| !used.contains(image))
                    .map(move |image| {
                        let mut next = renaming.clone();
                        next.insert(*id, image);
                        next
                    })
            })
            .collect()
    })
}

/// Every sequence of at most three positional URI directives drawn from the
/// bases `http://a/b/`, `c/` and `../d/` and the label `p` bound to
/// `http://x/` or `y#`: `1 + 5 + 25 + 125 = 156` sequences.
#[must_use]
pub fn uri_directive_sequences() -> Vec<Vec<UriDirective>> {
    let base = |iri: &str| UriDirective::Base(iri.to_owned());
    let prefix = |iri: &str| UriDirective::Prefix {
        label: "p".to_owned(),
        iri: iri.to_owned(),
    };
    sequences(&[
        base("http://a/b/"),
        base("c/"),
        base("../d/"),
        prefix("http://x/"),
        prefix("y#"),
    ])
}

/// One occurrence of each written form that follows a directive sequence:
/// an absolute IRI reference, a relative one, a prefixed name and `a`.
#[must_use]
pub const fn uri_written_forms() -> [&'static str; 4] {
    ["http://e/f", "g", "p:q", "a"]
}

/// Every ancestor-or-self chain of at most three elements, root first.
///
/// Each element holds no `xml:base` or one of `http://a/b/`, `c/` and
/// `../d/`, which gives `4 + 16 + 64 = 84` chains. Every element of a tree
/// of at most three elements, siblings included, has one of these chains.
#[must_use]
pub fn inherited_base_chains() -> Vec<Vec<Option<String>>> {
    let values = [
        None,
        Some("http://a/b/".to_owned()),
        Some("c/".to_owned()),
        Some("../d/".to_owned()),
    ];
    sequences(&values)
        .into_iter()
        .filter(|chain| !chain.is_empty())
        .collect()
}

/// Every standoff coordinate tuple in the bounded arithmetic carrier.
///
/// The tuple is `(start, end, origin, end_is_length)`. Each numeric axis is
/// `{0,1,2,u64::MAX}`, giving exactly `4^3 * 2 = 128` labelled cases.
#[must_use]
pub fn standoff_coordinates() -> Vec<(u64, u64, u64, bool)> {
    let values = [0, 1, 2, u64::MAX];
    let mut out = Vec::with_capacity(128);
    for start in values {
        for end in values {
            for origin in values {
                for length in [false, true] {
                    out.push((start, end, origin, length));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        indexed_sources, inherited_base_chains, injective_renamings, single_step_basis,
        standoff_coordinates, uri_directive_sequences,
    };
    use crate::resolution::indexed_get;

    #[test]
    fn bounds_are_exact_and_independently_counted() {
        let sources = indexed_sources();
        assert_eq!(sources.len(), 1usize << 3, "indexed sources");
        let candidate_count: usize = sources
            .iter()
            .map(indexed_get)
            .map(|(view, _)| single_step_basis(&view).len())
            .sum();
        // Each target is independently either absent (one add) or present
        // (one delete and two retargets): 3 * 2^2 + 3 * 3 * 2^2 = 48.
        assert_eq!(candidate_count, 3 * 4 + 3 * 3 * 4, "single steps");
        assert_eq!(injective_renamings(&[0, 1], 5).len(), 5 * 4);
        // Sum of 3^k for k = 0..=3.
        assert_eq!(uri_directive_sequences().len(), 1 + 5 + 25 + 125);
        assert_eq!(inherited_base_chains().len(), 4 + 16 + 64);
        assert_eq!(standoff_coordinates().len(), 4 * 4 * 4 * 2);
    }
}
