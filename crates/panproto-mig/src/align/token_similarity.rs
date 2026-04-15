//! Token-similarity alignment strategy.
//!
//! Splits identifier strings into a bag of word tokens (handling
//! `camelCase`, `snake_case`, `kebab-case`, and acronym boundaries),
//! then scores pairs by a convex combination of token Jaccard
//! similarity and character-bigram cosine similarity. The result is
//! independent of any specific protocol, alias table, or language, and
//! therefore serves as a general-purpose prior for the CSP solver.
//!
//! The output is validated downstream by the CSP's naturality check, so
//! false positives from the heuristic do not produce invalid morphisms.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};

/// Split an identifier into lowercase word tokens. Boundaries are
/// detected at:
///
/// * any of the separator characters `_ - . / ` and whitespace;
/// * camelCase transitions (lowercase letter or digit followed by an
///   uppercase letter);
/// * acronym→word transitions (uppercase letter followed by an uppercase
///   letter that is itself followed by a lowercase letter, e.g. `HTTPS`
///   in `HTTPServer` splits before `Server`);
/// * letter↔digit transitions in either direction.
///
/// Examples:
/// * `createdAt` → `["created", "at"]`
/// * `HTTPServer` → `["http", "server"]`
/// * `parseJSON` → `["parse", "json"]`
/// * `v2Endpoint` → `["v", "2", "endpoint"]`
#[must_use]
pub fn tokenize(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();

    for (i, &ch) in chars.iter().enumerate() {
        let is_sep = ch == '_' || ch == '-' || ch == '.' || ch == '/' || ch.is_whitespace();
        if is_sep {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            continue;
        }

        let prev = chars.get(i.wrapping_sub(1)).copied();
        let next = chars.get(i + 1).copied();

        let split_before = prev.is_some_and(|p| {
            // camelCase: lowercase|digit → uppercase
            let camel = (p.is_lowercase() || p.is_ascii_digit()) && ch.is_uppercase();
            // Acronym→word: prev upper, this upper, next lower (split before this).
            let acronym =
                p.is_uppercase() && ch.is_uppercase() && next.is_some_and(char::is_lowercase);
            // Letter↔digit
            let letter_digit = p.is_alphabetic() && ch.is_ascii_digit();
            let digit_letter = p.is_ascii_digit() && ch.is_alphabetic();
            camel || acronym || letter_digit || digit_letter
        });

        if split_before && !buf.is_empty() {
            out.push(std::mem::take(&mut buf));
        }
        for c in ch.to_lowercase() {
            buf.push(c);
        }
    }

    if !buf.is_empty() {
        out.push(buf);
    }

    out.into_iter().filter(|t| !t.is_empty()).collect()
}

/// Jaccard similarity between two token bags. Empty ∩ empty = 1.0.
#[must_use]
pub fn token_jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let set_a: std::collections::HashSet<&String> = a.iter().collect();
    let set_b: std::collections::HashSet<&String> = b.iter().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            intersection as f64 / union as f64
        }
    }
}

