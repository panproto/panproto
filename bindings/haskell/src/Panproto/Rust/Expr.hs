{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed expression language: the @'ExprBackend' 'Rust'@ instance.
--
-- The expression methods dispatch to the @expr@ domain of @panproto-c@
-- (see @crates\/panproto-c\/CONTRACT.md@):
--
--   * 'parseExpr' wraps @pp_expr_parse@ (UTF-8 source in, CBOR 'Expr'
--     out): it drives the @panproto_expr_parser@ Pratt parser, which is
--     not reimplemented on the Haskell side.
--   * 'evalFunc' wraps @pp_expr_eval_func@ (CBOR 'Expr' plus a CBOR
--     @Vec<(String, Literal)>@ environment in, CBOR 'Literal' out): it
--     calls @panproto_expr::eval@ with the default step\/depth limits.
--   * 'executeQuery' wraps @pp_query_execute@ (CBOR 'InstanceQuery' plus
--     a CBOR @WInstance@ and a schema handle in, a CBOR match list out):
--     it calls @inst::execute_query@.
--
-- The @eval_gat@ and @check@ entry points
-- (@pp_expr_eval_gat@\/@pp_expr_check@) also live in the @expr@ domain
-- and have correct Rust bodies, but they back the @GatBackend@ methods
-- 'Panproto.Gat.evalGatTerm'\/'Panproto.Gat.typecheckTerm', not
-- 'ExprBackend'. Their Haskell wiring lives in the @gat@ domain; this
-- module is exactly the three 'ExprBackend' methods.
--
-- == Query match representation
--
-- @pp_query_execute@ returns a CBOR array of match objects, each a map
-- with keys @node_id@, @anchor@, @value@, and @fields@ (the Rust side
-- serializes each @QueryMatch@ through @serde_json::Value@). The
-- @'QueryMatchRep' 'Rust'@ associated type is the decoded
-- 'RustQueryMatch' record: @node_id@ and @anchor@ are surfaced as typed
-- fields, while @value@ and @fields@ are kept as aeson 'Value's (the
-- general value shape the engine emits, mirrored by "Panproto.Json").
module Panproto.Rust.Expr
    ( -- * Query match representation
      RustQueryMatch (..)

      -- * The @'QueryMatchRep' 'Rust'@ data-instance constructor
    , QueryMatchRep (RustQueryMatchRep)
    ) where

import Control.Exception (throwIO)
import Data.Aeson (Value (..))
import Data.Aeson.Key qualified as Key
import Data.Aeson.KeyMap qualified as KM
import Data.ByteString.Lazy qualified as LBS
import Data.Scientific qualified as Sci
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
import Panproto.Expr
    ( ExprBackend (..)
    , decodeExpr
    , decodeLiteral
    , encodeEnvBindings
    , encodeExpr
    , encodeInstanceQuery
    )
import Panproto.Instance (encodeInstance, reifyInstance)
import Panproto.Json (cborToValue)
import Panproto.Rust (schemaRepHandle)
import Panproto.Rust.Instance () -- the @InstanceBackend Rust@ instance (a superclass of @ExprBackend Rust@)
import Panproto.Rust.FFI
    ( pp_expr_eval_func_at
    , pp_expr_parse_at
    , pp_query_execute_at
    )
import Panproto.Rust.Handle
    ( callVecOut
    , withSliceIn
    )

-- ---------------------------------------------------------------------------
-- Query match representation

-- | A single row returned by @pp_query_execute@ for the 'Rust' backend.
--
-- @node_id@ and @anchor@ are the matched node's numeric id and schema
-- anchor. @value@ is the node's scalar value (JSON @null@ when the node
-- has none), and @fields@ is the (possibly projected) field map, both
-- carried as aeson 'Value's because the engine emits them through
-- @serde_json::Value@.
data RustQueryMatch = RustQueryMatch
    { matchNodeId :: !Word32
    -- ^ The matched node's numeric id.
    , matchAnchor :: !Text
    -- ^ The matched node's schema anchor (vertex kind).
    , matchValue :: !Value
    -- ^ The matched node's scalar value, or 'Null' when absent.
    , matchFields :: !Value
    -- ^ The matched node's (possibly projected) fields, as a JSON object.
    }
    deriving stock (Eq, Show)

-- ---------------------------------------------------------------------------
-- Instance

