//! Complete reference operations assigned to `migration-search`.
//!
//! The schema morphism search is specified here by enumeration: every vertex
//! assignment of a source schema into a target schema is listed, its cost is
//! computed term by term in exact rational arithmetic and rounded once per
//! term to the fixed-point unit, and the optima are read off the whole list.
//! A returned morphism is checked against the two schemas alone. Nothing here
//! shares code with production, which poses a cost function network and
//! solves it by bucket elimination, branch and bound or partitioning.

use std::collections::{BTreeMap, BTreeSet};

/// One directed schema edge. The derived order compares source, target,
/// kind and name in that order, with an absent name before every name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    /// Source vertex id.
    pub src: String,
    /// Target vertex id.
    pub tgt: String,
    /// Edge kind.
    pub kind: String,
    /// Optional edge name.
    pub name: Option<String>,
}

impl Edge {
    /// An edge from its four fields.
    #[must_use]
    pub fn new(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
        Self {
            src: src.to_owned(),
            tgt: tgt.to_owned(),
            kind: kind.to_owned(),
            name: name.map(str::to_owned),
        }
    }
}

/// The part of a schema the search reads: vertex ids with their kinds, and
/// the edge set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    /// Vertex id to vertex kind.
    pub vertices: BTreeMap<String, String>,
    /// The edges.
    pub edges: BTreeSet<Edge>,
}

impl Schema {
    /// The edges leaving `vertex`.
    fn outgoing<'a>(&'a self, vertex: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |edge| edge.src == vertex)
    }

    /// The out-degree of `vertex`, counting parallel edges and loops.
    fn degree(&self, vertex: &str) -> usize {
        self.outgoing(vertex).count()
    }

    /// The distinct names on the edges leaving `vertex`.
    fn outgoing_names<'a>(&'a self, vertex: &'a str) -> BTreeSet<&'a str> {
        self.outgoing(vertex)
            .filter_map(|edge| edge.name.as_deref())
            .collect()
    }

    /// The edges whose two endpoints are vertices of the schema.
    fn internal_edges(&self) -> impl Iterator<Item = &Edge> + '_ {
        self.edges.iter().filter(|edge| {
            self.vertices.contains_key(&edge.src) && self.vertices.contains_key(&edge.tgt)
        })
    }

    /// The sub-schema induced on `keep`: those vertices and every edge with
    /// both endpoints among them.
    #[must_use]
    pub fn induced(&self, keep: &BTreeSet<String>) -> Self {
        Self {
            vertices: self
                .vertices
                .iter()
                .filter(|(id, _)| keep.contains(*id))
                .map(|(id, kind)| (id.clone(), kind.clone()))
                .collect(),
            edges: self
                .edges
                .iter()
                .filter(|edge| keep.contains(&edge.src) && keep.contains(&edge.tgt))
                .cloned()
                .collect(),
        }
    }
}

/// The properties a requested morphism must have beyond preserving kinds
/// and edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Request {
    /// The vertex map is injective on the vertices it maps.
    pub monic: bool,
    /// The vertex map is onto the target's vertices (total requests only).
    pub epic: bool,
    /// The map is injective and reflects arcs: between any two mapped
    /// vertices, source and target carry the same multiset of edge kinds.
    pub iso: bool,
}

/// A vertex assignment: each source vertex to a target vertex, or to `None`
/// when it is dropped.
pub type Assignment = BTreeMap<String, Option<String>>;

/// A total vertex map.
pub type VertexMap = BTreeMap<String, String>;

/// An edge map.
pub type EdgeMap = BTreeMap<Edge, Edge>;

/// The fixed-point scale of a quality cost: one unit is `10^-9`.
pub const COST_SCALE: u64 = 1_000_000_000;

/// The cost of an assignment. The derived order is lexicographic: quality
/// first, then the number of dropped source vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cost {
    /// The quality cost in units of `10^-9`: the sum over the cost terms of
    /// each term rounded to the nearest unit.
    pub quality_units: u64,
    /// The number of source vertices the assignment drops.
    pub drops: u64,
}

