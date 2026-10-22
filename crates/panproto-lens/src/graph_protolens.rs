//! Applicability-gated protolens execution for graph instances.

use panproto_inst::{GInstance, GraphComplement};
use panproto_schema::{Protocol, Schema};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{Protolens, ProtolensChain};

/// The concrete graph lens and identity-bound result of applying it.
#[derive(Debug)]
pub struct AppliedGraphProtolens {
    /// Instantiated concrete lens.
    pub lens: Lens,
    /// Projected graph view.
    pub view: GInstance,
    /// Complete graph complement, stamped with the concrete lens identity.
    pub complement: GraphComplement,
}

fn inapplicable(reasons: &[String]) -> LensError {
    LensError::ProtolensInapplicable {
        reasons: reasons.to_vec(),
    }
}

/// Check applicability, instantiate a protolens, and get its graph view.
///
/// # Errors
///
/// Returns a typed refusal before instantiation when the source precondition fails,
/// or an instantiation/execution error otherwise.
pub fn apply(
    protolens: &Protolens,
    schema: &Schema,
    protocol: &Protocol,
    source: &GInstance,
) -> Result<AppliedGraphProtolens, LensError> {
    let lens = protolens.instantiate_checked(schema, protocol)?;
    let (view, complement) = crate::graph_lens::get(&lens, source)?;
    Ok(AppliedGraphProtolens {
        lens,
        view,
        complement,
    })
}

/// Check every chain stage, instantiate the chain, and get its graph view.
///
/// # Errors
///
/// Returns a typed refusal before instantiation when any staged precondition fails,
/// or an instantiation/execution error otherwise.
pub fn apply_chain(
    chain: &ProtolensChain,
    schema: &Schema,
    protocol: &Protocol,
    source: &GInstance,
) -> Result<AppliedGraphProtolens, LensError> {
    chain
        .check_applicability_with(schema, protocol)
        .map_err(|reasons| inapplicable(&reasons))?;
    let lens = chain.instantiate(schema, protocol)?;
    let (view, complement) = crate::graph_lens::get(&lens, source)?;
    Ok(AppliedGraphProtolens {
        lens,
        view,
        complement,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::protolens::elementary;
    use panproto_inst::{ComplementShape, Node};
    use panproto_schema::SchemaBuilder;

    #[test]
    fn empty_chain_is_applicable_and_binds_the_complement() {
        let schema = Schema::default();
        let protocol = Protocol::default();
        let applied = apply_chain(
            &ProtolensChain::new(Vec::new()),
            &schema,
            &protocol,
            &GInstance::default(),
        )
        .unwrap();
        assert!(applied.complement.identity.is_some());
    }

    #[test]
    fn inapplicable_protolens_is_refused_before_instantiation() {
        let outcome = apply(
            &elementary::drop_sort("missing"),
            &Schema::default(),
            &Protocol::default(),
            &GInstance::default(),
        );
        assert!(matches!(
            outcome,
            Err(LensError::ProtolensInapplicable { .. })
        ));
    }

    #[test]
    fn a_nonidentity_protolens_applies_and_binds_its_exact_graph_envelope() {
        let protocol = Protocol {
            obj_kinds: vec!["record".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let source = GInstance::new().with_node(Node::new(7, "post"));
        let applied = apply(
            &elementary::rename_sort("record", "entry"),
            &schema,
            &protocol,
            &source,
        )
        .unwrap();

        assert_eq!(
            applied.view.node(7).map(|node| node.anchor.as_ref()),
            Some("post")
        );
        assert_eq!(
            applied.lens.tgt_schema.vertices["post"].kind.as_ref(),
            "entry"
        );
        let expected_identity = applied.lens.identity(ComplementShape::Graph);
        assert_eq!(
            applied.complement.identity.as_ref(),
            Some(&expected_identity)
        );
    }
}
