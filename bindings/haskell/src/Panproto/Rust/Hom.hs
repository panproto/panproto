{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed morphism search and the theory → schema → data cascade:
-- the @'HomBackend' 'Rust'@ instance.
--
-- Implements the @hom@ surface of @libpanproto_c@ (see
-- @crates\/panproto-c\/CONTRACT.md@'s @hom@ domain, seven entry points)
-- by dispatching to "Panproto.Rust.FFI" through the
-- "Panproto.Rust.Handle" combinators. The instance is an orphan by
-- design, matching the @SchemaBackend Rust@ \/ @MigrationBackend Rust@
-- instances in "Panproto.Rust" and "Panproto.Rust.Migration": the 'Rust'
-- tag lives in "Panproto.Class", and each backend implementation lives
-- in its own module so it can be compiled out via cabal flags.
--
-- Method-to-entry-point mapping:
--
-- * 'findMorphisms' → @pp_hom_find_morphisms@
--   (@hom_search::find_morphisms@). The 'SearchOptions' are CBOR-encoded
--   via 'encodeSearchOptions'; the engine returns a CBOR
--   @Vec\<FoundMorphism\>@ that 'decodeFoundMorphismList' reifies. Every
--   element attains the optimum, so they all carry the same
--   'Panproto.Hom.quality'.
-- * 'findBestMorphism' → @pp_hom_find_best_morphism@
--   (@hom_search::find_best_morphism@). Same input; the engine returns a
--   CBOR @Option\<FoundMorphism\>@ decoded by 'decodeFoundMorphismMaybe'.
-- * 'findSpan' → @pp_hom_find_span@
--   (@hom_search::find_span_constrained@). Two CBOR inputs, the
--   'Panproto.Hom.SearchOptions' and the
--   'Panproto.Hom.DomainConstraints', are pinned together by nesting
--   'withSliceIn'; the third handle is the protocol the induced apex is
--   validated against. The engine returns a CBOR span that
--   'decodeFoundSpan' reifies.
-- * 'spanToOverlap' → @pp_hom_span_to_overlap@
--   (@SchemaSpan::to_overlap@). The span goes back out through
--   'encodeFoundSpan' and the engine answers with a CBOR
--   'Panproto.Hom.SchemaOverlap'.
-- * 'induceSchemaMorphism' → @pp_hom_induce_schema_morphism@
--   (@cascade::induce_schema_morphism@). The 'TheoryMorphism' is encoded
--   with 'encodeMorphism'; the result is a CBOR 'SchemaMorphism' decoded
--   with 'decodeSchemaMorphism'.
-- * 'induceMigrationFromTheory' → @pp_hom_induce_migration_from_theory@
--   (@cascade::induce_migration_from_theory@). The frozen entry point has
--   a /dual out/: a CBOR 'SchemaMorphism' written to a 'VecU8' buffer
--   /and/ a fresh @MigrationWithSchemas@ slab handle written to a
--   @Ptr Word32@. The buffer and the handle out-params are marshalled
--   together by 'withVecAndHandleOut'. The @CompiledRep Rust@ that this
--   method must return is then materialized from the public
--   'MigrationBackend' surface: the induced 'SchemaMorphism' is lowered
--   to a 'Migration' (carrying the same vertex and edge maps the cascade
--   computed) and 'compile'd against the same source and target schemas,
--   producing an equivalent applyable @CompiledMigration@. The engine's
--   transient dual-out handle is released, since the data-family
--   constructor for @CompiledRep Rust@ is private to
--   "Panproto.Rust.Migration".
module Panproto.Rust.Hom () where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32)
import Foreign (alloca, peek, poke)
import Foreign.C.Types (CInt)
import Foreign.Ptr (Ptr)

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Gat (encodeMorphism)
import Panproto.Hom
    ( FoundMorphism (..)
    , HomBackend (..)
    , SchemaMorphism (..)
    , decodeFoundSpan
    , decodeSchemaMorphism
    , decodeSchemaOverlap
    , encodeDomainConstraints
    , encodeFoundSpan
    , encodeSearchOptions
    )
import Panproto.Migration (Migration (..), MigrationBackend (..), emptyMigration)
import Panproto.Rust (protocolRepHandle, schemaRepHandle)
import Panproto.Rust.FFI
    ( VecU8
    , pp_handle_free
    , pp_hom_find_best_morphism_at
    , pp_hom_find_morphisms_at
    , pp_hom_find_span_at
    , pp_hom_induce_migration_from_theory_at
    , pp_hom_induce_schema_morphism_at
    , pp_hom_span_to_overlap_at
    )
import Panproto.Rust.Handle
    ( callVecOut
    , checkStatus
    , consumeVecU8
    , withSliceIn
    , withVecU8Out
    )
import Panproto.Rust.Migration ()
import Panproto.Schema (Edge (..))

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR

instance HomBackend Rust where
    findMorphisms src tgt opts = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        bs <-
            withSliceIn (encodeSearchOptions opts) $ \ptr len ->
                callVecOut (pp_hom_find_morphisms_at sh th ptr len)
        case decodeFoundMorphismList bs of
            Right ms -> pure ms
            Left err -> throwIO (hostDecodeError "pp_hom_find_morphisms" err)

    findBestMorphism src tgt opts = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        bs <-
            withSliceIn (encodeSearchOptions opts) $ \ptr len ->
                callVecOut (pp_hom_find_best_morphism_at sh th ptr len)
        case decodeFoundMorphismMaybe bs of
            Right m -> pure m
            Left err -> throwIO (hostDecodeError "pp_hom_find_best_morphism" err)

    findSpan src tgt proto opts constraints = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
            ph = protocolRepHandle proto
        bs <-
            withSliceIn (encodeSearchOptions opts) $ \optsPtr optsLen ->
                withSliceIn (encodeDomainConstraints constraints) $ \conPtr conLen ->
                    callVecOut (pp_hom_find_span_at sh th ph optsPtr optsLen conPtr conLen)
        case decodeFoundSpan bs of
            Right s -> pure s
            Left err -> throwIO (hostDecodeError "pp_hom_find_span" err)

    spanToOverlap _ found = do
        bs <-
            withSliceIn (encodeFoundSpan found) $ \ptr len ->
                callVecOut (pp_hom_span_to_overlap_at ptr len)
        case decodeSchemaOverlap bs of
            Right o -> pure o
            Left err -> throwIO (hostDecodeError "pp_hom_span_to_overlap" err)

    induceSchemaMorphism theoryMorph src = do
        let sh = schemaRepHandle src
        bs <-
            withSliceIn (encodeMorphism theoryMorph) $ \ptr len ->
                callVecOut (pp_hom_induce_schema_morphism_at ptr len sh)
        case decodeSchemaMorphism bs of
            Right m -> pure m
            Left err -> throwIO (hostDecodeError "pp_hom_induce_schema_morphism" err)

    induceMigrationFromTheory theoryMorph src tgt = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        -- The dual-out FFI writes the CBOR schema morphism to the buffer
        -- and a fresh MigrationWithSchemas handle to the scalar slot.
        (bs, engineHandle) <-
            withSliceIn (encodeMorphism theoryMorph) $ \ptr len ->
                withVecAndHandleOut (pp_hom_induce_migration_from_theory_at ptr len sh th)
        schemaMorph <- case decodeSchemaMorphism bs of
            Right m -> pure m
            Left err -> throwIO (hostDecodeError "pp_hom_induce_migration_from_theory" err)
        -- The engine's transient dual-out handle cannot be wrapped as a
        -- @CompiledRep Rust@ from here (its data-family constructor is
        -- private to "Panproto.Rust.Migration"), so release it and
        -- rebuild an equivalent compiled migration through the public
        -- 'compile' surface from the induced schema morphism.
        --
        -- The cascade preserves vertex IDs (the induced morphism's
        -- vertex map is the identity on the source vertices, with only
        -- vertex /kinds/ and edge /kinds/ renamed), so the lowered
        -- migration's vertex-map targets live in the source vertex
        -- namespace. It is therefore compiled against @src@ as both
        -- endpoints: @mig::compile@ validates that every vertex-map
        -- target exists in the target schema, which holds for @src@ but
        -- not for an unrelated @tgt@. The compiled @Delta_F@ pullback
        -- this produces remaps the renamed edge kinds over the
        -- ID-stable vertex set, matching the cascade's own
        -- @induce_data_migration@ behavior.
        pp_handle_free engineHandle >>= checkStatus
        compiled <- compile (schemaMorphismToMigration schemaMorph) src src
        pure (schemaMorph, compiled)

-- ---------------------------------------------------------------------------
-- Dual-out marshalling

-- | Run an FFI call that writes /both/ a 'VecU8' buffer and a single
-- @u32@ handle to out-params (the shape of
-- @pp_hom_induce_migration_from_theory_at@), check its status, and
-- return the buffer bytes paired with the handle. The buffer is freed on
-- the way out by 'withVecU8Out'; the handle slot is the caller's to
-- manage.
--
-- This composes "Panproto.Rust.Handle"'s 'withVecU8Out' (which owns the
-- buffer lifecycle and the sentinel-pointer convention) with a stack
-- 'alloca' for the handle, deferring the status check until both
-- out-params are populated.
withVecAndHandleOut
    :: (Ptr VecU8 -> Ptr Word32 -> IO CInt)
    -- ^ FFI call taking @(Ptr VecU8, Ptr Word32)@ out-params.
    -> IO (LBS.ByteString, Word32)
