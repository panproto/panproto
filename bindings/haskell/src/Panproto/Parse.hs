{-# LANGUAGE TypeFamilies #-}

-- | Tree-sitter full-AST source parsing and emission.
--
-- The @parse@ surface of @panproto-c@ wraps @panproto_parse@'s
-- @ParserRegistry@: a registry of tree-sitter grammars (240+ languages
-- under the @group-all@ feature) that parses source bytes into a
-- full-AST 'Panproto.Schema.Schema' and emits a schema back to source.
-- Like the @io@ 'Panproto.Io.IoRegistryRep' and unlike the
-- serializable 'Panproto.Schema.Schema', the registry is not a value
-- type: it lives in the slab as an opaque handle (the Rust
-- @pp_parse_registry_new@ returns a @u32@ that subsequent @pp_parse_*@
-- calls index back into). It is therefore the associated
-- 'ParserRegistryRep', not a value mirrored on the Haskell side,
-- matching how 'Panproto.Class.SchemaRep' and
-- 'Panproto.Io.IoRegistryRep' carry handle-backed state.
--
-- The asymmetric parse\/emit lens (@panproto_parse::ParseEmitLens@) is
-- likewise handle-backed: 'lensFor' produces a 'ParseEmitLensRep' bound
-- to a single protocol against a registry, and the two round-trip laws
-- are machine-checkable on concrete inputs. The @EmitParse@ retraction
-- (@parse(emit(s)) ≅ s@ modulo byte positions) is checked by
-- 'checkEmitParse'; the @ParseEmit@ stability law (@emit(parse(b)) == b@
-- byte-for-byte when @b@ is parseable) by 'checkParseEmit'. Both report
-- @Nothing@ when the law holds and @Just@ the human-readable divergence
-- otherwise, mirroring the @Option<String>@ the Python surface returns
-- (and the @parse@ section of @crates\/panproto-c\/CONTRACT.md@, whose
-- entry points write an empty buffer on success).
--
-- 'SchemaBackend' is a superclass because every parse yields a schema
-- and every emit consumes one: 'parseFile' \/ 'parseWithProtocol'
-- return a 'Panproto.Class.SchemaRep', and 'emit' \/ 'emitPretty'
-- consume one.
--
-- The 'Panproto.Class.Rust' instance is authored in a later wave (in
-- @Panproto.Rust.Parse@); this module declares only the class.
module Panproto.Parse
    ( -- * Capability class
      ParseBackend (..)
    ) where

import Data.ByteString (ByteString)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)

