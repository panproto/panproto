{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed dataset operations: the @'DataBackend' 'Rust'@ instance.
--
-- Implements the @data@ surface of @libpanproto_c@ (see
-- @crates\/panproto-c\/CONTRACT.md@'s @data@ domain, six entry points)
-- by dispatching to "Panproto.Rust.FFI" through the
-- "Panproto.Rust.Handle" combinators. The instance is an orphan by
-- design, matching the other @*Backend Rust@ instances: the 'Rust' tag
-- lives in "Panproto.Class", and each backend implementation lives in
-- its own module so it can be compiled out via cabal flags.
--
-- A data set is handle-backed (a @panproto_vcs::DataSetObject@ living in
-- the slab), so 'DataSetRep' 'Rust' is just the slab @u32@ rather than a
-- serialized value. The complement carrier a forward migration produces
-- is itself a data-set handle on the Rust side (its @data@ field holds
-- the CBOR @Vec<Complement>@); this binding does not surface that
-- carrier directly. Instead, 'migrateForward' returns both handles and
-- the caller threads the complement set explicitly into 'migrateBackward'
-- through the shared 'Panproto.Instance.Complement' value type, matching
-- the 'DataBackend' class shape.
--
-- Method-to-entry-point mapping:
--
-- * 'storeDataset' → @pp_data_store_dataset@ (@inst::parse_json@). The
--   raw JSON-array payload is pinned as a borrowed slice; the fresh
--   @DataSet@ handle is wrapped as 'RustDataSet'.
-- * 'getDataset' → @pp_data_get_dataset@. The engine returns a CBOR
--   @Vec<WInstance>@; this module splits the outer list into per-record
--   bytes (via 'Codec.CBOR.Term.decodeTerm', which round-trips arbitrary
--   CBOR losslessly), decodes each with 'decodeInstance', and ingests it
--   as an 'InstanceRep' 'Rust'.
-- * 'migrateForward' → @pp_data_migrate_forward@ (@lens::auto_generate@
--   then @lens::get@). Two fresh @DataSet@ handles out, threaded through
--   'callTwoHandlesOut': the migrated data set and the complement
--   carrier.
-- * 'migrateBackward' → @pp_data_migrate_backward@ (@lens::put@). The
--   complement set is CBOR-encoded with 'encodeComplements' and pinned
--   as a borrowed slice.
-- * 'checkStaleness' → @pp_data_check_staleness@. The CBOR
--   @{ stale, data_schema_id, target_schema_id }@ report is decoded with
--   'decodeStalenessReport'.
-- * 'getMigrationComplement' → @pp_data_get_migration_complement@. A
--   stateless @Vec<Complement>@ round-trip: encode in, decode out.
--
-- The data-set handle lifecycle is the caller's to manage: 'storeDataset',
-- 'migrateForward', and 'migrateBackward' hand back 'DataSetRep' 'Rust'
-- values that each own a slab slot, released through 'releaseDataSet'
-- (@pp_handle_free@) or the 'withDataSet' bracket.
module Panproto.Rust.Data
    ( RustDataSet (..)
    , dataSetHandle
    , withDataSet
    ) where

import Control.Exception (bracket, throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (Proxy))
import Data.Text qualified as T
import Data.Word (Word32)

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Term (Term, decodeTerm, encodeTerm)
import Codec.CBOR.Write qualified as CBOR

import Panproto.Class (Rust)
import Panproto.Data
    ( DataBackend (..)
    , decodeStalenessReport
    )
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Instance
    ( Instance
    , decodeComplements
    , decodeInstance
    , encodeComplements
    , ingestInstance
    )
import Panproto.Rust (schemaRepHandle)
import Panproto.Rust.FFI
    ( pp_data_check_staleness_at
    , pp_data_get_dataset
    , pp_data_get_migration_complement_at
    , pp_data_migrate_backward_at
    , pp_data_migrate_forward
    , pp_data_store_dataset_at
    , pp_handle_free
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callTwoHandlesOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )
import Panproto.Rust.Instance ()
import Panproto.Rust.Lens ()

-- | The data-set representation for the 'Rust' backend: an opaque slab
-- handle pointing at a @panproto_vcs::DataSetObject@ resource (the
-- output of @pp_data_store_dataset@, either migration direction, or a
-- forward migration's complement carrier). The Rust @DataSetObject@ is
-- handle-backed rather than a serializable value, so this is just the
-- @u32@.
newtype RustDataSet = RustDataSet Word32
    deriving stock (Eq, Show)

instance DataBackend Rust where
    newtype DataSetRep Rust = RustDataSetRep RustDataSet

    storeDataset schema json = do
        let sh = schemaRepHandle schema
        handle <-
            withSliceIn (LBS.fromStrict json) $ \ptr len ->
                callHandleOut (pp_data_store_dataset_at sh ptr len)
        pure (RustDataSetRep (RustDataSet handle))

    getDataset (RustDataSetRep (RustDataSet handle)) = do
        bs <- callVecOut (pp_data_get_dataset handle)
        case decodeInstances bs of
            Right instances -> traverse (ingestInstance (Proxy @Rust)) instances
            Left err -> throwIO (hostDecodeError "pp_data_get_dataset" err)

    migrateForward (RustDataSetRep (RustDataSet handle)) src tgt = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        (dataH, compH) <- callTwoHandlesOut (pp_data_migrate_forward handle sh th)
        pure
            ( RustDataSetRep (RustDataSet dataH)
            , RustDataSetRep (RustDataSet compH)
            )

    migrateBackward (RustDataSetRep (RustDataSet handle)) complements src tgt = do
        let sh = schemaRepHandle src
            th = schemaRepHandle tgt
        restoredH <-
            withSliceIn (encodeComplements complements) $ \ptr len ->
                callHandleOut (pp_data_migrate_backward_at handle ptr len sh th)
        pure (RustDataSetRep (RustDataSet restoredH))

    checkStaleness (RustDataSetRep (RustDataSet handle)) schema = do
        let sh = schemaRepHandle schema
        bs <- callVecOut (pp_data_check_staleness_at handle sh)
        case decodeStalenessReport bs of
            Right report -> pure report
            Left err -> throwIO (hostDecodeError "pp_data_check_staleness" err)

    getMigrationComplement _ complements = do
        bs <-
            withSliceIn (encodeComplements complements) $ \ptr len ->
                callVecOut (pp_data_get_migration_complement_at ptr len)
        case decodeComplements bs of
            Right cs -> pure cs
            Left err -> throwIO (hostDecodeError "pp_data_get_migration_complement" err)

    releaseDataSet (RustDataSetRep (RustDataSet handle)) =
        pp_handle_free handle >>= checkStatus

-- ---------------------------------------------------------------------------
-- Handle lifecycle

-- | Project the 'RustDataSet' slab handle out of a @'DataSetRep' 'Rust'@.
-- The @RustDataSetRep@ data-family constructor is not exported, so this
-- is how sibling code reaches the underlying handle for lifecycle
-- management or further FFI calls.
dataSetHandle :: DataSetRep Rust -> RustDataSet
dataSetHandle (RustDataSetRep ds) = ds

-- | Run @action@ with the data set produced by @acquire@, releasing its
-- slab slot on every exit path (including exceptions). Mirrors
-- 'Panproto.Rust.Migration.withCompiled'.
withDataSet :: IO (DataSetRep Rust) -> (DataSetRep Rust -> IO a) -> IO a
withDataSet acquire = bracket acquire releaseDataSet

-- ---------------------------------------------------------------------------
-- Instance-list decoding
--
-- @pp_data_get_dataset@ returns a CBOR @Vec<WInstance>@. "Panproto.Instance"
-- exports the whole-blob 'decodeInstance' but no list decoder for it (the
-- per-element decoder is private), so the outer list is split here: each
-- element is decoded as a generic 'Codec.CBOR.Term.Term', re-serialized,
-- and run back through 'decodeInstance'. A 'Term' round-trips arbitrary
-- CBOR losslessly (including byte strings and tuple-keyed maps), so this
-- split is faithful where routing through an aeson 'Value' would not be.

-- | Decode a CBOR @Vec<WInstance>@ into the list of structured
-- 'Instance' values, rejecting trailing bytes. Splits the list with the
-- generic-'Term' decoder, then runs the whole-blob 'decodeInstance' on
-- each element's re-encoded bytes.
decodeInstances :: LBS.ByteString -> Either String [Instance]
decodeInstances bs =
    case CBOR.deserialiseFromBytes termListDecoder bs of
        Left err -> Left (show err)
        Right (rest, terms)
            | LBS.null rest -> traverse decodeTermInstance terms
            | otherwise -> Left "trailing bytes after CBOR-encoded instance list"

-- | Decode the outer CBOR list as a sequence of opaque 'Term's. Accepts
-- both definite- and indefinite-length lists, matching the tolerant
-- decoder idiom of the other domains.
termListDecoder :: Decoder s [Term]
termListDecoder = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = (:) <$> decodeTerm <*> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> decodeTerm <*> goIndef

-- | Re-encode a single instance 'Term' and decode it as a structured
-- 'Instance' through the canonical whole-blob codec.
decodeTermInstance :: Term -> Either String Instance
decodeTermInstance = decodeInstance . CBOR.toLazyByteString . encodeTerm

-- ---------------------------------------------------------------------------
-- Helpers

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
