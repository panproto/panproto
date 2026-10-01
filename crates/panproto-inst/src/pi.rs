//! Right Kan extension (`Pi_F`) for instances.
//!
//! For set-valued functor instances the right Kan extension at a target
//! vertex is the limit of the instance over the comma category under that
//! vertex: the product of the fibre's tables, equalised along the arrows
//! inside the fibre and cut down and multiplied out along every path out of
//! the vertex. For W-type instances, only vertex-injective migrations are
//! supported; a non-injective map is rejected rather than silently
//! producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator, same_value};
use crate::instance_hom::FInstanceHom;
use crate::metadata::Node;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// Right Kan extension (`Pi_F`) for set-valued functor instances.
///
/// The first projection of [`functor_pi_with_hom`]; see there.
///
/// # Errors
///
/// As [`functor_pi_with_hom`].
pub fn functor_pi(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<FInstance, RestrictError> {
    functor_pi_with_hom(
        instance,
        src_schema,
        tgt_schema,
        migration,
        max_product_size,
    )
    .map(|(pi, _)| pi)
}

/// Right Kan extension (`Pi_F`) for set-valued functor instances, with the
/// projection of every row of the result onto each fibre component.
///
/// `F` is the functor the migration states between the free categories on
/// the two schemas: it sends each source vertex (read as every table
/// operation reads them; see [`crate::functor_restrict`]) to its image, each
/// source edge the migration carries to its image edge, and each other edge
/// of `src_schema` with both endpoints in one fibre (an internal arrow) to
/// the identity of the target vertex. The table of a target vertex `d` is
/// the limit of the instance over the comma category `d/F`: its objects are
/// the pairs `(c, p)` of a source vertex `c` and a path `p` of `tgt_schema`
/// from `d` to `F(c)`, and its arrows are the arrows of `F`'s domain between
/// them. A row of the result is a cone: a row of `c` at each object `(c, p)`
/// such that every arrow relates the rows at its endpoints. The fibre of `d`
/// is the objects at the empty path, so the limit is the product of the
/// fibre's tables equalised along the internal arrows, then cut down and
/// multiplied out along every path out of `d`: a carried edge relates the
/// rows at its two ends (two carried edges meeting in one fibre take the
/// pullback, two merged parallel edges the equaliser), and a target edge no
/// source edge reaches makes the fibre at its end a further factor.
///
/// Every foreign key is read as a relation, so the cones of an instance
/// whose carried and internal foreign keys are total functions are exactly
/// the limit, and a row with no pair on an edge the limit follows from it
/// has no cone. A vertex none of whose fibre components holds a row is read
/// as an attribute (a column vertex), as a vertex outside the image is: it
/// constrains nothing, and the paths through it continue. The schema does
/// not say which vertices hold tables, so the result has a table only at the
/// image vertices whose fibre holds rows; a vertex outside the image of `F`
/// gets none.
///
/// The cones are counted on the finite graph of fibre assignments: a cone is
/// a choice, along each target edge out of each vertex it passes, of one
/// assignment at the edge's end that the carried edges relate to the one
/// before. Where every such choice is forced, the cones along a cycle of
/// `tgt_schema` are fixed by the row they enter it with, so a cycle adds no
/// cone, though it may remove some; where a choice along some cycle is free (a target loop with no
/// preimage over a fibre of two rows, say), every pass around the cycle
/// chooses again and the limit is an infinite product.
///
/// Row ids: when the fibre of `d` is one component and every cone at `d` is
/// fixed by its row of that component, each row of the result keeps the id
/// of that row (the projection is injective, so the limit is a subset of the
/// component's table); every other row of the result is fresh, allocated
/// from the least id above every row of the input, the target vertices taken
/// in ascending name, each target's cones in the product's order
/// (components in ascending name, the last varying fastest) and the cones
/// that agree on the fibre in the order of the target edges out of `d`
/// (ascending), the last varying fastest.
///
/// Foreign keys: a target edge `E: d -> d'` between two vertices with tables
/// relates each cone at `d` to the cone at `d'` it restricts to, the rows at
/// the objects `(c, E p)`; it is a function on the rows of `d`. The pairs of
/// an edge outside `F`'s domain are not carried.
///
/// The returned map sends each source vertex `s` of a fibre to the
/// projection of its target's rows onto the rows of `s`, as an
/// [`FInstanceHom`] whose table map sends the target table `F(s)` to `s`.
///
/// # Errors
///
/// Returns [`RestrictError::NoEdgeFound`] when the migration carries an edge
/// of `src_schema` to an edge `tgt_schema` does not have,
/// [`RestrictError::ProductSizeExceeded`] when the product of the tables of
/// a fibre of several components exceeds `max_product_size`, when the cones
/// at a vertex are more than `max_product_size` and more than the fibre rows
/// they extend, and, with `actual` saturated to `usize::MAX` as an
/// overflowing count is, when the limit at a vertex is infinite;
/// [`RestrictError::AttributeCollision`] when two components of one cone
/// disagree on a shared column, and [`RestrictError::RowIdsExhausted`] when
/// the fresh row ids run out. The fibre products are checked first, then
/// each target vertex's limit and cells, the vertices in ascending name, and
/// the first failure is returned.
pub fn functor_pi_with_hom(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    let edge_images = crate::adjunction::edge_images(migration);
    check_carried_edges(src_schema, tgt_schema, &edge_images)?;
    let fiber_map = build_fiber_map(migration);
    let comma = Comma::build(
        instance,
        src_schema,
        tgt_schema,
        &edge_images,
        &fiber_map,
        max_product_size,
    )?;

    // Check every target vertex before creating any row, so the first
    // failure in ascending name is the one returned.
    let mut roots: Vec<(usize, Vec<Row>)> = Vec::new();
    for (index, layer) in comma.layers.iter().enumerate() {
        if layer.is_table {
            comma.check_finite(index, max_product_size)?;
            let rows = comma
                .alive_states(index)
                .map(|state| merged_row(instance, layer, &layer.states[state]))
                .collect::<Result<Vec<Row>, RestrictError>>()?;
            roots.push((index, rows));
        }
    }
    let mut enumeration = Enumeration::new(&comma);

    let mut allocator = row_allocator(instance.row_ids());
    let mut out = FInstance::new();
    let mut projections: BTreeMap<Name, FInstanceHom> = BTreeMap::new();
    let mut row_of: HashMap<ExtId, RowId> = HashMap::new();
    let mut placed: Vec<(usize, Vec<(RowId, ExtId)>)> = Vec::with_capacity(roots.len());
    for (index, merged) in roots {
        let layer = &comma.layers[index];
        let cones: Vec<(usize, Row, Vec<ExtId>)> = comma
            .alive_states(index)
            .zip(merged)
            .map(|(state, row)| (state, row, enumeration.cones(index, state)))
            .collect();
        let keeps_ids =
            layer.components.len() == 1 && cones.iter().all(|(_, _, exts)| exts.len() == 1);
        let mut rows = Vec::new();
        for (state, row, exts) in cones {
            let components = &layer.states[state];
            for ext in exts {
                let id = if keeps_ids {
                    components[0]
                } else {
                    allocate_row(&mut allocator).map_err(|_| RestrictError::RowIdsExhausted)?
                };
                for (component, &src_row) in layer.components.iter().zip(components) {
                    projections
                        .entry(component.clone())
                        .or_default()
                        .row_map
                        .insert(id, src_row);
                }
                out.place_row(layer.vertex.clone(), id, row.clone());
                row_of.insert(ext, id);
                rows.push((id, ext));
            }
        }
        placed.push((index, rows));
    }

    for (index, rows) in &placed {
        let layer = &comma.layers[*index];
        for (slot_index, slot) in layer.slots.iter().enumerate() {
            if !comma.layers[slot.target].is_table {
                continue;
            }
            for &(p, ext) in rows {
                let child = enumeration.child(ext, slot_index);
                if let Some(&q) = child.and_then(|child| row_of.get(&child)) {
                    out.place_pair(slot.edge.clone(), p, q);
                }
            }
        }
    }

    let fibre_of: HashMap<&Name, &Name> = fiber_map
        .iter()
        .flat_map(|(tgt, fibre)| fibre.iter().map(move |src| (src, tgt)))
        .collect();
    let projections = projections
        .into_iter()
        .map(|(src, hom)| {
            let table_map = fibre_of
                .get(&src)
                .map(|&tgt| (tgt.clone(), src.clone()))
                .into_iter()
                .collect();
            (src, hom.with_table_map(table_map))
        })
        .collect();

    Ok((out, projections))
}

/// The row of a cone at its root layer: the union of the cells of its fibre
/// components.
///
/// # Errors
///
/// Returns [`RestrictError::AttributeCollision`] when two components hold
/// the same column with different values (by canonical bytes), which the
/// union would otherwise settle by overwriting one with the other.
fn merged_row(
    instance: &FInstance,
    layer: &Layer,
    components: &[RowId],
) -> Result<Row, RestrictError> {
    let mut merged = Row::new();
    for &id in components {
        for (col, val) in instance.row(id).into_iter().flatten() {
            if let Some(prior) = merged.get(col) {
                if !same_value(prior, val) {
                    return Err(RestrictError::AttributeCollision {
                        vertex: layer.vertex.to_string(),
                        column: col.clone(),
                    });
                }
            }
            merged.insert(col.clone(), val.clone());
        }
    }
    Ok(merged)
}

/// The choices a cone has at the end of one target edge: the states of the
/// target layer that the carried edges relate to each state of the source
/// layer, or every state of the target layer when no carried edge constrains
/// the edge.
enum Choices {
    /// Every state of the target layer.
    All,
    /// For each state of the source layer, the states of the target layer
    /// its carried edges relate it to, ascending.
    Lists(Vec<Vec<usize>>),
}

/// One target edge out of a layer, with the layer at its end.
struct Slot {
    /// The target edge.
    edge: Edge,
    /// The index of the layer at the edge's end.
    target: usize,
    /// The states of that layer each state of this one may be sent to.
    choices: Choices,
}

/// The objects of the comma category at one path, grouped by the target
/// vertex the path ends at: the fibre components of that vertex, and the
/// states, each an assignment of a row to every component.
struct Layer {
    /// The target vertex.
    vertex: Name,
    /// Whether the result has a table here: the vertex is in the image of
    /// `F` and some fibre component holds a row.
    is_table: bool,
    /// The fibre components the layer draws rows from, ascending; empty for
    /// a vertex outside the image or one read as an attribute.
    components: Vec<Name>,
    /// The assignments of a row to every component that the internal
    /// arrows relate, in the product's order; one empty assignment when
    /// there are no components.
    states: Vec<Vec<RowId>>,
    /// The target edges out of the vertex, ascending.
    slots: Vec<Slot>,
}

/// The comma categories `d/F` of every target vertex `d` at once, folded
/// into one finite graph: a layer per target vertex reachable from the
/// image, an edge between states for each pair of states a target edge may
/// relate, and the fixpoints that say which states extend to a cone along
/// every path, which extend in exactly one way, and over which the cones
/// are finite.
struct Comma {
    /// The layers, in ascending vertex name.
    layers: Vec<Layer>,
    /// Whether each state extends to a cone along every path out of it.
    alive: Vec<Vec<bool>>,
    /// Whether each live state extends in more than one way.
    branching: Vec<Vec<bool>>,
    /// Whether each live branching state has finitely many extensions.
    finite: Vec<Vec<bool>>,
    /// The number of extensions of each live state, saturated at
    /// `usize::MAX`; one for a state that does not branch.
    counts: Vec<Vec<usize>>,
}

/// A reference to a state: its layer and its index in the layer.
type StateRef = (usize, usize);

impl Comma {
    /// Build the layers, their states and slots, and solve the fixpoints.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError::ProductSizeExceeded`] when the product of the
    /// tables of a fibre of several components exceeds `max_product_size`,
    /// the layers taken in ascending vertex name.
    fn build(
        instance: &FInstance,
        src_schema: &Schema,
        tgt_schema: &Schema,
        edge_images: &HashMap<Edge, Edge>,
        fiber_map: &BTreeMap<Name, Vec<Name>>,
        max_product_size: usize,
    ) -> Result<Self, RestrictError> {
        // The target edges out of each target vertex, ascending.
        let mut out_edges: HashMap<&Name, Vec<&Edge>> = HashMap::new();
        for edge in tgt_schema.edges.keys() {
            out_edges.entry(&edge.src).or_default().push(edge);
        }
        for edges in out_edges.values_mut() {
            edges.sort_unstable();
        }
        // Every target vertex reachable from the image, ascending.
        let mut vertices: BTreeSet<&Name> = fiber_map.keys().collect();
        let mut frontier: Vec<&Name> = vertices.iter().copied().collect();
        while let Some(vertex) = frontier.pop() {
            for edge in out_edges.get(vertex).into_iter().flatten() {
                if vertices.insert(&edge.tgt) {
                    frontier.push(&edge.tgt);
                }
            }
        }
        let index_of: HashMap<&Name, usize> =
            vertices.iter().enumerate().map(|(i, &v)| (v, i)).collect();

        let mut layers: Vec<Layer> = Vec::with_capacity(vertices.len());
        for &vertex in &vertices {
            let fibre = fiber_map.get(vertex);
            let holds_rows = fibre.is_some_and(|f| f.iter().any(|c| instance.has_table(c)));
            let components: Vec<Name> = if holds_rows {
                fibre.cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            let internal = internal_arrows(src_schema, edge_images, &components);
            let states = fibre_states(instance, vertex, &components, &internal, max_product_size)?;
            layers.push(Layer {
                vertex: vertex.clone(),
                is_table: holds_rows,
                components,
                states,
                slots: Vec::new(),
            });
        }

        // The source edges carried to each target edge, ascending, whose
        // endpoints lie in the fibres of its endpoints.
        let fibre_of: HashMap<&Name, &Name> = fiber_map
            .iter()
            .flat_map(|(tgt, fibre)| fibre.iter().map(move |src| (src, tgt)))
            .collect();
        let mut carried: HashMap<&Edge, Vec<&Edge>> = HashMap::new();
        for (edge, image) in edge_images {
            if fibre_of.get(&edge.src) == Some(&&image.src)
                && fibre_of.get(&edge.tgt) == Some(&&image.tgt)
            {
                carried.entry(image).or_default().push(edge);
            }
        }
        for edges in carried.values_mut() {
            edges.sort_unstable();
        }

        // Index each layer's states by the row of each component.
        let by_row: Vec<Vec<HashMap<RowId, Vec<usize>>>> = layers
            .iter()
            .map(|layer| {
                let mut index: Vec<HashMap<RowId, Vec<usize>>> =
                    vec![HashMap::new(); layer.components.len()];
                for (state, rows) in layer.states.iter().enumerate() {
                    for (position, row) in rows.iter().enumerate() {
                        index[position].entry(*row).or_default().push(state);
                    }
                }
                index
            })
            .collect();

        for (from, &vertex) in vertices.iter().enumerate() {
            let mut slots = Vec::new();
            for &edge in out_edges.get(vertex).into_iter().flatten() {
                let target = index_of[&edge.tgt];
                let choices = slot_choices(
                    instance,
                    &layers[from],
                    &layers[target],
                    &by_row[target],
                    carried.get(edge).map_or(&[][..], Vec::as_slice),
                );
                slots.push(Slot {
                    edge: edge.clone(),
                    target,
                    choices,
                });
            }
            layers[from].slots = slots;
        }

        let mut comma = Self {
            alive: layers.iter().map(|l| vec![true; l.states.len()]).collect(),
            branching: layers.iter().map(|l| vec![false; l.states.len()]).collect(),
            finite: layers.iter().map(|l| vec![true; l.states.len()]).collect(),
            counts: layers.iter().map(|l| vec![1; l.states.len()]).collect(),
            layers,
        };
        comma.solve();
        Ok(comma)
    }

    /// The live states of a layer, ascending.
    fn alive_states(&self, layer: usize) -> impl Iterator<Item = usize> + '_ {
        self.alive[layer]
            .iter()
            .enumerate()
            .filter(|(_, alive)| **alive)
            .map(|(state, _)| state)
    }

    /// The live states a live state may be sent to along a slot, ascending.
    fn live_choices(&self, (layer, state): StateRef, slot: usize) -> Vec<usize> {
        let slot = &self.layers[layer].slots[slot];
        match &slot.choices {
            Choices::All => self.alive_states(slot.target).collect(),
            Choices::Lists(lists) => lists[state]
                .iter()
                .copied()
                .filter(|&c| self.alive[slot.target][c])
                .collect(),
        }
    }

    /// Refuse a target vertex whose limit is infinite or too large.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError::ProductSizeExceeded`] with `actual` at
    /// `usize::MAX` when some live state at the vertex has infinitely many
    /// extensions, and with the number of cones when they are more than
    /// `max_product_size` and more than the live states they extend.
    fn check_finite(&self, layer: usize, max_product_size: usize) -> Result<(), RestrictError> {
        let vertex = self.layers[layer].vertex.to_string();
        let mut total: usize = 0;
        let mut states: usize = 0;
        for state in self.alive_states(layer) {
            if !self.finite[layer][state] {
                return Err(RestrictError::ProductSizeExceeded {
                    vertex,
                    actual: usize::MAX,
                    limit: max_product_size,
                });
            }
            total = total.saturating_add(self.counts[layer][state]);
            states += 1;
        }
        if total > max_product_size && total > states {
            return Err(RestrictError::ProductSizeExceeded {
                vertex,
                actual: total,
                limit: max_product_size,
            });
        }
        Ok(())
    }
}

/// The states of a layer: the assignments of a row to every component that
/// the internal arrows relate, in the product's order (components in the
/// order given, the last varying fastest). With no components there is one
/// empty assignment.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] when the layer has several
/// components and the product of their tables exceeds `max_product_size`.
fn fibre_states(
    instance: &FInstance,
    vertex: &Name,
    components: &[Name],
    internal: &[&Edge],
    max_product_size: usize,
) -> Result<Vec<Vec<RowId>>, RestrictError> {
    let tables: Vec<Vec<RowId>> = components
        .iter()
        .map(|c| instance.rows(c).into_iter().map(|(id, _)| id).collect())
        .collect();
    if tables.len() > 1 {
        let product_size = tables
            .iter()
            .try_fold(1_usize, |acc, rows| acc.checked_mul(rows.len()))
            .unwrap_or(usize::MAX);
        if product_size > max_product_size {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: vertex.to_string(),
                actual: product_size,
                limit: max_product_size,
            });
        }
    }
    let position: HashMap<&Name, usize> =
        components.iter().enumerate().map(|(i, c)| (c, i)).collect();
    let mut states: Vec<Vec<RowId>> = vec![Vec::new()];
    for rows in &tables {
        let mut next = Vec::with_capacity(states.len() * rows.len());
        for state in &states {
            for &row in rows {
                let mut extended = state.clone();
                extended.push(row);
                next.push(extended);
            }
        }
        states = next;
    }
    states.retain(|state| {
        internal.iter().all(|edge| {
            instance.contains_pair(edge, state[position[&edge.src]], state[position[&edge.tgt]])
        })
    });
    Ok(states)
}

