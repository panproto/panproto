{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE TypeFamilies #-}

-- | The panproto expression language: a pure value-type and capability
-- layer mirroring @panproto_expr@ (the AST and evaluator) and
-- @panproto_inst@'s declarative query type.
--
-- The expression language is a pure functional language: lambda calculus
-- with pattern matching, records, lists, and around sixty built-in
-- operations on strings, numbers, records, and lists. Evaluation is
-- deterministic; there is no IO, mutation, or platform dependence.
--
-- This module is the pure value layer. It carries:
--
--   * 'Expr', 'Literal', 'Pattern', and 'BuiltinOp' value types that
--     mirror the Rust enums in @crates\/panproto-expr@ field-for-field.
--   * 'InstanceQuery', the declarative query type from
--     @crates\/panproto-inst@.
--   * Tolerant cborg codecs wire-compatible with the Rust side's
--     @ciborium@ serialization (externally tagged enums, @snake_case@
--     struct fields, @serde(default)@ + skip-unknown semantics).
--   * 'prettyPrintExpr', a pure reimplementation of
--     @panproto_expr_parser::pretty_print@.
--   * 'ExprBackend', the capability class whose engine methods
--     ('parseExpr', 'evalFunc', 'executeQuery') correspond to the
--     @pp_expr_parse@, @pp_expr_eval_func@, and @pp_query_execute@ FFI
--     entry points. No backend instance lives here: the Rust instance
--     lives in @Panproto.Rust.Expr@.
--
-- == Wire format
--
-- Rust derives @serde::Serialize@/@Deserialize@ on the AST enums and
-- transports them as CBOR via @ciborium@. serde's default (externally
-- tagged) enum representation is reproduced exactly by the encoders and
-- decoders here:
--
--   * A unit variant (e.g. @BuiltinOp::Add@, @Literal::Null@,
--     @Pattern::Wildcard@) is a bare CBOR string: @"Add"@.
--   * A newtype variant carrying one field (e.g. @Expr::Var@,
--     @Literal::Int@) is a one-key map: @{ "Var": <value> }@.
--   * A tuple variant carrying several fields (e.g. @Expr::Lam@,
--     @Expr::Builtin@) is a one-key map to an array:
--     @{ "Lam": [param, body] }@.
--   * A struct variant (e.g. @Expr::Match@, @Expr::Let@,
--     @Literal::Closure@) is a one-key map to a @snake_case@ map:
--     @{ "Match": { "scrutinee": …, "arms": … } }@.
--
-- A @Vec<(Arc<str>, T)>@ (record fields, closure environments, match
-- arms) is a CBOR array of two-element arrays. Float comparison follows
-- the Rust side's bit-level 'Eq'\/'Ord' (so two NaNs compare equal).
module Panproto.Expr
    ( -- * Expression AST
      Expr (..)
    , Pattern (..)
    , Literal (..)
    , BuiltinOp (..)
    , builtinName
    , builtinArity

      -- * Pretty printing
    , prettyPrintExpr

      -- * CBOR codecs
    , encodeExpr
    , decodeExpr
    , encodeLiteral
    , decodeLiteral
    , encodePattern
    , decodePattern

      -- * Queries
    , InstanceQuery (..)
    , emptyQuery
    , encodeInstanceQuery
    , decodeInstanceQuery
    , encodeEnvBindings
    , decodeEnvBindings

      -- * Capability class
    , ExprBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Bits (complement, shiftL, shiftR, testBit, (.&.), (.|.))
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Int (Int64)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word16, Word32, Word64, Word8)
import GHC.Float (castDoubleToWord64, castFloatToWord32, castWord32ToFloat, double2Float, float2Double)
import GHC.Generics (Generic)
import Panproto.Class (SchemaBackend (..))
import Panproto.Instance (InstanceBackend (..))

-- ---------------------------------------------------------------------------
-- Literal

-- | Haskell mirror of @panproto_expr::Literal@: the value type of the
-- expression language and the leaf of literal expressions.
--
-- 'Closure' is a first-class value produced by evaluating a 'Lam': it
-- captures the parameter name, the body expression, and the environment
-- bindings live at the point of creation.
data Literal
    = -- | Boolean value.
      LBool !Bool
    | -- | 64-bit signed integer.
      LInt !Int64
    | -- | 64-bit IEEE 754 float. Equality is bit-level (so NaN == NaN),
      -- matching the Rust @PartialEq@\/@Hash@ that uses @f64::to_bits@.
      LFloat !Double
    | -- | UTF-8 string.
      LStr !Text
    | -- | Raw bytes.
      LBytes !BS.ByteString
    | -- | Null \/ absent value.
      LNull
    | -- | A record: an ordered association list of field names to values.
      LRecord ![(Text, Literal)]
    | -- | A list of values.
      LList ![Literal]
    | -- | A closure: a captured lambda with its environment. The fields
      -- are the bound parameter name, the body expression, and the
      -- bindings captured at closure-creation time.
      LClosure !Text !Expr ![(Text, Literal)]
    deriving stock (Show, Generic)
    deriving anyclass (NFData)

-- | Structural equality with bit-level float comparison, matching the
-- Rust @impl PartialEq for Literal@ (which compares floats via
-- @f64::to_bits@ so that the relation agrees with @Eq@ and @Hash@).
instance Eq Literal where
    LBool a == LBool b = a == b
    LInt a == LInt b = a == b
    LFloat a == LFloat b = doubleToWord64 a == doubleToWord64 b
    LStr a == LStr b = a == b
    LBytes a == LBytes b = a == b
    LNull == LNull = True
    LRecord a == LRecord b = a == b
    LList a == LList b = a == b
    LClosure p1 b1 e1 == LClosure p2 b2 e2 = p1 == p2 && b1 == b2 && e1 == e2
    _ == _ = False

-- ---------------------------------------------------------------------------
-- Expr

-- | Haskell mirror of @panproto_expr::Expr@: an expression in the pure
-- functional language. Every variant is serializable, content
-- addressable, and evaluates deterministically.
data Expr
    = -- | Variable reference: @x@.
      Var !Text
    | -- | Lambda abstraction: @\\param -> body@.
      Lam !Text !Expr
    | -- | Function application: @func arg@.
      App !Expr !Expr
    | -- | Literal value.
      Lit !Literal
    | -- | Record construction: @{ name = expr, … }@.
      Record ![(Text, Expr)]
    | -- | List construction: @[expr, …]@.
      List ![Expr]
    | -- | Field access: @expr.field@.
      Field !Expr !Text
    | -- | Index access: @expr[index]@.
      Index !Expr !Expr
    | -- | Pattern matching: @case scrutinee of pat -> body; …@. The
      -- fields are the scrutinee and the @(pattern, body)@ arms (tried in
      -- order).
      Match !Expr ![(Pattern, Expr)]
    | -- | Let binding: @let name = value in body@. The fields are the
      -- bound name, the value to bind, and the body where the binding is
      -- visible.
      Let !Text !Expr !Expr
    | -- | Built-in operation applied to arguments.
      Builtin !BuiltinOp ![Expr]
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- Pattern