impl Cost {
    /// The quality this cost reports, `1 - quality_units / COST_SCALE`, as
    /// the exact pair `(COST_SCALE - quality_units, COST_SCALE)`; `None` when
    /// the quality cost exceeds the scale.
    #[must_use]
    pub const fn quality(self) -> Option<(u64, u64)> {
        match COST_SCALE.checked_sub(self.quality_units) {
            Some(numerator) => Some((numerator, COST_SCALE)),
            None => None,
        }
    }
}

/// A non-negative rational, kept reduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ratio {
    num: u128,
    den: u128,
}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

impl Ratio {
    const ZERO: Self = Self { num: 0, den: 1 };

    /// `num / den`, and zero when `den` is zero (an empty average).
    fn of(num: usize, den: usize) -> Self {
        if den == 0 {
            return Self::ZERO;
        }
        Self::reduced(
            u128::try_from(num).unwrap_or(u128::MAX),
            u128::try_from(den).unwrap_or(u128::MAX),
        )
    }

    const fn reduced(num: u128, den: u128) -> Self {
        let divisor = gcd(num, den);
        if divisor == 0 {
            return Self::ZERO;
        }
        Self {
            num: num / divisor,
            den: den / divisor,
        }
    }

    const fn add(self, other: Self) -> Self {
        Self::reduced(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    const fn mul(self, other: Self) -> Self {
        Self::reduced(self.num * other.num, self.den * other.den)
    }

    /// The value in units of `10^-9`, rounded to the nearest unit with a half
    /// rounded up, and whether the exact value lies further than `10^-5` of a
    /// unit from the midpoint between two units.
    fn units(self) -> (u64, bool) {
        let scaled = self.num * u128::from(COST_SCALE);
        let floor = scaled / self.den;
        let rem = scaled % self.den;
        let rounded = if 2 * rem >= self.den {
            floor + 1
        } else {
            floor
        };
        let gap = (2 * rem).abs_diff(self.den);
        let clear = gap * 100_000 > 2 * self.den;
        (u64::try_from(rounded).unwrap_or(u64::MAX), clear)
    }
}

/// The default weights of the four structural components: vertex name
/// `1/4`, edge name `1/4`, outgoing names `3/10`, out-degree `1/5`. The
/// alignment-evidence weight is zero.
const W_NAME: Ratio = Ratio { num: 1, den: 4 };
const W_EDGE: Ratio = Ratio { num: 1, den: 4 };
const W_PROP: Ratio = Ratio { num: 3, den: 10 };
const W_DEGREE: Ratio = Ratio { num: 1, den: 5 };

/// The Levenshtein distance between the bytes of `a` and of `b`: the least
/// number of byte insertions, deletions and substitutions turning one into
/// the other, by the full dynamic-programming table.
#[must_use]
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in table.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        table[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let substitute = table[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]);
            let delete = table[i - 1][j] + 1;
            let insert = table[i][j - 1] + 1;
            table[i][j] = substitute.min(delete).min(insert);
        }
    }
    table[a.len()][b.len()]
}

/// The target edge a source edge maps to when its endpoints map to `from`
/// and `to`: the target edge from `from` to `to` of the same kind and the
/// same name (an absent name matching an absent name) when there is one,
/// and otherwise the least target edge from `from` to `to` of the same kind.
/// `None` when no target edge from `from` to `to` has the kind.
#[must_use]
pub fn edge_image(target: &Schema, edge: &Edge, from: &str, to: &str) -> Option<Edge> {
    let candidates: Vec<&Edge> = target
        .edges
        .iter()
        .filter(|candidate| {
            candidate.src == from && candidate.tgt == to && candidate.kind == edge.kind
        })
        .collect();
    candidates
        .iter()
        .find(|candidate| candidate.name == edge.name)
        .or_else(|| candidates.iter().min())
        .map(|candidate| (*candidate).clone())
}

/// The cost terms of an assignment, one per source vertex followed by one
/// per source edge whose endpoints are both source vertices, in source
/// order; `None` when the assignment is not a feasible span assignment.
fn terms(source: &Schema, target: &Schema, assignment: &Assignment) -> Option<Vec<Ratio>> {
    if assignment.len() != source.vertices.len()
        || source
            .vertices
            .keys()
            .any(|id| !assignment.contains_key(id))
    {
        return None;
    }
    for (id, image) in assignment {
        if let Some(image) = image
            && target.vertices.get(image) != source.vertices.get(id)
        {
            return None;
        }
    }
    let mut out = Vec::new();
    for vertex in source.vertices.keys() {
        out.push(unary_term(
            source,
            target,
            vertex,
            assignment[vertex].as_deref(),
        ));
    }
    for edge in source.internal_edges() {
        out.push(edge_term(
            source,
            target,
            edge,
            assignment[&edge.src].as_deref(),
            assignment[&edge.tgt].as_deref(),
        )?);
    }
    Some(out)
}

