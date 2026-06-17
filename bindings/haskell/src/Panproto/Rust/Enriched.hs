{-# LANGUAGE TypeApplications #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust backend instance for the engine-backed schema/enrichment
-- operations declared in "Panproto.Enriched".
--
-- Every method dispatches to @libpanproto_c@ via "Panproto.Rust.FFI",
-- marshalling inputs with the "Panproto.Rust.Handle" combinators and
-- turning FFI status codes into 'Panproto.Errors.PanprotoError'
-- exceptions. The instance is an orphan by design: the 'Rust' tag and
-- its 'SchemaRep' \/ 'ProtocolRep' data instances live in "Panproto.Rust"
-- and "Panproto.Class", while the 'SchemaEngine' class lives in
-- "Panproto.Enriched"; this module joins the two.
--
-- The @RustSchemaRep@ \/ @RustProtocolRep@ data-family constructors are
-- not exported from "Panproto.Rust", so this module bridges between a
-- 'SchemaRep' \/ 'ProtocolRep' and a raw slab handle through the public
-- canonical codec: 'toCanonicalSchema' \/ 'toCanonical' to read the
-- representation out, 'Panproto.Rust.withRustSchema' \/
-- 'Panproto.Rust.withRustProtocol' to obtain a transient handle for the
-- FFI call, and 'fromCanonicalSchema' to rewrap the engine's result.
-- The transient handles are released automatically by the @with*@
-- brackets and by 'consumeSchemaHandle'.
module Panproto.Rust.Enriched () where

import Control.Exception (bracket, throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (Proxy))
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32, Word8)
import Foreign.C.Types (CInt, CSize)
import Foreign.Ptr (Ptr)

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Write qualified as CBOR
import Panproto.Canonical (CanonicalSchema (..))
import Panproto.Class
    ( ProtocolBackend (..)
    , Rust
    , SchemaBackend (..)
    )
import Panproto.Enriched
    ( SchemaEngine (..)
    , decodeSchemaMeta
    , encodeBuildOps
    , encodeMergerSpec
    , encodePolicySpec
    )
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Expr (encodeExpr)
import Panproto.Rust
    ( RustProtocol (..)
    , RustSchema (..)
    , withRustProtocol
    , withRustSchema
    )
