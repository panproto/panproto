{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE TypeFamilies #-}

-- | Data-set versioning and data-level migration.
--
-- A /data set/ is a snapshot of records that conform to a schema: the
-- @inst::parse_json@ of a JSON array, kept as a @Vec<WInstance>@ bound
-- to the object id of the schema it was parsed against. On the Rust
-- side this is @panproto_vcs::DataSetObject@ (a @schema_id@, the
-- `MessagePack`-encoded instances, and a record count); the C ABI's
-- @data@ domain (see @crates\/panproto-c\/CONTRACT.md@) keeps each data
-- set in the slab and hands callers an opaque @u32@ handle rather than
-- a serialized value.
--
-- This module mirrors that handle-backed shape. The data set itself is
-- /not/ a Haskell value type: it is an associated 'DataSetRep' on the
-- 'DataBackend' capability class, an opaque foreign handle for 'Rust'
-- or a thin in-memory carrier for a future native backend. The only
-- value type is the small 'StalenessReport' that
-- @pp_data_check_staleness@ emits.
--
-- == The six operations
--
-- 'DataBackend' declares the @data@ domain's six entry points as plain
-- 'IO' actions over the backend's 'DataSetRep':
--
-- * 'storeDataset' parses a JSON array against a schema, calling
--   @inst::parse_json@ per record, and stores the result
--   (@pp_data_store_dataset@).
-- * 'getDataset' materializes the stored records as 'InstanceRep's
--   (@pp_data_get_dataset@, CBOR @Vec<WInstance>@).
-- * 'migrateForward' auto-generates a lens between two schemas and runs
--   @lens::get@ per record, producing the migrated data set and a
--   parallel complement set (@pp_data_migrate_forward@).
-- * 'migrateBackward' runs @lens::put@ per record against a stored
--   complement set, reconstructing the source data set
--   (@pp_data_migrate_backward@).
-- * 'checkStaleness' compares the data set's bound schema id against a
--   target schema's id (@pp_data_check_staleness@).
-- * 'getMigrationComplement' round-trips a complement set for
--   validation (@pp_data_get_migration_complement@).
--
-- 'DataBackend' refines both 'LensBackend' (forward and backward
-- migration auto-generate and run a lens) and 'InstanceBackend'
-- (records are 'InstanceRep's), so it transitively refines
-- 'Panproto.Class.SchemaBackend': every data-set operation is anchored
-- to a schema. The 'Complement' threading 'migrateForward' to
-- 'migrateBackward' is the shared 'Panproto.Instance.Complement' value
-- type, carried as the @Vec<Complement>@ the @data@ domain marshals.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Data@); this
-- module declares only the class and the report value type. The codec
-- and aeson bridge for 'StalenessReport' follow the tolerant decoder
-- idiom of "Panproto.Vcs" and "Panproto.Instance": snake_case keys,
-- @serde(default)@ for absent fields, and a depth-first unknown-term
-- skipper for forward compatibility.
module Panproto.Data
    ( -- * Staleness report
      StalenessReport (..)
    , freshReport

      -- * Staleness report codecs
    , encodeStalenessReport
    , decodeStalenessReport

      -- * Capability class
    , DataBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString (ByteString)
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))
import Panproto.Instance (Complement, InstanceBackend (..))
import Panproto.Lens (LensBackend)

-- ---------------------------------------------------------------------------
-- Staleness report

