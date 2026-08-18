//! Every numeric default the evidence pipeline introduces, in one file.
//!
//! An audit of "where did this number come from" should be one file long for
//! the aggregation and selection design, so every constant that design
//! introduces lives here and carries the same doc shape: what it is, that it is
//! a **principled default rather than a calibrated value**, which published
//! source it comes from, and why it must not be tuned against the synthetic
//! autolens corpus. `every_default_is_documented` checks that shape against the
//! text of this file, so a constant declared around the macro fails rather than
//! escaping the discipline.
//!
//! The solver's resource limits are the deliberate exception. They are engineering
//! ceilings on what one search may spend rather than parameters of the
//! objective, no assignment's cost depends on them, and they live with the code
//! that spends against them:
//! [`DEFAULT_MEM_BYTES`](crate::solve::DEFAULT_MEM_BYTES),
//! [`DEFAULT_OP_BUDGET`](crate::solve::DEFAULT_OP_BUDGET) and
//! [`DEFAULT_SEARCH_NODES`](crate::solve::DEFAULT_SEARCH_NODES). Each carries
//! the same calibration marker as the constants below, and the table names them
//! so that an audit starting here finds them.
//!
//! # Calibration
//!
//! panproto has no labelled schema-matching corpus. Nothing below has been
//! fitted to panproto data, and the honest summary of what each number is
//! backed by is this:
//!
//! | Constant | Backed by | Not backed by |
//! |---|---|---|
//! | [`EVIDENCE_FLOOR`] | COMA's measured selection default, `Threshold(0.5)` (Do and Rahm, VLDB 2002, best of 36 configurations over 8208 series) | any measurement on panproto schemas |
//! | [`EVIDENCE_DELTA`] | the same measurement, `Delta(0.02)`, the only scale-free primitive of the three COMA compared | any measurement on panproto schemas |
//! | [`HYBRID_HIGH_CONFIDENCE`] | the sole numeric constant in `AgreementMakerLight`'s `Selector.java` and `CardinalitySelector.java` | any published table; AML's own papers never justify it |
//! | [`HYBRID_CARDINALITY`] | AML's `Selector`, whose hybrid branch relaxes the bound by exactly one | any measurement |
//! | [`CEILING_EXACT_IDENTIFIER`] and the five ceilings below it | AML's name-provenance weight table, inherited by Matcha, whose *ordering* is a fact about the input format's own semantics | the absolute values, which are conventional |
//! | [`CEILING_USER_SUPPLIED`] | the definition of a hint: a caller who states a correspondence is the most authoritative source available | any measurement |
//! | [`W_ANCHOR`] | nothing. It ships at zero so the rewrite is observably neutral | any measurement |
//! | [`DEFAULT_WEIGHTS`] | nothing. Unchanged from the weights the quality score has always used | any measurement, then or now |
//! | [`DEFAULT_MEM_BYTES`](crate::solve::DEFAULT_MEM_BYTES) | a machine limit, not a model parameter: the working set exact inference is allowed to allocate | any measurement of what schemas need |
//! | [`DEFAULT_OP_BUDGET`](crate::solve::DEFAULT_OP_BUDGET) | the same, counted in combine operations rather than bytes | any measurement |
//! | [`DEFAULT_SEARCH_NODES`](crate::solve::DEFAULT_SEARCH_NODES) | the same, counted in nodes opened by a search path | any measurement |
//!
//! The strategy priority table in [`StrategyTag::priority`] is uncalibrated in
//! exactly the same way and has never been validated against labelled data.
//! It lives next to the enum it orders rather than here because it is a total
//! order over variants rather than a numeric threshold, but it belongs in this
//! list.
//!
//! [`StrategyTag::priority`]: super::StrategyTag::priority

use crate::solve::cost::CostWeights;

