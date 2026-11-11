//! Applicability-gated protolens execution for table instances.

use panproto_inst::{BoundaryProtocols, FInstance, TableComplement};
use panproto_schema::Schema;

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{Protolens, ProtolensChain};

/// The concrete table lens and identity-bound result of applying it.
#[derive(Debug)]
pub struct AppliedTableProtolens {
    /// Instantiated concrete lens.
    pub lens: Lens,
    /// Projected table view.
    pub view: FInstance,
    /// Complete table complement, stamped with the concrete lens identity.
    pub complement: TableComplement,
}

fn inapplicable(reasons: &[String]) -> LensError {
    LensError::ProtolensInapplicable {
        reasons: reasons.to_vec(),
    }
}

/// Check applicability, instantiate a protolens, and get its table view.
///
/// # Errors
///
/// Returns a typed refusal before instantiation when the source precondition fails,
/// or an instantiation/execution error otherwise.
pub fn apply(
    protolens: &Protolens,
    schema: &Schema,
    protocols: BoundaryProtocols<'_>,
    source: &FInstance,
) -> Result<AppliedTableProtolens, LensError> {
    let lens = protolens.instantiate_checked(schema, protocols.src)?;
    let (view, complement) = crate::table_lens::get(&lens, source, protocols)?;
    Ok(AppliedTableProtolens {
        lens,
        view,
        complement,
    })
}

/// Check every chain stage, instantiate the chain, and get its table view.
///
/// # Errors
///
/// Returns a typed refusal before instantiation when any staged precondition fails,
/// or an instantiation/execution error otherwise.
pub fn apply_chain(
    chain: &ProtolensChain,
    schema: &Schema,
    protocols: BoundaryProtocols<'_>,
    source: &FInstance,
) -> Result<AppliedTableProtolens, LensError> {
    chain
        .check_applicability_with(schema, protocols.src)
        .map_err(|reasons| inapplicable(&reasons))?;
    let lens = chain.instantiate(schema, protocols.src)?;
    let (view, complement) = crate::table_lens::get(&lens, source, protocols)?;
    Ok(AppliedTableProtolens {
        lens,
        view,
        complement,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;

    use super::*;
    use crate::protolens::elementary;
    use panproto_inst::{ComplementShape, RowId};
    use panproto_schema::{Protocol, SchemaBuilder};

    #[test]
    fn empty_chain_is_applicable_and_binds_the_complement() {
        let schema = Schema::default();
        let protocol = Protocol::default();
        let applied = apply_chain(
            &ProtolensChain::new(Vec::new()),
            &schema,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
            &FInstance::default(),
        )
        .unwrap();
        assert!(applied.complement.identity.is_some());
    }

    #[test]
    fn inapplicable_protolens_is_refused_before_instantiation() {
        let protocol = Protocol::default();
        let outcome = apply(
            &elementary::drop_sort("missing"),
            &Schema::default(),
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
            &FInstance::default(),
        );
        assert!(matches!(
            outcome,
            Err(LensError::ProtolensInapplicable { .. })
        ));
    }

    #[test]
    fn a_nonidentity_protolens_applies_and_binds_its_exact_table_envelope() {
        let protocol = Protocol {
            non_value_kinds: ["record".to_owned(), "entry".to_owned()]
                .into_iter()
                .collect(),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let mut source = FInstance::new();
        source
            .insert_row("post", RowId(7), HashMap::default())
            .unwrap();
        let applied = apply(
            &elementary::rename_sort("record", "entry"),
            &schema,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
            &source,
        )
        .unwrap();

        assert_eq!(
            applied.view.table_of(RowId(7)).map(AsRef::as_ref),
            Some("post")
        );
        assert_eq!(
            applied.lens.tgt_schema.vertices["post"].kind.as_ref(),
            "entry"
        );
        let expected_identity = applied.lens.identity(ComplementShape::Functor);
        assert_eq!(
            applied.complement.identity.as_ref(),
            Some(&expected_identity)
        );
    }
}
