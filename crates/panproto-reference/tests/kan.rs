#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples and algebraic identities of the relational Kan
//! extensions on tiny instances.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::encode::{
    self, CellLayout, EdgeLayout, GraphKanError, GraphLayout, GraphSide,
};
use panproto_reference::enumerate::kan_extensions as bounded;
use panproto_reference::migration::Migration;
use panproto_reference::model::{GEdge, Graph, Node, SEdge, Table, Tree};
use panproto_reference::rel::{self, FunctorError, Hom, KanError, RelFunctor, RelSchema};
use panproto_reference::schema::Schema;
use panproto_reference::value::{Presence, Value};

fn kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (["int".to_owned()].into(), ["record".to_owned()].into())
}

fn rel_of(s: &Schema) -> rel::RelSchema {
    let (k, n) = kinds();
    rel::classify(s, &k, &n).unwrap()
}

fn functor(src: &Schema, tgt: &Schema, m: &Migration) -> RelFunctor {
    rel::entity_functor(&rel_of(src), &rel_of(tgt), m)
        .unwrap()
        .into_kan()
        .unwrap()
}

fn row(t: &str, cells: &[(&str, Value)]) -> (String, BTreeMap<String, Value>) {
    (
        t.to_owned(),
        cells
            .iter()
            .map(|(c, v)| ((*c).to_owned(), v.clone()))
            .collect(),
    )
}

fn input_nulls(table: &Table) -> BTreeSet<u64> {
    table
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values())
        .filter_map(|value| match value {
            Value::LabeledNull(id) => Some(*id),
            _ => None,
        })
        .collect()
}

fn assert_sigma_provenance(input: &Table, output: &Table, provenance: &rel::SigmaProvenance) {
    let input_nulls = input_nulls(input);
    for (id, (row, column)) in &provenance.fresh {
        assert!(!input_nulls.contains(id));
        assert_eq!(
            output
                .rows
                .get(row)
                .and_then(|(_, cells)| cells.get(column)),
            Some(&Value::LabeledNull(*id)),
        );
    }
    for (id, representative) in &provenance.merged {
        assert!(input_nulls.contains(id));
        assert!(
            output
                .rows
                .values()
                .flat_map(|(_, cells)| cells.values())
                .any(|value| panproto_reference::canonical::value_bytes(value)
                    == panproto_reference::canonical::value_bytes(representative))
        );
    }
}

fn assert_graph_provenance(
    graph: &Graph,
    nodes: &BTreeMap<u32, BTreeSet<u32>>,
    edges: &BTreeMap<u32, BTreeSet<u32>>,
    expected_nodes: &BTreeMap<u32, BTreeSet<u32>>,
    expected_edges: &BTreeMap<u32, BTreeSet<u32>>,
) {
    assert_eq!(
        nodes.keys().copied().collect::<BTreeSet<_>>(),
        graph.nodes.keys().copied().collect()
    );
    assert_eq!(
        edges.keys().copied().collect::<BTreeSet<_>>(),
        graph.edges.keys().copied().collect()
    );
    assert_eq!(nodes, expected_nodes);
    assert_eq!(edges, expected_edges);
}

type HomKey = (Vec<(u32, u32)>, Vec<(u64, Vec<u8>)>);

fn hom_key(hom: &Hom, domain: &Table) -> HomKey {
    let used = input_nulls(domain);
    let nulls = used
        .into_iter()
        .map(|id| {
            let image = hom
                .nulls
                .get(&id)
                .cloned()
                .unwrap_or(Value::LabeledNull(id));
            (
                id,
                panproto_reference::canonical::value_bytes(&image).unwrap(),
            )
        })
        .collect();
    (hom.rows.iter().map(|(a, b)| (*a, *b)).collect(), nulls)
}

fn hom_from_rows(domain: &Table, codomain: &Table, rows: BTreeMap<u32, u32>) -> Option<Hom> {
    let mut nulls = BTreeMap::new();
    for (source, (_, source_cells)) in &domain.rows {
        let target = rows.get(source)?;
        let (_, target_cells) = codomain.rows.get(target)?;
        for (column, value) in source_cells {
            let image = target_cells.get(column)?;
            match value {
                Value::LabeledNull(id) => {
                    if nulls.get(id).is_some_and(|prior: &Value| {
                        panproto_reference::canonical::value_bytes(prior)
                            != panproto_reference::canonical::value_bytes(image)
                    }) {
                        return None;
                    }
                    nulls.insert(*id, image.clone());
                }
                _ if panproto_reference::canonical::value_bytes(value)
                    != panproto_reference::canonical::value_bytes(image) =>
                {
                    return None;
                }
                _ => {}
            }
        }
    }
    let hom = Hom { rows, nulls };
    hom.check(domain, codomain).is_ok().then_some(hom)
}

fn sigma_on_hom(f: &RelFunctor, domain: &Table, codomain: &Table, hom: &Hom) -> Option<Hom> {
    hom.check(domain, codomain).ok()?;
    let sigma_domain = rel::sigma(f, domain).ok()?.0;
    let sigma_codomain = rel::sigma(f, codomain).ok()?.0;
    let rows = sigma_domain
        .rows
        .keys()
        .map(|row| Some((*row, *hom.rows.get(row)?)))
        .collect::<Option<_>>()?;
    hom_from_rows(&sigma_domain, &sigma_codomain, rows)
}

fn delta_on_hom(f: &RelFunctor, domain: &Table, codomain: &Table, hom: &Hom) -> Option<Hom> {
    hom.check(domain, codomain).ok()?;
    let (delta_domain, domain_copies) = rel::delta(f, domain).ok()?;
    let (delta_codomain, codomain_copies) = rel::delta(f, codomain).ok()?;
    let codomain_index: BTreeMap<_, _> = codomain_copies
        .iter()
        .map(|(copy, origin)| (origin.clone(), *copy))
        .collect();
    let rows = domain_copies
        .iter()
        .map(|(copy, (table, row))| {
            let image = *hom.rows.get(row)?;
            Some((*copy, *codomain_index.get(&(table.clone(), image))?))
        })
        .collect::<Option<_>>()?;
    hom_from_rows(&delta_domain, &delta_codomain, rows)
}

fn pi_on_hom(
    f: &RelFunctor,
    domain: &Table,
    codomain: &Table,
    hom: &Hom,
    max_product_size: usize,
) -> Option<Hom> {
    hom.check(domain, codomain).ok()?;
    let (pi_domain, domain_tuples) = rel::pi(f, domain, max_product_size).ok()?;
    let (pi_codomain, codomain_tuples) = rel::pi(f, codomain, max_product_size).ok()?;
    let codomain_index: BTreeMap<_, _> = codomain_tuples
        .iter()
        .map(|(product, origin)| (origin.clone(), *product))
        .collect();
    let rows = domain_tuples
        .iter()
        .map(|(product, (table, tuple))| {
            let tuple = tuple
                .iter()
                .map(|row| hom.rows.get(row).copied())
                .collect::<Option<Vec<_>>>()?;
            Some((*product, *codomain_index.get(&(table.clone(), tuple))?))
        })
        .collect::<Option<_>>()?;
    hom_from_rows(&pi_domain, &pi_codomain, rows)
}

fn sigma_clash_features(f: &RelFunctor, table: &Table) -> BTreeSet<(Vec<u8>, Vec<u8>)> {
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Term {
        Slot(u32, String),
        Null(u64),
        Constant(Vec<u8>),
    }

    let mut adjacent: BTreeMap<Term, BTreeSet<Term>> = BTreeMap::new();
    for (row, (source_table, cells)) in &table.rows {
        let Some(target_table) = f.tables.get(source_table) else {
            continue;
        };
        for target_column in f
            .tgt
            .columns
            .get(target_table)
            .into_iter()
            .flatten()
            .map(|(column, _)| column)
        {
            let slot = Term::Slot(*row, target_column.clone());
            adjacent.entry(slot.clone()).or_default();
            for value in f
                .columns
                .iter()
                .filter(|((table, _), image)| table == source_table && *image == target_column)
                .filter_map(|((_, column), _)| cells.get(column))
            {
                let value = match value {
                    Value::LabeledNull(id) => Term::Null(*id),
                    _ => Term::Constant(panproto_reference::canonical::value_bytes(value).unwrap()),
                };
                adjacent
                    .entry(slot.clone())
                    .or_default()
                    .insert(value.clone());
                adjacent.entry(value).or_default().insert(slot.clone());
            }
        }
    }

    let mut features = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for start in adjacent.keys() {
        if !seen.insert(start.clone()) {
            continue;
        }
        let mut stack = vec![start.clone()];
        let mut constants = BTreeSet::new();
        while let Some(term) = stack.pop() {
            if let Term::Constant(bytes) = &term {
                constants.insert(bytes.clone());
            }
            for next in &adjacent[&term] {
                if seen.insert(next.clone()) {
                    stack.push(next.clone());
                }
            }
        }
        let constants = constants.into_iter().collect::<Vec<_>>();
        for left in 0..constants.len() {
            for right in left + 1..constants.len() {
                features.insert((constants[left].clone(), constants[right].clone()));
            }
        }
    }
    features
}

