{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE TypeFamilies #-}

-- | Structured Haskell mirror of the panproto W-type instance.
--
-- A panproto instance is an attributed C-set: tree-shaped data
-- conforming to a schema. The C ABI carries instances across the FFI
-- boundary as a CBOR-encoded @panproto_inst::WInstance@ (the inst,
-- mig, lens, query, and data domains all marshal @WInstance@; see
-- @crates\/panproto-c\/CONTRACT.md@). This module mirrors that
-- semantic shape: a map of 'Node's keyed by numeric id, a list of
-- 'Arc's @(parent, child, edge)@, a list of hyper-edge 'Fan's, and a
-- 'root' anchored to a schema vertex.
--
-- The two precomputed indices on the Rust side (@parent_map@ and
-- @children_map@) are not stored as fields: they are derivable from
-- the arc set, so this module recomputes them on encode (the emitted
-- CBOR carries every field the Rust struct's @serde@ derive expects)
-- and exposes them as pure accessors ('parentOf', 'childrenOf').
--
-- 'Value' mirrors @panproto_inst::value::Value@, the free term
-- algebra of JSON-like leaf data, and 'FieldPresence' the
-- present\/null\/absent trichotomy of a node's value slot. 'NodeShape'
-- mirrors the structural-shape sum orthogonal to the schema anchor
-- (list source, XML aliased element, inline XML text run).
--
-- 'Complement' mirrors the lens @panproto_lens::Complement@: the data
-- discarded by a lens @get@ that @put@ needs to reconstruct the
-- source. The C ABI carries complements as CBOR @Complement@ (lens
-- domain) or @Vec<Complement>@ (data domain).
--
-- Codecs ('encodeInstance' \/ 'decodeInstance', 'encodeComplement' \/
-- 'decodeComplement', plus the @Vec<Complement>@ list pair) exchange
-- the CBOR shape @ciborium@ produces and consumes: snake_case keys,
-- @serde(default)@ for the optional fields, externally-tagged enums
-- for 'Value' \/ 'FieldPresence', the internally-tagged @{"kind": …}@
-- form for 'NodeShape', and unknown-field tolerance for forward
-- compatibility. They follow the tolerant decoder idiom of
-- "Panproto.Schema" and "Panproto.Errors": map-len-or-indef, key
-- dispatch, positional tuple accumulators (avoiding
-- @DuplicateRecordFields@ ambiguity), and a depth-first unknown-term
-- skipper.
module Panproto.Instance
    ( -- * Instance
      Instance (..)
    , emptyInstance

      -- * Nodes and values
    , Node (..)
    , emptyNode
    , NodeShape (..)
    , FieldPresence (..)
    , Value (..)
    , Arc
    , Fan (..)

      -- * Codecs
    , encodeInstance
    , decodeInstance
    , encodeComplement
    , decodeComplement
    , encodeComplements
    , decodeComplements

      -- * Instance accessors
    , nodeCount
    , arcCount
    , root
    , lookupNode
    , childrenOf
    , parentOf
    , elementCount

      -- * Complement
    , Complement (..)
    , emptyComplement
    , droppedNodeCount
    , droppedArcCount

      -- * Capability class
    , InstanceBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Int (Int64)
import Data.Kind (Type)
import Data.Maybe (fromMaybe)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32, Word64, Word8)
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))
import Panproto.Schema (Edge (..))

-- ---------------------------------------------------------------------------
-- Value

-- | A concrete leaf-or-opaque data value carried by a W-type node.
--
-- Mirrors @panproto_inst::value::Value@: the free term algebra of
-- JSON-like values. The variants partition into primitive atoms
-- ('VBool', 'VInt', 'VFloat', 'VStr', 'VBytes', 'VCidLink', 'VBlob',
-- 'VToken', 'VNull'), records ('VOpaque', 'VUnknown'), and the
-- free-monoid list object ('VList'). 'VUnknown' and 'VList' together
-- close the type under the two JSON constructors so schema-unanchored
-- data round-trips losslessly.
data Value
    = VBool !Bool
    | VInt !Int64
    | VFloat !Double
    | VStr !Text
    | VBytes ![Word8]
    | VCidLink !Text
    | VBlob !Text !Text !Word64
    -- ^ @ref_@, @mime@, @size@.
    | VToken !Text
    | VNull
    | VOpaque !Text !(HashMap Text Value)
    -- ^ @type_@ and opaque fields.
    | VUnknown !(HashMap Text Value)
    | VList ![Value]
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- FieldPresence