/// The number of source vertices with at least one named outgoing edge.
fn named_class(source: &Schema) -> usize {
    source
        .vertices
        .keys()
        .filter(|vertex| !source.outgoing_names(vertex).is_empty())
        .count()
}

/// The vertex term of `vertex` mapped to `image` (`None` for a drop): the
/// weighted name, degree and outgoing-name dissimilarities, the first two
/// averaged over the source vertices and the third over the source vertices
/// with a named outgoing edge.
fn unary_term(source: &Schema, target: &Schema, vertex: &str, image: Option<&str>) -> Ratio {
    let vertices = source.vertices.len();
    let name = image.map_or(Ratio::of(1, 1), |image| {
        Ratio::of(
            edit_distance(vertex, image),
            vertex.len().max(image.len()).max(1),
        )
    });
    let degree = image.map_or(Ratio::of(1, 1), |image| {
        let (d, e) = (source.degree(vertex), target.degree(image));
        Ratio::of(d.abs_diff(e), d.max(e))
    });
    let own = source.outgoing_names(vertex);
    let prop = if own.is_empty() {
        Ratio::ZERO
    } else {
        image.map_or(Ratio::of(1, 1), |image| {
            let other = target.outgoing_names(image);
            let shared = own.intersection(&other).count();
            let union = own.union(&other).count();
            Ratio::of(union - shared, union)
        })
    };
    W_NAME
        .mul(name)
        .mul(Ratio::of(1, vertices))
        .add(W_DEGREE.mul(degree).mul(Ratio::of(1, vertices)))
        .add(W_PROP.mul(prop).mul(Ratio::of(1, named_class(source))))
}

/// The edge term of `edge` with endpoint images `from` and `to`: zero when
/// its image keeps the name, the edge weight over the number of source edges
/// when it does not or when an endpoint is dropped, and `None` (infeasible)
/// when both endpoints are mapped and no target edge of the kind joins them.
fn edge_term(
    source: &Schema,
    target: &Schema,
    edge: &Edge,
    from: Option<&str>,
    to: Option<&str>,
) -> Option<Ratio> {
    let share = W_EDGE.mul(Ratio::of(1, source.internal_edges().count()));
    let (Some(from), Some(to)) = (from, to) else {
        return Some(share);
    };
    let image = edge_image(target, edge, from, to)?;
    Some(if image.name == edge.name {
        Ratio::ZERO
    } else {
        share
    })
}

/// The cost of a span assignment: the sum of its terms, each rounded to the
/// nearest unit of `10^-9`, and its number of drops. `None` when the
/// assignment is not a feasible span assignment: a source vertex missing or
/// mapped to a target vertex that is absent or of another kind, or a source
/// edge with both endpoints mapped and no target edge of its kind between
/// their images.
#[must_use]
pub fn cost(source: &Schema, target: &Schema, assignment: &Assignment) -> Option<Cost> {
    let terms = terms(source, target, assignment)?;
    let quality_units = terms.iter().map(|term| term.units().0).sum();
    let drops = assignment.values().filter(|image| image.is_none()).count();
    let drops = u64::try_from(drops).unwrap_or(u64::MAX);
    Some(Cost {
        quality_units,
        drops,
    })
}

/// Whether every term any assignment of `source` into `target` can carry
/// lies further than `10^-5` of a unit from the midpoint between two units,
/// so that rounding it is insensitive to any error below that size.
#[must_use]
pub fn terms_clear_of_ties(source: &Schema, target: &Schema) -> bool {
    let share = W_EDGE.mul(Ratio::of(1, source.internal_edges().count()));
    let edge_clear = share.units().1;
    let vertex_clear = source.vertices.iter().all(|(vertex, kind)| {
        unary_term(source, target, vertex, None).units().1
            && target
                .vertices
                .iter()
                .filter(|(_, other)| *other == kind)
                .all(|(image, _)| unary_term(source, target, vertex, Some(image)).units().1)
    });
    edge_clear && vertex_clear
}

/// Every assignment of the source vertices to same-kind target vertices,
/// and to `None` as well when `allow_drop` holds, in lexicographic order of
/// the source vertices with each vertex's targets in id order and `None`
/// last.
#[must_use]
pub fn assignments(source: &Schema, target: &Schema, allow_drop: bool) -> Vec<Assignment> {
    let mut out = vec![Assignment::new()];
    for (vertex, kind) in &source.vertices {
        let mut choices: Vec<Option<String>> = target
            .vertices
            .iter()
            .filter(|(_, other)| *other == kind)
            .map(|(id, _)| Some(id.clone()))
            .collect();
        if allow_drop {
            choices.push(None);
        }
        out = out
            .into_iter()
            .flat_map(|partial| {
                choices.iter().map(move |choice| {
                    let mut grown = partial.clone();
                    grown.insert(vertex.clone(), choice.clone());
                    grown
                })
            })
            .collect();
    }
    out
}

/// The kinds of the edges from `from` to `to`, as a sorted list.
fn arc_kinds(schema: &Schema, from: &str, to: &str) -> Vec<String> {
    let mut kinds: Vec<String> = schema
        .edges
        .iter()
        .filter(|edge| edge.src == from && edge.tgt == to)
        .map(|edge| edge.kind.clone())
        .collect();
    kinds.sort();
    kinds
}

/// Whether an assignment meets the request: injective on its mapped vertices
/// under `monic` or `iso`, onto the target vertices under `epic`, and under
/// `iso` with the same multiset of edge kinds from `u` to `v` in the source
/// as from the image of `u` to the image of `v` in the target, for every
/// ordered pair of mapped vertices.
fn meets(target: &Schema, source: &Schema, assignment: &Assignment, request: Request) -> bool {
    let mapped: Vec<(&String, &String)> = assignment
        .iter()
        .filter_map(|(vertex, image)| image.as_ref().map(|image| (vertex, image)))
        .collect();
    let images: BTreeSet<&String> = mapped.iter().map(|(_, image)| *image).collect();
    if (request.monic || request.iso) && images.len() != mapped.len() {
        return false;
    }
    if request.epic && images.len() != target.vertices.len() {
        return false;
    }
    if request.iso {
        for (u, gu) in &mapped {
            for (v, gv) in &mapped {
                if arc_kinds(source, u, v) != arc_kinds(target, gu, gv) {
                    return false;
                }
            }
        }
    }
    true
}

/// The least cost of the feasible assignments a search ranges over and every
/// assignment attaining it, in the order of [`assignments`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Optimum {
    /// The least cost.
    pub cost: Cost,
    /// Every assignment of that cost.
    pub assignments: Vec<Assignment>,
}

/// The optimal total morphisms: among the total assignments that are
/// feasible and meet the request (under `iso`, also onto the target vertices
/// and with as many source as target edges), the least cost and every
/// assignment attaining it. `None` when there is no such assignment.
#[must_use]
pub fn optimal_total(source: &Schema, target: &Schema, request: Request) -> Option<Optimum> {
    let request = if request.iso {
        Request {
            epic: true,
            ..request
        }
    } else {
        request
    };
    optimum(
        source,
        target,
        assignments(source, target, false)
            .into_iter()
            .filter(|assignment| meets(target, source, assignment, request)),
    )
}

/// The optimal span assignments: among the assignments, drops allowed, that
/// are feasible and meet the request, the least cost and every assignment
/// attaining it. The all-drop assignment is always feasible, so a span
/// optimum always exists; `None` only for a request with `epic`, which no
/// span can promise.
#[must_use]
pub fn optimal_span(source: &Schema, target: &Schema, request: Request) -> Option<Optimum> {
    if request.epic {
        return None;
    }
    optimum(
        source,
        target,
        assignments(source, target, true)
            .into_iter()
            .filter(|assignment| meets(target, source, assignment, request)),
    )
}

