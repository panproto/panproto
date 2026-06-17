{-# LANGUAGE TypeFamilies #-}

-- | High-level wrappers around the FFI surface in "Panproto.Rust.FFI".
--
-- This module turns FFI status codes into 'PanprotoError' (or
-- domain-specific) exceptions, manages 'VecU8' lifecycle via 'bracket',
-- and provides an exception-safe combinator vocabulary for marshalling
-- across the boundary. Everything returns 'IO'; effect-system adapters
-- lift it through "Panproto.Effect".
--
-- The combinators ('withSliceIn', 'callHandleOut', 'callTwoHandlesOut',
-- 'callVecOut', 'callScalarOut', 'callStatus') each pair an FFI shape
-- (slice in, handle out, vec out, scalar out, void-ish) with a status
-- check, so backend modules express an operation as a single combinator
-- application rather than re-deriving the @alloca@ \/ @peek@ \/
-- @checkStatus@ dance.
module Panproto.Rust.Handle
    ( -- * Status checking
      checkStatus
    , throwAs

      -- * Input marshalling
    , withSliceIn

      -- * Output combinators
    , callHandleOut
    , callTwoHandlesOut
    , callVecOut
    , callScalarOut
    , callStatus

      -- * Buffer marshalling
    , withVecU8Out
    , consumeVecU8

      -- * Last error
    , takeLastError
    ) where

import Control.Exception (Exception, SomeException, bracket, throwIO, try)
import Data.ByteString.Internal qualified as BSI (create)
import Data.ByteString.Lazy qualified as LBS
import Data.ByteString.Unsafe qualified as BSU
import Foreign (alloca, peek, poke)
import Foreign.C.Types (CInt (..), CSize)
import Foreign.Marshal.Utils (copyBytes)
import Foreign.Ptr (IntPtr (..), Ptr, castPtr, intPtrToPtr)
import Foreign.Storable (Storable)
import Data.Word (Word32, Word8)

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

-- | Like 'checkStatus', but raise a /domain-specific/ exception built
-- from the @(code, envelope)@ pair instead of the fallback
-- 'PanprotoError'. The first argument is the child constructor (e.g.
-- @ParseError@); on a non-@Ok@ status this drains the last-error slot
-- and throws @con status envelope@.
--
-- This lives in "Panproto.Rust.Handle" rather than "Panproto.Errors"
-- to break the import cycle: throwing requires the last-error drain,
-- which lives at the FFI layer, while "Panproto.Errors" must stay
-- FFI-free so the native-only build can use it.
throwAs
    :: Exception e
    => (PpStatus -> Maybe ErrorEnvelope -> e)
    -- ^ Domain child constructor.
    -> CInt
    -- ^ Raw status from the FFI call.
    -> IO ()
throwAs con c = case statusFromInt (fromIntegral c) of
    StatusOk -> pure ()
    other -> do
        envelope <- safeTakeLastError
        throwIO (con other envelope)
  where
    safeTakeLastError :: IO (Maybe ErrorEnvelope)
    safeTakeLastError = do
        result <- try takeLastError :: IO (Either SomeException (Maybe ErrorEnvelope))
        pure $ case result of
            Right e -> e
            Left _ -> Nothing

-- ---------------------------------------------------------------------------
-- Combinators

-- | Pin a lazy 'LBS.ByteString' and hand its contents to @action@ as a
-- @(Ptr Word8, CSize)@ pair, matching the @*_at@ glue argument shape.
--
-- Empty input passes the @0x1@ sentinel pointer with length @0@ (the
-- same convention 'withVecU8Out' uses): @safer_ffi@'s borrowed-slice
-- entry check requires a non-null pointer, and a zero length means the
-- pointer is never dereferenced. A strict copy is taken so the bytes
-- are contiguous and pinned for the duration of @action@.
withSliceIn :: LBS.ByteString -> (Ptr Word8 -> CSize -> IO a) -> IO a
withSliceIn lbs action
    | LBS.null lbs = action sentinelPtr 0
    | otherwise =
        BSU.unsafeUseAsCStringLen (LBS.toStrict lbs) $ \(ptr, len) ->
            action (castPtr ptr) (fromIntegral len)
  where
    sentinelPtr = intPtrToPtr (IntPtr 1)

-- | Run an FFI call that writes a single handle to a @Ptr Word32@
-- out-param, check its status, and return the handle.
callHandleOut :: (Ptr Word32 -> IO CInt) -> IO Word32
callHandleOut action =
    alloca $ \pHandle -> do
        status <- action pHandle
        checkStatus status
        peek pHandle

-- | Run an FFI call that writes two handles to two @Ptr Word32@
-- out-params, check its status, and return the pair.
callTwoHandlesOut :: (Ptr Word32 -> Ptr Word32 -> IO CInt) -> IO (Word32, Word32)
callTwoHandlesOut action =
    alloca $ \pA ->
        alloca $ \pB -> do
            status <- action pA pB
            checkStatus status
            (,) <$> peek pA <*> peek pB

-- | Run an FFI call that populates a 'VecU8' out-param, check its
-- status /before/ consuming, and return the bytes as a lazy
-- 'LBS.ByteString'. The buffer is freed on the way out.
callVecOut :: (Ptr VecU8 -> IO CInt) -> IO LBS.ByteString
callVecOut action =
    withVecU8Out
        (\pOut -> action pOut >>= checkStatus)
        consumeVecU8

-- | Run an FFI call that writes a 'Storable' scalar (a @u32@ count or
-- @f64@ distance) to a typed out-param, check its status, and return
-- the value. The first argument is an initial value poked into the
-- slot before the call (the result is read back regardless).
callScalarOut :: Storable a => a -> (Ptr a -> IO CInt) -> IO a
callScalarOut initial action =
    alloca $ \pOut -> do
        poke pOut initial
        status <- action pOut
        checkStatus status
        peek pOut

-- | Run a void-ish FFI call (one that mutates in place and returns only
-- a status, such as @pp_project_add_file@) and check its status.
callStatus :: IO CInt -> IO ()
callStatus action = action >>= checkStatus

-- | Allocate a 'VecU8' on the stack, initialize it as a /valid empty/
-- 'VecU8', hand its pointer to @action@, and ensure whatever buffer
-- lands in the slot is freed via 'pp_buf_free_at' on the way out —
-- even if @action@ or @callback@ throws.
--
-- The empty-vec sentinel is @{ ptr = 0x1, len = 0, cap = 0 }@.
-- Rust\'s @Vec@ drop is a no-op when @cap == 0@ (it never reads
-- through @ptr@), and @safer_ffi@\'s @NonNullOwned\<u8\>@ entry
-- check requires only that @ptr@ is non-zero. A literal @null@
-- would trip the entry check; @0x1@ has the alignment a @u8@ needs
-- (1) and is otherwise inert.
--
-- The free is registered as the @bracket@ release around the /whole/
-- call, so it runs whether @action@ succeeds, @action@ throws (e.g.
-- a status check inside it), or @callback@ throws. Freeing the
-- untouched sentinel is the @cap == 0@ no-op above, so this is safe
-- even on the error path where no buffer was allocated — the Haskell
-- side never has to assume the C entry left @*out@ untouched on a
-- non-@Ok@ status.
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
            (pure pVec)
            pp_buf_free_at
            (\p -> action p >> peek p >>= callback)

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
