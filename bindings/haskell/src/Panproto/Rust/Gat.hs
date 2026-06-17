{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed GAT operations: the @'GatBackend' 'Rust'@ instance.
--
-- Theories and models live in @libpanproto_c@'s slab as
-- @Resource::Theory@ and @Resource::Model@ handles. 'ingestTheory'
-- allocates a theory from a structured 'Theory'; 'colimitTheories' \/
-- 'checkMorphism' \/ 'freeModel' \/ 'checkModel' dispatch to the @gat@
-- domain of @panproto-c@ (see @crates\/panproto-c\/CONTRACT.md@):
-- @pp_gat_create_theory@, @pp_gat_colimit@, @pp_gat_check_morphism@,
-- @pp_gat_migrate_model@, @pp_gat_free_model@, @pp_gat_check_model@, and
-- @pp_gat_serialize_theory@.
--
-- == Reifying a theory handle
--
-- 'reifyTheory' calls @pp_gat_serialize_theory@, which emits the CBOR
-- 'Theory' in the same shape @pp_gat_create_theory@ ingests. It works
-- uniformly for an ingested theory and for an engine-produced one (a
-- 'colimitTheories' result), so a 'TheoryRep' need carry only the slab
-- handle: there is no Haskell-side structured cache.
--
-- == Models
--
-- A model is a @Resource::Model@ slab handle. @pp_gat_free_model@
-- constructs one and returns the handle. The model is fully evaluable and
-- its carrier is extractable across the boundary: @pp_gat_eval_in_model@
-- runs an operation's interpretation (a closure held in-process) on
-- argument 'ModelValue's and returns the result, and
-- @pp_gat_model_sort_interp@ emits the model's full carrier (its
-- sort-interpretation map). @pp_gat_check_model@ checks a model handle
-- against a theory handle and returns the equation-violation list.
--
-- Every model here is the deterministic @pp_gat_free_model@ of a theory
-- under a config, so a model serializes losslessly as its recipe: the
-- 'ModelRep' retains the source 'Theory' and the @max_depth@ \/
-- @max_terms_per_sort@ config it was built from, and re-running
-- 'freeModel' on that theory and config reconstructs an identical model.
-- 'reifyModel' returns the structured 'Model' (the theory name plus the
-- full @sort_interp@ read via @pp_gat_model_sort_interp@). The retained
-- theory also backs the cheaper 'modelTheoryNameIO' \/ 'sortInterpKeysIO'
-- accessors without re-reading the slab. (Python's @Model.theory_name@ \/
-- @Model.sort_interp_keys@ are the parity targets.)
--
-- == Operations with no @gat@-domain symbol
--
-- 'evalGatTerm' and 'typecheckTerm' map to @pp_expr_eval_gat@ \/
-- @pp_expr_check@, which live in the @expr@ domain
-- (@crates\/panproto-c\/src\/api\/expr.rs@), not @gat@. Both take the
-- 'TheoryRep' slab handle alongside a CBOR 'Term' and a CBOR environment
-- \/ context: 'evalGatTerm' encodes the env as @Vec<(String, ModelValue)>@
-- and decodes a 'ModelValue'; 'typecheckTerm' encodes the context as
-- @Vec<(String, String)>@ and decodes a 'TypecheckResult'.
--
-- 'migrateModel' is pure CBOR-in \/ CBOR-out and needs no theory or
-- model handle at all.
module Panproto.Rust.Gat
    ( -- * Theory representation
      RustTheory (..)
    , withRustTheory
    ) where

import Control.Exception (bracket, throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
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
    , Model (..)
    , ModelValue
    , MorphismCheckResult
    , Sort (..)
    , Term
    , Theory
    , TheoryMorphism (..)
    , TypecheckResult
    , decodeModelSortInterp
    , decodeModelSortInterpMap
    , decodeModelValueBytes
    , decodeMorphismCheckResult
    , decodeStringList
    , decodeTheory
    , decodeTypecheckResult
    , encodeFreeModelConfig
    , encodeModelSortInterp
    , encodeModelValueList
    , encodeMorphism
    , encodeSortContext
    , encodeTermBytes
    , encodeTermEnv
    , encodeTheory
    , sorts
    , theoryName
    )
