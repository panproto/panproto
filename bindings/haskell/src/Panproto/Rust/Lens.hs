{-# LANGUAGE BangPatterns #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed implementation of the @lens@ capability class.
--
-- The @LensBackend Rust@ instance is an orphan by design, like the
-- sibling backend instances in "Panproto.Rust" and
-- "Panproto.Rust.Instance": the 'Rust' tag lives in "Panproto.Class"
-- and each backend lives in its own module so it can be compiled out
-- via cabal flags.
--
-- Three artifacts cross the boundary as slab handles, each a thin
-- @u32@ wrapper:
--
-- * 'ChainRep' 'Rust' is a 'RustChain': a handle to a
--   @Resource::ProtolensChain@.
-- * 'LensRep' 'Rust' is a 'RustLens': a handle to a
--   @Resource::MigrationWithSchemas@ (a compiled migration plus its
--   source and target schemas).
-- * 'SymLensRep' 'Rust' is a 'RustSymLens': a handle to a
--   @Resource::SymmetricLensHandle@.
--
-- Instances and complements are /not/ handles: they cross as CBOR
-- @WInstance@ / @Complement@ values (see "Panproto.Instance"), so
-- 'lensGet' \/ 'lensPut' \/ 'symmetricSync' serialize through
-- 'encodeInstance' \/ 'encodeComplement' and decode with
-- 'decodeInstance' \/ 'decodeComplement'. The anchoring schemas remain
-- handles, read out of a @SchemaRep Rust@ via 'schemaRepHandle'.
--
-- == CBOR shapes
--
-- * @get_record@ returns a two-key map @{ view: bytes, complement:
--   bytes }@ whose values are CBOR byte strings, each a self-contained
--   CBOR item. The host decodes the outer map and runs the existing
--   whole-blob 'decodeInstance' \/ 'decodeComplement' codecs on each
--   field. This framing sidesteps writing nested @WInstance@ \/
--   @Complement@ sub-decoders on the host.
-- * @put_record@ and @symmetric_sync@ take the @WInstance@ and
--   @Complement@ as two separate input slices.
-- * @complement_spec@ returns a @camelCase@-keyed @ComplementSpec@ map;
--   the host reads the snake_case @kind@ enum and the @summary@ string,
--   surfacing them as @(ComplementKind, Text)@.
-- * @chain_to_json@ emits the lightweight @Vec\<ProtolensStepInfo\>@
--   summary array (the shape 'reifyChain' parses through the aeson
--   bridge), distinct from the full serde @ProtolensChain@ JSON that
--   @from_json@ ('chainFromJsonIO') parses.
--
-- == The summary \/ structure asymmetry
--
-- The pure 'ProtolensChain' value (see "Panproto.Lens") is the /step
-- summary/: each step carries only its name, endofunctor names, and
-- lossless flag. The engine's runnable chain additionally carries the
-- endofunctor /structure/ and the complement constructors, which the
-- summary does not. The C ABI offers no entry point that rebuilds a
-- runnable chain from a summary, so 'ingestChain' is lossless only for
-- the empty (identity) chain; a non-empty pure summary that did not
-- originate from the engine cannot be reconstructed and raises a
-- documented 'PanprotoError'. The engine-originated directions
-- ('autoGenerateProtolens', 'protolensFromDiff', 'compileDocument',
-- 'fuseChainIO', 'composeChain', 'chainFromJsonIO') all return runnable
-- handles directly and are unaffected.
module Panproto.Rust.Lens
    ( RustChain (..)
    , RustLens (..)
    , RustSymLens (..)
    ) where

import Control.Exception (throwIO)
import Data.ByteString (ByteString)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32, Word8)
import Foreign.C.Types (CInt, CSize)
import Foreign.Ptr (Ptr)

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Data.Aeson qualified as Aeson
import Data.Aeson.Key qualified as Key
import Data.Aeson.KeyMap qualified as KM
import Data.Scientific qualified as Sci
import Data.Vector qualified as V

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Instance
    ( InstanceBackend (..)
    , decodeComplement
    , decodeInstance
    , encodeComplement
    , encodeInstance
    )
