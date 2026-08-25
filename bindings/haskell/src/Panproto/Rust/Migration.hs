{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed migration operations: the @'MigrationBackend' 'Rust'@
-- instance.
--
-- Implements the @mig@ surface of @libpanproto_c@ (see
-- @crates\/panproto-c\/CONTRACT.md@'s @mig@ domain, seven entry points)
-- by dispatching to "Panproto.Rust.FFI" through the
-- "Panproto.Rust.Handle" combinators. The instance is an orphan by
-- design, matching the @ProtocolBackend Rust@ \/ @SchemaBackend Rust@
-- instances in "Panproto.Rust": the 'Rust' tag lives in
-- "Panproto.Class", and each backend implementation lives in its own
-- module so it can be compiled out via cabal flags.
--
-- Method-to-entry-point mapping:
--
-- * 'compile' → @pp_mig_compile@ (@mig::compile@). The 'Migration' spec
--   is CBOR-encoded via 'encodeMigration'; the source and target schema
--   slab handles are read out of the 'SchemaRep' 'Rust' values with
--   'schemaRepHandle'. The fresh @MigrationWithSchemas@ handle is
--   wrapped as 'RustCompiled'.
-- * 'checkExistence' → @pp_mig_check_existence@ (@mig::check_existence@).
--   The engine returns a CBOR @ExistenceReport@; this module decodes it
--   and returns its rendered error messages (empty when the report is
--   valid).
-- * 'liftRecord' → @pp_mig_lift_record@ (@mig::lift_wtype@). The input
--   record is reified to an "Panproto.Instance" 'Instance', CBOR-encoded
--   with 'encodeInstance', and the result is decoded with
--   'decodeInstance' and re-ingested.
-- * 'composeMigrations' → @pp_mig_compose@ (@helpers::compose_compiled@).
--   Two compiled handles in, one fresh bare-@Migration@ handle out.
-- * 'invertMigration' → @pp_mig_invert@ (@mig::invert@). CBOR
--   'Migration' in and out, decoded with 'decodeMigration'.
-- * 'checkCoverage' → built from repeated @pp_mig_lift_record@ calls
--   (see the method note for why the bundled-schema @pp_mig_coverage@
--   entry point is not used here).
-- * 'liftJson' → @pp_mig_lift_json@ (@inst::parse_json@ →
--   @mig::lift_wtype@ → @inst::to_json@). Raw JSON and a UTF-8 root
--   vertex in, raw JSON out.
--
-- The compiled-migration handle lifecycle is the caller's to manage:
-- 'compile' and 'composeMigrations' hand back a 'RustCompiled' that
-- owns a slab slot, released through 'releaseCompiled'
-- (@pp_handle_free@) or the 'withCompiled' bracket.
module Panproto.Rust.Migration
    ( RustCompiled (..)
    , adoptCompiled
    , rustCompiled
    , releaseCompiled
    , withCompiled
    ) where

import Control.Exception (bracket, throwIO, try)
import Data.Aeson (Value)
import Data.Aeson qualified as Aeson
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (Proxy))
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , SomePanprotoError
    , statusToInt
    )
import Panproto.Instance
    ( InstanceBackend (InstanceRep)
    , decodeInstance
    , encodeInstance
    , ingestInstance
    , reifyInstance
    )
import Panproto.Json (valueDecoder)
import Panproto.Migration
    ( MigrationBackend (..)
    , decodeMigration
    , encodeMigration
    )
import Panproto.Rust (protocolRepHandle, schemaRepHandle)
import Panproto.Rust.Instance ()
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_mig_check_existence_at
    , pp_mig_compile_at
    , pp_mig_compose
    , pp_mig_invert_at
    , pp_mig_lift_json_at
    , pp_mig_lift_record_at
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

-- | The compiled-migration representation for the 'Rust' backend: an
-- opaque slab handle pointing at a @MigrationWithSchemas@ resource (the
-- output of @pp_mig_compile@) or, after 'composeMigrations', a bare
-- @Migration@ resource. The Rust @CompiledMigration@ is handle-backed
-- rather than a serializable value, so this is just the @u32@.
newtype RustCompiled = RustCompiled Word32
    deriving stock (Eq, Show)

