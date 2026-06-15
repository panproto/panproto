{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}

-- | Structured Haskell view of a panproto @Protocol@.
--
-- A protocol names the schema and instance theories a data format
-- uses, the well-formedness rules for its edges, the recognized vertex
-- kinds and constraint sorts, and the structural and enrichment
-- feature flags that the GAT layer reads to choose which theory
-- fragments apply. The full field set already lives on
-- 'Panproto.Canonical.CanonicalProtocol' (the cold-path CBOR exchange
-- type), so 'Protocol' is a thin @newtype@ over it rather than a
-- duplicate record: it carries the same data and the same wire format,
-- adding only the pure smart constructors and the JSON view this layer
-- exposes.
--
-- Registry lookups and protocol resolution against the Rust backend
-- are a later wave; this module is pure.
module Panproto.Protocol
    ( -- * Protocol
      Protocol (..)
    , toCanonicalProtocol

      -- * Construction
    , fromTheories
    , protocolBuilder
    ) where

import Control.DeepSeq (NFData)
import Control.Monad.Trans.State.Strict (State, execState)
import Data.Aeson (ToJSON (..), object, (.=))
import Data.Text (Text)
import GHC.Generics (Generic)

import Panproto.Canonical
    ( CanonicalProtocol (..)
    , EdgeRule (..)
    , defaultProtocol
    )

-- | A panproto protocol. The single field is the
-- 'CanonicalProtocol' it wraps, so every protocol field is reachable
-- through @.canonical@ (for example @p.canonical.name@) and the value
-- round-trips through the same CBOR codec the canonical type uses.
newtype Protocol = Protocol {canonical :: CanonicalProtocol}
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Project the wrapped 'CanonicalProtocol'.
toCanonicalProtocol :: Protocol -> CanonicalProtocol
toCanonicalProtocol (Protocol c) = c

-- | Build a 'Protocol' from its theory names and feature flags.
--
-- Every structural and enrichment flag is an explicit argument so the
-- caller spells out the full GAT configuration; edge rules, object
-- kinds, and constraint sorts default to empty and are added with
-- 'protocolBuilder'. The two theory composition recipes are left
-- unset (the canonical codec emits them as @null@).
fromTheories
    :: Text
    -- ^ Protocol name.
    -> Text
    -- ^ Schema theory name.
    -> Text
    -- ^ Instance theory name.
    -> Bool
    -- ^ @has_order@
    -> Bool
    -- ^ @has_coproducts@
    -> Bool
    -- ^ @has_recursion@
    -> Bool
    -- ^ @has_causal@
    -> Bool
    -- ^ @nominal_identity@
    -> Bool
    -- ^ @has_defaults@
    -> Bool
    -- ^ @has_coercions@
    -> Bool
    -- ^ @has_mergers@
    -> Bool
    -- ^ @has_policies@
    -> Protocol
fromTheories
    pname
    schemaTheory'
    instanceTheory'
    hasOrder'
    hasCoproducts'
    hasRecursion'
    hasCausal'
    nominalIdentity'
    hasDefaults'
    hasCoercions'
    hasMergers'
    hasPolicies' =
        Protocol
            defaultProtocol
                { name = pname
                , schemaTheory = schemaTheory'
                , instanceTheory = instanceTheory'
                , hasOrder = hasOrder'
                , hasCoproducts = hasCoproducts'
                , hasRecursion = hasRecursion'
                , hasCausal = hasCausal'
                , nominalIdentity = nominalIdentity'
                , hasDefaults = hasDefaults'
                , hasCoercions = hasCoercions'
                , hasMergers = hasMergers'
                , hasPolicies = hasPolicies'
                }

-- | Assemble a 'Protocol' imperatively over the wrapped
-- 'CanonicalProtocol', starting from 'defaultProtocol'.
protocolBuilder :: State CanonicalProtocol () -> Protocol
protocolBuilder = Protocol . (`execState` defaultProtocol)

-- ---------------------------------------------------------------------------
-- JSON

instance ToJSON Protocol where
    toJSON (Protocol p) =
        object
            [ "name" .= p.name
            , "schema_theory" .= p.schemaTheory
            , "instance_theory" .= p.instanceTheory
            , "edge_rules" .= map edgeRuleJSON p.edgeRules
            , "obj_kinds" .= p.objKinds
            , "constraint_sorts" .= p.constraintSorts
            , "has_order" .= p.hasOrder
            , "has_coproducts" .= p.hasCoproducts
            , "has_recursion" .= p.hasRecursion
            , "has_causal" .= p.hasCausal
            , "nominal_identity" .= p.nominalIdentity
            , "has_defaults" .= p.hasDefaults
            , "has_coercions" .= p.hasCoercions
            , "has_mergers" .= p.hasMergers
            , "has_policies" .= p.hasPolicies
            ]
      where
        edgeRuleJSON r =
            object
                [ "edge_kind" .= r.edgeKind
                , "src_kinds" .= r.srcKinds
                , "tgt_kinds" .= r.tgtKinds
                ]