import Panproto.Lens
    ( ComplementKind (..)
    , LensBackend (..)
    , Stringency (..)
    , chainFromJson
    , composedOpticKind
    , isIdentityChain
    )
import Panproto.Rust (schemaRepHandle)
-- The @InstanceBackend Rust@ orphan instance (used by 'lensGet' /
-- 'lensPut' / the law checks to move 'InstanceRep's) lives here; import
-- it for its instance even though no name is referenced directly.
import Panproto.Rust.Instance ()
import Panproto.Rust.FFI
    ( VecU8
    , pp_handle_free
    , pp_lens_auto_generate_candidates_at
    , pp_lens_auto_generate_protolens_at
    , pp_lens_check_get_put_at
    , pp_lens_check_laws_at
    , pp_lens_check_put_get_at
    , pp_lens_compile_document_at
    , pp_lens_compose
    , pp_lens_get_record_at
    , pp_lens_put_record_at
    , pp_lens_symmetric_from_schemas
    , pp_lens_symmetric_sync_at
    , pp_protolens_chain_to_json
    , pp_protolens_complement_spec_at
    , pp_protolens_compose
    , pp_protolens_from_diff_at
    , pp_protolens_from_json_at
    , pp_protolens_fuse
    , pp_protolens_instantiate
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

-- ---------------------------------------------------------------------------
-- Handle wrappers

-- | A handle into panproto-c's slab pointing at a
-- @Resource::ProtolensChain@.
newtype RustChain = RustChain {chainHandle :: Word32}
    deriving stock (Eq, Show)

-- | A handle pointing at a @Resource::MigrationWithSchemas@ (a compiled
-- lens: a migration plus its source and target schemas).
newtype RustLens = RustLens {lensHandle :: Word32}
    deriving stock (Eq, Show)

-- | A handle pointing at a @Resource::SymmetricLensHandle@.
newtype RustSymLens = RustSymLens {symLensHandle :: Word32}
    deriving stock (Eq, Show)

-- ---------------------------------------------------------------------------
-- LensBackend instance

instance LensBackend Rust where
    newtype ChainRep Rust = RustChainRep RustChain
    newtype LensRep Rust = RustLensRep RustLens
    newtype SymLensRep Rust = RustSymLensRep RustSymLens

    -- Pure summary -> runnable handle. Lossless only for the identity
    -- chain (no steps); a non-empty summary cannot be reconstructed
    -- because the C ABI exposes no structure-from-summary builder.
    ingestChain _ chain
        | isIdentityChain chain =
            -- The engine's from_json parses the full serde ProtolensChain
            -- shape; an empty chain is { "steps": [] }.
            withSliceIn (utf8 "{\"steps\":[]}") $ \ptr len ->
                RustChainRep . RustChain <$> callHandleOut (pp_protolens_from_json_at ptr len)
        | otherwise =
            throwIO $
                unsupportedError
                    "ingestChain"
                    ( "cannot reconstruct a runnable protolens chain from a non-empty step "
                        <> "summary: the C ABI provides no structure-from-summary builder. "
                        <> "Obtain a runnable chain from the engine (autoGenerateProtolens, "
                        <> "protolensFromDiff, compileDocument, chainFromJsonIO) instead."
                    )

    reifyChain (RustChainRep (RustChain h)) = do
        bs <- callVecOut (pp_protolens_chain_to_json h)
        case chainFromJson bs of
            Right c -> pure c
            Left err -> throwIO $ hostDecodeError "pp_protolens_chain_to_json" err

    releaseChain (RustChainRep (RustChain h)) = freeHandle h
    releaseLens (RustLensRep (RustLens h)) = freeHandle h
    releaseSymLens (RustSymLensRep (RustSymLens h)) = freeHandle h

    lensGet (RustLensRep (RustLens h)) instRep = do
        i <- reifyInstance instRep
        bs <- withSliceIn (encodeInstance i) $ \ptr len ->
            callVecOut (pp_lens_get_record_at h ptr len)
        case decodeGetRecord bs of
            Left err -> throwIO $ hostDecodeError "pp_lens_get_record" err
            Right (viewBytes, compBytes) -> do
                view <- case decodeInstance viewBytes of
                    Right v -> ingestInstance proxyRust v
                    Left err -> throwIO $ hostDecodeError "pp_lens_get_record (view)" err
                comp <- case decodeComplement compBytes of
                    Right c -> pure c
                    Left err -> throwIO $ hostDecodeError "pp_lens_get_record (complement)" err
                pure (view, comp)

    lensPut (RustLensRep (RustLens h)) viewRep complement = do
        view <- reifyInstance viewRep
        bs <-
            withSliceIn (encodeInstance view) $ \viewPtr viewLen ->
                withSliceIn (encodeComplement complement) $ \compPtr compLen ->
                    callVecOut (pp_lens_put_record_at h viewPtr viewLen compPtr compLen)
        case decodeInstance bs of
            Right i -> ingestInstance proxyRust i
            Left err -> throwIO $ hostDecodeError "pp_lens_put_record" err

    checkLaws lensRep instRep =
        runLawCheck "pp_lens_check_laws" pp_lens_check_laws_at lensRep instRep

    checkGetPut lensRep instRep =
        runLawCheck "pp_lens_check_get_put" pp_lens_check_get_put_at lensRep instRep

    checkPutGet lensRep instRep =
        runLawCheck "pp_lens_check_put_get" pp_lens_check_put_get_at lensRep instRep

    composeLens (RustLensRep (RustLens l1)) (RustLensRep (RustLens l2)) =
        RustLensRep . RustLens <$> callHandleOut (pp_lens_compose l1 l2)

    autoGenerateLens left right stringency = do
        chainRep <- autoGenerateProtolens left right stringency
        -- A lens is the chain instantiated at the source schema. The C
        -- ABI does not surface the alignment quality score, so report
        -- 1.0: a chain the engine returned witnesses a valid morphism.
        lensRep <- instantiateChain chainRep left
        releaseChain chainRep
        pure (lensRep, 1.0)

    autoGenerateProtolens left right stringency = do
        let lh = schemaRepHandle left
            rh = schemaRepHandle right
        withSliceIn (stringencyBytes stringency) $ \ptr len ->
            RustChainRep . RustChain
                <$> callHandleOut (pp_lens_auto_generate_protolens_at lh rh ptr len)

    -- The C ABI's auto_generate_candidates returns each ranked candidate
    -- with its own instantiable @chain@: the candidate's @ProtolensChain@
    -- in the same serde shape @ProtolensChain::to_json@ emits, carried
    -- alongside the score / coverage / quality / per-step explanations.
    -- We decode the @candidates@ list, pull each candidate's @chain@
    -- sub-value out of the CBOR payload, re-serialize it as JSON, and feed
    -- it through @pp_protolens_from_json@ (the same path 'chainFromJsonIO'
    -- uses) to obtain a real runnable chain handle. The full
    -- @[ChainRep]@ is returned, one entry per candidate the engine ranked.
    autoGenerateCandidates left right topN stringency = do
        let lh = schemaRepHandle left
            rh = schemaRepHandle right
        bs <- withSliceIn (stringencyBytes stringency) $ \ptr len ->
            callVecOut
                (pp_lens_auto_generate_candidates_at lh rh (fromIntegral topN) ptr len)
        chainJsons <- case decodeCandidateChains bs of
            Right js -> pure js
            Left err -> throwIO $ hostDecodeError "pp_lens_auto_generate_candidates" err
        traverse (chainFromJsonIO proxyRust) chainJsons

    instantiateChain (RustChainRep (RustChain ch)) schema =
        RustLensRep . RustLens
            <$> callHandleOut (pp_protolens_instantiate ch (schemaRepHandle schema))

    chainComplementSpec (RustChainRep (RustChain ch)) schema = do
        bs <- callVecOut (pp_protolens_complement_spec_at ch (schemaRepHandle schema))
        case decodeComplementSpec bs of
            Right kindSummary -> pure kindSummary
            Left err -> throwIO $ hostDecodeError "pp_protolens_complement_spec" err

    protolensFromDiff diffBytes schema1 schema2 = do
        let s1 = schemaRepHandle schema1
            s2 = schemaRepHandle schema2
        withSliceIn diffBytes $ \ptr len ->
            RustChainRep . RustChain
                <$> callHandleOut (pp_protolens_from_diff_at ptr len s1 s2)

    composeChain (RustChainRep (RustChain c1)) (RustChainRep (RustChain c2)) =
        RustChainRep . RustChain <$> callHandleOut (pp_protolens_compose c1 c2)

    fuseChainIO (RustChainRep (RustChain ch)) =
        RustChainRep . RustChain <$> callHandleOut (pp_protolens_fuse ch)

    chainFromJsonIO _ json =
        withSliceIn json $ \ptr len ->
            RustChainRep . RustChain <$> callHandleOut (pp_protolens_from_json_at ptr len)

    -- The C ABI carries the optic kind only through the step summary
    -- (chain_to_json). Reify the chain and fold its steps through the
    -- optics lattice. This is the conservative two-point structural split
    -- (Iso vs Lens) the summary supports; a backend with the full
    -- endofunctor structure could report Prism / Affine / Traversal.
    composeLensOpticKind chainRep = composedOpticKind <$> reifyChain chainRep

    symmetricFromSchemas left right =
        RustSymLensRep . RustSymLens
            <$> callHandleOut
                (pp_lens_symmetric_from_schemas (schemaRepHandle left) (schemaRepHandle right))

    symmetricSync (RustSymLensRep (RustSymLens h)) viewRep complement rightToLeft = do
        view <- reifyInstance viewRep
        let direction = if rightToLeft then 1 else 0
        bs <-
            withSliceIn (encodeInstance view) $ \viewPtr viewLen ->
                withSliceIn (encodeComplement complement) $ \compPtr compLen ->
                    callVecOut
                        (pp_lens_symmetric_sync_at h viewPtr viewLen compPtr compLen direction)
        case decodeInstance bs of
            Right i -> ingestInstance proxyRust i
            Left err -> throwIO $ hostDecodeError "pp_lens_symmetric_sync" err

    compileDocument _ source format bodyVertex =
        withSliceIn (utf8 source) $ \srcPtr srcLen ->
            withSliceIn (utf8 format) $ \fmtPtr fmtLen ->
                withSliceIn (utf8 bodyVertex) $ \bodyPtr bodyLen ->
                    RustChainRep . RustChain
                        <$> callHandleOut
                            ( pp_lens_compile_document_at
                                srcPtr
                                srcLen
                                fmtPtr
                                fmtLen
                                bodyPtr
                                bodyLen
                            )

-- ---------------------------------------------------------------------------
-- Shared helpers

-- | The 'Rust' tag as a 'Proxy', for the 'InstanceBackend' methods that
-- take one.
proxyRust :: Proxy Rust
proxyRust = Proxy

-- | Encode 'Text' as a UTF-8 lazy 'LBS.ByteString' for a borrowed input
-- slice.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | The snake_case stringency tier name the C ABI's parser accepts.
stringencyBytes :: Stringency -> LBS.ByteString
stringencyBytes = utf8 . tierText
  where
    tierText = \case
        Strict -> "strict"
        Balanced -> "balanced"
        Lenient -> "lenient"
        Exploratory -> "exploratory"

-- | Free a slab handle, turning a non-@Ok@ status into a 'PanprotoError'.
-- Idempotent at the slab level (double free is a no-op on the Rust side).
freeHandle :: Word32 -> IO ()
freeHandle h = do
    status <- pp_handle_free h
    checkStatus status

-- | The shared shape of the three law-check FFI entry points.
type LawCheckFfi = Word32 -> Ptr Word8 -> CSize -> Ptr VecU8 -> IO CInt

-- | Run a law-check entry point: serialize the instance, call the FFI,
-- and raise when the law does not hold. The CBOR 'LawCheckResult' the
-- engine returns is inspected here so a violation surfaces as an
-- exception, matching the class contract (the law checks throw on
-- violation rather than returning the structured report).
runLawCheck :: String -> LawCheckFfi -> LensRep Rust -> InstanceRep Rust -> IO ()
runLawCheck site ffi (RustLensRep (RustLens h)) instRep = do
    i <- reifyInstance instRep
    bs <- withSliceIn (encodeInstance i) $ \ptr len ->
        callVecOut (ffi h ptr len)
    case decodeLawCheck bs of
        Left err -> throwIO $ hostDecodeError site err
        Right (holds, mViolation)
            | holds -> pure ()
            | otherwise ->
                throwIO $ lawViolationError site (maybe "lens law violation" id mViolation)

-- ---------------------------------------------------------------------------
-- CBOR decoders
--
-- All follow the tolerant idiom of "Panproto.Instance": decode a map,
-- accumulate the fields of interest positionally, skip unknown terms.

-- | Decode the @{ view: bytes, complement: bytes }@ payload
-- @pp_lens_get_record@ produces into the two inner CBOR blobs.
decodeGetRecord :: LBS.ByteString -> Either String (LBS.ByteString, LBS.ByteString)
decodeGetRecord bs =
    case CBOR.deserialiseFromBytes getRecordDecoder bs of
        Left err -> Left (show err)
        Right (rest, (mView, mComp))
            | not (LBS.null rest) -> Left "trailing bytes after get_record payload"
            | otherwise -> case (mView, mComp) of
                (Just v, Just c) -> Right (LBS.fromStrict v, LBS.fromStrict c)
                _ -> Left "get_record payload missing view or complement"

getRecordDecoder :: Decoder s (Maybe ByteString, Maybe ByteString)
getRecordDecoder = decodeMapWith (Nothing, Nothing) onKey
  where
    onKey acc@(v, c) key = case key of
        "view" -> (\b -> (Just b, c)) <$> Dec.decodeBytes
        "complement" -> (\b -> (v, Just b)) <$> Dec.decodeBytes
        _ -> skipTerm >> pure acc

-- | Decode a CBOR @LawCheckResult@ (@{ holds: bool, violation: str|null
-- }@), returning the flag and the optional violation message.
decodeLawCheck :: LBS.ByteString -> Either String (Bool, Maybe Text)
decodeLawCheck bs =
    case CBOR.deserialiseFromBytes lawCheckDecoder bs of
        Left err -> Left (show err)
        Right (rest, r)
            | LBS.null rest -> Right r
            | otherwise -> Left "trailing bytes after LawCheckResult"

lawCheckDecoder :: Decoder s (Bool, Maybe Text)
lawCheckDecoder = decodeMapWith (False, Nothing) onKey
  where
    onKey acc@(h, v) key = case key of
        "holds" -> (\b -> (b, v)) <$> Dec.decodeBool
        "violation" -> do
            tt <- Dec.peekTokenType
            case tt of
                Dec.TypeNull -> Dec.decodeNull >> pure (h, Nothing)
                Dec.TypeString -> (\t -> (h, Just t)) <$> Dec.decodeString
                _ -> skipTerm >> pure acc
        _ -> skipTerm >> pure acc

-- | Decode a CBOR @ComplementSpec@ (camelCase keys; the @kind@ enum is
-- snake_case), surfacing the classification and the human-readable
-- summary.
decodeComplementSpec :: LBS.ByteString -> Either String (ComplementKind, Text)
decodeComplementSpec bs =
    case CBOR.deserialiseFromBytes complementSpecDecoder bs of
        Left err -> Left (show err)
        Right (rest, kindSummary)
            | LBS.null rest -> Right kindSummary
            | otherwise -> Left "trailing bytes after ComplementSpec"

complementSpecDecoder :: Decoder s (ComplementKind, Text)
complementSpecDecoder = decodeMapWith (CEmpty, T.empty) onKey
  where
    onKey acc@(k, s) key = case key of
        "kind" -> (\t -> (parseKind t, s)) <$> Dec.decodeString
        "summary" -> (\t -> (k, t)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc
    parseKind t = case t of
        "empty" -> CEmpty
        "data_captured" -> CDataCaptured
        "defaults_required" -> CDefaultsRequired
        "mixed" -> CMixed
        _ -> CEmpty

-- | Decode the @{ candidates, coerce_proposals }@ payload
-- @pp_lens_auto_generate_candidates@ produces into one JSON-encoded
-- @ProtolensChain@ per candidate, ready to feed back through
-- @pp_protolens_from_json@.
--
-- Each candidate is a map carrying score / coverage / quality / steps
-- alongside a @chain@ sub-value: the candidate's @ProtolensChain@ in the
-- serde shape @ProtolensChain::to_json@ emits, nested as a CBOR term
-- inside the @ciborium@-encoded wrapper. We decode that sub-term into an
-- aeson 'Aeson.Value' and re-encode it as JSON, which is byte-for-shape
-- what @pp_protolens_from_json@ (@ProtolensChain::from_json@) parses.
decodeCandidateChains :: LBS.ByteString -> Either String [LBS.ByteString]
decodeCandidateChains bs =
    case CBOR.deserialiseFromBytes candidateChainsDecoder bs of
        Left err -> Left (show err)
        Right (rest, js)
            | LBS.null rest -> Right (map Aeson.encode (reverse js))
            | otherwise -> Left "trailing bytes after candidates payload"

-- | Walk the wrapper map, capturing each candidate's @chain@ as an aeson
-- 'Aeson.Value'. The accumulator collects chains in reverse order; the
-- caller reverses to restore the engine's ranking.
candidateChainsDecoder :: Decoder s [Aeson.Value]
candidateChainsDecoder = decodeMapWith [] onKey
  where
    onKey acc key = case key of
        "candidates" -> decodeCandidateList
        _ -> skipTerm >> pure acc

-- | Decode the @candidates@ list, pulling the @chain@ value out of each
-- candidate map. A candidate missing its @chain@ is an engine
-- inconsistency, so the decoder fails rather than silently dropping it.
decodeCandidateList :: Decoder s [Aeson.Value]
decodeCandidateList = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n []
        Nothing -> goIndef []
  where
    goN 0 acc = pure acc
    goN n acc = decodeCandidateChain >>= \c -> goN (n - 1 :: Int) (c : acc)
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else decodeCandidateChain >>= \c -> goIndef (c : acc)

-- | Decode a single candidate map, returning its @chain@ sub-value.
decodeCandidateChain :: Decoder s Aeson.Value
decodeCandidateChain = do
    mChain <- decodeMapWith Nothing onKey
    case mChain of
        Just c -> pure c
        Nothing -> fail "candidate entry is missing its chain"
  where
    onKey acc key = case key of
        "chain" -> Just <$> decodeCborValue
        _ -> skipTerm >> pure acc

-- | Decode an arbitrary CBOR term (as produced by @ciborium@ from a
-- @serde_json::Value@) into an aeson 'Aeson.Value', so it can be
-- re-serialized as JSON. Covers the JSON-representable token types;
-- byte strings and tags do not occur in a @serde_json::Value@ and are
-- rejected.
decodeCborValue :: Decoder s Aeson.Value
decodeCborValue = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> intValue <$> Dec.decodeWord64
        Dec.TypeUInt64 -> intValue <$> Dec.decodeWord64
        Dec.TypeNInt -> intValue <$> Dec.decodeInt64
        Dec.TypeNInt64 -> intValue <$> Dec.decodeInt64
        Dec.TypeInteger -> (Aeson.Number . fromInteger) <$> Dec.decodeInteger
        Dec.TypeFloat16 -> doubleValue <$> Dec.decodeFloat
        Dec.TypeFloat32 -> doubleValue <$> Dec.decodeFloat
        Dec.TypeFloat64 -> doubleValue <$> Dec.decodeDouble
        Dec.TypeBool -> Aeson.Bool <$> Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull >> pure Aeson.Null
        Dec.TypeString -> Aeson.String <$> Dec.decodeString
        Dec.TypeListLen -> decodeArrayValue
        Dec.TypeListLen64 -> decodeArrayValue
        Dec.TypeListLenIndef -> decodeArrayValue
        Dec.TypeMapLen -> decodeObjectValue
        Dec.TypeMapLen64 -> decodeObjectValue
        Dec.TypeMapLenIndef -> decodeObjectValue
        _ -> fail "Panproto.Rust.Lens: unexpected CBOR token in candidate chain"
  where
    intValue :: Integral a => a -> Aeson.Value
    intValue = Aeson.Number . fromIntegral . toInteger
    doubleValue :: Real a => a -> Aeson.Value
    doubleValue = Aeson.Number . Sci.fromFloatDigits . realToFrac @_ @Double

decodeArrayValue :: Decoder s Aeson.Value
decodeArrayValue = do
    len <- Dec.decodeListLenOrIndef
    items <- case len of
        Just n -> goN n []
        Nothing -> goIndef []
    pure (Aeson.Array (V.fromList (reverse items)))
  where
    goN 0 acc = pure acc
    goN n acc = decodeCborValue >>= \v -> goN (n - 1 :: Int) (v : acc)
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else decodeCborValue >>= \v -> goIndef (v : acc)

decodeObjectValue :: Decoder s Aeson.Value
decodeObjectValue = do
    len <- Dec.decodeMapLenOrIndef
    pairs <- case len of
        Just n -> goN n []
        Nothing -> goIndef []
    pure (Aeson.Object (KM.fromList (reverse pairs)))
  where
    pair = do
        k <- Dec.decodeString
        v <- decodeCborValue
        pure (Key.fromText k, v)
    goN 0 acc = pure acc
    goN n acc = pair >>= \p -> goN (n - 1 :: Int) (p : acc)
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else pair >>= \p -> goIndef (p : acc)

-- ---------------------------------------------------------------------------
-- Generic CBOR map / skip helpers (mirroring "Panproto.Lens")

decodeMapWith :: acc -> (acc -> Text -> Decoder s acc) -> Decoder s acc
decodeMapWith initial onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
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

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields. Mirrors the skipper in "Panproto.Lens".
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
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        _ -> fail "Panproto.Rust.Lens: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs

-- ---------------------------------------------------------------------------
-- Errors

-- | The 'PanprotoError' raised when the engine returned 'StatusOk' but
-- the CBOR \/ JSON bytes did not decode into the expected shape. Mirrors
-- the @host_decode@ tag the sibling backends use.
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
                        "panproto could not decode the bytes returned by "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }

-- | The 'PanprotoError' raised when a law check reports a violation.
lawViolationError :: String -> Text -> PanprotoError
lawViolationError site violation =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "lens_law_violation"
                    , message = T.pack site <> ": " <> violation
                    }
        }

-- | The 'PanprotoError' raised for an operation the C ABI does not
-- support (the summary-to-runnable-chain reconstruction gap).
unsupportedError :: String -> Text -> PanprotoError
unsupportedError site detail =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "unsupported"
                    , message = T.pack site <> ": " <> detail
                    }
        }
