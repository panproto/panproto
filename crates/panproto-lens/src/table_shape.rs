//! Generic lens-law execution for table-shaped instances.

use panproto_inst::{BoundaryProtocols, FInstance, PutDomainError, SynthIndex, TableComplement};
use panproto_schema::Protocol;

use crate::derived::DerivedMap;
use crate::lens_shape::LensShape;
use crate::{Lens, LensError};

/// The protocols that classify the source and target schemas as tables.
#[derive(Clone, Debug, Default)]
pub struct TableContext {
    /// Source protocol.
    pub src: Protocol,
    /// Target protocol.
    pub tgt: Protocol,
}

impl TableContext {
    const fn protocols(&self) -> BoundaryProtocols<'_> {
        BoundaryProtocols {
            src: &self.src,
            tgt: &self.tgt,
        }
    }
}

/// Table-shaped lens execution.
#[derive(Clone, Copy, Debug, Default)]
pub struct TableShape;

impl LensShape for TableShape {
    type Instance = FInstance;
    type Complement = TableComplement;
    type Context = TableContext;

    fn get(
        lens: &Lens,
        source: &Self::Instance,
        ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), LensError> {
        crate::table_lens::get(lens, source, ctx.protocols())
    }

    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<Self::Instance, LensError> {
        crate::table_lens::put(lens, view, complement, ctx.protocols())
    }

    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        crate::table_lens::check_put_domain(lens, view, complement, ctx.protocols())
    }

    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        panproto_inst::GInstance::from_finstance(instance).map_or_else(
            |_| SynthIndex::default(),
            |(_, embedding)| complement.synth_index(&embedding),
        )
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        let Ok((a_graph, _)) = panproto_inst::GInstance::from_finstance(a) else {
            return false;
        };
        let Ok((b_graph, _)) = panproto_inst::GInstance::from_finstance(b) else {
            return false;
        };
        crate::lens_shape::GraphShape::equivalent(&a_graph, ia, &b_graph, ib)
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        (!Self::equivalent(a, ia, b, ib)).then(|| "table instances differ".to_owned())
    }

    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        _derived: &DerivedMap,
    ) -> Option<String> {
        Self::divergence(a, ia, b, ib)
    }

    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
        if a.id_floor != b.id_floor {
            return Some("id_floor differs".to_owned());
        }
        if a.dropped_rows.len() != b.dropped_rows.len()
            || !a.dropped_rows.iter().all(|(id, row)| {
                b.dropped_rows.get(id).is_some_and(|other| {
                    row.table == other.table
                        && crate::table_lens::rows_equivalent_for_shape(&row.row, &other.row)
                })
            })
        {
            return Some("dropped_rows differ".to_owned());
        }
        if !crate::table_lens::value_maps_equivalent_for_shape(&a.dropped_cells, &b.dropped_cells) {
            return Some("dropped_cells differ".to_owned());
        }
        if a.dropped_fk_pairs != b.dropped_fk_pairs {
            return Some("dropped_fk_pairs differ".to_owned());
        }
        if a.original_fk_edges != b.original_fk_edges {
            return Some("original_fk_edges differ".to_owned());
        }
        if a.original_tables != b.original_tables {
            return Some("original_tables differ".to_owned());
        }
        if !crate::table_lens::value_maps_equivalent_for_shape(&a.original_cells, &b.original_cells)
        {
            return Some("original_cells differ".to_owned());
        }
        (a.synthesized_fk_pairs != b.synthesized_fk_pairs)
            .then(|| "synthesized_fk_pairs differ".to_owned())
    }

    fn congruence_witnesses(
        _instance: &Self::Instance,
        _index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_inst::{EdgeId, FInstance, RowId, SynthKey, TableComplement};
    use panproto_schema::Edge;

    use super::TableShape;
    use crate::lens_shape::LensShape;

    fn foreign_key() -> Edge {
        Edge {
            src: "author".into(),
            tgt: "book".into(),
            kind: "fk".into(),
            name: Some("wrote".into()),
        }
    }

    #[test]
    fn table_synth_index_names_a_synthesized_pair_in_its_view_embedding()
    -> Result<(), panproto_inst::InstError> {
        let edge = foreign_key();
        let mut view = FInstance::new();
        view.insert_row("author", RowId(4), HashMap::new())?;
        view.insert_row("book", RowId(9), HashMap::new())?;
        view.insert_foreign_key(edge.clone(), RowId(4), RowId(9))?;
        let mut complement = TableComplement::empty();
        complement.synthesized_fk_pairs = BTreeSet::from([(edge.clone(), RowId(4), RowId(9))]);

        let index = TableShape::synth_index(&view, &complement);
        assert_eq!(
            index.edges.get(&EdgeId(0)),
            Some(&SynthKey::Contract {
                src: 4,
                tgt: 9,
                edge,
            })
        );
        Ok(())
    }

    #[test]
    fn table_equivalence_rejects_noninjective_synthesized_pair_indices_symmetrically()
    -> Result<(), panproto_inst::InstError> {
        let edge = foreign_key();
        let mut table = FInstance::new();
        table.insert_row("author", RowId(4), HashMap::new())?;
        table.insert_row("author", RowId(5), HashMap::new())?;
        table.insert_row("book", RowId(9), HashMap::new())?;
        table.insert_row("book", RowId(10), HashMap::new())?;
        table.insert_foreign_key(edge.clone(), RowId(4), RowId(9))?;
        table.insert_foreign_key(edge.clone(), RowId(5), RowId(10))?;
        let key = SynthKey::Contract {
            src: 4,
            tgt: 9,
            edge,
        };
        let duplicated = panproto_inst::SynthIndex {
            nodes: BTreeMap::new(),
            edges: BTreeMap::from([(EdgeId(0), key.clone()), (EdgeId(1), key.clone())]),
        };
        let coherent = panproto_inst::SynthIndex {
            nodes: BTreeMap::new(),
            edges: BTreeMap::from([(EdgeId(0), key)]),
        };

        assert!(!TableShape::equivalent(
            &table,
            &duplicated,
            &table,
            &coherent,
        ));
        assert!(!TableShape::equivalent(
            &table,
            &coherent,
            &table,
            &duplicated,
        ));
        Ok(())
    }
}
