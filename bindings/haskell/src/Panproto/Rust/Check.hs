{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed diffing and compatibility classification.
--
-- Implements @instance CheckBackend Rust@ (the class lives in
-- "Panproto.Check"). Every method dispatches to @libpanproto_c@\'s
-- @check@ domain via "Panproto.Rust.FFI", turning status codes into
-- 'Panproto.Errors.PanprotoError' exceptions and decoding the CBOR
-- payloads with the codecs from "Panproto.Check".
--
-- The @CheckBackend Rust@ instance is an orphan by design, matching the
-- @ProtocolBackend Rust@ \/ @SchemaBackend Rust@ instances in
-- "Panproto.Rust": the 'Rust' tag lives in "Panproto.Class", and the
-- backend implementation lives in its own module so it can be compiled
-- out via cabal flags.
--
-- Method-to-entry-point mapping:
--
-- * 'diffSchemas' → @pp_check_diff_full@ (@check::diff@), decoded via
--   'decodeSchemaDiff'.
-- * 'diffAndClassify' → @pp_check_diff_full@ then @pp_check_classify@
--   (@check::classify@); the CBOR diff bytes are forwarded to the
--   classifier without a Haskell-side round-trip, and the report is
--   decoded via 'decodeCompatReport'.
-- * 'reportText' → @pp_check_report_text@ (@check::report_text@); the
--   report is encoded via 'encodeCompatReport' and the UTF-8 text bytes
--   are decoded back into 'Text'.
-- * 'reportJson' → @pp_check_report_json@ (@check::report_json@), same
--   marshalling as 'reportText' but the bytes carry a JSON document.
--
-- The slab handle inside @SchemaRep Rust@ \/ @ProtocolRep Rust@ is
-- reached through the canonical bridge ('withSchemaHandle' \/
-- 'withProtocolHandle'): the @RustSchemaRep@ \/ @RustProtocolRep@
-- constructors are not exported from "Panproto.Rust", so this module
-- re-ingests the canonical bytes via the exported 'withRustSchema' \/
-- 'withRustProtocol' brackets to obtain a handle it can hand to the FFI
-- calls. The brackets release the borrowed handles on every exit path.
module Panproto.Rust.Check () where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word8, Word32)
import Foreign.C.Types (CInt, CSize)
import Foreign.Ptr (Ptr)

import Panproto.Check
    ( CheckBackend (..)
    , CompatReport
    , decodeCompatReport
    , decodeSchemaDiff
    , encodeCompatReport
    )
import Panproto.Class
    ( ProtocolBackend (toCanonical)
    , ProtocolRep
    , Rust
    , SchemaBackend (toCanonicalSchema)
    , SchemaRep
    )
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Rust
    ( RustProtocol (..)
    , RustSchema (..)
    , withRustProtocol
    , withRustSchema
    )
import Panproto.Rust.FFI
    ( VecU8
    , pp_check_classify_at
    , pp_check_diff_full
    , pp_check_report_json_at
    , pp_check_report_text_at
    )
import Panproto.Rust.Handle (callVecOut, withSliceIn)

instance CheckBackend Rust where
    diffSchemas s1 s2 =
        withSchemaHandle s1 $ \h1 ->
            withSchemaHandle s2 $ \h2 -> do
                bs <- diffFullBytes h1 h2
                case decodeSchemaDiff bs of
                    Right d -> pure d
                    Left err -> throwIO $ hostDecodeError "pp_check_diff_full" err

    diffAndClassify s1 s2 proto =
        withSchemaHandle s1 $ \h1 ->
            withSchemaHandle s2 $ \h2 ->
                withProtocolHandle proto $ \ph -> do
                    diffBytes <- diffFullBytes h1 h2
                    report <- classifyBytes ph diffBytes
                    case decodeCompatReport report of
                        Right r -> pure r
                        Left err -> throwIO $ hostDecodeError "pp_check_classify" err

    reportText _ = renderReport "pp_check_report_text" pp_check_report_text_at

    reportJson _ = renderReport "pp_check_report_json" pp_check_report_json_at

-- ---------------------------------------------------------------------------
-- Handle accessors
--
-- @SchemaRep Rust@ wraps a 'RustSchema' (and @ProtocolRep Rust@ a
-- 'RustProtocol'), but those wrapper constructors are not exported from
-- "Panproto.Rust", so the live slab handle cannot be pattern-matched out
-- directly. Round-trip through the canonical bytes and re-ingest via the
-- exported bracket helpers, which expose the handle through the
-- 'RustSchema' \/ 'RustProtocol' accessors and guarantee release.

-- | Borrow a slab handle for a 'SchemaRep' 'Rust' for the duration of
-- @action@, releasing it afterwards (including on exceptions).
withSchemaHandle :: SchemaRep Rust -> (Word32 -> IO a) -> IO a
withSchemaHandle rep action = do
    canonical <- toCanonicalSchema rep
    withRustSchema canonical (\(RustSchema h) -> action h)

-- | Borrow a slab handle for a 'ProtocolRep' 'Rust' for the duration of
-- @action@, releasing it afterwards (including on exceptions).
withProtocolHandle :: ProtocolRep Rust -> (Word32 -> IO a) -> IO a
withProtocolHandle rep action = do
    canonical <- toCanonical rep
    withRustProtocol canonical (\(RustProtocol h) -> action h)

-- ---------------------------------------------------------------------------
-- FFI bridges

-- | Run @pp_check_diff_full@ over two schema handles and return the raw
-- CBOR @check::SchemaDiff@ bytes.
diffFullBytes :: Word32 -> Word32 -> IO LBS.ByteString
diffFullBytes h1 h2 = callVecOut (pp_check_diff_full h1 h2)

-- | Run @pp_check_classify@ with a protocol handle and CBOR diff bytes,
-- returning the raw CBOR @check::CompatReport@ bytes.
classifyBytes :: Word32 -> LBS.ByteString -> IO LBS.ByteString
classifyBytes proto diffBytes =
    withSliceIn diffBytes $ \ptr len ->
        callVecOut (pp_check_classify_at proto ptr len)

-- | Encode a 'CompatReport' to CBOR, hand it to a report renderer FFI
-- call (@pp_check_report_text@ \/ @pp_check_report_json@), and decode
-- the UTF-8 result bytes into 'Text'.
renderReport
    :: String
    -> (Ptr Word8 -> CSize -> Ptr VecU8 -> IO CInt)
    -> CompatReport
    -> IO Text
renderReport site call report =
    withSliceIn (encodeCompatReport report) $ \ptr len -> do
        bs <- callVecOut (call ptr len)
        case TE.decodeUtf8' (LBS.toStrict bs) of
            Right t -> pure t
            Left unicodeErr -> throwIO $ hostDecodeError site (show unicodeErr)

-- ---------------------------------------------------------------------------
-- Errors

-- | Wrap a host-side decode failure (malformed CBOR back from the
-- engine, or non-UTF-8 report bytes) as a 'PanprotoError' so callers see
-- the same exception shape as a boundary status failure.
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
