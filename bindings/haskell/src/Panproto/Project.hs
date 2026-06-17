{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE TypeFamilies #-}

-- | Multi-file project assembly: parse a tree of source files into a
-- single coproduct schema.
--
-- This is the value-type and capability-class layer for the @project@
-- surface of @panproto-c@ (see @crates\/panproto-c\/CONTRACT.md@'s
-- @project@ domain). Unlike the schema and instance surfaces, the
-- project surface has no pure value mirror of its working state: a
-- @panproto_project::ProjectBuilder@ is a /mutable handle/ in the C
-- ABI (@pp_project_add_file@ and @pp_project_add_directory@ mutate it
-- in place; @pp_project_build@ /consumes/ it to produce a
-- @ProjectSchema@), and @ProjectSchema@ is likewise an opaque handle
-- carrying the assembled coproduct schema plus per-file metadata.
-- Mirroring those as immutable Haskell ADTs would not match the ABI;
-- instead each is an associated data family ('ProjectBuilderRep' \/
-- 'ProjectSchemaRep') the backend fills with its own representation
-- (an opaque foreign handle for the Rust backend).
--
-- The one piece of project state that /does/ cross the boundary as a
-- plain value is the protocol map: @pp_project_protocol_map@ emits a
-- CBOR @HashMap<String, String>@ pairing each file path with the
-- protocol it was parsed under. 'ProtocolMap' is that value, with a
-- tolerant CBOR codec ('encodeProtocolMap' \/ 'decodeProtocolMap')
-- following the decoder idiom of "Panproto.Schema" and
-- "Panproto.Instance", and an aeson instance via the underlying
-- 'HashMap'.
--
-- The 'Rust' instance of 'ProjectBackend' is authored later (in
-- @Panproto.Rust.Project@); this module declares only the value types
-- and the class.
module Panproto.Project
    ( -- * Protocol map
      ProtocolMap (..)
    , emptyProtocolMap
    , protocolFor
    , filePaths
    , fileCount

      -- * Codecs
    , encodeProtocolMap
    , decodeProtocolMap

      -- * Capability class
    , ProjectBackend (..)
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
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Kind (Type)
import Data.List (sort)
import Data.Proxy (Proxy)
import Data.Text (Text)
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))

-- ---------------------------------------------------------------------------
-- Protocol map

