-- | Engine-backed schema operations: normalization, metadata, ATProto
-- lexicon ingest, and the enrichment surface (coercions, defaults,
-- mergers, policies, refinement subsort checks).
--
-- These operate on a backend's schema representation through the
-- categorical engine, so they live behind a capability class rather
-- than as pure functions. The class is filled in a later wave; the
-- 'Rust' instance is authored in "Panproto.Rust.Enriched".
module Panproto.Enriched () where