import Panproto.Rust.FFI
    ( pp_enriched_refinement_subsort_at
    , pp_handle_free
    , pp_schema_add_coercion_at
    , pp_schema_add_default_at
    , pp_schema_add_merger_at
    , pp_schema_add_policy_at
    , pp_schema_build_at
    , pp_schema_metadata
    , pp_schema_normalize
    , pp_schema_parse_atproto_lexicon_at
    , pp_schema_to_cbor
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callScalarOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

instance SchemaEngine Rust where
    buildSchemaEngine protoRep ops =
        withProtoHandle protoRep $ \ph ->
            withSliceIn (encodeBuildOps ops) $ \ptr len ->
                callHandleOut (pp_schema_build_at ph ptr len) >>= consumeSchemaHandle

    schemaMetadata schemaRep =
        withSchemaHandle schemaRep $ \sh -> do
            bs <- callVecOut (pp_schema_metadata sh)
            case decodeSchemaMeta bs of
                Right meta -> pure meta
                Left err -> throwIO (hostDecodeError "pp_schema_metadata" err)

    normalizeSchema schemaRep =
        withSchemaHandle schemaRep $ \sh ->
            callHandleOut (pp_schema_normalize sh) >>= consumeSchemaHandle

    parseAtprotoLexicon _ json =
        withSliceIn json $ \ptr len ->
            callHandleOut (pp_schema_parse_atproto_lexicon_at ptr len)
                >>= consumeSchemaHandle

    addCoercion schemaRep fromKind toKind expr =
        withSchemaHandle schemaRep $ \sh ->
            withSliceIn (encodeTextUtf8 fromKind) $ \fromPtr fromLen ->
                withSliceIn (encodeTextUtf8 toKind) $ \toPtr toLen ->
                    withSliceIn (encodeExpr expr) $ \exprPtr exprLen ->
                        callHandleOut
                            ( pp_schema_add_coercion_at
                                sh
                                fromPtr
                                fromLen
                                toPtr
                                toLen
                                exprPtr
                                exprLen
                            )
                            >>= consumeSchemaHandle

    addDefault schemaRep vertexName valueBytes =
        withSchemaHandle schemaRep $ \sh ->
            withSliceIn (encodeTextUtf8 vertexName) $ \vPtr vLen ->
                withSliceIn valueBytes $ \ePtr eLen ->
                    callHandleOut (pp_schema_add_default_at sh vPtr vLen ePtr eLen)
                        >>= consumeSchemaHandle

    addMerger schemaRep vertexName spec =
        addAnnotation pp_schema_add_merger_at schemaRep vertexName (encodeMergerSpec spec)

    addPolicy schemaRep vertexName spec =
        addAnnotation pp_schema_add_policy_at schemaRep vertexName (encodePolicySpec spec)

    refinementSubsort _ baseSort subConstraints superConstraints =
        withSliceIn (encodeTextUtf8 baseSort) $ \basePtr baseLen ->
            withSliceIn (encodePairs subConstraints) $ \subPtr subLen ->
                withSliceIn (encodePairs superConstraints) $ \superPtr superLen -> do
                    result <-
                        callScalarOut 0 $ \out ->
                            pp_enriched_refinement_subsort_at
                                basePtr
                                baseLen
                                subPtr
                                subLen
                                superPtr
                                superLen
                                out
                    pure (result /= (0 :: Word32))

-- ---------------------------------------------------------------------------
-- Handle bridging

-- | Run @action@ with a transient slab handle for @schemaRep@.
--
-- The handle is materialized by serializing the representation to its
-- 'CanonicalSchema' form and re-ingesting it through
-- 'Panproto.Rust.withRustSchema', which releases the transient slot when
-- @action@ returns (or throws). This sidesteps the unexported
-- @RustSchemaRep@ constructor while keeping the engine semantics intact:
-- the FFI sees a schema with identical content to @schemaRep@.
withSchemaHandle :: SchemaRep Rust -> (Word32 -> IO a) -> IO a
withSchemaHandle schemaRep action = do
    canonical <- toCanonicalSchema schemaRep
    withRustSchema canonical (\rs -> action rs.schemaHandle)

-- | Run @action@ with a transient slab handle for @protoRep@, releasing
-- the slot on the way out. See 'withSchemaHandle'.
withProtoHandle :: ProtocolRep Rust -> (Word32 -> IO a) -> IO a
withProtoHandle protoRep action = do
    canonical <- toCanonical protoRep
    withRustProtocol canonical (\rp -> action rp.handle)

-- | Take ownership of a fresh schema handle returned by an engine FFI
-- call, rewrapping it as a @'SchemaRep' 'Rust'@.
--
-- The engine returns a brand-new slab handle. It is serialized to its
-- 'CanonicalSchema' form, the engine handle is freed, and the bytes are
-- re-ingested through 'fromCanonicalSchema' so the returned
-- representation owns its own slot (which the caller releases via
-- 'releaseSchema' or 'Panproto.Rust.withRustSchema').
consumeSchemaHandle :: Word32 -> IO (SchemaRep Rust)
consumeSchemaHandle engineHandle =
    bracket (pure engineHandle) freeHandle $ \h -> do
        bs <- callVecOut (pp_schema_to_cbor h)
        fromCanonicalSchema (Proxy @Rust) (CanonicalSchema bs)
  where
    freeHandle h = pp_handle_free h >>= checkStatus

-- | Shared marshalling for the merger \/ policy annotation methods: each
-- takes a schema, a UTF-8 vertex name, and a CBOR spec payload, and
-- returns a fresh schema.
addAnnotation
    :: (Word32 -> Ptr Word8 -> CSize -> Ptr Word8 -> CSize -> Ptr Word32 -> IO CInt)
    -> SchemaRep Rust
    -> Text
    -> LBS.ByteString
    -> IO (SchemaRep Rust)
addAnnotation ffi schemaRep vertexName specBytes =
    withSchemaHandle schemaRep $ \sh ->
        withSliceIn (encodeTextUtf8 vertexName) $ \vPtr vLen ->
            withSliceIn specBytes $ \sPtr sLen ->
                callHandleOut (ffi sh vPtr vLen sPtr sLen) >>= consumeSchemaHandle

-- ---------------------------------------------------------------------------
-- Encoders shared across methods

-- | Encode 'Text' as its raw UTF-8 bytes. The @*_at@ glue treats UTF-8
-- argument slices as opaque byte spans, so no CBOR framing is added.
encodeTextUtf8 :: Text -> LBS.ByteString
encodeTextUtf8 = LBS.fromStrict . TE.encodeUtf8

-- | CBOR-encode a @[(Text, Text)]@ as the @Vec<(String, String)>@ the
-- refinement subsort check consumes: an array of two-element arrays.
encodePairs :: [(Text, Text)] -> LBS.ByteString
encodePairs pairs =
    CBOR.toLazyByteString $
        Enc.encodeListLen (fromIntegral (length pairs))
            <> foldMap encodePair pairs
  where
    encodePair (a, b) =
        Enc.encodeListLen 2 <> Enc.encodeString a <> Enc.encodeString b

-- | A 'PanprotoError' tagged @host_decode@ for when the CBOR returned by
-- an FFI call does not decode into the expected Haskell type.
hostDecodeError :: Text -> String -> PanprotoError
hostDecodeError site reason =
    PanprotoError
        { code = StatusSerialization
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusSerialization
                    , tag = "host_decode"
                    , message =
                        "panproto could not decode the CBOR returned by "
                            <> site
                            <> ": "
                            <> T.pack reason
                    }
        }
