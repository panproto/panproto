{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedLists #-}

-- | Pure-Haskell exchange types between the native and Rust backends.
--
-- These types are the cold-path FFI wire format: a record on the
-- Haskell side is encoded as a CBOR map keyed by Rust @serde@ field
-- names. Decoding is tolerant of unknown fields (Rust may add new
-- ones in future versions) and applies @serde(default)@ semantics
-- when fields are missing.
--
-- The shape mirrors @panproto_core::schema::Protocol@ as defined in
-- @crates\/panproto-schema\/src\/protocol.rs@.
module Panproto.Canonical
    ( -- * Protocol
      CanonicalProtocol (..)
    , defaultProtocol
    , encodeProtocol
    , decodeProtocol

      -- * Edge rules
    , EdgeRule (..)
    , emptyEdgeRule

      -- * Schema
    , CanonicalSchema (..)
    , canonicalSchemaBytes
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

-- ---------------------------------------------------------------------------
-- CanonicalSchema

-- | An opaque CBOR-encoded panproto @Schema@.
--
-- The Rust @Schema@ struct has twenty-odd fields, including
-- @HashMap@s with custom serde helpers, @Expr@-valued enrichment
-- maps, and precomputed adjacency indices. Mirroring the full
-- structure on the Haskell side is not implemented, so this
-- 'CanonicalSchema' carries the
-- CBOR bytes verbatim rather than a structured ADT. Use the Rust
-- backend (which round-trips through @ciborium@) to introspect.
--
-- The bytes are exactly what panproto-c\'s @pp_schema_to_cbor@
-- produced. They round-trip losslessly through the Rust backend
-- (@reify (hoist x) ≡ x@). The 'Native' backend treats them as
-- opaque and is therefore an identity functor — useful for storing
-- schemas in a Haskell-only data pipeline without entering the
-- Rust runtime, but not for inspection.
newtype CanonicalSchema = CanonicalSchema {schemaBytes :: LBS.ByteString}
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Project the underlying CBOR bytes from a 'CanonicalSchema'.
--
-- The schema bytes do NOT carry their length on the C side; they
-- are a single CBOR item. The 'LBS.ByteString' returned here is
-- safe to write to disk, send over a wire, or hand to a future
-- structured native decoder.
canonicalSchemaBytes :: CanonicalSchema -> LBS.ByteString
canonicalSchemaBytes (CanonicalSchema bs) = bs

-- ---------------------------------------------------------------------------
-- EdgeRule

-- | Haskell mirror of @panproto_core::schema::EdgeRule@. Edge rules
-- govern which vertex kinds are permitted at the source and target
-- of each edge kind.
data EdgeRule = EdgeRule
    { edgeKind :: !Text
    -- ^ The edge kind this rule governs (e.g. @"prop"@,
    -- @"record-schema"@).
    , srcKinds :: ![Text]
    -- ^ Permitted source vertex kinds. Empty list means \"any\".
    , tgtKinds :: ![Text]
    -- ^ Permitted target vertex kinds. Empty list means \"any\".
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | An 'EdgeRule' with an empty edge kind and unrestricted source /
-- target. Useful as a base in builder-style construction.
emptyEdgeRule :: EdgeRule
emptyEdgeRule =
    EdgeRule {edgeKind = T.empty, srcKinds = [], tgtKinds = []}

-- ---------------------------------------------------------------------------
-- CanonicalProtocol

-- | Haskell mirror of @panproto_core::schema::Protocol@.
--
-- The fields exposed here cover the structural surface every
-- panproto consumer needs (name, theory choice, edge rules,
-- vertex kinds, constraint sorts) plus the boolean feature flags
-- that the GAT layer reads to choose which enrichments apply.
-- Unknown fields written by future Rust versions decode silently
-- via 'decodeProtocol'\'s tolerant skipper.
data CanonicalProtocol = CanonicalProtocol
    { name :: !Text
    , schemaTheory :: !Text
    , instanceTheory :: !Text
    , edgeRules :: ![EdgeRule]
    , objKinds :: ![Text]
    -- ^ Vertex kinds considered \"object-like\" (containers).
    , constraintSorts :: ![Text]
    -- ^ Recognized constraint sorts (e.g. @"maxLength"@,
    -- @"format"@, @"minimum"@).
    , -- Structural feature flags (default 'False').
      hasOrder :: !Bool
    , hasCoproducts :: !Bool
    , hasRecursion :: !Bool
    , hasCausal :: !Bool
    , nominalIdentity :: !Bool
    , -- Enrichment feature flags (default 'False').
      hasDefaults :: !Bool
    , hasCoercions :: !Bool
    , hasMergers :: !Bool
    , hasPolicies :: !Bool
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | A 'CanonicalProtocol' with empty fields and the GAT theory names
-- panproto uses by default for graph-shaped schemas. Useful as a base
-- for tests and \'protocol\' DSL builders.
defaultProtocol :: CanonicalProtocol
defaultProtocol =
    CanonicalProtocol
        { name = T.empty
        , schemaTheory = "ThGraph"
        , instanceTheory = "ThWType"
        , edgeRules = []
        , objKinds = []
        , constraintSorts = []
        , hasOrder = False
        , hasCoproducts = False
        , hasRecursion = False
        , hasCausal = False
        , nominalIdentity = False
        , hasDefaults = False
        , hasCoercions = False
        , hasMergers = False
        , hasPolicies = False
        }

-- ---------------------------------------------------------------------------
-- Field name registry

-- | Field name on the Rust side. Rust uses @snake_case@ via @serde@'s
-- default casing, so we mirror that exactly.
fieldName :: ProtocolField -> Text
fieldName = \case
    FName -> "name"
    FSchemaTheory -> "schema_theory"
    FInstanceTheory -> "instance_theory"
    FSchemaComposition -> "schema_composition"
    FInstanceComposition -> "instance_composition"
    FEdgeRules -> "edge_rules"
    FObjKinds -> "obj_kinds"
    FConstraintSorts -> "constraint_sorts"
    FHasOrder -> "has_order"
    FHasCoproducts -> "has_coproducts"
    FHasRecursion -> "has_recursion"
    FHasCausal -> "has_causal"
    FNominalIdentity -> "nominal_identity"
    FHasDefaults -> "has_defaults"
    FHasCoercions -> "has_coercions"
    FHasMergers -> "has_mergers"
    FHasPolicies -> "has_policies"

-- | Internal: enumerable field tags so encode and decode share spelling.
data ProtocolField
    = FName
    | FSchemaTheory
    | FInstanceTheory
    | FSchemaComposition
    | FInstanceComposition
    | FEdgeRules
    | FObjKinds
    | FConstraintSorts
    | FHasOrder
    | FHasCoproducts
    | FHasRecursion
    | FHasCausal
    | FNominalIdentity
    | FHasDefaults
    | FHasCoercions
    | FHasMergers
    | FHasPolicies
    deriving stock (Eq, Show, Bounded, Enum)

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'CanonicalProtocol' to CBOR bytes compatible with the
-- Rust side\'s @ciborium@ deserialization of @Protocol@.
encodeProtocol :: CanonicalProtocol -> LBS.ByteString
encodeProtocol p =
    CBOR.toLazyByteString $
        Enc.encodeMapLen
            (fromIntegral (fromEnum (maxBound @ProtocolField) + 1))
            <> entry FName (Enc.encodeString p.name)
            <> entry FSchemaTheory (Enc.encodeString p.schemaTheory)
            <> entry FInstanceTheory (Enc.encodeString p.instanceTheory)
            -- The two `*_composition` fields are required (no
            -- `serde(default)`); we always emit `null` because
            -- `CompositionSpec` is not yet exposed at this layer.
            <> entry FSchemaComposition Enc.encodeNull
            <> entry FInstanceComposition Enc.encodeNull
            <> entry FEdgeRules (encodeEdgeRules p.edgeRules)
            <> entry FObjKinds (encodeStringList p.objKinds)
            <> entry FConstraintSorts (encodeStringList p.constraintSorts)
            <> entry FHasOrder (Enc.encodeBool p.hasOrder)
            <> entry FHasCoproducts (Enc.encodeBool p.hasCoproducts)
            <> entry FHasRecursion (Enc.encodeBool p.hasRecursion)
            <> entry FHasCausal (Enc.encodeBool p.hasCausal)
            <> entry FNominalIdentity (Enc.encodeBool p.nominalIdentity)
            <> entry FHasDefaults (Enc.encodeBool p.hasDefaults)
            <> entry FHasCoercions (Enc.encodeBool p.hasCoercions)
            <> entry FHasMergers (Enc.encodeBool p.hasMergers)
            <> entry FHasPolicies (Enc.encodeBool p.hasPolicies)
  where
    entry f v = Enc.encodeString (fieldName f) <> v

encodeStringList :: [Text] -> Enc.Encoding
encodeStringList xs =
    Enc.encodeListLen (fromIntegral (length xs))
        <> mconcat (map Enc.encodeString xs)

encodeEdgeRules :: [EdgeRule] -> Enc.Encoding
encodeEdgeRules xs =
    Enc.encodeListLen (fromIntegral (length xs))
        <> mconcat (map encodeEdgeRule xs)

encodeEdgeRule :: EdgeRule -> Enc.Encoding
encodeEdgeRule r =
    Enc.encodeMapLen 3
        <> Enc.encodeString "edge_kind"
        <> Enc.encodeString r.edgeKind
        <> Enc.encodeString "src_kinds"
        <> encodeStringList r.srcKinds
        <> Enc.encodeString "tgt_kinds"
        <> encodeStringList r.tgtKinds

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR bytes produced by panproto-c\'s @pp_protocol_serialize@
-- into a 'CanonicalProtocol'. Tolerates unknown fields and missing
-- optional fields (the latter fall back to 'defaultProtocol').
decodeProtocol :: LBS.ByteString -> Either String CanonicalProtocol
decodeProtocol bs =
    case CBOR.deserialiseFromBytes protocolDecoder bs of
        Left err -> Left (show err)
        Right (rest, p)
            | LBS.null rest -> Right p
            | otherwise -> Left "trailing bytes after CBOR-encoded protocol"

protocolDecoder :: Decoder s CanonicalProtocol
protocolDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> readEntries n defaultProtocol
        Nothing -> readEntriesIndef defaultProtocol

readEntries :: Int -> CanonicalProtocol -> Decoder s CanonicalProtocol
readEntries 0 acc = pure acc
readEntries n acc = do
    acc' <- readOneEntry acc
    readEntries (n - 1) acc'

readEntriesIndef :: CanonicalProtocol -> Decoder s CanonicalProtocol
readEntriesIndef acc = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure acc
        else do
            acc' <- readOneEntry acc
            readEntriesIndef acc'

readOneEntry :: CanonicalProtocol -> Decoder s CanonicalProtocol
readOneEntry acc = do
    key <- Dec.decodeString
    case key of
        k | k == fieldName FName ->
            (\v -> acc {name = v}) <$> Dec.decodeString
        k | k == fieldName FSchemaTheory ->
            (\v -> acc {schemaTheory = v}) <$> Dec.decodeString
        k | k == fieldName FInstanceTheory ->
            (\v -> acc {instanceTheory = v}) <$> Dec.decodeString
        k | k == fieldName FSchemaComposition -> do
            -- Composition specs decode opaquely; we always discard.
            skipTerm
            pure acc
        k | k == fieldName FInstanceComposition -> do
            skipTerm
            pure acc
        k | k == fieldName FEdgeRules ->
            (\v -> acc {edgeRules = v}) <$> decodeEdgeRules
        k | k == fieldName FObjKinds ->
            (\v -> acc {objKinds = v}) <$> decodeStringList
        k | k == fieldName FConstraintSorts ->
            (\v -> acc {constraintSorts = v}) <$> decodeStringList
        k | k == fieldName FHasOrder ->
            (\v -> acc {hasOrder = v}) <$> Dec.decodeBool
        k | k == fieldName FHasCoproducts ->
            (\v -> acc {hasCoproducts = v}) <$> Dec.decodeBool
        k | k == fieldName FHasRecursion ->
            (\v -> acc {hasRecursion = v}) <$> Dec.decodeBool
        k | k == fieldName FHasCausal ->
            (\v -> acc {hasCausal = v}) <$> Dec.decodeBool
        k | k == fieldName FNominalIdentity ->
            (\v -> acc {nominalIdentity = v}) <$> Dec.decodeBool
        k | k == fieldName FHasDefaults ->
            (\v -> acc {hasDefaults = v}) <$> Dec.decodeBool
        k | k == fieldName FHasCoercions ->
            (\v -> acc {hasCoercions = v}) <$> Dec.decodeBool
        k | k == fieldName FHasMergers ->
            (\v -> acc {hasMergers = v}) <$> Dec.decodeBool
        k | k == fieldName FHasPolicies ->
            (\v -> acc {hasPolicies = v}) <$> Dec.decodeBool
        _ -> do
            -- Unknown field: skip its value and keep going. This is
            -- what permits the Rust side to grow new fields without
            -- breaking the Haskell decoder.
            skipTerm
            pure acc

decodeStringList :: Decoder s [Text]
decodeStringList = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> replicateDecoder n Dec.decodeString
        Nothing -> readListIndef Dec.decodeString

replicateDecoder :: Int -> Decoder s a -> Decoder s [a]
replicateDecoder 0 _ = pure []
replicateDecoder k d = do
    x <- d
    xs <- replicateDecoder (k - 1) d
    pure (x : xs)

readListIndef :: Decoder s a -> Decoder s [a]
readListIndef d = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure []
        else do
            x <- d
            rest <- readListIndef d
            pure (x : rest)

decodeEdgeRules :: Decoder s [EdgeRule]
decodeEdgeRules = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> replicateDecoder n decodeEdgeRule
        Nothing -> readListIndef decodeEdgeRule

decodeEdgeRule :: Decoder s EdgeRule
decodeEdgeRule = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> readEdgeRuleEntries n emptyEdgeRule
        Nothing -> readEdgeRuleEntriesIndef emptyEdgeRule

readEdgeRuleEntries :: Int -> EdgeRule -> Decoder s EdgeRule
readEdgeRuleEntries 0 acc = pure acc
readEdgeRuleEntries n acc = do
    acc' <- readOneEdgeRuleEntry acc
    readEdgeRuleEntries (n - 1) acc'

readEdgeRuleEntriesIndef :: EdgeRule -> Decoder s EdgeRule
readEdgeRuleEntriesIndef acc = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure acc
        else do
            acc' <- readOneEdgeRuleEntry acc
            readEdgeRuleEntriesIndef acc'

readOneEdgeRuleEntry :: EdgeRule -> Decoder s EdgeRule
readOneEdgeRuleEntry acc = do
    key <- Dec.decodeString
    case key of
        "edge_kind" ->
            (\v -> acc {edgeKind = v}) <$> Dec.decodeString
        "src_kinds" ->
            (\v -> acc {srcKinds = v}) <$> decodeStringList
        "tgt_kinds" ->
            (\v -> acc {tgtKinds = v}) <$> decodeStringList
        _ -> do
            skipTerm
            pure acc

-- | Skip an arbitrary CBOR value. Implements depth-first descent over
-- nested arrays and maps so unknown fields with structured values do
-- not desync the surrounding decoder.
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
        if stop
            then pure ()
            else skipTerm >> skipTerm >> skipUntilBreakPairs

    skipBytesIndef = do
        Dec.decodeBytesIndef
        skipUntilBreakBytes

    skipStringIndef = do
        Dec.decodeStringIndef
        skipUntilBreakStrings

    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure ()
            else Dec.decodeBytes >> skipUntilBreakBytes

    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure ()
            else Dec.decodeString >> skipUntilBreakStrings
