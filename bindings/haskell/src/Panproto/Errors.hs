{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE ExistentialQuantification #-}

-- | Errors crossing the panproto-c FFI boundary, plus their Haskell
-- representation.
--
-- The Rust side returns a coarse status code ('PpStatus') and stashes
-- a CBOR-encoded 'ErrorEnvelope' that 'Panproto.Rust.Handle.takeLastError'
-- retrieves. 'PanprotoError' is the Haskell-side exception thrown by
-- 'Panproto.Rust.Handle.checkStatus' when the status code is non-zero.
--
-- Domain-specific failures (parsing, migration, lens laws, …) carry
-- the same @(code, envelope)@ payload but appear as distinct Haskell
-- types so callers can pattern on the surface that failed. They form
-- a single hierarchy rooted at 'SomePanprotoError': catching the
-- parent intercepts every panproto exception, while catching a child
-- intercepts only that surface. The recipe is the standard one from
-- "Control.Exception" (a parent existential plus per-child
-- @toException@ / @fromException@ routed through it).
module Panproto.Errors
    ( -- * Status codes
      PpStatus (..)
    , statusFromInt
    , statusToInt

      -- * Error envelopes
    , ErrorEnvelope (..)
    , envelopeStatus
    , decodeErrorEnvelope

      -- * Exception hierarchy
    , SomePanprotoError (..)
    , panprotoToException
    , panprotoFromException
    , PanprotoError (..)
    , WasmError
    , CheckError (..)
    , ExistenceCheckError (..)
    , ExprError (..)
    , GatError (..)
    , GitBridgeError (..)
    , IoError (..)
    , LensError (..)
    , MigrationError (..)
    , ParseError (..)
    , ProjectError (..)
    , SchemaValidationError (..)
    , VcsError (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Exception (Exception (..), SomeException, toException)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Typeable (cast)
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
    | StatusOperation
    -- ^ The entry point is a compiling stub awaiting engine wiring
    -- (wire code @7@, @PpStatus::Operation@).
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
    7 -> StatusOperation
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
    StatusOperation -> 7
    StatusUnknown n -> n

-- | Decoded form of @panproto_c::error::ErrorEnvelope@.
data ErrorEnvelope = ErrorEnvelope
    { status :: !Int
    , tag :: !Text
    , message :: !Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- Exception hierarchy

-- | Root of the panproto exception hierarchy.
--
-- Every panproto exception, whether the fallback 'PanprotoError' or a
-- domain-specific child like 'ParseError', is wrapped in a
-- 'SomePanprotoError' on its way to 'toException'. Handlers that want
-- to intercept @any@ panproto failure catch this type; handlers that
-- care about a single surface catch the corresponding child instead.
data SomePanprotoError = forall e. Exception e => SomePanprotoError e

instance Show SomePanprotoError where
    show (SomePanprotoError e) = show e

instance Exception SomePanprotoError

-- | Wrap a child exception as a 'SomeException' routed through
-- 'SomePanprotoError'. Children set @toException = panprotoToException@.
panprotoToException :: Exception e => e -> SomeException
panprotoToException = toException . SomePanprotoError

-- | Recover a child exception from a 'SomeException', succeeding when
-- the value is nested inside a 'SomePanprotoError' and downcasts to
-- the requested child type. Children set
-- @fromException = panprotoFromException@.
panprotoFromException :: Exception e => SomeException -> Maybe e
panprotoFromException x = do
    SomePanprotoError e <- fromException x
    cast e

-- | The fallback panproto exception: thrown by the Rust backend
-- whenever the FFI signals failure and no more specific surface is in
-- play. Carries the coarse 'PpStatus' and, when retrievable, the
-- decoded 'ErrorEnvelope'.
data PanprotoError = PanprotoError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception PanprotoError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | Backwards-compatible alias for 'PanprotoError'. The WASM-era
-- binding spelled the root failure @WasmError@; the FFI binding keeps
-- the name available so downstream catch sites need not change.
type WasmError = PanprotoError

-- | A schema failed validation against its protocol.
data SchemaValidationError = SchemaValidationError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception SchemaValidationError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | Compatibility classification (the @check@ surface) failed.
data CheckError = CheckError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception CheckError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A migration existence check reported the mapping is not a valid
-- migration.
data ExistenceCheckError = ExistenceCheckError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception ExistenceCheckError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | An expression failed to parse, evaluate, or typecheck.
data ExprError = ExprError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception ExprError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A GAT-layer operation (theory construction, morphism check, model
-- migration) failed.
data GatError = GatError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception GatError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | The git-import bridge failed (bad path, revspec, or repository).
data GitBridgeError = GitBridgeError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception GitBridgeError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | An instance I/O operation (parse or emit through the registry)
-- failed.
data IoError = IoError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception IoError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A lens operation (get/put, law check, protolens construction)
-- failed.
data LensError = LensError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception LensError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A migration operation (compile, lift, invert, compose) failed.
data MigrationError = MigrationError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception MigrationError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | Parsing source bytes into a schema failed.
data ParseError = ParseError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception ParseError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A multi-file project operation (add file/directory, build) failed.
data ProjectError = ProjectError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception ProjectError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | A version-control operation (commit, branch, merge, blame, …)
-- failed.
data VcsError = VcsError
    { code :: !PpStatus
    , envelope :: !(Maybe ErrorEnvelope)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

instance Exception VcsError where
    toException = panprotoToException
    fromException = panprotoFromException

-- | The 'PpStatus' carried by an 'ErrorEnvelope'.
--
-- Convenience wrapper over 'statusFromInt' applied to the envelope's
-- numeric @status@ field.
envelopeStatus :: ErrorEnvelope -> PpStatus
envelopeStatus env = statusFromInt env.status

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