fn optimum(
    source: &Schema,
    target: &Schema,
    candidates: impl Iterator<Item = Assignment>,
) -> Option<Optimum> {
    let mut best: Option<Optimum> = None;
    for assignment in candidates {
        let Some(cost) = cost(source, target, &assignment) else {
            continue;
        };
        match &mut best {
            Some(found) if found.cost < cost => {}
            Some(found) if found.cost == cost => found.assignments.push(assignment),
            _ => {
                best = Some(Optimum {
                    cost,
                    assignments: vec![assignment],
                });
            }
        }
    }
    best
}

/// Why a returned morphism or span leg is not what the request asked for,
/// in the order the checks run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Defect {
    /// The vertex map's domain is not the vertex set of the leg's domain.
    VertexDomain,
    /// A vertex maps to a vertex the target lacks or of another kind.
    VertexImage(String),
    /// Two mapped vertices share an image under an injective request.
    NotInjective,
    /// The image misses a target vertex under a surjective request.
    NotSurjective,
    /// The edge map's domain is not the edge set of the leg's domain.
    EdgeDomain,
    /// An edge's image is not a target edge of its kind between the images
    /// of its endpoints.
    EdgeImage(Edge),
    /// An edge's image drops a name that a target edge of its kind between
    /// the same images carries.
    NameNotKept(Edge),
    /// An edge's image is not the one [`edge_image`] chooses, or under `iso`
    /// the one [`paired_edge_images`] pairs it with.
    NotTheChosenImage(Edge),
    /// Two edges share an image under `iso`.
    EdgeMapNotInjective,
    /// The edge image misses a target edge under a total `iso` request.
    EdgeMapNotSurjective,
    /// Between two mapped vertices the source and target edge kinds differ
    /// under `iso`.
    ArcsNotReflected,
    /// The apex is not the sub-schema of the source induced on its vertices.
    ApexNotInduced,
    /// The left leg is not the identity on the apex.
    LeftNotInclusion,
}

/// Check a total morphism `(vertices, edges)` from `source` to `target`
/// against the two schemas and the request alone.
///
/// # Errors
///
/// The first [`Defect`] found, checking the vertex map, then injectivity
/// and surjectivity as requested, then each edge in source order, then under
/// `iso` the edge map's injectivity, surjectivity and the arc reflection.
pub fn check_total(
    source: &Schema,
    target: &Schema,
    request: Request,
    vertices: &VertexMap,
    edges: &EdgeMap,
) -> Result<(), Defect> {
    let request = if request.iso {
        Request {
            epic: true,
            ..request
        }
    } else {
        request
    };
    check_leg(source, target, request, vertices, edges)?;
    if request.iso && edges.values().collect::<BTreeSet<_>>().len() != target.edges.len() {
        return Err(Defect::EdgeMapNotSurjective);
    }
    Ok(())
}

/// Check a span `source <- apex -> target` given as the apex, the left
/// leg's vertex and edge maps, and the right leg's vertex and edge maps.
///
/// # Errors
///
/// [`Defect::ApexNotInduced`] when the apex is not the sub-schema of the
/// source induced on the apex vertices, [`Defect::LeftNotInclusion`] when the
/// left leg is not the identity on the apex, and otherwise the first defect
/// of the right leg as [`check_total`] orders them, the right leg being
/// checked as a morphism out of the apex.
pub fn check_span(
    source: &Schema,
    target: &Schema,
    request: Request,
    apex: &Schema,
    left: (&VertexMap, &EdgeMap),
    right: (&VertexMap, &EdgeMap),
) -> Result<(), Defect> {
    let kept: BTreeSet<String> = apex.vertices.keys().cloned().collect();
    if kept.iter().any(|id| !source.vertices.contains_key(id)) || source.induced(&kept) != *apex {
        return Err(Defect::ApexNotInduced);
    }
    let identity_vertices: VertexMap = kept.iter().map(|id| (id.clone(), id.clone())).collect();
    let identity_edges: EdgeMap = apex
        .edges
        .iter()
        .map(|edge| (edge.clone(), edge.clone()))
        .collect();
    if *left.0 != identity_vertices || *left.1 != identity_edges {
        return Err(Defect::LeftNotInclusion);
    }
    check_leg(apex, target, request, right.0, right.1)
}