/// The choices along one target edge from layer `from` to layer `to`: for
/// each state of `from`, the states of `to` that every carried edge relates
/// it to, read through `to_by_row` (the states of `to` holding each row of
/// each component). With no carried edge between the two layers' components
/// every state of `to` is a choice.
fn slot_choices(
    instance: &FInstance,
    from: &Layer,
    to: &Layer,
    to_by_row: &[HashMap<RowId, Vec<usize>>],
    carried: &[&Edge],
) -> Choices {
    let position = |layer: &Layer, vertex: &Name| layer.components.iter().position(|c| c == vertex);
    let constraints: Vec<(&Edge, usize, usize)> = carried
        .iter()
        .filter_map(|&edge| Some((edge, position(from, &edge.src)?, position(to, &edge.tgt)?)))
        .collect();
    let Some((&(first, src_at, tgt_at), rest)) = constraints.split_first() else {
        return Choices::All;
    };
    let lists = from
        .states
        .iter()
        .map(|state| {
            let mut choices: Vec<usize> = instance
                .targets_of(first, state[src_at])
                .flat_map(|row| to_by_row[tgt_at].get(&row).into_iter().flatten().copied())
                .filter(|&c| {
                    let candidate = &to.states[c];
                    rest.iter()
                        .all(|&(edge, s, t)| instance.contains_pair(edge, state[s], candidate[t]))
                })
                .collect();
            choices.sort_unstable();
            choices
        })
        .collect();
    Choices::Lists(lists)
}

impl Comma {
    /// Solve the fixpoints over the layers.
    ///
    /// A state is live when along every slot it has a live choice: the
    /// greatest such set, so a state whose every path stays inside live
    /// states extends to a cone however long the path. Since every layer is
    /// finite, a live state extends to a cone along every path out of it
    /// (Konig's lemma along the finitely many choices at each step). A live
    /// state branches when along some slot it has two live choices or its
    /// one live choice branches; a state that does not branch has exactly
    /// one extension. The extensions of a branching state are finite exactly
    /// when the branching states reachable from it form no cycle, and are
    /// then counted children first.
    fn solve(&mut self) {
        // Reverse indexes: the parents of each state along list slots, and
        // the (layer, slot) pairs whose `All` slot ends at each layer.
        let mut parents: Vec<Vec<Vec<(usize, usize, usize)>>> = self
            .layers
            .iter()
            .map(|l| vec![Vec::new(); l.states.len()])
            .collect();
        let mut all_parents: Vec<Vec<(usize, usize)>> = vec![Vec::new(); self.layers.len()];
        // The live choices of each state along each list slot.
        let mut live: Vec<Vec<Vec<usize>>> = Vec::with_capacity(self.layers.len());
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let mut per_slot = Vec::with_capacity(layer.slots.len());
            for (slot_index, slot) in layer.slots.iter().enumerate() {
                match &slot.choices {
                    Choices::All => {
                        all_parents[slot.target].push((layer_index, slot_index));
                        per_slot.push(Vec::new());
                    }
                    Choices::Lists(lists) => {
                        for (state, choices) in lists.iter().enumerate() {
                            for &c in choices {
                                parents[slot.target][c].push((layer_index, slot_index, state));
                            }
                        }
                        per_slot.push(lists.iter().map(Vec::len).collect());
                    }
                }
            }
            live.push(per_slot);
        }
        let mut alive_count: Vec<usize> = self.layers.iter().map(|l| l.states.len()).collect();