instance MigrationBackend Rust where
    newtype CompiledRep Rust = RustCompiledRep RustCompiled

    compile spec src tgt = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        handle <-
            withSliceIn (encodeMigration spec) $ \ptr len ->
                callHandleOut (pp_mig_compile_at sh th ptr len)
        pure (RustCompiledRep (RustCompiled handle))

    checkExistence spec proto src tgt = do
        let ph = protocolRepHandle proto
            sh = schemaRepHandle src
            th = schemaRepHandle tgt
        bs <-
            withSliceIn (encodeMigration spec) $ \ptr len ->
                callVecOut (pp_mig_check_existence_at ph sh th ptr len)
        case decodeExistenceMessages bs of
            Right msgs -> pure msgs
            Left err -> throwIO (hostDecodeError "pp_mig_check_existence" err)

    liftRecord (RustCompiledRep (RustCompiled handle)) recordRep = do
        record <- reifyInstance recordRep
        bs <-
            withSliceIn (encodeInstance record) $ \ptr len ->
                callVecOut (pp_mig_lift_record_at handle ptr len)
        case decodeInstance bs of
            Right i -> ingestInstance (Proxy @Rust) i
            Left err -> throwIO (hostDecodeError "pp_mig_lift_record" err)

    composeMigrations (RustCompiledRep (RustCompiled h1)) (RustCompiledRep (RustCompiled h2)) = do
        handle <- callHandleOut (pp_mig_compose h1 h2)
        pure (RustCompiledRep (RustCompiled handle))

    invertMigration spec src tgt = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        bs <-
            withSliceIn (encodeMigration spec) $ \ptr len ->
                callVecOut (pp_mig_invert_at ptr len sh th)
        case decodeMigration bs of
            Right m -> pure m
            Left err -> throwIO (hostDecodeError "pp_mig_invert" err)

    -- The frozen @pp_mig_coverage@ entry point takes explicit source and
    -- target schema handles, but the 'checkCoverage' method carries only
    -- the compiled migration and the records (the schemas are bundled
    -- inside the @MigrationWithSchemas@ slot, unreachable from here as
    -- separate handles). Coverage is therefore computed host-side by
    -- lifting each record through @pp_mig_lift_record@ (which reads the
    -- bundled schemas via @extract_migration_ref@) and tallying the
    -- outcomes, yielding the same total\/succeeded\/failed report lines.
    checkCoverage compiled records = do
        let total = length records
        outcomes <- traverse (tryLift compiled) records
        let failures = [msg | Left msg <- outcomes]
            failed = length failures
            succeeded = total - failed
        pure $
            [ "total: " <> tshow total
            , "succeeded: " <> tshow succeeded
            , "failed: " <> tshow failed
            , "coverage_percent: " <> tshow (coveragePercent total succeeded)
            ]
                <> zipWith
                    (\i msg -> "record " <> tshow (i :: Int) <> ": " <> msg)
                    [0 ..]
                    (take 20 failures)

    liftJson (RustCompiledRep (RustCompiled handle)) rootVertex jsonPayload = do
        -- FFI argument order is (migration, json, root_vertex); the
        -- method takes (compiled, rootVertex, jsonPayload), so the two
        -- text slices are pinned in the swapped order here.
        bs <-
            withSliceIn (utf8 jsonPayload) $ \jsonPtr jsonLen ->
                withSliceIn (utf8 rootVertex) $ \rootPtr rootLen ->
                    callVecOut (pp_mig_lift_json_at handle jsonPtr jsonLen rootPtr rootLen)
        case TE.decodeUtf8' (LBS.toStrict bs) of
            Right t -> pure t
            Left unicodeErr -> throwIO (hostDecodeError "pp_mig_lift_json" (show unicodeErr))

-- ---------------------------------------------------------------------------
-- Handle lifecycle

-- | Adopt a slab handle returned by an FFI operation as a compiled
-- migration representation. The returned value owns the handle and must
-- eventually be passed to 'releaseCompiled' (usually through
-- 'withCompiled').
adoptCompiled :: RustCompiled -> CompiledRep Rust
adoptCompiled = RustCompiledRep

-- | Project the 'RustCompiled' slab handle out of a @'CompiledRep'
-- 'Rust'@ (the value 'compile' and 'composeMigrations' return). The
-- @RustCompiledRep@ data-family constructor is not exported, so this is
-- the way sibling code reaches the underlying handle for lifecycle
-- management.
rustCompiled :: CompiledRep Rust -> RustCompiled
rustCompiled (RustCompiledRep c) = c