/// Character n-gram cosine similarity. Converts each string to a
/// multiset of character n-grams (n-length windows, padded with spaces),
/// then computes `cos(a, b) = a·b / (‖a‖ ‖b‖)`.
#[must_use]
pub fn char_ngram_cosine(a: &str, b: &str, n: usize) -> f64 {
    let grams_a = ngram_counts(a, n);
    let grams_b = ngram_counts(b, n);
    if grams_a.is_empty() || grams_b.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }

    #[allow(clippy::cast_precision_loss)]
    let norm_a: f64 = grams_a
        .values()
        .map(|c| (*c as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    #[allow(clippy::cast_precision_loss)]
    let norm_b: f64 = grams_b
        .values()
        .map(|c| (*c as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    let mut dot = 0.0;
    for (g, &ca) in &grams_a {
        if let Some(&cb) = grams_b.get(g) {
            #[allow(clippy::cast_precision_loss)]
            {
                dot += (ca as f64) * (cb as f64);
            }
        }
    }
    (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
}

fn ngram_counts(s: &str, n: usize) -> HashMap<String, usize> {
    let normalized: String = s
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect();
    let padded: Vec<char> = std::iter::repeat_n(' ', n.saturating_sub(1))
        .chain(normalized.chars())
        .chain(std::iter::repeat_n(' ', n.saturating_sub(1)))
        .collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    if n == 0 || padded.len() < n {
        return counts;
    }
    for window in padded.windows(n) {
        let gram: String = window.iter().collect();
        *counts.entry(gram).or_insert(0) += 1;
    }
    counts
}

/// Compound token similarity.
///
/// Computes the larger of `0.6 · Jaccard(tokens) + 0.4 · cosine(bigrams)`
/// and `cosine(bigrams)` (keeping room for names with no token overlap
/// but high char similarity like typos). Exactly equal strings score
/// `1.0`.
#[must_use]
pub fn token_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let ta = tokenize(a);
    let tb = tokenize(b);
    let jac = token_jaccard(&ta, &tb);
    let cos = char_ngram_cosine(a, b, 2);
    let combined = 0.6f64.mul_add(jac, 0.4 * cos);
    combined.max(cos).clamp(0.0, 1.0)
}

/// Emit token-similarity anchors. For each source vertex, find the
/// best-scoring kind-compatible target. If its score is above
/// `threshold`, emit an anchor with that score as confidence.
///
/// Priority is per-source (each source gets its best match), so ambiguous
/// many-to-one cases are resolved by [`super::resolve_anchors`] later.
#[must_use]
pub fn token_anchors(src: &Schema, tgt: &Schema, threshold: f64) -> Vec<Anchor> {
    let mut out = Vec::new();
    for src_id in src.vertices.keys() {
        let mut best: Option<(Name, f64)> = None;
        for tgt_id in tgt.vertices.keys() {
            if !kinds_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let score = token_similarity(src_id.as_str(), tgt_id.as_str());
            if best.as_ref().is_none_or(|(_, bs)| score > *bs) {
                best = Some((tgt_id.clone(), score));
            }
        }
        if let Some((tgt_id, score)) = best {
            if score >= threshold && score < 1.0 {
                // skip exact matches (covered by exact strategy)
                out.push(Anchor {
                    src: src_id.clone(),
                    tgt: tgt_id.clone(),
                    confidence: score,
                    strategy: StrategyTag::TokenSimilarity,
                    explanation: format!(
                        "token similarity {:.2}: {} ↔ {}",
                        score,
                        src_id.as_str(),
                        tgt_id.as_str()
                    ),
                });
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_camel_snake_kebab() {
        assert_eq!(tokenize("createdAt"), vec!["created", "at"]);
        assert_eq!(tokenize("created_at"), vec!["created", "at"]);
        assert_eq!(tokenize("created-at"), vec!["created", "at"]);
        assert_eq!(tokenize("CreatedAt"), vec!["created", "at"]);
        assert_eq!(tokenize("created at"), vec!["created", "at"]);
    }

    #[test]
    fn tokenize_handles_acronyms() {
        assert_eq!(tokenize("HTTPServer"), vec!["http", "server"]);
        assert_eq!(tokenize("parseJSON"), vec!["parse", "json"]);
        assert_eq!(tokenize("URLParser"), vec!["url", "parser"]);
    }

    #[test]
    fn jaccard_identical_tokens() {
        let a = tokenize("createdAt");
        let b = tokenize("created_at");
        assert!((token_jaccard(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_disjoint() {
        let a = tokenize("hello");
        let b = tokenize("world");
        assert!((token_jaccard(&a, &b) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn ngram_cosine_identical_strings() {
        assert!((char_ngram_cosine("hello", "hello", 2) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ngram_cosine_typo_variant_is_high() {
        let score = char_ngram_cosine("createdAt", "createAt", 2);
        assert!(score > 0.7, "typo variant should score high: {score}");
    }

    #[test]
    fn token_similarity_exact_is_one() {
        assert!((token_similarity("foo", "foo") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn token_similarity_casing_equivalence_is_high() {
        let score = token_similarity("createdAt", "created_at");
        assert!(
            score > 0.85,
            "casing-equivalent strings should score near 1.0: {score}"
        );
    }

    #[test]
    fn token_similarity_unrelated_is_low() {
        let score = token_similarity("createdAt", "authorId");
        assert!(
            score < 0.4,
            "unrelated identifiers should score low: {score}"
        );
    }
}
