//! Embedding-based alignment strategy (feature-gated).
//!
//! Defines an [`Embedder`] trait and the cross-scoring machinery that
//! consumes it. Concrete [`Embedder`] implementations (local models,
//! remote APIs) are out of scope for this crate: callers that want
//! language-model-based anchoring construct their own embedder and
//! invoke [`embedding_anchors`] directly, merging the result into
//! their seed pool.
//!
//! No embedder is wired into `run_strategies` automatically.
//! [`embedding_anchors`] is a separate opt-in entry point that runs
//! independently of the stringency tier.
//!
//! Gated behind the `lm_embeddings` cargo feature so the trait and
//! its scaffolding compile only when callers opt in.

use panproto_gat::Name;
use panproto_schema::Schema;
use thiserror::Error;

use super::{Anchor, StrategyTag, kinds_compatible};

/// An error produced by an [`Embedder`].
#[derive(Debug, Error)]
pub enum EmbedError {
    /// The underlying model or API returned an error. The string is
    /// the implementation's own message.
    #[error("embedder failure: {0}")]
    Failure(String),
    /// The returned vector has the wrong dimensionality.
    #[error("embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// The dimension the embedder advertised via [`Embedder::dim`].
        expected: usize,
        /// The actual length of the vector the embedder produced.
        actual: usize,
    },
}

/// Producer of fixed-dimension real-valued embeddings for a string.
///
/// Implementations must return vectors of the same length on every
/// call (equal to [`Embedder::dim`]). The trait is object-safe-adjacent
/// in spirit: callers typically hold a concrete type, but the API
/// shape is compatible with a `dyn Embedder` wrapper should that be
/// useful.
pub trait Embedder {
    /// Embed `text` into a fixed-dimension real vector.
    ///
    /// # Errors
    ///
    /// Returns [`EmbedError::Failure`] on implementation error and
    /// [`EmbedError::DimensionMismatch`] if the produced vector does
    /// not have length [`Embedder::dim`].
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>;

    /// The dimensionality of vectors produced by [`Embedder::embed`].
    fn dim(&self) -> usize;
}

/// Cosine similarity between two equal-length vectors in `[-1, 1]`,
/// clamped to `[0, 1]` for use as an anchor confidence.
///
/// Returns `0.0` when either input is the zero vector or when the
/// lengths differ. The latter is a defensive clamp; callers should
/// ensure matching dimensions via [`Embedder::dim`].
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot: f64 = 0.0;
    let mut na: f64 = 0.0;
    let mut nb: f64 = 0.0;
    for i in 0..a.len() {
        let x = f64::from(a[i]);
        let y = f64::from(b[i]);
        dot = x.mul_add(y, dot);
        na = x.mul_add(x, na);
        nb = y.mul_add(y, nb);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let cos = dot / (na.sqrt() * nb.sqrt());
    cos.clamp(0.0, 1.0)
}

/// Retrieve an embedding-friendly description of `vertex_id`: the
/// vertex id concatenated with any description constraint. Empty
/// descriptions and missing constraints leave just the id.
fn embedding_text(schema: &Schema, vertex_id: &Name) -> String {
    let mut out = vertex_id.as_str().to_owned();
    if let Some(cs) = schema.constraints.get(vertex_id) {
        for c in cs {
            if c.sort.as_str() == "description" && !c.value.is_empty() {
                out.push(' ');
                out.push_str(&c.value);
                break;
            }
        }
    }
    out
}