withVecAndHandleOut action =
    alloca $ \pHandle -> do
        poke pHandle (maxBound :: Word32)
        bytes <-
            withVecU8Out
                (\pVec -> action pVec pHandle >>= checkStatus)
                consumeVecU8
        handle <- peek pHandle
        pure (bytes, handle)

-- ---------------------------------------------------------------------------
-- Found-morphism collection decoders

-- | Decode a CBOR @Vec\<FoundMorphism\>@ (the @pp_hom_find_morphisms@
-- output) into a list of 'FoundMorphism', reusing the single-value
-- decoder for each element. Rejects trailing bytes, matching the
-- canonical decoder contract.
decodeFoundMorphismList :: LBS.ByteString -> Either String [FoundMorphism]
decodeFoundMorphismList = runWholeDecoder (decodeListOf foundMorphismDecoder) "found morphism list"

-- | Decode a CBOR @Option\<FoundMorphism\>@ (the
-- @pp_hom_find_best_morphism@ output: a CBOR @null@ for 'Nothing', the
-- morphism map otherwise) into a @Maybe FoundMorphism@.
decodeFoundMorphismMaybe :: LBS.ByteString -> Either String (Maybe FoundMorphism)
decodeFoundMorphismMaybe = runWholeDecoder (decodeMaybeOf foundMorphismDecoder) "found morphism option"