type CellKey = (u32, String);
type RelationKey = (SEdge, u32, u32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ExpectedOrigins {
    rows: BTreeMap<u32, BTreeSet<u32>>,
    cells: BTreeMap<CellKey, BTreeSet<CellKey>>,
    relations: BTreeMap<RelationKey, BTreeSet<RelationKey>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SigmaTerm {
    Slot(u32, String),
    Null(u64),
    Constant(Vec<u8>),
}

fn sigma_term(value: &Value) -> SigmaTerm {
    match value {
        Value::LabeledNull(id) => SigmaTerm::Null(*id),
        _ => SigmaTerm::Constant(panproto_reference::canonical::value_bytes(value).unwrap()),
    }
}

fn sigma_components(
    f: &RelFunctor,
    input: &Table,
    output: &Table,
    fresh: &BTreeMap<u64, (u32, String)>,
) -> BTreeMap<SigmaTerm, usize> {
    let mut adjacent: BTreeMap<SigmaTerm, BTreeSet<SigmaTerm>> = BTreeMap::new();
    let fresh_by_slot: BTreeMap<_, _> =
        fresh.iter().map(|(id, slot)| (slot.clone(), *id)).collect();
    for (row, (_, output_cells)) in &output.rows {
        let Some((source_table, input_cells)) = input.rows.get(row) else {
            continue;
        };
        for column in output_cells.keys() {
            let slot = SigmaTerm::Slot(*row, column.clone());
            adjacent.entry(slot.clone()).or_default();
            let mut values = f
                .columns
                .iter()
                .filter(|((table, _), image)| table == source_table && *image == column)
                .filter_map(|((_, source_column), _)| input_cells.get(source_column))
                .map(sigma_term)
                .collect::<Vec<_>>();
            if let Some(id) = fresh_by_slot.get(&(*row, column.clone())) {
                values.push(SigmaTerm::Null(*id));
            }
            for value in values {
                adjacent
                    .entry(slot.clone())
                    .or_default()
                    .insert(value.clone());
                adjacent.entry(value).or_default().insert(slot.clone());
            }
        }
    }
    let mut components = BTreeMap::new();
    let mut next_component = 0usize;
    for start in adjacent.keys() {
        if components.contains_key(start) {
            continue;
        }
        let mut stack = vec![start.clone()];
        components.insert(start.clone(), next_component);
        while let Some(term) = stack.pop() {
            for neighbor in &adjacent[&term] {
                if components
                    .insert(neighbor.clone(), next_component)
                    .is_none()
                {
                    stack.push(neighbor.clone());
                }
            }
        }
        next_component += 1;
    }
    components
}

fn expected_sigma_provenance(
    f: &RelFunctor,
    input: &Table,
    output: &Table,
) -> rel::SigmaProvenance {
    let mut next_null = input_nulls(input)
        .last()
        .map_or(0, |id| id.checked_add(1).unwrap());
    let mut provenance = rel::SigmaProvenance::default();
    for (row, (_, output_cells)) in &output.rows {
        let (source_table, _) = &input.rows[row];
        for column in output_cells.keys() {
            let has_preimage = f
                .columns
                .iter()
                .any(|((table, _), image)| table == source_table && image == column);
            if !has_preimage {
                provenance.fresh.insert(next_null, (*row, column.clone()));
                next_null = next_null.checked_add(1).unwrap();
            }
        }
    }
    let components = sigma_components(f, input, output, &provenance.fresh);
    for null in input_nulls(input) {
        let Some(component) = components.get(&SigmaTerm::Null(null)) else {
            continue;
        };
        let representative = components
            .iter()
            .find_map(|(term, candidate)| {
                (*candidate == *component)
                    .then_some(term)
                    .and_then(|term| match term {
                        SigmaTerm::Constant(bytes) => {
                            panproto_reference::canonical::decode_value(bytes)
                        }
                        _ => None,
                    })
            })
            .or_else(|| {
                components
                    .iter()
                    .filter_map(|(term, candidate)| {
                        (*candidate == *component).then_some(term).and_then(|term| {
                            if let SigmaTerm::Null(id) = term {
                                Some(Value::LabeledNull(*id))
                            } else {
                                None
                            }
                        })
                    })
                    .min_by_key(|value| panproto_reference::canonical::value_bytes(value).unwrap())
            });
        if let Some(representative) = representative
            && representative != Value::LabeledNull(null)
        {
            provenance.merged.insert(null, representative);
        }
    }
    provenance
}

fn literal_graph_and_layout(table: &Table, side: &GraphSide) -> (Graph, GraphLayout) {
    let mut graph = Graph::default();
    let mut layout = GraphLayout::default();
    let mut next_node = table
        .rows
        .keys()
        .next_back()
        .map_or(0, |row| row.checked_add(1).unwrap());
    let mut next_edge = 0u32;
    for (row, (table_name, cells)) in &table.rows {
        let table_node = Node::new(*row, table_name);
        graph.nodes.insert(*row, table_node.clone());
        layout.table_nodes.insert(*row, table_node);
        for (column, value) in cells {
            let arrow = &side.columns[table_name][column];
            let leaf_id = next_node;
            next_node = next_node.checked_add(1).unwrap();
            let mut leaf = Node::new(leaf_id, &arrow.tgt);
            leaf.value = Some(if *value == Value::Null {
                Presence::Null
            } else {
                Presence::Present(value.clone())
            });
            graph.nodes.insert(leaf_id, leaf.clone());
            let edge_id = next_edge;
            next_edge = next_edge.checked_add(1).unwrap();
            graph.edges.insert(
                edge_id,
                GEdge {
                    src: *row,
                    tgt: leaf_id,
                    label: arrow.clone(),
                    position: None,
                },
            );
            layout.cells.insert(
                (*row, column.clone()),
                CellLayout {
                    root: leaf_id,
                    attribute: EdgeLayout {
                        id: Some(edge_id),
                        position: None,
                        has_source: true,
                    },
                    nodes: BTreeMap::from([(leaf_id, leaf)]),
                    edges: BTreeMap::new(),
                },
            );
        }
    }
    for (relation, source, target) in &table.pairs {
        let edge_id = next_edge;
        next_edge = next_edge.checked_add(1).unwrap();
        graph.edges.insert(
            edge_id,
            GEdge {
                src: *source,
                tgt: *target,
                label: relation.clone(),
                position: None,
            },
        );
        layout.relations.insert(
            (relation.clone(), *source, *target),
            EdgeLayout {
                id: Some(edge_id),
                position: None,
                has_source: true,
            },
        );
    }
    layout.reserved_node_ids = graph.nodes.keys().copied().collect();
    layout.reserved_edge_ids = graph.edges.keys().copied().collect();
    (graph, layout)
}

fn moved_leaf(cell: &CellLayout, anchor: &str) -> CellLayout {
    assert!(cell.edges.is_empty());
    let mut moved = cell.clone();
    for node in moved.nodes.values_mut() {
        anchor.clone_into(&mut node.anchor);
    }
    moved
}

fn relation_is_injective(f: &RelFunctor, source: &SEdge) -> bool {
    let Some(image) = f.relations.get(source) else {
        return false;
    };
    f.relations
        .values()
        .filter(|candidate| *candidate == image)
        .count()
        == 1
        && f.fibre(&image.src).len() == 1
        && f.fibre(&image.tgt).len() == 1
}

fn relation_layout(
    input: &GraphLayout,
    contributors: &BTreeSet<RelationKey>,
    retain_id: bool,
) -> EdgeLayout {
    let entries = contributors
        .iter()
        .filter_map(|key| input.relations.get(key))
        .collect::<Vec<_>>();
    EdgeLayout {
        id: retain_id
            .then(|| entries.first().and_then(|entry| entry.id))
            .flatten(),
        position: entries.iter().filter_map(|entry| entry.position).min(),
        has_source: !entries.is_empty(),
    }
}

fn contributor_maps(
    graph: &Graph,
    output_layout: &GraphLayout,
    input_layout: &GraphLayout,
    origins: &ExpectedOrigins,
) -> (BTreeMap<u32, BTreeSet<u32>>, BTreeMap<u32, BTreeSet<u32>>) {
    let mut nodes: BTreeMap<_, BTreeSet<_>> = graph
        .nodes
        .keys()
        .map(|node| (*node, BTreeSet::new()))
        .collect();
    let mut edges: BTreeMap<_, BTreeSet<_>> = graph
        .edges
        .keys()
        .map(|edge| (*edge, BTreeSet::new()))
        .collect();
    for (row, contributors) in &origins.rows {
        nodes.get_mut(row).unwrap().extend(contributors);
    }
    for (cell, output_cell) in &output_layout.cells {
        let source_cells = origins
            .cells
            .get(cell)
            .into_iter()
            .flatten()
            .filter_map(|key| input_layout.cells.get(key))
            .collect::<Vec<_>>();
        let source_nodes = source_cells
            .iter()
            .flat_map(|cell| cell.nodes.keys().copied())
            .collect::<BTreeSet<_>>();
        let source_edges = source_cells
            .iter()
            .flat_map(|cell| {
                cell.attribute
                    .id
                    .into_iter()
                    .chain(cell.edges.keys().copied())
            })
            .collect::<BTreeSet<_>>();
        for node in output_cell.nodes.keys() {
            nodes.get_mut(node).unwrap().extend(&source_nodes);
        }
        if let Some(edge) = output_cell.attribute.id {
            edges.get_mut(&edge).unwrap().extend(&source_edges);
        }
        for edge in output_cell.edges.keys() {
            edges.get_mut(edge).unwrap().extend(&source_edges);
        }
    }
    for (relation, output_layout) in &output_layout.relations {
        let source_edges = origins
            .relations
            .get(relation)
            .into_iter()
            .flatten()
            .filter_map(|key| input_layout.relations.get(key)?.id)
            .collect::<BTreeSet<_>>();
        if let Some(edge) = output_layout.id {
            edges.get_mut(&edge).unwrap().extend(source_edges);
        }
    }
    (nodes, edges)
}

fn literal_whole(schema: &RelSchema) -> GraphSide {
    GraphSide {
        tables: schema.tables.clone(),
        relations: schema.relations.clone(),
        columns: schema.columns.clone(),
        value_vertices: schema.value_vertices.clone(),
        inner_edges: schema.inner_edges.clone(),
    }
}

fn literal_kept(functor: &RelFunctor) -> GraphSide {
    let tables = functor.tables.keys().cloned().collect::<BTreeSet<_>>();
    let mut columns = tables
        .iter()
        .map(|table| (table.clone(), BTreeMap::new()))
        .collect::<BTreeMap<_, _>>();
    for (table, column) in functor.columns.keys() {
        let arrow = functor
            .src
            .columns
            .get(table)
            .and_then(|source_columns| source_columns.get(column))
            .unwrap_or_else(|| panic!("kept column {table}.{column} has no source arrow"));
        columns
            .entry(table.clone())
            .or_default()
            .insert(column.clone(), arrow.clone());
    }
    let mut value_vertices = columns
        .values()
        .flat_map(|table_columns| table_columns.values().map(|arrow| arrow.tgt.clone()))
        .collect::<BTreeSet<_>>();
    loop {
        let targets = functor
            .src
            .inner_edges
            .iter()
            .filter(|edge| value_vertices.contains(&edge.src))
            .map(|edge| edge.tgt.clone())
            .collect::<Vec<_>>();
        let before = value_vertices.len();
        value_vertices.extend(targets);
        if value_vertices.len() == before {
            break;
        }
    }
    let inner_edges = functor
        .src
        .inner_edges
        .iter()
        .filter(|edge| value_vertices.contains(&edge.src))
        .cloned()
        .collect();
    GraphSide {
        tables,
        relations: functor.relations.keys().cloned().collect(),
        columns,
        value_vertices,
        inner_edges,
    }
}

fn expected_sigma_adapter(
    f: &RelFunctor,
    input: &Table,
    input_layout: &GraphLayout,
    output: &Table,
) -> (rel::SigmaProvenance, GraphLayout, ExpectedOrigins) {
    let provenance = expected_sigma_provenance(f, input, output);
    let components = sigma_components(f, input, output, &provenance.fresh);
    let target = literal_whole(&f.tgt);
    let mut layout = GraphLayout {
        reserved_node_ids: input_layout.reserved_node_ids.clone(),
        reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
        ..GraphLayout::default()
    };
    let mut origins = ExpectedOrigins::default();
    for (row, (target_table, output_cells)) in &output.rows {
        if let Some(node) = input_layout.table_nodes.get(row) {
            layout.table_nodes.insert(*row, node.clone());
            origins.rows.insert(*row, BTreeSet::from([*row]));
        }
        for (target_column, output_value) in output_cells {
            let target_arrow = &target.columns[target_table][target_column];
            let component = components[&SigmaTerm::Slot(*row, target_column.clone())];
            let mut contributors = BTreeSet::new();
            let mut candidates = Vec::new();
            for (source_row, (source_table, source_cells)) in &input.rows {
                for ((table, source_column), image_column) in &f.columns {
                    if table != source_table {
                        continue;
                    }
                    let Some(value) = source_cells.get(source_column) else {
                        continue;
                    };
                    if components.get(&sigma_term(value)) != Some(&component) {
                        continue;
                    }
                    contributors.insert((*source_row, source_column.clone()));
                    let image_table = &f.tables[source_table];
                    let image_arrow = &target.columns[image_table][image_column];
                    candidates.push((
                        source_table.clone(),
                        *source_row,
                        source_column.clone(),
                        image_arrow,
                        value,
                    ));
                }
            }
            origins
                .cells
                .insert((*row, target_column.clone()), contributors);
            candidates.sort_by(|left, right| {
                (&left.0, left.1, &left.2).cmp(&(&right.0, right.1, &right.2))
            });
            if let Some((_, source_row, source_column, image_arrow, _)) = candidates
                .into_iter()
                .find(|(_, _, _, image_arrow, value)| {
                    image_arrow.tgt == target_arrow.tgt
                        && panproto_reference::canonical::value_bytes(value)
                            == panproto_reference::canonical::value_bytes(output_value)
                })
                && let Some(cell) = input_layout.cells.get(&(source_row, source_column))
            {
                layout.cells.insert(
                    (*row, target_column.clone()),
                    moved_leaf(cell, &image_arrow.tgt),
                );
            }
            assert!(target.columns[target_table].contains_key(target_column));
        }
    }
    for (target_edge, source, target_row) in &output.pairs {
        let contributors = f
            .relations
            .iter()
            .filter(|(_, image)| *image == target_edge)
            .filter(|(source_edge, _)| {
                input
                    .pairs
                    .contains(&((*source_edge).clone(), *source, *target_row))
            })
            .map(|(source_edge, _)| (source_edge.clone(), *source, *target_row))
            .collect::<BTreeSet<_>>();
        let retain = contributors.len() == 1
            && contributors
                .first()
                .is_some_and(|(edge, _, _)| relation_is_injective(f, edge));
        let key = (target_edge.clone(), *source, *target_row);
        layout.relations.insert(
            key.clone(),
            relation_layout(input_layout, &contributors, retain),
        );
        origins.relations.insert(key, contributors);
    }
    (provenance, layout, origins)
}

fn expected_delta_provenance(f: &RelFunctor, input: &Table) -> rel::DeltaProvenance {
    let mut next = input
        .rows
        .keys()
        .next_back()
        .map_or(0, |row| row.checked_add(1).unwrap());
    let mut used = BTreeSet::new();
    let mut provenance = BTreeMap::new();
    for (source_table, target_table) in &f.tables {
        for (row, (owner, _)) in &input.rows {
            if owner != target_table {
                continue;
            }
            let output = if used.insert(*row) {
                *row
            } else {
                let fresh = next;
                next = next.checked_add(1).unwrap();
                fresh
            };
            provenance.insert(output, (source_table.clone(), *row));
        }
    }
    provenance
}

fn expected_delta_adapter(
    f: &RelFunctor,
    input: &Table,
    input_layout: &GraphLayout,
    output: &Table,
) -> (rel::DeltaProvenance, GraphLayout, ExpectedOrigins) {
    let provenance = expected_delta_provenance(f, input);
    let source = literal_kept(f);
    let mut layout = GraphLayout {
        reserved_node_ids: input_layout.reserved_node_ids.clone(),
        reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
        ..GraphLayout::default()
    };
    let mut origins = ExpectedOrigins::default();
    for (row, (source_table, original)) in &provenance {
        if let Some(node) = input_layout.table_nodes.get(original) {
            layout.table_nodes.insert(*row, node.clone());
        }
        origins.rows.insert(*row, BTreeSet::from([*original]));
        for column in output.rows[row].1.keys() {
            let target_column = &f.columns[&(source_table.clone(), column.clone())];
            origins.cells.insert(
                (*row, column.clone()),
                BTreeSet::from([(*original, target_column.clone())]),
            );
            if let Some(cell) = input_layout.cells.get(&(*original, target_column.clone())) {
                let arrow = &source.columns[source_table][column];
                layout
                    .cells
                    .insert((*row, column.clone()), moved_leaf(cell, &arrow.tgt));
            }
        }
    }
    for (source_edge, source_row, target_row) in &output.pairs {
        let (_, original_source) = provenance[source_row];
        let (_, original_target) = provenance[target_row];
        let contributors = BTreeSet::from([(
            f.relations[source_edge].clone(),
            original_source,
            original_target,
        )]);
        let key = (source_edge.clone(), *source_row, *target_row);
        layout.relations.insert(
            key.clone(),
            relation_layout(
                input_layout,
                &contributors,
                relation_is_injective(f, source_edge),
            ),
        );
        origins.relations.insert(key, contributors);
    }
    (provenance, layout, origins)
}

fn table_tuples(table: &Table, fibre: &[String]) -> Vec<Vec<u32>> {
    let mut tuples = vec![Vec::new()];
    for owner in fibre {
        let rows = table
            .rows
            .iter()
            .filter_map(|(row, (candidate, _))| (candidate == owner).then_some(*row))
            .collect::<Vec<_>>();
        tuples = tuples
            .into_iter()
            .flat_map(|prefix| {
                rows.iter().map(move |row| {
                    let mut tuple = prefix.clone();
                    tuple.push(*row);
                    tuple
                })
            })
            .collect();
    }
    tuples
}

fn tuple_cells(
    f: &RelFunctor,
    input: &Table,
    target_table: &str,
    fibre: &[String],
    tuple: &[u32],
) -> Option<BTreeMap<String, Value>> {
    let mut output = BTreeMap::new();
    for target_column in f.tgt.columns[target_table].keys() {
        let mut value: Option<Value> = None;
        for (index, source_table) in fibre.iter().enumerate() {
            let cells = &input.rows[&tuple[index]].1;
            for ((table, source_column), image) in &f.columns {
                if table == source_table && image == target_column {
                    let candidate = cells.get(source_column)?;
                    match &value {
                        None => value = Some(candidate.clone()),
                        Some(prior)
                            if panproto_reference::canonical::value_bytes(prior)
                                == panproto_reference::canonical::value_bytes(candidate) => {}
                        Some(_) => return None,
                    }
                }
            }
        }
        output.insert(target_column.clone(), value?);
    }
    Some(output)
}

fn expected_pi_provenance(f: &RelFunctor, input: &Table) -> rel::PiProvenance {
    let mut next = input
        .rows
        .keys()
        .next_back()
        .map_or(0, |row| row.checked_add(1).unwrap());
    let mut order = f
        .tgt
        .tables
        .iter()
        .map(|table| (f.fibre(table).first().cloned(), table.clone()))
        .collect::<Vec<_>>();
    order.sort_by(|(left_fibre, left), (right_fibre, right)| {
        left_fibre
            .is_none()
            .cmp(&right_fibre.is_none())
            .then_with(|| left_fibre.cmp(right_fibre))
            .then_with(|| left.cmp(right))
    });
    let mut provenance = BTreeMap::new();
    for (_, target_table) in order {
        let fibre = f.fibre(&target_table);
        for tuple in table_tuples(input, &fibre) {
            if tuple_cells(f, input, &target_table, &fibre, &tuple).is_none() {
                continue;
            }
            let row = if let [only] = tuple.as_slice() {
                *only
            } else {
                let fresh = next;
                next = next.checked_add(1).unwrap();
                fresh
            };
            provenance.insert(row, (target_table.clone(), tuple));
        }
    }
    provenance
}

fn expected_pi_adapter(
    f: &RelFunctor,
    input: &Table,
    input_layout: &GraphLayout,
    output: &Table,
) -> (rel::PiProvenance, GraphLayout, ExpectedOrigins) {
    let provenance = expected_pi_provenance(f, input);
    let target = literal_whole(&f.tgt);
    let mut layout = GraphLayout {
        reserved_node_ids: input_layout.reserved_node_ids.clone(),
        reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
        ..GraphLayout::default()
    };
    let mut origins = ExpectedOrigins::default();
    for (row, (target_table, tuple)) in &provenance {
        let fibre = f.fibre(target_table);
        if let [only] = tuple.as_slice()
            && let Some(node) = input_layout.table_nodes.get(only)
        {
            layout.table_nodes.insert(*row, node.clone());
        }
        origins.rows.insert(*row, tuple.iter().copied().collect());
        for (target_column, output_value) in &output.rows[row].1 {
            let mut contributors = BTreeSet::new();
            let mut candidates = Vec::new();
            for (source_table, source_row) in fibre.iter().zip(tuple) {
                for ((table, source_column), image) in &f.columns {
                    if table == source_table && image == target_column {
                        contributors.insert((*source_row, source_column.clone()));
                        candidates.push((
                            source_table.clone(),
                            *source_row,
                            source_column.clone(),
                            &input.rows[source_row].1[source_column],
                        ));
                    }
                }
            }
            origins
                .cells
                .insert((*row, target_column.clone()), contributors);
            candidates.sort_by(|left, right| {
                (&left.0, left.1, &left.2).cmp(&(&right.0, right.1, &right.2))
            });
            if let Some((_, source_row, source_column, _)) =
                candidates.into_iter().find(|(_, _, _, value)| {
                    panproto_reference::canonical::value_bytes(value)
                        == panproto_reference::canonical::value_bytes(output_value)
                })
                && let Some(cell) = input_layout.cells.get(&(source_row, source_column))
            {
                let arrow = &target.columns[target_table][target_column];
                layout
                    .cells
                    .insert((*row, target_column.clone()), moved_leaf(cell, &arrow.tgt));
            }
        }
    }
    for (target_edge, source_row, target_row) in &output.pairs {
        let (_, source_tuple) = &provenance[source_row];
        let (_, target_tuple) = &provenance[target_row];
        let source_fibre = f.fibre(&target_edge.src);
        let target_fibre = f.fibre(&target_edge.tgt);
        let contributors = f
            .relations
            .iter()
            .filter(|(_, image)| *image == target_edge)
            .filter_map(|(source_edge, _)| {
                let source = source_fibre
                    .iter()
                    .position(|table| table == &source_edge.src)
                    .and_then(|index| source_tuple.get(index))?;
                let target = target_fibre
                    .iter()
                    .position(|table| table == &source_edge.tgt)
                    .and_then(|index| target_tuple.get(index))?;
                input
                    .pairs
                    .contains(&(source_edge.clone(), *source, *target))
                    .then(|| (source_edge.clone(), *source, *target))
            })
            .collect::<BTreeSet<_>>();
        let retain = contributors.len() == 1
            && contributors
                .first()
                .is_some_and(|(edge, _, _)| relation_is_injective(f, edge));
        let key = (target_edge.clone(), *source_row, *target_row);
        layout.relations.insert(
            key.clone(),
            relation_layout(input_layout, &contributors, retain),
        );
        origins.relations.insert(key, contributors);
    }
    (provenance, layout, origins)
}

type ExpectedGraph = (
    Graph,
    GraphLayout,
    BTreeMap<u32, BTreeSet<u32>>,
    BTreeMap<u32, BTreeSet<u32>>,
);

fn claim_graph_id(candidate: Option<u32>, claimed: &mut BTreeSet<u32>, next: &mut u32) -> u32 {
    if let Some(id) = candidate.filter(|id| !claimed.contains(id)) {
        claimed.insert(id);
        return id;
    }
    let id = *next;
    *next = next.checked_add(1).unwrap();
    assert!(claimed.insert(id));
    id
}

fn materialize_relations(
    output: &Table,
    layout: &GraphLayout,
    graph: &mut Graph,
    actual_layout: &mut GraphLayout,
    claimed_edges: &mut BTreeSet<u32>,
    next_edge: &mut u32,
) {
    for (relation, source, target) in &output.pairs {
        let requested = layout
            .relations
            .get(&(relation.clone(), *source, *target))
            .copied()
            .unwrap_or_default();
        let edge_id = claim_graph_id(requested.id, claimed_edges, next_edge);
        graph.edges.insert(
            edge_id,
            GEdge {
                src: *source,
                tgt: *target,
                label: relation.clone(),
                position: requested.position,
            },
        );
        actual_layout.relations.insert(
            (relation.clone(), *source, *target),
            EdgeLayout {
                id: Some(edge_id),
                ..requested
            },
        );
    }
}

fn materialize_table_nodes(
    output: &Table,
    layout: &GraphLayout,
    graph: &mut Graph,
    actual_layout: &mut GraphLayout,
) {
    for (row, (table, _)) in &output.rows {
        let mut node = layout
            .table_nodes
            .get(row)
            .cloned()
            .unwrap_or_else(|| Node::new(*row, table));
        node.id = *row;
        node.anchor.clone_from(table);
        actual_layout.table_nodes.insert(*row, node.clone());
        graph.nodes.insert(*row, node);
    }
}

fn expected_graph(
    output: &Table,
    side: &GraphSide,
    layout: &GraphLayout,
    input_layout: &GraphLayout,
    origins: &ExpectedOrigins,
) -> ExpectedGraph {
    let mut graph = Graph::default();
    let mut actual_layout = GraphLayout::default();
    let mut claimed_nodes = output.rows.keys().copied().collect::<BTreeSet<_>>();
    let mut claimed_edges = BTreeSet::new();
    let mut next_node = output
        .rows
        .keys()
        .copied()
        .chain(layout.reserved_node_ids.iter().copied())
        .max()
        .map_or(0, |id| id.checked_add(1).unwrap());
    let mut next_edge = layout
        .reserved_edge_ids
        .iter()
        .next_back()
        .map_or(0, |id| id.checked_add(1).unwrap());
    materialize_table_nodes(output, layout, &mut graph, &mut actual_layout);
    for (row, (table, cells)) in &output.rows {
        for (column, value) in cells {
            let arrow = &side.columns[table][column];
            let (node, edge, materialized) = if let Some(cell) =
                layout.cells.get(&(*row, column.clone()))
            {
                assert_eq!(cell.nodes.len(), 1);
                assert!(cell.edges.is_empty());
                let source_node = cell.root;
                let node_id = claim_graph_id(Some(source_node), &mut claimed_nodes, &mut next_node);
                let mut node = cell.nodes[&source_node].clone();
                node.id = node_id;
                let edge_id = claim_graph_id(cell.attribute.id, &mut claimed_edges, &mut next_edge);
                let edge = GEdge {
                    src: *row,
                    tgt: node_id,
                    label: arrow.clone(),
                    position: cell.attribute.position,
                };
                let materialized = CellLayout {
                    root: node_id,
                    attribute: EdgeLayout {
                        id: Some(edge_id),
                        ..cell.attribute
                    },
                    nodes: BTreeMap::from([(node_id, node.clone())]),
                    edges: BTreeMap::new(),
                };
                ((node_id, node), (edge_id, edge), materialized)
            } else {
                let node_id = claim_graph_id(None, &mut claimed_nodes, &mut next_node);
                let mut node = Node::new(node_id, &arrow.tgt);
                node.value = Some(if *value == Value::Null {
                    Presence::Null
                } else {
                    Presence::Present(value.clone())
                });
                let edge_id = claim_graph_id(None, &mut claimed_edges, &mut next_edge);
                let edge = GEdge {
                    src: *row,
                    tgt: node_id,
                    label: arrow.clone(),
                    position: None,
                };
                let materialized = CellLayout {
                    root: node_id,
                    attribute: EdgeLayout {
                        id: Some(edge_id),
                        position: None,
                        has_source: false,
                    },
                    nodes: BTreeMap::from([(node_id, node.clone())]),
                    edges: BTreeMap::new(),
                };
                ((node_id, node), (edge_id, edge), materialized)
            };
            graph.nodes.insert(node.0, node.1);
            graph.edges.insert(edge.0, edge.1);
            actual_layout
                .cells
                .insert((*row, column.clone()), materialized);
        }
    }
    materialize_relations(
        output,
        layout,
        &mut graph,
        &mut actual_layout,
        &mut claimed_edges,
        &mut next_edge,
    );
    actual_layout.reserved_node_ids = graph.nodes.keys().copied().collect();
    actual_layout.reserved_edge_ids = graph.edges.keys().copied().collect();
    let (nodes, edges) = contributor_maps(&graph, &actual_layout, input_layout, origins);
    (graph, actual_layout, nodes, edges)
}

#[derive(Clone, Debug)]
struct GraphTransport {
    nodes: BTreeMap<u32, u32>,
    edges: BTreeMap<u32, u32>,
}

fn graph_transport(
    source: &GraphLayout,
    target: &GraphLayout,
    table_transport: &panproto_reference::enumerate::kan_extensions::ProfileTransport,
) -> GraphTransport {
    let mut nodes = BTreeMap::new();
    let mut edges = BTreeMap::new();
    let insert = |map: &mut BTreeMap<u32, u32>, source, target| {
        assert_eq!(map.insert(source, target), None);
    };
    for (source_row, target_row) in &table_transport.rows {
        let source_node = &source.table_nodes[source_row];
        let target_node = &target.table_nodes[target_row];
        insert(&mut nodes, source_node.id, target_node.id);
    }
    for ((source_row, column), source_cell) in &source.cells {
        let target_row = table_transport.rows[source_row];
        let target_cell = &target.cells[&(target_row, column.clone())];
        assert_eq!(source_cell.nodes.len(), 1);
        assert_eq!(target_cell.nodes.len(), 1);
        assert!(source_cell.edges.is_empty());
        assert!(target_cell.edges.is_empty());
        insert(&mut nodes, source_cell.root, target_cell.root);
        insert(
            &mut edges,
            source_cell.attribute.id.unwrap(),
            target_cell.attribute.id.unwrap(),
        );
    }
    for ((relation, source_row, target_row), source_edge) in &source.relations {
        let target_key = (
            relation.clone(),
            table_transport.rows[source_row],
            table_transport.rows[target_row],
        );
        let target_edge = &target.relations[&target_key];
        insert(&mut edges, source_edge.id.unwrap(), target_edge.id.unwrap());
    }
    assert_eq!(
        nodes.values().copied().collect::<BTreeSet<_>>().len(),
        nodes.len(),
    );
    assert_eq!(
        edges.values().copied().collect::<BTreeSet<_>>().len(),
        edges.len(),
    );
    GraphTransport { nodes, edges }
}

fn transported_value(
    value: &Value,
    transport: &panproto_reference::enumerate::kan_extensions::ProfileTransport,
) -> Value {
    match value {
        Value::LabeledNull(id) => Value::LabeledNull(transport.nulls[id]),
        _ => value.clone(),
    }
}

fn assert_graph_transport(
    source: &Graph,
    target: &Graph,
    table_transport: &panproto_reference::enumerate::kan_extensions::ProfileTransport,
    graph_transport: &GraphTransport,
) {
    assert_eq!(
        graph_transport
            .nodes
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        source.nodes.keys().copied().collect(),
    );
    assert_eq!(
        graph_transport
            .nodes
            .values()
            .copied()
            .collect::<BTreeSet<_>>(),
        target.nodes.keys().copied().collect(),
    );
    assert_eq!(
        graph_transport
            .edges
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        source.edges.keys().copied().collect(),
    );
    assert_eq!(
        graph_transport
            .edges
            .values()
            .copied()
            .collect::<BTreeSet<_>>(),
        target.edges.keys().copied().collect(),
    );
    for (source_id, target_id) in &graph_transport.nodes {
        let mut expected = source.nodes[source_id].clone();
        expected.id = *target_id;
        if let Some(Presence::Present(value)) = &mut expected.value {
            *value = transported_value(value, table_transport);
        }
        expected.fields = expected
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), transported_value(value, table_transport)))
            .collect();
        expected.annotations = expected
            .annotations
            .iter()
            .map(|(key, value)| (key.clone(), transported_value(value, table_transport)))
            .collect();
        assert_eq!(expected, target.nodes[target_id]);
    }
    for (source_id, target_id) in &graph_transport.edges {
        let mut expected = source.edges[source_id].clone();
        expected.src = graph_transport.nodes[&expected.src];
        expected.tgt = graph_transport.nodes[&expected.tgt];
        assert_eq!(expected, target.edges[target_id]);
    }
}

