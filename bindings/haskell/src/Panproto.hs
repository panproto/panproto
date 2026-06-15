{-# LANGUAGE CPP #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Top-level entry point for the panproto Haskell binding.
--
-- Re-exports the canonical exchange types, the structured value types,
-- the capability classes, and the effect adaptors. When built with the
-- @rust@ flag the Rust backend is re-exported too. The domain surface
-- modules (migration, lens, check, …) are imported so the umbrella
-- carries their instances; each is promoted into this module's export
-- list in the wave that gives it a public API. The native backend
-- instances arrive transitively from "Panproto.Native.Protocol" and
-- "Panproto.Native.Schema".
module Panproto
    ( -- * Exchange types
      module Panproto.Canonical

      -- * Structured value types
    , module Panproto.Schema
    , module Panproto.Protocol

      -- * Errors
    , module Panproto.Errors

      -- * Capability classes
    , module Panproto.Class

      -- * Effect adaptors
    , module Panproto.Effect

#ifdef PANPROTO_RUST_BACKEND
      -- * Rust backend
    , module Panproto.Rust
#endif
    ) where

import Panproto.Canonical
import Panproto.Class
import Panproto.Effect
import Panproto.Errors
import Panproto.Protocol
import Panproto.Schema

-- Domain surfaces: imported for their (forthcoming) instances. Each is
-- promoted to the export list above when it gains a public API.
import Panproto.Check ()
import Panproto.Data ()
import Panproto.Expr ()
import Panproto.Gat ()
import Panproto.Graph ()
import Panproto.Hom ()
import Panproto.Instance ()
import Panproto.Io ()
import Panproto.Lens ()
import Panproto.Migration ()
import Panproto.Migration.Combinators ()
import Panproto.Native.Protocol ()
import Panproto.Native.Schema ()
import Panproto.Vcs ()

#ifdef PANPROTO_LENS_ADAPTORS
import Panproto.Lens.Optics ()
#endif

#ifdef PANPROTO_PARSE
import Panproto.Parse ()
#endif

#ifdef PANPROTO_PROJECT
import Panproto.Project ()
#endif

#ifdef PANPROTO_GIT
import Panproto.Git ()
#endif

#ifdef PANPROTO_RUST_BACKEND
import Panproto.Rust
#endif
