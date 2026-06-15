{-# LANGUAGE CPP #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE UndecidableInstances #-}

-- | Effect-system adaptors for panproto operations.
--
-- Every panproto operation in this binding returns plain 'IO'. To run
-- those operations inside a richer monad, a caller lifts them through
-- 'liftPanproto'. The 'MonadPanproto' class names that capability and
-- provides instances for 'IO' and the common @transformers@ stacks, so
-- @mtl@-style code can call panproto without an explicit 'liftIO' at
-- every site.
--
-- When the @effectful@ flag is set, this module additionally exposes a
-- first-class @effectful@ 'Panproto' effect and an 'Eff' instance of
-- 'MonadPanproto', letting @effectful@ programs run panproto
-- operations through the same vocabulary. With the flag off (the
-- default) the module compiles against @transformers@ and @mtl@ alone,
-- so the @effectful-core@ dependency is never required for a default
-- build.
module Panproto.Effect
    ( MonadPanproto (..)
#ifdef PANPROTO_EFFECTFUL
    , Panproto
    , runPanproto
#endif
    ) where

import Control.Monad.IO.Class (MonadIO, liftIO)
import Control.Monad.Trans.Class (lift)
import Control.Monad.Trans.Except (ExceptT)
import Control.Monad.Trans.Reader (ReaderT)
import Control.Monad.Trans.State.Lazy qualified as Lazy
import Control.Monad.Trans.State.Strict qualified as Strict

#ifdef PANPROTO_EFFECTFUL
import Effectful (Dispatch (Static), DispatchOf, Eff, Effect, IOE, (:>))
import Effectful.Dispatch.Static
    ( SideEffects (WithSideEffects)
    , StaticRep
    , evalStaticRep
    , getStaticRep
    , unsafeEff_
    )
#endif

-- | A monad in which panproto's @IO@-returning operations can run.
--
-- 'liftPanproto' is the single primitive: it embeds an @IO@ panproto
-- action into the carrier monad. For @transformers@ stacks this is
-- 'liftIO' threaded through the transformer layers; for a bare @IO@ it
-- is the identity.
class MonadIO m => MonadPanproto m where
    -- | Run an @IO@ panproto operation in the carrier monad.
    liftPanproto :: IO a -> m a
    liftPanproto = liftIO

instance MonadPanproto IO where
    liftPanproto = id

instance MonadPanproto m => MonadPanproto (ReaderT r m) where
    liftPanproto = lift . liftPanproto

instance MonadPanproto m => MonadPanproto (Lazy.StateT s m) where
    liftPanproto = lift . liftPanproto

instance MonadPanproto m => MonadPanproto (Strict.StateT s m) where
    liftPanproto = lift . liftPanproto

instance MonadPanproto m => MonadPanproto (ExceptT e m) where
    liftPanproto = lift . liftPanproto

#ifdef PANPROTO_EFFECTFUL

-- | An @effectful@ effect for panproto operations.
--
-- The effect is a thin static effect: it carries no state and exists
-- only to gate the 'MonadPanproto' instance for 'Eff'. 'runPanproto'
-- discharges it against the ambient 'IOE'.
data Panproto :: Effect

type instance DispatchOf Panproto = Static WithSideEffects

data instance StaticRep Panproto = Panproto

-- | Discharge the 'Panproto' effect, requiring an ambient 'IOE' to run
-- the underlying @IO@ operations against.
runPanproto :: IOE :> es => Eff (Panproto : es) a -> Eff es a
runPanproto = evalStaticRep Panproto

instance (IOE :> es, Panproto :> es) => MonadPanproto (Eff es) where
    liftPanproto io = do
        -- Consult the effect's (unit) representation so the
        -- @Panproto :> es@ constraint is load-bearing: the instance is
        -- usable only once 'runPanproto' has brought the effect into
        -- scope, not in any @IOE@-carrying stack.
        Panproto <- getStaticRep
        unsafeEff_ io

#endif