import Panproto.Rust.FFI
    ( pp_expr_check_at
    , pp_expr_eval_gat_at
    , pp_gat_check_model
    , pp_gat_check_morphism_at
    , pp_gat_colimit
    , pp_gat_create_theory_at
    , pp_gat_eval_in_model_at
    , pp_gat_free_model_at
    , pp_gat_migrate_model_at
    , pp_gat_model_sort_interp
    , pp_gat_serialize_theory
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
    -- The bare slab handle. 'reifyTheory' recovers the structured
    -- 'Theory' on demand via @pp_gat_serialize_theory@, so no
    -- Haskell-side cache is kept and an engine-produced handle (a colimit
    -- result) reifies the same way an ingested one does.
    newtype TheoryRep Rust = RustTheoryRep RustTheory

    -- A @Resource::Model@ slab handle paired with the recipe that built
    -- it: the source 'Theory' and the @(max_depth, max_terms_per_sort)@
    -- config. The recipe is deterministic, so re-running 'freeModel' on it
    -- reconstructs an identical model (a model serializes losslessly as
    -- its recipe). The retained theory's name and sort names back
    -- 'modelTheoryNameIO' \/ 'sortInterpKeysIO' without re-reading the
    -- slab; the model's full carrier is read on demand through
    -- @pp_gat_model_sort_interp@ in 'reifyModel' \/ 'modelSortInterp'.
    data ModelRep Rust = RustModelRep !Word32 !Theory !(Int, Int)

    ingestTheory _ t = RustTheoryRep <$> ingestRustTheory t

    reifyTheory (RustTheoryRep (RustTheory th)) = reifyRustTheory th

    releaseTheory (RustTheoryRep r) = freeRustTheory r

    colimitTheories
        (RustTheoryRep (RustTheory t1))
        (RustTheoryRep (RustTheory t2))
        (RustTheoryRep (RustTheory shared)) = do
            h <- callHandleOut (pp_gat_colimit t1 t2 shared)
            pure (RustTheoryRep (RustTheory h))

    checkMorphism morph (RustTheoryRep (RustTheory dom)) (RustTheoryRep (RustTheory cod)) =
        checkRustMorphism morph dom cod

    migrateModel _ = migrateRustModel

    freeModel (RustTheoryRep (RustTheory th)) maxDepth maxTerms =
        freeRustModel th maxDepth maxTerms

    checkModel (RustModelRep model _ _) (RustTheoryRep (RustTheory th)) =
        checkRustModel model th

    -- Served from the recipe captured at 'freeModel' time (see module
    -- header): a free model interprets exactly its source theory, whose
    -- sort names key its carrier sets.
    modelTheoryNameIO (RustModelRep _ theory _) = pure (theoryName theory)
    sortInterpKeysIO (RustModelRep _ theory _) = pure [s.sortName | s <- sorts theory]

    -- Evaluate an operation's in-process closure against argument values,
    -- returning the resulting 'ModelValue' (see module header).
    evalInModel (RustModelRep model _ _) opName args =
        evalRustInModel model opName args

    -- The model's full carrier, read from the slab via
    -- @pp_gat_model_sort_interp@.
    modelSortInterp (RustModelRep model _ _) = modelSortInterpRust model

    -- Reify to the structured 'Model': the recipe's theory name plus the
    -- live carrier read from the slab.
    reifyModel (RustModelRep model theory _) = do
        si <- modelSortInterpRust model
        pure Model {theory = theoryName theory, sortInterp = si}

    releaseModel (RustModelRep model _ _) = freeRustModelHandle model

    -- Served by the @expr@ domain against a theory handle (see module
    -- header): @pp_expr_eval_gat@ \/ @pp_expr_check@.
    evalGatTerm (RustTheoryRep (RustTheory th)) term env =
        evalRustGatTerm th term env
    typecheckTerm (RustTheoryRep (RustTheory th)) term ctx =
        typecheckRustTerm th term ctx

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

-- | Reify a theory handle to a structured 'Theory' via
-- @pp_gat_serialize_theory@. Works for both ingested and engine-produced
-- (colimit-result) handles.
reifyRustTheory :: Word32 -> IO Theory
reifyRustTheory th = do
    bs <- callVecOut (pp_gat_serialize_theory th)
    case decodeTheory bs of
        Right t -> pure t
        Left err -> throwIO (hostDecodeError "pp_gat_serialize_theory" err)

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
-- Free model construction and checking

-- | Construct the free model of a theory handle under a @max_depth@ \/
-- @max_terms_per_sort@ bound. Wraps @pp_gat_free_model@: the bound goes
-- in as a borrowed CBOR @{ max_depth, max_terms_per_sort }@ slice, the
-- theory as a slab handle, and a fresh @Resource::Model@ handle comes
-- back. The source theory is reified (via @pp_gat_serialize_theory@) and
-- retained alongside the config so the model is reconstructable from its
-- recipe and its name \/ sort names back 'modelTheoryNameIO' \/
-- 'sortInterpKeysIO' (see the module header).
freeRustModel :: Word32 -> Int -> Int -> IO (ModelRep Rust)
freeRustModel th maxDepth maxTerms = do
    theory <- reifyRustTheory th
    model <-
        withSliceIn (encodeFreeModelConfig maxDepth maxTerms) $ \cfgPtr cfgLen ->
            callHandleOut (pp_gat_free_model_at th cfgPtr cfgLen)
    pure (RustModelRep model theory (maxDepth, maxTerms))

-- | Check a model handle against a theory handle, decoding the CBOR
-- @Vec\<String\>@ of equation-violation descriptions. Wraps
-- @pp_gat_check_model@.
checkRustModel :: Word32 -> Word32 -> IO [Text]
checkRustModel model th = do
    bs <- callVecOut (pp_gat_check_model model th)
    case decodeStringList bs of
        Right vs -> pure vs
        Left err -> throwIO (hostDecodeError "pp_gat_check_model" err)

-- | Evaluate an operation in a model handle: the UTF-8 op name and a CBOR
-- @Vec\<ModelValue\>@ of arguments go in as borrowed slices, the result
-- 'ModelValue' comes back as CBOR. Wraps @pp_gat_eval_in_model@.
evalRustInModel :: Word32 -> Text -> [ModelValue] -> IO ModelValue
evalRustInModel model opName args = do
    bs <-
        withSliceIn (textToSlice opName) $ \namePtr nameLen ->
            withSliceIn (encodeModelValueList args) $ \argsPtr argsLen ->
                callVecOut (pp_gat_eval_in_model_at model namePtr nameLen argsPtr argsLen)
    case decodeModelValueBytes bs of
        Right v -> pure v
        Left err -> throwIO (hostDecodeError "pp_gat_eval_in_model" err)

-- | Read a model handle's full carrier (its sort-interpretation map) as a
-- CBOR @HashMap\<String, Vec\<ModelValue\>\>@. Wraps
-- @pp_gat_model_sort_interp@.
modelSortInterpRust :: Word32 -> IO (HashMap Text [ModelValue])
modelSortInterpRust model = do
    bs <- callVecOut (pp_gat_model_sort_interp model)
    case decodeModelSortInterpMap bs of
        Right si -> pure si
        Left err -> throwIO (hostDecodeError "pp_gat_model_sort_interp" err)

-- | Release a model slab handle.
freeRustModelHandle :: Word32 -> IO ()
freeRustModelHandle h = do
    status <- pp_handle_free h
    checkStatus status

-- ---------------------------------------------------------------------------
-- Term evaluation and typechecking (expr domain, theory-aware)

-- | Evaluate a GAT 'Term' to a 'ModelValue' under a variable environment
-- and a theory handle. Wraps @pp_expr_eval_gat@: the term and the
-- @Vec<(String, ModelValue)>@ environment go in as borrowed CBOR slices,
-- the theory as a slab handle, and the result 'ModelValue' comes back as
-- CBOR.
evalRustGatTerm :: Word32 -> Term -> [(Text, ModelValue)] -> IO ModelValue
evalRustGatTerm th term env = do
    bs <-
        withSliceIn (encodeTermBytes term) $ \termPtr termLen ->
            withSliceIn (encodeTermEnv env) $ \envPtr envLen ->
                callVecOut (pp_expr_eval_gat_at termPtr termLen envPtr envLen th)
    case decodeModelValueBytes bs of
        Right v -> pure v
        Left err -> throwIO (hostDecodeError "pp_expr_eval_gat" err)

-- | Typecheck a GAT 'Term' against a theory handle under a typing context
-- (variable name to sort name). Wraps @pp_expr_check@: the term and the
-- @Vec<(String, String)>@ context go in as borrowed CBOR slices, the
-- theory as a slab handle, and the @{ well_formed, output_sort, error }@
-- verdict comes back as a CBOR 'TypecheckResult'. The engine returns
-- 'StatusOk' for both well-formed and ill-formed terms, so a 'False'
-- 'Panproto.Gat.wellFormed' is a normal result, not an exception.
typecheckRustTerm :: Word32 -> Term -> [(Text, Text)] -> IO TypecheckResult
typecheckRustTerm th term ctx = do
    bs <-
        withSliceIn (encodeTermBytes term) $ \termPtr termLen ->
            withSliceIn (encodeSortContext ctx) $ \ctxPtr ctxLen ->
                callVecOut (pp_expr_check_at termPtr termLen th ctxPtr ctxLen)
    case decodeTypecheckResult bs of
        Right r -> pure r
        Left err -> throwIO (hostDecodeError "pp_expr_check" err)

-- ---------------------------------------------------------------------------
-- Marshalling helpers

-- | Encode 'Text' as the UTF-8 byte buffer the @*_at@ glue expects.
textToSlice :: Text -> LBS.ByteString
textToSlice = LBS.fromStrict . TE.encodeUtf8

-- ---------------------------------------------------------------------------
-- Errors

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
