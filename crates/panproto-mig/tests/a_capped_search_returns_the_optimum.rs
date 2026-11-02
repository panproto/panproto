//! A search capped at one result returns the optimum, and a lift along it
//! either round-trips the record or refuses it with a typed error.
//!
//! The case is an `ATProto` profile record `{handle, bio}` migrated to a
//! profile lexicon. When the target declares `handle`, `bio` and
//! `avatarUrl`, the optimum maps each source field to its namesake, the
//! capped search agrees with [`find_best_morphism`], the answer is the same
//! on every freshly built pair of schemas, and the record survives the
//! lift and the JSON writer unchanged. When the target has no `bio`, the
//! optimum sends both `handle` and `bio` to the target's `handle`, a
//! single-valued edge, and the lift refuses rather than putting two values
//! on it.

use std::error::Error;

use panproto_gat::Name;
use panproto_inst::{BoundaryProtocols, RestrictError, parse_json, to_json};
use panproto_mig::hom_search::morphism_to_migration;
use panproto_mig::{
    FoundMorphism, LiftError, SearchOptions, compile, find_best_morphism, find_morphisms,
    lift_wtype,
};
use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const NSID_SRC: &str = "test.profile.src";
const NSID_TGT: &str = "test.profile.tgt";

fn lexicon(nsid: &str, fields: &[&str]) -> TestResult<Schema> {
    let properties: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .map(|f| ((*f).to_owned(), serde_json::json!({"type": "string"})))
        .collect();
    Ok(atproto::parse_lexicon(&serde_json::json!({
        "lexicon": 1,
        "id": nsid,
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "required": ["handle"],
                    "properties": properties,
                }
            }
        }
    }))?)
}

fn options(max_results: usize) -> SearchOptions {
    SearchOptions {
        max_results,
        ..SearchOptions::default()
    }
}

/// The target edge a source field edge is sent to, by the field's name.
fn image_of_field(found: &FoundMorphism, field: &str) -> Option<String> {
    found
        .edge_map
        .iter()
        .find(|(src, _)| src.name.as_deref() == Some(field))
        .and_then(|(_, tgt)| tgt.name.as_deref().map(str::to_owned))
}

fn record() -> serde_json::Value {
    serde_json::json!({"handle": "alice.test", "bio": "hi"})
}

/// Compile the migration of `found`, parse the record and lift it: the
/// lift's own outcome, inside the outcome of the steps before it.
fn lift_record(
    src: &Schema,
    tgt: &Schema,
    found: &FoundMorphism,
) -> TestResult<Result<serde_json::Value, LiftError>> {
    let protocol = atproto::protocol();
    let migration = morphism_to_migration(found);
    let compiled = compile(src, tgt, &migration)?;
    let instance = parse_json(src, &protocol, NSID_SRC, &record())?;
    let lifted = lift_wtype(
        &compiled,
        src,
        tgt,
        &instance,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    );
    Ok(lifted.map(|view| to_json(tgt, &view)))
}

#[test]
fn the_capped_search_maps_each_field_to_its_namesake_and_round_trips() -> TestResult {
    let mut answers = Vec::new();
    for _ in 0..8 {
        let src = lexicon(NSID_SRC, &["handle", "bio"])?;
        let tgt = lexicon(NSID_TGT, &["handle", "bio", "avatarUrl"])?;
        let capped = find_morphisms(&src, &tgt, &options(1))?;
        let best = find_best_morphism(&src, &tgt, &options(0))?.ok_or("no total morphism")?;
        assert_eq!(capped.morphisms.len(), 1);
        let first = capped
            .morphisms
            .first()
            .ok_or("the capped search is empty")?;
        assert_eq!(first.vertex_map, best.vertex_map);
        assert_eq!(first.edge_map, best.edge_map);
        assert_eq!(image_of_field(first, "handle").as_deref(), Some("handle"));
        assert_eq!(image_of_field(first, "bio").as_deref(), Some("bio"));
        let lifted = lift_record(&src, &tgt, first)??;
        assert_eq!(lifted, record());
        let mut vertices: Vec<(Name, Name)> = first
            .vertex_map
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect();
        vertices.sort();
        let mut edges: Vec<(Option<String>, Option<String>)> = first
            .edge_map
            .iter()
            .map(|(a, b)| {
                (
                    a.name.as_deref().map(str::to_owned),
                    b.name.as_deref().map(str::to_owned),
                )
            })
            .collect();
        edges.sort();
        answers.push((vertices, edges));
    }
    assert!(
        answers.windows(2).all(|w| w[0] == w[1]),
        "the optimum differs across freshly built schemas: {answers:?}"
    );
    Ok(())
}

#[test]
fn two_fields_on_one_single_valued_edge_are_refused() -> TestResult {
    let src = lexicon(NSID_SRC, &["handle", "bio"])?;
    let tgt = lexicon(NSID_TGT, &["handle", "avatarUrl"])?;
    let best = find_best_morphism(&src, &tgt, &options(0))?.ok_or("no total morphism")?;
    assert_eq!(image_of_field(&best, "handle").as_deref(), Some("handle"));
    assert_eq!(image_of_field(&best, "bio").as_deref(), Some("handle"));
    let Err(err) = lift_record(&src, &tgt, &best)? else {
        return Err("the lift accepted two values for one field".into());
    };
    assert!(
        matches!(
            &err,
            LiftError::Restrict(RestrictError::MultipleArcsOnSingleValuedEdge { edge, count, .. })
                if edge.name.as_deref() == Some("handle") && *count == 2
        ),
        "{err:?}"
    );
    Ok(())
}
