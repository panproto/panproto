//! A strategy pin may break a tie among optima. It may not cost objective value.
//!
//! `auto_lens` merges its alignment strategies' proposals into
//! [`SearchOptions::hard_pins`], which collapses each pinned vertex's domain to
//! that one target and `⊥`. Releasing those pins therefore hands the search a
//! strictly larger feasible set, and the optimum over a superset is never worse.
//! The objective is the packed pair `(quality_cost, drops)` read
//! lexicographically, so "never worse" means higher quality, or equal quality
//! and fewer drops.
//!
//! `best_of_pinned_and_released` used to compare the two attempts on the drop
//! half alone. That kept the pinned answer on every pair where releasing raised
//! the quality without changing how many source vertices were mapped, and on
//! those pairs the pinned answer could not have been the better one. Measured
//! over the 5852 ordered pairs of the lexicon corpus at the balanced tier,
//! releasing raised the quality on 199 pairs and lowered it on none; on 66 of
//! those the coverage tied, so the strictly worse answer was returned.
//!
//! The pair below is the widest of those 66. Both attempts map nine of the ten
//! source vertices, so the drop counts are equal and nothing in the old
//! comparison could separate them, while the qualities are 0.3321 pinned
//! against 0.6572 released.
//!
//! A synthetic fixture was tried first and could not reproduce it: the
//! edge-label strategy is good enough that on a schema pair small enough to
//! write out by hand it proposes the free optimum, and the pins agree with the
//! search. The case needs schemas whose structure the strategies get partly
//! wrong, which is what the corpus supplies and a two-edge fixture does not.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_lens::auto_lens::{AutoLensConfig, Stringency, auto_generate};
use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_protocols::web_document::atproto::parse_lexicon;
use panproto_schema::Schema;

const PROTOLENS: &str = include_str!("../../../lexicons/dev/panproto/schema/protolens.json");
const INDUCTIVE: &str = include_str!("../../../lexicons/dev/panproto/schema/inductive.json");

fn schema(text: &str) -> Schema {
    let lexicon = serde_json::from_str(text).expect("a committed lexicon parses as JSON");
    parse_lexicon(&lexicon).expect("a committed lexicon parses as a schema")
}

#[test]
fn a_strategy_pin_never_costs_quality_the_released_search_would_have_had() {
    let protocol = panproto_protocols::atproto::protocol();
    let src = schema(PROTOLENS);
    let tgt = schema(INDUCTIVE);

    let released = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");

    let config = AutoLensConfig {
        stringency: Stringency::Lenient,
        ..AutoLensConfig::default()
    };
    let generated = auto_generate(&src, &tgt, &protocol, &config).expect("a lens");

    // The premise. If this stops holding the fixture has drifted and the
    // assertion below would pass for the wrong reason, so it is asserted
    // rather than assumed.
    assert!(
        released.quality > 0.5,
        "the released search should reach a quality the pinned one does not: {}",
        released.quality
    );

    assert!(
        generated.alignment_quality >= released.quality - 1e-9,
        "the strategy pins cost quality the released search would have had: \
         {} against {}",
        generated.alignment_quality,
        released.quality
    );
}