-- | The file-path-to-protocol mapping a built 'ProjectSchemaRep'
-- carries: each source file path (relative to the project root) paired
-- with the protocol name it was parsed under (e.g. @"typescript"@,
-- @"python"@, @"rust"@). Mirrors the @protocol_map@ field of
-- @panproto_project::ProjectSchema@, which the C ABI marshals as a CBOR
-- @HashMap<String, String>@ (paths rendered via 'Path::display').
newtype ProtocolMap = ProtocolMap
    { entries :: HashMap Text Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A protocol map with no files.
emptyProtocolMap :: ProtocolMap
emptyProtocolMap = ProtocolMap HM.empty

-- | The protocol a given file path was parsed under, if the path is in
-- the map.
protocolFor :: ProtocolMap -> Text -> Maybe Text
protocolFor m path = HM.lookup path m.entries

-- | Every file path in the map, sorted for a stable order.
filePaths :: ProtocolMap -> [Text]
filePaths m = sort (HM.keys m.entries)

-- | The number of files in the project. Mirrors the Python
-- @ProjectBuilder.file_count@ \/ @ProjectSchema@ file tally.
fileCount :: ProtocolMap -> Int
fileCount m = HM.size m.entries

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations the @project@ surface of @panproto-c@ exposes (see
-- @CONTRACT.md@'s @project@ domain).
--
-- The shape is deliberately stateful to match the C ABI. A
-- 'ProjectBuilderRep' is a mutable handle: 'builderNew' allocates one,
-- 'addFile' and 'addDirectory' mutate it in place, and 'buildProject'
-- consumes it (the builder should not be reused afterward) to produce
-- a 'ProjectSchemaRep'. The schema rep is itself a handle from which
-- 'projectSchemaGet' extracts the assembled coproduct
-- 'Panproto.Class.SchemaRep' and 'projectProtocolMap' reads the
-- path-to-protocol mapping.
--
-- 'SchemaBackend' is a superclass because 'projectSchemaGet' returns a
-- 'SchemaRep' of the same backend: a project assembles into a schema
-- the rest of the backend's schema surface then operates on.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Project@);
-- this module declares only the class.
class SchemaBackend back => ProjectBackend back where
    -- | Backend-specific representation of a @ProjectBuilder@. A mutable
    -- handle: for 'Panproto.Class.Rust' an opaque foreign handle into
    -- the slab.
    data ProjectBuilderRep back :: Type

    -- | Backend-specific representation of a @ProjectSchema@. An opaque
    -- handle carrying the assembled coproduct schema and per-file
    -- metadata.
    data ProjectSchemaRep back :: Type

    -- | Allocate a fresh, empty project builder. Wraps
    -- @pp_project_builder_new@ (@ProjectBuilder::new@).
    builderNew :: Proxy back -> IO (ProjectBuilderRep back)

    -- | Add a single file to the builder, given its path (relative to
    -- the project root) and raw content. Mutates the builder in place.
    -- Wraps @pp_project_add_file@ (@ProjectBuilder::add_file@).
    addFile :: ProjectBuilderRep back -> Text -> ByteString -> IO ()

    -- | Add every file under a directory recursively, given the
    -- directory path. Mutates the builder in place. Wraps
    -- @pp_project_add_directory@ (@ProjectBuilder::add_directory@).
    addDirectory :: ProjectBuilderRep back -> Text -> IO ()

    -- | Assemble the builder into a project schema via coproduct
    -- construction. /Consumes/ the builder (it should not be used
    -- afterward). Wraps @pp_project_build@ (@ProjectBuilder::build@).
    buildProject :: ProjectBuilderRep back -> IO (ProjectSchemaRep back)

    -- | Extract the assembled coproduct schema from a built project.
    -- Wraps @pp_project_schema_get@.
    projectSchemaGet :: ProjectSchemaRep back -> IO (SchemaRep back)

    -- | Read the file-path-to-protocol mapping from a built project.
    -- Wraps @pp_project_protocol_map@.
    projectProtocolMap :: ProjectSchemaRep back -> IO (HashMap Text Text)

    -- | Convenience: scan a directory and build it into a project
    -- schema in one step. Equivalent to 'builderNew', 'addDirectory',
    -- then 'buildProject'. Mirrors the Python @parse_project@.
    parseProject :: Proxy back -> Text -> IO (ProjectSchemaRep back)
    -- ^ The proxy fixes the backend; the 'Text' is the directory path.
    parseProject p dir = do
        builder <- builderNew p
        addDirectory builder dir
        buildProject builder

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'ProtocolMap' to the CBOR @HashMap<String, String>@ shape
-- @ciborium@ deserializes (a plain string-keyed map).
encodeProtocolMap :: ProtocolMap -> LBS.ByteString
encodeProtocolMap (ProtocolMap m) =
    CBOR.toLazyByteString $
        Enc.encodeMapLen (fromIntegral (HM.size m))
            <> HM.foldMapWithKey
                (\k v -> Enc.encodeString k <> Enc.encodeString v)
                m

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @HashMap<String, String>@ bytes (the
-- @pp_project_protocol_map@ output) into a 'ProtocolMap'. Tolerant of
-- definite- or indefinite-length maps; fails on trailing bytes or
-- malformed input.
decodeProtocolMap :: LBS.ByteString -> Either String ProtocolMap
decodeProtocolMap bs =
    case CBOR.deserialiseFromBytes protocolMapDecoder bs of
        Left err -> Left (show err)
        Right (rest, m)
            | LBS.null rest -> Right m
            | otherwise -> Left "trailing bytes after CBOR-encoded protocol map"

protocolMapDecoder :: Decoder s ProtocolMap
protocolMapDecoder = ProtocolMap . HM.fromList <$> go
  where
    go = do
        mapLen <- Dec.decodeMapLenOrIndef
        case mapLen of
            Just n -> goN n
            Nothing -> goIndef
    goN 0 = pure []
    goN n = do
        k <- Dec.decodeString
        v <- Dec.decodeString
        ((k, v) :) <$> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                k <- Dec.decodeString
                v <- Dec.decodeString
                ((k, v) :) <$> goIndef