        // Liveness: kill every state with a slot without a live choice, and
        // propagate to its parents.
        let mut dead: Vec<StateRef> = Vec::new();
        for (layer_index, layer) in self.layers.iter().enumerate() {
            for (slot_index, slot) in layer.slots.iter().enumerate() {
                match &slot.choices {
                    Choices::All => {
                        if self.layers[slot.target].states.is_empty() {
                            dead.extend((0..layer.states.len()).map(|s| (layer_index, s)));
                        }
                    }
                    Choices::Lists(_) => dead.extend(
                        live[layer_index][slot_index]
                            .iter()
                            .enumerate()
                            .filter(|(_, count)| **count == 0)
                            .map(|(s, _)| (layer_index, s)),
                    ),
                }
            }
        }
        let mut emptied = vec![false; self.layers.len()];
        while let Some((layer, state)) = dead.pop() {
            if !std::mem::replace(&mut self.alive[layer][state], false) {
                continue;
            }
            alive_count[layer] -= 1;
            for &(parent_layer, slot, parent) in &parents[layer][state] {
                let count = &mut live[parent_layer][slot][parent];
                *count -= 1;
                if *count == 0 {
                    dead.push((parent_layer, parent));
                }
            }
            if alive_count[layer] == 0 && !std::mem::replace(&mut emptied[layer], true) {
                for &(parent_layer, _) in &all_parents[layer] {
                    for parent in 0..self.layers[parent_layer].states.len() {
                        dead.push((parent_layer, parent));
                    }
                }
            }
        }
        self.solve_branching(&parents, &all_parents, &live, &alive_count);
        self.solve_counts(&parents, &all_parents);
    }
}

/// The parents of each state along list slots: `(layer, slot, state)`.
type Parents = [Vec<Vec<(usize, usize, usize)>>];

impl Comma {
    /// Mark the live states that branch: those with two live choices along
    /// some slot, then every live parent of a branching state (its one live
    /// choice there branches, or it already had two).
    fn solve_branching(
        &mut self,
        parents: &Parents,
        all_parents: &[Vec<(usize, usize)>],
        live: &[Vec<Vec<usize>>],
        alive_count: &[usize],
    ) {
        let mut queue: Vec<StateRef> = Vec::new();
        for (layer_index, layer) in self.layers.iter().enumerate() {
            for (state, &alive) in self.alive[layer_index].iter().enumerate() {
                if !alive {
                    continue;
                }
                let branches = layer
                    .slots
                    .iter()
                    .enumerate()
                    .any(|(slot_index, slot)| match slot.choices {
                        Choices::All => alive_count[slot.target] >= 2,
                        Choices::Lists(_) => live[layer_index][slot_index][state] >= 2,
                    });
                if branches {
                    self.branching[layer_index][state] = true;
                    queue.push((layer_index, state));
                }
            }
        }
        let mut marked_all = vec![false; self.layers.len()];
        while let Some((layer, state)) = queue.pop() {
            for &(parent_layer, _, parent) in &parents[layer][state] {
                let p = parent;
                if self.alive[parent_layer][p] && !self.branching[parent_layer][p] {
                    self.branching[parent_layer][p] = true;
                    queue.push((parent_layer, parent));
                }
            }
            for &(parent_layer, _) in &all_parents[layer] {
                if std::mem::replace(&mut marked_all[parent_layer], true) {
                    continue;
                }
                for p in 0..self.layers[parent_layer].states.len() {
                    if self.alive[parent_layer][p] && !self.branching[parent_layer][p] {
                        self.branching[parent_layer][p] = true;
                        queue.push((parent_layer, p));
                    }
                }
            }
        }
    }