-- | Run a decoder over the whole input, demanding it consume every byte.
runWholeDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runWholeDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

-- | A single 'FoundMorphism' element decoder, sequenceable inside the
-- list and option decoders. The field handling mirrors the pinned wire
-- shape of "Panproto.Hom"'s 'Panproto.Hom.encodeFoundMorphism':
-- @vertex_map@ (a plain string-keyed CBOR map, since a @Name@ is
-- transparent over a string), @edge_map@ (the @map_as_vec@ array of
-- @[edge, edge]@ pairs, the @Edge@ key not being usable as a CBOR map
-- key), and @quality@ (a double, tolerating an integer encoding). Absent
-- fields default to empty maps and a zero quality; unknown fields are
-- skipped for forward compatibility.
foundMorphismDecoder :: Decoder s FoundMorphism
foundMorphismDecoder = decodeFields (HM.empty, HM.empty, 0) build handler
  where
    build (vm, em, q) = FoundMorphism vm em q
    handler acc@(vm, em, q) key = case key of
        "vertex_map" -> (\v -> (v, em, q)) <$> decodeTextMap Dec.decodeString
        "edge_map" -> (\v -> (vm, v, q)) <$> decodeEdgeKeyMap decodeEdge
        "quality" -> (\v -> (vm, em, v)) <$> decodeDouble
        _ -> skipTerm >> pure acc

-- | Decode a CBOR list (definite or indefinite) of elements.
decodeListOf :: Decoder s a -> Decoder s [a]
decodeListOf decA = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = (:) <$> decA <*> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> decA <*> goIndef

-- | Decode a CBOR @null@ as 'Nothing', anything else as 'Just' via the
-- element decoder. Matches @ciborium@'s @Option\<T\>@ encoding (a bare
-- @null@ for @None@, the @T@ value for @Some@).
decodeMaybeOf :: Decoder s a -> Decoder s (Maybe a)
decodeMaybeOf decA = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> decA

-- ---------------------------------------------------------------------------
-- CBOR field primitives
--
-- These mirror the tolerant-decoder idiom of "Panproto.Hom" and
-- "Panproto.Migration": a tuple accumulator threaded through a per-key
-- handler, snake_case keys, @map_as_vec@ for the @Edge@-keyed map, an
-- integer-tolerant double, and a depth-first unknown-term skipper.

-- | Decode a CBOR map, threading a tuple accumulator through an entry
-- handler and applying a constructor at the end.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> build <$> goN n initial
        Nothing -> build <$> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = do
        k <- Dec.decodeString
        acc' <- onKey acc k
        goN (n - 1 :: Int) acc'
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else do
                k <- Dec.decodeString
                acc' <- onKey acc k
                goIndef acc'

-- | Decode a CBOR map with text keys into a 'HashMap'.
decodeTextMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeTextMap decV = HM.fromList <$> decodeMapPairs Dec.decodeString decV

