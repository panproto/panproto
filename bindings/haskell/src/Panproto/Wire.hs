{-# LANGUAGE DerivingStrategies #-}

-- | An exact, typed carrier for a CBOR subtree whose Rust type is not yet
-- modelled by the public Haskell API.
--
-- Keeping the decoded 'Term' rather than translating through JSON preserves
-- byte strings, integer widths, enum tags, and tuple-keyed maps.  The encoder
-- may choose a different but equivalent definite-length representation; the
-- serde value is unchanged.
module Panproto.Wire
    ( WireTerm (..)
    , encodeWireTerm
    , decodeWireTerm
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Term (Term, decodeTerm, encodeTerm)
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData (..))
import Data.Aeson (FromJSON (..), ToJSON (..))
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS

-- | One arbitrary CBOR value.  This is used only at wire surfaces where a
-- lossy catch-all would make engine → host → engine round trips unsafe.
newtype WireTerm = WireTerm {unWireTerm :: Term}
    deriving stock (Eq, Show)

instance NFData WireTerm where
    rnf (WireTerm term) = term `seq` ()

-- The Aeson instances use a byte array rather than pretending that every CBOR
-- value is JSON.  In particular, this keeps CBOR byte strings distinct from
-- text strings.
instance ToJSON WireTerm where
    toJSON = toJSON . BS.unpack . LBS.toStrict . CBOR.toLazyByteString . encodeWireTerm

instance FromJSON WireTerm where
    parseJSON value = do
        bytes <- BS.pack <$> parseJSON value
        case CBOR.deserialiseFromBytes decodeWireTerm (LBS.fromStrict bytes) of
            Left err -> fail (show err)
            Right (rest, term)
                | LBS.null rest -> pure term
                | otherwise -> fail "trailing bytes after wire term"

-- | Encode a retained CBOR subtree.
encodeWireTerm :: WireTerm -> Encoding
encodeWireTerm (WireTerm term) = encodeTerm term

-- | Decode one CBOR subtree without interpreting or discarding any part.
decodeWireTerm :: Decoder s WireTerm
decodeWireTerm = WireTerm <$> decodeTerm