-- | Haskell mirror of @panproto_expr::Pattern@: a destructuring pattern
-- for 'Match' arms.
data Pattern
    = -- | Matches anything, binds nothing: @_@.
      PWildcard
    | -- | Matches anything, binds the value to a name.
      PVar !Text
    | -- | Matches a specific literal value.
      PLit !Literal
    | -- | Matches a record with per-field patterns.
      PRecord ![(Text, Pattern)]
    | -- | Matches a list with per-element patterns.
      PList ![Pattern]
    | -- | Matches a tagged constructor with argument patterns.
      PConstructor !Text ![Pattern]
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- BuiltinOp

-- | Haskell mirror of @panproto_expr::BuiltinOp@: the built-in operations,
-- grouped by domain. Each has a fixed arity (see 'builtinArity'). All
-- operations are pure and deterministic.
--
-- The graph-traversal builtins ('OpEdge', 'OpChildren', 'OpHasEdge',
-- 'OpEdgeCount', 'OpAnchor') require an instance context and only do
-- useful work under instance-aware evaluation; the standard evaluator
-- returns 'LNull' for them.
data BuiltinOp
    = -- Arithmetic (7)
      OpAdd
    | OpSub
    | OpMul
    | OpDiv
    | OpMod
    | OpNeg
    | OpAbs
    | -- Rounding (3)
      OpFloor
    | OpCeil
    | OpRound
    | -- Comparison (6)
      OpEq
    | OpNeq
    | OpLt
    | OpLte
    | OpGt
    | OpGte
    | -- Boolean (3)
      OpAnd
    | OpOr
    | OpNot
    | -- String (10)
      OpConcat
    | OpLen
    | OpSlice
    | OpUpper
    | OpLower
    | OpTrim
    | OpSplit
    | OpJoin
    | OpReplace
    | OpContains
    | -- List (9)
      OpMap
    | OpFilter
    | OpFold
    | OpAppend
    | OpHead
    | OpTail
    | OpReverse
    | OpFlatMap
    | OpLength
    | -- Record (4)
      OpMergeRecords
    | OpKeys
    | OpValues
    | OpHasField
    | -- Utility (3)
      OpDefaultVal
    | OpClamp
    | OpTruncateStr
    | -- Type coercions (6)
      OpIntToFloat
    | OpFloatToInt
    | OpIntToStr
    | OpFloatToStr
    | OpStrToInt
    | OpStrToFloat
    | -- Type inspection (3)
      OpTypeOf
    | OpIsNull
    | OpIsList
    | -- Graph traversal (5)
      OpEdge
    | OpChildren
    | OpHasEdge
    | OpEdgeCount
    | OpAnchor
    deriving stock (Eq, Show, Bounded, Enum, Generic)
    deriving anyclass (NFData)

-- | The serde variant tag for a builtin: its Rust enum variant name.
-- This is the bare string used on the wire and the key in
-- 'builtinFromTag'.
builtinTag :: BuiltinOp -> Text
builtinTag = \case
    OpAdd -> "Add"
    OpSub -> "Sub"
    OpMul -> "Mul"
    OpDiv -> "Div"
    OpMod -> "Mod"
    OpNeg -> "Neg"
    OpAbs -> "Abs"
    OpFloor -> "Floor"
    OpCeil -> "Ceil"
    OpRound -> "Round"
    OpEq -> "Eq"
    OpNeq -> "Neq"
    OpLt -> "Lt"
    OpLte -> "Lte"
    OpGt -> "Gt"
    OpGte -> "Gte"
    OpAnd -> "And"
    OpOr -> "Or"
    OpNot -> "Not"
    OpConcat -> "Concat"
    OpLen -> "Len"
    OpSlice -> "Slice"
    OpUpper -> "Upper"
    OpLower -> "Lower"
    OpTrim -> "Trim"
    OpSplit -> "Split"
    OpJoin -> "Join"
    OpReplace -> "Replace"
    OpContains -> "Contains"
    OpMap -> "Map"
    OpFilter -> "Filter"
    OpFold -> "Fold"
    OpAppend -> "Append"
    OpHead -> "Head"
    OpTail -> "Tail"
    OpReverse -> "Reverse"
    OpFlatMap -> "FlatMap"
    OpLength -> "Length"
    OpMergeRecords -> "MergeRecords"
    OpKeys -> "Keys"
    OpValues -> "Values"
    OpHasField -> "HasField"
    OpDefaultVal -> "DefaultVal"
    OpClamp -> "Clamp"
    OpTruncateStr -> "TruncateStr"
    OpIntToFloat -> "IntToFloat"
    OpFloatToInt -> "FloatToInt"
    OpIntToStr -> "IntToStr"
    OpFloatToStr -> "FloatToStr"
    OpStrToInt -> "StrToInt"
    OpStrToFloat -> "StrToFloat"
    OpTypeOf -> "TypeOf"
    OpIsNull -> "IsNull"
    OpIsList -> "IsList"
    OpEdge -> "Edge"
    OpChildren -> "Children"
    OpHasEdge -> "HasEdge"
    OpEdgeCount -> "EdgeCount"
    OpAnchor -> "Anchor"

-- | Resolve a serde variant tag back to a 'BuiltinOp'. Returns 'Nothing'
-- for an unrecognized tag (a builtin a newer Rust release added).
builtinFromTag :: Text -> Maybe BuiltinOp
builtinFromTag t = lookup t table
  where
    table = [(builtinTag op, op) | op <- [minBound .. maxBound]]

-- | The canonical function name for a builtin in surface syntax, as used
-- by 'prettyPrintExpr'\'s call-syntax fallback. Mirrors @builtin_name@
-- in @crates\/panproto-expr-parser\/src\/pretty.rs@.
builtinName :: BuiltinOp -> Text
builtinName = \case
    OpAdd -> "add"
    OpSub -> "sub"
    OpMul -> "mul"
    OpDiv -> "div"
    OpMod -> "mod"
    OpNeg -> "neg"
    OpAbs -> "abs"
    OpFloor -> "floor"
    OpCeil -> "ceil"
    OpRound -> "round"
    OpEq -> "eq"
    OpNeq -> "neq"
    OpLt -> "lt"
    OpLte -> "lte"
    OpGt -> "gt"
    OpGte -> "gte"
    OpAnd -> "and"
    OpOr -> "or"
    OpNot -> "not"
    OpConcat -> "concat"
    OpLen -> "len"
    OpSlice -> "slice"
    OpUpper -> "upper"
    OpLower -> "lower"
    OpTrim -> "trim"
    OpSplit -> "split"
    OpJoin -> "join"
    OpReplace -> "replace"
    OpContains -> "contains"
    OpMap -> "map"
    OpFilter -> "filter"
    OpFold -> "fold"
    OpAppend -> "append"
    OpHead -> "head"
    OpTail -> "tail"
    OpReverse -> "reverse"
    OpFlatMap -> "flat_map"
    OpLength -> "length"
    OpMergeRecords -> "merge"
    OpKeys -> "keys"
    OpValues -> "values"
    OpHasField -> "has_field"
    OpDefaultVal -> "default"
    OpClamp -> "clamp"
    OpTruncateStr -> "truncate_str"
    OpIntToFloat -> "int_to_float"
    OpFloatToInt -> "float_to_int"
    OpIntToStr -> "int_to_str"
    OpFloatToStr -> "float_to_str"
    OpStrToInt -> "str_to_int"
    OpStrToFloat -> "str_to_float"
    OpTypeOf -> "type_of"
    OpIsNull -> "is_null"
    OpIsList -> "is_list"
    OpEdge -> "edge"
    OpChildren -> "children"
    OpHasEdge -> "has_edge"
    OpEdgeCount -> "edge_count"
    OpAnchor -> "anchor"