-- | Decode an @Edge -> v@ map from the @map_as_vec@ array of pairs.
decodeEdgeKeyMap :: Decoder s v -> Decoder s (HashMap Edge v)
decodeEdgeKeyMap decV = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        e <- decodeEdge
        v <- decV
        pure (e, v)

-- | Decode a @panproto_schema::Edge@ from the @ciborium@ struct shape,
-- building positionally to sidestep @DuplicateRecordFields@ ambiguity,
-- matching the decoder in "Panproto.Hom".
decodeEdge :: Decoder s Edge
decodeEdge = decodeFields (T.empty, T.empty, T.empty, Nothing) build handler
  where
    build (s, t, k, n) = Edge s t k n
    handler acc@(s, t, k, n) key = case key of
        "src" -> (\v -> (v, t, k, n)) <$> Dec.decodeString
        "tgt" -> (\v -> (s, v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (s, t, v, n)) <$> Dec.decodeString
        "name" -> (\v -> (s, t, k, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

-- | Decode a CBOR map's key/value pairs (definite or indefinite) into an
-- association list.
decodeMapPairs :: Decoder s k -> Decoder s v -> Decoder s [(k, v)]
decodeMapPairs decK decV = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = do
        k <- decK
        v <- decV
        ((k, v) :) <$> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                k <- decK
                v <- decV
                ((k, v) :) <$> goIndef

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> Dec.decodeString

-- | Decode a CBOR floating-point quality score, tolerating an integer
-- encoding (e.g. an exact @1.0@ a producer wrote as the integer @1@).
decodeDouble :: Decoder s Double
decodeDouble = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeFloat16 -> realToFrac <$> Dec.decodeFloat
        Dec.TypeFloat32 -> realToFrac <$> Dec.decodeFloat
        Dec.TypeFloat64 -> Dec.decodeDouble
        Dec.TypeUInt -> fromIntegral <$> Dec.decodeWord
        Dec.TypeUInt64 -> fromIntegral <$> Dec.decodeWord64
        Dec.TypeNInt -> fromIntegral <$> Dec.decodeInt
        Dec.TypeNInt64 -> fromIntegral <$> Dec.decodeInt64
        Dec.TypeInteger -> fromInteger <$> Dec.decodeInteger
        _ -> fail "decodeDouble: expected a numeric quality score"

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields for forward compatibility.
skipTerm :: Decoder s ()
skipTerm = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> () <$ Dec.decodeWord
        Dec.TypeUInt64 -> () <$ Dec.decodeWord64
        Dec.TypeNInt -> () <$ Dec.decodeInt
        Dec.TypeNInt64 -> () <$ Dec.decodeInt64
        Dec.TypeInteger -> () <$ Dec.decodeInteger
        Dec.TypeFloat16 -> () <$ Dec.decodeFloat
        Dec.TypeFloat32 -> () <$ Dec.decodeFloat
        Dec.TypeFloat64 -> () <$ Dec.decodeDouble
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> Dec.decodeStringIndef >> skipUntilBreakStrings
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> Dec.decodeBytesIndef >> skipUntilBreakBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        _ -> fail "decodeHom: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs
    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes
    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings

-- ---------------------------------------------------------------------------
-- Errors

-- | A 'PanprotoError' tagged @host_decode@ for when the bytes the engine
-- returned do not decode into the expected shape. Matches the
-- @host_decode@ envelope the sibling Rust backend modules raise.
hostDecodeError :: String -> String -> PanprotoError
hostDecodeError site reason =
    PanprotoError
        { code = StatusSerialization
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusSerialization
                    , tag = "host_decode"
                    , message =
                        "panproto could not decode the result of "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }

-- ---------------------------------------------------------------------------
-- Migration lowering

-- | Lower a 'SchemaMorphism' to a 'Migration' for compilation, mirroring
-- @panproto_mig::hom_search::morphism_to_migration@: carry the vertex
-- and edge maps straight across and leave every resolver table empty (a
-- cascade-induced morphism is total on the source schema, so no
-- contraction resolution is needed).
schemaMorphismToMigration :: SchemaMorphism -> Migration
schemaMorphismToMigration m =
    Migration
        { vertexMap = m.vertexMap
        , edgeMap = m.edgeMap
        , hyperEdgeMap = emptyMigration.hyperEdgeMap
        , labelMap = emptyMigration.labelMap
        , resolver = emptyMigration.resolver
        , hyperResolver = emptyMigration.hyperResolver
        , exprResolvers = emptyMigration.exprResolvers
        }