/// Emit anchors for every `(src, tgt)` pair whose embedding cosine
/// similarity exceeds `threshold`, gated on [`kinds_compatible`].
///
/// The source and target texts are built from the vertex id plus any
/// description constraint so that both name and prose evidence feed
/// into the score.
///
/// Each source vertex emits at most one anchor, pointing at its
/// single best target. Targets are not deduplicated here: two source
/// vertices can propose the same target independently, and
/// [`crate::align::resolve_anchors`] handles monic-mode resolution
/// across strategies. This mirrors the behaviour of sibling strategies
/// which also produce multiple-target-per-target proposals and rely on
/// the resolver for uniqueness.
///
/// # Errors
///
/// Returns the first [`EmbedError`] produced by `embedder.embed` and
/// any dimension mismatch detected while cross-scoring.
pub fn embedding_anchors<E: Embedder>(
    src: &Schema,
    tgt: &Schema,
    embedder: &E,
    threshold: f64,
) -> Result<Vec<Anchor>, EmbedError> {
    let mut src_ids: Vec<&Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut tgt_ids: Vec<&Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let expected_dim = embedder.dim();

    let mut src_vecs: Vec<(&Name, Vec<f32>)> = Vec::with_capacity(src_ids.len());
    for id in &src_ids {
        let text = embedding_text(src, id);
        let vec = embedder.embed(&text)?;
        if vec.len() != expected_dim {
            return Err(EmbedError::DimensionMismatch {
                expected: expected_dim,
                actual: vec.len(),
            });
        }
        src_vecs.push((id, vec));
    }
    let mut tgt_vecs: Vec<(&Name, Vec<f32>)> = Vec::with_capacity(tgt_ids.len());
    for id in &tgt_ids {
        let text = embedding_text(tgt, id);
        let vec = embedder.embed(&text)?;
        if vec.len() != expected_dim {
            return Err(EmbedError::DimensionMismatch {
                expected: expected_dim,
                actual: vec.len(),
            });
        }
        tgt_vecs.push((id, vec));
    }

    let mut out = Vec::new();
    for (src_id, src_vec) in &src_vecs {
        let mut best: Option<(&Name, f64)> = None;
        for (tgt_id, tgt_vec) in &tgt_vecs {
            if !kinds_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let score = cosine_similarity(src_vec, tgt_vec);
            if best.as_ref().is_none_or(|(_, bs)| score > *bs) {
                best = Some((tgt_id, score));
            }
        }
        if let Some((tgt_id, score)) = best
            && score >= threshold
        {
            out.push(Anchor {
                src: (*src_id).clone(),
                tgt: (*tgt_id).clone(),
                confidence: score,
                strategy: StrategyTag::Llm,
                explanation: format!(
                    "embedding cosine {:.3}: {} ↔ {}",
                    score,
                    src_id.as_str(),
                    tgt_id.as_str()
                ),
            });
        }
    }
    Ok(out)
}

/// Deterministic hash-based [`Embedder`] intended for tests and
/// fallback scenarios. Not semantically meaningful: two strings with
/// similar meaning will not score high on each other.
///
/// Useful for exercising the cosine-similarity plumbing and verifying
/// end-to-end integration before plugging in a real model.
#[derive(Clone, Debug)]
pub struct HashEmbedder {
    /// Vector dimension produced by this embedder.
    pub dim: usize,
}

impl HashEmbedder {
    /// Build a hash embedder producing `dim`-dimensional vectors.
    #[must_use]
    pub const fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        if self.dim == 0 {
            return Err(EmbedError::Failure("zero-dimensional embedder".into()));
        }
        // Project `text` into `dim` buckets by token hash. Each token
        // contributes unit weight to its bucket. The result is a
        // sparse bag-of-hashed-tokens vector that reliably maps equal
        // inputs to equal outputs and has nontrivial cosine overlap
        // on shared tokens; it is not a semantic embedding.
        let mut v = vec![0.0f32; self.dim];
        for tok in super::token_similarity::tokenize(text) {
            let h = blake3::hash(tok.as_bytes());
            let bytes = h.as_bytes();
            // Use the first 8 bytes as a u64 seed.
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[..8]);
            let seed = u64::from_le_bytes(buf);
            let dim_u64 = u64::try_from(self.dim).unwrap_or(u64::MAX);
            let bucket = usize::try_from(seed % dim_u64).unwrap_or(0);
            v[bucket] += 1.0;
        }
        Ok(v)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    #[test]
    fn cosine_identical_vectors_is_one() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-9);
    }

    #[test]
    fn cosine_mismatched_length_returns_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_zero_vector_is_zero() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    fn proto() -> Protocol {
        Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn hash_embedder_end_to_end() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("a_shared_token", "string", None::<&str>)
            .unwrap()
            .vertex("completely_different", "string", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("shared_a_token", "string", None::<&str>)
            .unwrap()
            .vertex("utterly_alien_words", "string", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let embedder = HashEmbedder::new(64);
        let anchors = embedding_anchors(&src, &tgt, &embedder, 0.5).unwrap();
        // At least one anchor must surface: `a_shared_token` tokenizes
        // to the same multiset as `shared_a_token`, so their hash
        // vectors should be equal and cosine should land at 1.0.
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "a_shared_token" && a.tgt.as_str() == "shared_a_token"),
            "expected a_shared_token ↔ shared_a_token anchor in {:?}",
            anchors
                .iter()
                .map(|a| (a.src.as_str(), a.tgt.as_str(), a.confidence))
                .collect::<Vec<_>>()
        );
        for anchor in &anchors {
            assert_eq!(anchor.strategy, StrategyTag::Llm);
            assert!(anchor.confidence >= 0.5);
        }
    }

    #[test]
    fn hash_embedder_zero_dim_errors() {
        let e = HashEmbedder::new(0);
        assert!(e.embed("anything").is_err());
    }

    #[test]
    fn hash_embedder_emits_requested_dim() {
        let e = HashEmbedder::new(16);
        let v = e.embed("hello").unwrap();
        assert_eq!(v.len(), 16);
        assert_eq!(e.dim(), 16);
    }
}