fn assert_contributor_transport(
    source: &BTreeMap<u32, BTreeSet<u32>>,
    target: &BTreeMap<u32, BTreeSet<u32>>,
    output: &BTreeMap<u32, u32>,
    input: &BTreeMap<u32, u32>,
) {
    assert_eq!(source.len(), target.len());
    for (source_id, contributors) in source {
        let target_id = output[source_id];
        let expected = contributors
            .iter()
            .map(|contributor| input[contributor])
            .collect::<BTreeSet<_>>();
        assert_eq!(target[&target_id], expected);
    }
}

fn assert_tree_transport(
    source: &Tree,
    target: &Tree,
    table_transport: &panproto_reference::enumerate::kan_extensions::ProfileTransport,
    nodes: &BTreeMap<u32, u32>,
) {
    assert!(source.fans.is_empty());
    assert!(target.fans.is_empty());
    assert_eq!(
        nodes.keys().copied().collect::<BTreeSet<_>>(),
        source.nodes.keys().copied().collect(),
    );
    assert_eq!(
        nodes.values().copied().collect::<BTreeSet<_>>(),
        target.nodes.keys().copied().collect(),
    );
    assert_eq!(nodes[&source.root], target.root);
    for (source_id, source_node) in &source.nodes {
        let target_id = nodes[source_id];
        let mut expected = source_node.clone();
        expected.id = target_id;
        if let Some(Presence::Present(value)) = &mut expected.value {
            *value = transported_value(value, table_transport);
        }
        expected.fields = expected
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), transported_value(value, table_transport)))
            .collect();
        expected.annotations = expected
            .annotations
            .iter()
            .map(|(key, value)| (key.clone(), transported_value(value, table_transport)))
            .collect();
        assert_eq!(expected, target.nodes[&target_id]);
    }
    let expected_arcs = source
        .arcs
        .iter()
        .map(|(from, to, edge)| (nodes[from], nodes[to], edge.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(expected_arcs, target.arcs.iter().cloned().collect());
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TreeDomainFeature {
    Tree,
    RootCount(usize),
    SecondParent(BTreeSet<u32>),
    Unreached(BTreeSet<u32>),
}

fn tree_domain_feature(graph: &Graph) -> TreeDomainFeature {
    assert!(
        graph
            .nodes
            .values()
            .all(|node| node.shape != panproto_reference::model::NodeShape::HyperEdge)
    );
    for edge in graph.edges.values() {
        assert!(graph.nodes.contains_key(&edge.src));
        assert!(graph.nodes.contains_key(&edge.tgt));
        assert_eq!(edge.position, None);
        assert!(!matches!(
            edge.label.kind.as_str(),
            "hyper_parent" | "hyper_label"
        ));
    }
    let children = graph
        .edges
        .values()
        .map(|edge| edge.tgt)
        .collect::<BTreeSet<_>>();
    let roots = graph
        .nodes
        .keys()
        .filter(|node| !children.contains(node))
        .copied()
        .collect::<Vec<_>>();
    let [root] = roots.as_slice() else {
        return TreeDomainFeature::RootCount(roots.len());
    };
    let mut parents = graph
        .nodes
        .keys()
        .map(|node| (*node, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for edge in graph.edges.values() {
        parents.get_mut(&edge.tgt).unwrap().insert(edge.src);
    }
    let second_parent = parents
        .iter()
        .filter_map(|(node, values)| (values.len() >= 2).then_some(*node))
        .collect::<BTreeSet<_>>();
    if !second_parent.is_empty() {
        return TreeDomainFeature::SecondParent(second_parent);
    }
    let unreached = graph
        .nodes
        .keys()
        .filter(|node| {
            let mut current = **node;
            for _ in 0..graph.nodes.len() {
                if current == *root {
                    return false;
                }
                let Some(parent) = parents[&current].first() else {
                    return true;
                };
                current = *parent;
            }
            current != *root
        })
        .copied()
        .collect::<BTreeSet<_>>();
    if unreached.is_empty() {
        TreeDomainFeature::Tree
    } else {
        TreeDomainFeature::Unreached(unreached)
    }
}

fn assert_input_tree_transport(
    source: &Result<Tree, encode::TreeKanError>,
    target: &Result<Tree, encode::TreeKanError>,
    source_graph: &Graph,
    target_graph: &Graph,
    table: &panproto_reference::enumerate::kan_extensions::ProfileTransport,
    graph: &GraphTransport,
) {
    let source_feature = tree_domain_feature(source_graph);
    let target_feature = tree_domain_feature(target_graph);
    match (&source_feature, &target_feature, source, target) {
        (TreeDomainFeature::Tree, TreeDomainFeature::Tree, Ok(source), Ok(target)) => {
            assert_tree_transport(source, target, table, &graph.nodes);
        }
        (
            TreeDomainFeature::RootCount(source_count),
            TreeDomainFeature::RootCount(target_count),
            Err(encode::TreeKanError::RootCount {
                count: source_actual,
            }),
            Err(encode::TreeKanError::RootCount {
                count: target_actual,
            }),
        ) => {
            assert_eq!(source_count, source_actual);
            assert_eq!(target_count, target_actual);
            assert_eq!(source_count, target_count);
        }
        (
            TreeDomainFeature::SecondParent(source_nodes),
            TreeDomainFeature::SecondParent(target_nodes),
            Err(encode::TreeKanError::NotATree(encode::NotATree::SecondParent {
                node: source_node,
            })),
            Err(encode::TreeKanError::NotATree(encode::NotATree::SecondParent {
                node: target_node,
            })),
        ) => {
            assert!(source_nodes.contains(source_node));
            assert!(target_nodes.contains(target_node));
            assert_eq!(
                source_nodes
                    .iter()
                    .map(|node| graph.nodes[node])
                    .collect::<BTreeSet<_>>(),
                *target_nodes,
            );
        }
        (
            TreeDomainFeature::Unreached(source_nodes),
            TreeDomainFeature::Unreached(target_nodes),
            Err(encode::TreeKanError::NotATree(encode::NotATree::Unreached { node: source_node })),
            Err(encode::TreeKanError::NotATree(encode::NotATree::Unreached { node: target_node })),
        ) => {
            assert!(source_nodes.contains(source_node));
            assert!(target_nodes.contains(target_node));
            assert_eq!(
                source_nodes
                    .iter()
                    .map(|node| graph.nodes[node])
                    .collect::<BTreeSet<_>>(),
                *target_nodes,
            );
        }
        _ => panic!(
            "input tree-domain features are not transported: {source_feature:?}/{source:?} -> {target_feature:?}/{target:?}"
        ),
    }
}

fn assert_tree_route_transport(
    route: TreeRoute,
    source: &Result<Tree, encode::TreeKanError>,
    target: &Result<Tree, encode::TreeKanError>,
    graph_transport: Option<&GraphTransport>,
    table_transport: Option<&panproto_reference::enumerate::kan_extensions::ProfileTransport>,
) -> bool {
    assert_eq!(tree_outcome(source), tree_outcome(target));
    match (source, target) {
        (Ok(source), Ok(target)) => {
            assert_tree_transport(
                source,
                target,
                table_transport.unwrap(),
                &graph_transport.unwrap().nodes,
            );
            true
        }
        (
            Err(encode::TreeKanError::Graph(GraphKanError::Adjunction(source))),
            Err(encode::TreeKanError::Graph(GraphKanError::Adjunction(target))),
        ) if route == TreeRoute::Sigma
            && matches!(
                source.as_ref(),
                rel::AdjunctionError::SigmaConstantClash { .. }
            )
            && matches!(
                target.as_ref(),
                rel::AdjunctionError::SigmaConstantClash { .. }
            ) =>
        {
            false
        }
        (Err(source), Err(target)) => {
            assert_eq!(source, target);
            false
        }
        _ => unreachable!("equal closed outcomes cannot mix success and refusal"),
    }
}

fn first_collision<K: Clone + Ord, V: Clone + Ord>(
    pairs: impl IntoIterator<Item = (K, V)>,
) -> Option<(K, K, V)> {
    let mut pairs = pairs.into_iter().collect::<Vec<_>>();
    pairs.sort();
    let mut seen: BTreeMap<V, K> = BTreeMap::new();
    for (source, target) in pairs {
        if let Some(first) = seen.get(&target)
            && first != &source
        {
            return Some((first.clone(), source, target));
        }
        seen.insert(target, source);
    }
    None
}

fn expected_pi_tree_precheck(f: &RelFunctor) -> Option<encode::TreeKanError> {
    let mut vertices = f
        .tables
        .iter()
        .map(|(source, target)| (source.clone(), target.clone()))
        .collect::<Vec<_>>();
    let kept_values = literal_kept(f).value_vertices;
    vertices.extend(kept_values.into_iter().map(|source| {
        let target = f
            .value_vertices
            .get(&source)
            .cloned()
            .unwrap_or_else(|| source.clone());
        (source, target)
    }));
    vertices.sort();
    vertices.dedup();
    if let Some((first, second, target)) = first_collision(vertices) {
        return Some(encode::TreeKanError::NonInjectiveVertexMap {
            first,
            second,
            target,
        });
    }
    None
}

fn expected_pi_tree(f: &RelFunctor, graph: &Graph) -> Result<Tree, encode::TreeKanError> {
    if let Some(error) = expected_pi_tree_precheck(f) {
        return Err(error);
    }
    graph_tree_form(graph)
}

fn literal_adjunction_error(error: &KanError) -> rel::AdjunctionError {
    match error {
        KanError::SigmaConstantClash {
            row,
            column,
            left,
            right,
        } => rel::AdjunctionError::SigmaConstantClash {
            row: *row,
            column: column.clone(),
            left: left.clone(),
            right: right.clone(),
        },
        KanError::ProductSizeExceeded {
            vertex,
            actual,
            limit,
        } => rel::AdjunctionError::ProductSizeExceeded {
            vertex: vertex.clone(),
            actual: *actual,
            limit: *limit,
        },
        KanError::AttributeOutsideImage { table, column } => {
            rel::AdjunctionError::AttributeOutsideImage {
                table: table.clone(),
                column: column.clone(),
            }
        }
        other => panic!("literal route has no expected conversion for {other:?}"),
    }
}

fn expected_delta_tree(f: &RelFunctor, graph: &Graph) -> Result<Tree, encode::TreeKanError> {
    let arrow = |schema: &RelSchema, table: &str, column: &str| {
        schema
            .columns
            .get(table)
            .and_then(|columns| columns.get(column))
            .cloned()
    };
    let mut vertices = f
        .tables
        .iter()
        .map(|(source, target)| (source.clone(), target.clone()))
        .collect::<Vec<_>>();
    let mut edges = f
        .relations
        .iter()
        .map(|(source, target)| (source.clone(), target.clone()))
        .collect::<Vec<_>>();
    for ((source_table, source_column), target_column) in &f.columns {
        let target_table = &f.tables[source_table];
        let source_arrow = arrow(&f.src, source_table, source_column).unwrap();
        let target_arrow = arrow(&f.tgt, target_table, target_column).unwrap();
        vertices.push((source_arrow.tgt.clone(), target_arrow.tgt.clone()));
        edges.push((source_arrow, target_arrow));
    }
    vertices.sort();
    vertices.dedup();
    edges.sort();
    edges.dedup();
    if let Some((first, second, target)) = first_collision(vertices) {
        return Err(encode::TreeKanError::NonInjectiveVertexMap {
            first,
            second,
            target,
        });
    }
    if let Some((_, _, target)) = first_collision(edges) {
        return Err(encode::TreeKanError::NonInjectiveEdgeMap {
            target: Box::new(target),
        });
    }
    graph_tree_form(graph)
}

fn graph_tree_form(graph: &Graph) -> Result<Tree, encode::TreeKanError> {
    let children: BTreeSet<u32> = graph.edges.values().map(|edge| edge.tgt).collect();
    let roots: Vec<u32> = graph
        .nodes
        .keys()
        .filter(|node| !children.contains(node))
        .copied()
        .collect();
    match roots.as_slice() {
        [root] => {
            let mut parent = BTreeMap::new();
            for (edge_id, edge) in &graph.edges {
                if !graph.nodes.contains_key(&edge.src) || !graph.nodes.contains_key(&edge.tgt) {
                    return Err(encode::TreeKanError::NotATree(
                        encode::NotATree::DanglingEdge { edge: *edge_id },
                    ));
                }
                if edge.position.is_some() {
                    return Err(encode::TreeKanError::NotATree(
                        encode::NotATree::PositionedEdge { edge: *edge_id },
                    ));
                }
                if matches!(edge.label.kind.as_str(), "hyper_parent" | "hyper_label") {
                    return Err(encode::TreeKanError::NotATree(
                        encode::NotATree::ReservedArc { edge: *edge_id },
                    ));
                }
                if edge.tgt == *root {
                    return Err(encode::TreeKanError::NotATree(
                        encode::NotATree::RootHasParent { edge: *edge_id },
                    ));
                }
                if parent.insert(edge.tgt, edge.src).is_some() {
                    return Err(encode::TreeKanError::NotATree(
                        encode::NotATree::SecondParent { node: edge.tgt },
                    ));
                }
            }
            for node in graph.nodes.keys() {
                let mut current = *node;
                let mut steps = 0usize;
                while current != *root {
                    match parent.get(&current) {
                        Some(next) if steps < graph.nodes.len() => {
                            current = *next;
                            steps += 1;
                        }
                        _ => {
                            return Err(encode::TreeKanError::NotATree(
                                encode::NotATree::Unreached { node: *node },
                            ));
                        }
                    }
                }
            }
            Ok(Tree {
                nodes: graph.nodes.clone(),
                arcs: graph
                    .edges
                    .values()
                    .map(|edge| (edge.src, edge.tgt, edge.label.clone()))
                    .collect(),
                fans: Vec::new(),
                root: *root,
            })
        }
        many => Err(encode::TreeKanError::RootCount { count: many.len() }),
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NotATreeKind {
    MissingRoot,
    DanglingEdge,
    PositionedEdge,
    ReservedArc,
    ArcIntoFan,
    RootHasParent,
    SecondParent,
    FanLabel,
    FanData,
    Fan,
    Unreached,
}

const ALL_NOT_A_TREE_KINDS: [NotATreeKind; 11] = [
    NotATreeKind::MissingRoot,
    NotATreeKind::DanglingEdge,
    NotATreeKind::PositionedEdge,
    NotATreeKind::ReservedArc,
    NotATreeKind::ArcIntoFan,
    NotATreeKind::RootHasParent,
    NotATreeKind::SecondParent,
    NotATreeKind::FanLabel,
    NotATreeKind::FanData,
    NotATreeKind::Fan,
    NotATreeKind::Unreached,
];

const fn not_a_tree_kind(error: &encode::NotATree) -> NotATreeKind {
    match error {
        encode::NotATree::MissingRoot => NotATreeKind::MissingRoot,
        encode::NotATree::DanglingEdge { .. } => NotATreeKind::DanglingEdge,
        encode::NotATree::PositionedEdge { .. } => NotATreeKind::PositionedEdge,
        encode::NotATree::ReservedArc { .. } => NotATreeKind::ReservedArc,
        encode::NotATree::ArcIntoFan { .. } => NotATreeKind::ArcIntoFan,
        encode::NotATree::RootHasParent { .. } => NotATreeKind::RootHasParent,
        encode::NotATree::SecondParent { .. } => NotATreeKind::SecondParent,
        encode::NotATree::FanLabel { .. } => NotATreeKind::FanLabel,
        encode::NotATree::FanData { .. } => NotATreeKind::FanData,
        encode::NotATree::Fan { .. } => NotATreeKind::Fan,
        encode::NotATree::Unreached { .. } => NotATreeKind::Unreached,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TreeDomain {
    Tree,
    RootCount(usize),
    NotATree(NotATreeKind),
}

fn tree_domain(result: &Result<Tree, encode::TreeKanError>) -> TreeDomain {
    match result {
        Ok(_) => TreeDomain::Tree,
        Err(encode::TreeKanError::RootCount { count }) => TreeDomain::RootCount(*count),
        Err(encode::TreeKanError::NotATree(error)) => TreeDomain::NotATree(not_a_tree_kind(error)),
        Err(other) => {
            panic!("direct literal graph conversion reached an impossible stage: {other:?}")
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TreeEncodeKind {
    ReservedVocabulary,
    SecondParent,
    DanglingFan,
    DanglingArc,
    NodeIdsExhausted,
    EdgeIdsExhausted,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum AdjunctionKind {
    NonConforming,
    SigmaConstantClash,
    RowIdsExhausted,
    NullIdsExhausted,
    AttributeOutsideImage,
    ProductSizeExceeded,
    MalformedTable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum GraphOutcome {
    NotEncodable,
    Adjunction(AdjunctionKind),
    RelSchema,
    Functor,
    Hom,
    NodeIdsExhausted,
    EdgeIdsExhausted,
    EdgePositionsExhausted,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TreeOutcome {
    Ok,
    Encode(TreeEncodeKind),
    NonInjectiveVertexMap,
    NonInjectiveEdgeMap,
    Graph(GraphOutcome),
    RootCount(usize),
    NotATree(NotATreeKind),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TreeRoute {
    Sigma,
    Pi4,
    Pi81,
    Delta,
}

fn tree_outcome(result: &Result<Tree, encode::TreeKanError>) -> TreeOutcome {
    match result {
        Ok(_) => TreeOutcome::Ok,
        Err(encode::TreeKanError::Encode(error)) => TreeOutcome::Encode(match error {
            encode::TreeEncodeError::ReservedVocabulary => TreeEncodeKind::ReservedVocabulary,
            encode::TreeEncodeError::SecondParent { .. } => TreeEncodeKind::SecondParent,
            encode::TreeEncodeError::DanglingFan => TreeEncodeKind::DanglingFan,
            encode::TreeEncodeError::DanglingArc => TreeEncodeKind::DanglingArc,
            encode::TreeEncodeError::NodeIdsExhausted => TreeEncodeKind::NodeIdsExhausted,
            encode::TreeEncodeError::EdgeIdsExhausted => TreeEncodeKind::EdgeIdsExhausted,
        }),
        Err(encode::TreeKanError::NonInjectiveVertexMap { .. }) => {
            TreeOutcome::NonInjectiveVertexMap
        }
        Err(encode::TreeKanError::NonInjectiveEdgeMap { .. }) => TreeOutcome::NonInjectiveEdgeMap,
        Err(encode::TreeKanError::Graph(error)) => TreeOutcome::Graph(match error {
            GraphKanError::NotEncodable(_) => GraphOutcome::NotEncodable,
            GraphKanError::Adjunction(error) => GraphOutcome::Adjunction(match error.as_ref() {
                rel::AdjunctionError::NonConforming(_) => AdjunctionKind::NonConforming,
                rel::AdjunctionError::SigmaConstantClash { .. } => {
                    AdjunctionKind::SigmaConstantClash
                }
                rel::AdjunctionError::RowIdsExhausted => AdjunctionKind::RowIdsExhausted,
                rel::AdjunctionError::NullIdsExhausted => AdjunctionKind::NullIdsExhausted,
                rel::AdjunctionError::AttributeOutsideImage { .. } => {
                    AdjunctionKind::AttributeOutsideImage
                }
                rel::AdjunctionError::ProductSizeExceeded { .. } => {
                    AdjunctionKind::ProductSizeExceeded
                }
                rel::AdjunctionError::MalformedTable => AdjunctionKind::MalformedTable,
            }),
            GraphKanError::RelSchema(_) => GraphOutcome::RelSchema,
            GraphKanError::Functor(_) => GraphOutcome::Functor,
            GraphKanError::Hom(_) => GraphOutcome::Hom,
            GraphKanError::NodeIdsExhausted => GraphOutcome::NodeIdsExhausted,
            GraphKanError::EdgeIdsExhausted => GraphOutcome::EdgeIdsExhausted,
            GraphKanError::EdgePositionsExhausted { .. } => GraphOutcome::EdgePositionsExhausted,
        }),
        Err(encode::TreeKanError::RootCount { count }) => TreeOutcome::RootCount(*count),
        Err(encode::TreeKanError::NotATree(error)) => TreeOutcome::NotATree(not_a_tree_kind(error)),
    }
}

fn record_tree_outcome(
    route: TreeRoute,
    result: &Result<Tree, encode::TreeKanError>,
    counts: &mut BTreeMap<(TreeRoute, TreeOutcome), usize>,
) {
    *counts.entry((route, tree_outcome(result))).or_default() += 1;
}

fn all_tree_outcomes() -> Vec<TreeOutcome> {
    let mut outcomes = vec![
        TreeOutcome::Ok,
        TreeOutcome::Encode(TreeEncodeKind::ReservedVocabulary),
        TreeOutcome::Encode(TreeEncodeKind::SecondParent),
        TreeOutcome::Encode(TreeEncodeKind::DanglingFan),
        TreeOutcome::Encode(TreeEncodeKind::DanglingArc),
        TreeOutcome::Encode(TreeEncodeKind::NodeIdsExhausted),
        TreeOutcome::Encode(TreeEncodeKind::EdgeIdsExhausted),
        TreeOutcome::NonInjectiveVertexMap,
        TreeOutcome::NonInjectiveEdgeMap,
        TreeOutcome::Graph(GraphOutcome::NotEncodable),
        TreeOutcome::Graph(GraphOutcome::RelSchema),
        TreeOutcome::Graph(GraphOutcome::Functor),
        TreeOutcome::Graph(GraphOutcome::Hom),
        TreeOutcome::Graph(GraphOutcome::NodeIdsExhausted),
        TreeOutcome::Graph(GraphOutcome::EdgeIdsExhausted),
        TreeOutcome::Graph(GraphOutcome::EdgePositionsExhausted),
    ];
    // The bounded family contains at most six rows, but output constructions
    // can create additional roots. The census below pins every positive
    // payload; these candidates ensure any new payload is rejected rather
    // than hidden by an aggregate bucket.
    outcomes.extend((0..=81).map(TreeOutcome::RootCount));
    outcomes.extend(
        [
            AdjunctionKind::NonConforming,
            AdjunctionKind::SigmaConstantClash,
            AdjunctionKind::RowIdsExhausted,
            AdjunctionKind::NullIdsExhausted,
            AdjunctionKind::AttributeOutsideImage,
            AdjunctionKind::ProductSizeExceeded,
            AdjunctionKind::MalformedTable,
        ]
        .into_iter()
        .map(|kind| TreeOutcome::Graph(GraphOutcome::Adjunction(kind))),
    );
    outcomes.extend(ALL_NOT_A_TREE_KINDS.into_iter().map(TreeOutcome::NotATree));
    outcomes
}

fn domain_counts(entries: &[(TreeDomain, usize)]) -> BTreeMap<TreeDomain, usize> {
    entries.iter().copied().collect()
}

fn expected_source_tree_domains() -> Vec<BTreeMap<TreeDomain, usize>> {
    let small_two = domain_counts(&[
        (TreeDomain::Tree, 8),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 48),
        (TreeDomain::RootCount(3), 256),
        (TreeDomain::RootCount(4), 768),
        (TreeDomain::RootCount(5), 2_048),
        (TreeDomain::RootCount(6), 4_096),
    ]);
    let one_column = domain_counts(&[
        (TreeDomain::Tree, 4),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 16),
        (TreeDomain::RootCount(3), 64),
    ]);
    let self_relation = domain_counts(&[
        (TreeDomain::Tree, 17),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 665),
        (TreeDomain::RootCount(3), 26_161),
        (TreeDomain::RootCount(4), 10_532),
        (TreeDomain::RootCount(5), 1_424),
        (TreeDomain::RootCount(6), 64),
    ]);
    let related = domain_counts(&[
        (TreeDomain::Tree, 612),
        (TreeDomain::RootCount(0), 22_101),
        (TreeDomain::RootCount(2), 1_360),
        (TreeDomain::RootCount(3), 64),
        (TreeDomain::NotATree(NotATreeKind::SecondParent), 7_712),
        (TreeDomain::NotATree(NotATreeKind::Unreached), 1_184),
    ]);
    let floats = domain_counts(&[
        (TreeDomain::Tree, 16),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 256),
        (TreeDomain::RootCount(3), 4_096),
    ]);
    vec![
        small_two.clone(),
        small_two,
        one_column.clone(),
        one_column.clone(),
        one_column,
        self_relation.clone(),
        related,
        floats.clone(),
        floats,
        self_relation,
    ]
}

fn expected_target_tree_domains() -> Vec<BTreeMap<TreeDomain, usize>> {
    let floats = domain_counts(&[
        (TreeDomain::Tree, 16),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 256),
        (TreeDomain::RootCount(3), 4_096),
    ]);
    let one_column = domain_counts(&[
        (TreeDomain::Tree, 4),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 16),
        (TreeDomain::RootCount(3), 64),
    ]);
    let outside_image = domain_counts(&[
        (TreeDomain::Tree, 5),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 21),
        (TreeDomain::RootCount(3), 85),
        (TreeDomain::RootCount(4), 84),
        (TreeDomain::RootCount(5), 80),
        (TreeDomain::RootCount(6), 64),
    ]);
    let related = domain_counts(&[
        (TreeDomain::Tree, 612),
        (TreeDomain::RootCount(0), 22_101),
        (TreeDomain::RootCount(2), 1_360),
        (TreeDomain::RootCount(3), 64),
        (TreeDomain::NotATree(NotATreeKind::SecondParent), 7_712),
        (TreeDomain::NotATree(NotATreeKind::Unreached), 1_184),
    ]);
    let self_relation = domain_counts(&[
        (TreeDomain::Tree, 17),
        (TreeDomain::RootCount(0), 1),
        (TreeDomain::RootCount(2), 665),
        (TreeDomain::RootCount(3), 26_161),
        (TreeDomain::RootCount(4), 10_532),
        (TreeDomain::RootCount(5), 1_424),
        (TreeDomain::RootCount(6), 64),
    ]);
    vec![
        floats.clone(),
        one_column.clone(),
        one_column.clone(),
        floats,
        outside_image,
        related.clone(),
        related,
        one_column.clone(),
        one_column,
        self_relation,
    ]
}

fn expected_tree_route_outcomes() -> BTreeMap<(TreeRoute, TreeOutcome), usize> {
    BTreeMap::from([
        ((TreeRoute::Sigma, TreeOutcome::Ok), 692),
        (
            (
                TreeRoute::Sigma,
                TreeOutcome::Graph(GraphOutcome::Adjunction(AdjunctionKind::SigmaConstantClash)),
            ),
            14,
        ),
        ((TreeRoute::Pi4, TreeOutcome::Ok), 633),
        ((TreeRoute::Pi4, TreeOutcome::NonInjectiveVertexMap), 65),
        (
            (
                TreeRoute::Pi4,
                TreeOutcome::Graph(GraphOutcome::Adjunction(
                    AdjunctionKind::AttributeOutsideImage,
                )),
            ),
            4,
        ),
        ((TreeRoute::Pi4, TreeOutcome::RootCount(2)), 4),
        ((TreeRoute::Pi81, TreeOutcome::Ok), 633),
        ((TreeRoute::Pi81, TreeOutcome::NonInjectiveVertexMap), 65),
        (
            (
                TreeRoute::Pi81,
                TreeOutcome::Graph(GraphOutcome::Adjunction(
                    AdjunctionKind::AttributeOutsideImage,
                )),
            ),
            4,
        ),
        ((TreeRoute::Pi81, TreeOutcome::RootCount(2)), 4),
        ((TreeRoute::Delta, TreeOutcome::Ok), 653),
        ((TreeRoute::Delta, TreeOutcome::NonInjectiveVertexMap), 640),
        ((TreeRoute::Delta, TreeOutcome::RootCount(0)), 1),
    ])
}

/// a(x), b(y) and an FK a -f-> b.
fn ab() -> Schema {
    Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.x", "int")
        .vertex("b.y", "int")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("b", "b.y", "prop", Some("y"))
        .edge("a", "b", "fk", Some("f"))
}

fn identity(s: &Schema) -> Migration {
    Migration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.clone(),
        ..Migration::default()
    }
}

fn sample() -> Table {
    let mut t = Table::default();
    t.rows.insert(0, row("a", &[("x", Value::Int(1))]));
    t.rows.insert(1, row("a", &[("x", Value::LabeledNull(0))]));
    t.rows.insert(2, row("b", &[("y", Value::Int(2))]));
    t.pairs
        .insert((SEdge::new("a", "b", "fk", Some("f")), 0, 2));
    t
}

#[test]
fn every_extension_along_the_identity_returns_the_input() {
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let i = sample();
    assert_eq!(rel::delta(&f, &i).unwrap().0, i);
    assert_eq!(rel::sigma(&f, &i).unwrap().0, i);
    let (p, _) = rel::pi(&f, &i, usize::MAX).unwrap();
    assert!(rel::row_bijection(&p, &i).is_some());
}

#[test]
fn a_row_bijection_refuses_dangling_pairs_without_panicking() {
    let malformed = Table {
        rows: BTreeMap::from([(0, row("a", &[]))]),
        pairs: BTreeSet::from([(SEdge::new("a", "a", "fk", Some("f")), 0, 1)]),
    };
    assert_eq!(rel::row_bijection(&malformed, &malformed), None);
    assert_eq!(rel::row_bijection(&Table::default(), &malformed), None);
    assert_eq!(rel::row_bijection(&malformed, &Table::default()), None);
}

#[test]
fn homs_refuses_a_dangling_pair_in_the_domain_without_panicking() {
    let edge = SEdge::new("a", "a", "fk", Some("f"));
    let well_formed = Table {
        rows: BTreeMap::from([(0, row("a", &[]))]),
        pairs: BTreeSet::new(),
    };
    let malformed = Table {
        rows: well_formed.rows.clone(),
        pairs: BTreeSet::from([(edge, 0, 1)]),
    };

    let result = std::panic::catch_unwind(|| rel::homs(&malformed, &well_formed));
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn homs_refuses_a_dangling_pair_in_the_codomain() {
    let edge = SEdge::new("a", "a", "fk", Some("f"));
    let well_formed = Table {
        rows: BTreeMap::from([(0, row("a", &[]))]),
        pairs: BTreeSet::new(),
    };
    let malformed = Table {
        rows: well_formed.rows.clone(),
        pairs: BTreeSet::from([(edge, 0, 1)]),
    };

    assert!(rel::homs(&well_formed, &malformed).is_empty());
}

#[test]
fn homs_retain_and_normalize_labelled_null_maps() {
    let domain = Table {
        rows: BTreeMap::from([(0, row("a", &[("x", Value::LabeledNull(4))]))]),
        pairs: BTreeSet::new(),
    };
    let codomain = Table {
        rows: BTreeMap::from([
            (7, row("a", &[("x", Value::Int(9))])),
            (8, row("a", &[("x", Value::LabeledNull(6))])),
        ]),
        pairs: BTreeSet::new(),
    };
    let maps = rel::homs(&domain, &codomain);
    assert_eq!(maps.len(), 2);
    assert!(maps.iter().any(|map| {
        map.rows == BTreeMap::from([(0, 7)]) && map.nulls == BTreeMap::from([(4, Value::Int(9))])
    }));
    assert!(maps.iter().any(|map| {
        map.rows == BTreeMap::from([(0, 8)])
            && map.nulls == BTreeMap::from([(4, Value::LabeledNull(6))])
    }));

    let omitted = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::new(),
    };
    let explicit_identity = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::from([(4, Value::LabeledNull(4))]),
    };
    assert!(omitted.same_as(&explicit_identity, &domain));

    let first = Hom {
        rows: BTreeMap::from([(0, 7)]),
        nulls: BTreeMap::from([(4, Value::LabeledNull(6))]),
    };
    let second = Hom {
        rows: BTreeMap::from([(7, 11)]),
        nulls: BTreeMap::from([(6, Value::Int(12))]),
    };
    assert_eq!(
        first.then(&second),
        Hom {
            rows: BTreeMap::from([(0, 11)]),
            nulls: BTreeMap::from([(4, Value::Int(12)), (6, Value::Int(12))]),
        }
    );
}

#[test]
fn entity_functor_separates_entity_acceptance_from_value_structure_refusal() {
    let inner = SEdge::new("v", "w", "item", Some("child"));
    let changed = SEdge::new("v2", "w2", "item", Some("renamed"));
    let src = RelSchema {
        value_vertices: BTreeSet::from(["v".to_owned(), "w".to_owned()]),
        inner_edges: BTreeSet::from([inner.clone()]),
        kinds: BTreeMap::from([
            ("v".to_owned(), "value".to_owned()),
            ("w".to_owned(), "value".to_owned()),
        ]),
        ..RelSchema::default()
    };
    let tgt = RelSchema {
        value_vertices: BTreeSet::from(["v2".to_owned(), "w2".to_owned()]),
        inner_edges: BTreeSet::from([changed.clone()]),
        kinds: BTreeMap::from([
            ("v2".to_owned(), "value".to_owned()),
            ("w2".to_owned(), "value".to_owned()),
        ]),
        ..RelSchema::default()
    };
    let migration = Migration {
        vertex_remap: BTreeMap::from([
            ("v".to_owned(), "v2".to_owned()),
            ("w".to_owned(), "w2".to_owned()),
        ]),
        edge_remap: BTreeMap::from([(inner.clone(), changed)]),
        ..Migration::default()
    };
    let entity = rel::entity_functor(&src, &tgt, &migration).expect("step 4a accepts");
    assert_eq!(entity.value_vertices, migration.vertex_remap);
    assert_eq!(
        entity.into_kan(),
        Err(FunctorError::ValueStructureRemap {
            edge: Box::new(inner),
        })
    );
}

#[test]
fn entity_functor_refuses_cross_divide_remaps_and_rebuilds_surviving_relations() {
    let relation = SEdge::new("a", "b", "fk", Some("f"));
    let renamed_relation = SEdge::new("x", "y", "fk", Some("f"));
    let attribute = SEdge::new("x", "v", "prop", Some("c"));
    let src = RelSchema {
        tables: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        relations: BTreeSet::from([relation.clone()]),
        kinds: BTreeMap::from([
            ("a".to_owned(), "record".to_owned()),
            ("b".to_owned(), "record".to_owned()),
        ]),
        ..RelSchema::default()
    };
    let tgt = RelSchema {
        tables: BTreeSet::from(["x".to_owned(), "y".to_owned()]),
        relations: BTreeSet::from([renamed_relation.clone()]),
        columns: BTreeMap::from([(
            "x".to_owned(),
            BTreeMap::from([("c".to_owned(), attribute.clone())]),
        )]),
        value_vertices: BTreeSet::from(["v".to_owned()]),
        kinds: BTreeMap::from([
            ("x".to_owned(), "record".to_owned()),
            ("y".to_owned(), "record".to_owned()),
            ("v".to_owned(), "value".to_owned()),
        ]),
        ..RelSchema::default()
    };
    let renamed = Migration {
        surviving_edges: BTreeSet::from([relation.clone()]),
        vertex_remap: BTreeMap::from([
            ("a".to_owned(), "x".to_owned()),
            ("b".to_owned(), "y".to_owned()),
        ]),
        ..Migration::default()
    };
    let functor = rel::entity_functor(&src, &tgt, &renamed)
        .unwrap()
        .into_kan()
        .unwrap();
    assert_eq!(functor.relations[&relation], renamed_relation);

    let crossing = Migration {
        vertex_remap: renamed.vertex_remap,
        edge_remap: BTreeMap::from([(relation.clone(), attribute.clone())]),
        ..Migration::default()
    };
    assert_eq!(
        rel::entity_functor(&src, &tgt, &crossing),
        Err(FunctorError::IncoherentEdgeImage {
            edge: Box::new(relation),
            image: Box::new(attribute),
        })
    );
}

#[test]
fn a_merge_of_two_tables_makes_product_rows_and_null_cells() {
    // a(x), b(y) -> m(x, y): Pi takes the product, joining on no shared
    // column; Sigma keeps each row with a fresh null for the other column.
    let source = ab();
    let target = Schema::default()
        .vertex("m", "record")
        .vertex("m.x", "int")
        .vertex("m.y", "int")
        .edge("m", "m.x", "prop", Some("x"))
        .edge("m", "m.y", "prop", Some("y"));
    let mig = Migration {
        vertex_remap: [("a", "m"), ("b", "m"), ("a.x", "m.x"), ("b.y", "m.y")]
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
        edge_remap: [
            (
                SEdge::new("a", "a.x", "prop", Some("x")),
                SEdge::new("m", "m.x", "prop", Some("x")),
            ),
            (
                SEdge::new("b", "b.y", "prop", Some("y")),
                SEdge::new("m", "m.y", "prop", Some("y")),
            ),
        ]
        .into(),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &mig);
    let inst = sample();
    let (pi_inst, prov) = rel::pi(&functor, &inst, usize::MAX).unwrap();
    assert_eq!(pi_inst.rows.len(), 2);
    assert_eq!(
        prov.values().map(|(_, tu)| tu.clone()).collect::<Vec<_>>(),
        vec![vec![0, 2], vec![1, 2]]
    );
    let (sg, sp) = rel::sigma(&functor, &inst).unwrap();
    assert_eq!(sg.rows[&0].1["y"], Value::LabeledNull(1));
    assert_eq!(sg.rows[&2].1["x"], Value::LabeledNull(3));
    assert_eq!(sp.fresh.len(), 3);
}

#[test]
fn pi_checks_multi_table_fibres_before_enumerating_their_product() {
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record");
    let target = Schema::default().vertex("m", "record");
    let migration = Migration {
        vertex_remap: BTreeMap::from([
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]),
        ..Migration::default()
    };
    let f = functor(&source, &target, &migration);
    let input = Table {
        rows: BTreeMap::from([
            (0, row("a", &[])),
            (1, row("a", &[])),
            (2, row("b", &[])),
            (3, row("b", &[])),
        ]),
        pairs: BTreeSet::new(),
    };
    assert_eq!(
        rel::pi(&f, &input, 3),
        Err(KanError::ProductSizeExceeded {
            vertex: "m".to_owned(),
            actual: 4,
            limit: 3,
        })
    );
    assert_eq!(rel::pi(&f, &input, 4).unwrap().0.rows.len(), 4);
}

#[test]
fn pi_equalises_an_intra_fibre_relation_carried_to_a_loop() {
    // src_a, src_b both to m, two rows each, FK a -> b = {(a0, b1)} carried
    // to the loop m -> m: four product rows, and the loop holds exactly the
    // pairs (u, v) with (u at a, v at b) = (a0, b1).
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("f"));
    let target = Schema::default()
        .vertex("m", "record")
        .edge("m", "m", "fk", Some("f"));
    let fk = SEdge::new("a", "b", "fk", Some("f"));
    let lp = SEdge::new("m", "m", "fk", Some("f"));
    let mig = Migration {
        vertex_remap: [
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]
        .into(),
        edge_remap: [(fk.clone(), lp.clone())].into(),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &mig);
    let mut inst = Table::default();
    for (id, tb) in [(0, "a"), (1, "a"), (2, "b"), (3, "b")] {
        inst.rows.insert(id, row(tb, &[]));
    }
    inst.pairs.insert((fk, 0, 3));
    let (pi_inst, prov) = rel::pi(&functor, &inst, usize::MAX).unwrap();
    assert_eq!(pi_inst.rows.len(), 4);
    let tuple = |id: u32| prov[&id].1.clone();
    let loops: Vec<(Vec<u32>, Vec<u32>)> = pi_inst
        .pairs
        .iter()
        .map(|(e, u, v)| {
            assert_eq!(*e, lp);
            (tuple(*u), tuple(*v))
        })
        .collect();
    assert_eq!(loops.len(), 4);
    assert!(loops.iter().all(|(u, v)| u[0] == 0 && v[1] == 3));
}

#[test]
fn two_columns_merged_into_one_clash_unless_equal() {
    let source = Schema::default()
        .vertex("r", "record")
        .vertex("r.x", "int")
        .vertex("r.y", "int")
        .edge("r", "r.x", "prop", Some("x"))
        .edge("r", "r.y", "prop", Some("y"));
    let target = Schema::default()
        .vertex("r", "record")
        .vertex("r.z", "int")
        .edge("r", "r.z", "prop", Some("z"));
    let merged = SEdge::new("r", "r.z", "prop", Some("z"));
    let mig = Migration {
        surviving_verts: ["r".to_owned()].into(),
        vertex_remap: [
            ("r.x".to_owned(), "r.z".to_owned()),
            ("r.y".to_owned(), "r.z".to_owned()),
        ]
        .into(),
        edge_remap: [
            (SEdge::new("r", "r.x", "prop", Some("x")), merged.clone()),
            (SEdge::new("r", "r.y", "prop", Some("y")), merged),
        ]
        .into(),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &mig);
    let mut inst = Table::default();
    inst.rows
        .insert(0, row("r", &[("x", Value::Int(1)), ("y", Value::Int(1))]));
    assert_eq!(
        rel::sigma(&functor, &inst).unwrap().0.rows[&0].1["z"],
        Value::Int(1)
    );
    inst.rows
        .insert(0, row("r", &[("x", Value::Int(1)), ("y", Value::Int(2))]));
    assert!(matches!(
        rel::sigma(&functor, &inst),
        Err(KanError::SigmaConstantClash { row: 0, .. })
    ));
    // Float cells clash on bits: 0.0 against -0.0, NaN against -NaN.
    inst.rows.insert(
        0,
        row(
            "r",
            &[
                ("x", Value::Float(0.0f64.to_bits())),
                ("y", Value::Float((-0.0f64).to_bits())),
            ],
        ),
    );
    assert!(rel::sigma(&functor, &inst).is_err());
    inst.rows.insert(
        0,
        row(
            "r",
            &[
                ("x", Value::Float(0x7ff8_0000_0000_0000)),
                ("y", Value::Float(0x7ff8_0000_0000_0000)),
            ],
        ),
    );
    assert!(rel::sigma(&functor, &inst).is_ok());
}

#[test]
fn sigma_and_delta_hom_sets_have_equal_sizes_on_tiny_instances() {
    // Hom(Sigma I, J) and Hom(I, Delta J) are in bijection; on a rename the
    // two counts agree for every small J.
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let i = sample();
    let (si, _) = rel::sigma(&f, &i).unwrap();
    for j in [sample(), {
        let mut t = sample();
        t.rows.insert(5, row("b", &[("y", Value::Int(2))]));
        t
    }] {
        let (dj, _) = rel::delta(&f, &j).unwrap();
        assert_eq!(rel::homs(&si, &j).len(), rel::homs(&i, &dj).len());
    }
}

#[test]
fn table_adjunction_transposes_are_mutual_inverses_and_preserve_identities() {
    let schema = ab();
    let f = functor(&schema, &schema, &identity(&schema));
    let table = sample();
    let identity = Hom {
        rows: table.rows.keys().map(|row| (*row, *row)).collect(),
        nulls: BTreeMap::new(),
    };

    for induced in [
        rel::sigma_unit(&f, &table).unwrap(),
        rel::sigma_counit(&f, &table).unwrap(),
        rel::pi_unit(&f, &table, 81).unwrap(),
        rel::pi_counit(&f, &table, 81).unwrap(),
    ] {
        assert!(induced.same_as(&identity, &table));
    }

    let sigma_forward = rel::sigma_transpose_forward(&f, &table, &table, &identity).unwrap();
    let sigma_back = rel::sigma_transpose_inverse(&f, &table, &table, &sigma_forward).unwrap();
    assert!(sigma_forward.same_as(&identity, &table));
    assert!(sigma_back.same_as(&identity, &table));

    let pi_forward = rel::pi_transpose_forward(&f, &table, &table, &identity, 81).unwrap();
    let pi_back = rel::pi_transpose_inverse(&f, &table, &table, &pi_forward, 81).unwrap();
    assert!(pi_forward.same_as(&identity, &table));
    assert!(pi_back.same_as(&identity, &table));
}

#[test]
fn hom_composition_preserves_implicit_null_identities_and_is_associative() {
    let table = |value| Table {
        rows: BTreeMap::from([(0, row("t", &[("x", value)]))]),
        pairs: BTreeSet::new(),
    };
    let a = table(Value::LabeledNull(0));
    let b = table(Value::LabeledNull(1));
    let c = table(Value::Int(2));
    let d = table(Value::Int(2));
    let identity_a = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::new(),
    };
    let identity_c = identity_a.clone();
    let a_to_b = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::from([(0, Value::LabeledNull(1))]),
    };
    let b_to_c = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::from([(1, Value::Int(2))]),
    };
    assert!(identity_a.check(&a, &a).is_ok());
    assert!(a_to_b.check(&a, &b).is_ok());
    assert!(b_to_c.check(&b, &c).is_ok());
    assert!(identity_c.check(&c, &d).is_ok());

    let direct = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::from([(0, Value::Int(2))]),
    };
    assert!(identity_a.compose(&direct).unwrap().same_as(&direct, &a));
    assert!(direct.compose(&identity_c).unwrap().same_as(&direct, &a));

    let left = a_to_b
        .compose(&b_to_c)
        .unwrap()
        .compose(&identity_c)
        .unwrap();
    let right = a_to_b
        .compose(&b_to_c.compose(&identity_c).unwrap())
        .unwrap();
    assert!(left.same_as(&direct, &a));
    assert!(right.same_as(&direct, &a));
    assert!(left.same_as(&right, &a));
    assert!(left.check(&a, &d).is_ok());
}

struct NaturalityFixture {
    functor: RelFunctor,
    source_before: Table,
    source_middle: Table,
    source_after: Table,
    target_middle: Table,
    target_after: Table,
    source_pre_map: Hom,
    sigma_hom: Hom,
    target_post_map: Hom,
}

fn assert_sigma_naturality(fixture: &NaturalityFixture) {
    let sigma_pre = sigma_on_hom(
        &fixture.functor,
        &fixture.source_before,
        &fixture.source_middle,
        &fixture.source_pre_map,
    )
    .unwrap();
    let delta_post = delta_on_hom(
        &fixture.functor,
        &fixture.target_middle,
        &fixture.target_after,
        &fixture.target_post_map,
    )
    .unwrap();
    let sigma_transpose = rel::sigma_transpose_forward(
        &fixture.functor,
        &fixture.source_middle,
        &fixture.target_middle,
        &fixture.sigma_hom,
    )
    .unwrap();
    let direct = sigma_pre
        .compose(&fixture.sigma_hom)
        .unwrap()
        .compose(&fixture.target_post_map)
        .unwrap();
    let transposed = rel::sigma_transpose_forward(
        &fixture.functor,
        &fixture.source_before,
        &fixture.target_after,
        &direct,
    )
    .unwrap();
    let natural = fixture
        .source_pre_map
        .compose(&sigma_transpose)
        .unwrap()
        .compose(&delta_post)
        .unwrap();
    assert!(transposed.same_as(&natural, &fixture.source_before));
    let inverse = rel::sigma_transpose_inverse(
        &fixture.functor,
        &fixture.source_before,
        &fixture.target_after,
        &natural,
    )
    .unwrap();
    let sigma_source_before = rel::sigma(&fixture.functor, &fixture.source_before)
        .unwrap()
        .0;
    assert!(inverse.same_as(&direct, &sigma_source_before));
}

fn assert_pi_naturality(fixture: &NaturalityFixture) {
    let target_pre_map = Hom {
        rows: BTreeMap::from([(30, 20), (31, 21)]),
        nulls: BTreeMap::from([(3, Value::LabeledNull(2))]),
    };
    let pi_hom = Hom {
        rows: BTreeMap::from([(20, 0), (21, 1)]),
        nulls: BTreeMap::from([(2, Value::LabeledNull(1))]),
    };
    let source_post_map = Hom {
        rows: BTreeMap::from([(0, 40), (1, 41)]),
        nulls: BTreeMap::from([(1, Value::LabeledNull(4))]),
    };
    let delta_pre = delta_on_hom(
        &fixture.functor,
        &fixture.target_after,
        &fixture.target_middle,
        &target_pre_map,
    )
    .unwrap();
    let pi_post = pi_on_hom(
        &fixture.functor,
        &fixture.source_middle,
        &fixture.source_after,
        &source_post_map,
        81,
    )
    .unwrap();
    let pi_transpose = rel::pi_transpose_forward(
        &fixture.functor,
        &fixture.target_middle,
        &fixture.source_middle,
        &pi_hom,
        81,
    )
    .unwrap();
    let direct = delta_pre
        .compose(&pi_hom)
        .unwrap()
        .compose(&source_post_map)
        .unwrap();
    let transposed = rel::pi_transpose_forward(
        &fixture.functor,
        &fixture.target_after,
        &fixture.source_after,
        &direct,
        81,
    )
    .unwrap();
    let natural = target_pre_map
        .compose(&pi_transpose)
        .unwrap()
        .compose(&pi_post)
        .unwrap();
    assert!(transposed.same_as(&natural, &fixture.target_after));
    let inverse = rel::pi_transpose_inverse(
        &fixture.functor,
        &fixture.target_after,
        &fixture.source_after,
        &natural,
        81,
    )
    .unwrap();
    let delta_target_after = rel::delta(&fixture.functor, &fixture.target_after)
        .unwrap()
        .0;
    assert!(inverse.same_as(&direct, &delta_target_after));
}

#[test]
fn table_adjunction_transposes_are_natural_in_both_variables() {
    use panproto_reference::enumerate::kan_extensions as bounded;

    let case = &bounded::family()[2];
    let source = rel::classify(
        &case.source_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let target = rel::classify(
        &case.target_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let functor = rel::entity_functor(&source, &target, &case.migration())
        .unwrap()
        .into_kan()
        .unwrap();
    let source_table = |ids: [u32; 2], null: u64| Table {
        rows: BTreeMap::from([
            (ids[0], row("post", &[("title", Value::LabeledNull(null))])),
            (ids[1], row("post", &[("title", Value::Int(0))])),
        ]),
        pairs: BTreeSet::new(),
    };
    let target_table = |ids: [u32; 2], null: u64| Table {
        rows: BTreeMap::from([
            (ids[0], row("post", &[("name", Value::LabeledNull(null))])),
            (ids[1], row("post", &[("name", Value::Int(0))])),
        ]),
        pairs: BTreeSet::new(),
    };
    let arrow = |from: [u32; 2], to: [u32; 2], source_null: u64, target_null: u64| Hom {
        rows: BTreeMap::from([(from[0], to[0]), (from[1], to[1])]),
        nulls: BTreeMap::from([(source_null, Value::LabeledNull(target_null))]),
    };

    let fixture = NaturalityFixture {
        functor,
        source_before: source_table([10, 11], 0),
        source_middle: source_table([0, 1], 1),
        source_after: source_table([40, 41], 4),
        target_middle: target_table([20, 21], 2),
        target_after: target_table([30, 31], 3),
        source_pre_map: arrow([10, 11], [0, 1], 0, 1),
        sigma_hom: arrow([0, 1], [20, 21], 1, 2),
        target_post_map: arrow([20, 21], [30, 31], 2, 3),
    };
    assert_sigma_naturality(&fixture);
    assert_pi_naturality(&fixture);
}

#[derive(Default)]
struct OneRowStats {
    sigma_arrows: usize,
    pi_arrows: usize,
    sigma_refusals: usize,
    pi_refusals: usize,
}

fn check_one_row_sigma(
    case_name: &str,
    functor: &RelFunctor,
    input: &Table,
    target: &Table,
    delta: &Table,
    sigma: &Result<(Table, rel::SigmaProvenance), KanError>,
    stats: &mut OneRowStats,
) {
    match sigma {
        Ok((sigma_table, _)) => {
            let left = rel::homs(sigma_table, target);
            let right = rel::homs(input, delta);
            assert_eq!(left.len(), right.len(), "{case_name} Sigma–Delta");
            for hom in &left {
                let image = rel::sigma_transpose_forward(functor, input, target, hom).unwrap();
                assert!(
                    right
                        .iter()
                        .any(|candidate| candidate.same_as(&image, input))
                );
                let back = rel::sigma_transpose_inverse(functor, input, target, &image).unwrap();
                assert!(back.same_as(hom, sigma_table));
                stats.sigma_arrows += 1;
            }
            for hom in &right {
                let image = rel::sigma_transpose_inverse(functor, input, target, hom).unwrap();
                let back = rel::sigma_transpose_forward(functor, input, target, &image).unwrap();
                assert!(back.same_as(hom, input));
            }
        }
        Err(KanError::SigmaConstantClash { .. }) => stats.sigma_refusals += 1,
        other => panic!("{case_name} Sigma: {other:?}"),
    }
}

fn check_one_row_pi(
    case_name: &str,
    functor: &RelFunctor,
    input: &Table,
    target: &Table,
    delta: &Table,
    pi: &Result<(Table, rel::PiProvenance), KanError>,
    stats: &mut OneRowStats,
) {
    match pi {
        Ok((pi_table, _)) => {
            let left = rel::homs(delta, input);
            let right = rel::homs(target, pi_table);
            assert_eq!(left.len(), right.len(), "{case_name} Delta–Pi");
            for hom in &left {
                let image = rel::pi_transpose_forward(functor, target, input, hom, 81).unwrap();
                assert!(
                    right
                        .iter()
                        .any(|candidate| candidate.same_as(&image, target))
                );
                let back = rel::pi_transpose_inverse(functor, target, input, &image, 81).unwrap();
                assert!(back.same_as(hom, delta));
                stats.pi_arrows += 1;
            }
            for hom in &right {
                let image = rel::pi_transpose_inverse(functor, target, input, hom, 81).unwrap();
                let back = rel::pi_transpose_forward(functor, target, input, &image, 81).unwrap();
                assert!(back.same_as(hom, target));
            }
        }
        Err(KanError::AttributeOutsideImage { .. }) => stats.pi_refusals += 1,
        other => panic!("{case_name} Pi: {other:?}"),
    }
}

#[test]
fn table_adjunction_transposes_biject_the_complete_one_row_family() {
    use panproto_reference::enumerate::kan_extensions as bounded;

    let value_kinds = BTreeSet::from(["value".to_owned()]);
    let non_value_kinds = BTreeSet::from(["record".to_owned()]);
    let mut stats = OneRowStats::default();
    for case in bounded::family() {
        let source = rel::classify(&case.source_schema(), &value_kinds, &non_value_kinds).unwrap();
        let target = rel::classify(&case.target_schema(), &value_kinds, &non_value_kinds).unwrap();
        let functor = rel::entity_functor(&source, &target, &case.migration())
            .unwrap()
            .into_kan()
            .unwrap();
        for input in bounded::instances(&case.source, 1, 2, &case.cells) {
            let sigma = rel::sigma(&functor, &input);
            let pi = rel::pi(&functor, &input, 81);
            for target_instance in bounded::instances(&case.target, 1, 2, &case.cells) {
                let (delta, _) = rel::delta(&functor, &target_instance).unwrap();
                check_one_row_sigma(
                    case.name,
                    &functor,
                    &input,
                    &target_instance,
                    &delta,
                    &sigma,
                    &mut stats,
                );
                check_one_row_pi(
                    case.name,
                    &functor,
                    &input,
                    &target_instance,
                    &delta,
                    &pi,
                    &mut stats,
                );
            }
        }
    }
    assert!(stats.sigma_arrows > 0);
    assert!(stats.pi_arrows > 0);
    assert!(stats.sigma_refusals > 0);
    assert!(stats.pi_refusals > 0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct O14PairStats {
    pairs: u64,
    weighted_pairs: u64,
    sigma_homs: u64,
    pi_bound4_homs: u64,
    pi_bound81_homs: u64,
    pi_naturality_refusals: u64,
    sigma_refusals: u64,
    pi_outside_image: u64,
    pi_over_bound: u64,
}

type SourceProfile = (
    Table,
    u64,
    Result<(Table, rel::SigmaProvenance), KanError>,
    Result<(Table, rel::PiProvenance), KanError>,
    Result<(Table, rel::PiProvenance), KanError>,
);
type TargetProfile = (Table, u64, Table);

fn source_profiles_for_case(
    classes: &BTreeMap<bounded::HomProfile, bounded::HomProfileClass>,
    case_index: usize,
    functor: &RelFunctor,
) -> Vec<SourceProfile> {
    classes
        .iter()
        .filter(|(profile, _)| {
            profile.case == case_index && profile.side == bounded::CaseSide::Source
        })
        .map(|(profile, class)| {
            let certificate = bounded::profile_multiplicity_certificate(
                profile.case,
                profile.side,
                &class.representative,
            )
            .unwrap();
            let input = class.representative.clone();
            let sigma = rel::sigma(functor, &input);
            let pi4 = rel::pi(functor, &input, 4);
            let pi81 = rel::pi(functor, &input, 81);
            (input, certificate.multiplicity, sigma, pi4, pi81)
        })
        .collect()
}

fn target_profiles_for_case(
    classes: &BTreeMap<bounded::HomProfile, bounded::HomProfileClass>,
    case_index: usize,
    functor: &RelFunctor,
) -> Vec<TargetProfile> {
    classes
        .iter()
        .filter(|(profile, _)| {
            profile.case == case_index && profile.side == bounded::CaseSide::Target
        })
        .map(|(profile, class)| {
            let certificate = bounded::profile_multiplicity_certificate(
                profile.case,
                profile.side,
                &class.representative,
            )
            .unwrap();
            let target = class.representative.clone();
            let delta = rel::delta(functor, &target).unwrap().0;
            (target, certificate.multiplicity, delta)
        })
        .collect()
}

fn check_sigma_adjoint_pair(
    f: &RelFunctor,
    input: &Table,
    target: &Table,
    sigma: &Table,
    delta: &Table,
) -> u64 {
    let left = rel::homs(sigma, target);
    let right = rel::homs(input, delta);
    let unit = rel::sigma_unit(f, input).unwrap();
    let counit = rel::sigma_counit(f, target).unwrap();
    let right_keys: BTreeSet<_> = right.iter().map(|hom| hom_key(hom, input)).collect();
    assert_eq!(left.len(), right.len());
    for hom in &left {
        let image = rel::sigma_transpose_forward(f, input, target, hom).unwrap();
        assert!(right_keys.contains(&hom_key(&image, input)));
        let back = rel::sigma_transpose_inverse(f, input, target, &image).unwrap();
        assert!(back.same_as(hom, sigma));

        // Phi(h) = eta_I ; Delta(h), the component formula from which
        // two-variable naturality follows by functoriality and composition.
        let delta_h = delta_on_hom(f, sigma, target, hom).unwrap();
        let formula = unit.compose(&delta_h).unwrap();
        assert!(image.same_as(&formula, input));
    }
    let left_keys: BTreeSet<_> = left.iter().map(|hom| hom_key(hom, sigma)).collect();
    for hom in &right {
        let image = rel::sigma_transpose_inverse(f, input, target, hom).unwrap();
        assert!(left_keys.contains(&hom_key(&image, sigma)));
        let back = rel::sigma_transpose_forward(f, input, target, &image).unwrap();
        assert!(back.same_as(hom, input));

        // Phi^-1(k) = Sigma(k) ; epsilon_J.
        let sigma_h = sigma_on_hom(f, input, delta, hom).unwrap();
        let formula = sigma_h.compose(&counit).unwrap();
        assert!(image.same_as(&formula, sigma));
    }
    u64::try_from(left.len()).unwrap()
}

fn check_pi_adjoint_pair(
    f: &RelFunctor,
    target: &Table,
    input: &Table,
    delta: &Table,
    pi: &Table,
    bound: usize,
) -> (u64, u64) {
    let left = rel::homs(delta, input);
    let right = rel::homs(target, pi);
    let unit = match rel::pi_unit(f, target, bound) {
        Ok(unit) => Some(unit),
        Err(rel::TransposeError::Adjunction(error))
            if matches!(*error, rel::AdjunctionError::ProductSizeExceeded { .. }) =>
        {
            None
        }
        Err(error) => panic!("unexpected Pi unit refusal at bound {bound}: {error:?}"),
    };
    let counit = rel::pi_counit(f, input, bound).unwrap();
    let mut naturality_refusals = 0u64;
    let right_keys: BTreeSet<_> = right.iter().map(|hom| hom_key(hom, target)).collect();
    assert_eq!(left.len(), right.len());
    for hom in &left {
        let image = rel::pi_transpose_forward(f, target, input, hom, bound).unwrap();
        assert!(right_keys.contains(&hom_key(&image, target)));
        let back = rel::pi_transpose_inverse(f, target, input, &image, bound).unwrap();
        assert!(back.same_as(hom, delta));

        // Psi(h) = eta_J ; Pi(h).
        match &unit {
            Some(unit) => {
                let pi_h = pi_on_hom(f, delta, input, hom, bound).unwrap();
                let formula = unit.compose(&pi_h).unwrap();
                assert!(image.same_as(&formula, target));
            }
            None => naturality_refusals += 1,
        }
    }
    let left_keys: BTreeSet<_> = left.iter().map(|hom| hom_key(hom, delta)).collect();
    for hom in &right {
        let image = rel::pi_transpose_inverse(f, target, input, hom, bound).unwrap();
        assert!(left_keys.contains(&hom_key(&image, delta)));
        let back = rel::pi_transpose_forward(f, target, input, &image, bound).unwrap();
        assert!(back.same_as(hom, target));

        // Psi^-1(k) = Delta(k) ; epsilon_I.
        let delta_h = delta_on_hom(f, target, pi, hom).unwrap();
        let formula = delta_h.compose(&counit).unwrap();
        assert!(image.same_as(&formula, delta));
    }
    (u64::try_from(left.len()).unwrap(), naturality_refusals)
}

/// Pairs between two progress lines of the representative-pair executor.
const O14_PROGRESS_INTERVAL: u64 = 250_000;

/// Print pairs done, elapsed seconds and integer pairs per second to stderr.
fn report_o14_progress(pairs: u64, elapsed: std::time::Duration) {
    let millis = elapsed.as_millis().max(1);
    let rate = u128::from(pairs) * 1_000 / millis;
    eprintln!(
        "O14 progress: pairs={pairs}, elapsed={}.{:03}s, pairs/s={rate}",
        elapsed.as_secs(),
        elapsed.subsec_millis(),
    );
}

fn execute_o14_representative_pair_prefix(limit: u64) -> O14PairStats {
    let classes = bounded::hom_profile_classes(3, 6).unwrap();
    let family = bounded::family();
    let value_kinds = BTreeSet::from(["value".to_owned()]);
    let non_value_kinds = BTreeSet::from(["record".to_owned()]);
    let started = std::time::Instant::now();
    let mut stats = O14PairStats::default();

    for (case_index, case) in family.iter().enumerate() {
        let source = rel::classify(&case.source_schema(), &value_kinds, &non_value_kinds).unwrap();
        let target = rel::classify(&case.target_schema(), &value_kinds, &non_value_kinds).unwrap();
        let functor = rel::entity_functor(&source, &target, &case.migration())
            .unwrap()
            .into_kan()
            .unwrap();
        let source_profiles = source_profiles_for_case(&classes, case_index, &functor);
        let target_profiles = target_profiles_for_case(&classes, case_index, &functor);

        for (input, input_weight, sigma, pi4, pi81) in &source_profiles {
            for (target, target_weight, delta) in &target_profiles {
                if stats.pairs == limit {
                    break;
                }
                stats.pairs += 1;
                if stats.pairs % O14_PROGRESS_INTERVAL == 0 {
                    report_o14_progress(stats.pairs, started.elapsed());
                }
                stats.weighted_pairs += input_weight * target_weight;
                match sigma {
                    Ok((sigma, _)) => {
                        stats.sigma_homs +=
                            check_sigma_adjoint_pair(&functor, input, target, sigma, delta);
                    }
                    Err(KanError::SigmaConstantClash { .. }) => stats.sigma_refusals += 1,
                    Err(other) => panic!("{} unexpected Sigma refusal: {other:?}", case.name),
                }
                match pi4 {
                    Ok((pi, _)) => {
                        let (homs, refusals) =
                            check_pi_adjoint_pair(&functor, target, input, delta, pi, 4);
                        stats.pi_bound4_homs += homs;
                        stats.pi_naturality_refusals += refusals;
                    }
                    Err(KanError::AttributeOutsideImage { .. }) => stats.pi_outside_image += 1,
                    Err(KanError::ProductSizeExceeded { .. }) => stats.pi_over_bound += 1,
                    Err(other) => panic!("{} unexpected Pi(bound=4) refusal: {other:?}", case.name),
                }
                match pi81 {
                    Ok((pi, _)) => {
                        let (homs, refusals) =
                            check_pi_adjoint_pair(&functor, target, input, delta, pi, 81);
                        stats.pi_bound81_homs += homs;
                        stats.pi_naturality_refusals += refusals;
                    }
                    Err(KanError::AttributeOutsideImage { .. }) => stats.pi_outside_image += 1,
                    Err(KanError::ProductSizeExceeded { .. }) => stats.pi_over_bound += 1,
                    Err(other) => {
                        panic!("{} unexpected Pi(bound=81) refusal: {other:?}", case.name)
                    }
                }
            }
            if stats.pairs == limit {
                break;
            }
        }
        if stats.pairs == limit {
            break;
        }
    }

    println!(
        "O14 representative executor: pairs={}, weighted={}, Sigma homs={}, Sigma refusals={}, Pi(4) homs={}, Pi(81) homs={}, Pi naturality refusals={}, Pi outside-image={}, Pi over-bound={}, elapsed={:?}",
        stats.pairs,
        stats.weighted_pairs,
        stats.sigma_homs,
        stats.sigma_refusals,
        stats.pi_bound4_homs,
        stats.pi_bound81_homs,
        stats.pi_naturality_refusals,
        stats.pi_outside_image,
        stats.pi_over_bound,
        started.elapsed(),
    );
    stats
}

#[test]
fn o14_executes_every_representative_profile_pair() {
    let stats = execute_o14_representative_pair_prefix(u64::MAX);
    assert_eq!(stats.pairs, 19_120_495);
    assert_eq!(stats.weighted_pairs, 3_918_714_467);
    assert!(stats.sigma_homs > 0);
    assert!(stats.pi_bound4_homs > 0);
    assert!(stats.pi_bound81_homs > 0);
    assert!(stats.sigma_refusals > 0);
    assert!(stats.pi_outside_image > 0);
    assert!(stats.pi_over_bound > 0);
}

#[test]
#[ignore = "development-only throughput pilot; O14 acceptance is the unlimited test"]
fn o14_representative_profile_pair_throughput_pilot() {
    let limit = std::env::var("PANPROTO_KAN_PAIR_LIMIT")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(10_000);
    assert!(limit < 19_120_495);
    let stats = execute_o14_representative_pair_prefix(limit);
    assert_eq!(stats.pairs, limit);
}

#[derive(Default)]
struct MemberStats {
    members: u64,
    sigma_transports: u64,
    delta_transports: u64,
    pi_transports: u64,
    sigma_refusals: u64,
    pi_refusals: u64,
    adapter_comparisons: u64,
    adapter_successes: u64,
    adapter_refusals: u64,
    tree_comparisons: u64,
    tree_successes: u64,
    tree_refusals: u64,
    tree_routes: BTreeMap<(TreeRoute, TreeOutcome), usize>,
}

struct MemberContext<'a> {
    object: &'a bounded::Object,
    representative: &'a Table,
    functor: &'a RelFunctor,
    input_transport: &'a bounded::ProfileTransport,
    input_layout: &'a GraphLayout,
    representative_layout: &'a GraphLayout,
    input_graph_transport: &'a GraphTransport,
    input_tree: &'a Result<Tree, encode::TreeKanError>,
    representative_tree: &'a Result<Tree, encode::TreeKanError>,
}

struct MemberAdapter<'a> {
    left: &'a Table,
    right: &'a Table,
    transport: &'a bounded::ProfileTransport,
    side: &'a GraphSide,
    left_layout: &'a GraphLayout,
    right_layout: &'a GraphLayout,
    left_origins: &'a ExpectedOrigins,
    right_origins: &'a ExpectedOrigins,
}

fn check_member_graph_transport(
    context: &MemberContext<'_>,
    adapter: &MemberAdapter<'_>,
) -> GraphTransport {
    let (left_graph, left_actual_layout, left_nodes, left_edges) = expected_graph(
        adapter.left,
        adapter.side,
        adapter.left_layout,
        context.input_layout,
        adapter.left_origins,
    );
    let (right_graph, right_actual_layout, right_nodes, right_edges) = expected_graph(
        adapter.right,
        adapter.side,
        adapter.right_layout,
        context.representative_layout,
        adapter.right_origins,
    );
    let output_graph_transport =
        graph_transport(&left_actual_layout, &right_actual_layout, adapter.transport);
    assert_graph_transport(
        &left_graph,
        &right_graph,
        adapter.transport,
        &output_graph_transport,
    );
    assert_contributor_transport(
        &left_nodes,
        &right_nodes,
        &output_graph_transport.nodes,
        &context.input_graph_transport.nodes,
    );
    assert_contributor_transport(
        &left_edges,
        &right_edges,
        &output_graph_transport.edges,
        &context.input_graph_transport.edges,
    );
    output_graph_transport
}

fn record_member_tree_route(
    context: &MemberContext<'_>,
    route: TreeRoute,
    left_result: &Result<Tree, encode::TreeKanError>,
    right_result: &Result<Tree, encode::TreeKanError>,
    output_graph_transport: Option<&GraphTransport>,
    output_transport: Option<&bounded::ProfileTransport>,
    stats: &mut MemberStats,
) {
    record_tree_outcome(route, left_result, &mut stats.tree_routes);
    stats.tree_comparisons += 1;
    if assert_tree_route_transport(
        route,
        left_result,
        right_result,
        output_graph_transport,
        output_transport,
    ) {
        stats.tree_successes += 1;
    } else {
        stats.tree_refusals += 1;
    }
    assert!(matches!(
        (context.input_tree, context.representative_tree),
        (Ok(_), Ok(_))
    ));
}

const fn member_tree_pair<'a>(context: &'a MemberContext<'_>) -> Option<(&'a Tree, &'a Tree)> {
    match (context.input_tree, context.representative_tree) {
        (Ok(left), Ok(right)) => Some((left, right)),
        _ => None,
    }
}

fn check_member_sigma_success(
    context: &MemberContext<'_>,
    left: &(Table, rel::SigmaProvenance),
    right: &(Table, rel::SigmaProvenance),
    stats: &mut MemberStats,
) {
    let output_transport = bounded::ProfileTransport {
        rows: context.input_transport.rows.clone(),
        nulls: bounded::sigma_output_null_transport(context.input_transport, &left.1, &right.1)
            .unwrap(),
    };
    assert!(output_transport.check(&left.0, &right.0));
    let (_, left_layout, left_origins) = expected_sigma_adapter(
        context.functor,
        &context.object.table,
        context.input_layout,
        &left.0,
    );
    let (_, right_layout, right_origins) = expected_sigma_adapter(
        context.functor,
        context.representative,
        context.representative_layout,
        &right.0,
    );
    let target_side = literal_whole(&context.functor.tgt);
    let adapter = MemberAdapter {
        left: &left.0,
        right: &right.0,
        transport: &output_transport,
        side: &target_side,
        left_layout: &left_layout,
        right_layout: &right_layout,
        left_origins: &left_origins,
        right_origins: &right_origins,
    };
    let output_graph_transport = check_member_graph_transport(context, &adapter);
    stats.adapter_comparisons += 1;
    stats.adapter_successes += 1;
    if let Some((left_tree, right_tree)) = member_tree_pair(context) {
        record_member_tree_route(
            context,
            TreeRoute::Sigma,
            &encode::t_sigma(context.functor, left_tree),
            &encode::t_sigma(context.functor, right_tree),
            Some(&output_graph_transport),
            Some(&output_transport),
            stats,
        );
    }
    stats.sigma_transports += 1;
}

fn check_member_sigma_clash(
    context: &MemberContext<'_>,
    left: &KanError,
    right: &KanError,
    stats: &mut MemberStats,
) {
    let KanError::SigmaConstantClash {
        row: left_row,
        column: left_column,
        left: left_left,
        right: left_right,
    } = left
    else {
        unreachable!()
    };
    let KanError::SigmaConstantClash {
        row: right_row,
        column: right_column,
        left: right_left,
        right: right_right,
    } = right
    else {
        unreachable!()
    };
    assert!(context.object.table.rows.contains_key(left_row));
    assert!(context.representative.rows.contains_key(right_row));
    assert_eq!(left_column, right_column);
    let clash_pair = |first: &Value, second: &Value| {
        let mut pair = [
            panproto_reference::canonical::value_bytes(first).unwrap(),
            panproto_reference::canonical::value_bytes(second).unwrap(),
        ];
        pair.sort();
        pair
    };
    let left_pair = clash_pair(left_left, left_right);
    let right_pair = clash_pair(right_left, right_right);
    let left_features = sigma_clash_features(context.functor, &context.object.table);
    let right_features = sigma_clash_features(context.functor, context.representative);
    assert_eq!(left_features, right_features);
    assert!(left_features.contains(&(left_pair[0].clone(), left_pair[1].clone())));
    assert!(right_features.contains(&(right_pair[0].clone(), right_pair[1].clone())));
    stats.adapter_comparisons += 1;
    stats.adapter_refusals += 1;
    if let Some((left_tree, right_tree)) = member_tree_pair(context) {
        record_member_tree_route(
            context,
            TreeRoute::Sigma,
            &encode::t_sigma(context.functor, left_tree),
            &encode::t_sigma(context.functor, right_tree),
            None,
            None,
            stats,
        );
    }
    stats.sigma_refusals += 1;
}

fn check_member_sigma(context: &MemberContext<'_>, stats: &mut MemberStats) {
    let left = rel::sigma(context.functor, &context.object.table);
    let right = rel::sigma(context.functor, context.representative);
    match (&left, &right) {
        (Ok(left), Ok(right)) => check_member_sigma_success(context, left, right, stats),
        (
            Err(left @ KanError::SigmaConstantClash { .. }),
            Err(right @ KanError::SigmaConstantClash { .. }),
        ) => {
            check_member_sigma_clash(context, left, right, stats);
        }
        _ => panic!("Sigma profile mismatch: {left:?} versus {right:?}"),
    }
}

fn check_member_pi_success(
    context: &MemberContext<'_>,
    bound: usize,
    left: &(Table, rel::PiProvenance),
    right: &(Table, rel::PiProvenance),
    stats: &mut MemberStats,
) {
    let output_transport = bounded::ProfileTransport {
        rows: bounded::pi_output_row_transport(context.input_transport, &left.1, &right.1).unwrap(),
        nulls: context.input_transport.nulls.clone(),
    };
    assert!(output_transport.check(&left.0, &right.0));
    let (_, left_layout, left_origins) = expected_pi_adapter(
        context.functor,
        &context.object.table,
        context.input_layout,
        &left.0,
    );
    let (_, right_layout, right_origins) = expected_pi_adapter(
        context.functor,
        context.representative,
        context.representative_layout,
        &right.0,
    );
    let target_side = literal_whole(&context.functor.tgt);
    let adapter = MemberAdapter {
        left: &left.0,
        right: &right.0,
        transport: &output_transport,
        side: &target_side,
        left_layout: &left_layout,
        right_layout: &right_layout,
        left_origins: &left_origins,
        right_origins: &right_origins,
    };
    let output_graph_transport = check_member_graph_transport(context, &adapter);
    stats.adapter_comparisons += 1;
    stats.adapter_successes += 1;
    if let Some((left_tree, right_tree)) = member_tree_pair(context) {
        let route = if bound == 4 {
            TreeRoute::Pi4
        } else {
            TreeRoute::Pi81
        };
        record_member_tree_route(
            context,
            route,
            &encode::t_pi(context.functor, left_tree, bound),
            &encode::t_pi(context.functor, right_tree, bound),
            Some(&output_graph_transport),
            Some(&output_transport),
            stats,
        );
    }
    stats.pi_transports += 1;
}

fn check_member_pi_refusal(
    context: &MemberContext<'_>,
    bound: usize,
    left: &KanError,
    right: &KanError,
    stats: &mut MemberStats,
) {
    assert_eq!(left, right);
    assert!(matches!(
        left,
        KanError::AttributeOutsideImage { .. } | KanError::ProductSizeExceeded { .. }
    ));
    stats.adapter_comparisons += 1;
    stats.adapter_refusals += 1;
    if let Some((left_tree, right_tree)) = member_tree_pair(context) {
        let route = if bound == 4 {
            TreeRoute::Pi4
        } else {
            TreeRoute::Pi81
        };
        record_member_tree_route(
            context,
            route,
            &encode::t_pi(context.functor, left_tree, bound),
            &encode::t_pi(context.functor, right_tree, bound),
            None,
            None,
            stats,
        );
    }
    stats.pi_refusals += 1;
}

fn check_member_pi(context: &MemberContext<'_>, bound: usize, stats: &mut MemberStats) {
    let left = rel::pi(context.functor, &context.object.table, bound);
    let right = rel::pi(context.functor, context.representative, bound);
    match (&left, &right) {
        (Ok(left), Ok(right)) => check_member_pi_success(context, bound, left, right, stats),
        (Err(left), Err(right)) => check_member_pi_refusal(context, bound, left, right, stats),
        _ => panic!("Pi(bound={bound}) profile mismatch: {left:?} versus {right:?}"),
    }
}

fn check_member_delta(context: &MemberContext<'_>, stats: &mut MemberStats) {
    let (left, left_provenance) = rel::delta(context.functor, &context.object.table).unwrap();
    let (right, right_provenance) = rel::delta(context.functor, context.representative).unwrap();
    let output_transport = bounded::ProfileTransport {
        rows: bounded::delta_output_row_transport(
            context.input_transport,
            &left_provenance,
            &right_provenance,
        )
        .unwrap(),
        nulls: context.input_transport.nulls.clone(),
    };
    assert!(output_transport.check(&left, &right));
    let (_, left_layout, left_origins) = expected_delta_adapter(
        context.functor,
        &context.object.table,
        context.input_layout,
        &left,
    );
    let (_, right_layout, right_origins) = expected_delta_adapter(
        context.functor,
        context.representative,
        context.representative_layout,
        &right,
    );
    let kept_side = literal_kept(context.functor);
    let adapter = MemberAdapter {
        left: &left,
        right: &right,
        transport: &output_transport,
        side: &kept_side,
        left_layout: &left_layout,
        right_layout: &right_layout,
        left_origins: &left_origins,
        right_origins: &right_origins,
    };
    let output_graph_transport = check_member_graph_transport(context, &adapter);
    stats.adapter_comparisons += 1;
    stats.adapter_successes += 1;
    if let Some((left_tree, right_tree)) = member_tree_pair(context) {
        record_member_tree_route(
            context,
            TreeRoute::Delta,
            &encode::t_delta(context.functor, left_tree),
            &encode::t_delta(context.functor, right_tree),
            Some(&output_graph_transport),
            Some(&output_transport),
            stats,
        );
    }
    stats.delta_transports += 1;
}

fn assert_member_stats(stats: &MemberStats) {
    assert_eq!(stats.members, 248_552);
    assert_eq!(stats.sigma_transports, 127_818);
    assert_eq!(stats.sigma_refusals, 6_386);
    assert_eq!(stats.delta_transports, 114_348);
    assert_eq!(stats.pi_transports, 218_062);
    assert_eq!(stats.pi_refusals, 50_346);
    assert_eq!(stats.adapter_comparisons, 516_960);
    assert_eq!(stats.adapter_successes, 460_228);
    assert_eq!(stats.adapter_refusals, 56_732);
    assert_eq!(stats.tree_comparisons, 3_412);
    assert_eq!(stats.tree_successes, 2_611);
    assert_eq!(stats.tree_refusals, 801);
    assert_eq!(stats.tree_routes, expected_tree_route_outcomes());
    for route in [
        TreeRoute::Sigma,
        TreeRoute::Pi4,
        TreeRoute::Pi81,
        TreeRoute::Delta,
    ] {
        for outcome in all_tree_outcomes() {
            if !expected_tree_route_outcomes().contains_key(&(route, outcome)) {
                assert_eq!(
                    stats
                        .tree_routes
                        .get(&(route, outcome))
                        .copied()
                        .unwrap_or_default(),
                    0,
                    "unexpected member {route:?}/{outcome:?} bucket",
                );
            }
        }
    }
    println!(
        "O14 labelled-member certificate: members={}, Sigma transports={}, Sigma refusals={}, Delta transports={}, Pi transports={}, Pi refusals={}, adapter comparisons={} ({} success/{} refusal), tree comparisons={} ({} success/{} refusal)",
        stats.members,
        stats.sigma_transports,
        stats.sigma_refusals,
        stats.delta_transports,
        stats.pi_transports,
        stats.pi_refusals,
        stats.adapter_comparisons,
        stats.adapter_successes,
        stats.adapter_refusals,
        stats.tree_comparisons,
        stats.tree_successes,
        stats.tree_refusals,
    );
}

#[test]
fn o14_every_labelled_member_has_checked_operation_transports() {
    let family = bounded::family();
    let value_kinds = BTreeSet::from(["value".to_owned()]);
    let non_value_kinds = BTreeSet::from(["record".to_owned()]);
    let functors = family
        .iter()
        .map(|case| {
            let source =
                rel::classify(&case.source_schema(), &value_kinds, &non_value_kinds).unwrap();
            let target =
                rel::classify(&case.target_schema(), &value_kinds, &non_value_kinds).unwrap();
            rel::entity_functor(&source, &target, &case.migration())
                .unwrap()
                .into_kan()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let classes = bounded::hom_profile_classes(3, 6).unwrap();
    let mut stats = MemberStats::default();

    for object in bounded::objects(3, 6) {
        let (profile, relabeling) = bounded::hom_profile(&object).unwrap();
        let representative = &classes[&profile].representative;
        let input_transport = bounded::ProfileTransport {
            rows: relabeling.rows,
            nulls: relabeling.nulls,
        };
        assert!(input_transport.check(&object.table, representative));
        let functor = &functors[object.case];
        let input_side = match object.side {
            bounded::CaseSide::Source => literal_whole(&functor.src),
            bounded::CaseSide::Target => literal_whole(&functor.tgt),
        };
        let (input_graph, input_layout) = literal_graph_and_layout(&object.table, &input_side);
        let (representative_graph, representative_layout) =
            literal_graph_and_layout(representative, &input_side);
        let input_graph_transport =
            graph_transport(&input_layout, &representative_layout, &input_transport);
        assert_graph_transport(
            &input_graph,
            &representative_graph,
            &input_transport,
            &input_graph_transport,
        );
        let input_tree = graph_tree_form(&input_graph);
        let representative_tree = graph_tree_form(&representative_graph);
        assert_input_tree_transport(
            &input_tree,
            &representative_tree,
            &input_graph,
            &representative_graph,
            &input_transport,
            &input_graph_transport,
        );
        let identity = Hom {
            rows: object.table.rows.keys().map(|row| (*row, *row)).collect(),
            nulls: BTreeMap::new(),
        };
        let conjugated =
            bounded::conjugate_hom(&identity, &input_transport, &input_transport).unwrap();
        let representative_identity = Hom {
            rows: representative.rows.keys().map(|row| (*row, *row)).collect(),
            nulls: BTreeMap::new(),
        };
        assert!(conjugated.same_as(&representative_identity, representative));

        let context = MemberContext {
            object: &object,
            representative,
            functor,
            input_transport: &input_transport,
            input_layout: &input_layout,
            representative_layout: &representative_layout,
            input_graph_transport: &input_graph_transport,
            input_tree: &input_tree,
            representative_tree: &representative_tree,
        };
        match object.side {
            bounded::CaseSide::Source => {
                check_member_sigma(&context, &mut stats);
                check_member_pi(&context, 4, &mut stats);
                check_member_pi(&context, 81, &mut stats);
            }
            bounded::CaseSide::Target => check_member_delta(&context, &mut stats),
        }
        stats.members += 1;
    }
    assert_member_stats(&stats);
}

#[derive(Default)]
struct LiteralStats {
    objects: usize,
    sigma_outputs: usize,
    sigma_clashes: usize,
    delta_outputs: usize,
    pi_outputs: [usize; 2],
    pi_products: [usize; 2],
    pi_outside: [usize; 2],
    graph_squares: usize,
    source_tree_domains: Vec<BTreeMap<TreeDomain, usize>>,
    target_tree_domains: Vec<BTreeMap<TreeDomain, usize>>,
    tree_routes: BTreeMap<(TreeRoute, TreeOutcome), usize>,
}

struct LiteralContext<'a> {
    case: &'a bounded::Case,
    case_index: usize,
    functor: &'a RelFunctor,
    source: &'a RelSchema,
    target: &'a RelSchema,
    source_side: &'a GraphSide,
    target_side: &'a GraphSide,
}

fn check_literal_sigma(
    context: &LiteralContext<'_>,
    input: &Table,
    input_graph: &Graph,
    input_layout: &GraphLayout,
    input_tree: Option<&Tree>,
    stats: &mut LiteralStats,
) {
    match rel::sigma(context.functor, input) {
        Ok((output, provenance)) => {
            rel::conform(&output, &context.target.side()).unwrap();
            let (expected_provenance, expected_layout, expected_origins) =
                expected_sigma_adapter(context.functor, input, input_layout, &output);
            assert_eq!(provenance, expected_provenance);
            assert_sigma_provenance(input, &output, &provenance);
            let (expected_graph, _, expected_nodes, expected_edges) = expected_graph(
                &output,
                context.target_side,
                &expected_layout,
                input_layout,
                &expected_origins,
            );
            let (graph_output, graph_provenance) =
                encode::g_sigma_along_with_provenance(context.functor, input_graph).unwrap();
            assert_eq!(graph_output, expected_graph);
            assert_eq!(graph_provenance.table, provenance);
            assert_graph_provenance(
                &graph_output,
                &graph_provenance.nodes,
                &graph_provenance.edges,
                &expected_nodes,
                &expected_edges,
            );
            let (encoded, _) = encode::encode_graph(&graph_output, context.target_side).unwrap();
            assert!(panproto_reference::migration::table_square(
                &output, &encoded
            ));
            stats.graph_squares += 1;
            if let Some(tree) = input_tree {
                let expected = graph_tree_form(&expected_graph);
                let actual = encode::t_sigma(context.functor, tree);
                assert_eq!(actual, expected, "{} tree Sigma", context.case.name);
                record_tree_outcome(TreeRoute::Sigma, &actual, &mut stats.tree_routes);
            }
            stats.sigma_outputs += 1;
        }
        Err(error @ KanError::SigmaConstantClash { .. }) => {
            let expected_error = literal_adjunction_error(&error);
            assert_eq!(
                encode::g_sigma_along(context.functor, input_graph),
                Err(GraphKanError::Adjunction(Box::new(expected_error.clone()))),
            );
            if let Some(tree) = input_tree {
                let expected = Err(encode::TreeKanError::Graph(GraphKanError::Adjunction(
                    Box::new(expected_error),
                )));
                let actual = encode::t_sigma(context.functor, tree);
                assert_eq!(actual, expected, "{} refused tree Sigma", context.case.name);
                record_tree_outcome(TreeRoute::Sigma, &actual, &mut stats.tree_routes);
            }
            stats.sigma_clashes += 1;
        }
        other => panic!("{} Sigma: {other:?}", context.case.name),
    }
}

fn record_literal_pi_tree(
    context: &LiteralContext<'_>,
    input_tree: Option<&Tree>,
    expected_graph: Option<&Graph>,
    expected_error: Option<rel::AdjunctionError>,
    bound: usize,
    stats: &mut LiteralStats,
) {
    let Some(tree) = input_tree else { return };
    let expected = expected_graph.map_or_else(
        || {
            expected_pi_tree_precheck(context.functor).map_or_else(
                || {
                    Err(encode::TreeKanError::Graph(GraphKanError::Adjunction(
                        Box::new(expected_error.unwrap()),
                    )))
                },
                Err,
            )
        },
        |graph| expected_pi_tree(context.functor, graph),
    );
    let actual = encode::t_pi(context.functor, tree, bound);
    assert_eq!(actual, expected, "{} tree Pi({bound})", context.case.name);
    let route = if bound == 4 {
        TreeRoute::Pi4
    } else {
        TreeRoute::Pi81
    };
    record_tree_outcome(route, &actual, &mut stats.tree_routes);
}

fn check_literal_pi(
    context: &LiteralContext<'_>,
    input: &Table,
    input_graph: &Graph,
    input_layout: &GraphLayout,
    input_tree: Option<&Tree>,
    bound: usize,
    stats: &mut LiteralStats,
) {
    let index = usize::from(bound != bounded::PRODUCT_BOUNDS[0]);
    match rel::pi(context.functor, input, bound) {
        Ok((output, provenance)) => {
            rel::conform(&output, &context.target.side()).unwrap();
            let (expected_provenance, expected_layout, expected_origins) =
                expected_pi_adapter(context.functor, input, input_layout, &output);
            assert_eq!(provenance, expected_provenance);
            let (expected_graph, _, expected_nodes, expected_edges) = expected_graph(
                &output,
                context.target_side,
                &expected_layout,
                input_layout,
                &expected_origins,
            );
            let (graph_output, graph_provenance) =
                encode::g_pi_along_with_provenance(context.functor, input_graph, bound).unwrap();
            assert_eq!(graph_output, expected_graph);
            assert_eq!(graph_provenance.table, provenance);
            assert_graph_provenance(
                &graph_output,
                &graph_provenance.nodes,
                &graph_provenance.edges,
                &expected_nodes,
                &expected_edges,
            );
            let (encoded, _) = encode::encode_graph(&graph_output, context.target_side).unwrap();
            assert!(panproto_reference::migration::table_square(
                &output, &encoded
            ));
            stats.graph_squares += 1;
            record_literal_pi_tree(
                context,
                input_tree,
                Some(&expected_graph),
                None,
                bound,
                stats,
            );
            stats.pi_outputs[index] += 1;
        }
        Err(error @ KanError::ProductSizeExceeded { actual, limit, .. }) => {
            assert!(actual > limit);
            assert_eq!(limit, bound);
            check_literal_pi_refusal(context, input_graph, input_tree, &error, bound, stats);
            stats.pi_products[index] += 1;
        }
        Err(error @ KanError::AttributeOutsideImage { .. }) => {
            assert!(!context.case.pi);
            check_literal_pi_refusal(context, input_graph, input_tree, &error, bound, stats);
            stats.pi_outside[index] += 1;
        }
        other => panic!("{} Pi({bound}): {other:?}", context.case.name),
    }
}

fn check_literal_pi_refusal(
    context: &LiteralContext<'_>,
    input_graph: &Graph,
    input_tree: Option<&Tree>,
    error: &KanError,
    bound: usize,
    stats: &mut LiteralStats,
) {
    let expected_error = literal_adjunction_error(error);
    assert_eq!(
        encode::g_pi_along(context.functor, input_graph, bound),
        Err(GraphKanError::Adjunction(Box::new(expected_error.clone()))),
    );
    record_literal_pi_tree(
        context,
        input_tree,
        None,
        Some(expected_error),
        bound,
        stats,
    );
}

fn check_literal_source(context: &LiteralContext<'_>, input: &Table, stats: &mut LiteralStats) {
    rel::conform(input, &context.source.side()).unwrap();
    let (input_graph, input_layout) = literal_graph_and_layout(input, context.source_side);
    assert_eq!(
        encode::decode_table(input, context.source_side, &GraphLayout::default()).unwrap(),
        input_graph,
    );
    let input_tree_result = graph_tree_form(&input_graph);
    *stats.source_tree_domains[context.case_index]
        .entry(tree_domain(&input_tree_result))
        .or_default() += 1;
    let input_tree = input_tree_result.as_ref().ok();
    check_literal_sigma(
        context,
        input,
        &input_graph,
        &input_layout,
        input_tree,
        stats,
    );
    for bound in bounded::PRODUCT_BOUNDS {
        check_literal_pi(
            context,
            input,
            &input_graph,
            &input_layout,
            input_tree,
            bound,
            stats,
        );
    }
}

fn check_literal_target(context: &LiteralContext<'_>, input: &Table, stats: &mut LiteralStats) {
    rel::conform(input, &context.target.side()).unwrap();
    let (output, provenance) = rel::delta(context.functor, input).unwrap();
    rel::conform(&output, &context.functor.kept_side()).unwrap();
    let (input_graph, input_layout) = literal_graph_and_layout(input, context.target_side);
    assert_eq!(
        encode::decode_table(input, context.target_side, &GraphLayout::default()).unwrap(),
        input_graph,
    );
    let kept_side = literal_kept(context.functor);
    let (expected_provenance, expected_layout, expected_origins) =
        expected_delta_adapter(context.functor, input, &input_layout, &output);
    assert_eq!(provenance, expected_provenance);
    let (expected_graph, _, expected_nodes, expected_edges) = expected_graph(
        &output,
        &kept_side,
        &expected_layout,
        &input_layout,
        &expected_origins,
    );
    let (graph_output, graph_provenance) =
        encode::g_delta_along_with_provenance(context.functor, &input_graph).unwrap();
    assert_eq!(graph_output, expected_graph);
    assert_eq!(graph_provenance.table, provenance);
    assert_graph_provenance(
        &graph_output,
        &graph_provenance.nodes,
        &graph_provenance.edges,
        &expected_nodes,
        &expected_edges,
    );
    let (encoded, _) = encode::encode_graph(&graph_output, &kept_side).unwrap();
    assert!(panproto_reference::migration::table_square(
        &output, &encoded
    ));
    stats.graph_squares += 1;
    let input_tree_result = graph_tree_form(&input_graph);
    *stats.target_tree_domains[context.case_index]
        .entry(tree_domain(&input_tree_result))
        .or_default() += 1;
    if let Ok(tree) = input_tree_result {
        let expected = expected_delta_tree(context.functor, &expected_graph);
        let actual = encode::t_delta(context.functor, &tree);
        assert_eq!(actual, expected, "{} tree Delta", context.case.name);
        record_tree_outcome(TreeRoute::Delta, &actual, &mut stats.tree_routes);
    }
    stats.delta_outputs += 1;
}

fn assert_literal_tree_stats(stats: &LiteralStats) {
    assert_eq!(stats.source_tree_domains, expected_source_tree_domains());
    assert_eq!(stats.target_tree_domains, expected_target_tree_domains());
    let source_total: usize = stats
        .source_tree_domains
        .iter()
        .flat_map(BTreeMap::values)
        .sum();
    let target_total: usize = stats
        .target_tree_domains
        .iter()
        .flat_map(BTreeMap::values)
        .sum();
    let source_trees: usize = stats
        .source_tree_domains
        .iter()
        .map(|counts| counts.get(&TreeDomain::Tree).copied().unwrap_or_default())
        .sum();
    let target_trees: usize = stats
        .target_tree_domains
        .iter()
        .map(|counts| counts.get(&TreeDomain::Tree).copied().unwrap_or_default())
        .sum();
    assert_eq!((source_total, target_total), (134_204, 114_348));
    assert_eq!((source_trees, target_trees), (706, 1_294));
    for (route, expected) in [
        (TreeRoute::Sigma, source_trees),
        (TreeRoute::Pi4, source_trees),
        (TreeRoute::Pi81, source_trees),
        (TreeRoute::Delta, target_trees),
    ] {
        assert_eq!(
            stats
                .tree_routes
                .iter()
                .filter(|((candidate, _), _)| *candidate == route)
                .map(|(_, count)| count)
                .sum::<usize>(),
            expected,
        );
    }
    assert_eq!(stats.tree_routes.values().sum::<usize>(), 3_412);
    assert_eq!(stats.tree_routes, expected_tree_route_outcomes());
}

fn assert_literal_tree_outcomes(stats: &LiteralStats) {
    let successes = stats
        .tree_routes
        .iter()
        .filter(|((_, outcome), _)| *outcome == TreeOutcome::Ok)
        .map(|(_, count)| count)
        .sum::<usize>();
    assert_eq!(successes, 2_611);
    assert_eq!(stats.tree_routes.values().sum::<usize>() - successes, 801);
    for route in [
        TreeRoute::Sigma,
        TreeRoute::Pi4,
        TreeRoute::Pi81,
        TreeRoute::Delta,
    ] {
        for outcome in all_tree_outcomes() {
            let count = stats
                .tree_routes
                .get(&(route, outcome))
                .copied()
                .unwrap_or_default();
            let allowed = match route {
                TreeRoute::Sigma => matches!(
                    outcome,
                    TreeOutcome::Ok
                        | TreeOutcome::Graph(GraphOutcome::Adjunction(
                            AdjunctionKind::SigmaConstantClash
                        ))
                        | TreeOutcome::RootCount(_)
                        | TreeOutcome::NotATree(_)
                ),
                TreeRoute::Pi4 | TreeRoute::Pi81 => matches!(
                    outcome,
                    TreeOutcome::Ok
                        | TreeOutcome::NonInjectiveVertexMap
                        | TreeOutcome::Graph(GraphOutcome::Adjunction(
                            AdjunctionKind::ProductSizeExceeded
                                | AdjunctionKind::AttributeOutsideImage
                        ))
                        | TreeOutcome::RootCount(_)
                        | TreeOutcome::NotATree(_)
                ),
                TreeRoute::Delta => matches!(
                    outcome,
                    TreeOutcome::Ok
                        | TreeOutcome::NonInjectiveVertexMap
                        | TreeOutcome::NonInjectiveEdgeMap
                        | TreeOutcome::RootCount(_)
                        | TreeOutcome::NotATree(_)
                ),
            };
            if !allowed {
                assert_eq!(count, 0, "impossible {route:?}/{outcome:?} bucket");
            }
            if route == TreeRoute::Pi81
                && outcome
                    == TreeOutcome::Graph(GraphOutcome::Adjunction(
                        AdjunctionKind::ProductSizeExceeded,
                    ))
            {
                assert_eq!(count, 0);
            }
        }
    }
}

fn assert_literal_stats(stats: &LiteralStats) {
    assert_eq!(stats.objects, 248_552);
    assert_eq!(stats.sigma_outputs, 127_818);
    assert_eq!(stats.sigma_clashes, 6_386);
    assert_eq!(stats.delta_outputs, 114_348);
    assert_eq!(stats.pi_outputs, [83_943, 134_119]);
    assert_eq!(stats.pi_products, [50_176, 0]);
    assert_eq!(stats.pi_outside, [85, 85]);
    assert_eq!(stats.graph_squares, 460_228);
    assert_literal_tree_stats(stats);
    assert_literal_tree_outcomes(stats);
    println!(
        "bounded table Kan: objects={}; Sigma ok={}, clash={}; Delta ok={}; Pi bounds {:?}, ok={:?}, over-bound={:?}, outside-image={:?}; graph/table squares={}; source tree domains={:?}; target tree domains={:?}; tree routes={:?}",
        stats.objects,
        stats.sigma_outputs,
        stats.sigma_clashes,
        stats.delta_outputs,
        bounded::PRODUCT_BOUNDS,
        stats.pi_outputs,
        stats.pi_products,
        stats.pi_outside,
        stats.graph_squares,
        stats.source_tree_domains,
        stats.target_tree_domains,
        stats.tree_routes,
    );
}
#[test]
fn complete_bounded_family_has_typed_kan_results_and_refusals() {
    let value_kinds = BTreeSet::from(["value".to_owned()]);
    let non_value_kinds = BTreeSet::from(["record".to_owned()]);
    let mut stats = LiteralStats {
        source_tree_domains: vec![BTreeMap::new(); 10],
        target_tree_domains: vec![BTreeMap::new(); 10],
        ..LiteralStats::default()
    };

    for (case_index, case) in bounded::family().into_iter().enumerate() {
        let source = rel::classify(&case.source_schema(), &value_kinds, &non_value_kinds).unwrap();
        let target = rel::classify(&case.target_schema(), &value_kinds, &non_value_kinds).unwrap();
        let functor = rel::entity_functor(&source, &target, &case.migration())
            .unwrap()
            .into_kan()
            .unwrap();
        let source_side = literal_whole(&source);
        let target_side = literal_whole(&target);
        let sources = bounded::instances(&case.source, 3, 6, &case.cells);
        let targets = bounded::instances(&case.target, 3, 6, &case.cells);
        stats.objects += sources.len() + targets.len();
        let context = LiteralContext {
            case: &case,
            case_index,
            functor: &functor,
            source: &source,
            target: &target,
            source_side: &source_side,
            target_side: &target_side,
        };
        for input in &sources {
            check_literal_source(&context, input, &mut stats);
        }
        for input in &targets {
            check_literal_target(&context, input, &mut stats);
        }
    }
    assert_literal_stats(&stats);
}
#[test]
fn sigma_profile_transport_derives_fresh_null_images_from_slots() {
    use panproto_reference::enumerate::kan_extensions as bounded;

    let case = bounded::family()[3].clone();
    let source = rel::classify(
        &case.source_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let target = rel::classify(
        &case.target_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let functor = rel::entity_functor(&source, &target, &case.migration())
        .unwrap()
        .into_kan()
        .unwrap();
    let make = |id, null| Table {
        rows: BTreeMap::from([(id, row("a", &[("x", Value::LabeledNull(null))]))]),
        pairs: BTreeSet::new(),
    };
    let left = bounded::Object {
        case: 3,
        side: bounded::CaseSide::Source,
        table: make(0, 0),
    };
    let right = bounded::Object {
        case: 3,
        side: bounded::CaseSide::Source,
        table: make(1, 1),
    };
    let (left_profile, left_map) = bounded::hom_profile(&left).unwrap();
    let (right_profile, right_map) = bounded::hom_profile(&right).unwrap();
    assert_eq!(left_profile, right_profile);
    let input_transport = left_map.between(&right_map).unwrap();
    assert_eq!(input_transport.rows, BTreeMap::from([(0, 1)]));
    assert_eq!(input_transport.nulls, BTreeMap::from([(0, 1)]));
    assert!(input_transport.check(&left.table, &right.table));
    let identity_left = Hom {
        rows: BTreeMap::from([(0, 0)]),
        nulls: BTreeMap::new(),
    };
    let conjugated =
        bounded::conjugate_hom(&identity_left, &input_transport, &input_transport).unwrap();
    assert!(conjugated.same_as(
        &Hom {
            rows: BTreeMap::from([(1, 1)]),
            nulls: BTreeMap::new(),
        },
        &right.table,
    ));

    let (left_output, left_provenance) = rel::sigma(&functor, &left.table).unwrap();
    let (right_output, right_provenance) = rel::sigma(&functor, &right.table).unwrap();
    assert_eq!(left_output.rows[&0].1["y"], Value::LabeledNull(1));
    assert_eq!(right_output.rows[&1].1["y"], Value::LabeledNull(2));
    assert_eq!(
        left_provenance.fresh,
        BTreeMap::from([(1, (0, "y".to_owned()))])
    );
    assert_eq!(
        right_provenance.fresh,
        BTreeMap::from([(2, (1, "y".to_owned()))])
    );
    let output_transport = bounded::ProfileTransport {
        rows: input_transport.rows.clone(),
        nulls: bounded::sigma_output_null_transport(
            &input_transport,
            &left_provenance,
            &right_provenance,
        )
        .unwrap(),
    };
    assert_eq!(output_transport.nulls, BTreeMap::from([(0, 1), (1, 2)]));
    assert!(output_transport.check(&left_output, &right_output));
}

#[test]
fn sigma_profile_transport_normalizes_a_swapped_merged_null_class() {
    use panproto_reference::enumerate::kan_extensions as bounded;

    let case = bounded::family()[7].clone();
    let source = rel::classify(
        &case.source_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let target = rel::classify(
        &case.target_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let functor = rel::entity_functor(&source, &target, &case.migration())
        .unwrap()
        .into_kan()
        .unwrap();
    let make = |first, second| Table {
        rows: BTreeMap::from([(
            0,
            row(
                "t",
                &[
                    ("a1", Value::LabeledNull(first)),
                    ("a2", Value::LabeledNull(second)),
                ],
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let left = bounded::Object {
        case: 7,
        side: bounded::CaseSide::Source,
        table: make(0, 1),
    };
    let right = bounded::Object {
        case: 7,
        side: bounded::CaseSide::Source,
        table: make(1, 0),
    };
    let (left_profile, left_map) = bounded::hom_profile(&left).unwrap();
    let (right_profile, right_map) = bounded::hom_profile(&right).unwrap();
    assert_eq!(left_profile, right_profile);
    let input_transport = left_map.between(&right_map).unwrap();
    assert_eq!(input_transport.nulls, BTreeMap::from([(0, 1), (1, 0)]));
    assert!(input_transport.check(&left.table, &right.table));
    let (left_output, left_provenance) = rel::sigma(&functor, &left.table).unwrap();
    let (right_output, right_provenance) = rel::sigma(&functor, &right.table).unwrap();
    assert_eq!(left_output.rows[&0].1["b"], Value::LabeledNull(0));
    assert_eq!(right_output.rows[&0].1["b"], Value::LabeledNull(0));
    assert_eq!(
        left_provenance.merged,
        BTreeMap::from([(1, Value::LabeledNull(0))])
    );
    assert_eq!(
        right_provenance.merged,
        BTreeMap::from([(1, Value::LabeledNull(0))])
    );
    let output_transport = bounded::ProfileTransport {
        rows: input_transport.rows.clone(),
        nulls: bounded::sigma_output_null_transport(
            &input_transport,
            &left_provenance,
            &right_provenance,
        )
        .unwrap(),
    };
    assert_eq!(output_transport.nulls, BTreeMap::from([(0, 0)]));
    assert!(output_transport.check(&left_output, &right_output));
}

#[test]
fn delta_and_pi_profile_transports_follow_copy_and_tuple_provenance() {
    use panproto_reference::enumerate::kan_extensions as bounded;

    let case = bounded::family()[0].clone();
    let source = rel::classify(
        &case.source_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let target = rel::classify(
        &case.target_schema(),
        &BTreeSet::from(["value".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let functor = rel::entity_functor(&source, &target, &case.migration())
        .unwrap()
        .into_kan()
        .unwrap();

    let left_target = bounded::Object {
        case: 0,
        side: bounded::CaseSide::Target,
        table: Table {
            rows: BTreeMap::from([(0, row("m", &[("x", Value::Int(0)), ("y", Value::Int(1))]))]),
            pairs: BTreeSet::new(),
        },
    };
    let right_target = bounded::Object {
        case: 0,
        side: bounded::CaseSide::Target,
        table: Table {
            rows: BTreeMap::from([(5, row("m", &[("x", Value::Int(0)), ("y", Value::Int(1))]))]),
            pairs: BTreeSet::new(),
        },
    };
    let (left_profile, left_map) = bounded::hom_profile(&left_target).unwrap();
    let (right_profile, right_map) = bounded::hom_profile(&right_target).unwrap();
    assert_eq!(left_profile, right_profile);
    let target_transport = left_map.between(&right_map).unwrap();
    assert!(target_transport.check(&left_target.table, &right_target.table));
    let (left_delta, left_delta_provenance) = rel::delta(&functor, &left_target.table).unwrap();
    let (right_delta, right_delta_provenance) = rel::delta(&functor, &right_target.table).unwrap();
    let delta_transport = bounded::ProfileTransport {
        rows: bounded::delta_output_row_transport(
            &target_transport,
            &left_delta_provenance,
            &right_delta_provenance,
        )
        .unwrap(),
        nulls: target_transport.nulls.clone(),
    };
    assert_eq!(delta_transport.rows, BTreeMap::from([(0, 5), (1, 6)]));
    assert!(delta_transport.check(&left_delta, &right_delta));

    let left_source = bounded::Object {
        case: 0,
        side: bounded::CaseSide::Source,
        table: Table {
            rows: BTreeMap::from([
                (0, row("a", &[("x", Value::LabeledNull(0))])),
                (1, row("b", &[("y", Value::Int(1))])),
            ]),
            pairs: BTreeSet::new(),
        },
    };
    let right_source = bounded::Object {
        case: 0,
        side: bounded::CaseSide::Source,
        table: Table {
            rows: BTreeMap::from([
                (5, row("a", &[("x", Value::LabeledNull(1))])),
                (6, row("b", &[("y", Value::Int(1))])),
            ]),
            pairs: BTreeSet::new(),
        },
    };
    let (left_profile, left_map) = bounded::hom_profile(&left_source).unwrap();
    let (right_profile, right_map) = bounded::hom_profile(&right_source).unwrap();
    assert_eq!(left_profile, right_profile);
    let source_transport = left_map.between(&right_map).unwrap();
    assert!(source_transport.check(&left_source.table, &right_source.table));
    let (left_pi, left_pi_provenance) = rel::pi(&functor, &left_source.table, 81).unwrap();
    let (right_pi, right_pi_provenance) = rel::pi(&functor, &right_source.table, 81).unwrap();
    let pi_transport = bounded::ProfileTransport {
        rows: bounded::pi_output_row_transport(
            &source_transport,
            &left_pi_provenance,
            &right_pi_provenance,
        )
        .unwrap(),
        nulls: source_transport.nulls.clone(),
    };
    assert_eq!(pi_transport.rows, BTreeMap::from([(2, 7)]));
    assert_eq!(pi_transport.nulls, BTreeMap::from([(0, 1)]));
    assert!(pi_transport.check(&left_pi, &right_pi));
}

#[test]
fn every_extension_along_the_identity_keeps_row_ids_exactly() {
    // Row ids 5 and 7, not numbered from 0: the identity returns them as
    // they are, Pi included (each fibre is one table, whose rows keep their
    // ids).
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let mut i = Table::default();
    i.rows.insert(5, row("a", &[("x", Value::Int(1))]));
    i.rows.insert(7, row("b", &[("y", Value::Int(2))]));
    i.pairs
        .insert((SEdge::new("a", "b", "fk", Some("f")), 5, 7));
    assert_eq!(rel::delta(&f, &i).unwrap().0, i);
    assert_eq!(rel::sigma(&f, &i).unwrap().0, i);
    let (p, prov) = rel::pi(&f, &i, usize::MAX).unwrap();
    assert_eq!(p, i);
    assert_eq!(
        prov,
        BTreeMap::from([
            (5, ("a".to_owned(), vec![5])),
            (7, ("b".to_owned(), vec![7]))
        ])
    );
}

/// Four record tables a, b, c, d, and an unmapped table u.
fn four_and_unmapped() -> Schema {
    ["a", "b", "c", "d", "u"]
        .into_iter()
        .fold(Schema::default(), |s, v| s.vertex(v, "record"))
}

#[test]
fn pi_allocates_fresh_rows_above_every_input_row_in_fibre_order() {
    // y <- {a, b}, x <- {c, d}, and k outside the image. Fresh ids start
    // above the input's greatest row id (9, at the unmapped table u): the
    // image tables first, ordered by the least table of their fibre (y
    // before x), then the tables outside the image by name.
    let source = four_and_unmapped();
    let target = ["k", "x", "y"]
        .into_iter()
        .fold(Schema::default(), |s, v| s.vertex(v, "record"));
    let mig = Migration {
        vertex_remap: [("a", "y"), ("b", "y"), ("c", "x"), ("d", "x")]
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    let mut i = Table::default();
    for (id, t) in [(0, "a"), (1, "b"), (2, "c"), (3, "d"), (9, "u")] {
        i.rows.insert(id, row(t, &[]));
    }
    let (p, prov) = rel::pi(&f, &i, usize::MAX).unwrap();
    assert_eq!(
        prov,
        BTreeMap::from([
            (10, ("y".to_owned(), vec![0, 1])),
            (11, ("x".to_owned(), vec![2, 3])),
            (12, ("k".to_owned(), vec![])),
        ])
    );
    assert_eq!(p.rows.keys().copied().collect::<Vec<_>>(), vec![10, 11, 12]);
}

#[test]
fn delta_and_pi_allocate_the_maximum_once_then_report_exhaustion() {
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record");
    let target = Schema::default().vertex("m", "record");
    let migration = Migration {
        vertex_remap: [
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]
        .into(),
        ..Migration::default()
    };
    let f = functor(&source, &target, &migration);

    let target_instance = |id| Table {
        rows: BTreeMap::from([(id, row("m", &[]))]),
        pairs: BTreeSet::new(),
    };
    let (delta_near, _) = rel::delta(&f, &target_instance(u32::MAX - 1)).unwrap();
    assert_eq!(
        delta_near.rows.keys().copied().collect::<Vec<_>>(),
        vec![u32::MAX - 1, u32::MAX]
    );
    assert_eq!(
        rel::delta(&f, &target_instance(u32::MAX)),
        Err(KanError::RowIdsExhausted)
    );

    let source_instance = |high| Table {
        rows: BTreeMap::from([(0, row("a", &[])), (high, row("b", &[]))]),
        pairs: BTreeSet::new(),
    };
    let (pi_near, _) = rel::pi(&f, &source_instance(u32::MAX - 1), usize::MAX).unwrap();
    assert_eq!(
        pi_near.rows.keys().copied().collect::<Vec<_>>(),
        vec![u32::MAX]
    );
    assert_eq!(
        rel::pi(&f, &source_instance(u32::MAX), usize::MAX),
        Err(KanError::RowIdsExhausted)
    );
}

#[test]
fn sigma_numbers_fresh_nulls_above_every_input_null() {
    // The input's greatest null (7) sits at the unmapped table u; the fresh
    // null of the target column z starts above it.
    let source = Schema::default()
        .vertex("r", "record")
        .vertex("u", "record")
        .vertex("r.x", "int")
        .vertex("u.w", "int")
        .edge("r", "r.x", "prop", Some("x"))
        .edge("u", "u.w", "prop", Some("w"));
    let target = Schema::default()
        .vertex("r", "record")
        .vertex("r.x", "int")
        .vertex("r.z", "int")
        .edge("r", "r.x", "prop", Some("x"))
        .edge("r", "r.z", "prop", Some("z"));
    let mig = Migration {
        surviving_verts: ["r".to_owned(), "r.x".to_owned()].into(),
        surviving_edges: [SEdge::new("r", "r.x", "prop", Some("x"))].into(),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    let mut i = Table::default();
    i.rows.insert(0, row("r", &[("x", Value::Int(1))]));
    i.rows.insert(1, row("u", &[("w", Value::LabeledNull(7))]));
    let (out, prov) = rel::sigma(&f, &i).unwrap();
    assert_eq!(out.rows[&0].1["z"], Value::LabeledNull(8));
    assert_eq!(prov.fresh, BTreeMap::from([(8, (0, "z".to_owned()))]));
}

#[test]
fn sigma_records_as_merged_only_the_nulls_another_value_represents() {
    // Two null cells merged into one column: the class {null 0, null 1} is
    // represented by its least null, so only null 1 is merged; null 0
    // represents itself.
    let source = Schema::default()
        .vertex("r", "record")
        .vertex("r.x", "int")
        .vertex("r.y", "int")
        .edge("r", "r.x", "prop", Some("x"))
        .edge("r", "r.y", "prop", Some("y"));
    let target = Schema::default()
        .vertex("r", "record")
        .vertex("r.z", "int")
        .edge("r", "r.z", "prop", Some("z"));
    let merged = SEdge::new("r", "r.z", "prop", Some("z"));
    let mig = Migration {
        surviving_verts: ["r".to_owned()].into(),
        vertex_remap: [
            ("r.x".to_owned(), "r.z".to_owned()),
            ("r.y".to_owned(), "r.z".to_owned()),
        ]
        .into(),
        edge_remap: [
            (SEdge::new("r", "r.x", "prop", Some("x")), merged.clone()),
            (SEdge::new("r", "r.y", "prop", Some("y")), merged),
        ]
        .into(),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    let mut i = Table::default();
    i.rows.insert(
        0,
        row(
            "r",
            &[("x", Value::LabeledNull(0)), ("y", Value::LabeledNull(1))],
        ),
    );
    i.rows.insert(
        1,
        row("r", &[("x", Value::LabeledNull(2)), ("y", Value::Int(4))]),
    );
    let (out, prov) = rel::sigma(&f, &i).unwrap();
    assert_eq!(out.rows[&0].1["z"], Value::LabeledNull(0));
    assert_eq!(out.rows[&1].1["z"], Value::Int(4));
    assert_eq!(
        prov.merged,
        BTreeMap::from([(1, Value::LabeledNull(0)), (2, Value::Int(4))])
    );
}

#[test]
fn pi_refuses_a_target_column_outside_the_image() {
    let s = Schema::default().vertex("a", "record");
    let t = Schema::default()
        .vertex("a", "record")
        .vertex("a.z", "int")
        .edge("a", "a.z", "prop", Some("z"));
    let m = Migration {
        surviving_verts: ["a".to_owned()].into(),
        ..Migration::default()
    };
    let f = functor(&s, &t, &m);
    assert_eq!(
        rel::pi(&f, &Table::default(), usize::MAX),
        Err(KanError::AttributeOutsideImage {
            table: "a".into(),
            column: "z".into()
        })
    );
}