instance ExprBackend Rust where
    data QueryMatchRep Rust = RustQueryMatchRep RustQueryMatch
        deriving stock (Eq, Show)

    parseExpr _ source = do
        bs <- withSliceIn (utf8 source) $ \ptr len ->
            callVecOut (pp_expr_parse_at ptr len)
        case decodeExpr bs of
            Right e -> pure e
            Left err -> throwIO (hostDecodeError "pp_expr_parse" err)

    evalFunc _ expr env = do
        bs <-
            withSliceIn (encodeExpr expr) $ \exprPtr exprLen ->
                withSliceIn (encodeEnvBindings env) $ \envPtr envLen ->
                    callVecOut (pp_expr_eval_func_at exprPtr exprLen envPtr envLen)
        case decodeLiteral bs of
            Right l -> pure l
            Left err -> throwIO (hostDecodeError "pp_expr_eval_func" err)

    executeQuery query instRep schema = do
        let sh = schemaRepHandle schema
        inst <- reifyInstance instRep
        bs <-
            withSliceIn (encodeInstanceQuery query) $ \queryPtr queryLen ->
                withSliceIn (encodeInstance inst) $ \instPtr instLen ->
                    callVecOut (pp_query_execute_at queryPtr queryLen instPtr instLen sh)
        case decodeMatchList bs of
            Right ms -> pure (map RustQueryMatchRep ms)
            Left err -> throwIO (hostDecodeError "pp_query_execute" err)

-- ---------------------------------------------------------------------------
-- Helpers

-- | Encode 'Text' as a UTF-8 lazy 'LBS.ByteString' for a borrowed input
-- slice.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | Decode the CBOR match list returned by @pp_query_execute@: a JSON
-- array of @{ node_id, anchor, value, fields }@ objects. The whole
-- buffer is decoded as one aeson 'Value' (via "Panproto.Json"), then
-- each array element is projected into a 'RustQueryMatch'.
decodeMatchList :: LBS.ByteString -> Either String [RustQueryMatch]
decodeMatchList bs = do
    value <- cborToValue bs
    case value of
        Array xs -> traverse matchFromValue (toList xs)
        Null -> Right []
        other -> Left ("expected a CBOR array of matches, got " <> describe other)
  where
    toList = foldr (:) []

-- | Project a single match object into a 'RustQueryMatch'. Tolerant of a
-- missing @value@ (treated as 'Null') and a missing @fields@ (treated as
-- an empty object), matching the engine's @skip_serializing@ behaviour.
matchFromValue :: Value -> Either String RustQueryMatch
matchFromValue (Object o) = do
    nid <- requireField "node_id" >>= asWord32
    anchorVal <- requireField "anchor" >>= asText
    let val = lookupField "value"
        fields = case lookupField "fields" of
            Null -> Object KM.empty
            other -> other
    Right
        RustQueryMatch
            { matchNodeId = nid
            , matchAnchor = anchorVal
            , matchValue = val
            , matchFields = fields
            }
  where
    lookupField k = maybe Null id (KM.lookup (Key.fromText k) o)
    requireField k = case KM.lookup (Key.fromText k) o of
        Just v -> Right v
        Nothing -> Left ("query match is missing the " <> T.unpack k <> " field")
matchFromValue other = Left ("expected a query match object, got " <> describe other)

asWord32 :: Value -> Either String Word32
asWord32 (Number n) = case Sci.toBoundedInteger n :: Maybe Word32 of
    Just w -> Right w
    Nothing -> Left ("node_id is out of range for Word32: " <> show n)
asWord32 other = Left ("node_id is not a number: " <> describe other)

asText :: Value -> Either String Text
asText (String t) = Right t
asText other = Left ("anchor is not a string: " <> describe other)

-- | A short human-readable tag for an aeson 'Value' constructor, for
-- decode-error messages.
describe :: Value -> String
describe = \case
    Object _ -> "an object"
    Array _ -> "an array"
    String _ -> "a string"
    Number _ -> "a number"
    Bool _ -> "a boolean"
    Null -> "null"

-- | Build the 'PanprotoError' raised when the engine returned 'StatusOk'
-- but the CBOR bytes did not decode into the expected shape. Mirrors the
-- @host_decode@ envelope tag the other "Panproto.Rust.*" backends use.
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

