//! Higher stringency tiers must align whatever lower tiers align.
//!
//! [`Stringency`] documents that its tiers form a superset ladder, and
//! the corpus in `panproto-lens` asserts that across its synthetic
//! cases. Those cases are small enough that the anchor set barely
//! changes between tiers, so they never exercised the way the ladder
//! actually broke.
//!
//! Two real ATProto lexicons do. `app.bsky.feed.post` against
//! `app.bsky.actor.profile` yields 26 resolved anchors at `Lenient` and
//! 57 at `Exploratory`, and the extra ones come from the strategies
//! only `Exploratory` runs. Since `align::resolve_anchors` keeps a
//! single winner per source vertex, a higher-confidence structural
//! anchor displaced one the lower tier depended on. While anchors were
//! pinned through `SearchOptions::initial`, which collapses a vertex's
//! domain to exactly the pinned target, that displacement left the CSP
//! unsatisfiable and `Exploratory` reported no morphism on a pair
//! `Lenient` aligned.

use panproto_core::lens::{self, AutoLensConfig, Stringency};
use panproto_core::protocols;
use panproto_core::schema::{Protocol, Schema};

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");

fn lexicon(source: &str) -> Schema {
    let json: serde_json::Value = serde_json::from_str(source).expect("lexicon parses as JSON");
    protocols::atproto::parse_lexicon(&json).expect("lexicon parses as a schema")
}

fn aligns_at(src: &Schema, tgt: &Schema, tier: Stringency) -> bool {
    let protocol = Protocol {
        name: "atproto".into(),
        ..Default::default()
    };
    let config = AutoLensConfig {
        stringency: tier,
        ..Default::default()
    };
    lens::auto_generate(src, tgt, &protocol, &config).is_ok()
}

/// Ignored by default because it costs roughly a minute in release and
/// eight in debug, which would dominate the suite.
///
/// The cost is the soft-anchor retry on this pair, and it is not the
/// number of vertex assignments: capping the retry at a single
/// assignment leaves the wall time unchanged. Whatever dominates has
/// not been identified, so the number stands as measured rather than
/// explained. Run it with:
///
/// ```text
/// cargo nextest run --release -p panproto-core -- --ignored
/// ```
#[test]
#[ignore = "roughly a minute in release; see the comment above"]
fn exploratory_aligns_whatever_lenient_aligns() {
    let post = lexicon(FEED_POST);
    let profile = lexicon(ACTOR_PROFILE);

    assert!(
        aligns_at(&post, &profile, Stringency::Lenient),
        "the fixture pair no longer aligns at Lenient, so this test has stopped \
         guarding anything; pick a pair that does"
    );
    assert!(
        aligns_at(&post, &profile, Stringency::Exploratory),
        "Exploratory found no morphism on a pair Lenient aligns. A tier-exclusive \
         alignment strategy has displaced an anchor the lower tier relied on, and \
         the displacement is reaching the solver as a pin rather than a preference"
    );
}
