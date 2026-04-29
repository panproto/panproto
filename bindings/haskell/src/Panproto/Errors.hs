{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}

-- | Errors crossing the panproto-c FFI boundary, plus their Haskell
-- representation.
--
-- The Rust side returns a coarse status code ('PpStatus') and stashes
-- a CBOR-encoded 'ErrorEnvelope' that 'Panproto.Rust.Handle.takeLastError'
-- retrieves. 'PanprotoError' is the Haskell-side exception thrown by
-- 'Panproto.Rust.Handle.checkStatus' when the status code is non-zero.
module Panproto.Errors
    ( PpStatus (..)
    , statusFromInt
    , statusToInt
    , ErrorEnvelope (..)
    , decodeErrorEnvelope
    , PanprotoError (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Exception (Exception)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import GHC.Generics (Generic)

-- | Mirror of @panproto_c::error::PpStatus@. Values are stable: the
-- numeric encoding is part of the C ABI contract.
data PpStatus
    = StatusOk
    | StatusErr
    | StatusPanic
    | StatusInvalidHandle
    | StatusTypeMismatch
    | StatusSerialization
    | StatusInternal
    | StatusUnknown !Int
    -- ^ Forward-compatibility: an unrecognized code from a newer
    -- @panproto-c@. Treated as a hard error by callers but preserves
    -- the numeric value for diagnostic purposes.
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Convert a raw @i32@ from the FFI to a 'PpStatus'.
statusFromInt :: Int -> PpStatus
statusFromInt = \case
    0 -> StatusOk
    1 -> StatusErr
    2 -> StatusPanic
    3 -> StatusInvalidHandle
    4 -> StatusTypeMismatch
    5 -> StatusSerialization
    6 -> StatusInternal
    n -> StatusUnknown n

-- | Convert a 'PpStatus' to its @i32@ wire form.
statusToInt :: PpStatus -> Int
statusToInt = \case
    StatusOk -> 0
    StatusErr -> 1
    StatusPanic -> 2
    StatusInvalidHandle -> 3
    StatusTypeMismatch -> 4
    StatusSerialization -> 5
    StatusInternal -> 6
    StatusUnknown n -> n

-- | Decoded form of @panproto_c::error::ErrorEnvelope@.
data ErrorEnvelope = ErrorEnvelope
    { status :: !Int
    , tag :: !Text
    , message :: !Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Exception thrown by the Rust backend when the FFI signals failure.
data PanprotoError = PanprotoError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Exception)

-- | Decode the CBOR envelope written by @pp_last_error_take@.
decodeErrorEnvelope :: LBS.ByteString -> Either String ErrorEnvelope
decodeErrorEnvelope bs
    | LBS.null bs = Left "no error envelope (empty buffer)"
    | otherwise =
        case CBOR.deserialiseFromBytes envelopeDecoder bs of
            Left err -> Left (show err)
            Right (rest, env)
                | LBS.null rest -> Right env
                | otherwise -> Left "trailing bytes after CBOR-encoded error envelope"

envelopeDecoder :: Decoder s ErrorEnvelope
envelopeDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    let initial = ErrorEnvelope {status = 0, tag = mempty, message = mempty}
    case mapLen of
        Just n -> readPairs n initial
        Nothing -> readPairsIndef initial

readPairs :: Int -> ErrorEnvelope -> Decoder s ErrorEnvelope
readPairs 0 acc = pure acc
readPairs n acc = do
    acc' <- readOnePair acc
    readPairs (n - 1) acc'

readPairsIndef :: ErrorEnvelope -> Decoder s ErrorEnvelope
readPairsIndef acc = do
    stop <- Dec.decodeBreakOr
    if stop
        then pure acc
        else do
            acc' <- readOnePair acc
            readPairsIndef acc'

readOnePair :: ErrorEnvelope -> Decoder s ErrorEnvelope
readOnePair acc = do
    key <- Dec.decodeString
    case key of
        "status" -> do
            v <- Dec.decodeInt
            pure acc {status = v}
        "tag" -> do
            v <- Dec.decodeString
            pure acc {tag = v}
        "message" -> do
            v <- Dec.decodeString
            pure acc {message = v}
        _ -> do
            -- Skip unknown key's value to stay synced.
            skipValue
            pure acc

-- | Minimal value-skipper: error envelopes only contain int and string
-- values, so a lookahead-based dispatcher suffices.
skipValue :: Decoder s ()
skipValue = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> () <$ Dec.decodeWord
        Dec.TypeUInt64 -> () <$ Dec.decodeWord64
        Dec.TypeNInt -> () <$ Dec.decodeInt
        Dec.TypeNInt64 -> () <$ Dec.decodeInt64
        Dec.TypeInteger -> () <$ Dec.decodeInteger
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        _ -> fail "decodeErrorEnvelope: unexpected CBOR shape in unknown field"