/// Declare a public constant and record its doc comment for
/// [`tests::every_default_is_documented`].
///
/// The registry is what makes the calibration discipline checkable rather than
/// aspirational: a constant added to this file through the macro is checked,
/// and a constant added around the macro fails the name-set assertion in the
/// same test.
macro_rules! documented_defaults {
    (
        $(
            $(#[doc = $doc:expr])+
            pub const $name:ident: $ty:ty = $value:expr;
        )+
    ) => {
        $(
            $(#[doc = $doc])+
            pub const $name: $ty = $value;
        )+

        /// Every constant declared through the macro, paired with the lines of
        /// its doc comment.
        #[cfg(test)]
        const REGISTRY: &[(&str, &[&str])] = &[
            $( (stringify!($name), &[$($doc),+]) ),+
        ];
    };
}

documented_defaults! {
    /// Absolute evidence floor below which a candidate is not reported in a
    /// [`Selection`](super::evidence::Selection).
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. panproto has no labelled schema-matching corpus, so the number
    /// comes from published practice (COMA, Do and Rahm, VLDB 2002, where
    /// `Threshold(0.5) + Delta(0.02)` was the best of 36 selection
    /// configurations measured over 8208 series) rather than from a fit to
    /// panproto data. It is a starting point to be replaced once a labelled
    /// corpus exists.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    ///
    /// # Scale
    ///
    /// COMA's 0.5 is a floor on an aggregated *similarity*, which is on the
    /// same scale as a single matcher's output.
    /// [`Evidence::score`](super::evidence::Evidence::score) is a fixed arity
    /// mean over [`FAMILIES`](super::evidence::FAMILIES), so a candidate
    /// supported by `k` of the six families cannot exceed `k / 6`. The two
    /// scales are not the same, and a candidate carried by one family alone
    /// therefore never clears this floor. That is the conservative reading and
    /// the one [`RowFilter::default`](super::evidence::RowFilter::default)
    /// takes; [`RowFilter::relative_only`](super::evidence::RowFilter::relative_only)
    /// keeps COMA's actual decision rule, the relative delta, without the
    /// absolute cut.
    pub const EVIDENCE_FLOOR: f64 = 0.5;

    /// Relative tolerance around the best candidate for a source vertex.
    ///
    /// Every candidate scoring at least `(1 - EVIDENCE_DELTA)` times the best
    /// score for its source survives the row filter, so the decision is made
    /// on the *gap* between the best and the runner up rather than on an
    /// absolute cut. That is what makes it scale free, and therefore what
    /// makes it survive miscalibration.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. It is COMA's `Delta(0.02)` (Do and Rahm, VLDB 2002), which
    /// measured beyond 0.7 average Overall while the best `Threshold` variant
    /// stayed below 0.3, and it has not been fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const EVIDENCE_DELTA: f64 = 0.02;

    /// Score above which
    /// [`Cardinality::Hybrid`](super::evidence::Cardinality::Hybrid) relaxes
    /// its cardinality bound.
    ///
    /// It is not a filter. Nothing is deleted by crossing it; what changes is
    /// the constraint, from "one target per source" to
    /// "[`HYBRID_CARDINALITY`] targets per source". It encodes a two regime
    /// model of matcher reliability: above the line two competing claims are
    /// more likely to be a genuine one-to-many relation than a mistake, below
    /// it the score cannot tell those cases apart.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. It is the sole numeric constant in `AgreementMakerLight`'s
    /// selection logic, hard coded as `0.75` in both `Selector.java` and
    /// `CardinalitySelector.java`. Secondary summaries of the AML system
    /// paper report `0.7`; the code is the citable source and it says `0.75`.
    /// No published table justifies either number.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const HYBRID_HIGH_CONFIDENCE: f64 = 0.75;

    /// How many targets per source, and sources per target,
    /// [`Cardinality::Hybrid`](super::evidence::Cardinality::Hybrid) admits
    /// above [`HYBRID_HIGH_CONFIDENCE`].
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. AML's `Selector` is `CardinalitySelector(1)` with the hybrid
    /// branch testing `<=` where the strict branch tests `<`, so the high
    /// confidence regime relaxes the bound by exactly one. Nothing has been
    /// fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const HYBRID_CARDINALITY: u8 = 1;

    /// Confidence ceiling for
    /// [`Provenance::ExactIdentifier`](super::evidence::Provenance::ExactIdentifier).
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. The ceilings generalise `AgreementMakerLight`'s
    /// name-provenance weights (local name 1.0, label 0.95, exact synonym 0.9,
    /// related synonym 0.85), inherited by Matcha, which weighs a match by
    /// how the name was declared *before* any string metric runs. The
    /// *ordering* is a fact about the input format's own semantics and
    /// transfers across corpora; the absolute values are conventional and
    /// have not been fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_EXACT_IDENTIFIER: f64 = 1.00;

    /// Confidence ceiling for
    /// [`Provenance::DeclaredLabel`](super::evidence::Provenance::DeclaredLabel).
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. It is AML's name-provenance weight for a declared primary label, one
    /// step below an identical canonical identifier. The ordering is a fact
    /// about the input format; the value is conventional and has not been
    /// fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_DECLARED_LABEL: f64 = 0.95;

    /// Confidence ceiling for
    /// [`Provenance::DeclaredEdgeLabel`](super::evidence::Provenance::DeclaredEdgeLabel).
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. It is AML's name-provenance weight for a declared exact synonym,
    /// applied here to the label an edge carries rather than the identifier a
    /// vertex carries: an edge label is declared, but it names a field of a
    /// container rather than the thing itself. The ordering is a fact about
    /// the input format; the value is conventional and has not been fitted to
    /// panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_DECLARED_EDGE_LABEL: f64 = 0.90;

    /// Confidence ceiling for
    /// [`Provenance::Synonym`](super::evidence::Provenance::Synonym).
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. It is AML's name-provenance weight for a declared related synonym or
    /// cross reference, which is what a dictionary alias between two
    /// identifiers is. The ordering is a fact about the input format; the
    /// value is conventional and has not been fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_SYNONYM: f64 = 0.85;

    /// Confidence ceiling for
    /// [`Provenance::Derived`](super::evidence::Provenance::Derived).
    ///
    /// Derived evidence is read from a *transformation* of a declared string:
    /// tokenisation, stemming, abbreviation expansion, prefix splitting. The
    /// string is declared, the comparison is not.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. AML's published weight table stops at related synonyms, so
    /// this extends its ordering by one step rather than quoting it. Nothing
    /// has been fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_DERIVED: f64 = 0.80;

    /// Confidence ceiling for
    /// [`Provenance::Inferred`](super::evidence::Provenance::Inferred).
    ///
    /// Inferred evidence reads no declared correspondence at all: structural
    /// position, degree, colour refinement, a coercion witness between
    /// carriers. It is the weakest provenance the pipeline admits.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value. AML's published weight table stops at related synonyms, so
    /// this extends its ordering by two steps rather than quoting it. Nothing
    /// has been fitted to panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_INFERRED: f64 = 0.75;

    /// Confidence ceiling for
    /// [`Provenance::UserSupplied`](super::evidence::Provenance::UserSupplied).
    ///
    /// A caller who states a correspondence is the most authoritative source
    /// the pipeline has, so the ceiling does not cut a user hint down. The
    /// hint is still **soft**: it becomes a cost reduction, never a domain
    /// restriction. A caller who means "this is fixed" uses
    /// [`SearchOptions`](crate::SearchOptions) rather than an anchor.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value, and it is the one ceiling that follows from a definition rather
    /// than from AML's weight table. Nothing has been fitted to panproto
    /// data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const CEILING_USER_SUPPLIED: f64 = 1.00;

    /// The weight the search objective puts on anchor evidence.
    ///
    /// Evidence enters the objective through the single term
    /// `W_ANCHOR * (1 - ev(v, a)) / |V_s|` and through nothing else, so at
    /// zero the term is the same constant on every value of every variable
    /// and cannot change which assignment is optimal.
    ///
    /// **It ships at zero on purpose.** The evidence rewrite changes how
    /// anchors are aggregated, which anchors survive, and what a caller sees;
    /// shipping it with a non-zero weight would change the search result at
    /// the same time, and there would be no way to tell which change moved a
    /// ranking. Raising it is a separate change that carries a corpus
    /// selection diff.
    ///
    /// **calibration:** none. This is a principled default, not a calibrated
    /// value, and unlike the rest of this file it is not even a published
    /// one: no source suggests how heavily aggregated matcher evidence should
    /// count against structural agreement. Nothing has been fitted to
    /// panproto data.
    ///
    /// Do not tune this against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const W_ANCHOR: f64 = 0.0;

    /// The component weights the search objective uses.
    ///
    /// Restated here, rather than only in
    /// [`solve::cost`](crate::solve::cost), because the calibration status of
    /// the four quality weights is the same as that of every other number in
    /// this file and an audit should not have to visit two modules to
    /// establish it. The anchor component is [`W_ANCHOR`].
    ///
    /// **calibration:** none. Each is a principled default, not a calibrated
    /// value. `[0.25, 0.25, 0.30, 0.20]` are the weights the quality score
    /// has always used, kept unchanged so that any behaviour change is
    /// attributable to the structural rewrite rather than to a reweighting.
    /// They were never fitted to anything.
    ///
    /// Do not tune these against `crates/panproto-lens/tests/autolens_corpus.rs`:
    /// that corpus is synthetic and its expectations were themselves derived
    /// from engine behaviour, so fitting to it is circular.
    pub const DEFAULT_WEIGHTS: CostWeights = crate::solve::cost::DEFAULT_WEIGHTS;
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    /// Every `pub const` this file declares, read out of the source text.
    ///
    /// The registry cannot see a constant declared around the macro, so
    /// comparing the registry against itself proves nothing. This reads the
    /// file instead, which is the only view that contains both the constants
    /// the macro produced and the ones it did not.
    ///
    /// The macro's own rule line declares `pub const $name`, whose name is not
    /// an identifier in upper snake case, so the pattern skips it without
    /// needing to know where the macro body ends.
    fn declared_constants() -> Vec<String> {
        include_str!("defaults.rs")
            .lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix("pub const ")?;
                let name = rest.split(':').next()?.trim();
                let named_well = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
                named_well.then(|| name.to_owned())
            })
            .collect()
    }

    /// The calibration discipline, enforced.
    ///
    /// Every constant reached through the macro must say what it is backed by
    /// (`calibration:`), must say that it is not a fit (`not a calibrated
    /// value`), and must warn against tuning it against the synthetic corpus
    /// (`autolens_corpus`).
    ///
    /// The registry is then checked against the file's own text rather than
    /// against a second copy of itself, which is what makes the discipline
    /// checkable: a constant declared around the macro appears in the source
    /// scan and not in the registry, and fails here rather than passing
    /// silently.
    #[test]
    fn every_default_is_documented() {
        assert!(!REGISTRY.is_empty(), "the registry lost its entries");

        for (name, doc) in REGISTRY {
            // A doc comment is one attribute per line, and a sentence runs
            // across several of them, so the markers are only visible once the
            // lines are rejoined and the wrapping is squeezed out.
            let text = doc
                .iter()
                .flat_map(|line| line.split_whitespace())
                .collect::<Vec<&str>>()
                .join(" ");
            assert!(
                text.contains("calibration:"),
                "`{name}` has no `calibration:` marker"
            );
            assert!(
                text.contains("not a calibrated value"),
                "`{name}` does not say it is not a calibrated value"
            );
            assert!(
                text.contains("autolens_corpus"),
                "`{name}` does not warn against tuning on the synthetic corpus"
            );
        }

        let names: Vec<&str> = REGISTRY.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "EVIDENCE_FLOOR",
                "EVIDENCE_DELTA",
                "HYBRID_HIGH_CONFIDENCE",
                "HYBRID_CARDINALITY",
                "CEILING_EXACT_IDENTIFIER",
                "CEILING_DECLARED_LABEL",
                "CEILING_DECLARED_EDGE_LABEL",
                "CEILING_SYNONYM",
                "CEILING_DERIVED",
                "CEILING_INFERRED",
                "CEILING_USER_SUPPLIED",
                "W_ANCHOR",
                "DEFAULT_WEIGHTS",
            ],
            "a constant was added or removed without updating this list"
        );

        let declared = declared_constants();
        assert!(
            !declared.is_empty(),
            "the source scan found nothing, so it is not reading this file"
        );
        assert_eq!(
            declared, names,
            "a `pub const` in this file is not in the registry, so it was \
             declared around the macro and carries no calibration doc"
        );
    }

    /// The source scan is the load-bearing half of
    /// [`every_default_is_documented`], so it is checked against a line the
    /// macro would not produce.
    ///
    /// Without this, a scan that silently matched nothing, or that matched the
    /// macro's own rule line, would leave the guard passing for the wrong
    /// reason.
    #[test]
    fn the_source_scan_sees_a_constant_declared_around_the_macro() {
        let smuggled = "pub const SMUGGLED_KNOB: f64 = 0.37;";
        let name = smuggled
            .trim_start()
            .strip_prefix("pub const ")
            .and_then(|rest| rest.split(':').next())
            .map(str::trim);
        assert_eq!(name, Some("SMUGGLED_KNOB"));

        // And the macro's own rule line, which declares `pub const $name`, is
        // skipped rather than reported as a constant.
        assert!(!declared_constants().iter().any(|n| n.contains('$')));
        assert!(!declared_constants().iter().any(|n| n == "SMUGGLED_KNOB"));
    }

    /// The ceiling ordering is the load-bearing part of the provenance table.
    /// The absolute values are conventional; that an identifier outranks a
    /// label, a label an edge label, and so on down to an inference, is not.
    #[test]
    fn provenance_ceilings_are_ordered() {
        let descending = [
            CEILING_EXACT_IDENTIFIER,
            CEILING_DECLARED_LABEL,
            CEILING_DECLARED_EDGE_LABEL,
            CEILING_SYNONYM,
            CEILING_DERIVED,
            CEILING_INFERRED,
        ];
        for pair in descending.windows(2) {
            assert!(pair[0] > pair[1], "{} !> {}", pair[0], pair[1]);
        }
        assert_eq!(CEILING_USER_SUPPLIED, CEILING_EXACT_IDENTIFIER);
        for ceiling in descending {
            assert!((0.0..=1.0).contains(&ceiling));
        }
    }

    /// The anchor weight ships at zero, and the restated weight vector agrees
    /// with the one the objective actually reads.
    #[test]
    fn anchor_weight_ships_neutral() {
        assert_eq!(W_ANCHOR, 0.0);
        assert_eq!(DEFAULT_WEIGHTS.anchor(), W_ANCHOR);
        assert_eq!(DEFAULT_WEIGHTS, crate::solve::cost::DEFAULT_WEIGHTS);
        assert_eq!(DEFAULT_WEIGHTS.as_array(), [0.25, 0.25, 0.30, 0.20]);
    }

    /// The selection defaults are in range for what they gate.
    #[test]
    fn selection_defaults_are_in_range() {
        assert!((0.0..=1.0).contains(&EVIDENCE_FLOOR));
        assert!((0.0..=1.0).contains(&EVIDENCE_DELTA));
        assert!((0.0..=1.0).contains(&HYBRID_HIGH_CONFIDENCE));
        const { assert!(HYBRID_CARDINALITY >= 1) };
    }
}
