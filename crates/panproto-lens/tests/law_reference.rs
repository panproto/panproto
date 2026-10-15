//! The PutGet gate compares a user-edited view under the index of the
//! complement it is put against.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::{CompiledMigration, EdgeId, PutDomainError, SynthIndex, SynthKey};
use panproto_lens::laws::{LawShape, Mutation, MutationCases, SmallScope, check_put_get_for_shape};
use panproto_lens::{DerivedMap, Lens, LensShape};
use panproto_schema::Schema;

struct IndexProbeShape;

impl LensShape for IndexProbeShape {
    type Instance = u8;
    type Complement = u8;
    type Context = ();

    fn get(
        _lens: &Lens,
        source: &Self::Instance,
        _ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), panproto_lens::LensError> {
        Ok(match source {
            10 => (3, 1),
            12 => (3, 2),
            source => (*source, 0),
        })
    }

    fn put(
        _lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<Self::Instance, panproto_lens::LensError> {
        Ok(match (*view, *complement) {
            (1, 0) | (3, 1) => 10,
            (1 | 3, 2) => 12,
            (view, _) => view,
        })
    }

    fn check_put_domain(
        _lens: &Lens,
        _view: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        Ok(())
    }

    fn synth_index(_instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        let mut index = SynthIndex::default();
        index.nodes.insert(
            u32::from(*complement),
            SynthKey::Expand {
                source_edge: EdgeId(0),
                hop: 0,
            },
        );
        index
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        if a % 2 != b % 2 {
            return false;
        }
        match (*a, *b) {
            (1, 3) => ia.nodes.contains_key(&0) && ib.nodes.contains_key(&1),
            (3, 1) => ia.nodes.contains_key(&1) && ib.nodes.contains_key(&0),
            _ => true,
        }
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        (!Self::equivalent(a, ia, b, ib)).then(|| format!("{a} and {b} have different parity"))
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
        (a != b).then(|| format!("{a} != {b}"))
    }

    fn congruence_witnesses(
        _instance: &Self::Instance,
        _index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        Vec::new()
    }
}

impl LawShape for IndexProbeShape {
    fn mutation_cases(
        _lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        if mutation == Mutation::ValueEdit && *view == 0 {
            MutationCases::Cases(vec![1])
        } else {
            MutationCases::Skip(panproto_lens::SkipReason::NoCandidate)
        }
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        a == b
    }

    fn congruent_states(
        _view: &Self::Instance,
        _complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        Vec::new()
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement {
        if from == to { *complement } else { 2 }
    }

    fn locality_divergence(
        _lens: &Lens,
        _source: &Self::Instance,
        _original_view: &Self::Instance,
        _edited_view: &Self::Instance,
        _restored: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Option<String> {
        None
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        (*a).min(*b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        (original != edited).then_some(*original)
    }

    fn enumerate_small(
        _schema: &Schema,
        _scope: SmallScope,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        Vec::new()
    }
}

#[test]
fn a_user_edited_view_is_compared_under_its_put_complement_index() {
    let lens = Lens {
        compiled: CompiledMigration::default(),
        src_schema: Schema::default(),
        tgt_schema: Schema::default(),
    };
    assert!(check_put_get_for_shape::<IndexProbeShape>(&lens, &0, &()).is_ok());

    let candidate = 1;
    let put_complement = 0;
    let once =
        IndexProbeShape::put(&lens, &candidate, &put_complement, &()).expect("candidate put");
    let (actual, actual_complement) =
        IndexProbeShape::get(&lens, &once, &()).expect("candidate re-get");
    let candidate_index = IndexProbeShape::synth_index(&candidate, &put_complement);
    let actual_index = IndexProbeShape::synth_index(&actual, &actual_complement);
    assert!(IndexProbeShape::equivalent(
        &candidate,
        &candidate_index,
        &actual,
        &actual_index,
    ));
    let unrelated_index = IndexProbeShape::synth_index(&candidate, &2);
    assert!(!IndexProbeShape::equivalent(
        &candidate,
        &unrelated_index,
        &actual,
        &actual_index,
    ));
}