-- | The expected number of arguments for a builtin. Mirrors
-- @BuiltinOp::arity@ in @crates\/panproto-expr\/src\/expr.rs@.
builtinArity :: BuiltinOp -> Int
builtinArity = \case
    -- Unary.
    OpNeg -> 1
    OpAbs -> 1
    OpFloor -> 1
    OpCeil -> 1
    OpRound -> 1
    OpNot -> 1
    OpUpper -> 1
    OpLower -> 1
    OpTrim -> 1
    OpHead -> 1
    OpTail -> 1
    OpReverse -> 1
    OpKeys -> 1
    OpValues -> 1
    OpIntToFloat -> 1
    OpFloatToInt -> 1
    OpIntToStr -> 1
    OpFloatToStr -> 1
    OpStrToInt -> 1
    OpStrToFloat -> 1
    OpTypeOf -> 1
    OpIsNull -> 1
    OpIsList -> 1
    OpLen -> 1
    OpLength -> 1
    OpChildren -> 1
    OpEdgeCount -> 1
    OpAnchor -> 1
    -- Binary.
    OpAdd -> 2
    OpSub -> 2
    OpMul -> 2
    OpDiv -> 2
    OpMod -> 2
    OpEq -> 2
    OpNeq -> 2
    OpLt -> 2
    OpLte -> 2
    OpGt -> 2
    OpGte -> 2
    OpAnd -> 2
    OpOr -> 2
    OpConcat -> 2
    OpSplit -> 2
    OpJoin -> 2
    OpAppend -> 2
    OpMap -> 2
    OpFilter -> 2
    OpHasField -> 2
    OpMergeRecords -> 2
    OpContains -> 2
    OpFlatMap -> 2
    OpEdge -> 2
    OpHasEdge -> 2
    OpDefaultVal -> 2
    OpTruncateStr -> 2
    -- Ternary.
    OpSlice -> 3
    OpReplace -> 3
    OpFold -> 3
    OpClamp -> 3

-- ---------------------------------------------------------------------------
-- Encoding: Literal

-- | Encode a 'Literal' to CBOR bytes compatible with the Rust side's
-- @ciborium@ serialization of @Literal@.
encodeLiteral :: Literal -> LBS.ByteString
encodeLiteral = CBOR.toLazyByteString . encLiteral

encLiteral :: Literal -> Encoding
encLiteral = \case
    LBool b -> variant1 "Bool" (Enc.encodeBool b)
    LInt n -> variant1 "Int" (Enc.encodeInt64 n)
    LFloat f -> variant1 "Float" (encMinimalFloat f)
    LStr s -> variant1 "Str" (Enc.encodeString s)
    LBytes bs -> variant1 "Bytes" (encByteArray bs)
    LNull -> Enc.encodeString "Null"
    LRecord fields -> variant1 "Record" (encNamedPairs encLiteral fields)
    LList items -> variant1 "List" (encList encLiteral items)
    LClosure param bdy env ->
        variant1 "Closure" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "param"
                <> Enc.encodeString param
                <> Enc.encodeString "body"
                <> encExpr bdy
                <> Enc.encodeString "env"
                <> encNamedPairs encLiteral env

