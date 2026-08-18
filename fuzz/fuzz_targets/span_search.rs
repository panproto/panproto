#![no_main]
//! Fuzz target for the span search.
//!
//! Two well-formed schemas go in and a span comes out. What is checked is the
//! contract the module documents rather than the implementation:
//!
//! 1. The search never refuses. `find_span` is documented to return a span for
//!    every pair, because the all-`⊥` assignment is always feasible.
//! 2. Both legs are genuine schema morphisms, by
//!    [`check_migration_morphism`] rather than by inspection.
//! 3. The apex validates against the protocol.
//! 4. The reported quality is the quality of the returned assignment, scored
//!    by a network rebuilt from pristine clones of the two schemas. This is
//!    the metamorphic half: it catches a search that reports the cost of a
//!    different assignment from the one it hands back.
//! 5. The digest is stable across a repeat call in the same process.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run span_search -- -max_total_time=300
//! ```
//!
//! Setting `SPAN_DIAG` in the environment makes one execution report the shape
//! of the pair it drew and time the same pair under each option setting. It is
//! off by default because it runs three extra searches, and it is what turns a
//! slow unit from "something here is slow" into a named path.

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use panproto_gat::Name;
use panproto_mig::solve::DEFAULT_MEM_BYTES;
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::{
    Assignment, DEFAULT_WEIGHTS, DomainConstraints, SchemaSpan, SearchOptions, ValId,
    check_migration_morphism, find_span,
};
use panproto_schema::{Protocol, Schema, validate};

mod model;

/// Rebuild the assignment the span reports, in the variable order the network
/// numbers its variables by, which is ascending source-vertex name.
fn assignment_of(cfn: &panproto_mig::Cfn, span: &SchemaSpan) -> Option<Assignment> {
    let mut values = Vec::with_capacity(cfn.n_variables());
    for var in cfn.variable_ids() {
        let variable = cfn.variable(var)?;
        let source = Name::from(variable.name().as_ref());
        match span.right.vertex_map.get(&source) {
            Some(image) => values.push(variable.value_id(image)?),
            None => values.push(ValId::BOTTOM),
        }
    }
    Some(Assignment::from_values(values))
}

/// Every invariant that holds of a span whatever the options were.
fn check_span(src: &Schema, tgt: &Schema, protocol: &Protocol, span: &SchemaSpan) {
    // The apex is a schema in its own right.
    let errors = validate(&span.apex, protocol);
    assert!(errors.is_empty(), "the apex does not validate: {errors:?}");

    // Both legs are morphisms, checked at the theory level.
    check_migration_morphism(&span.apex, src, &span.left)
        .expect("the left leg must be a schema morphism");
    check_migration_morphism(&span.apex, tgt, &span.right)
        .expect("the right leg must be a schema morphism");

    // The left leg is an inclusion, so it renames nothing and covers the apex.
    assert_eq!(span.left.vertex_map.len(), span.apex.vertices.len());
    for (source, image) in &span.left.vertex_map {
        assert_eq!(source, image, "the left leg renames nothing");
        assert!(span.apex.vertices.contains_key(source));
    }

    // The apex is induced on a subset of the source.
    for id in span.apex.vertices.keys() {
        assert!(
            src.vertices.contains_key(id),
            "apex vertex {id} is not a source vertex"
        );
    }

    // Coverage is the fraction of the source the apex holds.
    let expected = if src.vertices.is_empty() {
        1.0
    } else {
        span.apex.vertices.len() as f64 / src.vertices.len() as f64
    };
    assert!(
        (span.apex_coverage - expected).abs() < 1e-9,
        "coverage {} disagrees with {expected}",
        span.apex_coverage
    );

    // The reported bounds bracket the reported quality.
    let (low, high) = span.quality_bounds;
    assert!(
        low <= span.quality + 1e-9 && span.quality <= high + 1e-9,
        "quality {} outside bounds {low}..{high}",
        span.quality
    );
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Some((src, tgt)) = model::pair(&mut u, 9) else {
        return;
    };
    let protocol = model::protocol();

    let monic = u.arbitrary::<bool>().unwrap_or(false);
    let iso = u.arbitrary::<bool>().unwrap_or(false);
    let opts = SearchOptions {
        monic,
        iso,
        ..SearchOptions::default()
    };

    let diagnose = std::env::var_os("SPAN_DIAG").is_some();
    if diagnose {
        eprintln!(
            "src: {} vertices, {} edges, {} variants, {} recursion points, {} spans",
            src.schema.vertices.len(),
            src.schema.edges.len(),
            src.schema.variants.len(),
            src.schema.recursion_points.len(),
            src.schema.spans.len()
        );
        eprintln!(
            "tgt: {} vertices, {} edges",
            tgt.schema.vertices.len(),
            tgt.schema.edges.len()
        );
        eprintln!("opts: monic={monic} iso={iso}");

        // The same pair under each option setting, so that a slow unit names
        // which path is slow rather than merely that one is.
        for (label, probe) in [
            ("default", SearchOptions::default()),
            (
                "monic",
                SearchOptions {
                    monic: true,
                    ..SearchOptions::default()
                },
            ),
            (
                "iso",
                SearchOptions {
                    iso: true,
                    ..SearchOptions::default()
                },
            ),
        ] {
            let clock = std::time::Instant::now();
            let outcome = find_span(&src.schema, &tgt.schema, &protocol, &probe);
            eprintln!(
                "  {label}: {:?} -> {}",
                clock.elapsed(),
                outcome.map_or_else(
                    |error| format!("Err({error})"),
                    |span| format!(
                        "apex {} vertices, limit_hit {:?}",
                        span.apex.vertices.len(),
                        span.certificate.limit_hit
                    )
                )
            );
        }
    }

    // 1. The search never refuses.
    let started = std::time::Instant::now();
    let span = find_span(&src.schema, &tgt.schema, &protocol, &opts)
        .expect("find_span is documented never to refuse a well-formed pair");
    if diagnose {
        eprintln!(
            "find_span took {:?}; path {:?}; limit_hit {:?}; proven_optimal {}",
            started.elapsed(),
            span.certificate.path,
            span.certificate.limit_hit,
            span.certificate.proven_optimal
        );
    }

    check_span(&src.schema, &tgt.schema, &protocol, &span);

    // 4. The reported quality is the quality of the returned assignment,
    //    scored against a network built from pristine clones.
    let pristine_src = src.schema.clone();
    let pristine_tgt = tgt.schema.clone();
    let cfn = build_cfn(
        &pristine_src,
        &pristine_tgt,
        &opts,
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        DEFAULT_MEM_BYTES,
    )
    .expect("the network posed once, so it poses again");
    let assignment =
        assignment_of(&cfn, &span).expect("the span's right leg must lie inside the domains");
    let scored = cfn.quality_of(&assignment);
    assert!(
        (scored - span.quality).abs() < 1e-9,
        "reported quality {} but the returned assignment scores {scored}",
        span.quality
    );

    // 5. The digest is stable across a repeat call.
    let again = find_span(&src.schema, &tgt.schema, &protocol, &opts)
        .expect("the second call refuses no more than the first");
    assert_eq!(
        span.certificate.apex_digest, again.certificate.apex_digest,
        "the apex digest moved between two calls on the same input"
    );
    assert_eq!(span.right.vertex_map, again.right.vertex_map);
    assert_eq!(span.left.edge_map, again.left.edge_map);
    assert_eq!(span.right.edge_map, again.right.edge_map);
});
