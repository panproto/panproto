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

    fn synth_index(_complement: &Self::Complement) -> SynthIndex {
        SynthIndex::default()
    }

    fn equivalent(
        a: &Self::Instance,
        _ia: &SynthIndex,
        b: &Self::Instance,
        _ib: &SynthIndex,
    ) -> bool {
        a == b
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