-- | Whether a node's value slot is present with a value, explicitly
-- null, or absent (not provided). Mirrors
-- @panproto_inst::value::FieldPresence@.
data FieldPresence
    = Present !Value
    | Null
    | Absent
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- NodeShape

-- | The structural shape of a node, orthogonal to its schema anchor.
-- Mirrors @panproto_inst::metadata::NodeShape@. 'Plain' is the
-- default; the CST extractors set 'ListShape', 'XmlElement', or
-- 'XmlTextSegment' to drive serialization choices in emitters.
data NodeShape
    = Plain
    | ListShape
    | XmlElement !Text
    -- ^ The original XML tag name the node was aliased from.
    | XmlTextSegment
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Node

-- | A node in a W-type instance tree. Mirrors
-- @panproto_inst::metadata::Node@. Each node is anchored to a schema
-- vertex and carries optional value data, a discriminator (for union
-- vertices), and extra fields for round-trip fidelity.
data Node = Node
    { id :: !Word32
    -- ^ Unique numeric identifier within the instance.
    , anchor :: !Text
    -- ^ The schema vertex this node is anchored to.
    , value :: !(Maybe FieldPresence)
    -- ^ The node's value, if it is a leaf.
    , discriminator :: !(Maybe Text)
    -- ^ Discriminator for union-typed vertices (e.g. the @$type@ value).
    , extraFields :: !(HashMap Text Value)
    -- ^ Extra fields preserved for round-trip fidelity.
    , position :: !(Maybe Word32)
    -- ^ Position in an ordered collection, if any.
    , shape :: !NodeShape
    -- ^ Structural shape, orthogonal to the schema anchor.
    , annotations :: !(HashMap Text Value)
    -- ^ Out-of-band annotations (metadata distinct from data).
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A node with the given id and anchor vertex, no value, no
-- discriminator, no extra fields, and the default 'Plain' shape.
emptyNode :: Word32 -> Text -> Node
emptyNode nid a =
    Node
        { id = nid
        , anchor = a
        , value = Nothing
        , discriminator = Nothing
        , extraFields = HM.empty
        , position = Nothing
        , shape = Plain
        , annotations = HM.empty
        }

-- ---------------------------------------------------------------------------
-- Arc and Fan

-- | An arc connecting a parent node to a child node along a schema
-- edge: @(parent_id, child_id, edge)@. Mirrors the Rust
-- @(u32, u32, Edge)@ tuple.
type Arc = (Word32, Word32, Edge)

-- | A hyper-edge fan: a parent node connected to labeled child
-- positions from a hyper-edge's signature. Mirrors
-- @panproto_inst::fan::Fan@.
data Fan = Fan
    { hyperEdgeId :: !Text
    -- ^ The schema hyper-edge this fan instantiates.
    , parent :: !Word32
    -- ^ The parent node id.
    , children :: !(HashMap Text Word32)
    -- ^ Labeled child positions: label name to node id.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Instance

-- | A W-type instance: tree-shaped data conforming to a schema.
-- Mirrors @panproto_inst::WInstance@.
--
-- Nodes are keyed by their numeric id; 'arcs' connect them along
-- schema edges; 'fans' realize hyper-edges. The tree is rooted at
-- 'root', whose node is anchored to 'schemaRoot'. The Rust
-- @parent_map@ and @children_map@ indices are derived rather than
-- stored (see 'parentOf' \/ 'childrenOf').
data Instance = Instance
    { nodes :: !(HashMap Word32 Node)
    -- ^ All nodes keyed by their numeric id.
    , arcs :: ![Arc]
    -- ^ Arcs: @(parent_id, child_id, schema_edge)@.
    , fans :: ![Fan]
    -- ^ Hyper-edge fans.
    , rootId :: !Word32
    -- ^ Root node id.
    , schemaRoot :: !Text
    -- ^ Schema vertex the root node is anchored to.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | An instance with no nodes, arcs, or fans, rooted at id @0@ with an
-- empty schema-root anchor. Useful as a builder base and test fixture.
emptyInstance :: Instance
emptyInstance =
    Instance
        { nodes = HM.empty
        , arcs = []
        , fans = []
        , rootId = 0
        , schemaRoot = T.empty
        }

-- ---------------------------------------------------------------------------
-- Instance accessors

-- | Number of nodes. Mirrors @WInstance::node_count@ (the Python
-- @Instance.node_count@).
nodeCount :: Instance -> Int
nodeCount i = HM.size i.nodes

-- | Number of arcs. Mirrors @WInstance::arc_count@ (the Python
-- @Instance.arc_count@).
arcCount :: Instance -> Int
arcCount i = length i.arcs

-- | The root node id. Mirrors @WInstance::root@ (the Python
-- @Instance.root@).
root :: Instance -> Word32
root i = i.rootId

-- | The number of elements (nodes) in this instance. Mirrors
-- @Instance::element_count@, which for the W-type shape is the node
-- count. This is the pure counterpart of the @pp_inst_element_count@
-- FFI entry point.
elementCount :: Instance -> Int
elementCount = nodeCount

-- | Look up a node by id.
lookupNode :: Instance -> Word32 -> Maybe Node
lookupNode i nid = HM.lookup nid i.nodes

-- | The child node ids of a node, derived from the arc set (the index
-- is not stored). Mirrors @WInstance::children@.
childrenOf :: Instance -> Word32 -> [Word32]
childrenOf i nid = [c | (p, c, _) <- i.arcs, p == nid]

-- | The parent node id of a node, derived from the arc set (the index
-- is not stored). Mirrors @WInstance::parent@.
parentOf :: Instance -> Word32 -> Maybe Word32
parentOf i nid =
    case [p | (p, c, _) <- i.arcs, c == nid] of
        (p : _) -> Just p
        [] -> Nothing

-- ---------------------------------------------------------------------------
-- Complement

-- | The lens complement: data discarded by a lens @get@ that @put@
-- needs to reconstruct the source. Mirrors @panproto_lens::Complement@.
--
-- The optional fields ('originalExtraFields', 'arcEdges',
-- 'originalValues', 'synthesizedNodes', and 'sourceFingerprint') carry
-- @serde(default)@ on the Rust side and decode to their empty\/zero
-- values when absent.
data Complement = Complement
    { droppedNodes :: !(HashMap Word32 Node)
    -- ^ Nodes from the source absent from the target view.
    , droppedArcs :: ![Arc]
    -- ^ Arcs from the source absent from the target view.
    , droppedFans :: ![Fan]
    -- ^ Fans whose parent or children were dropped during @get@.
    , contractionChoices :: ![((Word32, Word32), Edge)]
    -- ^ Resolver decisions made during ancestor contraction. Keyed by
    -- a @(u32, u32)@ tuple on the Rust side, lowered to an array of
    -- pairs by @ciborium@'s map serialization.
    , originalParent :: !(HashMap Word32 Word32)
    -- ^ Original parent mapping before contraction.
    , sourceFingerprint :: !Word64
    -- ^ Fingerprint of the source schema at @get@ time.
    , originalExtraFields :: !(HashMap Word32 (HashMap Text Value))
    -- ^ Pre-transform @extra_fields@ for nodes that had field
    -- transforms applied.
    , arcEdges :: ![((Word32, Word32), Edge)]
    -- ^ Exact edge used for every view arc, keyed by
    -- @(parent_id, child_id)@.
    , originalValues :: !(HashMap Word32 (Maybe FieldPresence))
    -- ^ Pre-coercion @node.value@ for nodes that had @__value__@
    -- transforms applied.
    , synthesizedNodes :: ![Word32]
    -- ^ View node ids synthesized during forward eval that @put@ must
    -- drop when reconstructing the source.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | An empty complement (no data discarded). Mirrors
-- @Complement::empty@.
emptyComplement :: Complement
emptyComplement =
    Complement
        { droppedNodes = HM.empty
        , droppedArcs = []
        , droppedFans = []
        , contractionChoices = []
        , originalParent = HM.empty
        , sourceFingerprint = 0
        , originalExtraFields = HM.empty
        , arcEdges = []
        , originalValues = HM.empty
        , synthesizedNodes = []
        }

-- | Number of dropped nodes. Mirrors the Python
-- @Complement.dropped_node_count@.
droppedNodeCount :: Complement -> Int
droppedNodeCount c = HM.size c.droppedNodes

-- | Number of dropped arcs. Mirrors the Python
-- @Complement.dropped_arc_count@.
droppedArcCount :: Complement -> Int
droppedArcCount c = length c.droppedArcs

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations the @instance@ surface of @panproto-c@ exposes (see
-- @CONTRACT.md@'s @instance@ domain). Each backend carries instances
-- in its own 'InstanceRep' (an opaque foreign handle for 'Rust', a
-- thin wrapper around the value for 'Native') and bridges to the
-- shared 'Instance' value type through 'ingestInstance' \/
-- 'reifyInstance'.
--
-- 'SchemaBackend' is a superclass because every instance operation is
-- anchored to a schema: 'validateInstance' and 'jsonToInstance' both
-- take a 'Panproto.Class.SchemaRep' to anchor against.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Instance@);
-- this module declares only the class.
class SchemaBackend back => InstanceBackend back where
    -- | Backend-specific representation of an 'Instance'. For 'Rust'
    -- an opaque foreign handle; for 'Native' a wrapper around the
    -- value.
    data InstanceRep back :: Type

    -- | Ingest a structured 'Instance' into the backend.
    ingestInstance :: Proxy back -> Instance -> IO (InstanceRep back)

    -- | Materialize the backend representation as a structured
    -- 'Instance'.
    reifyInstance :: InstanceRep back -> IO Instance

    -- | Release any resources held by the representation. Idempotent
    -- at the slab level, as with the other backend reps.
    releaseInstance :: InstanceRep back -> IO ()

    -- | Validate an instance against a schema, returning the list of
    -- human-readable validation messages (empty means valid). Wraps
    -- @pp_inst_validate@ (@inst::validate_wtype@).
    validateInstance :: SchemaRep back -> InstanceRep back -> IO [Text]

    -- | Render an instance as a JSON document anchored to a schema.
    -- Wraps @pp_inst_to_json@ (@inst::to_json@).
    instanceToJson :: SchemaRep back -> InstanceRep back -> IO Text

    -- | Parse a JSON document into an instance, anchored to a schema
    -- and rooted at the named vertex. Wraps @pp_inst_json_to_instance@
    -- (@inst::parse_json@).
    jsonToInstance :: SchemaRep back -> Text -> Text -> IO (InstanceRep back)
    -- ^ Schema, root vertex name, JSON payload.

    -- | The number of elements (nodes) in an instance. Wraps
    -- @pp_inst_element_count@ (@WInstance::node_count@). The pure
    -- counterpart is 'elementCount'.
    elementCountIO :: InstanceRep back -> IO Int

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode an 'Instance' to the CBOR @WInstance@ shape @ciborium@
-- deserializes. The @parent_map@ and @children_map@ indices are
-- recomputed from the arc set so the emitted CBOR carries every field
-- the Rust struct requires.
encodeInstance :: Instance -> LBS.ByteString
encodeInstance i =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 7
            <> kv "nodes" (encodeNodeMap i.nodes)
            <> kv "arcs" (encodeArcs i.arcs)
            <> kv "fans" (encodeList encodeFan i.fans)
            <> kv "root" (Enc.encodeWord32 i.rootId)
            <> kv "schema_root" (Enc.encodeString i.schemaRoot)
            <> kv "parent_map" (encodeWordMap Enc.encodeWord32 parentMap)
            <> kv "children_map" (encodeWordMap (encodeList Enc.encodeWord32) childrenMap)
  where
    kv k v = Enc.encodeString k <> v
    parentMap = HM.fromList [(c, p) | (p, c, _) <- i.arcs]
    childrenMap =
        foldr (\(p, c, _) -> HM.insertWith (flip (<>)) p [c]) HM.empty i.arcs

-- | Encode a 'Complement' to its CBOR shape.
encodeComplement :: Complement -> LBS.ByteString
encodeComplement = CBOR.toLazyByteString . complementEncoding

-- | Encode a list of complements as the CBOR @Vec<Complement>@ the
-- data domain marshals.
encodeComplements :: [Complement] -> LBS.ByteString
encodeComplements = CBOR.toLazyByteString . encodeList complementEncoding

complementEncoding :: Complement -> Encoding
complementEncoding c =
    Enc.encodeMapLen 10
        <> kv "dropped_nodes" (encodeNodeMap c.droppedNodes)
        <> kv "dropped_arcs" (encodeArcs c.droppedArcs)
        <> kv "dropped_fans" (encodeList encodeFan c.droppedFans)
        <> kv "contraction_choices" (encodePairMap c.contractionChoices)
        <> kv "original_parent" (encodeWordMap Enc.encodeWord32 c.originalParent)
        <> kv "source_fingerprint" (Enc.encodeWord64 c.sourceFingerprint)
        <> kv "original_extra_fields" (encodeWordMap encodeValueMap c.originalExtraFields)
        <> kv "arc_edges" (encodePairMap c.arcEdges)
        <> kv "original_values" (encodeWordMap encodeMaybePresence c.originalValues)
        <> kv "synthesized_nodes" (encodeList Enc.encodeWord32 c.synthesizedNodes)
  where
    kv k v = Enc.encodeString k <> v

encodeNode :: Node -> Encoding
encodeNode n =
    Enc.encodeMapLen 8
        <> kv "id" (Enc.encodeWord32 n.id)
        <> kv "anchor" (Enc.encodeString n.anchor)
        <> kv "value" (encodeMaybePresence n.value)
        <> kv "discriminator" (encodeMaybeText n.discriminator)
        <> kv "extra_fields" (encodeValueMap n.extraFields)
        <> kv "position" (maybe Enc.encodeNull Enc.encodeWord32 n.position)
        <> kv "shape" (encodeNodeShape n.shape)
        <> kv "annotations" (encodeValueMap n.annotations)
  where
    kv k v = Enc.encodeString k <> v

encodeNodeShape :: NodeShape -> Encoding
encodeNodeShape = \case
    Plain -> tag "plain"
    ListShape -> tag "list"
    XmlTextSegment -> tag "xml_text_segment"
    XmlElement t ->
        Enc.encodeMapLen 2
            <> Enc.encodeString "kind"
            <> Enc.encodeString "xml_element"
            <> Enc.encodeString "tag"
            <> Enc.encodeString t
  where
    tag k = Enc.encodeMapLen 1 <> Enc.encodeString "kind" <> Enc.encodeString k

encodeFan :: Fan -> Encoding
encodeFan f =
    Enc.encodeMapLen 3
        <> Enc.encodeString "hyper_edge_id"
        <> Enc.encodeString f.hyperEdgeId
        <> Enc.encodeString "parent"
        <> Enc.encodeWord32 f.parent
        <> Enc.encodeString "children"
        <> encodeWordValMap Enc.encodeWord32 f.children

encodeArcs :: [Arc] -> Encoding
encodeArcs xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap encodeArc xs
  where
    encodeArc (p, c, e) =
        Enc.encodeListLen 3 <> Enc.encodeWord32 p <> Enc.encodeWord32 c <> encodeEdge e

-- | Encode a @panproto_schema::Edge@ in the @ciborium@ struct shape.
encodeEdge :: Edge -> Encoding
encodeEdge e =
    Enc.encodeMapLen 4
        <> Enc.encodeString "src" <> Enc.encodeString e.src
        <> Enc.encodeString "tgt" <> Enc.encodeString e.tgt
        <> Enc.encodeString "kind" <> Enc.encodeString e.kind
        <> Enc.encodeString "name" <> encodeMaybeText e.name

-- | Encode a node's value slot. @None@ encodes as CBOR null;
-- @Some(FieldPresence)@ encodes the externally-tagged presence.
encodeMaybePresence :: Maybe FieldPresence -> Encoding
encodeMaybePresence = maybe Enc.encodeNull encodeFieldPresence

-- | Encode a 'FieldPresence' as an externally-tagged @serde@ enum.
encodeFieldPresence :: FieldPresence -> Encoding
encodeFieldPresence = \case
    Present v -> Enc.encodeMapLen 1 <> Enc.encodeString "Present" <> encodeValue v
    Null -> Enc.encodeString "Null"
    Absent -> Enc.encodeString "Absent"

-- | Encode a 'Value' as an externally-tagged @serde@ enum. Unit-like
-- 'VNull' is a bare string; the rest are single-key maps.
encodeValue :: Value -> Encoding
encodeValue = \case
    VBool b -> variant "Bool" (Enc.encodeBool b)
    VInt n -> variant "Int" (Enc.encodeInt64 n)
    VFloat d -> variant "Float" (Enc.encodeDouble d)
    VStr t -> variant "Str" (Enc.encodeString t)
    VBytes bs -> variant "Bytes" (Enc.encodeBytes (BS.pack bs))
    VCidLink t -> variant "CidLink" (Enc.encodeString t)
    VToken t -> variant "Token" (Enc.encodeString t)
    VNull -> Enc.encodeString "Null"
    VBlob r m s ->
        variant "Blob" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "ref_" <> Enc.encodeString r
                <> Enc.encodeString "mime" <> Enc.encodeString m
                <> Enc.encodeString "size" <> Enc.encodeWord64 s
    VOpaque t fields ->
        variant "Opaque" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "type_" <> Enc.encodeString t
                <> Enc.encodeString "fields" <> encodeValueMap fields
    VUnknown m -> variant "Unknown" (encodeValueMap m)
    VList xs -> variant "List" (encodeList encodeValue xs)
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

-- | Encode a @HashMap Text Value@ as a CBOR map.
encodeValueMap :: HashMap Text Value -> Encoding
encodeValueMap m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> encodeValue v) m

-- | Encode a @HashMap Word32 Node@ as a CBOR map with integer keys.
encodeNodeMap :: HashMap Word32 Node -> Encoding
encodeNodeMap = encodeWordMap encodeNode

-- | Encode a @HashMap Word32 v@ as a CBOR map with integer keys.
encodeWordMap :: (v -> Encoding) -> HashMap Word32 v -> Encoding
encodeWordMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeWord32 k <> enc v) m

-- | Encode a @HashMap Text Word32@-style fan-children map.
encodeWordValMap :: (Word32 -> Encoding) -> HashMap Text Word32 -> Encoding
encodeWordValMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

-- | Encode a @(Word32, Word32) -> Edge@ map as @ciborium@'s array of
-- @[[a, b], edge]@ pairs (a tuple-keyed map lowers to a sequence).
encodePairMap :: [((Word32, Word32), Edge)] -> Encoding
encodePairMap xs =
    Enc.encodeListLen (fromIntegral (length xs))
        <> foldMap
            ( \((a, b), e) ->
                Enc.encodeListLen 2
                    <> (Enc.encodeListLen 2 <> Enc.encodeWord32 a <> Enc.encodeWord32 b)
                    <> encodeEdge e
            )
            xs

encodeMaybeText :: Maybe Text -> Encoding
encodeMaybeText = maybe Enc.encodeNull Enc.encodeString

encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @WInstance@ bytes into a structured 'Instance'.
-- Tolerant of unknown fields and missing optional fields; the
-- precomputed-index fields (@parent_map@, @children_map@), if present,
-- are skipped and recomputed on demand.
decodeInstance :: LBS.ByteString -> Either String Instance
decodeInstance = runDecoder instanceDecoder "instance"

-- | Decode CBOR @Complement@ bytes into a structured 'Complement'.
decodeComplement :: LBS.ByteString -> Either String Complement
decodeComplement = runDecoder complementDecoder "complement"

-- | Decode CBOR @Vec<Complement>@ bytes (the data domain shape).
decodeComplements :: LBS.ByteString -> Either String [Complement]
decodeComplements = runDecoder (decodeListOf complementDecoder) "complement list"

runDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

instanceDecoder :: Decoder s Instance
instanceDecoder = decodeMapWith emptyInstance onKey
  where
    onKey acc key = case key of
        "nodes" -> (\v -> acc {nodes = v}) <$> decodeNodeMap
        "arcs" -> (\v -> acc {arcs = v}) <$> decodeArcs
        "fans" -> (\v -> acc {fans = v}) <$> decodeListOf decodeFan
        "root" -> (\v -> acc {rootId = v}) <$> decodeWord32
        "schema_root" -> (\v -> acc {schemaRoot = v}) <$> Dec.decodeString
        -- Precomputed indices and unknown fields: skip the value.
        _ -> skipTerm >> pure acc

complementDecoder :: Decoder s Complement
complementDecoder = decodeMapWith emptyComplement onKey
  where
    onKey acc key = case key of
        "dropped_nodes" -> (\v -> acc {droppedNodes = v}) <$> decodeNodeMap
        "dropped_arcs" -> (\v -> acc {droppedArcs = v}) <$> decodeArcs
        "dropped_fans" -> (\v -> acc {droppedFans = v}) <$> decodeListOf decodeFan
        "contraction_choices" -> (\v -> acc {contractionChoices = v}) <$> decodePairMap
        "original_parent" -> (\v -> acc {originalParent = v}) <$> decodeWordMap decodeWord32
        "source_fingerprint" -> (\v -> acc {sourceFingerprint = v}) <$> decodeWord64
        "original_extra_fields" ->
            (\v -> acc {originalExtraFields = v}) <$> decodeWordMap decodeValueMap
        "arc_edges" -> (\v -> acc {arcEdges = v}) <$> decodePairMap
        "original_values" ->
            (\v -> acc {originalValues = v}) <$> decodeWordMap decodeMaybePresence
        "synthesized_nodes" ->
            (\v -> acc {synthesizedNodes = v}) <$> decodeListOf decodeWord32
        _ -> skipTerm >> pure acc

-- The struct decoders below build positionally rather than via record
-- update: with 'DuplicateRecordFields', a record update like
-- @acc {id = v}@ is ambiguous because the field name alone does not
-- determine the datatype. Threading a tuple accumulator and applying
-- the constructor at the end sidesteps that while tolerating field
-- reordering and unknown fields.

decodeNode :: Decoder s Node
decodeNode = decodeFields initial build handler
  where
    initial =
        ( 0 :: Word32
        , T.empty
        , Nothing
        , Nothing
        , HM.empty
        , Nothing
        , Plain
        , HM.empty
        )
    build (i, a, v, d, ef, p, sh, ann) = Node i a v d ef p sh ann
    handler acc@(i, a, v, d, ef, p, sh, ann) key = case key of
        "id" -> (\x -> (x, a, v, d, ef, p, sh, ann)) <$> decodeWord32
        "anchor" -> (\x -> (i, x, v, d, ef, p, sh, ann)) <$> Dec.decodeString
        "value" -> (\x -> (i, a, x, d, ef, p, sh, ann)) <$> decodeMaybePresence
        "discriminator" -> (\x -> (i, a, v, x, ef, p, sh, ann)) <$> decodeMaybeText
        "extra_fields" -> (\x -> (i, a, v, d, x, p, sh, ann)) <$> decodeValueMap
        "position" -> (\x -> (i, a, v, d, ef, x, sh, ann)) <$> decodeMaybeWord32
        "shape" -> (\x -> (i, a, v, d, ef, p, x, ann)) <$> decodeNodeShape
        "annotations" -> (\x -> (i, a, v, d, ef, p, sh, x)) <$> decodeValueMap
        _ -> skipTerm >> pure acc

decodeNodeShape :: Decoder s NodeShape
decodeNodeShape = decodeFields (T.empty, Nothing) build handler
  where
    build (kind, mtag) = case kind of
        "list" -> ListShape
        "xml_text_segment" -> XmlTextSegment
        "xml_element" -> XmlElement (fromMaybe T.empty mtag)
        _ -> Plain
    handler acc@(kind, mtag) key = case key of
        "kind" -> (\v -> (v, mtag)) <$> Dec.decodeString
        "tag" -> (\v -> (kind, Just v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeFan :: Decoder s Fan
decodeFan = decodeFields (T.empty, 0 :: Word32, HM.empty) build handler
  where
    build (hid, p, ch) = Fan hid p ch
    handler acc@(hid, p, ch) key = case key of
        "hyper_edge_id" -> (\v -> (v, p, ch)) <$> Dec.decodeString
        "parent" -> (\v -> (hid, v, ch)) <$> decodeWord32
        "children" -> (\v -> (hid, p, v)) <$> decodeWordValMap decodeWord32
        _ -> skipTerm >> pure acc

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

-- | Decode a node's value slot: CBOR null means @None@, anything else
-- is a @Some(FieldPresence)@.
decodeMaybePresence :: Decoder s (Maybe FieldPresence)
decodeMaybePresence = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> decodeFieldPresence

-- | Decode an externally-tagged 'FieldPresence': a bare string for the
-- unit variants, or a single-key map @{"Present": value}@.
decodeFieldPresence :: Decoder s FieldPresence
decodeFieldPresence = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            s <- Dec.decodeString
            case s of
                "Null" -> pure Null
                "Absent" -> pure Absent
                other -> fail ("decodeFieldPresence: unknown unit variant " <> T.unpack other)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            k <- Dec.decodeString
            case k of
                "Present" -> Present <$> decodeValue
                other -> fail ("decodeFieldPresence: unknown variant " <> T.unpack other)

-- | Decode an externally-tagged 'Value': a bare string for 'VNull',
-- or a single-key map for the rest.
decodeValue :: Decoder s Value
decodeValue = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            s <- Dec.decodeString
            case s of
                "Null" -> pure VNull
                other -> fail ("decodeValue: unknown unit variant " <> T.unpack other)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            k <- Dec.decodeString
            case k of
                "Bool" -> VBool <$> Dec.decodeBool
                "Int" -> VInt <$> decodeInt64
                "Float" -> VFloat <$> Dec.decodeDouble
                "Str" -> VStr <$> Dec.decodeString
                "Bytes" -> VBytes . BS.unpack <$> Dec.decodeBytes
                "CidLink" -> VCidLink <$> Dec.decodeString
                "Token" -> VToken <$> Dec.decodeString
                "Null" -> pure VNull
                "Blob" -> decodeBlob
                "Opaque" -> decodeOpaque
                "Unknown" -> VUnknown <$> decodeValueMap
                "List" -> VList <$> decodeListOf decodeValue
                other -> fail ("decodeValue: unknown variant " <> T.unpack other)

decodeBlob :: Decoder s Value
decodeBlob = decodeFields (T.empty, T.empty, 0 :: Word64) build handler
  where
    build (r, m, s) = VBlob r m s
    handler acc@(r, m, s) key = case key of
        "ref_" -> (\v -> (v, m, s)) <$> Dec.decodeString
        "mime" -> (\v -> (r, v, s)) <$> Dec.decodeString
        "size" -> (\v -> (r, m, v)) <$> decodeWord64
        _ -> skipTerm >> pure acc

decodeOpaque :: Decoder s Value
decodeOpaque = decodeFields (T.empty, HM.empty) build handler
  where
    build (t, fields) = VOpaque t fields
    handler acc@(t, fields) key = case key of
        "type_" -> (\v -> (v, fields)) <$> Dec.decodeString
        "fields" -> (\v -> (t, v)) <$> decodeValueMap
        _ -> skipTerm >> pure acc

-- | Decode a CBOR map of @Text -> Value@.
decodeValueMap :: Decoder s (HashMap Text Value)
decodeValueMap = decodeTextKeyMap decodeValue

-- | Decode a CBOR map with text keys into a 'HashMap'.
decodeTextKeyMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeTextKeyMap decV = HM.fromList <$> decodeMapPairs Dec.decodeString decV

-- | Decode the fan-children map @Text -> Word32@.
decodeWordValMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeWordValMap = decodeTextKeyMap

-- | Decode a CBOR map with @Word32@ keys.
decodeWordMap :: Decoder s v -> Decoder s (HashMap Word32 v)
decodeWordMap decV = HM.fromList <$> decodeMapPairs decodeWord32 decV

decodeNodeMap :: Decoder s (HashMap Word32 Node)
decodeNodeMap = decodeWordMap decodeNode

-- | Decode the @arcs@ list: an array of @[parent, child, edge]@ arrays.
decodeArcs :: Decoder s [Arc]
decodeArcs = decodeListOf arc
  where
    arc = do
        _ <- Dec.decodeListLenOrIndef
        p <- decodeWord32
        c <- decodeWord32
        e <- decodeEdge
        pure (p, c, e)

-- | Decode a @(Word32, Word32) -> Edge@ tuple-keyed map from the
-- @[[a, b], edge]@ array of pairs.
decodePairMap :: Decoder s [((Word32, Word32), Edge)]
decodePairMap = decodeListOf pair
  where
    pair = do
        _ <- Dec.decodeListLenOrIndef
        (a, b) <- tupleKey
        e <- decodeEdge
        pure ((a, b), e)
    tupleKey = do
        _ <- Dec.decodeListLenOrIndef
        a <- decodeWord32
        b <- decodeWord32
        pure (a, b)

-- | Decode a CBOR map, threading a tuple accumulator through an entry
-- handler and applying a constructor at the end. The handler consumes
-- the value for the decoded key.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = build <$> decodeMapWith initial onKey

-- | Fold over a CBOR map's entries (definite or indefinite length),
-- dispatching each key through the handler.
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

-- | Decode a CBOR map's key/value pairs (definite or indefinite) into
-- an association list.
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

-- | Decode a CBOR list (definite or indefinite).
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

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> Dec.decodeString

decodeMaybeWord32 :: Decoder s (Maybe Word32)
decodeMaybeWord32 = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> decodeWord32

decodeWord32 :: Decoder s Word32
decodeWord32 = fromIntegral <$> Dec.decodeWord64

decodeWord64 :: Decoder s Word64
decodeWord64 = Dec.decodeWord64

decodeInt64 :: Decoder s Int64
decodeInt64 = Dec.decodeInt64

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown or precomputed-index fields.
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
        _ -> fail "decodeInstance: unsupported CBOR token while skipping"
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
