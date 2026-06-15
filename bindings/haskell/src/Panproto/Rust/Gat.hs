{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed GAT operations: the @'GatBackend' 'Rust'@ instance.
--
-- Theories live in @libpanproto_c@'s slab as @Resource::Theory@ handles;
-- 'ingestTheory' allocates one from a structured 'Theory' and
-- 'colimitTheories' \/ 'checkMorphism' dispatch to the @gat@ domain of
-- @panproto-c@ (see @crates\/panproto-c\/CONTRACT.md@):
-- @pp_gat_create_theory@, @pp_gat_colimit@, @pp_gat_check_morphism@, and
-- @pp_gat_migrate_model@.
--
-- == Reifying a theory handle
--
-- The @gat@ C ABI is exactly those four entry points; it exposes no
-- theory serializer (no @pp_gat_serialize@), so a bare slab handle
-- cannot be read back to CBOR. (The WASM reference surface
-- @crates\/panproto-wasm\/src\/api\/gat.rs@ is identical in this
-- respect, and the Python binding reifies by holding the structured
-- @Theory@ in the wrapper rather than round-tripping the engine.) The
-- Rust 'TheoryRep' therefore pairs the slab handle with the structured
-- 'Theory' it was built from, so 'reifyTheory' returns that value
-- directly. A handle produced by the engine ('colimitTheories') has no
-- such Haskell-side structured form; 'reifyTheory' on it raises a clear
-- 'PanprotoError' rather than fabricate one.
--
-- == Operations with no @gat@-domain symbol
--
-- Three things the pure 'GatBackend' class advertises cannot be served
-- by this backend:
--
-- * 'freeModel' and 'checkModel' map to @gat::free_model@ \/
--   @gat::check_model@, which are not exposed across the C ABI (a
--   'Model' carries operation closures that cannot serialize). They
--   throw 'PanprotoError' with 'StatusOperation'.
--
-- * 'evalGatTerm' and 'typecheckTerm' map to @pp_expr_eval_gat@ \/
--   @pp_expr_check@, which live in the @expr@ domain
--   (@crates\/panproto-c\/src\/api\/expr.rs@), not @gat@. That module is
--   wired separately; until it is, these throw 'PanprotoError' with
--   'StatusOperation' rather than dispatch into an unimplemented symbol.
--   The FFI declarations (@pp_expr_eval_gat_at@, @pp_expr_check_at@)
--   already exist, so wiring them is a localized follow-up that does not
--   touch the @gat@ surface.
--
-- A 'ModelRep' for the Rust backend never holds a slab handle: no @gat@
-- entry point produces a model handle. 'migrateModel' is pure CBOR-in \/
-- CBOR-out and needs no theory or model handle at all.
module Panproto.Rust.Gat
    ( -- * Theory representation
      RustTheory (..)
    , withRustTheory
    ) where

import Control.Exception (bracket, throwIO)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32)

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Gat
    ( GatBackend (..)
    , Model
    , MorphismCheckResult
    , Theory
    , TheoryMorphism (..)
    , decodeModelSortInterp
    , decodeMorphismCheckResult
    , encodeModelSortInterp
    , encodeMorphism
    , encodeTheory
    )
