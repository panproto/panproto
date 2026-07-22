{-# LANGUAGE CPP #-}
{-# LANGUAGE RankNTypes #-}

-- | Optic-ecosystem adaptors for the panproto lens surface.
--
-- This module is built only when the @optics-adaptors@ or
-- @lens-adaptors@ cabal flag is on (both set the @PANPROTO_LENS_ADAPTORS@
-- CPP macro). It prefers <https://hackage.haskell.org/package/optics-core optics-core>
-- when @optics-adaptors@ is on, and falls back to
-- <https://hackage.haskell.org/package/lens lens> (van Laarhoven) when
-- only @lens-adaptors@ is. The two are selected by Cabal's
-- per-dependency @MIN_VERSION_optics_core@ \/ @MIN_VERSION_lens@ macros.
--
-- == Why a complement-carrying lens is not a lawful @Lens'@
--
-- A "Panproto.Lens" lens is an asymmetric lens with an explicit
-- complement:
--
-- > get : s -> (a, c)
-- > put : (a, c) -> s
--
-- An optics-ecosystem @Lens' s a@ is a /van Laarhoven/ lens: a pair
--
-- > view :: s -> a
-- > set  :: a -> s -> s
--
-- satisfying @set (view s) s = s@, @view (set a s) = a@, and
-- @set b (set a s) = set b s@. The crucial difference is that
-- @set :: a -> s -> s@ takes the /whole original @s@/ as its second
-- argument, so the discarded information is recovered /from @s@
-- itself/. A panproto @put@ instead takes @(a, c)@: the discarded
-- information lives in a separate complement @c@ that the original @s@
-- is not available to supply. There is in general no total function
-- @a -> s -> s@ recovering it, because two different sources @s1@,
-- @s2@ with the same view @a@ can carry /different/ complements
-- (@c1 \/= c2@), and @put@ distinguishes them while @set@ (given only
-- @a@ and one of the @s@) cannot. So a complement-carrying lens
-- is /not/ presentable as a lawful @Lens' s a@: forcing it into that
-- shape would silently drop the complement and break @GetPut@
-- (@put (get s) = s@) for every @s@ whose complement is non-empty.
--
-- This is exactly the lossy\/lossless split the engine already tracks.
-- A "Panproto.Lens.ProtolensStep" is /lossless/ precisely when its
-- complement is empty (@'Panproto.Lens.OpticKind'@ 'Panproto.Lens.Iso'),
-- and a chain is lossless when every step is. For the lossless subset
-- the complement carries no information, so @put@ degenerates to a
-- function of the view alone and the lens /does/ coincide with a
-- lawful van Laarhoven lens. This module therefore exposes:
--
-- * __read-only 'Getter's__ over the pure structural values
--   ("Panproto.Lens.ProtolensChain", "Panproto.Lens.ProtolensStep",
--   "Panproto.Schema.Schema", "Panproto.Instance.Instance"). A 'Getter'
--   makes no lawfulness claim beyond being a pure projection, so these
--   are always lawful: they are just @view@ with no @set@.
--
-- * __lawful @Lens'@es__ only for the trivially-recoverable subset: the
--   record fields of the structural value types, where @put@ is plain
--   record update and the three van Laarhoven laws hold by
--   construction. These never cross into engine-computed,
--   complement-carrying territory.
--
-- No optic here runs a lens (@get@\/@put@) or instantiates a chain;
-- those are 'IO' operations on "Panproto.Class" backends. These are
-- pure views and field lenses over the structural layer only.
module Panproto.Lens.Optics
    (
#ifdef PANPROTO_LENS_ADAPTORS
      -- * Read-only chain views (lawful 'Getter's)
      chainStepsGetter
    , chainLengthGetter
    , chainLosslessGetter
    , composedOpticKindGetter

      -- * Lawful field lenses on the structural layer
    , stepsLens
    , stepNameLens
    , stepLosslessLens

      -- * Read-only schema and instance views
    , schemaVertexCountGetter
    , instanceNodeCountGetter
#endif
    ) where

#ifdef PANPROTO_LENS_ADAPTORS

import Data.Text (Text)

import Panproto.Instance (Instance)
import Panproto.Instance qualified as Inst
import Panproto.Lens
    ( OpticKind
    , ProtolensChain (..)
    , ProtolensStep (..)
    , chainLength
    , chainLossless
    , composedOpticKind
    )
import Panproto.Schema (Schema)
import Panproto.Schema qualified as Schema

#if defined(MIN_VERSION_optics_core)

import Optics.Core (Getter, Lens', lens, to)

-- | A read-only view of a chain's steps. Lawful as a 'Getter': it is a
-- pure projection.
chainStepsGetter :: Getter ProtolensChain [ProtolensStep]
chainStepsGetter = to (.steps)

-- | A read-only view of a chain's step count.
chainLengthGetter :: Getter ProtolensChain Int
chainLengthGetter = to chainLength

-- | A read-only view of whether a chain is lossless (every step's
-- complement empty). This is the predicate that decides whether the
-- chain coincides with a lawful van Laarhoven lens.
chainLosslessGetter :: Getter ProtolensChain Bool
chainLosslessGetter = to chainLossless

-- | A read-only view of a chain's composed optic kind.
composedOpticKindGetter :: Getter ProtolensChain OpticKind
composedOpticKindGetter = to composedOpticKind

-- | A lawful field 'Lens'' onto a chain's steps. @put@ is record
-- update, so all three van Laarhoven laws hold by construction. This is
-- a lens over the /structural representation/, not the complement-carrying
-- lens it describes.
stepsLens :: Lens' ProtolensChain [ProtolensStep]
stepsLens = lens (.steps) (\c ss -> c {steps = ss})

-- | A lawful field 'Lens'' onto a step's name.
stepNameLens :: Lens' ProtolensStep Text
stepNameLens = lens (.name) (\s n -> s {name = n})

-- | A lawful field 'Lens'' onto a step's lossless flag.
stepLosslessLens :: Lens' ProtolensStep Bool
stepLosslessLens = lens (.lossless) (\s l -> s {lossless = l})

-- | A read-only view of a schema's vertex count: a pure projection,
-- lawful as a 'Getter'.
schemaVertexCountGetter :: Getter Schema Int
schemaVertexCountGetter = to Schema.vertexCount

-- | A read-only view of an instance's node count.
instanceNodeCountGetter :: Getter Instance Int
instanceNodeCountGetter = to Inst.nodeCount

#elif defined(MIN_VERSION_lens)

import Control.Lens (Getter, Lens', lens, to)

-- | A read-only view of a chain's steps. Lawful as a 'Getter': it is a
-- pure projection.
chainStepsGetter :: Getter ProtolensChain [ProtolensStep]
chainStepsGetter = to (.steps)

-- | A read-only view of a chain's step count.
chainLengthGetter :: Getter ProtolensChain Int
chainLengthGetter = to chainLength

-- | A read-only view of whether a chain is lossless (every step's
-- complement empty). This is the predicate that decides whether the
-- chain coincides with a lawful van Laarhoven lens.
chainLosslessGetter :: Getter ProtolensChain Bool
chainLosslessGetter = to chainLossless

-- | A read-only view of a chain's composed optic kind.
composedOpticKindGetter :: Getter ProtolensChain OpticKind
composedOpticKindGetter = to composedOpticKind

-- | A lawful field 'Lens'' onto a chain's steps. @put@ is record
-- update, so all three van Laarhoven laws hold by construction. This is
-- a lens over the /structural representation/, not the complement-carrying
-- lens it describes.
stepsLens :: Lens' ProtolensChain [ProtolensStep]
stepsLens = lens (.steps) (\c ss -> c {steps = ss})

-- | A lawful field 'Lens'' onto a step's name.
stepNameLens :: Lens' ProtolensStep Text
stepNameLens = lens (.name) (\s n -> s {name = n})

-- | A lawful field 'Lens'' onto a step's lossless flag.
stepLosslessLens :: Lens' ProtolensStep Bool
stepLosslessLens = lens (.lossless) (\s l -> s {lossless = l})

-- | A read-only view of a schema's vertex count: a pure projection,
-- lawful as a 'Getter'.
schemaVertexCountGetter :: Getter Schema Int
schemaVertexCountGetter = to Schema.vertexCount

-- | A read-only view of an instance's node count.
instanceNodeCountGetter :: Getter Instance Int
instanceNodeCountGetter = to Inst.nodeCount

#else

-- The lens-adaptors cabal stanza always pulls in exactly one of
-- @optics-core@ or @lens@, so one of the branches above is taken. This
-- guard exists only so the module is well formed if the macro is set
-- without either dependency; it deliberately fails to compile with a
-- clear message rather than emitting an empty surface.
#error "Panproto.Lens.Optics: PANPROTO_LENS_ADAPTORS is set but neither optics-core nor lens is available."

#endif

#endif
