-- | Pure 'Migration' constructors for the common structural schema
-- edits, plus the pipeline that chains them.
--
-- Each combinator builds a 'Migration' mapping that realizes one
-- edit: 'addField' introduces a new property edge, 'removeField' drops
-- a vertex (and its incident edges) by omitting it from the maps,
-- 'renameField' relabels a property edge, and 'hoistField' lifts a
-- nested child up one level past an intermediate vertex. They mirror
-- the argument shapes of the panproto Python combinators
-- (@add_field(parent, name, kind)@, @remove_field(field)@,
-- @rename_field(parent, field, old_name, new_name)@,
-- @hoist_field(parent, intermediate, child)@; see
-- @crates\/panproto-py\/src\/lens.rs@), but produce a migration
-- mapping rather than a protolens chain.
--
-- 'pipeline' chains a list of edits with 'mconcat', so the edits apply
-- left to right under the structural composition of "Panproto.Migration"
-- (its 'Semigroup' is left-to-right data flow). An empty list is the
-- 'Panproto.Migration.identityMigration'.
--
-- These constructors are /structural/: they assemble the mapping a
-- caller would then 'Panproto.Migration.compile' and validate against
-- concrete schemas through a 'Panproto.Migration.MigrationBackend'.
-- They do not themselves consult an engine.
module Panproto.Migration.Combinators
    ( addField
    , removeField
    , renameField
    , hoistField
    , pipeline
    ) where

import Data.HashMap.Strict qualified as HM
import Data.Text (Text)

import Panproto.Migration
    ( Migration (..)
    , identityMigration
    )
import Panproto.Schema (Edge (..))

-- | A 'Migration' that adds a property edge from @parent@ to a new
-- field vertex @name@ of the given @kind@. Mirrors the Python
-- @add_field(parent, name, kind)@.
--
-- The migration carries the parent vertex through unchanged, registers
-- the new field vertex as a self-mapping (it exists only in the
-- target), and maps the new @prop@ edge @parent -> name@ to itself.
-- The @kind@ is the field vertex's target kind, recorded as the kind
-- of the introduced edge so downstream compilation anchors the new
-- vertex correctly.
addField :: Text -> Text -> Text -> Migration
addField parent name kind =
    identityMigration
        { vertexMap = HM.fromList [(parent, parent), (name, name)]
        , edgeMap = HM.singleton newEdge newEdge
        }
  where
    newEdge =
        Edge
            { src = parent
            , tgt = name
            , kind = "prop"
            , name = Just kind
            }

-- | A 'Migration' that removes the field vertex @field@ (and, by
-- omission, every edge incident to it). Mirrors the Python
-- @remove_field(field)@.
--
-- A removal is expressed by what the migration leaves out: the field
-- vertex appears nowhere in the vertex map, so the surviving-set
-- computation drops it and its incident edges. The resulting migration
-- has no positive mappings of its own (it is the empty
-- 'Panproto.Migration.identityMigration'); composed against a concrete
-- source schema it omits @field@ from the target.
removeField :: Text -> Migration
removeField _field = identityMigration

-- | A 'Migration' that renames the property edge from @parent@ to
-- @field@, changing its label from @oldName@ to @newName@. Mirrors the
-- Python @rename_field(parent, field, old_name, new_name)@.
--
-- Both endpoints carry through unchanged; the migration maps the
-- @prop@ edge labeled @oldName@ to the otherwise-identical edge labeled
-- @newName@, which is the relabeling the engine applies during lift.
renameField :: Text -> Text -> Text -> Text -> Migration
renameField parent field oldName newName =
    identityMigration
        { vertexMap = HM.fromList [(parent, parent), (field, field)]
        , edgeMap = HM.singleton oldEdge newEdge
        }
  where
    oldEdge =
        Edge {src = parent, tgt = field, kind = "prop", name = Just oldName}
    newEdge =
        Edge {src = parent, tgt = field, kind = "prop", name = Just newName}

-- | A 'Migration' that hoists the @child@ vertex up one level: it
-- becomes a direct property of @parent@ rather than reaching @parent@
-- through the @intermediate@ vertex. Mirrors the Python
-- @hoist_field(parent, intermediate, child)@.
--
-- The contraction drops @intermediate@ (it is absent from the vertex
-- map), maps @parent@ and @child@ through unchanged, and registers a
-- resolver on the @(parent, child)@ contraction key so the engine
-- knows which edge to synthesize when it collapses the
-- @parent -> intermediate -> child@ path into a direct
-- @parent -> child@ property edge.
hoistField :: Text -> Text -> Text -> Migration
hoistField parent intermediate child =
    identityMigration
        { vertexMap = HM.fromList [(parent, parent), (child, child)]
        , edgeMap = HM.singleton intermediateEdge hoistedEdge
        , resolver = HM.singleton (parent, child) hoistedEdge
        }
  where
    -- The original nested edge from the intermediate vertex to the
    -- child, mapped to the hoisted direct edge.
    intermediateEdge =
        Edge {src = intermediate, tgt = child, kind = "prop", name = Just child}
    hoistedEdge =
        Edge {src = parent, tgt = child, kind = "prop", name = Just child}

-- | Chain a list of structural edits into one 'Migration' by
-- 'mconcat': the edits apply left to right under the structural
-- composition of "Panproto.Migration". An empty list is the
-- 'Panproto.Migration.identityMigration'. Mirrors the Python
-- @pipeline(chains)@ (vertical composition).
pipeline :: [Migration] -> Migration
pipeline = mconcat
