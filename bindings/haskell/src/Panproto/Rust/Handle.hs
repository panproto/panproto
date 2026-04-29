{-# LANGUAGE TypeFamilies #-}

-- | High-level wrappers around the FFI surface in "Panproto.Rust.FFI".
--
-- This module turns FFI status codes into 'PanprotoError' exceptions,
-- manages 'VecU8' lifecycle via 'bracket', and provides exception-
-- safe helpers for marshalling buffers across the boundary.
-- Everything returns 'IO'; effect-system adapters live in separate
-- packages.
module Panproto.Rust.Handle
    ( -- * Status checking
      checkStatus

      -- * Buffer marshalling
    , withVecU8Out
    , consumeVecU8

      -- * Last error
    , takeLastError
    ) where

import Control.Exception (SomeException, bracket, throwIO, try)
import Data.ByteString.Internal qualified as BSI (create)
import Data.ByteString.Lazy qualified as LBS
import Foreign (alloca, peek, poke)
import Foreign.C.Types (CInt (..))
import Foreign.Marshal.Utils (copyBytes)
import Foreign.Ptr (IntPtr (..), Ptr, intPtrToPtr)

import Panproto.Errors
    ( ErrorEnvelope
    , PanprotoError (..)
    , PpStatus (..)
    , decodeErrorEnvelope
    , statusFromInt
    )
import Panproto.Rust.FFI
    ( VecU8 (..)
    , pp_buf_free_at
    , pp_last_error_take
    )

-- | If @status@ is non-zero, drain the last-error slot, package the
-- result as a 'PanprotoError', and throw it. Use after every FFI
-- call that returns a status code.
--
-- If the envelope retrieval itself fails for any reason (CBOR
-- malformed, slot empty, or 'pp_last_error_take' itself errors), we
-- still raise 'PanprotoError' but with @envelope = Nothing@, so the
-- *original* status is never masked.
checkStatus :: CInt -> IO ()
checkStatus c = case statusFromInt (fromIntegral c) of
    StatusOk -> pure ()
    other -> do
        envelope <- safeTakeLastError
        throwIO PanprotoError {code = other, envelope}
  where
    safeTakeLastError :: IO (Maybe ErrorEnvelope)
    safeTakeLastError = do
        result <- try takeLastError :: IO (Either SomeException (Maybe ErrorEnvelope))
        pure $ case result of
            Right e -> e
            Left _ -> Nothing

-- | Allocate a 'VecU8' on the stack, initialize it as a /valid empty/
-- 'VecU8', hand its pointer to @action@, and ensure the resulting
-- buffer is freed via 'pp_buf_free_at' on the way out — even if
-- @action@ or @callback@ throws.
--
-- The empty-vec sentinel is @{ ptr = 0x1, len = 0, cap = 0 }@.
-- Rust\'s @Vec@ drop is a no-op when @cap == 0@ (it never reads
-- through @ptr@), and @safer_ffi@\'s @NonNullOwned\<u8\>@ entry
-- check requires only that @ptr@ is non-zero. A literal @null@
-- would trip the entry check; @0x1@ has the alignment a @u8@ needs
-- (1) and is otherwise inert.
--
-- The @callback@ receives the populated 'VecU8' AFTER @action@
-- returns and BEFORE the buffer is freed; that is the place to copy
-- bytes into Haskell-managed storage.
withVecU8Out
    :: (Ptr VecU8 -> IO ())
    -- ^ FFI call that populates the out-param.
    -> (VecU8 -> IO a)
    -- ^ Inspect / copy the populated buffer.
    -> IO a
withVecU8Out action callback =
    alloca $ \pVec -> do
        poke pVec emptyVecU8
        bracket
            (action pVec >> peek pVec)
            (\_ -> pp_buf_free_at pVec)
            callback

-- | A valid empty 'VecU8' with a non-null sentinel pointer. See
-- 'withVecU8Out' for why @0x1@ specifically.
emptyVecU8 :: VecU8
emptyVecU8 = VecU8 (intPtrToPtr (IntPtr 1)) 0 0

-- | Copy a 'VecU8' into a lazy 'LBS.ByteString' (the format
-- @cborg@\'s 'Codec.CBOR.Read.deserialiseFromBytes' consumes). The
-- original buffer is freed by the surrounding 'withVecU8Out'.
consumeVecU8 :: VecU8 -> IO LBS.ByteString
consumeVecU8 VecU8 {vecPtr, vecLen}
    | vecLen == 0 = pure LBS.empty
    | otherwise = do
        bs <- BSI.create (fromIntegral vecLen) $ \dst ->
            copyBytes dst vecPtr (fromIntegral vecLen)
        pure (LBS.fromStrict bs)

-- | Take the most recent error envelope, returning 'Nothing' when
-- the slot is empty or the bytes do not decode.
takeLastError :: IO (Maybe ErrorEnvelope)
takeLastError = withVecU8Out populate inspect
  where
    populate p = do
        s <- pp_last_error_take p
        case statusFromInt (fromIntegral s) of
            StatusOk -> pure ()
            other -> throwIO PanprotoError {code = other, envelope = Nothing}

    inspect v = do
        bs <- consumeVecU8 v
        pure $
            if LBS.null bs
                then Nothing
                else either (const Nothing) Just (decodeErrorEnvelope bs)