/// Check one leg out of `domain` into `target`.
fn check_leg(
    domain: &Schema,
    target: &Schema,
    request: Request,
    vertices: &VertexMap,
    edges: &EdgeMap,
) -> Result<(), Defect> {
    if vertices.keys().ne(domain.vertices.keys()) {
        return Err(Defect::VertexDomain);
    }
    for (vertex, image) in vertices {
        if target.vertices.get(image) != domain.vertices.get(vertex) {
            return Err(Defect::VertexImage(vertex.clone()));
        }
    }
    let assignment: Assignment = vertices
        .iter()
        .map(|(vertex, image)| (vertex.clone(), Some(image.clone())))
        .collect();
    let images: BTreeSet<&String> = vertices.values().collect();
    if (request.monic || request.iso) && images.len() != vertices.len() {
        return Err(Defect::NotInjective);
    }
    if request.epic && images.len() != target.vertices.len() {
        return Err(Defect::NotSurjective);
    }
    if edges.keys().ne(domain.internal_edges()) {
        return Err(Defect::EdgeDomain);
    }
    let paired = request
        .iso
        .then(|| paired_edge_images(domain, target, vertices));
    for (edge, image) in edges {
        let (from, to) = (&vertices[&edge.src], &vertices[&edge.tgt]);
        if !target.edges.contains(image)
            || image.src != *from
            || image.tgt != *to
            || image.kind != edge.kind
        {
            return Err(Defect::EdgeImage(edge.clone()));
        }
        let named_available = target.edges.iter().any(|candidate| {
            candidate.src == *from
                && candidate.tgt == *to
                && candidate.kind == edge.kind
                && candidate.name == edge.name
        });
        if named_available && image.name != edge.name {
            return Err(Defect::NameNotKept(edge.clone()));
        }
        let chosen = paired.as_ref().map_or_else(
            || edge_image(target, edge, from, to),
            |paired| paired.get(edge).cloned(),
        );
        if chosen.as_ref() != Some(image) {
            return Err(Defect::NotTheChosenImage(edge.clone()));
        }
    }
    if request.iso {
        if edges.values().collect::<BTreeSet<_>>().len() != edges.len() {
            return Err(Defect::EdgeMapNotInjective);
        }
        if !meets(target, domain, &assignment, request) {
            return Err(Defect::ArcsNotReflected);
        }
    }
    Ok(())
}

/// The edge images an isomorphism request pairs the edges of `domain` with,
/// under the vertex map `vertices`: within each ordered pair of images, every
/// edge that a target edge of its kind and name joins the images is paired
/// with that edge, and then the remaining edges, least first, are each paired
/// with the least remaining target edge of their kind. An edge left without a
/// partner is absent from the map.
#[must_use]
pub fn paired_edge_images(domain: &Schema, target: &Schema, vertices: &VertexMap) -> EdgeMap {
    let mut groups: BTreeMap<(&str, &str), Vec<&Edge>> = BTreeMap::new();
    for edge in domain.internal_edges() {
        if let (Some(from), Some(to)) = (vertices.get(&edge.src), vertices.get(&edge.tgt)) {
            groups
                .entry((from.as_str(), to.as_str()))
                .or_default()
                .push(edge);
        }
    }
    let mut out = EdgeMap::new();
    for ((from, to), sources) in groups {
        let mut free: Vec<&Edge> = target
            .edges
            .iter()
            .filter(|candidate| candidate.src == from && candidate.tgt == to)
            .collect();
        let mut rest = Vec::new();
        for edge in sources {
            match free
                .iter()
                .position(|candidate| candidate.kind == edge.kind && candidate.name == edge.name)
            {
                Some(at) => {
                    out.insert(edge.clone(), free.remove(at).clone());
                }
                None => rest.push(edge),
            }
        }
        for edge in rest {
            if let Some(at) = free
                .iter()
                .position(|candidate| candidate.kind == edge.kind)
            {
                out.insert(edge.clone(), free.remove(at).clone());
            }
        }
    }
    out
}

/// The 64-bit FNV-1a digest of `bytes`: the offset basis
/// `0xcbf29ce484222325`, and for each byte an exclusive or followed by a
/// multiplication by the prime `0x100000001b3` modulo `2^64`.
#[must_use]
pub fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
