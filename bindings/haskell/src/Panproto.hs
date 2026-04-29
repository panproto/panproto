{-# LANGUAGE CPP #-}

-- | Top-level entry point for the panproto Haskell binding.
--
-- Re-exports the canonical exchange types, the capability classes,
-- and (when built with the @rust@ flag) the Rust backend. The native
-- backend instances are imported transitively from "Panproto.Native.Protocol".
module Panproto
    ( -- * Exchange types
      module Panproto.Canonical

      -- * Errors
    , module Panproto.Errors

      -- * Capability classes
    , module Panproto.Class

#ifdef PANPROTO_RUST_BACKEND
      -- * Rust backend
    , module Panproto.Rust
#endif
    ) where

import Panproto.Canonical
import Panproto.Class
import Panproto.Errors
import Panproto.Native.Protocol ()
import Panproto.Native.Schema ()

#ifdef PANPROTO_RUST_BACKEND
import Panproto.Rust
#endif
