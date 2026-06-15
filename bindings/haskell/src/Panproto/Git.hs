{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}

-- | The git to panproto-vcs bridge: import a git repository's history
-- into a fresh in-memory schematic-version-control store.
--
-- This mirrors the @git@ surface of @panproto-c@ (see
-- @crates\/panproto-c\/CONTRACT.md@): a single @pp_git_import@ entry
-- point that opens a git repository, walks the requested revision into
-- a fresh @VcsRepo@ store, and hands back a new repository handle plus a
-- CBOR-encoded @{ commit_count, head_id }@ summary. The Rust engine
-- behind it is @panproto_git::import_git_repo@, whose
-- @panproto_git::ImportResult@ this module's 'GitImportResult'
-- summarizes.
--
-- == Wire format
--
-- The summary crosses the boundary as a CBOR map keyed by Rust @serde@
-- field names (@snake_case@): @commit_count@ (a count) and @head_id@
-- (the lowercase hex rendering of the imported HEAD's
-- @panproto_vcs::ObjectId@, matching the C ABI and the Python surface,
-- which both expose object ids as hex strings rather than raw bytes).
-- 'decodeGitImportResult' is tolerant in the usual way: it applies
-- @serde(default)@ semantics for missing fields, skips unknown fields so
-- the Rust side can grow new ones, and accepts both definite- and
-- indefinite-length CBOR maps.
--
-- == Two layers
--
-- 1. 'GitImportResult' is the backend-independent value type: the
--    decoded summary, with a tolerant cborg codec and an aeson surface
--    routed through the canonical 'VcsObjectId' rendering.
--
-- 2. 'GitBackend' is the capability class, parameterized by a backend
--    tag and refining "Panproto.Vcs"'s 'VcsBackend'. It declares the
--    single @import@ operation as a plain 'IO' action that returns the
--    imported repository handle ('RepoRep', the same backend-specific
--    representation 'Panproto.Vcs.vcsInitB' yields) alongside the
--    summary. The FFI instance (@GitBackend Rust@) lands in Wave 2 in
--    "Panproto.Rust.Git"; only the class is defined here.
--
-- 'headId' is carried as the bare hex 'Text' rather than a
-- 'Panproto.Vcs.VcsObjectId' so that 'GitImportResult' has the same
-- aeson and 'Hashable' surface as the other value types in this binding;
-- 'Panproto.Vcs.VcsObjectId' is the typed wrapper to lift it into when a
-- caller wants object-id identity (@VcsObjectId result.headId@).
module Panproto.Git
    ( -- * Import summary
      GitImportResult (..)
    , defaultGitImportResult

      -- * Codec
    , decodeGitImportResult

      -- * Capability class
    , GitBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Word (Word64)
import GHC.Generics (Generic)

import Panproto.Vcs (RepoRep, VcsBackend)

-- ---------------------------------------------------------------------------
-- Import summary

-- | The summary @pp_git_import@ returns alongside the new repository
-- handle, mirroring the wire-relevant fields of
-- @panproto_git::ImportResult@.
--
-- The Rust @ImportResult@ also carries the per-commit object-id mapping
-- it builds during import; that does not cross the FFI boundary. The
-- wire summary keeps what a caller needs to confirm the import: how many
-- commits were ingested and which object id HEAD now resolves to.
data GitImportResult = GitImportResult
    { commitCount :: !Word64
    -- ^ @serde@ field: @commit_count@. Number of commits imported (the
    -- length of the walked revision range; @0@ when HEAD was already
    -- known and nothing new was ingested).
    , headId :: !Text
    -- ^ @serde@ field: @head_id@. Object id the imported HEAD resolves
    -- to, as a lowercase hex string (the @Display@ rendering of
    -- @panproto_vcs::ObjectId@). Wrap in 'Panproto.Vcs.VcsObjectId' for
    -- typed object-id identity. Empty when the imported range was empty.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | The empty summary: no commits imported, HEAD at the empty object id.
-- Seeds the tolerant decoder's @serde(default)@ accumulator.
defaultGitImportResult :: GitImportResult
defaultGitImportResult =
    GitImportResult
        { commitCount = 0
        , headId = Text.empty
        }

-- ---------------------------------------------------------------------------
-- Codec

-- | Decode the CBOR @{ commit_count, head_id }@ summary
-- @pp_git_import@ writes into a 'GitImportResult'.
--
-- Tolerant: missing fields fall back to 'defaultGitImportResult', and
-- unknown fields are skipped so a newer @panproto-c@ can add fields
-- without breaking this decoder. Rejects trailing bytes.
decodeGitImportResult :: LBS.ByteString -> Either String GitImportResult
decodeGitImportResult bs =
    case CBOR.deserialiseFromBytes gitImportResultDecoder bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left "trailing bytes after CBOR-encoded GitImportResult"

gitImportResultDecoder :: Decoder s GitImportResult
gitImportResultDecoder = decodeMap defaultGitImportResult $ \acc key -> case key of
    "commit_count" -> (\v -> acc {commitCount = v}) <$> Dec.decodeWord64
    "head_id" -> (\v -> acc {headId = v}) <$> Dec.decodeString
    _ -> acc <$ skipTerm

-- ---------------------------------------------------------------------------
-- Capability class

-- | Backends that can import a git repository into a fresh schematic
-- version-control store, mirroring the @git@ surface of @panproto-c@.
--
-- This refines "Panproto.Vcs"'s 'VcsBackend': importing produces a
-- repository handle, and that handle is the same backend-specific
-- 'RepoRep' the rest of the version-control porcelain operates on, so a
-- git-importing backend must first be a version-control backend.
-- 'gitImport' returns that imported handle alongside the
-- 'GitImportResult' summary; the caller then drives it with the
-- 'VcsBackend' operations (log, branch, diff, …) or releases it with
-- 'Panproto.Vcs.releaseRepo'.
--
-- Only the class is declared in Wave 1. The @GitBackend Rust@ instance,
-- which dispatches 'gitImport' to the @pp_git_import@ FFI call, lands in
-- Wave 2 in "Panproto.Rust.Git". The 'Proxy' selects the backend: the
-- import takes no backend-tagged representation as input (it produces
-- one), so the tag is supplied explicitly, as with
-- 'Panproto.Class.fromCanonical'.
class VcsBackend back => GitBackend back where
    -- | @import@: open the git repository at @repo_path@, walk the
    -- revision named by @revspec@ into a fresh in-memory store, and
    -- return the imported repository handle plus the import summary.
    --
    -- @repo_path@ and @revspec@ are UTF-8 (a filesystem path and a git
    -- revision specifier, respectively). Maps to @pp_git_import@, which
    -- calls @panproto_git::import_git_repo@.
    gitImport
        :: Proxy back
        -> Text
        -- ^ @repo_path@: path to the git repository.
        -> Text
        -- ^ @revspec@: the revision specifier to import.
        -> IO (RepoRep back, GitImportResult)

-- ---------------------------------------------------------------------------
-- Shared CBOR plumbing
--
-- A pared-down copy of the tolerant map decoder used across this
-- binding (see "Panproto.Vcs"): fold a CBOR map keyed by Rust @serde@
-- field names through a step function, applying @serde(default)@
-- semantics for missing fields and skipping unknown ones.

-- | Decode a CBOR map into a record accumulator. Starting from
-- @initial@ (the @serde(default)@ value), each @(key, value)@ pair is
-- folded through @step@, which reads the value for a recognized key or
-- skips it for an unknown one. Accepts both definite- and
-- indefinite-length maps.
decodeMap :: a -> (a -> Text -> Decoder s a) -> Decoder s a
decodeMap initial step = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = readPair acc >>= goN (n - 1)

    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else readPair acc >>= goIndef

    readPair acc = do
        key <- Dec.decodeString
        step acc key

-- | Skip an arbitrary CBOR value, descending into nested arrays and
-- maps so an unknown field with structured contents does not desync the
-- surrounding decoder. Mirrors @Panproto.Vcs.skipTerm@.
skipTerm :: Decoder s ()
skipTerm = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> () <$ Dec.decodeWord
        Dec.TypeUInt64 -> () <$ Dec.decodeWord64
        Dec.TypeNInt -> () <$ Dec.decodeInt
        Dec.TypeNInt64 -> () <$ Dec.decodeInt64
        Dec.TypeInteger -> () <$ Dec.decodeInteger
        Dec.TypeFloat16 -> () <$ Dec.decodeFloat
        Dec.TypeFloat32 -> () <$ Dec.decodeFloat
        Dec.TypeFloat64 -> () <$ Dec.decodeDouble
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> skipBytesIndef
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> skipStringIndef
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        Dec.TypeBreak -> () <$ Dec.decodeBreakOr
        Dec.TypeInvalid -> fail "skipTerm: invalid CBOR token"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)

    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak

    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure ()
            else skipTerm >> skipTerm >> skipUntilBreakPairs

    skipBytesIndef = do
        Dec.decodeBytesIndef
        skipUntilBreakBytes

    skipStringIndef = do
        Dec.decodeStringIndef
        skipUntilBreakStrings

    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes

    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings
