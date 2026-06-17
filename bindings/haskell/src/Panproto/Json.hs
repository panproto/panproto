{-# LANGUAGE DerivingStrategies #-}

-- | A small bridge between CBOR (the panproto-c cold-path wire format)
-- and 'Data.Aeson.Value' (the shape Haskell consumers expect from
-- @to_dict@ / @to_json@ / @from_json@ surfaces).
--
-- The panproto Python binding exposes @to_dict@, @to_json@, and
-- @from_json@ on most value types; the Haskell binding reaches the
-- same surface by routing the CBOR a function returns through
-- 'cborToValue', and routing a caller-supplied 'Value' back through
-- 'valueToCbor'. The mapping is total in both directions for the
-- subset of CBOR @ciborium@ emits: a CBOR map becomes a JSON object
-- (keys are coerced to text), a CBOR array becomes a JSON array,
-- and scalars map to the obvious JSON scalars.
--
-- Object keys: JSON object keys are always text. A CBOR map with a
-- non-string key (a tuple key, say, from a @map_as_vec@-serialized
-- field) is therefore not directly representable as a JSON object;
-- such maps round-trip through the array form, which is what the Rust
-- side serializes them as. 'cborToValue' only sees string-keyed maps
-- in practice because the @serde_helpers@ already lower tuple-keyed
-- maps to arrays before they reach the boundary.
module Panproto.Json
    ( -- * CBOR / JSON bridge
      cborToValue
    , valueToCbor
    , decodeCborValue
    , encodeJsonValue

      -- * Streaming codecs
    , encodeValue
    , valueDecoder

      -- * Aeson re-exports
    , Value (..)
    , encode
    , eitherDecode
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Data.Aeson (Value (..), eitherDecode, encode)
import Data.Aeson.Key qualified as Key
import Data.Aeson.KeyMap qualified as KM
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Scientific (fromFloatDigits, toRealFloat)
import Data.Scientific qualified as Sci
import Data.Text qualified as T
import Data.Vector qualified as V

-- | Encode an aeson 'Value' to CBOR bytes the Rust side can decode
-- with @ciborium@.
valueToCbor :: Value -> LBS.ByteString
valueToCbor = CBOR.toLazyByteString . encodeValue

-- | Decode CBOR bytes produced by the panproto-c boundary into an
-- aeson 'Value'. Fails with a 'Left' on malformed input or trailing
-- bytes.
cborToValue :: LBS.ByteString -> Either String Value
cborToValue bs =
    case CBOR.deserialiseFromBytes valueDecoder bs of
        Left err -> Left (show err)
        Right (rest, v)
            | LBS.null rest -> Right v
            | otherwise -> Left "trailing bytes after CBOR-encoded value"

-- | Alias for 'cborToValue', spelled to match the @decode@/@encode@
-- pairing of the other modules.
decodeCborValue :: LBS.ByteString -> Either String Value
decodeCborValue = cborToValue

-- | Alias for 'valueToCbor'.
encodeJsonValue :: Value -> LBS.ByteString
encodeJsonValue = valueToCbor

-- ---------------------------------------------------------------------------
-- Encoding

encodeValue :: Value -> Encoding
encodeValue = \case
    Null -> Enc.encodeNull
    Bool b -> Enc.encodeBool b
    String t -> Enc.encodeString t
    Number n -> encodeScientific n
    Array xs ->
        Enc.encodeListLen (fromIntegral (V.length xs))
            <> foldMap encodeValue xs
    Object o ->
        Enc.encodeMapLen (fromIntegral (KM.size o))
            <> KM.foldMapWithKey
                (\k v -> Enc.encodeString (Key.toText k) <> encodeValue v)
                o

-- | Encode a 'Sci.Scientific' as a CBOR integer when it is integral
-- and fits, otherwise as a double. @ciborium@ accepts both for a
-- @serde_json::Value::Number@.
encodeScientific :: Sci.Scientific -> Encoding
encodeScientific n =
    case Sci.toBoundedInteger n :: Maybe Int of
        Just i -> Enc.encodeInt i
        Nothing -> Enc.encodeDouble (toRealFloat n)

-- ---------------------------------------------------------------------------
-- Decoding

valueDecoder :: Decoder s Value
valueDecoder = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> intValue
        Dec.TypeUInt64 -> intValue
        Dec.TypeNInt -> intValue
        Dec.TypeNInt64 -> intValue
        Dec.TypeInteger -> intValue
        Dec.TypeFloat16 -> doubleValue
        Dec.TypeFloat32 -> doubleValue
        Dec.TypeFloat64 -> doubleValue
        Dec.TypeBool -> Bool <$> Dec.decodeBool
        Dec.TypeNull -> Null <$ Dec.decodeNull
        Dec.TypeString -> String <$> Dec.decodeString
        Dec.TypeStringIndef -> String . T.concat <$> decodeStringChunks
        Dec.TypeBytes -> String . decodeBytesAsHex <$> Dec.decodeBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= decodeArrayN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= decodeArrayN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> decodeArrayIndef
        Dec.TypeMapLen -> Dec.decodeMapLen >>= decodeObjectN
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= decodeObjectN
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> decodeObjectIndef
        Dec.TypeTag -> Dec.decodeTag >> valueDecoder
        Dec.TypeTag64 -> Dec.decodeTag64 >> valueDecoder
        _ -> fail "cborToValue: unsupported CBOR token type"
  where
    intValue = Number . fromInteger <$> Dec.decodeInteger
    doubleValue = Number . fromFloatDigits <$> Dec.decodeDouble

decodeStringChunks :: Decoder s [T.Text]
decodeStringChunks = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure []
        else (:) <$> Dec.decodeString <*> decodeStringChunks

-- | A CBOR byte string has no JSON-native form; render it as a
-- lowercase hex 'String' so it survives the bridge without loss of
-- the original bytes' content.
decodeBytesAsHex :: BS.ByteString -> T.Text
decodeBytesAsHex = T.pack . concatMap byteHex . BS.unpack
  where
    byteHex w = [hexDigit (w `div` 16), hexDigit (w `mod` 16)]
    hexDigit d
        | d < 10 = toEnum (fromEnum '0' + fromIntegral d)
        | otherwise = toEnum (fromEnum 'a' + fromIntegral d - 10)

decodeArrayN :: Int -> Decoder s Value
decodeArrayN n = Array . V.fromList <$> go n
  where
    go 0 = pure []
    go k = (:) <$> valueDecoder <*> go (k - 1)

decodeArrayIndef :: Decoder s Value
decodeArrayIndef = Array . V.fromList <$> go
  where
    go = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> valueDecoder <*> go

decodeObjectN :: Int -> Decoder s Value
decodeObjectN n = Object . KM.fromList <$> go n
  where
    go 0 = pure []
    go k = do
        key <- Dec.decodeString
        v <- valueDecoder
        rest <- go (k - 1)
        pure ((Key.fromText key, v) : rest)

decodeObjectIndef :: Decoder s Value
decodeObjectIndef = Object . KM.fromList <$> go
  where
    go = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                key <- Dec.decodeString
                v <- valueDecoder
                rest <- go
                pure ((Key.fromText key, v) : rest)
