{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE TypeFamilies #-}

-- | Bidirectional lenses and protolens chains.
--
-- panproto lenses are asymmetric lenses carrying an explicit
-- complement (Diskin et al., 2011). A lens between a source @s@ and a
-- view @a@ is a pair
--
-- > get : s -> (a, c)
-- > put : (a, c) -> s
--
-- where @c@ is the /complement/ (see 'Panproto.Instance.Complement'):
-- the data @get@ discards that @put@ needs to reconstruct the source.
-- This is the structure of a (very well-behaved) asymmetric lens with
-- complement, /not/ a van Laarhoven optic: the round-trip laws
-- (@GetPut@: @put (get s) = s@; @PutGet@: @fst (get (put (a, c))) = a@)
-- are checked at runtime by the engine against a concrete test
-- instance, because the lens is computed from a schema migration rather
-- than written by hand. The optic-ecosystem view of the
-- /complement-free/ lawful subset lives in "Panproto.Lens.Optics".
--
-- This module splits into two layers.
--
-- == Pure structural layer (this module's value types)
--
-- A 'ProtolensChain' is a /schema-independent/ value: an ordered list
-- of 'ProtolensStep's, each naming a source and target theory
-- endofunctor and recording whether the step is lossless. It mirrors
-- the @Vec<ProtolensStepInfo>@ JSON shape that @panproto-c@'s
-- @pp_protolens_chain_to_json@ emits (see @crates\/panproto-c\/CONTRACT.md@,
-- the @lens@ section). Chains compose /purely/ by concatenating their
-- steps ('composeChainPure'), with 'identityChain' the unit; this is
-- the 'Semigroup' \/ 'Monoid' structure, and 'LensArr' lifts it into a
-- 'Control.Category.Category'. 'fuseChain' collapses a chain to a
-- single step (the structural counterpart of @ProtolensChain::fuse@).
-- None of these touch a schema or a backend: they are pure rewrites of
-- the chain's step list.
--
-- The 'composedOpticKind' of a chain folds the per-step 'OpticKind's
-- through the optics lattice ('Iso' the identity, 'Traversal'
-- absorbing, @Lens + Prism = Affine@), mirroring
-- @panproto_lens::optic::OpticKind::compose@. This classification is
-- what "Panproto.Lens.Optics" consults to decide which steps project
-- to a genuine optic.
--
-- == Runnable layer (the 'LensBackend' class)
--
-- Actually /running/ a lens (@get@ \/ @put@), instantiating a chain at
-- a schema, checking the laws, or auto-generating a lens between two
-- schemas requires the engine. Those operations are handle-backed and
-- live behind the 'LensBackend' capability class, whose 'IO' signatures
-- mirror the eighteen @lens@-domain @panproto-c@ entry points. Each
-- backend carries its runnable artifacts in its own associated
-- representations ('ChainRep', 'LensRep', 'SymLensRep'), bridging to the
-- pure 'ProtolensChain' through 'ingestChain' \/ 'reifyChain'. The
-- 'Rust' instance lives in @Panproto.Rust.Lens@; this module declares
-- the class.
--
-- The CBOR codecs ('encodeChain' \/ 'decodeChain') and the aeson
-- bridge ('chainToJson' \/ 'chainFromJson') follow the tolerant decoder
-- idiom of "Panproto.Instance" and "Panproto.Schema": snake_case keys,
-- @serde(default)@ for absent fields, positional tuple accumulators
-- (sidestepping field-name ambiguity), and a depth-first unknown-term
-- skipper for forward compatibility.
module Panproto.Lens
    ( -- * Optic classification
      OpticKind (..)
    , composeOpticKind

      -- * Auto-generation stringency
    , Stringency (..)

      -- * Protolens chains (pure, structural)
      -- $purechain
    , ProtolensStep (..)
    , emptyStep
    , ProtolensChain (..)
    , identityChain
    , singletonChain
    , chainSteps
    , chainLength
    , isIdentityChain
    , chainLossless
    , composeChainPure
    , fuseChain
    , composedOpticKind

      -- * Category wrapper
    , LensArr (..)

      -- * Chain codecs
    , encodeChain
    , decodeChain
    , chainToJson
    , chainFromJson

      -- * Law and complement reports
    , LawCheckResult (..)
    , ComplementKind (..)

      -- * Capability class
    , LensBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.Category (Category)
import Control.Category qualified as Cat
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.Aeson qualified as Aeson
import Data.Aeson.Key qualified as Key
import Data.Aeson.KeyMap qualified as KM
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))
import Panproto.Instance (Complement, InstanceBackend (..))

-- ---------------------------------------------------------------------------
-- OpticKind

-- | The optics-hierarchy classification of a protolens step or chain.
-- Mirrors @panproto_lens::optic::OpticKind@.
--
-- The kinds form a lattice under composition ('composeOpticKind'): an
-- 'Iso' is a bijection needing no complement; a 'Lens' projects,
-- capturing dropped data in the complement; a 'Prism' injects, with the
-- complement a variant tag; an 'Affine' is a lens-composed-with-prism
-- (a partial focus); and a 'Traversal' has multiple foci, with the
-- complement tracking positions.
data OpticKind
    = Iso
    | Lens
    | Prism
    | Affine
    | Traversal
    deriving stock (Eq, Show, Bounded, Enum, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | Compose two optic kinds along the optics lattice. Mirrors
-- @OpticKind::compose@: 'Iso' is the identity, 'Traversal' absorbs
-- everything, homogeneous 'Lens' \/ 'Prism' compositions are preserved,
-- and any 'Lens' + 'Prism' mix (or anything touching 'Affine') yields
-- 'Affine'.
--
-- This is an associative operation with 'Iso' as unit, giving the
-- 'Monoid' instance on 'OpticKind' (where the chain's composed kind is
-- @'foldMap' (.optic) steps@). The lattice is commutative on the cases
-- that matter, so the 'Semigroup' below is well defined.
composeOpticKind :: OpticKind -> OpticKind -> OpticKind
composeOpticKind a b = case (a, b) of
    (Iso, x) -> x
    (x, Iso) -> x
    (Traversal, _) -> Traversal
    (_, Traversal) -> Traversal
    (Lens, Lens) -> Lens
    (Prism, Prism) -> Prism
    _ -> Affine

-- | @('<>') = 'composeOpticKind'@; see its note on associativity.
instance Semigroup OpticKind where
    (<>) = composeOpticKind

-- | 'Iso' is the identity of optic-kind composition.
instance Monoid OpticKind where
    mempty = Iso

-- ---------------------------------------------------------------------------
-- Stringency

-- | The auto-generation alignment tier. Mirrors
-- @panproto_lens::Stringency@ (serialized snake_case). The tiers are
-- strategy presets; threshold changes and evidence-dependent strategies
-- mean that their final candidate pools are not guaranteed to be nested:
--
-- * 'Strict': exact identifiers, exact suffixes, and edge labels; total
--   morphisms only.
-- * 'Balanced' (the engine default): adds aliases, token similarity, and
--   description similarity; total morphisms only.
-- * 'Lenient': adds wrap/unwrap, type-signature, WL-refinement, and
--   neighborhood evidence; permits spans.
-- * 'Exploratory': adds structural and registered coercion-witness
--   proposals; permits spans.
data Stringency
    = Strict
    | Balanced
    | Lenient
    | Exploratory
    deriving stock (Eq, Show, Bounded, Enum, Generic)
    deriving anyclass (NFData, Hashable)

-- | Combining two tier presets takes the one with the higher permissiveness
-- rank.
instance Semigroup Stringency where
    a <> b = if fromEnum a >= fromEnum b then a else b

-- | 'Strict' is the unit: since @('<>')@ is the join (looser tier wins)
-- and 'Strict' is the least tier, @'Strict' '<>' x = x@ and
-- @x '<>' 'Strict' = x@ for every tier, so the monoid identity laws
-- hold. (The /engine/ default tier for auto-generation is 'Balanced',
-- a separate notion: it is the starting tier a caller passes, not the
-- neutral element of tier composition.)
instance Monoid Stringency where
    mempty = Strict

-- | The snake_case wire form @serde@ expects.
stringencyText :: Stringency -> Text
stringencyText = \case
    Strict -> "strict"
    Balanced -> "balanced"
    Lenient -> "lenient"
    Exploratory -> "exploratory"

instance ToJSON Stringency where
    toJSON = Aeson.String . stringencyText

instance FromJSON Stringency where
    parseJSON = Aeson.withText "Stringency" $ \t ->
        case t of
            "strict" -> pure Strict
            "balanced" -> pure Balanced
            "lenient" -> pure Lenient
            "exploratory" -> pure Exploratory
            other -> fail ("unknown stringency " <> T.unpack other)

-- ---------------------------------------------------------------------------
-- ProtolensStep

-- $purechain
--
-- The chain value type and its combinators are /pure/ and
-- /schema-independent/. They never instantiate a step at a schema, run
-- @get@ \/ @put@, or touch a backend: they manipulate the structural
-- step list directly. Running a chain is the job of the 'LensBackend'
-- class below.

-- | One step of a 'ProtolensChain'. Mirrors
-- @panproto_c::api::helpers::ProtolensStepInfo@, the JSON shape
-- @pp_protolens_chain_to_json@ emits: a step name, the names of its
-- source and target theory endofunctors, and whether the step is
-- lossless (its instantiated lens has an empty complement).
--
-- The full Rust @Protolens@ carries the endofunctor /structure/ and a
-- @ComplementConstructor@; this binding mirrors the serialized
-- /summary/, which is what the structural chain layer needs and what
-- round-trips through the FFI JSON surface.
data ProtolensStep = ProtolensStep
    { name :: !Text
    -- ^ Human-readable step name.
    , sourceEndofunctor :: !Text
    -- ^ Name of the source theory endofunctor @F@.
    , targetEndofunctor :: !Text
    -- ^ Name of the target theory endofunctor @G@.
    , lossless :: !Bool
    -- ^ Whether the step's instantiated lens has an empty complement.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A step with empty names that is lossless: the identity-shaped step.
emptyStep :: ProtolensStep
emptyStep =
    ProtolensStep
        { name = T.empty
        , sourceEndofunctor = T.empty
        , targetEndofunctor = T.empty
        , lossless = True
        }

-- | The optic kind a step classifies as. Lossless steps are isomorphism
-- shaped ('Iso'); lossy steps are projections ('Lens'). The Rust engine
-- can further distinguish 'Prism' \/ 'Affine' \/ 'Traversal' from the
-- endofunctor structure, which this summary view does not carry, so the
-- structural classification is the conservative two-point split. A
-- backend that has the full @Protolens@ in hand reports the precise
-- kind through 'composeLensOpticKind'.
stepOpticKind :: ProtolensStep -> OpticKind
stepOpticKind s = if s.lossless then Iso else Lens

-- ---------------------------------------------------------------------------
-- ProtolensChain

-- | A chain of protolens steps, composed vertically. Mirrors
-- @panproto_lens::ProtolensChain@: each step's target endofunctor feeds
-- into the next step's source. The chain is a /pure structural value/:
-- it carries no schema and no engine handle. Instantiating it at a
-- schema (which produces a runnable lens) is 'instantiateChain' on the
-- 'LensBackend' class.
newtype ProtolensChain = ProtolensChain
    { steps :: [ProtolensStep]
    -- ^ The ordered protolens steps.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The empty chain: the structural identity. Instantiating it at any
-- schema yields the identity lens. It is the unit of 'composeChainPure'
-- and of the 'Monoid' instance.
identityChain :: ProtolensChain
identityChain = ProtolensChain []

-- | A one-step chain.
singletonChain :: ProtolensStep -> ProtolensChain
singletonChain s = ProtolensChain [s]

-- | The chain's steps.
chainSteps :: ProtolensChain -> [ProtolensStep]
chainSteps c = c.steps

-- | The number of steps.
chainLength :: ProtolensChain -> Int
chainLength c = length c.steps

-- | Whether the chain is the structural identity (no steps).
isIdentityChain :: ProtolensChain -> Bool
isIdentityChain c = null c.steps

-- | Whether the whole chain is lossless: every step is lossless (and so
-- the composed complement is empty). An empty chain is vacuously
-- lossless.
chainLossless :: ProtolensChain -> Bool
chainLossless c = all (.lossless) c.steps

-- | Compose two chains by concatenating their steps: @first@ then
-- @second@. This is the pure, structural vertical composition, matching
-- the engine's @ProtolensChain::compose@ (which extends one step list
-- with the other) and the Python @ProtolensChain.compose@. It does not
-- check that adjacent endofunctors agree; that is verified when the
-- composed chain is instantiated at a schema.
composeChainPure :: ProtolensChain -> ProtolensChain -> ProtolensChain
composeChainPure first second = ProtolensChain (first.steps <> second.steps)

-- | @('<>') = 'composeChainPure'@.
instance Semigroup ProtolensChain where
    (<>) = composeChainPure

-- | 'identityChain' is the unit.
instance Monoid ProtolensChain where
    mempty = identityChain

-- | Collapse a chain to a single fused step, the structural counterpart
-- of @ProtolensChain::fuse@. The fused step's source endofunctor is the
-- first step's source, its target is the last step's target, its name
-- joins the step names (last-to-first, matching the engine's
-- @"theta.eta"@ vertical-composition naming), and it is lossless exactly
-- when every step was. An identity chain fuses to a lossless identity
-- step.
--
-- The engine's @fuse@ also requires a runnable instantiation to collapse
-- the migration; here the fusion is purely structural (the step
-- summary), so it is total and never fails.
fuseChain :: ProtolensChain -> ProtolensStep
fuseChain c = case c.steps of
    [] -> emptyStep
    (s0 : rest) ->
        let lastStep = last (s0 : rest)
            joinedName =
                T.intercalate "." (reverse (map (.name) (s0 : rest)))
         in ProtolensStep
                { name = joinedName
                , sourceEndofunctor = s0.sourceEndofunctor
                , targetEndofunctor = lastStep.targetEndofunctor
                , lossless = chainLossless c
                }

-- | The composed optic kind of the chain, folding each step's
-- 'stepOpticKind' through the optics lattice. Mirrors
-- @ProtolensChain::composed_optic_kind@. An empty chain is 'Iso'.
composedOpticKind :: ProtolensChain -> OpticKind
composedOpticKind c = foldMap stepOpticKind c.steps

-- ---------------------------------------------------------------------------
-- Category wrapper

-- | A 'ProtolensChain' presented as a morphism in a 'Category'. The
-- phantom type parameters @a@ and @b@ stand for the source and target
-- schema-shapes a chain transforms /between/; they are not tracked at
-- the value level (the chain is structural), so the wrapper is a thin
-- newtype over 'ProtolensChain'.
--
-- The 'Category' instance is the pure structural composition: 'Cat.id'
-- is 'identityChain' and @('Cat..')@ is 'composeChainPure' with the
-- arguments flipped to match @(.)@'s right-to-left reading (@g . f@
-- runs @f@ then @g@).
newtype LensArr a b = LensArr
    { chain :: ProtolensChain
    -- ^ The underlying structural chain.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Structural composition of chains as a category. @'Cat.id'@ is the
-- empty chain; @g '.' f@ concatenates @f@'s steps before @g@'s.
instance Category LensArr where
    id = LensArr identityChain
    LensArr g . LensArr f = LensArr (composeChainPure f g)

-- ---------------------------------------------------------------------------
-- Law and complement reports

-- | The result of a lens-law check. Mirrors
-- @panproto_c::api::helpers::LawCheckResult@: whether the law held on
-- the tested instance, and a human-readable violation message when it
-- did not. The 'LensBackend' law checks raise on violation rather than
-- returning this, but the type is exposed so a backend that surfaces the
-- structured report (e.g. via JSON) can reuse it.
data LawCheckResult = LawCheckResult
    { holds :: !Bool
    -- ^ Whether the law holds on the tested instance.
    , violation :: !(Maybe Text)
    -- ^ Description of the violation, if any.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The classification of a complement's role. Mirrors
-- @panproto_lens::complement_type::ComplementKind@ (serialized
-- snake_case): whether the lens needs no complement ('CEmpty'), captures
-- data for the backward direction ('CDataCaptured'), requires
-- user-supplied defaults for the forward direction
-- ('CDefaultsRequired'), or both ('CMixed').
data ComplementKind
    = CEmpty
    | CDataCaptured
    | CDefaultsRequired
    | CMixed
    deriving stock (Eq, Show, Bounded, Enum, Generic)
    deriving anyclass (NFData, Hashable)

-- | The snake_case wire form @serde@ uses for 'ComplementKind'.
complementKindText :: ComplementKind -> Text
complementKindText = \case
    CEmpty -> "empty"
    CDataCaptured -> "data_captured"
    CDefaultsRequired -> "defaults_required"
    CMixed -> "mixed"

instance ToJSON ComplementKind where
    toJSON = Aeson.String . complementKindText

instance FromJSON ComplementKind where
    parseJSON = Aeson.withText "ComplementKind" $ \t ->
        case t of
            "empty" -> pure CEmpty
            "data_captured" -> pure CDataCaptured
            "defaults_required" -> pure CDefaultsRequired
            "mixed" -> pure CMixed
            other -> fail ("unknown complement kind " <> T.unpack other)

-- ---------------------------------------------------------------------------
-- Chain JSON bridge

-- | Render a chain as the @Vec<ProtolensStepInfo>@ JSON array
-- @pp_protolens_chain_to_json@ emits: a top-level JSON array of step
-- objects keyed @name@, @source_endofunctor@, @target_endofunctor@,
-- @lossless@.
chainToJson :: ProtolensChain -> LBS.ByteString
chainToJson c = Aeson.encode (map stepToJson c.steps)

-- | Parse the @Vec<ProtolensStepInfo>@ JSON array back into a chain.
-- Tolerant of the @serde@ snake_case spelling; absent fields fall back
-- to 'emptyStep'\'s defaults.
chainFromJson :: LBS.ByteString -> Either String ProtolensChain
chainFromJson bs =
    case Aeson.eitherDecode bs of
        Left err -> Left err
        Right vals -> ProtolensChain <$> traverse stepFromJson vals

-- | A step rendered as its JSON object with the @serde@ snake_case keys.
stepToJson :: ProtolensStep -> Aeson.Value
stepToJson s =
    Aeson.object
        [ ("name", Aeson.String s.name)
        , ("source_endofunctor", Aeson.String s.sourceEndofunctor)
        , ("target_endofunctor", Aeson.String s.targetEndofunctor)
        , ("lossless", Aeson.Bool s.lossless)
        ]

-- | Decode a step from its JSON object, reading the @serde@ snake_case
-- keys with defaults for absent fields.
stepFromJson :: Aeson.Value -> Either String ProtolensStep
stepFromJson = \case
    Aeson.Object o ->
        Right
            ProtolensStep
                { name = textField o "name"
                , sourceEndofunctor = textField o "source_endofunctor"
                , targetEndofunctor = textField o "target_endofunctor"
                , lossless = boolField o "lossless"
                }
    _ -> Left "protolens step: expected a JSON object"
  where
    textField o k = case KM.lookup (Key.fromText k) o of
        Just (Aeson.String t) -> t
        _ -> T.empty
    boolField o k = case KM.lookup (Key.fromText k) o of
        Just (Aeson.Bool b) -> b
        _ -> True

-- ---------------------------------------------------------------------------
-- Chain CBOR codec

-- | Encode a chain to the CBOR shape @ciborium@ produces for
-- @ProtolensChain@: a one-key map @{ "steps": [ProtolensStepInfo] }@,
-- each step a snake_case-keyed map. (The Rust @Protolens@ serializes its
-- endofunctor structure too; this binding carries the summary fields the
-- structural layer needs, mirroring the JSON surface.)
encodeChain :: ProtolensChain -> LBS.ByteString
encodeChain c =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 1
            <> Enc.encodeString "steps"
            <> encodeList encodeStep c.steps

encodeStep :: ProtolensStep -> Encoding
encodeStep s =
    Enc.encodeMapLen 4
        <> kv "name" (Enc.encodeString s.name)
        <> kv "source_endofunctor" (Enc.encodeString s.sourceEndofunctor)
        <> kv "target_endofunctor" (Enc.encodeString s.targetEndofunctor)
        <> kv "lossless" (Enc.encodeBool s.lossless)
  where
    kv k v = Enc.encodeString k <> v

encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- | Decode CBOR @ProtolensChain@ bytes into a structured chain.
-- Tolerant of unknown fields and missing optional fields, following the
-- decoder idiom of "Panproto.Instance".
decodeChain :: LBS.ByteString -> Either String ProtolensChain
decodeChain bs =
    case CBOR.deserialiseFromBytes chainDecoder bs of
        Left err -> Left (show err)
        Right (rest, c)
            | LBS.null rest -> Right c
            | otherwise -> Left "trailing bytes after CBOR-encoded protolens chain"

chainDecoder :: Decoder s ProtolensChain
chainDecoder = decodeMapWith identityChain onKey
  where
    onKey acc key = case key of
        "steps" -> (\v -> ProtolensChain v) <$> decodeListOf decodeStep
        _ -> skipTerm >> pure acc

-- The step decoder builds positionally rather than via record update so
-- it tolerates field reordering and unknown fields, matching the
-- "Panproto.Instance" / "Panproto.Schema" idiom.
decodeStep :: Decoder s ProtolensStep
decodeStep = decodeFields initial build handler
  where
    initial = (T.empty, T.empty, T.empty, True)
    build (n, src, tgt, l) = ProtolensStep n src tgt l
    handler acc@(n, src, tgt, l) key = case key of
        "name" -> (\v -> (v, src, tgt, l)) <$> Dec.decodeString
        "source_endofunctor" -> (\v -> (n, v, tgt, l)) <$> Dec.decodeString
        "target_endofunctor" -> (\v -> (n, src, v, l)) <$> Dec.decodeString
        "lossless" -> (\v -> (n, src, tgt, v)) <$> Dec.decodeBool
        _ -> skipTerm >> pure acc

decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = build <$> decodeMapWith initial onKey

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

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields.
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
        _ -> fail "decodeChain: unsupported CBOR token while skipping"
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
-- Capability class

-- | The runnable @lens@ surface of @panproto-c@ (see @CONTRACT.md@'s
-- @lens@ domain, eighteen entries). Every operation that executes a lens
-- or instantiates a chain needs the engine, so they are handle-backed
-- and live here with plain 'IO' signatures.
--
-- 'InstanceBackend' is a superclass (and 'SchemaBackend' transitively):
-- @get@ \/ @put@ move 'InstanceRep's, auto-generation and instantiation
-- consume 'SchemaRep's, and the 'Complement' threading @get@ to @put@ is
-- the shared 'Panproto.Instance.Complement' value type. Each backend
-- carries:
--
-- * 'ChainRep': a runnable handle to a 'ProtolensChain', bridged to the
--   pure value through 'ingestChain' \/ 'reifyChain';
-- * 'LensRep': a concrete lens instantiated at a schema (a compiled
--   migration plus its source and target schemas);
-- * 'SymLensRep': a symmetric lens synchronizing two schemas
--   bidirectionally.
--
-- The law checks ('checkLaws', 'checkGetPut', 'checkPutGet') throw on
-- violation (mirroring the Python surface, which raises) rather than
-- returning a 'LawCheckResult'; the structured report type is exposed
-- separately for backends that surface it.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Lens@); this
-- module declares only the class.
class (SchemaBackend back, InstanceBackend back) => LensBackend back where
    -- | Runnable representation of a 'ProtolensChain'. For 'Rust' an
    -- opaque foreign handle; for 'Native' a wrapper around the value.
    data ChainRep back :: Type

    -- | A concrete lens (compiled migration plus source\/target
    -- schemas), the artifact of instantiating a chain or auto-generating
    -- between two schemas.
    data LensRep back :: Type

    -- | A symmetric lens synchronizing two schemas in both directions.
    data SymLensRep back :: Type

    -- | Ingest a pure 'ProtolensChain' into the backend, producing a
    -- runnable 'ChainRep'.
    ingestChain :: Proxy back -> ProtolensChain -> IO (ChainRep back)

    -- | Materialize a 'ChainRep' as the pure 'ProtolensChain' summary.
    -- Wraps @pp_protolens_chain_to_json@ (@chain_to_json@).
    reifyChain :: ChainRep back -> IO ProtolensChain

    -- | Release any resources held by a representation. Idempotent at
    -- the slab level, as with the other backend reps.
    releaseChain :: ChainRep back -> IO ()

    -- | Release a 'LensRep'.
    releaseLens :: LensRep back -> IO ()

    -- | Release a 'SymLensRep'.
    releaseSymLens :: SymLensRep back -> IO ()

    -- | Project an instance through a lens: produce a view and the
    -- complement @put@ needs to reconstruct the source. Wraps
    -- @pp_lens_get_record@ (@lens::get@).
    lensGet :: LensRep back -> InstanceRep back -> IO (InstanceRep back, Complement)

    -- | Reconstruct a source instance from a (possibly modified) view
    -- and a complement. Wraps @pp_lens_put_record@ (@lens::put@).
    lensPut :: LensRep back -> InstanceRep back -> Complement -> IO (InstanceRep back)

    -- | Check both lens laws on a test instance, throwing on violation.
    -- Wraps @pp_lens_check_laws@ (@lens::check_laws@).
    checkLaws :: LensRep back -> InstanceRep back -> IO ()

    -- | Check the @GetPut@ law (@put (get s) = s@), throwing on
    -- violation. Wraps @pp_lens_check_get_put@ (@lens::check_get_put@).
    checkGetPut :: LensRep back -> InstanceRep back -> IO ()

    -- | Check the @PutGet@ law (@fst (get (put (a, c))) = a@), throwing
    -- on violation. Wraps @pp_lens_check_put_get@
    -- (@lens::check_put_get@).
    checkPutGet :: LensRep back -> InstanceRep back -> IO ()

    -- | Compose two lenses sequentially: @self ; other@. Wraps
    -- @pp_lens_compose@ (@lens::compose@).
    composeLens :: LensRep back -> LensRep back -> IO (LensRep back)

    -- | Auto-generate a lens between two schemas at the given
    -- 'Stringency', returning the lens and its alignment quality score
    -- in @[0, 1]@. Wraps @pp_lens_auto_generate_protolens@ /
    -- @auto_generate_lens@ (@lens::auto_generate@).
    autoGenerateLens
        :: SchemaRep back
        -> SchemaRep back
        -> Stringency
        -> IO (LensRep back, Double)

    -- | Auto-generate a chain between two schemas at the given
    -- 'Stringency'. Wraps @pp_lens_auto_generate_protolens@
    -- (@lens::auto_generate@): the protolens-chain form of
    -- 'autoGenerateLens'.
    autoGenerateProtolens
        :: SchemaRep back
        -> SchemaRep back
        -> Stringency
        -> IO (ChainRep back)

    -- | Auto-generate the top-@n@ candidate chains between two schemas,
    -- with any coercion proposals. Wraps
    -- @pp_lens_auto_generate_candidates@
    -- (@lens::auto_generate_candidates@).
    autoGenerateCandidates
        :: SchemaRep back
        -> SchemaRep back
        -> Int
        -> Stringency
        -> IO [ChainRep back]

    -- | Instantiate a chain at a concrete schema, producing a runnable
    -- lens. Wraps @pp_protolens_instantiate@
    -- (@ProtolensChain::instantiate@).
    instantiateChain :: ChainRep back -> SchemaRep back -> IO (LensRep back)

    -- | The static complement specification of a chain at a schema: what
    -- the complement will contain without running @get@. Wraps
    -- @pp_protolens_complement_spec@ (@lens::chain_complement_spec@). The
    -- 'ComplementKind' is the classification; the 'Text' is the
    -- human-readable summary.
    chainComplementSpec :: ChainRep back -> SchemaRep back -> IO (ComplementKind, Text)

    -- | Build a chain from a structural schema diff (CBOR @DiffSpec@)
    -- between two schemas. Wraps @pp_protolens_from_diff@
    -- (@lens::diff_to_protolens@).
    protolensFromDiff
        :: LBS.ByteString
        -> SchemaRep back
        -> SchemaRep back
        -> IO (ChainRep back)

    -- | Compose two chains into one (concatenated steps), as a backend
    -- handle. Wraps @pp_protolens_compose@. The pure counterpart is
    -- 'composeChainPure'.
    composeChain :: ChainRep back -> ChainRep back -> IO (ChainRep back)

    -- | Fuse a chain's steps into a single-step chain. Wraps
    -- @pp_protolens_fuse@ (@ProtolensChain::fuse@). The pure structural
    -- counterpart is 'fuseChain'.
    fuseChainIO :: ChainRep back -> IO (ChainRep back)

    -- | Parse a raw JSON @ProtolensChain@ document into a runnable chain
    -- handle. Wraps @pp_protolens_from_json@ (@ProtolensChain::from_json@).
    chainFromJsonIO :: Proxy back -> LBS.ByteString -> IO (ChainRep back)

    -- | The composed optic kind of a chain, as classified by the engine
    -- from the full endofunctor structure (finer than the structural
    -- 'composedOpticKind', which only distinguishes 'Iso' from 'Lens').
    composeLensOpticKind :: ChainRep back -> IO OpticKind

    -- | Auto-generate a symmetric lens synchronizing two schemas in both
    -- directions. Wraps @pp_lens_symmetric_from_schemas@
    -- (@SymmetricLens::auto_symmetric@).
    symmetricFromSchemas
        :: SchemaRep back
        -> SchemaRep back
        -> IO (SymLensRep back)

    -- | Synchronize a view through a symmetric lens. The 'Bool' is the
    -- direction: 'False' is left-to-right, 'True' is right-to-left
    -- (matching the C @direction@ byte, @0@\/@1@). Wraps
    -- @pp_lens_symmetric_sync@.
    symmetricSync
        :: SymLensRep back
        -> InstanceRep back
        -> Complement
        -> Bool
        -> IO (InstanceRep back)

    -- | Compile a lens-DSL document (JSON or YAML source) into a runnable
    -- chain, anchored at a body vertex. The first 'Text' is the source,
    -- the second the format (@"json"@ or @"yaml"@), the third the body
    -- vertex name. Wraps @pp_lens_compile_document@ (@panproto-lens-dsl@).
    compileDocument :: Proxy back -> Text -> Text -> Text -> IO (ChainRep back)