-- | The Rust @Vec<u8>@ for @Literal::Bytes@ is serialized by @ciborium@
-- as a CBOR array of integers (serde's default for byte vectors), not as
-- a CBOR byte string. Mirror that exactly.
encByteArray :: BS.ByteString -> Encoding
encByteArray bs =
    Enc.encodeListLen (fromIntegral (BS.length bs))
        <> mconcat [Enc.encodeWord8 w | w <- BS.unpack bs]

-- | Encode a 'Double' using the same shortest-width float selection
-- @ciborium@ applies, so the bytes are identical to the Rust side.
--
-- ciborium (via @ciborium-ll@) emits a CBOR half-precision float when the
-- value round-trips through binary16, otherwise a single-precision float
-- when it round-trips through binary32, otherwise a double. cborg's
-- 'Enc.encodeFloat16' and 'Enc.encodeFloat' take a 'Float'; the
-- representability tests below decide which width preserves the value
-- exactly. NaN is emitted as the canonical half NaN (@0xf97e00@), again
-- matching ciborium.
encMinimalFloat :: Double -> Encoding
encMinimalFloat d
    | isNaN d = Enc.encodeFloat16 (0 / 0)
    | fitsHalf d = Enc.encodeFloat16 (double2Float d)
    | fitsFloat d = Enc.encodeFloat (double2Float d)
    | otherwise = Enc.encodeDouble d

-- | Does @d@ survive a binary32 round-trip exactly?
fitsFloat :: Double -> Bool
fitsFloat d = float2Double (double2Float d) == d

-- | Does @d@ survive a binary16 round-trip exactly? Requires that it
-- first fit binary32 (so 'double2Float' is lossless), then that the
-- binary16 encode/decode of that 'Float' reproduce it bit-for-bit.
fitsHalf :: Double -> Bool
fitsHalf d =
    fitsFloat d
        && let f = double2Float d
            in halfToFloat (floatToHalf f) == f

-- ---------------------------------------------------------------------------
-- Encoding: Expr

-- | Encode an 'Expr' to CBOR bytes compatible with the Rust side's
-- @ciborium@ serialization of @Expr@.
encodeExpr :: Expr -> LBS.ByteString
encodeExpr = CBOR.toLazyByteString . encExpr

encExpr :: Expr -> Encoding
encExpr = \case
    Var n -> variant1 "Var" (Enc.encodeString n)
    Lam param bdy -> variant1 "Lam" (encTuple2 (Enc.encodeString param) (encExpr bdy))
    App f x -> variant1 "App" (encTuple2 (encExpr f) (encExpr x))
    Lit lit -> variant1 "Lit" (encLiteral lit)
    Record fields -> variant1 "Record" (encNamedPairs encExpr fields)
    List items -> variant1 "List" (encList encExpr items)
    Field inner fld -> variant1 "Field" (encTuple2 (encExpr inner) (Enc.encodeString fld))
    Index inner idx -> variant1 "Index" (encTuple2 (encExpr inner) (encExpr idx))
    Match scrut as ->
        variant1 "Match" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "scrutinee"
                <> encExpr scrut
                <> Enc.encodeString "arms"
                <> encList encArm as
    Let nm v bdy ->
        variant1 "Let" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "name"
                <> Enc.encodeString nm
                <> Enc.encodeString "value"
                <> encExpr v
                <> Enc.encodeString "body"
                <> encExpr bdy
    Builtin op args -> variant1 "Builtin" (encTuple2 (encBuiltin op) (encList encExpr args))

encArm :: (Pattern, Expr) -> Encoding
encArm (pat, e) = encTuple2 (encPattern pat) (encExpr e)

-- ---------------------------------------------------------------------------
-- Encoding: Pattern

-- | Encode a 'Pattern' to CBOR bytes compatible with the Rust side's
-- @ciborium@ serialization of @Pattern@.
encodePattern :: Pattern -> LBS.ByteString
encodePattern = CBOR.toLazyByteString . encPattern

encPattern :: Pattern -> Encoding
encPattern = \case
    PWildcard -> Enc.encodeString "Wildcard"
    PVar n -> variant1 "Var" (Enc.encodeString n)
    PLit lit -> variant1 "Lit" (encLiteral lit)
    PRecord fields -> variant1 "Record" (encNamedPairs encPattern fields)
    PList pats -> variant1 "List" (encList encPattern pats)
    PConstructor ctor args ->
        variant1 "Constructor" (encTuple2 (Enc.encodeString ctor) (encList encPattern args))

-- ---------------------------------------------------------------------------
-- Encoding: BuiltinOp and helpers

encBuiltin :: BuiltinOp -> Encoding
encBuiltin = Enc.encodeString . builtinTag

-- | An externally-tagged enum variant carrying a single payload:
-- @{ "Tag": <payload> }@.
variant1 :: Text -> Encoding -> Encoding
variant1 tag payload = Enc.encodeMapLen 1 <> Enc.encodeString tag <> payload

-- | A serde tuple of two elements: a fixed-length CBOR array.
encTuple2 :: Encoding -> Encoding -> Encoding
encTuple2 a b = Enc.encodeListLen 2 <> a <> b

-- | A homogeneous CBOR array.
encList :: (a -> Encoding) -> [a] -> Encoding
encList enc xs = Enc.encodeListLen (fromIntegral (length xs)) <> mconcat (map enc xs)

-- | A @Vec<(Arc<str>, T)>@: an array of two-element @[name, value]@
-- arrays.
encNamedPairs :: (a -> Encoding) -> [(Text, a)] -> Encoding
encNamedPairs enc = encList (\(k, v) -> encTuple2 (Enc.encodeString k) (enc v))

-- ---------------------------------------------------------------------------
-- Decoding: Literal

-- | Decode CBOR bytes produced by the Rust side into a 'Literal'.
-- Tolerant of unknown struct fields in 'LClosure'.
decodeLiteral :: LBS.ByteString -> Either String Literal
decodeLiteral = runDecoder litDecoder

litDecoder :: Decoder s Literal
litDecoder = decodeExternallyTagged unitLit taggedLit
  where
    unitLit "Null" = Just LNull
    unitLit _ = Nothing

    taggedLit tag = case tag of
        "Bool" -> Just (LBool <$> Dec.decodeBool)
        "Int" -> Just (LInt <$> decodeInt64)
        "Float" -> Just (LFloat <$> Dec.decodeDouble)
        "Str" -> Just (LStr <$> Dec.decodeString)
        "Bytes" -> Just (LBytes <$> decodeByteArray)
        "Record" -> Just (LRecord <$> decodeNamedPairs litDecoder)
        "List" -> Just (LList <$> decodeArray litDecoder)
        "Closure" -> Just decodeClosure
        _ -> Nothing

decodeClosure :: Decoder s Literal
decodeClosure = do
    (param, bdy, env) <- decodeStructFields (T.empty, Lit LNull, []) step
    pure (LClosure param bdy env)
  where
    step (param, bdy, env) key = case key of
        "param" -> (\v -> (v, bdy, env)) <$> Dec.decodeString
        "body" -> (\v -> (param, v, env)) <$> exprDecoder
        "env" -> (\v -> (param, bdy, v)) <$> decodeNamedPairs litDecoder
        _ -> (param, bdy, env) <$ skipTerm

-- | The byte-array form for @Literal::Bytes@: a CBOR array of small
-- integers. Tolerates an indefinite-length array.
decodeByteArray :: Decoder s BS.ByteString
decodeByteArray = BS.pack <$> decodeArray decodeWord8Lenient

decodeWord8Lenient :: Decoder s Word8
decodeWord8Lenient = fromIntegral <$> Dec.decodeWord

-- ---------------------------------------------------------------------------
-- Decoding: Expr

-- | Decode CBOR bytes produced by the Rust side into an 'Expr'. Tolerant
-- of unknown struct fields in 'Match' and 'Let'.
decodeExpr :: LBS.ByteString -> Either String Expr
decodeExpr = runDecoder exprDecoder

exprDecoder :: Decoder s Expr
exprDecoder = decodeExternallyTagged (const Nothing) taggedExpr
  where
    taggedExpr tag = case tag of
        "Var" -> Just (Var <$> Dec.decodeString)
        "Lam" -> Just (decodeTuple2 Lam Dec.decodeString exprDecoder)
        "App" -> Just (decodeTuple2 App exprDecoder exprDecoder)
        "Lit" -> Just (Lit <$> litDecoder)
        "Record" -> Just (Record <$> decodeNamedPairs exprDecoder)
        "List" -> Just (List <$> decodeArray exprDecoder)
        "Field" -> Just (decodeTuple2 Field exprDecoder Dec.decodeString)
        "Index" -> Just (decodeTuple2 Index exprDecoder exprDecoder)
        "Match" -> Just decodeMatch
        "Let" -> Just decodeLet
        "Builtin" -> Just (decodeTuple2 Builtin builtinDecoder (decodeArray exprDecoder))
        _ -> Nothing

decodeMatch :: Decoder s Expr
decodeMatch = do
    (scrut, as) <- decodeStructFields (Lit LNull, []) step
    pure (Match scrut as)
  where
    step (scrut, as) key = case key of
        "scrutinee" -> (\v -> (v, as)) <$> exprDecoder
        "arms" -> (\v -> (scrut, v)) <$> decodeArray decodeArm
        _ -> (scrut, as) <$ skipTerm

decodeLet :: Decoder s Expr
decodeLet = do
    (nm, v, bdy) <- decodeStructFields (T.empty, Lit LNull, Lit LNull) step
    pure (Let nm v bdy)
  where
    step (nm, v, bdy) key = case key of
        "name" -> (\x -> (x, v, bdy)) <$> Dec.decodeString
        "value" -> (\x -> (nm, x, bdy)) <$> exprDecoder
        "body" -> (\x -> (nm, v, x)) <$> exprDecoder
        _ -> (nm, v, bdy) <$ skipTerm

decodeArm :: Decoder s (Pattern, Expr)
decodeArm = decodeTuple2 (,) patternDecoder exprDecoder

-- ---------------------------------------------------------------------------
-- Decoding: Pattern

-- | Decode CBOR bytes produced by the Rust side into a 'Pattern'.
decodePattern :: LBS.ByteString -> Either String Pattern
decodePattern = runDecoder patternDecoder

patternDecoder :: Decoder s Pattern
patternDecoder = decodeExternallyTagged unitPat taggedPat
  where
    unitPat "Wildcard" = Just PWildcard
    unitPat _ = Nothing

    taggedPat tag = case tag of
        "Var" -> Just (PVar <$> Dec.decodeString)
        "Lit" -> Just (PLit <$> litDecoder)
        "Record" -> Just (PRecord <$> decodeNamedPairs patternDecoder)
        "List" -> Just (PList <$> decodeArray patternDecoder)
        "Constructor" -> Just (decodeTuple2 PConstructor Dec.decodeString (decodeArray patternDecoder))
        _ -> Nothing

-- ---------------------------------------------------------------------------
-- Decoding: BuiltinOp

builtinDecoder :: Decoder s BuiltinOp
builtinDecoder = do
    tag <- Dec.decodeString
    case builtinFromTag tag of
        Just op -> pure op
        Nothing -> fail ("unknown BuiltinOp variant: " <> T.unpack tag)

-- ---------------------------------------------------------------------------
-- Decoding combinators

-- | Decode an externally-tagged serde enum.
--
-- The next CBOR token is either a bare string (a unit variant) or a
-- one-key map whose key is the variant tag (every other variant kind).
-- @unit@ resolves a bare-string tag; @tagged@ supplies the payload
-- decoder for a map-keyed tag.
decodeExternallyTagged
    :: (Text -> Maybe a)
    -- ^ Resolve a bare-string unit variant.
    -> (Text -> Maybe (Decoder s a))
    -- ^ Supply the payload decoder for a map-keyed variant.
    -> Decoder s a
decodeExternallyTagged unit tagged = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            tag <- Dec.decodeString
            case unit tag of
                Just v -> pure v
                Nothing -> fail ("unexpected unit-variant tag: " <> T.unpack tag)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            tag <- Dec.decodeString
            case tagged tag of
                Just dec -> dec
                Nothing -> fail ("unknown variant tag: " <> T.unpack tag)

-- | Decode a serde tuple of two elements: a fixed-length CBOR array.
decodeTuple2 :: (a -> b -> c) -> Decoder s a -> Decoder s b -> Decoder s c
decodeTuple2 f da db = do
    _ <- Dec.decodeListLenOrIndef
    a <- da
    b <- db
    pure (f a b)

-- | Decode a homogeneous CBOR array, handling both definite and
-- indefinite length.
decodeArray :: Decoder s a -> Decoder s [a]
decodeArray dec = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> replicateDecoder n dec
        Nothing -> readUntilBreak dec

-- | Decode a @Vec<(Arc<str>, T)>@: an array of two-element @[name, value]@
-- arrays.
decodeNamedPairs :: Decoder s a -> Decoder s [(Text, a)]
decodeNamedPairs dec = decodeArray (decodeTuple2 (,) Dec.decodeString dec)

-- | Decode the body of a serde struct variant (the inner map): repeatedly
-- read @(key, value)@ pairs, threading them through @step@. Unknown keys
-- are the responsibility of @step@ (which should 'skipTerm').
decodeStructFields :: acc -> (acc -> Text -> Decoder s acc) -> Decoder s acc
decodeStructFields initial step = do
    len <- Dec.decodeMapLenOrIndef
    case len of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = do
        key <- Dec.decodeString
        acc' <- step acc key
        goN (n - 1) acc'
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else do
                key <- Dec.decodeString
                acc' <- step acc key
                goIndef acc'

replicateDecoder :: Int -> Decoder s a -> Decoder s [a]
replicateDecoder 0 _ = pure []
replicateDecoder k d = do
    x <- d
    xs <- replicateDecoder (k - 1) d
    pure (x : xs)

readUntilBreak :: Decoder s a -> Decoder s [a]
readUntilBreak d = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure []
        else do
            x <- d
            rest <- readUntilBreak d
            pure (x : rest)

-- | Decode a signed 64-bit integer, accepting either CBOR major type
-- (unsigned or negative).
decodeInt64 :: Decoder s Int64
decodeInt64 = Dec.decodeInt64

-- | Run a top-level decoder over a complete buffer, rejecting trailing
-- bytes.
runDecoder :: (forall s. Decoder s a) -> LBS.ByteString -> Either String a
runDecoder dec bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left "trailing bytes after CBOR value"

-- ---------------------------------------------------------------------------
-- Skipping unknown CBOR terms (for tolerant struct decoding)

-- | Skip an arbitrary CBOR value via depth-first descent, so an unknown
-- struct field with a structured value does not desync the decoder.
-- Mirrors the skipper in "Panproto.Canonical".
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
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs

    skipBytesIndef = do
        _ <- Dec.decodeBytesIndef
        skipUntilBreakBytes

    skipStringIndef = do
        _ <- Dec.decodeStringIndef
        skipUntilBreakStrings

    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes

    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings

-- ---------------------------------------------------------------------------
-- InstanceQuery

-- | Haskell mirror of @panproto_inst::InstanceQuery@: a declarative query
-- over any instance shape.
--
-- The query is a composite of anchor selection, optional path navigation,
-- an optional predicate, optional grouping and projection, and an
-- optional result limit. The predicate is an 'Expr' evaluated per node
-- with the node's observable stalk bound as variables.
data InstanceQuery = InstanceQuery
    { anchor :: !Text
    -- ^ Select nodes with this anchor (vertex kind).
    , predicate :: !(Maybe Expr)
    -- ^ Optional predicate on node values \/ fields.
    , groupBy :: !(Maybe Text)
    -- ^ Optional: group results by this field name.
    , project :: !(Maybe [Text])
    -- ^ Optional: project to these fields only.
    , limit :: !(Maybe Int)
    -- ^ Optional: cap the number of results.
    , path :: ![Text]
    -- ^ Optional: traverse these edge kinds before selecting.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | A query selecting one anchor with no predicate, grouping,
-- projection, limit, or path. Matches @InstanceQuery::default()@ once an
-- anchor is supplied.
emptyQuery :: Text -> InstanceQuery
emptyQuery a =
    InstanceQuery
        { anchor = a
        , predicate = Nothing
        , groupBy = Nothing
        , project = Nothing
        , limit = Nothing
        , path = []
        }

-- | Encode an 'InstanceQuery' to CBOR compatible with the Rust side's
-- @ciborium@ serialization of @InstanceQuery@.
--
-- @anchor@ is always emitted (a @serde(transparent)@ string). The
-- optional fields follow the Rust @skip_serializing_if@ attributes: a
-- @Nothing@ option and an empty @path@ are omitted from the map, so the
-- map length is computed from the present fields.
encodeInstanceQuery :: InstanceQuery -> LBS.ByteString
encodeInstanceQuery q = CBOR.toLazyByteString (Enc.encodeMapLen count <> entries)
  where
    optional :: Bool -> Encoding -> Encoding
    optional present e = if present then e else mempty

    presentCount =
        1 -- anchor
            + boolToInt (hasPredicate q)
            + boolToInt (hasGroupBy q)
            + boolToInt (hasProject q)
            + boolToInt (hasLimit q)
            + boolToInt (not (null q.path))
    count = fromIntegral presentCount

    entries =
        (Enc.encodeString "anchor" <> Enc.encodeString q.anchor)
            <> optional (hasPredicate q) (field "predicate" (maybe mempty encExpr q.predicate))
            <> optional (hasGroupBy q) (field "group_by" (maybe mempty Enc.encodeString q.groupBy))
            <> optional (hasProject q) (field "project" (maybe mempty (encList Enc.encodeString) q.project))
            <> optional (hasLimit q) (field "limit" (maybe mempty (Enc.encodeInt) q.limit))
            <> optional (not (null q.path)) (field "path" (encList Enc.encodeString q.path))

    field k v = Enc.encodeString k <> v

    hasPredicate iq = maybe False (const True) iq.predicate
    hasGroupBy iq = maybe False (const True) iq.groupBy
    hasProject iq = maybe False (const True) iq.project
    hasLimit iq = maybe False (const True) iq.limit

    boolToInt b = if b then 1 else 0 :: Int

-- | Decode CBOR bytes into an 'InstanceQuery'. Tolerant of unknown
-- fields and missing optional fields (which fall back to the
-- 'emptyQuery' defaults).
decodeInstanceQuery :: LBS.ByteString -> Either String InstanceQuery
decodeInstanceQuery = runDecoder queryDecoder

queryDecoder :: Decoder s InstanceQuery
queryDecoder = decodeStructFields (emptyQuery T.empty) step
  where
    step acc key = case key of
        "anchor" -> (\v -> acc {anchor = v}) <$> Dec.decodeString
        "predicate" -> (\v -> acc {predicate = Just v}) <$> exprDecoder
        "group_by" -> (\v -> acc {groupBy = Just v}) <$> Dec.decodeString
        "project" -> (\v -> acc {project = Just v}) <$> decodeArray Dec.decodeString
        "limit" -> (\v -> acc {limit = Just v}) <$> Dec.decodeInt
        "path" -> (\v -> acc {path = v}) <$> decodeArray Dec.decodeString
        _ -> acc <$ skipTerm

-- ---------------------------------------------------------------------------
-- Eval environment bindings

-- | Encode the environment for 'evalFunc': a @Vec<(String, Literal)>@
-- matching the @pp_expr_eval_func@ @env@ argument. The wire form is a
-- CBOR array of two-element @[name, literal]@ arrays.
encodeEnvBindings :: [(Text, Literal)] -> LBS.ByteString
encodeEnvBindings = CBOR.toLazyByteString . encNamedPairs encLiteral

-- | Decode a @Vec<(String, Literal)>@ environment.
decodeEnvBindings :: LBS.ByteString -> Either String [(Text, Literal)]
decodeEnvBindings = runDecoder (decodeNamedPairs litDecoder)

-- ---------------------------------------------------------------------------
-- Pretty printing

-- | Precedence levels for 'prettyPrintExpr'. Higher binds tighter.
-- Mirrors @Prec@ in @crates\/panproto-expr-parser\/src\/pretty.rs@.
data Prec
    = PrecTop
    | PrecPipe
    | PrecOr
    | PrecAnd
    | PrecCmp
    | PrecConcat
    | PrecAddSub
    | PrecMulDiv
    | PrecUnary
    | PrecApp
    | PrecAtom
    deriving stock (Eq, Ord)

-- | Associativity of a binary operator.
data Assoc = AssocLeft | AssocRight
    deriving stock (Eq)

-- | Pretty print an expression to Haskell-style surface syntax with
-- minimal parentheses, reproducing @panproto_expr_parser::pretty_print@
-- exactly.
--
-- This is a pure function: the Rust pretty printer has no IO, no
-- environment, and no nondeterminism, so it can be reimplemented here
-- without an engine round-trip. The output is designed to round-trip
-- through the parser: @parse (tokenize (prettyPrintExpr e)) == e@ for
-- well-formed expressions.
prettyPrintExpr :: Expr -> Text
prettyPrintExpr e = renderChunks (writeExpr e PrecTop)

-- A tiny "string builder" over 'Text' kept as a difference-free
-- concatenation. We accumulate into a list of chunks and concatenate
-- once, which keeps the translation close to the Rust @String@ pushes
-- while staying O(n).
newtype Chunks = Chunks [Text]

instance Semigroup Chunks where
    Chunks a <> Chunks b = Chunks (a <> b)

instance Monoid Chunks where
    mempty = Chunks []

chunk :: Text -> Chunks
chunk t = Chunks [t]

renderChunks :: Chunks -> Text
renderChunks (Chunks ts) = T.concat ts

-- | Write an expression at the given precedence context, wrapping in
-- parentheses when its own precedence is lower than @ctx@.
writeExpr :: Expr -> Prec -> Chunks
writeExpr expr ctx = case expr of
    Var nm -> chunk nm
    Lit lit -> writeLiteral lit
    Lam param bdy ->
        parenWhen (ctx > PrecTop) (writeLambdaChain param bdy)
    App _ _ -> writeApp expr ctx
    Record fields -> writeRecordExpr fields
    List items ->
        chunk "[" <> commaSep [writeExpr i PrecTop | i <- items] <> chunk "]"
    Field inner fld ->
        writeExpr inner PrecAtom <> chunk "." <> chunk fld
    Index inner idx ->
        writeExpr inner PrecAtom <> chunk "[" <> writeExpr idx PrecTop <> chunk "]"
    Match scrut as -> writeMatch scrut as ctx
    Let nm v bdy -> writeLet nm v bdy ctx
    Builtin op args -> writeBuiltin op args ctx

-- | A chain of nested lambdas printed as @\\x y z -> body@.
writeLambdaChain :: Text -> Expr -> Chunks
writeLambdaChain firstParam firstBody =
    chunk "\\" <> chunk firstParam <> go firstBody
  where
    go (Lam param inner) = chunk " " <> chunk param <> go inner
    go bdy = chunk " -> " <> writeExpr bdy PrecTop

-- | Function application, collecting the curried spine: @f x y z@.
writeApp :: Expr -> Prec -> Chunks
writeApp expr ctx =
    parenWhen (ctx > PrecApp) $
        writeExpr headExpr PrecApp <> mconcat [chunk " " <> writeExpr a PrecAtom | a <- spine]
  where
    (headExpr, spine) = collectSpine expr []
    collectSpine (App f x) acc = collectSpine f (x : acc)
    collectSpine h acc = (h, acc)

-- | A record expression with field punning where the value is the
-- variable of the same name.
writeRecordExpr :: [(Text, Expr)] -> Chunks
writeRecordExpr fields =
    chunk "{ " <> commaSep (map fieldChunk fields) <> chunk " }"
  where
    fieldChunk (nm, Var v) | v == nm = chunk nm
    fieldChunk (nm, val) = chunk nm <> chunk " = " <> writeExpr val PrecTop

-- | A match expression, recognizing the @if/then/else@ shape (a
-- @True -> …@ arm followed by a @_ -> …@ arm).
writeMatch :: Expr -> [(Pattern, Expr)] -> Prec -> Chunks
writeMatch scrut as ctx =
    case as of
        [(PLit (LBool True), thenB), (PWildcard, elseB)] ->
            parenWhen (ctx > PrecTop) $
                chunk "if "
                    <> writeExpr scrut PrecTop
                    <> chunk " then "
                    <> writeExpr thenB PrecTop
                    <> chunk " else "
                    <> writeExpr elseB PrecTop
        _ ->
            parenWhen (ctx > PrecTop) $
                chunk "case "
                    <> writeExpr scrut PrecTop
                    <> chunk " of\n"
                    <> joinChunks (chunk "\n") (map writeArm as)
  where
    writeArm (pat, bdy) =
        chunk "  " <> writePattern pat <> chunk " -> " <> writeExpr bdy PrecTop

-- | A let binding, collapsing chained lets into a layout block.
writeLet :: Text -> Expr -> Expr -> Prec -> Chunks
writeLet nm v bdy ctx =
    parenWhen (ctx > PrecTop) $
        if length bindings == 1
            then
                chunk "let "
                    <> chunk nm
                    <> chunk " = "
                    <> writeExpr v PrecTop
                    <> chunk " in "
                    <> writeExpr finalBody PrecTop
            else
                chunk "let\n"
                    <> mconcat [chunk "  " <> chunk n <> chunk " = " <> writeExpr ev PrecTop <> chunk "\n" | (n, ev) <- bindings]
                    <> chunk "in "
                    <> writeExpr finalBody PrecTop
  where
    (bindings, finalBody) = collectLets [(nm, v)] bdy
    collectLets acc (Let n2 v2 b2) = collectLets (acc <> [(n2, v2)]) b2
    collectLets acc fb = (acc, fb)

-- | A builtin operation: infix where the parser supports it, otherwise
-- prefix or call syntax.
writeBuiltin :: BuiltinOp -> [Expr] -> Prec -> Chunks
writeBuiltin op args ctx
    | Just (sym, prec, assoc) <- infixInfo op
    , [l, r] <- args =
        let (leftCtx, rightCtx) = case assoc of
                AssocLeft -> (prec, nextPrec prec)
                AssocRight -> (nextPrec prec, prec)
         in parenWhen (ctx > prec) $
                writeExpr l leftCtx <> chunk " " <> chunk sym <> chunk " " <> writeExpr r rightCtx
    | op == OpEdge
    , [target, Lit (LStr edgeName)] <- args =
        parenWhen (ctx > PrecAtom) $
            writeExpr target PrecAtom <> chunk " -> " <> chunk edgeName
    | op == OpNeg
    , [a] <- args =
        parenWhen (ctx > PrecUnary) (chunk "-" <> writeExpr a PrecAtom)
    | op == OpNot
    , [a] <- args =
        parenWhen (ctx > PrecUnary) (chunk "not " <> writeExpr a PrecAtom)
    | otherwise =
        parenWhen (ctx > PrecApp && not (null args)) $
            chunk (builtinName op) <> mconcat [chunk " " <> writeExpr a PrecAtom | a <- args]

-- | Map a builtin to its infix symbol, precedence, and associativity, or
-- 'Nothing' for builtins that print as a call. Mirrors @infix_info@.
infixInfo :: BuiltinOp -> Maybe (Text, Prec, Assoc)
infixInfo = \case
    OpOr -> Just ("||", PrecOr, AssocLeft)
    OpAnd -> Just ("&&", PrecAnd, AssocLeft)
    OpEq -> Just ("==", PrecCmp, AssocRight)
    OpNeq -> Just ("/=", PrecCmp, AssocRight)
    OpLt -> Just ("<", PrecCmp, AssocRight)
    OpLte -> Just ("<=", PrecCmp, AssocRight)
    OpGt -> Just (">", PrecCmp, AssocRight)
    OpGte -> Just (">=", PrecCmp, AssocRight)
    OpConcat -> Just ("++", PrecConcat, AssocRight)
    OpAdd -> Just ("+", PrecAddSub, AssocLeft)
    OpSub -> Just ("-", PrecAddSub, AssocLeft)
    OpMul -> Just ("*", PrecMulDiv, AssocLeft)
    OpDiv -> Just ("/", PrecMulDiv, AssocLeft)
    OpMod -> Just ("%", PrecMulDiv, AssocLeft)
    _ -> Nothing

-- | The next higher precedence level. Mirrors @next_prec@.
nextPrec :: Prec -> Prec
nextPrec = \case
    PrecTop -> PrecPipe
    PrecPipe -> PrecOr
    PrecOr -> PrecAnd
    PrecAnd -> PrecCmp
    PrecCmp -> PrecConcat
    PrecConcat -> PrecAddSub
    PrecAddSub -> PrecMulDiv
    PrecMulDiv -> PrecUnary
    PrecUnary -> PrecApp
    PrecApp -> PrecAtom
    PrecAtom -> PrecAtom

-- | Write a literal value. Mirrors @write_literal@.
writeLiteral :: Literal -> Chunks
writeLiteral = \case
    LBool True -> chunk "True"
    LBool False -> chunk "False"
    LInt n -> chunk (T.pack (show n))
    LFloat f -> chunk (showFloatLit f)
    LStr s -> chunk "\"" <> chunk (escapeStr s) <> chunk "\""
    LBytes bytes ->
        chunk "[" <> commaSep [chunk (T.pack (show w)) | w <- BS.unpack bytes] <> chunk "]"
    LNull -> chunk "Nothing"
    LRecord fields ->
        chunk "{ "
            <> commaSep [chunk nm <> chunk " = " <> writeLiteral val | (nm, val) <- fields]
            <> chunk " }"
    LList items -> chunk "[" <> commaSep (map writeLiteral items) <> chunk "]"
    LClosure param bdy _ ->
        chunk "\\" <> chunk param <> chunk " -> " <> writeExpr bdy PrecTop

-- | Format a float so the parser reads it back as a float: ensure a
-- decimal point is present. Mirrors @write_literal@'s float branch.
showFloatLit :: Double -> Text
showFloatLit f =
    let s = T.pack (show f)
     in if T.any (== '.') s then s else s <> ".0"

-- | Escape a string body for double-quoted surface syntax. Mirrors the
-- escape set in @write_literal@.
escapeStr :: Text -> Text
escapeStr = T.concatMap esc
  where
    esc '\\' = "\\\\"
    esc '"' = "\\\""
    esc '\n' = "\\n"
    esc '\r' = "\\r"
    esc '\t' = "\\t"
    esc c = T.singleton c

-- | Write a pattern. Mirrors @write_pattern@.
writePattern :: Pattern -> Chunks
writePattern = \case
    PWildcard -> chunk "_"
    PVar nm -> chunk nm
    PLit lit -> writeLiteral lit
    PRecord fields ->
        chunk "{ " <> commaSep (map fieldChunk fields) <> chunk " }"
    PList pats -> chunk "[" <> commaSep (map writePattern pats) <> chunk "]"
    PConstructor ctor args ->
        chunk ctor <> mconcat [chunk " " <> argChunk a | a <- args]
  where
    fieldChunk (nm, PVar v) | v == nm = chunk nm
    fieldChunk (nm, p) = chunk nm <> chunk " = " <> writePattern p
    argChunk a =
        let needsParens = case a of
                PConstructor _ inner -> not (null inner)
                _ -> False
         in if needsParens then chunk "(" <> writePattern a <> chunk ")" else writePattern a

-- | Wrap chunks in parentheses when the condition holds.
parenWhen :: Bool -> Chunks -> Chunks
parenWhen True c = chunk "(" <> c <> chunk ")"
parenWhen False c = c

-- | Join chunks with @", "@ between them.
commaSep :: [Chunks] -> Chunks
commaSep = joinChunks (chunk ", ")

-- | Join chunks with a separator between them.
joinChunks :: Chunks -> [Chunks] -> Chunks
joinChunks _ [] = mempty
joinChunks _ [x] = x
joinChunks sep (x : xs) = x <> sep <> joinChunks sep xs

-- ---------------------------------------------------------------------------
-- Float bit reinterpretation (for bit-level Literal equality)

-- | Reinterpret a 'Double' as its IEEE 754 bit pattern, with all NaNs
-- collapsed to one canonical bit pattern.
--
-- This makes 'Literal' equality agree with the Rust @impl PartialEq for
-- Literal@, which compares floats via @f64::to_bits@ so that the relation
-- is consistent with @Eq@\/@Hash@ (in particular two NaNs compare equal).
-- 'castDoubleToWord64' is GHC's primitive bit-level reinterpretation, the
-- exact analogue of Rust's @f64::to_bits@.
doubleToWord64 :: Double -> Word64
doubleToWord64 d
    | isNaN d = 0x7ff8000000000000 -- canonical quiet NaN
    | otherwise = castDoubleToWord64 d

-- | Convert a binary32 'Float' to the bit pattern of an IEEE-754
-- binary16 (half) using round-to-nearest, ties-to-even. Used only to
-- decide whether 'encMinimalFloat' may emit a half-precision value.
floatToHalf :: Float -> Word16
floatToHalf f =
    let bits = castFloatToWord32 f
        sign = fromIntegral ((bits `shiftR` 16) .&. 0x8000) :: Word16
        expo = fromIntegral ((bits `shiftR` 23) .&. 0xff) :: Int
        mant = bits .&. 0x7fffff :: Word32
     in if expo == 0xff
            then -- Inf / NaN: keep NaN payload non-zero so it stays a NaN.
                sign .|. 0x7c00 .|. (if mant /= 0 then 0x0200 else 0)
            else
                let unbiased = expo - 127 + 15
                 in if unbiased >= 0x1f
                        then sign .|. 0x7c00 -- overflow to half infinity
                        else
                            if unbiased <= 0
                                then -- Subnormal or zero in half.
                                    if unbiased < -10
                                        then sign -- too small: signed zero
                                        else
                                            let m = mant .|. 0x800000 -- restore implicit 1
                                                shiftAmt = 14 - unbiased -- (1 - unbiased) + 13
                                                halfMant = roundShift m shiftAmt
                                             in sign .|. fromIntegral halfMant
                                else
                                    let halfMant = roundShift mant 13
                                     in -- Rounding may carry into the exponent; adding
                                        -- the (possibly incremented) mantissa to the
                                        -- exponent field absorbs that carry correctly.
                                        sign
                                            .|. (fromIntegral unbiased `shiftL` 10)
                                            + fromIntegral halfMant

-- | Shift @m@ right by @n@ bits with round-to-nearest, ties-to-even.
roundShift :: Word32 -> Int -> Word32
roundShift m n
    | n <= 0 = m `shiftL` negate n
    | otherwise =
        let truncated = m `shiftR` n
            roundBitSet = testBit m (n - 1)
            stickySet = (m .&. (bit' (n - 1) - 1)) /= 0
            lsbSet = testBit truncated 0
         in if roundBitSet && (stickySet || lsbSet)
                then truncated + 1
                else truncated
  where
    bit' k = 1 `shiftL` k :: Word32

-- | Convert an IEEE-754 binary16 (half) bit pattern back to a binary32
-- 'Float'. Exact (binary16 ⊂ binary32), so this is the inverse used by
-- 'fitsHalf' to test for an exact half round-trip.
halfToFloat :: Word16 -> Float
halfToFloat h =
    let sign = (fromIntegral h .&. 0x8000) `shiftL` 16 :: Word32
        expo = (fromIntegral h `shiftR` 10) .&. 0x1f :: Word32
        mant = fromIntegral h .&. 0x3ff :: Word32
     in castWord32ToFloat $ case () of
            _
                | expo == 0 && mant == 0 -> sign -- signed zero
                | expo == 0 ->
                    -- Subnormal half: normalize into a binary32 normal.
                    let (e, m) = normSub mant 0
                        f32exp = 127 - 15 + 1 - e :: Word32
                     in sign .|. (f32exp `shiftL` 23) .|. ((m .&. 0x3ff) `shiftL` 13)
                | expo == 0x1f ->
                    -- Inf / NaN.
                    sign .|. 0x7f800000 .|. (mant `shiftL` 13)
                | otherwise ->
                    let f32exp = expo - 15 + 127
                     in sign .|. (f32exp `shiftL` 23) .|. (mant `shiftL` 13)
  where
    -- Left-shift the subnormal mantissa until its leading 1 reaches bit
    -- 10, counting the shifts so the caller can debias the exponent.
    normSub :: Word32 -> Word32 -> (Word32, Word32)
    normSub m shifts
        | m .&. 0x400 /= 0 = (shifts, m .&. complement 0x400)
        | otherwise = normSub (m `shiftL` 1) (shifts + 1)

-- ---------------------------------------------------------------------------
-- ExprBackend capability class

-- | Operations of the expression language: parsing surface syntax,
-- evaluating a function expression against an environment, and executing
-- a declarative query over an instance.
--
-- These mirror the @expr@ section of the panproto-c C ABI contract:
--
--   * 'parseExpr' wraps @pp_expr_parse@ (UTF-8 source in, CBOR 'Expr'
--     out; the engine runs the @panproto_expr_parser@ Pratt parser,
--     which is not reimplemented purely on the Haskell side).
--   * 'evalFunc' wraps @pp_expr_eval_func@ (CBOR 'Expr' plus a
--     @Vec<(String, Literal)>@ environment in, CBOR 'Literal' out;
--     calls @panproto_expr::eval@).
--   * 'executeQuery' wraps @pp_query_execute@ (CBOR 'InstanceQuery' plus
--     a @WInstance@ in, a CBOR match list out; calls
--     @inst::execute_query@).
--
-- No backend instance lives in this module: the value layer is pure and
-- backend-agnostic. The Rust instance is supplied separately in
-- @Panproto.Rust.Expr@. 'parseExpr' and 'evalFunc' return plain 'IO'
-- because the engine they call is foreign; 'prettyPrintExpr' stays a pure
-- function above because the Rust pretty printer is itself pure.
--
-- 'executeQuery' needs the backend's instance representation in addition
-- to its schema representation, so 'SchemaBackend' is a superclass and
-- the instance representation is an associated data family on this class.
-- ('InstanceRep' could later move onto a dedicated @InstanceBackend@
-- class added as a second superclass; the method signature is written so
-- that move is mechanical.)
class (SchemaBackend back, InstanceBackend back) => ExprBackend back where
    -- | Parse surface syntax into an 'Expr' AST using the engine's
    -- parser. Throws the backend's error type on a tokenize or parse
    -- failure. The 'Proxy' fixes the backend whose engine runs the parse.
    parseExpr :: Proxy back -> Text -> IO Expr

    -- | Evaluate a (closed-over-the-environment) function expression
    -- against a list of @(name, value)@ bindings, returning the resulting
    -- 'Literal'. Mirrors @pp_expr_eval_func@. The 'Proxy' fixes the
    -- backend whose engine runs the evaluation.
    evalFunc :: Proxy back -> Expr -> [(Text, Literal)] -> IO Literal

    -- | Execute a declarative query against an instance in the context of
    -- a schema, returning the matching nodes. Mirrors @pp_query_execute@:
    -- the result is the engine's match list (node id, anchor, optional
    -- value, and projected fields per match), surfaced here as a list of
    -- 'QueryMatchRep' so the backend controls the field representation.
    executeQuery
        :: InstanceQuery
        -> InstanceRep back
        -> SchemaRep back
        -> IO [QueryMatchRep back]

    -- | The backend-specific representation of a single query match (the
    -- @QueryMatch@ rows returned by @pp_query_execute@).
    data QueryMatchRep back :: Type