    /// Count the extensions of every live branching state, children first,
    /// and mark finite exactly the ones counted: a branching state waits on
    /// its branching live choices, so one that reaches a cycle of branching
    /// states is never counted and has infinitely many extensions.
    fn solve_counts(&mut self, parents: &Parents, all_parents: &[Vec<(usize, usize)>]) {
        let is_branching = |comma: &Self, (layer, state): StateRef| {
            comma.alive[layer][state] && comma.branching[layer][state]
        };
        // Branching live states per layer, still uncounted.
        let mut uncounted: Vec<usize> = (0..self.layers.len())
            .map(|layer| {
                (0..self.layers[layer].states.len())
                    .filter(|&s| self.alive[layer][s] && self.branching[layer][s])
                    .count()
            })
            .collect();
        let mut pending: Vec<Vec<usize>> = self
            .layers
            .iter()
            .map(|l| vec![0; l.states.len()])
            .collect();
        let mut ready: Vec<StateRef> = Vec::new();
        for (layer_index, layer) in self.layers.iter().enumerate() {
            for (state, pending_here) in pending[layer_index].iter_mut().enumerate() {
                let here = (layer_index, state);
                if !is_branching(self, here) {
                    continue;
                }
                let mut waits = 0;
                for (slot_index, slot) in layer.slots.iter().enumerate() {
                    match &slot.choices {
                        Choices::All => waits += usize::from(uncounted[slot.target] > 0),
                        Choices::Lists(_) => {
                            waits += self
                                .live_choices(here, slot_index)
                                .into_iter()
                                .filter(|&c| is_branching(self, (slot.target, c)))
                                .count();
                        }
                    }
                }
                *pending_here = waits;
                self.finite[layer_index][state] = false;
                if waits == 0 {
                    ready.push(here);
                }
            }
        }
        let mut layer_sums: Vec<Option<usize>> = vec![None; self.layers.len()];
        while let Some((layer, state)) = ready.pop() {
            let mut count: usize = 1;
            for slot_index in 0..self.layers[layer].slots.len() {
                let slot = &self.layers[layer].slots[slot_index];
                let sum = match &slot.choices {
                    Choices::All => {
                        let target = slot.target;
                        *layer_sums[target].get_or_insert_with(|| {
                            self.alive_states(target)
                                .fold(0_usize, |acc, c| acc.saturating_add(self.counts[target][c]))
                        })
                    }
                    Choices::Lists(_) => self
                        .live_choices((layer, state), slot_index)
                        .into_iter()
                        .fold(0_usize, |acc, c| {
                            acc.saturating_add(self.counts[slot.target][c])
                        }),
                };
                count = count.saturating_mul(sum);
            }
            self.counts[layer][state] = count;
            self.finite[layer][state] = true;

            for &(parent_layer, _, parent) in &parents[layer][state] {
                if is_branching(self, (parent_layer, parent)) {
                    let waits = &mut pending[parent_layer][parent];
                    *waits -= 1;
                    if *waits == 0 {
                        ready.push((parent_layer, parent));
                    }
                }
            }
            uncounted[layer] -= 1;
            if uncounted[layer] == 0 {
                for &(parent_layer, _) in &all_parents[layer] {
                    for (parent, waits) in pending[parent_layer].iter_mut().enumerate() {
                        let here = (parent_layer, parent);
                        if is_branching(self, here) {
                            *waits -= 1;
                            if *waits == 0 {
                                ready.push(here);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An extension of a state to a cone along every path out of it.
type ExtId = usize;

/// The cones of the comma categories, built on demand: each extension is
/// its state and, for a branching state, the extension chosen along each
/// slot; a state that does not branch has one extension, which stores no
/// choices since each is its one live choice's.
struct Enumeration<'a> {
    /// The solved comma categories.
    comma: &'a Comma,
    /// Each extension: its state and its choices, one per slot (empty when
    /// the state does not branch).
    arena: Vec<(StateRef, Box<[ExtId]>)>,
    /// The extensions of each state built so far, in enumeration order.
    memo: HashMap<StateRef, Vec<ExtId>>,
}

impl<'a> Enumeration<'a> {
    /// An empty enumeration over `comma`.
    fn new(comma: &'a Comma) -> Self {
        Self {
            comma,
            arena: Vec::new(),
            memo: HashMap::new(),
        }
    }

    /// Record a new extension.
    fn push(&mut self, state: StateRef, choices: Box<[ExtId]>) -> ExtId {
        let id = self.arena.len();
        self.arena.push((state, choices));
        id
    }

    /// The one extension of a live state that does not branch.
    fn leaf(&mut self, state: StateRef) -> ExtId {
        if let Some(&id) = self.memo.get(&state).and_then(|exts| exts.first()) {
            return id;
        }
        let id = self.push(state, Box::new([]));
        self.memo.insert(state, vec![id]);
        id
    }

    /// The live choices of `state` along `slot`, as states.
    fn choices(&self, state: StateRef, slot: usize) -> Vec<StateRef> {
        let target = self.comma.layers[state.0].slots[slot].target;
        self.comma
            .live_choices(state, slot)
            .into_iter()
            .map(|c| (target, c))
            .collect()
    }

    /// The extensions of a live state with finitely many: one for a state
    /// that does not branch, and otherwise one per choice of an extension
    /// along every slot, the slots in ascending edge order with the last
    /// varying fastest and each slot's extensions taken choice by choice.
    fn cones(&mut self, layer: usize, state: usize) -> Vec<ExtId> {
        let mut stack: Vec<(StateRef, bool)> = vec![((layer, state), false)];
        while let Some((here, expanded)) = stack.pop() {
            if self.memo.contains_key(&here) {
                continue;
            }
            if !self.comma.branching[here.0][here.1] {
                self.leaf(here);
                continue;
            }
            let slots = self.comma.layers[here.0].slots.len();
            if !expanded {
                stack.push((here, true));
                for slot in 0..slots {
                    for child in self.choices(here, slot) {
                        if !self.memo.contains_key(&child) {
                            stack.push((child, false));
                        }
                    }
                }
                continue;
            }
            let options: Vec<Vec<ExtId>> = (0..slots)
                .map(|slot| {
                    self.choices(here, slot)
                        .into_iter()
                        .flat_map(|child| self.memo.get(&child).cloned().unwrap_or_default())
                        .collect()
                })
                .collect();
            let mut picks: Vec<Vec<ExtId>> = vec![Vec::new()];
            for option in &options {
                let mut next = Vec::with_capacity(picks.len() * option.len());
                for pick in &picks {
                    for &ext in option {
                        let mut extended = pick.clone();
                        extended.push(ext);
                        next.push(extended);
                    }
                }
                picks = next;
            }
            let exts = picks
                .into_iter()
                .map(|pick| self.push(here, pick.into_boxed_slice()))
                .collect();
            self.memo.insert(here, exts);
        }
        self.memo.get(&(layer, state)).cloned().unwrap_or_default()
    }

    /// The extension a cone takes along `slot`: the choice it stores, or, for
    /// a state that does not branch, the one extension of its one live
    /// choice.
    fn child(&mut self, ext: ExtId, slot: usize) -> Option<ExtId> {
        let (state, choices) = self.arena.get(ext)?;
        if let Some(&chosen) = choices.get(slot) {
            return Some(chosen);
        }
        let state = *state;
        let only = self.choices(state, slot).into_iter().next()?;
        Some(self.leaf(only))
    }
}

/// Check that every edge of `src` the migration carries lands on an edge of
/// `tgt`, taking the edges in ascending order. The carried edges and their
/// images are `edge_images`, read by [`crate::adjunction::edge_images`].
fn check_carried_edges(
    src: &Schema,
    tgt: &Schema,
    edge_images: &HashMap<Edge, Edge>,
) -> Result<(), RestrictError> {
    let mut edges: Vec<&Edge> = src.edges.keys().collect();
    edges.sort_unstable();
    for edge in edges {
        if let Some(image) = edge_images.get(edge) {
            if !tgt.edges.contains_key(image) {
                return Err(RestrictError::NoEdgeFound {
                    src: image.src.to_string(),
                    tgt: image.tgt.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// The internal arrows of a fibre: the edges of the source schema whose
/// endpoints both lie in the fibre and which the migration carries to no
/// target edge, in ascending order.
fn internal_arrows<'a>(
    schema: &'a Schema,
    edge_images: &HashMap<Edge, Edge>,
    fiber: &[Name],
) -> Vec<&'a Edge> {
    let mut arrows: Vec<&Edge> = fiber
        .iter()
        .flat_map(|src| fiber.iter().map(move |tgt| (src, tgt)))
        .flat_map(|(src, tgt)| schema.edges_between(src, tgt))
        .filter(|edge| !edge_images.contains_key(*edge))
        .collect();
    arrows.sort_unstable();
    arrows.dedup();
    arrows
}

/// Build the fiber map of a migration: each target vertex with the source
/// vertices it is the image of.
///
/// The source vertices are the migration's domain as
/// [`crate::adjunction::source_vertices`] reads it, the one reading every
/// table operation shares. Target vertices come in ascending name and each
/// fiber's source vertices in name order, so downstream consumers see a
/// factor order that does not depend on hash iteration order.
fn build_fiber_map(migration: &CompiledMigration) -> BTreeMap<Name, Vec<Name>> {
    let mut fiber_map: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
    for source in crate::adjunction::source_vertices(migration) {
        let target = crate::adjunction::map_vertex(migration, &source);
        fiber_map.entry(target).or_default().push(source);
    }
    fiber_map
}

/// Right Kan extension (`Pi_F`) for W-type instances — vertex-injective only.
///
/// This function is defined only for migrations that are injective on
/// vertices: each target vertex has exactly one source vertex in its fiber.
/// Under that restriction the extension is a relabeling — it remaps anchors
/// and edges, preserving the tree structure — and constructs no product.
///
/// A non-injective migration (two or more source vertices mapping to one
/// target vertex) would require building a product of subtrees, which is not
/// implemented here; see [`functor_pi`] for the set-valued product. Rather
/// than silently emit `Sigma`-shaped output mislabeled as `Pi`, this
/// function rejects such a migration with
/// [`RestrictError::NonInjectiveVertexMap`].
///
/// Like the total left Kan extension, every source node's anchor must be
/// remapped or surviving; an unmapped anchor is reported via
/// [`RestrictError::UnmappedAnchor`] instead of being dropped silently.
///
/// The `max_product_nodes` parameter is retained for signature compatibility
/// with the set-valued [`functor_pi`] path; because the vertex-injective case
/// never forms a product, it imposes no bound here.
///
/// # Errors
///
/// - [`RestrictError::NonInjectiveVertexMap`] if two or more source vertices
///   map to the same target vertex.
/// - [`RestrictError::UnmappedAnchor`] if a source node's anchor is neither
///   remapped nor surviving.
/// - [`RestrictError::RootPruned`] if the root cannot be mapped, or another
///   [`RestrictError`] variant if edge resolution fails.
pub fn wtype_pi(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_nodes: usize,
) -> Result<WInstance, RestrictError> {
    let fiber_map = build_fiber_map(migration);

    // Pi over W-types is defined here only for vertex-injective migrations:
    // each target vertex must have exactly one source vertex in its fiber. A
    // fiber with multiple sources would require a product of subtrees, which
    // this function does not build (see `functor_pi` for the set-valued
    // product). Reject rather than silently return Sigma-shaped output.
    for (tgt, srcs) in &fiber_map {
        if srcs.len() > 1 {
            return Err(RestrictError::NonInjectiveVertexMap {
                target: tgt.clone(),
                sources: srcs.clone(),
            });
        }
    }

    // The vertex-injective case never forms a product, so `max_product_nodes`
    // imposes no bound; it is kept only for signature compatibility with the
    // set-valued `functor_pi` path.
    let _ = max_product_nodes;

    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;

    let root_anchor = &root_node.anchor;
    if !migration.surviving_verts.contains(root_anchor)
        && !migration.vertex_remap.contains_key(root_anchor)
    {
        return Err(RestrictError::RootPruned);
    }

    // Remap nodes
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(instance.nodes.len());
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            // No image in the target schema; a total Pi cannot map this node.
            return Err(RestrictError::UnmappedAnchor {
                anchor: node.anchor.clone(),
                node_id: id,
            });
        }
        // Apply value transforms (coercions and op-to-term assignments) to
        // the Pi node.
        let transforms = migration.value_transforms(&node.anchor);
        if !transforms.is_empty() {
            let ctx = crate::wtype::TransformContext::new(None, instance, id, &transforms);
            crate::wtype::apply_field_transforms(&mut new_node, &transforms, &ctx)?;
        }
        new_nodes.insert(id, new_node);
    }

    // Remap arcs
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::with_capacity(instance.arcs.len());
    for &(parent, child, ref edge) in &instance.arcs {
        if !new_nodes.contains_key(&parent) || !new_nodes.contains_key(&child) {
            continue;
        }

        if let Some(new_edge) = migration.edge_remap.get(edge) {
            new_arcs.push((parent, child, new_edge.clone()));
        } else if migration.surviving_edges.contains(edge) {
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            if edge.src == *parent_anchor && edge.tgt == *child_anchor {
                new_arcs.push((parent, child, edge.clone()));
            } else {
                let resolved =
                    resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
                new_arcs.push((parent, child, resolved));
            }
        } else {
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            let resolved =
                resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
            new_arcs.push((parent, child, resolved));
        }
    }

    // Reconstruct fans
    let surviving_ids: rustc_hash::FxHashSet<u32> = new_nodes.keys().copied().collect();
    let empty_ancestors = rustc_hash::FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_ids,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    Ok(WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use panproto_schema::Vertex;
    use smallvec::smallvec;

    use super::*;
    use crate::value::Value;

    fn make_test_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
        let mut between = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: edges.iter().map(|e| (e.clone(), e.kind.clone())).collect(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between,
            ..Default::default()
        }
    }

    /// The source and target schemas a table test runs Pi against: the
    /// source declares every foreign key of the instance, the `declared`
    /// edges and the edges the migration carries; the target declares their
    /// images.
    fn schemas(
        inst: &FInstance,
        migration: &CompiledMigration,
        declared: &[Edge],
    ) -> (Schema, Schema) {
        let mut src: Vec<Edge> = inst
            .foreign_keys()
            .into_iter()
            .map(|(e, _)| e.clone())
            .collect();
        src.extend(declared.iter().cloned());
        src.extend(migration.edge_remap.keys().cloned());
        src.extend(migration.surviving_edges.iter().cloned());
        src.sort_unstable();
        src.dedup();
        let mut tgt: Vec<Edge> = migration.edge_remap.values().cloned().collect();
        tgt.extend(migration.surviving_edges.iter().cloned());
        tgt.sort_unstable();
        tgt.dedup();
        (make_test_schema(&[], &src), make_test_schema(&[], &tgt))
    }

    /// Pi along `migration`, against the schemas of [`schemas`].
    fn run_pi(
        inst: &FInstance,
        migration: &CompiledMigration,
        declared: &[Edge],
        max: usize,
    ) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
        let (src, tgt) = schemas(inst, migration, declared);
        functor_pi_with_hom(inst, &src, &tgt, migration, max)
    }

    // --- functor_pi tests ---

    #[test]
    fn functor_pi_single_fiber_copies_table() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));
        let inst = FInstance::new()
            .with_table("users", vec![row.clone()])
            .unwrap();

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("users")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(result.table_count(), 1);
        assert_eq!(result.row_count("users"), 1);
    }

    #[test]
    fn functor_pi_multi_fiber_cartesian_product() {
        let rows_a = vec![
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(1));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(2));
                r
            },
        ];
        let rows_b = vec![{
            let mut r = HashMap::new();
            r.insert("y".to_string(), Value::Int(10));
            r
        }];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_b", rows_b))
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        // 2 * 1 = 2 product rows
        assert_eq!(result.row_count("merged"), 2);
        // Each product row should have both x and y columns
        for (_, row) in result.rows("merged") {
            assert!(row.contains_key("x"));
            assert!(row.contains_key("y"));
        }
    }

    #[test]
    fn functor_pi_product_size_exceeded() {
        let rows_a = vec![
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(1));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(2));
                r
            },
        ];
        let rows_b = vec![
            {
                let mut r = HashMap::new();
                r.insert("y".to_string(), Value::Int(10));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("y".to_string(), Value::Int(20));
                r
            },
        ];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_b", rows_b))
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Limit to 2 but product would be 4
        let result = run_pi(&inst, &migration, &[], 2).map(|(pi, _)| pi);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                RestrictError::ProductSizeExceeded {
                    actual: 4,
                    limit: 2,
                    ..
                }
            ),
            "expected ProductSizeExceeded, got {err:?}"
        );
    }

    #[test]
    fn functor_pi_preserves_fk_from_product_side_to_untouched() {
        // src_a and src_c both map to `merged` (a two-element fiber → product);
        // `other` is untouched. An FK from src_a to `other` must survive,
        // remapped onto the product rows of `merged`.
        let rows_a = vec![
            HashMap::from([("a".to_string(), Value::Int(0))]),
            HashMap::from([("a".to_string(), Value::Int(1))]),
        ];
        let rows_c = vec![HashMap::from([("c".to_string(), Value::Int(0))])];
        let rows_o = vec![
            HashMap::from([("b".to_string(), Value::Int(0))]),
            HashMap::from([("b".to_string(), Value::Int(1))]),
        ];

        let fk_edge = Edge {
            src: "src_a".into(),
            tgt: "other".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        // src_a holds rows 0 and 1, src_c row 2, other rows 3 and 4; src_a
        // row 0 references other row 4 and row 1 references row 3.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_c", rows_c))
            .and_then(|f| f.with_table("other", rows_o))
            .and_then(|f| {
                f.with_foreign_key(
                    fk_edge.clone(),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(3))],
                )
            })
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_c"), Name::from("merged"));

        let new_edge = Edge {
            src: "merged".into(),
            tgt: "other".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let mut edge_remap = HashMap::new();
        edge_remap.insert(fk_edge, new_edge.clone());

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged"), Name::from("other")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(result.row_count("merged"), 2);
        assert_eq!(result.row_count("other"), 2);

        // The product rows are fresh, from 5 up: (a0, c2) is 5 and (a1, c2)
        // is 6. `other` is a one-component fibre and keeps its ids.
        assert_eq!(
            result.pairs(&new_edge),
            &BTreeSet::from([(RowId(5), RowId(4)), (RowId(6), RowId(3))])
        );
    }

    #[test]
    fn functor_pi_preserves_fk_between_two_product_sides() {
        // src_a + src_a2 → m1 (product); src_b + src_b2 → m2 (product). An FK
        // from src_a to src_b — both product-side — must survive, remapped onto
        // the product rows of m1 and m2.
        let rows_a = vec![
            HashMap::from([("a".to_string(), Value::Int(0))]),
            HashMap::from([("a".to_string(), Value::Int(1))]),
        ];
        let rows_p = vec![HashMap::from([("a2".to_string(), Value::Int(0))])];
        let rows_b = vec![
            HashMap::from([("b".to_string(), Value::Int(0))]),
            HashMap::from([("b".to_string(), Value::Int(1))]),
        ];
        let rows_q = vec![HashMap::from([("b2".to_string(), Value::Int(0))])];

        let fk_edge = Edge {
            src: "src_a".into(),
            tgt: "src_b".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        // src_a holds rows 0 and 1, src_a2 row 2, src_b rows 3 and 4, src_b2
        // row 5; src_a row 0 references src_b row 4 and row 1 row 3.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_a2", rows_p))
            .and_then(|f| f.with_table("src_b", rows_b))
            .and_then(|f| f.with_table("src_b2", rows_q))
            .and_then(|f| {
                f.with_foreign_key(
                    fk_edge.clone(),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(3))],
                )
            })
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("m1"));
        vertex_remap.insert(Name::from("src_a2"), Name::from("m1"));
        vertex_remap.insert(Name::from("src_b"), Name::from("m2"));
        vertex_remap.insert(Name::from("src_b2"), Name::from("m2"));

        let new_edge = Edge {
            src: "m1".into(),
            tgt: "m2".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let mut edge_remap = HashMap::new();
        edge_remap.insert(fk_edge, new_edge.clone());

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("m1"), Name::from("m2")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(result.row_count("m1"), 2);
        assert_eq!(result.row_count("m2"), 2);

        // m1's product rows are 6 = (a0, a2_2) and 7 = (a1, a2_2); m2's are
        // 8 = (b3, b2_5) and 9 = (b4, b2_5).
        assert_eq!(
            result.pairs(&new_edge),
            &BTreeSet::from([(RowId(6), RowId(9)), (RowId(7), RowId(8))])
        );
    }

    // --- wtype_pi tests ---

    #[test]
    fn wtype_pi_identity_migration() {
        let edge = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        let schema = make_test_schema(&["root", "leaf"], std::slice::from_ref(&edge));

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "leaf"));
        let arcs = vec![(0, 1, edge.clone())];
        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("leaf")]),
            surviving_edges: HashSet::from([edge]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = wtype_pi(&inst, &schema, &migration, 10_000).unwrap();
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.arc_count(), 1);
    }

    #[test]
    fn wtype_pi_rejects_non_injective_vertex_map() {
        let schema = make_test_schema(&["merged"], &[]);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "src_a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("src_a"));

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // src_a and src_b both map to `merged`: a non-injective vertex map,
        // which wtype_pi rejects rather than producing Sigma-shaped output.
        // The bound argument is irrelevant to this outcome.
        let err = wtype_pi(&inst, &schema, &migration, 10_000).unwrap_err();
        match err {
            RestrictError::NonInjectiveVertexMap { target, sources } => {
                assert_eq!(target, Name::from("merged"));
                assert_eq!(sources, vec![Name::from("src_a"), Name::from("src_b")]);
            }
            other => panic!("expected NonInjectiveVertexMap, got {other:?}"),
        }
    }

    #[test]
    fn wtype_pi_errors_on_unmapped_anchor() {
        let schema = make_test_schema(&["root"], &[]);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "orphan"));
        let edge = Edge {
            src: "root".into(),
            tgt: "orphan".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let inst = WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, Name::from("root"));

        // `root` survives; `orphan` is neither remapped nor surviving, so it
        // has no image in the target and a total Pi must report it.
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let err = wtype_pi(&inst, &schema, &migration, 10_000).unwrap_err();
        assert!(
            matches!(err, RestrictError::UnmappedAnchor { node_id: 1, .. }),
            "expected UnmappedAnchor for node 1, got {err:?}"
        );
    }

    /// A fiber component with an empty table is a factor of the product, so it
    /// empties the product rather than being dropped from the fiber.
    #[test]
    fn functor_pi_empty_factor_empties_product() {
        let rows_a = vec![
            HashMap::from([("x".to_string(), Value::Int(1))]),
            HashMap::from([("x".to_string(), Value::Int(2))]),
        ];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_b", Vec::new()))
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("merged"));
        migration.vertex_remap = vertex_remap;

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(
            result.row_count("merged"),
            0,
            "a product with an empty factor has no rows",
        );
    }

    /// An absent source table is the empty table, so it empties the product the
    /// same way a present-but-empty one does.
    #[test]
    fn functor_pi_absent_factor_empties_product() {
        let rows_a = vec![HashMap::from([("x".to_string(), Value::Int(1))])];
        let inst = FInstance::new().with_table("src_a", rows_a).unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("merged"));
        migration.vertex_remap = vertex_remap;

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(
            result.row_count("merged"),
            0,
            "an absent factor is the empty table, so the product is empty",
        );
    }

    /// A loop on each vertex of a two-vertex fibre, both carried to one loop
    /// on the target: the comma category holds `(a, L^n)` and `(b, L^n)` for
    /// every `n`, each fixed by the one before, so the limit is the product
    /// of the fibre's tables and the loop relates each row to itself.
    #[test]
    fn carried_loops_in_a_two_vertex_fibre_take_the_limit() {
        fn self_edge(v: &str) -> Edge {
            Edge {
                src: v.into(),
                tgt: v.into(),
                kind: "ref".into(),
                name: Some("link".into()),
            }
        }

        let inst = FInstance::new()
            .with_table("a", vec![HashMap::from([("n".to_string(), Value::Int(0))])])
            .and_then(|f| {
                f.with_table("b", vec![HashMap::from([("m".to_string(), Value::Int(0))])])
            })
            .and_then(|f| f.with_foreign_key(self_edge("a"), [(RowId(0), RowId(0))]))
            .and_then(|f| f.with_foreign_key(self_edge("b"), [(RowId(1), RowId(1))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("b"), Name::from("t"));
        migration.edge_remap.insert(self_edge("a"), self_edge("t"));
        migration.edge_remap.insert(self_edge("b"), self_edge("t"));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        // One cone, (0, 1), at the fresh id 2.
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(2)])
        );
        assert_eq!(
            pi.pairs(&self_edge("t")),
            &BTreeSet::from([(RowId(2), RowId(2))])
        );
    }

    /// Assert that Pi refused the limit at `vertex` as infinite: a
    /// [`RestrictError::ProductSizeExceeded`] whose count is saturated.
    fn assert_infinite<T: std::fmt::Debug>(result: Result<T, RestrictError>, vertex: &str) {
        match result {
            Err(RestrictError::ProductSizeExceeded {
                vertex: v,
                actual: usize::MAX,
                ..
            }) => assert_eq!(v, vertex),
            other => panic!("expected an infinite limit at {vertex}, got {other:?}"),
        }
    }

    /// A loop edge `v -> v` of kind `fk` named `name`.
    fn loop_on(v: &str, name: &str) -> Edge {
        Edge {
            src: v.into(),
            tgt: v.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// Two two-row tables merged into one target vertex, with a foreign key
    /// from `src_a` to `src_b` that the migration carries nowhere.
    fn intra_fibre_fixture() -> (FInstance, CompiledMigration, Edge) {
        let fk = Edge {
            src: "src_a".into(),
            tgt: "src_b".into(),
            kind: "fk".into(),
            name: Some("ref".into()),
        };
        let cell = |k: &str, v: i64| HashMap::from([(k.to_owned(), Value::Int(v))]);
        // src_a holds rows 0 (a0) and 1 (a1), src_b rows 2 (b0) and 3 (b1);
        // the foreign key relates a0 to b1.
        let inst = FInstance::new()
            .with_table("src_a", vec![cell("a", 0), cell("a", 1)])
            .and_then(|f| f.with_table("src_b", vec![cell("b", 0), cell("b", 1)]))
            .and_then(|f| f.with_foreign_key(fk.clone(), [(RowId(0), RowId(3))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("src_a"), Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("src_b"), Name::from("m"));
        (inst, migration, fk)
    }

    #[test]
    fn functor_pi_equalises_along_an_intra_fibre_foreign_key() {
        let (inst, migration, _) = intra_fibre_fixture();
        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        let rows: Vec<(i64, i64)> = result
            .rows("m")
            .into_iter()
            .map(|(_, row)| {
                let cell_of = |k: &str| match row.get(k) {
                    Some(Value::Int(v)) => *v,
                    other => panic!("cell {k} is {other:?}"),
                };
                (cell_of("a"), cell_of("b"))
            })
            .collect();
        assert_eq!(
            rows,
            vec![(0, 1)],
            "the limit keeps only the product rows whose components the internal arrow relates",
        );
    }

    /// An internal arrow with no pairs is still an arrow of the fibre: the
    /// limit equalises along the empty relation and is empty. Adding a pair
    /// can only enlarge the limit.
    #[test]
    fn an_internal_foreign_key_without_pairs_empties_the_limit() {
        let (with_pair, migration, fk) = intra_fibre_fixture();
        let mut without_pair = with_pair.clone();
        assert!(without_pair.remove_foreign_key(&fk, RowId(0), RowId(3)));
        let with_rows = run_pi(&with_pair, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap()
            .row_count("m");
        let without_rows = run_pi(&without_pair, &migration, std::slice::from_ref(&fk), 100)
            .map(|(pi, _)| pi)
            .unwrap()
            .row_count("m");
        assert_eq!((with_rows, without_rows), (1, 0));
    }

    #[test]
    fn a_carried_edge_the_target_lacks_is_refused() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        let loop_edge = Edge {
            src: "m".into(),
            tgt: "m".into(),
            kind: "fk".into(),
            name: Some("ref".into()),
        };
        migration.edge_remap.insert(fk, loop_edge);
        let (src, _) = schemas(&inst, &migration, &[]);
        assert!(matches!(
            functor_pi(&inst, &src, &Schema::default(), &migration, 100),
            Err(RestrictError::NoEdgeFound { .. })
        ));
    }

    /// The intra-fibre foreign key made a total function and remapped to a
    /// loop on `m`: the cone's row of `src_a` at each iterate `L^n` is fixed
    /// by nothing (the loop only fixes `src_b` at `L^(n+1)`), so the limit is
    /// an infinite product and Pi refuses it rather than returning the
    /// product of the fibre.
    #[test]
    fn pi_dispatch_rejects_a_fibre_with_an_internal_edge() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        let total = inst
            .with_foreign_key(fk.clone(), [(RowId(1), RowId(2))])
            .unwrap();
        migration.edge_remap.insert(fk, loop_on("m", "ref"));
        assert_infinite(run_pi(&total, &migration, &[], 100), "m");
    }

    /// The intra-fibre foreign key `{(a0, b1)}` remapped to a loop on `m`:
    /// every cone draws `a0` at each iterate and `b1` at each iterate after
    /// the first, so the limit is the two rows `(a0, b0)` and `(a0, b1)`,
    /// each sent by the loop to `(a0, b1)`.
    #[test]
    fn an_internal_edge_sent_to_a_loop_takes_the_limit_over_its_iterates() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        migration.edge_remap.insert(fk, loop_on("m", "ref"));
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            drawn(&pi, &projections, "m", "src_a"),
            vec![(RowId(4), RowId(0)), (RowId(5), RowId(0))]
        );
        assert_eq!(
            drawn(&pi, &projections, "m", "src_b"),
            vec![(RowId(4), RowId(2)), (RowId(5), RowId(3))]
        );
        assert_eq!(
            pi.pairs(&loop_on("m", "ref")),
            &BTreeSet::from([(RowId(4), RowId(5)), (RowId(5), RowId(5))])
        );
    }

    /// The same foreign key listed as surviving: the migration relabels it to
    /// the loop on `m`, so Pi takes the same limit as for the remapped one.
    #[test]
    fn a_surviving_intra_fibre_edge_travels_to_its_loop() {
        let (inst, mut remapped, fk) = intra_fibre_fixture();
        let mut surviving = remapped.clone();
        surviving.surviving_edges.insert(fk.clone());
        remapped.edge_remap.insert(fk.clone(), loop_on("m", "ref"));
        let src = make_test_schema(&[], std::slice::from_ref(&fk));
        let tgt = make_test_schema(&[], &[loop_on("m", "ref")]);
        assert_eq!(
            functor_pi(&inst, &src, &tgt, &surviving, 100).unwrap(),
            functor_pi(&inst, &src, &tgt, &remapped, 100).unwrap()
        );
    }

    /// A one-vertex fibre `{a}` whose loop, a total function, is relabelled
    /// onto `t`: every object of the comma category is reached from `a` along
    /// exactly one path, so the limit is `a`'s table, its rows keep their ids
    /// and the loop travels with its pairs.
    #[test]
    fn a_one_vertex_fibre_carries_its_loop() {
        let cell = |v: i64| HashMap::from([("n".to_owned(), Value::Int(v))]);
        let inst = FInstance::new()
            .with_table("a", vec![cell(0), cell(1)])
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "next"),
                    [(RowId(0), RowId(1)), (RowId(1), RowId(1))],
                )
            })
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "next"), loop_on("t", "next"));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let ids: Vec<RowId> = pi.rows("t").into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![RowId(0), RowId(1)]);
        assert_eq!(
            pi.pairs(&loop_on("t", "next")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(1), RowId(1))])
        );
    }

    /// The same loop without a pair at row 1: the cone through row 1 has no
    /// row at the next iterate, and row 0 leads to row 1, so neither row is
    /// a cone and the limit is empty. A foreign key is read as a relation,
    /// and a row with no pair on an edge the limit follows has no cone.
    #[test]
    fn a_row_without_a_pair_on_a_followed_edge_has_no_cone() {
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| f.with_foreign_key(loop_on("a", "next"), [(RowId(0), RowId(1))]))
            .unwrap();
        let migration = migration_of(
            &[("a", "t")],
            &[(loop_on("a", "next"), loop_on("t", "next"))],
        );
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(pi.total_row_count(), 0);
    }

    /// Two loops of a one-vertex fibre carried to one loop: both send the
    /// row at each iterate to the row at the next, so the limit is the rows
    /// on which the two loops agree, iterated: row 0 (both send it to 0),
    /// not row 1 (`p` sends it to 1, `q` to 0).
    #[test]
    fn a_one_vertex_fibre_whose_loops_share_an_image_takes_their_equaliser() {
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "p"),
                    [(RowId(0), RowId(0)), (RowId(1), RowId(1))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "q"),
                    [(RowId(0), RowId(0)), (RowId(1), RowId(0))],
                )
            })
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "p"), loop_on("t", "p"));
        migration
            .edge_remap
            .insert(loop_on("a", "q"), loop_on("t", "p"));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(0)])
        );
        assert_eq!(
            pi.pairs(&loop_on("t", "p")),
            &BTreeSet::from([(RowId(0), RowId(0))])
        );
    }

    /// A one-vertex fibre with one loop sent to the identity and one carried
    /// to a loop: the limit is the rows the identity loop fixes, at every
    /// iterate of the carried loop, each fixed by the one before.
    #[test]
    fn a_carried_loop_beside_a_loop_sent_to_the_identity_takes_the_limit() {
        let inst = FInstance::new()
            .with_table("a", vec![HashMap::new()])
            .and_then(|f| f.with_foreign_key(loop_on("a", "c"), [(RowId(0), RowId(0))]))
            .and_then(|f| f.with_foreign_key(loop_on("a", "i"), [(RowId(0), RowId(0))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "c"), loop_on("t", "c"));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(0)])
        );
        assert_eq!(
            pi.pairs(&loop_on("t", "c")),
            &BTreeSet::from([(RowId(0), RowId(0))])
        );
    }

    /// Two edges `e1: a1 -> b1` and `e2: a2 -> b2` merged into one edge
    /// `E: m1 -> m2`. `Pi(E)` sends `(x1, x2)` to `(e1 x1, e2 x2)`, so two
    /// product rows are related exactly when both components are: the
    /// relation is the intersection of the edges' fan-outs, not their union,
    /// and a carried edge without pairs empties it.
    #[test]
    fn merged_edges_relate_product_rows_componentwise() {
        let edge = |src: &str, tgt: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some("e".into()),
        };
        let two = |col: &str| {
            vec![
                HashMap::from([(col.to_owned(), Value::Int(0))]),
                HashMap::from([(col.to_owned(), Value::Int(1))]),
            ]
        };
        // a1 holds rows 0, 1; a2 rows 2, 3; b1 rows 4, 5; b2 rows 6, 7.
        let base = FInstance::new()
            .with_table("a1", two("a1"))
            .and_then(|f| f.with_table("a2", two("a2")))
            .and_then(|f| f.with_table("b1", two("b1")))
            .and_then(|f| f.with_table("b2", two("b2")))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("a1", "b1"),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(5))],
                )
            })
            .unwrap();
        let inst = base
            .clone()
            .with_foreign_key(
                edge("a2", "b2"),
                [(RowId(2), RowId(7)), (RowId(3), RowId(6))],
            )
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration
            .surviving_verts
            .extend([Name::from("m1"), Name::from("m2")]);
        for (src, tgt) in [("a1", "m1"), ("a2", "m1"), ("b1", "m2"), ("b2", "m2")] {
            migration
                .vertex_remap
                .insert(Name::from(src), Name::from(tgt));
        }
        migration
            .edge_remap
            .insert(edge("a1", "b1"), edge("m1", "m2"));
        migration
            .edge_remap
            .insert(edge("a2", "b2"), edge("m1", "m2"));

        // m1's rows: 8 = (0, 2), 9 = (0, 3), 10 = (1, 2), 11 = (1, 3); m2's:
        // 12 = (4, 6), 13 = (4, 7), 14 = (5, 6), 15 = (5, 7).
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let r = |a: u32, b: u32| (RowId(a), RowId(b));
        assert_eq!(
            pi.pairs(&edge("m1", "m2")),
            &BTreeSet::from([r(8, 13), r(9, 12), r(10, 15), r(11, 14)]),
        );

        let (without, _) = run_pi(&base, &migration, &[], 100).unwrap();
        assert!(without.pairs(&edge("m1", "m2")).is_empty());
    }

    #[test]
    fn functor_pi_allocates_rows_in_product_order() {
        let (inst, migration, _) = intra_fibre_fixture();
        let mut unconstrained = inst.clone();
        let fk = unconstrained.all_pairs()[0].clone();
        assert!(unconstrained.remove_foreign_key(&fk.0, fk.1, fk.2));
        let (result, projections) = run_pi(&unconstrained, &migration, &[], 100).unwrap();
        // Fresh ids start above the input's largest (3), in product order:
        // src_a varies slowest, src_b fastest.
        assert_eq!(
            result.table("m").unwrap().ids(),
            vec![RowId(4), RowId(5), RowId(6), RowId(7)]
        );
        let onto = |src: &str| {
            projections[&Name::from(src)]
                .row_map
                .values()
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(onto("src_a"), vec![RowId(0), RowId(0), RowId(1), RowId(1)]);
        assert_eq!(onto("src_b"), vec![RowId(2), RowId(3), RowId(2), RowId(3)]);
        // Each projection preserves cells column by column.
        for (product_row, component) in &projections[&Name::from("src_b")].row_map {
            assert_eq!(
                result.row(*product_row).unwrap().get("b"),
                inst.row(*component).unwrap().get("b")
            );
        }
    }

    #[test]
    fn a_one_component_fibre_keeps_its_row_ids() {
        let inst = FInstance::new()
            .with_rows(
                "users",
                [(RowId(41), HashMap::new()), (RowId(7), HashMap::new())],
            )
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("users"));
        let (result, projections) = run_pi(&inst, &migration, &[], 10).unwrap();
        assert_eq!(
            result.table("users").unwrap().ids(),
            vec![RowId(7), RowId(41)]
        );
        assert_eq!(
            projections[&Name::from("users")].row_map,
            BTreeMap::from([(RowId(7), RowId(7)), (RowId(41), RowId(41))])
        );
    }

    /// Carrying foreign keys through Pi is linear in pairs plus rows: each
    /// pair reads the result rows drawn from its two source rows through an
    /// index, not by scanning the endpoint tables.
    #[test]
    fn carrying_foreign_keys_through_pi_is_linear() {
        const ROWS: u32 = 20_000;
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "fk".into(),
            name: None,
        };
        let cells = |n: u32| HashMap::from([("n".to_owned(), Value::Int(i64::from(n)))]);
        let inst = FInstance::new()
            .with_table("a", (0..ROWS).map(cells))
            .and_then(|f| f.with_table("b", (0..ROWS).map(cells)))
            .and_then(|f| {
                f.with_foreign_key(edge.clone(), (0..ROWS).map(|i| (RowId(i), RowId(ROWS + i))))
            })
            .unwrap();
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("a"), Name::from("b")]),
            surviving_edges: HashSet::from([edge]),
            ..CompiledMigration::default()
        };
        let began = std::time::Instant::now();
        let (pi, _) = run_pi(&inst, &migration, &[], usize::MAX).unwrap();
        let took = began.elapsed();
        assert_eq!(pi, inst, "Pi along the identity is the identity");
        assert!(
            took < std::time::Duration::from_secs(3),
            "Pi over {ROWS} pairs took {took:?}"
        );
    }

    /// A two-component fibre `{a, b} -> m` whose rows `0` (in `a`) and `1`
    /// (in `b`) both hold the column `x`.
    fn shared_column_fibre(left: Value, right: Value) -> (FInstance, CompiledMigration) {
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::from([("x".to_owned(), left)]))])
            .and_then(|f| f.with_rows("b", [(RowId(1), HashMap::from([("x".to_owned(), right)]))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("b"), Name::from("m"));
        (inst, migration)
    }

    /// Fibre components agree on a shared column exactly when the cells
    /// have the same canonical bytes, so two `NaN` cells agree.
    #[test]
    fn components_holding_nan_agree() {
        let (inst, migration) = shared_column_fibre(Value::Float(f64::NAN), Value::Float(f64::NAN));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let rows = pi.rows("m");
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].1.get("x"), Some(Value::Float(v)) if v.is_nan()));
    }

    /// `0.0` and `-0.0` are different cells, so components holding them
    /// collide rather than merging into whichever came last.
    #[test]
    fn components_holding_signed_zeros_collide() {
        let (inst, migration) = shared_column_fibre(Value::Float(0.0), Value::Float(-0.0));
        let err = run_pi(&inst, &migration, &[], 100).unwrap_err();
        assert!(
            matches!(&err, RestrictError::AttributeCollision { vertex, column } if vertex == "m" && column == "x"),
            "{err:?}"
        );
    }

    /// Pi is computed over the image of `F`: a target vertex no source vertex
    /// maps to has no table, as the rustdoc of [`functor_pi_with_hom`]
    /// states.
    #[test]
    fn a_target_vertex_outside_the_image_gets_no_table() {
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("a".into());
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::new())])
            .unwrap();
        let pi = functor_pi(
            &inst,
            &make_test_schema(&["a"], &[]),
            &make_test_schema(&["a", "t"], &[]),
            &migration,
            100,
        )
        .unwrap();
        assert_eq!(pi.table_names(), vec![&Name::from("a")]);
        assert_eq!(pi.row_count("t"), 0);
    }

    // --- Pi is the limit over the comma category d/F ---

    /// An edge `src -> tgt` of kind `fk` named `name`.
    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// `n` rows without cells.
    fn blank(n: usize) -> Vec<Row> {
        vec![Row::new(); n]
    }

    /// A migration sending each `(source, target)` vertex and each
    /// `(source, target)` edge as listed, every target vertex surviving.
    fn migration_of(vertices: &[(&str, &str)], edges: &[(Edge, Edge)]) -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        for &(src, tgt) in vertices {
            migration.vertex_remap.insert(src.into(), tgt.into());
            migration.surviving_verts.insert(tgt.into());
        }
        for (src, tgt) in edges {
            migration.edge_remap.insert(src.clone(), tgt.clone());
        }
        migration
    }

    /// Pi along `migration`, against the schemas of [`schemas`] with the
    /// target-only edges `extra` added to the target.
    fn pi_with_target_edges(
        inst: &FInstance,
        migration: &CompiledMigration,
        extra: &[Edge],
    ) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
        let (src, tgt) = schemas(inst, migration, &[]);
        let mut edges: Vec<Edge> = tgt.edges.keys().cloned().collect();
        edges.extend(extra.iter().cloned());
        functor_pi_with_hom(inst, &src, &make_test_schema(&[], &edges), migration, 100)
    }

    /// The rows of `vertex` with the component each drew from `component`.
    fn drawn(
        pi: &FInstance,
        projections: &BTreeMap<Name, FInstanceHom>,
        vertex: &str,
        component: &str,
    ) -> Vec<(RowId, RowId)> {
        pi.rows(vertex)
            .into_iter()
            .map(|(id, _)| (id, projections[&Name::from(component)].row_map[&id]))
            .collect()
    }

    /// Two components of one fibre carried along the same target edge into
    /// one component: `Pi_F(m1)` is the pullback `I(a1) x_{I(b)} I(a2)`, so
    /// only the rows whose components meet the same row of `b` survive.
    #[test]
    fn edges_meeting_in_a_target_fibre_take_the_pullback() {
        // a1 holds 0, 1; a2 holds 2, 3; b holds 4, 5.
        let inst = FInstance::new()
            .with_table("a1", blank(2))
            .and_then(|f| f.with_table("a2", blank(2)))
            .and_then(|f| f.with_table("b", blank(2)))
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a1", "b", "e1"),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(5))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a2", "b", "e2"),
                    [(RowId(2), RowId(4)), (RowId(3), RowId(5))],
                )
            })
            .unwrap();
        let migration = migration_of(
            &[("a1", "m1"), ("a2", "m1"), ("b", "m2")],
            &[
                (fk("a1", "b", "e1"), fk("m1", "m2", "E")),
                (fk("a2", "b", "e2"), fk("m1", "m2", "E")),
            ],
        );
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        // The two surviving rows are fresh, 6 = (0, 2) and 7 = (1, 3); `b`
        // keeps its ids.
        assert_eq!(
            drawn(&pi, &projections, "m1", "a1"),
            vec![(RowId(6), RowId(0)), (RowId(7), RowId(1))]
        );
        assert_eq!(
            drawn(&pi, &projections, "m1", "a2"),
            vec![(RowId(6), RowId(2)), (RowId(7), RowId(3))]
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(6), RowId(4)), (RowId(7), RowId(5))])
        );
    }

    /// Two parallel edges merged into one: `Pi_F(m1)` is their equaliser,
    /// the rows of `a` both edges send to the same row of `b`.
    #[test]
    fn parallel_edges_merged_into_one_take_the_equaliser() {
        // a holds 0, 1; b holds 2, 3.
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| f.with_table("b", blank(2)))
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a", "b", "e"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(3))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a", "b", "f"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap();
        let migration = migration_of(
            &[("a", "m1"), ("b", "m2")],
            &[
                (fk("a", "b", "e"), fk("m1", "m2", "E")),
                (fk("a", "b", "f"), fk("m1", "m2", "E")),
            ],
        );
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("m1").map(crate::functor::Table::ids),
            Some(vec![RowId(0)])
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(0), RowId(2))])
        );
    }

    /// A target edge with no preimage between two image vertices: the comma
    /// category `m1/F` holds `(b, E)` as well as `(a, id)`, so `Pi_F(m1)` is
    /// `I(a) x I(b)` and `E` sends each row to its `b` component.
    #[test]
    fn a_target_only_edge_adds_its_codomain_as_a_factor() {
        // a holds 0; b holds 1, 2.
        let inst = FInstance::new()
            .with_table("a", blank(1))
            .and_then(|f| f.with_table("b", blank(2)))
            .unwrap();
        let migration = migration_of(&[("a", "m1"), ("b", "m2")], &[]);
        let edge = fk("m1", "m2", "E");
        let (pi, projections) =
            pi_with_target_edges(&inst, &migration, std::slice::from_ref(&edge)).unwrap();
        assert_eq!(
            drawn(&pi, &projections, "m1", "a"),
            vec![(RowId(3), RowId(0)), (RowId(4), RowId(0))]
        );
        assert_eq!(
            pi.pairs(&edge),
            &BTreeSet::from([(RowId(3), RowId(1)), (RowId(4), RowId(2))])
        );
    }

    /// A target-only loop on an image vertex whose fibre holds two rows: the
    /// comma category holds an object for every iterate of the loop, each
    /// unconstrained, so `Pi_F(m)` is an infinite product and Pi refuses it
    /// as a product larger than any bound.
    #[test]
    fn a_target_only_loop_over_two_rows_is_an_infinite_limit() {
        let inst = FInstance::new().with_table("a", blank(2)).unwrap();
        let migration = migration_of(&[("a", "m")], &[]);
        let err = pi_with_target_edges(&inst, &migration, &[loop_on("m", "L")]).unwrap_err();
        assert!(
            matches!(&err, RestrictError::ProductSizeExceeded { vertex, actual: usize::MAX, limit: 100 } if vertex == "m"),
            "{err:?}"
        );
    }

    /// A carried edge into one component of a two-vertex target fibre: the
    /// other component `b2` is reached along `E` too, so `Pi_F(m1)` is
    /// `I(a) x I(b2)` and `E` stays a function.
    #[test]
    fn a_carried_edge_into_one_component_reaches_the_others() {
        // a holds 0; b holds 1; b2 holds 2, 3.
        let inst = FInstance::new()
            .with_table("a", blank(1))
            .and_then(|f| f.with_table("b", blank(1)))
            .and_then(|f| f.with_table("b2", blank(2)))
            .and_then(|f| f.with_foreign_key(fk("a", "b", "e"), [(RowId(0), RowId(1))]))
            .unwrap();
        let migration = migration_of(
            &[("a", "m1"), ("b", "m2"), ("b2", "m2")],
            &[(fk("a", "b", "e"), fk("m1", "m2", "E"))],
        );
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        // m1's rows are 4 and 5, m2's 6 = (1, 2) and 7 = (1, 3).
        assert_eq!(
            drawn(&pi, &projections, "m1", "a"),
            vec![(RowId(4), RowId(0)), (RowId(5), RowId(0))]
        );
        assert_eq!(
            drawn(&pi, &projections, "m2", "b2"),
            vec![(RowId(6), RowId(2)), (RowId(7), RowId(3))]
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(4), RowId(6)), (RowId(5), RowId(7))])
        );
    }

    /// A brute-force oracle for `Pi_F` over acyclic targets: it lists the
    /// comma category `d/F` object by object and enumerates its cones.
    mod limit_oracle {
        use std::collections::{BTreeMap, HashMap};

        use proptest::prelude::*;

        use super::{blank, fk, make_test_schema};
        use crate::functor::FInstance;
        use crate::view_ref::RowId;
        use crate::wtype::CompiledMigration;
        use panproto_gat::Name;
        use panproto_schema::Edge;

        /// A small linear congruential generator, so one seed fixes a case.
        struct Lcg(u64);

        impl Lcg {
            fn below(&mut self, n: usize) -> usize {
                self.0 = self
                    .0
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                usize::try_from(self.0 >> 33).unwrap() % n
            }
        }

        /// A case: the instance, the migration, the vertex map, the source
        /// edges sent to the identity, the carried edges with their images
        /// and every target edge. Target vertex `ti` only has edges to `tj`
        /// with `i < j`, so every comma category is finite.
        struct Case {
            inst: FInstance,
            migration: CompiledMigration,
            vertex_map: BTreeMap<String, String>,
            internal: Vec<Edge>,
            carried: Vec<(Edge, Edge)>,
            target_edges: Vec<Edge>,
        }

        fn case(seed: u64) -> Case {
            let mut r = Lcg(seed);
            let sources = 1 + r.below(4);
            let targets = 1 + r.below(3);
            let mut vertex_map = BTreeMap::new();
            let mut inst = FInstance::new();
            for i in 0..sources {
                let name = format!("s{i}");
                vertex_map.insert(name.clone(), format!("t{}", r.below(targets)));
                inst = inst.with_table(name, blank(r.below(3))).unwrap();
            }
            let index = |t: &str| t[1..].parse::<usize>().unwrap();
            let (mut internal, mut carried) = (Vec::new(), Vec::new());
            for k in 0..r.below(5) {
                let a = format!("s{}", r.below(sources));
                let b = format!("s{}", r.below(sources));
                let (ta, tb) = (vertex_map[&a].clone(), vertex_map[&b].clone());
                let edge = fk(&a, &b, &format!("e{k}"));
                if ta == tb {
                    internal.push(edge.clone());
                } else if index(&ta) < index(&tb) {
                    carried.push((edge.clone(), fk(&ta, &tb, &format!("E{}", r.below(2)))));
                } else {
                    continue;
                }
                let ids = |v: &str| -> Vec<RowId> {
                    inst.rows(v).into_iter().map(|(id, _)| id).collect()
                };
                let (srcs, tgts) = (ids(&a), ids(&b));
                let mut pairs = Vec::new();
                let relation = r.below(4) == 0;
                for &s in &srcs {
                    if relation {
                        for &t in &tgts {
                            if r.below(2) == 0 {
                                pairs.push((s, t));
                            }
                        }
                    } else if !tgts.is_empty() {
                        pairs.push((s, tgts[r.below(tgts.len())]));
                    }
                }
                inst = inst.with_foreign_key(edge, pairs).unwrap();
            }
            let mut target_edges: Vec<Edge> = carried.iter().map(|(_, e)| e.clone()).collect();
            for k in 0..r.below(3) {
                let (i, j) = (r.below(targets), r.below(targets));
                if i < j {
                    target_edges.push(fk(&format!("t{i}"), &format!("t{j}"), &format!("X{k}")));
                }
            }
            target_edges.sort_unstable();
            target_edges.dedup();
            let mut migration = CompiledMigration::default();
            for (src, tgt) in &vertex_map {
                migration
                    .vertex_remap
                    .insert(src.as_str().into(), tgt.as_str().into());
                migration.surviving_verts.insert(tgt.as_str().into());
            }
            for (src, tgt) in &carried {
                migration.edge_remap.insert(src.clone(), tgt.clone());
            }
            Case {
                inst,
                migration,
                vertex_map,
                internal,
                carried,
                target_edges,
            }
        }
        /// The assignments of a row of its domain to every object that
        /// relate the rows at the two ends of every arrow, found object by
        /// object, each arrow checked once both its ends are assigned.
        fn enumerate_cones(
            inst: &FInstance,
            domains: &[Vec<RowId>],
            arrows: &[(usize, usize, Edge)],
        ) -> Vec<Vec<RowId>> {
            let objects = domains.len();
            // Enumerate the assignments, object by object, checking each
            // arrow once both its ends are assigned.
            let mut cones: Vec<Vec<RowId>> = Vec::new();
            let mut partial: Vec<RowId> = Vec::new();
            let mut choice: Vec<usize> = Vec::new();
            loop {
                if partial.len() == objects {
                    cones.push(partial.clone());
                } else if !domains[partial.len()].is_empty() {
                    choice.push(0);
                    partial.push(domains[partial.len()][0]);
                    let k = partial.len() - 1;
                    if arrows.iter().all(|(i, j, e)| {
                        (*i.max(j) != k) || inst.contains_pair(e, partial[*i], partial[*j])
                    }) {
                        continue;
                    }
                }
                // Advance the last choice, backtracking as needed.
                while let Some(last) = choice.pop() {
                    partial.pop();
                    let k = partial.len();
                    if last + 1 < domains[k].len() {
                        choice.push(last + 1);
                        partial.push(domains[k][last + 1]);
                        if arrows.iter().all(|(i, j, e)| {
                            (*i.max(j) != k) || inst.contains_pair(e, partial[*i], partial[*j])
                        }) {
                            break;
                        }
                    }
                }
                if choice.is_empty() {
                    break;
                }
            }
            cones
        }

        /// Every path of `edges` out of `d` with the vertex it ends at, the
        /// empty path first; finite since the edges go up in index.
        fn paths_from(d: &str, edges: &[Edge]) -> Vec<(Vec<Edge>, String)> {
            let mut paths: Vec<(Vec<Edge>, String)> = vec![(Vec::new(), d.to_owned())];
            let mut next = 0;
            while next < paths.len() {
                let (path, end) = paths[next].clone();
                for edge in edges.iter().filter(|e| *e.src == *end) {
                    let mut longer = path.clone();
                    longer.push(edge.clone());
                    paths.push((longer, edge.tgt.to_string()));
                }
                next += 1;
            }
            paths
        }

        /// A root tuple: the row a cone draws from each fibre component.
        type Tuple = Vec<RowId>;

        /// The oracle's cones at every target vertex whose fibre holds rows:
        /// for each, the cones as their root tuple and, per target edge out
        /// of the vertex into such a vertex, the tuple at the end of that
        /// edge. `None` when a vertex's naive search space is too large.
        #[allow(clippy::type_complexity)]
        fn oracle(case: &Case) -> Option<BTreeMap<String, Vec<(Tuple, BTreeMap<Edge, Tuple>)>>> {
            let mut fibres: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (src, tgt) in &case.vertex_map {
                fibres.entry(tgt.clone()).or_default().push(src.clone());
            }
            let holds = |w: &str| {
                fibres
                    .get(w)
                    .is_some_and(|f| f.iter().any(|c| case.inst.row_count(c) > 0))
            };
            let mut result = BTreeMap::new();
            for d in fibres.keys().filter(|d| holds(d)) {
                let paths = paths_from(d, &case.target_edges);
                let path_index: HashMap<Vec<Edge>, usize> = paths
                    .iter()
                    .enumerate()
                    .map(|(i, (p, _))| (p.clone(), i))
                    .collect();
                // Objects (c, p) at the paths whose end holds rows.
                let mut objects: Vec<(String, usize)> = Vec::new();
                for (i, (_, end)) in paths.iter().enumerate() {
                    if holds(end) {
                        for c in &fibres[end] {
                            objects.push((c.clone(), i));
                        }
                    }
                }
                let object_index: HashMap<(String, usize), usize> = objects
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (o.clone(), i))
                    .collect();
                let mut arrows: Vec<(usize, usize, Edge)> = Vec::new();
                for (i, (c, p)) in objects.iter().enumerate() {
                    for e in case.internal.iter().filter(|e| *e.src == **c) {
                        arrows.push((i, object_index[&(e.tgt.to_string(), *p)], e.clone()));
                    }
                    for (e, image) in case.carried.iter().filter(|(e, _)| *e.src == **c) {
                        let mut longer = paths[*p].0.clone();
                        longer.push(image.clone());
                        if let Some(&j) = path_index
                            .get(&longer)
                            .and_then(|q| object_index.get(&(e.tgt.to_string(), *q)))
                        {
                            arrows.push((i, j, e.clone()));
                        }
                    }
                }
                let domains: Vec<Vec<RowId>> = objects
                    .iter()
                    .map(|(c, _)| case.inst.rows(c).into_iter().map(|(id, _)| id).collect())
                    .collect();
                let bound = domains
                    .iter()
                    .try_fold(1_usize, |acc, d| acc.checked_mul(d.len().max(1)));
                if bound.is_none_or(|b| b > 50_000) {
                    return None;
                }
                let cones = enumerate_cones(&case.inst, &domains, &arrows);
                let fibre = &fibres[d];
                let at =
                    |cone: &Vec<RowId>, c: &String, p: usize| cone[object_index[&(c.clone(), p)]];
                let listed = cones
                    .iter()
                    .map(|cone| {
                        let root: Tuple = fibre.iter().map(|c| at(cone, c, 0)).collect();
                        let ends = case
                            .target_edges
                            .iter()
                            .filter(|e| *e.src == **d && holds(&e.tgt))
                            .map(|e| {
                                let p = path_index[&vec![e.clone()]];
                                let tuple = fibres[&e.tgt.to_string()]
                                    .iter()
                                    .map(|c| at(cone, c, p))
                                    .collect();
                                (e.clone(), tuple)
                            })
                            .collect();
                        (root, ends)
                    })
                    .collect();
                result.insert(d.clone(), listed);
            }
            Some(result)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(2048))]

            /// On every acyclic case, Pi's rows at each vertex are the
            /// oracle's cones (compared by the rows they draw from the
            /// fibre), each target edge between tables is a function, and
            /// it sends each cone to the cone the oracle restricts it to.
            #[test]
            fn pi_is_the_limit_over_the_comma_category(seed in any::<u64>()) {
                let case = case(seed);
                let Some(expected) = oracle(&case) else {
                    return Ok(());
                };
                let mut src_edges = case.internal.clone();
                src_edges.extend(case.carried.iter().map(|(e, _)| e.clone()));
                let (pi, projections) = super::functor_pi_with_hom(
                    &case.inst,
                    &make_test_schema(&[], &src_edges),
                    &make_test_schema(&[], &case.target_edges),
                    &case.migration,
                    1_000_000,
                )
                .unwrap();
                let mut fibres: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for (src, tgt) in &case.vertex_map {
                    fibres.entry(tgt.clone()).or_default().push(src.clone());
                }
                let tuple = |id: RowId, vertex: &str| -> Vec<RowId> {
                    fibres[vertex]
                        .iter()
                        .map(|c| projections[&Name::from(c.as_str())].row_map[&id])
                        .collect()
                };
                let mut tables: Vec<String> = pi.table_names().into_iter().map(ToString::to_string).collect();
                tables.sort_unstable();
                let mut nonempty: Vec<String> = expected
                    .iter()
                    .filter(|(_, cones)| !cones.is_empty())
                    .map(|(d, _)| d.clone())
                    .collect();
                nonempty.sort_unstable();
                prop_assert_eq!(tables, nonempty);
                for (d, cones) in &expected {
                    let mut want: Vec<(Tuple, BTreeMap<Edge, Tuple>)> = cones.clone();
                    want.sort();
                    let mut got: Vec<(Tuple, BTreeMap<Edge, Tuple>)> = Vec::new();
                    // The target edges out of `d` into a vertex with a table.
                    let out: Vec<&Edge> = case
                        .target_edges
                        .iter()
                        .filter(|e| *e.src == **d && expected.contains_key(&e.tgt.to_string()))
                        .collect();
                    for (id, _) in pi.rows(d) {
                        let mut ends = BTreeMap::new();
                        for &edge in &out {
                            let targets: Vec<RowId> = pi.targets_of(edge, id).collect();
                            prop_assert_eq!(targets.len(), 1, "edge {:?} is a function at {:?}", edge, id);
                            ends.insert(edge.clone(), tuple(targets[0], &edge.tgt));
                        }
                        got.push((tuple(id, d), ends));
                    }
                    got.sort();
                    prop_assert_eq!(got, want, "vertex {}", d);
                }
            }

            /// Along the identity of a schema with cycles and loops, Pi of an
            /// instance whose foreign keys are total functions is that
            /// instance, ids and pairs included: every cone is fixed by its
            /// row at the empty path.
            #[test]
            fn pi_along_the_identity_of_a_cyclic_schema_is_the_identity(seed in any::<u64>()) {
                let mut r = Lcg(seed);
                let vertices = 1 + r.below(3);
                let mut inst = FInstance::new();
                let mut migration = CompiledMigration::default();
                for i in 0..vertices {
                    let name = format!("s{i}");
                    inst = inst.with_table(name.clone(), blank(1 + r.below(3))).unwrap();
                    migration.surviving_verts.insert(name.as_str().into());
                }
                let mut edges = Vec::new();
                for k in 0..r.below(5) {
                    let (a, b) = (format!("s{}", r.below(vertices)), format!("s{}", r.below(vertices)));
                    let edge = fk(&a, &b, &format!("e{k}"));
                    let tgts: Vec<RowId> = inst.rows(&b).into_iter().map(|(id, _)| id).collect();
                    let pairs: Vec<(RowId, RowId)> = inst
                        .rows(&a)
                        .into_iter()
                        .map(|(id, _)| (id, tgts[r.below(tgts.len())]))
                        .collect();
                    inst = inst.with_foreign_key(edge.clone(), pairs).unwrap();
                    migration.surviving_edges.insert(edge.clone());
                    edges.push(edge);
                }
                let schema = make_test_schema(&[], &edges);
                let (pi, projections) =
                    super::functor_pi_with_hom(&inst, &schema, &schema, &migration, 1_000_000).unwrap();
                prop_assert_eq!(&pi, &inst);
                for (_, hom) in projections {
                    prop_assert!(hom.row_map.iter().all(|(a, b)| a == b));
                }
            }
        }
    }
}
