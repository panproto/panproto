//! Exact isomorphism of RDF graphs given as sets of triples, shared by the
//! conformance tests of the RDF routes.

use std::collections::{BTreeMap, BTreeSet};

use panproto_io::rdf::{Term, Triple};

/// The bound on assignments the isomorphism search tries.
const STEP_BOUND: u64 = 1_000_000;

fn blank_labels(set: &BTreeSet<Triple>) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for t in set {
        for term in [&t.subject, &t.object] {
            if let Term::Blank(label) = term {
                out.insert(label.clone());
            }
        }
    }
    out.into_iter().collect()
}

fn ground(set: &BTreeSet<Triple>) -> BTreeSet<&Triple> {
    set.iter()
        .filter(|t| !matches!(t.subject, Term::Blank(_)) && !matches!(t.object, Term::Blank(_)))
        .collect()
}

/// The colour of each blank node: refined from its triples, each blank
/// node in them shown by its colour, until the partition stops growing.
fn colours(set: &BTreeSet<Triple>, nodes: &[String]) -> BTreeMap<String, String> {
    let mut colour: BTreeMap<String, String> =
        nodes.iter().map(|n| (n.clone(), String::new())).collect();
    let mut classes = 1;
    for _ in 0..=nodes.len() {
        let show = |term: &Term, colour: &BTreeMap<String, String>| match term {
            Term::Blank(label) => format!("B[{}]", colour[label]),
            other => format!("{other:?}"),
        };
        let mut next: BTreeMap<String, String> = BTreeMap::new();
        for node in nodes {
            let mut signature: Vec<String> = set
                .iter()
                .filter_map(|t| {
                    let s = matches!(&t.subject, Term::Blank(l) if l == node);
                    let o = matches!(&t.object, Term::Blank(l) if l == node);
                    (s || o).then(|| {
                        format!(
                            "{s}{o}|{}|{}|{}",
                            show(&t.subject, &colour),
                            t.predicate,
                            show(&t.object, &colour)
                        )
                    })
                })
                .collect();
            signature.sort();
            next.insert(
                node.clone(),
                format!("{}#{}", colour[node], signature.join(";")),
            );
        }
        let count = next.values().collect::<BTreeSet<_>>().len();
        colour = next;
        if count == classes {
            break;
        }
        classes = count;
    }
    colour
}

/// `t` with its blank nodes renamed by `map`, or `None` when `map` does not
/// fix all of them.
fn mapped(t: &Triple, map: &BTreeMap<&str, &str>) -> Option<Triple> {
    let term = |term: &Term| match term {
        Term::Blank(label) => map
            .get(label.as_str())
            .map(|l| Term::Blank((*l).to_owned())),
        other => Some(other.clone()),
    };
    Some(Triple {
        subject: term(&t.subject)?,
        predicate: t.predicate.clone(),
        object: term(&t.object)?,
    })
}

/// The backtracking search for a bijection of blank nodes.
struct Search<'s> {
    a: &'s BTreeSet<Triple>,
    b: &'s BTreeSet<Triple>,
    order: Vec<&'s str>,
    candidates: BTreeMap<&'s str, Vec<&'s str>>,
    steps: u64,
}

impl<'s> Search<'s> {
    fn consistent(&self, map: &BTreeMap<&'s str, &'s str>) -> bool {
        self.a
            .iter()
            .filter_map(|t| mapped(t, map))
            .all(|t| self.b.contains(&t))
    }

    fn go(
        &mut self,
        i: usize,
        map: &mut BTreeMap<&'s str, &'s str>,
        used: &mut BTreeSet<&'s str>,
    ) -> bool {
        if i == self.order.len() {
            return true;
        }
        let node = self.order[i];
        for candidate in self.candidates[node].clone() {
            if used.contains(candidate) {
                continue;
            }
            self.steps += 1;
            if self.steps > STEP_BOUND {
                return false;
            }
            map.insert(node, candidate);
            used.insert(candidate);
            if self.consistent(map) && self.go(i + 1, map, used) {
                return true;
            }
            map.remove(node);
            used.remove(candidate);
        }
        false
    }
}

/// Whether two sets of triples are isomorphic: equal under some bijection
/// of their blank nodes (RDF 1.1 Concepts Sec 3.6).
///
/// Ground triples are compared as sets. Blank nodes are coloured by the
/// triples they occur in, with every blank node replaced by its colour,
/// and the colours refined until the partition stops growing (at most one
/// round per blank node). A bijection is then searched for by backtracking
/// over blank nodes of equal colour, each partial assignment checked
/// against the triples whose blank nodes it fixes. The search is bounded
/// by `STEP_BOUND` assignments; a search that reaches the bound counts as
/// not isomorphic, so the test fails rather than passing unchecked.
pub fn isomorphic(a: &BTreeSet<Triple>, b: &BTreeSet<Triple>) -> bool {
    if a.len() != b.len() || ground(a) != ground(b) {
        return false;
    }
    let (blanks_a, blanks_b) = (blank_labels(a), blank_labels(b));
    if blanks_a.len() != blanks_b.len() {
        return false;
    }
    let (colour_a, colour_b) = (colours(a, &blanks_a), colours(b, &blanks_b));
    let mut candidates: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in &blanks_a {
        candidates.insert(
            node,
            blanks_b
                .iter()
                .filter(|other| colour_b[*other] == colour_a[node])
                .map(String::as_str)
                .collect(),
        );
    }
    let mut order: Vec<&str> = blanks_a.iter().map(String::as_str).collect();
    order.sort_by_key(|node| candidates[node].len());
    let mut search = Search {
        a,
        b,
        order,
        candidates,
        steps: 0,
    };
    search.go(0, &mut BTreeMap::new(), &mut BTreeSet::new())
}