-- | The result of a staleness check: whether a stored data set's schema
-- still matches a target schema, with both schema ids for diagnosis.
-- Mirrors the CBOR @{ stale, data_schema_id, target_schema_id }@ that
-- @pp_data_check_staleness@ emits.
--
-- A data set is /stale/ exactly when its bound schema id differs from
-- the target schema id: the data was parsed against one schema and the
-- caller is asking whether it conforms to another. The two ids are the
-- hex renderings of the respective @panproto_vcs::ObjectId@s (blake3
-- hashes), matching the @schema_id.to_string()@ the Rust side writes.
data StalenessReport = StalenessReport
    { stale :: !Bool
    -- ^ @serde@ field: @stale@. 'True' when the data set's schema id
    -- differs from the target's.
    , dataSchemaId :: !Text
    -- ^ @serde@ field: @data_schema_id@. Hex id of the schema the data
    -- set was parsed against.
    , targetSchemaId :: !Text
    -- ^ @serde@ field: @target_schema_id@. Hex id of the schema the
    -- staleness was checked against.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A report for a data set that is up to date against the given schema
-- id: not stale, both ids equal. The @serde(default)@ seed for the
-- decoder.
freshReport :: Text -> StalenessReport
freshReport schemaId =
    StalenessReport
        { stale = False
        , dataSchemaId = schemaId
        , targetSchemaId = schemaId
        }

-- ---------------------------------------------------------------------------
-- Staleness report codecs

-- | Encode a 'StalenessReport' to the CBOR map @ciborium@ deserializes:
-- a three-key map with the @serde@ snake_case field names.
encodeStalenessReport :: StalenessReport -> LBS.ByteString
encodeStalenessReport r =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 3
            <> kv "stale" (Enc.encodeBool r.stale)
            <> kv "data_schema_id" (Enc.encodeString r.dataSchemaId)
            <> kv "target_schema_id" (Enc.encodeString r.targetSchemaId)
  where
    kv k v = Enc.encodeString k <> v

-- | Decode the CBOR @{ stale, data_schema_id, target_schema_id }@ a
-- staleness check emits. Tolerant of unknown fields and missing
-- optional fields, following the decoder idiom of "Panproto.Vcs".
decodeStalenessReport :: LBS.ByteString -> Either String StalenessReport
decodeStalenessReport bs =
    case CBOR.deserialiseFromBytes reportDecoder bs of
        Left err -> Left (show err)
        Right (rest, r)
            | LBS.null rest -> Right r
            | otherwise -> Left "trailing bytes after CBOR-encoded staleness report"

reportDecoder :: Decoder s StalenessReport
reportDecoder = decodeMap (freshReport T.empty) $ \acc key -> case key of
    "stale" -> (\v -> acc {stale = v}) <$> Dec.decodeBool
    "data_schema_id" -> (\v -> acc {dataSchemaId = v}) <$> Dec.decodeString
    "target_schema_id" -> (\v -> acc {targetSchemaId = v}) <$> Dec.decodeString
    _ -> acc <$ skipTerm

-- ---------------------------------------------------------------------------
-- Capability class

-- | The @data@ surface of @panproto-c@ (see @CONTRACT.md@'s @data@
-- domain, six entries). A data set is handle-backed, so the operations
-- are plain 'IO' actions over a backend-specific 'DataSetRep' rather
-- than over a serialized value.
--
-- 'LensBackend' and 'InstanceBackend' are superclasses (and
-- 'Panproto.Class.SchemaBackend' transitively): forward and backward
-- migration auto-generate and run a lens, records materialize as
-- 'InstanceRep's, and every store or migration is anchored to a
-- 'Panproto.Class.SchemaRep'. The 'Complement' set threading
-- 'migrateForward' to 'migrateBackward' is the shared
-- 'Panproto.Instance.Complement' value type.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Data@); this
-- module declares only the class.
class (LensBackend back, InstanceBackend back) => DataBackend back where
    -- | Backend-specific representation of a stored data set. For 'Rust'
    -- an opaque foreign handle (a slab @u32@ wrapping a
    -- @panproto_vcs::DataSetObject@); for a native backend an in-memory
    -- carrier around the parsed records and their schema id.
    data DataSetRep back :: Type

    -- | Store a data set from a JSON array of records, binding it to a
    -- schema. Each record is parsed with @inst::parse_json@ against the
    -- schema's inferred root vertex. The 'ByteString' is the raw JSON
    -- array (a single object is treated as a one-element array, matching
    -- the Rust surface). Wraps @pp_data_store_dataset@
    -- (@inst::parse_json@).
    storeDataset :: SchemaRep back -> ByteString -> IO (DataSetRep back)
    -- ^ Schema to bind to, raw JSON-array payload.

    -- | Materialize the stored records as backend instance
    -- representations. The C ABI returns CBOR @Vec<WInstance>@; this
    -- binding reifies each into an 'InstanceRep' (bridged to the shared
    -- 'Panproto.Instance.Instance' value through
    -- 'Panproto.Instance.reifyInstance'), so callers stay in the
    -- handle-backed instance surface rather than handling raw bytes.
    -- Wraps @pp_data_get_dataset@.
    getDataset :: DataSetRep back -> IO [InstanceRep back]

    -- | Migrate a data set forward between two schemas. Auto-generates a
    -- lens from the source to the target schema and runs @lens::get@ per
    -- record, returning the migrated data set and a parallel data set of
    -- the per-record 'Complement's that @put@ needs for the backward
    -- direction. Wraps @pp_data_migrate_forward@ (@lens::auto_generate@
    -- then @lens::get@).
    migrateForward
        :: DataSetRep back
        -> SchemaRep back
        -> SchemaRep back
        -> IO (DataSetRep back, DataSetRep back)
    -- ^ Source data set, source schema, target schema; yields the
    -- migrated data set and the complement set.

    -- | Migrate a data set backward, reconstructing the source. Runs
    -- @lens::put@ per record against the supplied complement set (paired
    -- positionally with the data set's records), using a lens
    -- auto-generated between the same two schemas. Wraps
    -- @pp_data_migrate_backward@ (@lens::put@).
    migrateBackward
        :: DataSetRep back
        -> [Complement]
        -> SchemaRep back
        -> SchemaRep back
        -> IO (DataSetRep back)
    -- ^ Migrated data set, complement set, source schema, target schema;
    -- yields the reconstructed source data set.

    -- | Check whether a data set is stale against a schema: whether the
    -- schema it was parsed under still matches the given schema. Wraps
    -- @pp_data_check_staleness@.
    checkStaleness :: DataSetRep back -> SchemaRep back -> IO StalenessReport

    -- | Round-trip a complement set through the @data@ domain's CBOR
    -- @Vec<Complement>@ form, validating that it deserializes and
    -- re-serializes cleanly. Takes a 'Data.Proxy.Proxy' to pin the
    -- backend, since the operation is a stateless validation that holds
    -- no data-set handle. Wraps @pp_data_get_migration_complement@.
    getMigrationComplement :: Proxy back -> [Complement] -> IO [Complement]

    -- | Release the resources held by a data-set representation.
    -- Idempotent at the slab level, as with the other backend reps.
    releaseDataSet :: DataSetRep back -> IO ()

-- ---------------------------------------------------------------------------
-- Shared CBOR plumbing
--
-- The same tolerant map fold and unknown-term skipper as "Panproto.Vcs"
-- and "Panproto.Instance": seed from a @serde(default)@ value, fold each
-- key through a handler, skip unknown fields, and accept both definite-
-- and indefinite-length maps.

-- | Decode a CBOR map into a record accumulator, folding each
-- @(key, value)@ pair through @step@ and skipping unrecognized keys.
decodeMap :: a -> (a -> Text -> Decoder s a) -> Decoder s a
decodeMap initial step = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = readPair acc >>= goN (n - 1)
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else readPair acc >>= goIndef
    readPair acc = do
        key <- Dec.decodeString
        step acc key

-- | Skip an arbitrary CBOR value, descending into nested arrays and
-- maps so an unknown field with structured contents does not desync the
-- surrounding decoder. Mirrors @Panproto.Vcs.skipTerm@.
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
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> skipBytesIndef
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> skipStringIndef
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        Dec.TypeBreak -> () <$ Dec.decodeBreakOr
        Dec.TypeInvalid -> fail "skipTerm: invalid CBOR token"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs
    skipBytesIndef = do
        Dec.decodeBytesIndef
        skipUntilBreakBytes
    skipStringIndef = do
        Dec.decodeStringIndef
        skipUntilBreakStrings
    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes
    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings
