{-# LANGUAGE CPP #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Top-level entry point for the panproto Haskell binding.
--
-- Re-exports the canonical exchange types, the structured value types,
-- the capability classes, the domain surfaces, and the effect adaptors,
-- so @import Panproto@ brings the whole binding into scope. The value
-- modules are re-exported wholesale (@module Panproto.X@); their export
-- lists are disjoint except for six constructor names that "Panproto.Gat"
-- shares with four other domains, all spelled the same but denoting
-- different types:
--
-- * @Iso@ (@Gat.CoercionClass@ vs. @Lens.OpticKind@),
-- * @Var@, @App@, @Let@ (@Gat.Term@ vs. @Expr.Expr@),
-- * @Null@ (@Gat.ValueKind@ vs. @Instance.FieldPresence@),
-- * @SortName@ (@Gat.SortExpr@ vs. @Hom@'s construct-kind type).
--
-- To keep the umbrella unambiguous, these six names are imported
-- @hiding@ from "Panproto.Gat", so the unqualified spelling at
-- @import Panproto@ resolves to the @Lens@ \/ @Expr@ \/ @Instance@ \/
-- @Hom@ constructor; the "Panproto.Gat" constructors of the same name
-- stay reachable through a direct @import Panproto.Gat@ (which is the
-- module they are documented under anyway). Every other name is
-- re-exported unqualified.
--
-- When built with the @rust@ flag the Rust capability instances are
-- brought into scope as well: "Panproto.Rust" is re-exported (its
-- @RustProtocol@ \/ @RustSchema@ representation helpers are part of the
-- public surface), and every other @Panproto.Rust.<Domain>@ instance
-- module is imported for its orphan instances alone (@()@ import), so a
-- plain @import Panproto@ user can call the @Rust@-backed methods of
-- every capability class without importing each backend module by hand.
-- The native backend instances arrive transitively from
-- "Panproto.Native.Protocol" and "Panproto.Native.Schema".
module Panproto
    ( -- * Exchange types
      module Panproto.Canonical

      -- * Structured value types
    , module Panproto.Schema
    , module Panproto.Protocol
    , module Panproto.Instance
    , module Panproto.Enriched

      -- * Errors
    , module Panproto.Errors

      -- * Capability classes
    , module Panproto.Class

      -- * Domain surfaces
    , module Panproto.Migration
    , module Panproto.Migration.Combinators
    , module Panproto.Check
    , module Panproto.Lens
    , module Panproto.Io
    , module Panproto.Gat
    , module Panproto.Expr
    , module Panproto.Hom
    , module Panproto.Graph
    , module Panproto.Data
    , module Panproto.Vcs

      -- * Effect adaptors
    , module Panproto.Effect

#ifdef PANPROTO_RUST_BACKEND
      -- * Rust backend
    , module Panproto.Rust
#endif
    ) where

import Panproto.Canonical
import Panproto.Check
import Panproto.Class
import Panproto.Data
import Panproto.Effect
import Panproto.Enriched
import Panproto.Errors
import Panproto.Expr
-- The six names below are constructors "Panproto.Gat" shares (by
-- spelling, not identity) with Lens \/ Expr \/ Instance \/ Hom. Hiding
-- them here lets the umbrella re-export the other domains' versions
-- unqualified; the Gat constructors remain available via a direct
-- @import Panproto.Gat@.
import Panproto.Gat hiding (App, Iso, Let, Null, SortName, Var)
import Panproto.Graph
import Panproto.Hom
import Panproto.Instance
import Panproto.Io
import Panproto.Lens
import Panproto.Migration
import Panproto.Migration.Combinators
import Panproto.Protocol
import Panproto.Schema
import Panproto.Vcs

-- The native backend instances arrive transitively through these two
-- modules (each defines an orphan @*Backend Native@ instance); they have
-- no public value surface of their own.
import Panproto.Native.Protocol ()
import Panproto.Native.Schema ()

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
-- "Panproto.Rust" carries the @ProtocolBackend Rust@ \/ @SchemaBackend
-- Rust@ instances and the @Rust*@ representation helpers (re-exported
-- above). The remaining backend modules are imported for their orphan
-- capability instances only: bringing them into scope here means a
-- plain @import Panproto@ can dispatch every class method to the Rust
-- backend without importing each one by hand.
import Panproto.Rust
import Panproto.Rust.Check ()
import Panproto.Rust.Data ()
import Panproto.Rust.Enriched ()
import Panproto.Rust.Expr ()
import Panproto.Rust.Gat ()
import Panproto.Rust.Graph ()
import Panproto.Rust.Hom ()
import Panproto.Rust.Instance ()
import Panproto.Rust.Io ()
import Panproto.Rust.Lens ()
import Panproto.Rust.Migration ()
import Panproto.Rust.Vcs ()
#endif

#if defined(PANPROTO_RUST_BACKEND) && defined(PANPROTO_PARSE)
import Panproto.Rust.Parse ()
#endif

#if defined(PANPROTO_RUST_BACKEND) && defined(PANPROTO_PROJECT)
import Panproto.Rust.Project ()
#endif

#if defined(PANPROTO_RUST_BACKEND) && defined(PANPROTO_GIT)
import Panproto.Rust.Git ()
#endif