import Panproto.Rust.FFI
    ( pp_gat_check_morphism_at
    , pp_gat_colimit
    , pp_gat_create_theory_at
    , pp_gat_migrate_model_at
    , pp_handle_free
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

-- ---------------------------------------------------------------------------
-- Theory handle

-- | A handle into @panproto-c@\'s slab pointing at a @Resource::Theory@.
newtype RustTheory = RustTheory {theoryHandle :: Word32}
    deriving stock (Eq, Show)

-- | Bracket a 'RustTheory' from a structured 'Theory' so its slab slot
-- is released even when the inner action throws. Preferred over manual
-- 'ingestTheory' \/ 'releaseTheory' pairing.
withRustTheory :: Theory -> (RustTheory -> IO a) -> IO a
withRustTheory t = bracket (ingestRustTheory t) freeRustTheory

-- ---------------------------------------------------------------------------
-- Instance

instance GatBackend Rust where
    -- The slab handle paired with the structured theory it was built
    -- from, if known. 'Just' for an ingested theory ('ingestTheory'),
    -- 'Nothing' for an engine-produced one ('colimitTheories'), which
    -- has no Haskell-side structured form.
    data TheoryRep Rust = RustTheoryRep !RustTheory !(Maybe Theory)

    -- A 'Model' never gets a slab handle in the @gat@ C ABI: no entry
    -- point produces one. The representation is inert; the
    -- model-producing methods ('freeModel', 'checkModel') are
    -- unsupported by this backend (see the module header).
    data ModelRep Rust = RustModelUnsupported

    ingestTheory _ t = do
        h <- ingestRustTheory t
        pure (RustTheoryRep h (Just t))

    reifyTheory (RustTheoryRep _ (Just t)) = pure t
    reifyTheory (RustTheoryRep _ Nothing) = throwIO reifyUnavailableError

    releaseTheory (RustTheoryRep r _) = freeRustTheory r

    colimitTheories
        (RustTheoryRep (RustTheory t1) _)
        (RustTheoryRep (RustTheory t2) _)
        (RustTheoryRep (RustTheory shared) _) = do
            h <- callHandleOut (pp_gat_colimit t1 t2 shared)
            pure (RustTheoryRep (RustTheory h) Nothing)

    checkMorphism morph (RustTheoryRep (RustTheory dom) _) (RustTheoryRep (RustTheory cod) _) =
        checkRustMorphism morph dom cod

    migrateModel _ = migrateRustModel

    -- Unsupported by the @gat@ C ABI (see module header).
    freeModel _ _ _ = unsupportedModelOp "freeModel" "gat::free_model"
    checkModel _ _ = unsupportedModelOp "checkModel" "gat::check_model"
    modelTheoryNameIO _ = unsupportedModelOp "modelTheoryNameIO" "a model handle"
    sortInterpKeysIO _ = unsupportedModelOp "sortInterpKeysIO" "a model handle"
    releaseModel RustModelUnsupported = pure ()

    -- Wired in the @expr@ domain, not @gat@ (see module header).
    evalGatTerm _ _ _ = unsupportedExprOp "evalGatTerm" "pp_expr_eval_gat"
    typecheckTerm _ _ _ = unsupportedExprOp "typecheckTerm" "pp_expr_check"

-- ---------------------------------------------------------------------------
-- Theory lifecycle

ingestRustTheory :: Theory -> IO RustTheory
ingestRustTheory t =
    withSliceIn (encodeTheory t) $ \ptr len ->
        RustTheory <$> callHandleOut (pp_gat_create_theory_at ptr len)

freeRustTheory :: RustTheory -> IO ()
freeRustTheory (RustTheory h) = do
    status <- pp_handle_free h
    checkStatus status

-- ---------------------------------------------------------------------------
-- Morphism checking and model migration

checkRustMorphism :: TheoryMorphism -> Word32 -> Word32 -> IO MorphismCheckResult
checkRustMorphism morph dom cod = do
    bs <-
        withSliceIn (encodeMorphism morph) $ \ptr len ->
            callVecOut (pp_gat_check_morphism_at ptr len dom cod)
    case decodeMorphismCheckResult bs of
        Right r -> pure r
        Left err -> throwIO (hostDecodeError "pp_gat_check_morphism" err)

migrateRustModel :: TheoryMorphism -> Model -> IO Model
migrateRustModel morph model = do
    bs <-
        withSliceIn (encodeModelSortInterp model) $ \mPtr mLen ->
            withSliceIn (encodeMorphism morph) $ \morphPtr morphLen ->
                callVecOut (pp_gat_migrate_model_at mPtr mLen morphPtr morphLen)
    -- The reindexed sort interpretations are bound to the morphism's
    -- domain theory: the result model interprets the domain.
    case decodeModelSortInterp morph.domain bs of
        Right m -> pure m
        Left err -> throwIO (hostDecodeError "pp_gat_migrate_model" err)

-- ---------------------------------------------------------------------------
-- Errors

-- | 'reifyTheory' on an engine-produced handle (e.g. a colimit result),
-- which has no Haskell-side structured 'Theory' and no @gat@-domain
-- serializer to recover one.
reifyUnavailableError :: PanprotoError
reifyUnavailableError =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "unsupported"
                    , message =
                        "reifyTheory: the gat C ABI exposes no theory serializer, "
                            <> "so an engine-produced theory handle (colimit result) "
                            <> "cannot be read back to a structured Theory"
                    }
        }

-- | Throw for a model-producing operation that has no @gat@-domain C ABI
-- symbol (@gat::free_model@, @gat::check_model@, or a model handle).
unsupportedModelOp :: Text -> Text -> IO a
unsupportedModelOp method backing =
    throwIO (unsupportedError method backing "the gat C ABI exposes no model handle")

-- | Throw for a term operation served by the @expr@ domain rather than
-- @gat@. The @expr@ surface is wired separately.
unsupportedExprOp :: Text -> Text -> IO a
unsupportedExprOp method backing =
    throwIO (unsupportedError method backing "wired in the expr domain (api/expr.rs)")

unsupportedError :: Text -> Text -> Text -> PanprotoError
unsupportedError method backing reason =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "unsupported"
                    , message =
                        method
                            <> " (backed by "
                            <> backing
                            <> ") is not available through the GatBackend Rust instance: "
                            <> reason
                    }
        }

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
                        "panproto could not decode the CBOR returned by "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }
