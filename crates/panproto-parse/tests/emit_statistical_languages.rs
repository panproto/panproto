//! AST-faithful (Grade 2) emit round-trip for the statistical-modelling
//! languages: Stan, BUGS, and JAGS. These grammars ship no upstream
//! tree-sitter corpus, so the corpus audit cannot exercise them; this
//! test pins their behaviour on real models instead.
//!
//! The contract is the canonical-section law in its AST-equivalence
//! form: parsing the emitted text must recover the *same* abstract
//! syntax tree (kind multiset and edge multiset) as the source, after
//! `forget_layout` strips the byte-offset / interstitial fibre. This is
//! weaker than byte-identity (the emitter may reflow whitespace) but is
//! exactly what transpilation requires: the program means the same
//! thing. Byte-identity (Grade 3) is a separate, stronger goal handled
//! by the verified-protocol fixed-point gate.

// Compile only when at least one statistical grammar is enabled; under
// the workspace-default `group-core` (which has none of them) the whole
// file is empty, so the helpers are never orphaned dead code.
#![cfg(all(
    feature = "grammars",
    any(feature = "lang-stan", feature = "lang-bugs", feature = "lang-jags")
))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;
use panproto_schema::{edge_multiset, kind_multiset};

/// Parse `src`, strip the layout fibre, emit the canonical section, and
/// assert the re-parsed AST is structurally identical (Grade 2).
fn assert_ast_faithful(protocol: &str, ext: &str, src: &str) {
    let reg = ParserRegistry::new();
    let file = format!("model.{ext}");
    let s1 = reg
        .parse_with_protocol(protocol, src.as_bytes(), &file)
        .unwrap_or_else(|e| panic!("{protocol} parse failed: {e}"));
    // The source must itself parse cleanly: an ERROR node means the
    // grammar rejected it, and the round-trip would be vacuous.
    assert!(
        !s1.vertices
            .values()
            .any(|v| v.kind.as_ref().contains("ERROR")),
        "{protocol}: source did not parse cleanly (ERROR node present)\n{src}"
    );
    let abstract_schema = s1.forget_layout();
    let emitted = reg
        .emit_pretty_with_protocol(protocol, &abstract_schema)
        .unwrap_or_else(|e| panic!("{protocol} emit failed: {e}"));
    let s2 = reg
        .parse_with_protocol(protocol, &emitted, &file)
        .unwrap_or_else(|e| {
            panic!(
                "{protocol} re-parse of emitted text failed: {e}\nemitted:\n{}",
                String::from_utf8_lossy(&emitted)
            )
        });
    let (ka, kb) = (kind_multiset(&abstract_schema), kind_multiset(&s2));
    let (ea, eb) = (edge_multiset(&abstract_schema), edge_multiset(&s2));
    assert!(
        ka == kb && ea == eb,
        "{protocol}: emitted text re-parsed to a different AST.\n\
         emitted:\n{}\nkind delta: {:?}\n",
        String::from_utf8_lossy(&emitted),
        kind_delta(&ka, &kb)
    );
}

/// Per-kind count difference (re-parse minus source), nonzero entries
/// only, for failure diagnostics.
fn kind_delta(
    a: &std::collections::BTreeMap<String, usize>,
    b: &std::collections::BTreeMap<String, usize>,
) -> Vec<(String, i64)> {
    let mut keys: std::collections::BTreeSet<&String> = a.keys().collect();
    keys.extend(b.keys());
    keys.into_iter()
        .filter_map(|k| {
            let d = i64::try_from(*b.get(k).unwrap_or(&0)).unwrap_or(0)
                - i64::try_from(*a.get(k).unwrap_or(&0)).unwrap_or(0);
            (d != 0).then(|| (k.clone(), d))
        })
        .collect()
}

#[cfg(feature = "lang-stan")]
#[test]
fn stan_models_are_ast_faithful() {
    // Eight-schools-style hierarchical normal model.
    assert_ast_faithful(
        "stan",
        "stan",
        "data {\n  int<lower=0> N;\n  vector[N] y;\n  vector<lower=0>[N] sigma;\n}\n\
         parameters {\n  real mu;\n  real<lower=0> tau;\n  vector[N] theta;\n}\n\
         model {\n  theta ~ normal(mu, tau);\n  y ~ normal(theta, sigma);\n}\n",
    );
    // Logistic regression with a transformed-parameters block.
    assert_ast_faithful(
        "stan",
        "stan",
        "data {\n  int<lower=0> N;\n  int<lower=0> K;\n  matrix[N, K] x;\n  array[N] int<lower=0, upper=1> y;\n}\n\
         parameters {\n  vector[K] beta;\n  real alpha;\n}\n\
         model {\n  beta ~ normal(0, 1);\n  alpha ~ normal(0, 5);\n  y ~ bernoulli_logit(alpha + x * beta);\n}\n",
    );
    // A user-defined function and a for loop.
    assert_ast_faithful(
        "stan",
        "stan",
        "functions {\n  real rho(real x) {\n    return exp(-x);\n  }\n}\n\
         model {\n  for (i in 1:10) {\n    target += rho(i);\n  }\n}\n",
    );
}

#[cfg(feature = "lang-bugs")]
#[test]
fn bugs_models_are_ast_faithful() {
    // Classic rats growth-curve model.
    assert_ast_faithful(
        "bugs",
        "bug",
        "model {\n  for (i in 1:N) {\n    for (j in 1:T) {\n      Y[i, j] ~ dnorm(mu[i, j], tau.c)\n      mu[i, j] <- alpha[i] + beta[i] * (x[j] - xbar)\n    }\n    alpha[i] ~ dnorm(alpha.c, alpha.tau)\n    beta[i] ~ dnorm(beta.c, beta.tau)\n  }\n  tau.c ~ dgamma(0.001, 0.001)\n  alpha.c ~ dnorm(0.0, 1.0E-6)\n}\n",
    );
    // Seeds random-effects logistic model fragment.
    assert_ast_faithful(
        "bugs",
        "bug",
        "model {\n  for (i in 1:N) {\n    r[i] ~ dbin(p[i], n[i])\n    logit(p[i]) <- alpha0 + alpha1 * x1[i] + b[i]\n    b[i] ~ dnorm(0.0, tau)\n  }\n  alpha0 ~ dnorm(0.0, 1.0E-6)\n  tau ~ dgamma(0.001, 0.001)\n}\n",
    );
}

#[cfg(feature = "lang-jags")]
#[test]
fn jags_models_are_ast_faithful() {
    // Hierarchical normal with a deterministic precision link.
    assert_ast_faithful(
        "jags",
        "jags",
        "model {\n  for (i in 1:N) {\n    y[i] ~ dnorm(mu, tau)\n  }\n  mu ~ dnorm(0, 1.0E-6)\n  tau <- pow(sigma, -2)\n  sigma ~ dunif(0, 100)\n}\n",
    );
    // Poisson regression with a log link.
    assert_ast_faithful(
        "jags",
        "jags",
        "model {\n  for (i in 1:N) {\n    y[i] ~ dpois(lambda[i])\n    log(lambda[i]) <- beta0 + beta1 * x[i]\n  }\n  beta0 ~ dnorm(0, 0.001)\n  beta1 ~ dnorm(0, 0.001)\n}\n",
    );
}