import Panproto.Class (SchemaBackend (..))

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations the @parse@ surface of @panproto-c@ exposes (see
-- @CONTRACT.md@'s @parse@ domain). The registry methods marshal a
-- handle-backed 'ParserRegistryRep' against a schema; 'availableGrammars'
-- enumerates the compiled-in grammars from the backend tag alone.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Parse@); this module declares only the class.
class SchemaBackend back => ParseBackend back where
    -- | Backend-specific representation of the parser registry. For
    -- 'Panproto.Class.Rust' this is an opaque foreign handle into the
    -- slab (the registry is not a serializable value, so there is no
    -- canonical bridge); for a future 'Panproto.Class.Native' backend a
    -- wrapper around the in-process @ParserRegistry@.
    data ParserRegistryRep back :: Type

    -- | Backend-specific representation of a parse\/emit lens bound to a
    -- single protocol. For 'Panproto.Class.Rust' this is an opaque
    -- foreign handle; constructed by 'lensFor' against a registry.
    data ParseEmitLensRep back :: Type

    -- | Create a registry populated with every built-in tree-sitter
    -- grammar. Wraps @pp_parse_registry_new@ (@ParserRegistry::new@).
    registryNew :: Proxy back -> IO (ParserRegistryRep back)

    -- | Parse a source file into a full-AST schema, auto-detecting the
    -- language from the path's extension. Wraps @pp_parse_file@
    -- (@ParserRegistry::parse_file@).
    parseFile
        :: ParserRegistryRep back
        -> Text
        -- ^ File path (the extension selects the grammar).
        -> ByteString
        -- ^ Source bytes.
        -> IO (SchemaRep back)

    -- | Parse source bytes into a full-AST schema under an explicitly
    -- named protocol's grammar. Wraps @pp_parse_with_protocol@
    -- (@ParserRegistry::parse_with_protocol@).
    parseWithProtocol
        :: ParserRegistryRep back
        -> Text
        -- ^ Protocol name (e.g. @"rust"@, @"python"@, @"json"@).
        -> ByteString
        -- ^ Source bytes.
        -> Text
        -- ^ File path recorded on the parsed schema.
        -> IO (SchemaRep back)

    -- | Detect the language protocol for a file path, or 'Nothing' when
    -- no grammar claims the extension. Wraps @pp_parse_detect_language@
    -- (@ParserRegistry::detect_language@, an @Option<&str>@).
    detectLanguage :: ParserRegistryRep back -> Text -> IO (Maybe Text)

    -- | Emit a schema back to source bytes under the named protocol,
    -- preserving parse-derived byte positions and interstitial text.
    -- Wraps @pp_parse_emit@ (@ParserRegistry::emit_with_protocol@).
    emit
        :: ParserRegistryRep back
        -> Text
        -- ^ Protocol name.
        -> SchemaRep back
        -- ^ Schema to emit.
        -> IO ByteString

    -- | Render a by-construction schema to source bytes via the
    -- grammar's production walker. Unlike 'emit', does not require the
    -- schema to carry parse-derived byte positions or interstitial
    -- constraints. Wraps @pp_parse_emit_pretty@
    -- (@ParserRegistry::emit_pretty_with_protocol@).
    emitPretty
        :: ParserRegistryRep back
        -> Text
        -- ^ Protocol name.
        -> SchemaRep back
        -- ^ Schema to render.
        -> IO ByteString

    -- | List the names of every protocol (grammar) registered in the
    -- registry. Wraps @pp_parse_protocol_names@
    -- (@ParserRegistry::protocol_names@).
    protocolNames :: ParserRegistryRep back -> IO [Text]

    -- | List every grammar compiled in by feature flags, independent of
    -- any registry. Wraps @pp_parse_available_grammars@
    -- (@panproto_grammars::grammars@). Needs no registry handle.
    availableGrammars :: Proxy back -> IO [Text]

    -- | Construct a parse\/emit lens bound to @protocol@ against the
    -- registry. Mirrors the Python @AstParserRegistry.lens@; the
    -- resulting handle scopes the round-trip law checks below.
    lensFor :: ParserRegistryRep back -> Text -> IO (ParseEmitLensRep back)

    -- | Verify the @EmitParse@ retraction on a schema: that parsing the
    -- emitted source recovers the schema up to byte positions. Returns
    -- 'Nothing' when the law holds, or 'Just' the divergence text. Wraps
    -- @pp_parse_check_emit_parse@ (@check_emit_parse@).
    checkEmitParse :: ParseEmitLensRep back -> SchemaRep back -> IO (Maybe Text)

    -- | Verify the @ParseEmit@ stability law on source bytes: that
    -- emitting the parsed schema reproduces the bytes exactly. Returns
    -- 'Nothing' when the law holds, or 'Just' the divergence text. Wraps
    -- @pp_parse_check_parse_emit@ (@check_parse_emit@).
    checkParseEmit :: ParseEmitLensRep back -> ByteString -> IO (Maybe Text)

    -- | Release any resources held by the registry. As with the other
    -- backend reps, this is idempotent at the slab level (a freed slot
    -- stays freed; a second free is a no-op).
    releaseRegistry :: ParserRegistryRep back -> IO ()

    -- | Release any resources held by a parse\/emit lens. Idempotent at
    -- the slab level, like 'releaseRegistry'.
    releaseLens :: ParseEmitLensRep back -> IO ()