-- | Release the slab slot a @'CompiledRep' 'Rust'@ owns
-- (@pp_handle_free@). Idempotent at the slab level: freeing an
-- already-freed handle is a no-op on the Rust side.
releaseCompiled :: CompiledRep Rust -> IO ()
releaseCompiled (RustCompiledRep (RustCompiled handle)) =
    pp_handle_free handle >>= checkStatus

-- | Run @action@ with the compiled migration produced by @acquire@,
-- releasing its slab slot on every exit path (including exceptions).
withCompiled :: IO (CompiledRep Rust) -> (CompiledRep Rust -> IO a) -> IO a
withCompiled acquire = bracket acquire releaseCompiled

-- ---------------------------------------------------------------------------
-- Coverage helpers

-- | Lift a single record through a compiled migration, capturing a
-- per-record failure as a rendered message rather than propagating the
-- exception. Mirrors the per-record @lift_wtype@ loop of
-- @pp_mig_coverage@\'s Rust implementation.
tryLift :: CompiledRep Rust -> InstanceRep Rust -> IO (Either Text ())
tryLift compiled record = do
    result <- try (liftRecord compiled record) :: IO (Either SomePanprotoError (InstanceRep Rust))
    pure $ case result of
        Right _ -> Right ()
        Left e -> Left (tshow e)

-- | Percentage of records that lifted successfully. An empty batch is
-- reported as fully covered, matching the Rust coverage report.
coveragePercent :: Int -> Int -> Double
coveragePercent total succeeded
    | total > 0 = (fromIntegral succeeded / fromIntegral total) * 100.0
    | otherwise = 100.0

-- ---------------------------------------------------------------------------
-- Existence report decoding

-- | Decode a CBOR @mig::ExistenceReport@ (a map with a @valid@ boolean
-- and an @errors@ array) into the rendered error messages: an empty
-- list when the report is valid, otherwise one JSON-rendered line per
-- structured @ExistenceError@. Rejects trailing bytes, matching the
-- canonical decoder contract.
decodeExistenceMessages :: LBS.ByteString -> Either String [Text]
decodeExistenceMessages bs =
    case CBOR.deserialiseFromBytes existenceReportDecoder bs of
        Left err -> Left (show err)
        Right (rest, msgs)
            | LBS.null rest -> Right msgs
            | otherwise -> Left "trailing bytes after CBOR-encoded existence report"

-- | Decode the @{ valid, errors }@ report map. The @errors@ entries are
-- @ExistenceError@ values; rather than mirror the @#[non_exhaustive]@
-- variant taxonomy, each is decoded as a generic JSON 'Value' and
-- rendered, so a future variant still produces a readable line. When
-- @valid@ is true the messages are dropped (an empty list signals a
-- well-formed migration).
existenceReportDecoder :: Decoder s [Text]
existenceReportDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    (valid, errors) <- case mapLen of
        Just n -> readEntries n (False, [])
        Nothing -> readEntriesIndef (False, [])
    pure (if valid then [] else map renderError errors)
  where
    readEntries 0 acc = pure acc
    readEntries n acc = readEntry acc >>= readEntries (n - 1 :: Int)
    readEntriesIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else readEntry acc >>= readEntriesIndef

    readEntry (valid, errors) = do
        key <- Dec.decodeString
        case key of
            "valid" -> (\v -> (v, errors)) <$> Dec.decodeBool
            "errors" -> (\es -> (valid, es)) <$> decodeErrorList
            _ -> skipValue >> pure (valid, errors)

    decodeErrorList = do
        len <- Dec.decodeListLenOrIndef
        case len of
            Just n -> goN n
            Nothing -> goIndef
      where
        goN 0 = pure []
        goN n = (:) <$> valueDecoder <*> goN (n - 1 :: Int)
        goIndef = do
            stop <- Dec.decodeBreakOr
            if stop then pure [] else (:) <$> valueDecoder <*> goIndef

-- | Render a structured existence error 'Value' as a compact JSON line.
renderError :: Value -> Text
renderError = TE.decodeUtf8 . LBS.toStrict . Aeson.encode

-- | Skip an unknown report field while staying in sync, by decoding it
-- as a generic 'Value' and discarding it.
skipValue :: Decoder s ()
skipValue = () <$ valueDecoder

-- ---------------------------------------------------------------------------
-- Shared helpers

-- | Encode 'Text' as UTF-8 lazy bytes for a borrowed input slice. The
-- @*_at@ glue treats UTF-8 argument slices as opaque byte spans, so no
-- CBOR framing is added.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | 'show' into 'Text'.
tshow :: Show a => a -> Text
tshow = T.pack . show

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
