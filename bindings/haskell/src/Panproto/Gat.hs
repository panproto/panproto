{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE TypeFamilies #-}

-- | Generalized algebraic theories, theory morphisms, and their models.
--
-- A generalized algebraic theory (GAT) is a named collection of sorts,
-- operations (typed term constructors), and equations (axioms). Sorts
-- may be dependent: a sort can be parameterized by terms of other
-- sorts, which is the feature distinguishing GATs from ordinary
-- algebraic theories. This module mirrors the value-level shape of
-- @panproto_gat@ (see @crates\/panproto-gat\/src@): 'Theory', 'Sort',
-- 'Operation', 'Equation', 'Term', 'SortExpr', 'TheoryMorphism',
-- 'Model', and 'ModelValue'.
--
-- The codecs ('encodeTheory' \/ 'decodeTheory', 'encodeMorphism' \/
-- 'decodeMorphism', 'encodeModelSortInterp' \/ 'decodeModelSortInterp')
-- exchange the CBOR shape @ciborium@ produces and consumes, following
-- the tolerant decoder idiom of "Panproto.Schema" and
-- "Panproto.Instance": map-len-or-indef, key dispatch, positional
-- tuple accumulators, @serde(default)@ for the optional fields, and a
-- depth-first unknown-term skipper for forward compatibility.
--
-- Three Rust shapes are mirrored with deliberate simplifications:
--
-- * 'SortExpr' is @#[serde(untagged)]@ on the Rust side: @Name(n)@
--   serializes as the bare string @"n"@ and @App@ as a struct with
--   @name@ and @args@ fields. The codec dispatches on the CBOR token
--   type (string vs. map) to recover the variant.
--
-- * The 'Theory' @directed_eqs@ and @policies@ fields and the
--   'Operation' carry @panproto_expr::Expr@ values on the Rust side
--   (rewrite implementations, conflict-resolution expressions). The
--   panproto expression AST is not yet mirrored in Haskell
--   ("Panproto.Expr" is a later wave), so both fields store the
--   round-trippable 'Data.Aeson.Value' the expressions serialize to;
--   the codec preserves them verbatim. Both are @serde(default)@, so a
--   theory that uses neither encodes them as empty lists and decodes
--   cleanly from a payload that omits them.
--
-- * 'Model' carries only its theory name and sort interpretations. The
--   Rust @Model@ also holds @op_interp@: a map of operation names to
--   closures (@Arc\<dyn Fn(...)\>@). Closures cannot serialize, so they
--   do not cross the FFI boundary at all; @pp_gat_migrate_model@
--   marshals just the sort-interpretation map
--   (@HashMap\<String, Vec\<ModelValue\>\>@) and reindexes it along a
--   morphism (see @crates\/panproto-c\/CONTRACT.md@). 'Model' and
--   'migrateModel' mirror that: they move the sort-interp portion only.
module Panproto.Gat
    ( -- * Theory
      Theory (..)
    , emptyTheory
    , encodeTheory
    , decodeTheory

      -- * Sorts
    , Sort (..)
    , simpleSort
    , SortParam (..)
    , SortExpr (..)
    , sortExprHead
    , sortExprArgs
    , SortKind (..)
    , ValueKind (..)
    , SortClosure (..)
    , CoercionClass (..)

      -- * Operations
    , Operation (..)
    , Implicit (..)
    , opArity
    , explicitArity

      -- * Terms and equations
    , Term (..)
    , CaseBranch (..)
    , Equation (..)

      -- * Theory accessors
    , theoryName
    , sortCount
    , opCount
    , eqCount
    , sorts
    , ops
    , eqs

      -- * Theory morphism
    , TheoryMorphism (..)
    , emptyMorphism
    , encodeMorphism
    , decodeMorphism
    , MorphismCheckResult (..)
    , decodeMorphismCheckResult

      -- * Model
    , Model (..)
    , emptyModel
    , ModelValue (..)
    , modelTheoryName
    , sortInterpKeys
    , encodeModelSortInterp
    , decodeModelSortInterp
    , encodeModelValue
    , decodeModelValue

      -- * GAT term evaluation and typechecking
    , encodeTermBytes
    , decodeTermBytes
    , encodeTermEnv
    , decodeModelValueBytes
    , encodeSortContext
    , decodeTypecheckResult

      -- * Builder
    , TheoryBuilderM
    , buildTheory
    , sort
    , op
    , eq

      -- * Capability class
    , GatBackend (..)
    , TypecheckResult (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Monad.Trans.State.Strict (State, execState, modify')
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Int (Int64)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T

import Panproto.Json (Value, encodeValue, valueDecoder)

import GHC.Generics (Generic)

-- ---------------------------------------------------------------------------
-- ValueKind

-- | The primitive value kind a value sort ranges over. Mirrors
-- @panproto_gat::sort::ValueKind@, an externally-tagged unit-variant
-- @serde@ enum (a bare CBOR string per variant).
data ValueKind
    = Bool
    | Int
    | Float
    | Str
    | Bytes
    | Token
    | Null
    | Any
    deriving stock (Eq, Show, Generic, Bounded, Enum)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- CoercionClass

-- | The round-trip classification of a coercion. Mirrors
-- @panproto_gat::sort::CoercionClass@, an externally-tagged
-- unit-variant @serde@ enum.
data CoercionClass
    = Iso
    | Retraction
    | Projection
    | Opaque
    deriving stock (Eq, Show, Generic, Bounded, Enum)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- SortKind

-- | The kind of a sort, distinguishing structural sorts from value,
-- coercion, and merger sorts. Mirrors @panproto_gat::sort::SortKind@: an
-- externally-tagged @serde@ enum with one unit variant ('Structural')
-- and three data-carrying variants.
data SortKind
    = Structural
    | Val !ValueKind
    | Coercion !ValueKind !ValueKind !CoercionClass
    -- ^ A directed coercion morphism: @from@ value kind, @to@ value
    -- kind, and round-trip @class@.
    | Merger !ValueKind
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- SortClosure

-- | A sort's closure attribute. Mirrors
-- @panproto_gat::sort::SortClosure@: 'Open' (any op with this output
-- head may inhabit the sort) or 'Closed' (an exhaustive list of
-- constructor op names). Externally-tagged on the wire: 'Open' is a
-- bare string, 'Closed' a single-key map.
data SortClosure
    = Open
    | Closed ![Text]
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- SortExpr

-- | A sort expression: a plain sort name or a dependent sort applied to
-- argument terms. Mirrors @panproto_gat::sort::SortExpr@.
--
-- The Rust type is @#[serde(untagged)]@, so 'SortName' serializes as a
-- bare string and 'SortApp' as a struct with @name@ and @args@. The
-- normalization invariant @SortApp { args = [] } == SortName@ holds on
-- the Rust side; the derived 'Eq' here treats the two spellings as
-- distinct, so construct via 'sortApp'-equivalent care (the codec only
-- ever produces 'SortName' for the empty-argument case, matching the
-- Rust @app@ smart constructor).
data SortExpr
    = SortName !Text
    | SortApp !Text ![Term]
    -- ^ A dependent sort applied to argument terms: the sort @name@ and
    -- the argument @terms@, one per declared sort parameter.
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | The bare sort name, ignoring any applied arguments. Mirrors
-- @SortExpr::head@.
sortExprHead :: SortExpr -> Text
sortExprHead = \case
    SortName n -> n
    SortApp n _ -> n

-- | The argument terms, if any; empty list for 'SortName'. Mirrors
-- @SortExpr::args@.
sortExprArgs :: SortExpr -> [Term]
sortExprArgs = \case
    SortName _ -> []
    SortApp _ as -> as

-- ---------------------------------------------------------------------------
-- Term

-- | A term in a GAT expression. Mirrors @panproto_gat::eq::Term@: an
-- externally-tagged @serde@ enum built from variables, operation
-- applications, case analyses, typed holes, and let-bindings.
--
-- The variants use positional fields rather than record syntax: the
-- @Hole@ and @Let@ Rust variants both carry a @name@ field but at
-- different types (@Option\<Arc\<str\>\>@ vs @Arc\<str\>@), which a
-- shared record selector cannot express.
data Term
    = Var !Text
    -- ^ A variable reference (e.g. @x@, @a@).
    | App !Text ![Term]
    -- ^ An operation applied to arguments (e.g. @add(x, y)@): @op@ name
    -- and argument terms.
    | Case !Term ![CaseBranch]
    -- ^ A case analysis on a closed-sort scrutinee: the @scrutinee@ term
    -- and one @branch@ per constructor of its closed sort.
    | Hole !(Maybe Text)
    -- ^ A typed hole: a placeholder with an optional name.
    | Let !Text !Term !Term
    -- ^ A local @let name = bound in body@ binding: @name@, @bound@
    -- term, and @body@ in which @name@ is in scope.
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | One branch of a 'Case' expression. Mirrors
-- @panproto_gat::eq::CaseBranch@.
data CaseBranch = CaseBranch
    { constructor :: !Text
    -- ^ Constructor op name.
    , binders :: ![Text]
    -- ^ Local binders; one per input of @constructor@.
    , branchBody :: !Term
    -- ^ The branch body.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Implicit

-- | Whether an 'Operation' input is implicit (recovered by unification
-- at the call site) or explicit (caller-supplied). Mirrors
-- @panproto_gat::op::Implicit@, an externally-tagged unit-variant enum
-- (@No@ \/ @Yes@).
data Implicit
    = ExplicitParam
    | ImplicitParam
    deriving stock (Eq, Show, Generic, Bounded, Enum)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Operation

-- | An operation (term constructor) in a GAT. Mirrors
-- @panproto_gat::op::Operation@. Each operation has typed inputs and a
-- typed output, where types are sort expressions. Input parameter names
-- are in scope in later input sorts and in the output sort, enabling
-- dependent signatures.
data Operation = Operation
    { opName :: !Text
    -- ^ The operation name (e.g. @src@, @compose@).
    , inputs :: ![(Text, SortExpr, Implicit)]
    -- ^ Typed inputs as @(param_name, sort_expr, implicit)@ triples.
    , output :: !SortExpr
    -- ^ The output sort expression; may reference any input parameter.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | The total number of inputs (implicit and explicit). Mirrors
-- @Operation::arity@.
opArity :: Operation -> Int
opArity o = length o.inputs

-- | The number of explicit (caller-supplied) inputs. Mirrors
-- @Operation::explicit_arity@.
explicitArity :: Operation -> Int
explicitArity o = length [() | (_, _, ExplicitParam) <- o.inputs]

-- ---------------------------------------------------------------------------
-- SortParam and Sort

-- | A parameter of a dependent sort. Mirrors
-- @panproto_gat::sort::SortParam@.
data SortParam = SortParam
    { paramName :: !Text
    -- ^ The parameter name (e.g. @a@, @b@).
    , paramSort :: !SortExpr
    -- ^ The sort expression this parameter ranges over.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A sort declaration in a GAT. Mirrors @panproto_gat::sort::Sort@.
-- Sorts are the types of a GAT; they may be simple (no parameters) or
-- dependent (parameterized by terms of other sorts), and may be
-- declared closed against an enumerated set of constructors. The @kind@
-- and @closure@ fields are @serde(default)@ on the Rust side.
data Sort = Sort
    { sortName :: !Text
    -- ^ The sort name (e.g. @Vertex@, @Hom@).
    , params :: ![SortParam]
    -- ^ Parameters this sort depends on. Empty for simple sorts.
    , kind :: !SortKind
    -- ^ The kind of this sort (structural, value, coercion, or merger).
    , closure :: !SortClosure
    -- ^ Closure attribute. Defaults to 'Open'.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A simple (non-dependent) sort with structural kind and open
-- closure. Mirrors @Sort::simple@.
simpleSort :: Text -> Sort
simpleSort n =
    Sort {sortName = n, params = [], kind = Structural, closure = Open}

-- ---------------------------------------------------------------------------
-- Equation

-- | An equation (axiom) in a GAT: a judgemental equality between two
-- terms that must hold in every model of the theory. Mirrors
-- @panproto_gat::eq::Equation@.
data Equation = Equation
    { eqName :: !Text
    -- ^ A human-readable name (e.g. @left_identity@).
    , lhs :: !Term
    -- ^ The left-hand side of the equality.
    , rhs :: !Term
    -- ^ The right-hand side of the equality.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Theory

-- | A generalized algebraic theory: a named collection of sorts,
-- operations, and equations, with optional inheritance via @extends@.
-- Mirrors @panproto_gat::theory::Theory@.
--
-- The Rust @Theory@ also carries @directed_eqs@ (rewrite rules) and
-- @policies@ (conflict-resolution policies); both embed
-- @panproto_expr::Expr@ values and both are @serde(default)@. The
-- expression AST is not yet mirrored in Haskell, so this type stores
-- those two collections as round-trippable aeson 'Value's
-- ('directedEqs', 'policies') and the codec preserves them verbatim.
-- The five @FxHashMap@ index caches on the Rust side are rebuilt from
-- the vectors at construction time and are not part of the serialized
-- shape, so they have no Haskell counterpart.
data Theory = Theory
    { name :: !Text
    -- ^ The theory name (e.g. @Monoid@, @Category@).
    , extends :: ![Text]
    -- ^ Names of parent theories this theory extends.
    , theorySorts :: ![Sort]
    -- ^ Sort declarations.
    , theoryOps :: ![Operation]
    -- ^ Operation declarations.
    , theoryEqs :: ![Equation]
    -- ^ Equations (axioms).
    , directedEqs :: ![Value]
    -- ^ Directed equations (rewrite rules), preserved as aeson 'Value's
    -- because they embed @panproto_expr::Expr@. @serde(default)@.
    , policies :: ![Value]
    -- ^ Conflict-resolution policies, preserved as aeson 'Value's
    -- because they embed @panproto_expr::Expr@. @serde(default)@.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A theory with the given name and no sorts, operations, equations,
-- directed equations, or policies. Useful as a builder base and test
-- fixture.
emptyTheory :: Text -> Theory
emptyTheory n =
    Theory
        { name = n
        , extends = []
        , theorySorts = []
        , theoryOps = []
        , theoryEqs = []
        , directedEqs = []
        , policies = []
        }

-- ---------------------------------------------------------------------------
-- Theory accessors

-- | The theory name. Mirrors the Python @Theory.name@.
theoryName :: Theory -> Text
theoryName t = t.name

-- | Number of sorts. Mirrors the Python @Theory.sort_count@.
sortCount :: Theory -> Int
sortCount t = length t.theorySorts

-- | Number of operations. Mirrors the Python @Theory.op_count@.
opCount :: Theory -> Int
opCount t = length t.theoryOps

-- | Number of equations. Mirrors the Python @Theory.eq_count@.
eqCount :: Theory -> Int
eqCount t = length t.theoryEqs

-- | The sort declarations. Mirrors the Python @Theory.sorts@.
sorts :: Theory -> [Sort]
sorts t = t.theorySorts

-- | The operation declarations. Mirrors the Python @Theory.ops@.
ops :: Theory -> [Operation]
ops t = t.theoryOps

-- | The equations. Mirrors the Python @Theory.eqs@.
eqs :: Theory -> [Equation]
eqs t = t.theoryEqs

-- ---------------------------------------------------------------------------
-- TheoryMorphism

-- | A structure-preserving map between two theories. Mirrors
-- @panproto_gat::morphism::TheoryMorphism@. A valid morphism preserves
-- sort arities, operation type signatures, and equations.
data TheoryMorphism = TheoryMorphism
    { morphismName :: !Text
    -- ^ A human-readable name for this morphism.
    , domain :: !Text
    -- ^ The name of the domain theory.
    , codomain :: !Text
    -- ^ The name of the codomain theory.
    , sortMap :: !(HashMap Text Text)
    -- ^ Mapping from domain sort names to codomain sort names.
    , opMap :: !(HashMap Text Text)
    -- ^ Mapping from domain operation names to codomain operation names.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A morphism with the given name, domain, and codomain and empty sort
-- and operation maps.
emptyMorphism :: Text -> Text -> Text -> TheoryMorphism
emptyMorphism n d c =
    TheoryMorphism
        { morphismName = n
        , domain = d
        , codomain = c
        , sortMap = HM.empty
        , opMap = HM.empty
        }

-- | The result of a morphism validity check. Mirrors
-- @panproto_c::api::helpers::MorphismCheckResult@, the CBOR shape
-- @pp_gat_check_morphism@ emits.
data MorphismCheckResult = MorphismCheckResult
    { valid :: !Bool
    -- ^ Whether the morphism is valid.
    , morphismError :: !(Maybe Text)
    -- ^ Human-readable description of the failure, if any.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- ModelValue and Model

-- | A value in a model interpretation. Mirrors
-- @panproto_gat::model::ModelValue@: an externally-tagged @serde@ enum
-- of JSON-like values. The elements that sorts are interpreted as, and
-- the values operations produce and consume.
data ModelValue
    = MVStr !Text
    | MVInt !Int64
    | MVBool !Bool
    | MVList ![ModelValue]
    | MVMap !(HashMap Text ModelValue)
    | MVNull
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A model (interpretation) of a theory, carrying only the
-- serializable portion. Mirrors the FFI-visible slice of
-- @panproto_gat::model::Model@.
--
-- The Rust @Model@ maps each sort to a carrier set of 'ModelValue's
-- (@sort_interp@) and each operation to a closure (@op_interp@). The
-- closures (@Arc\<dyn Fn(...)\>@) cannot serialize and never cross the
-- FFI boundary, so this type carries only the theory name and the
-- sort-interpretation map. @pp_gat_migrate_model@ likewise marshals and
-- reindexes the sort-interp map alone (see
-- @crates\/panproto-c\/CONTRACT.md@), and 'migrateModel' mirrors that.
data Model = Model
    { theory :: !Text
    -- ^ The name of the theory this model interprets.
    , sortInterp :: !(HashMap Text [ModelValue])
    -- ^ Sort interpretations: each sort name maps to its carrier set.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A model for the given theory with no sort interpretations. Mirrors
-- @Model::new@.
emptyModel :: Text -> Model
emptyModel t = Model {theory = t, sortInterp = HM.empty}

-- | The name of the theory this model interprets. Mirrors the Python
-- @Model.theory_name@.
modelTheoryName :: Model -> Text
modelTheoryName m = m.theory

-- | The sort names that have carrier sets in this model. Mirrors the
-- Python @Model.sort_interp_keys@.
sortInterpKeys :: Model -> [Text]
sortInterpKeys m = HM.keys m.sortInterp

-- ---------------------------------------------------------------------------
-- TheoryBuilder

-- | A 'State'-monad builder for assembling a 'Theory' imperatively.
-- Mirrors the Python @TheoryBuilder@: each combinator appends one
-- declaration to the in-progress theory and 'buildTheory' finalizes it.
type TheoryBuilderM = State Theory

-- | Run a builder against 'emptyTheory' for the given theory name.
buildTheory :: Text -> TheoryBuilderM () -> Theory
buildTheory n = (`execState` emptyTheory n)

-- | Append a sort declaration.
sort :: Sort -> TheoryBuilderM ()
sort s = modify' $ \t -> t {theorySorts = t.theorySorts <> [s]}

-- | Append an operation declaration.
op :: Operation -> TheoryBuilderM ()
op o = modify' $ \t -> t {theoryOps = t.theoryOps <> [o]}

-- | Append an equational axiom.
eq :: Equation -> TheoryBuilderM ()
eq e = modify' $ \t -> t {theoryEqs = t.theoryEqs <> [e]}

-- ---------------------------------------------------------------------------
-- Capability class

-- | The result of typechecking a GAT 'Term' against a 'Theory'. Mirrors
-- the @{ well_formed, output_sort, error }@ CBOR shape @pp_expr_check@
-- emits (@gat::typecheck_term@).
data TypecheckResult = TypecheckResult
    { wellFormed :: !Bool
    -- ^ Whether the term is well-formed in the theory.
    , outputSort :: !(Maybe Text)
    -- ^ The inferred output sort, if the term is well-formed. The engine
    -- emits @SortExpr::to_string()@ (e.g. @"Vertex"@, @"Hom(a, b)"@), not
    -- a structured 'SortExpr', so this field carries that rendered form
    -- verbatim.
    , typecheckError :: !(Maybe Text)
    -- ^ Human-readable description of the failure, if any.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Operations the @gat@ surface of @panproto-c@ exposes (see
-- @CONTRACT.md@'s @gat@ domain), plus the two @expr@ entry points that
-- operate on GAT 'Term's within a 'Theory' (@pp_expr_eval_gat@ and
-- @pp_expr_check@).
--
-- Each backend carries theories and models in its own 'TheoryRep' and
-- 'ModelRep' (an opaque foreign handle for the Rust backend, a thin
-- wrapper around the value for a pure backend) and bridges to the
-- shared 'Theory' and 'Model' value types through 'ingestTheory' \/
-- 'reifyTheory' and the model-producing operations.
--
-- The 'Rust' instance is authored later (in @Panproto.Rust.Gat@); this
-- module declares only the class.
class GatBackend back where
    -- | Backend-specific representation of a 'Theory'. For the Rust
    -- backend an opaque foreign handle; for a pure backend a wrapper
    -- around the value.
    data TheoryRep back :: Type

    -- | Backend-specific representation of a 'Model'. Models are not
    -- serializable in full (operation interpretations are closures), so
    -- this is always an opaque, non-reifiable handle; introspection
    -- goes through 'modelTheoryNameIO' \/ 'sortInterpKeysIO' and
    -- 'checkModel'.
    data ModelRep back :: Type

    -- | Ingest a structured 'Theory' into the backend. Wraps
    -- @pp_gat_create_theory@ (@gat::create_theory@).
    ingestTheory :: Proxy back -> Theory -> IO (TheoryRep back)

    -- | Materialize the backend representation as a structured 'Theory'.
    reifyTheory :: TheoryRep back -> IO Theory

    -- | Release any resources held by the theory representation.
    -- Idempotent at the slab level, as with the other backend reps.
    releaseTheory :: TheoryRep back -> IO ()

    -- | Compute the colimit (pushout) of two theories over a shared
    -- sub-theory. Wraps @pp_gat_colimit@ (@gat::colimit_by_name@).
    colimitTheories
        :: TheoryRep back
        -> TheoryRep back
        -> TheoryRep back
        -- ^ First theory, second theory, shared sub-theory (the apex).
        -> IO (TheoryRep back)

    -- | Construct the free (initial) model of a theory, bounded by a
    -- maximum term-generation depth and a per-sort term cap. Wraps
    -- @gat::free_model@ (the Python @free_model@).
    freeModel
        :: TheoryRep back
        -> Int
        -> Int
        -- ^ @max_depth@ and @max_terms_per_sort@.
        -> IO (ModelRep back)

    -- | Check a model against its theory, returning the list of
    -- equation-violation descriptions (empty means the model satisfies
    -- every equation). Wraps @gat::check_model@ (the Python
    -- @check_model@).
    checkModel :: ModelRep back -> TheoryRep back -> IO [Text]

    -- | Check that a theory morphism is well-defined between two
    -- theories. Wraps @pp_gat_check_morphism@ (@gat::check_morphism@).
    checkMorphism
        :: TheoryMorphism
        -> TheoryRep back
        -> TheoryRep back
        -- ^ Morphism, domain theory, codomain theory.
        -> IO MorphismCheckResult

    -- | Migrate a model along a theory morphism, reindexing the
    -- sort-interpretation map (the only serializable portion of a
    -- 'Model'). The 'Proxy' selects the backend; the operation is
    -- otherwise pure CBOR-in / CBOR-out with no theory handle. Wraps
    -- @pp_gat_migrate_model@ (@gat::migrate_model@).
    migrateModel :: Proxy back -> TheoryMorphism -> Model -> IO Model

    -- | Evaluate a GAT 'Term' to a 'ModelValue' under an environment
    -- (variable bindings) and a theory. Wraps @pp_expr_eval_gat@
    -- (@panproto_expr::eval@ over GAT terms).
    evalGatTerm
        :: TheoryRep back
        -> Term
        -> [(Text, ModelValue)]
        -- ^ Theory, term, environment bindings.
        -> IO ModelValue

    -- | Typecheck a GAT 'Term' against a theory under a sort context
    -- (variable-to-sort-name bindings). Wraps @pp_expr_check@
    -- (@gat::typecheck_term@).
    typecheckTerm
        :: TheoryRep back
        -> Term
        -> [(Text, Text)]
        -- ^ Theory, term, context (variable name to sort name).
        -> IO TypecheckResult

    -- | The theory name a model interprets. Wraps the Python
    -- @Model.theory_name@; the pure counterpart is 'modelTheoryName'.
    modelTheoryNameIO :: ModelRep back -> IO Text

    -- | The sort names that have carrier sets in a model. Wraps the
    -- Python @Model.sort_interp_keys@; the pure counterpart is
    -- 'sortInterpKeys'.
    sortInterpKeysIO :: ModelRep back -> IO [Text]

    -- | Release any resources held by the model representation.
    releaseModel :: ModelRep back -> IO ()

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'Theory' to the CBOR shape @ciborium@ deserializes. The
-- @directed_eqs@ and @policies@ aeson 'Value's are re-encoded to CBOR
-- verbatim; the index caches are not emitted (Rust rebuilds them on
-- deserialization).
encodeTheory :: Theory -> LBS.ByteString
encodeTheory t =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 7
            <> kv "name" (Enc.encodeString t.name)
            <> kv "extends" (encodeList Enc.encodeString t.extends)
            <> kv "sorts" (encodeList encodeSort t.theorySorts)
            <> kv "ops" (encodeList encodeOperation t.theoryOps)
            <> kv "eqs" (encodeList encodeEquation t.theoryEqs)
            <> kv "directed_eqs" (encodeList encodeValue t.directedEqs)
            <> kv "policies" (encodeList encodeValue t.policies)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a 'TheoryMorphism' to its CBOR shape. The sort and operation
-- maps are string-keyed, so they encode as plain CBOR maps.
encodeMorphism :: TheoryMorphism -> LBS.ByteString
encodeMorphism m =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 5
            <> kv "name" (Enc.encodeString m.morphismName)
            <> kv "domain" (Enc.encodeString m.domain)
            <> kv "codomain" (Enc.encodeString m.codomain)
            <> kv "sort_map" (encodeTextMap Enc.encodeString m.sortMap)
            <> kv "op_map" (encodeTextMap Enc.encodeString m.opMap)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a model's sort-interpretation map as the CBOR
-- @HashMap\<String, Vec\<ModelValue\>\>@ shape @pp_gat_migrate_model@
-- consumes. Operation interpretations are not part of the wire shape.
encodeModelSortInterp :: Model -> LBS.ByteString
encodeModelSortInterp m =
    CBOR.toLazyByteString $
        encodeTextMap (encodeList encodeModelValue) m.sortInterp

-- | Encode a 'Term' to the top-level CBOR shape @pp_expr_eval_gat@ and
-- @pp_expr_check@ consume as their @expr@ argument.
encodeTermBytes :: Term -> LBS.ByteString
encodeTermBytes = CBOR.toLazyByteString . encodeTerm

-- | Decode a top-level CBOR 'Term'.
decodeTermBytes :: LBS.ByteString -> Either String Term
decodeTermBytes = runDecoder decodeTerm "term"

-- | Encode a variable environment for @pp_expr_eval_gat@: the
-- @Vec<(String, ModelValue)>@ the @env@ argument expects. @serde@
-- serializes a @Vec@ of pairs as a CBOR array of two-element
-- @[name, value]@ arrays.
encodeTermEnv :: [(Text, ModelValue)] -> LBS.ByteString
encodeTermEnv = CBOR.toLazyByteString . encodeNamedPairs encodeModelValue

-- | Encode a typing context for @pp_expr_check@: the
-- @Vec<(String, String)>@ the @context@ argument expects (variable name
-- to sort name). Like 'encodeTermEnv', a CBOR array of two-element
-- @[name, sort]@ arrays.
encodeSortContext :: [(Text, Text)] -> LBS.ByteString
encodeSortContext = CBOR.toLazyByteString . encodeNamedPairs Enc.encodeString

-- | Encode a @Vec@ of @(name, value)@ pairs as a CBOR array of
-- two-element arrays, matching @serde@'s tuple encoding.
encodeNamedPairs :: (v -> Encoding) -> [(Text, v)] -> Encoding
encodeNamedPairs enc =
    encodeList (\(k, v) -> Enc.encodeListLen 2 <> Enc.encodeString k <> enc v)

encodeSort :: Sort -> Encoding
encodeSort s =
    Enc.encodeMapLen 4
        <> kv "name" (Enc.encodeString s.sortName)
        <> kv "params" (encodeList encodeSortParam s.params)
        <> kv "kind" (encodeSortKind s.kind)
        <> kv "closure" (encodeSortClosure s.closure)
  where
    kv k v = Enc.encodeString k <> v

encodeSortParam :: SortParam -> Encoding
encodeSortParam p =
    Enc.encodeMapLen 2
        <> Enc.encodeString "name"
        <> Enc.encodeString p.paramName
        <> Enc.encodeString "sort"
        <> encodeSortExpr p.paramSort

-- | Encode a 'SortExpr' in the @#[serde(untagged)]@ form: 'SortName' as
-- a bare string, 'SortApp' as a @{ name, args }@ struct.
encodeSortExpr :: SortExpr -> Encoding
encodeSortExpr = \case
    SortName n -> Enc.encodeString n
    SortApp n as ->
        Enc.encodeMapLen 2
            <> Enc.encodeString "name"
            <> Enc.encodeString n
            <> Enc.encodeString "args"
            <> encodeList encodeTerm as

-- | Encode a 'SortKind' as an externally-tagged @serde@ enum: the unit
-- variant 'Structural' as a bare string, the data variants as
-- single-key maps.
encodeSortKind :: SortKind -> Encoding
encodeSortKind = \case
    Structural -> Enc.encodeString "Structural"
    Val vk -> variant "Val" (encodeValueKind vk)
    Coercion f tk c ->
        variant "Coercion" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "from"
                <> encodeValueKind f
                <> Enc.encodeString "to"
                <> encodeValueKind tk
                <> Enc.encodeString "class"
                <> encodeCoercionClass c
    Merger vk -> variant "Merger" (encodeValueKind vk)
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

encodeSortClosure :: SortClosure -> Encoding
encodeSortClosure = \case
    Open -> Enc.encodeString "Open"
    Closed cs ->
        Enc.encodeMapLen 1
            <> Enc.encodeString "Closed"
            <> encodeList Enc.encodeString cs

encodeValueKind :: ValueKind -> Encoding
encodeValueKind = Enc.encodeString . valueKindTag

encodeCoercionClass :: CoercionClass -> Encoding
encodeCoercionClass = Enc.encodeString . coercionClassTag

encodeOperation :: Operation -> Encoding
encodeOperation o =
    Enc.encodeMapLen 3
        <> kv "name" (Enc.encodeString o.opName)
        <> kv "inputs" (encodeList encodeInput o.inputs)
        <> kv "output" (encodeSortExpr o.output)
  where
    kv k v = Enc.encodeString k <> v
    encodeInput (pn, se, imp) =
        Enc.encodeListLen 3
            <> Enc.encodeString pn
            <> encodeSortExpr se
            <> encodeImplicit imp

encodeImplicit :: Implicit -> Encoding
encodeImplicit = \case
    ExplicitParam -> Enc.encodeString "No"
    ImplicitParam -> Enc.encodeString "Yes"

encodeEquation :: Equation -> Encoding
encodeEquation e =
    Enc.encodeMapLen 3
        <> Enc.encodeString "name"
        <> Enc.encodeString e.eqName
        <> Enc.encodeString "lhs"
        <> encodeTerm e.lhs
        <> Enc.encodeString "rhs"
        <> encodeTerm e.rhs

-- | Encode a 'Term' as an externally-tagged @serde@ enum: 'Var' as a
-- single-key map, the rest as single-key maps over their struct bodies.
encodeTerm :: Term -> Encoding
encodeTerm = \case
    Var v -> variant "Var" (Enc.encodeString v)
    App o as ->
        variant "App" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "op"
                <> Enc.encodeString o
                <> Enc.encodeString "args"
                <> encodeList encodeTerm as
    Case sc bs ->
        variant "Case" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "scrutinee"
                <> encodeTerm sc
                <> Enc.encodeString "branches"
                <> encodeList encodeCaseBranch bs
    Hole mn ->
        variant "Hole" $
            Enc.encodeMapLen 1
                <> Enc.encodeString "name"
                <> maybe Enc.encodeNull Enc.encodeString mn
    Let n b bd ->
        variant "Let" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "name"
                <> Enc.encodeString n
                <> Enc.encodeString "bound"
                <> encodeTerm b
                <> Enc.encodeString "body"
                <> encodeTerm bd
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

encodeCaseBranch :: CaseBranch -> Encoding
encodeCaseBranch b =
    Enc.encodeMapLen 3
        <> Enc.encodeString "constructor"
        <> Enc.encodeString b.constructor
        <> Enc.encodeString "binders"
        <> encodeList Enc.encodeString b.binders
        <> Enc.encodeString "body"
        <> encodeTerm b.branchBody

-- | Encode a 'ModelValue' as an externally-tagged @serde@ enum: 'MVNull'
-- as a bare string, the rest as single-key maps.
encodeModelValue :: ModelValue -> Encoding
encodeModelValue = \case
    MVStr t -> variant "Str" (Enc.encodeString t)
    MVInt n -> variant "Int" (Enc.encodeInt64 n)
    MVBool b -> variant "Bool" (Enc.encodeBool b)
    MVList xs -> variant "List" (encodeList encodeModelValue xs)
    MVMap m -> variant "Map" (encodeTextMap encodeModelValue m)
    MVNull -> Enc.encodeString "Null"
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

-- | Encode a string-keyed 'HashMap' as a CBOR map.
encodeTextMap :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeTextMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @Theory@ bytes into a structured 'Theory'. Tolerant of
-- unknown fields and missing @serde(default)@ fields (@directed_eqs@,
-- @policies@); the index-cache fields, if present, are skipped.
decodeTheory :: LBS.ByteString -> Either String Theory
decodeTheory = runDecoder theoryDecoder "theory"

-- | Decode CBOR @TheoryMorphism@ bytes into a structured
-- 'TheoryMorphism'.
decodeMorphism :: LBS.ByteString -> Either String TheoryMorphism
decodeMorphism = runDecoder morphismDecoder "theory morphism"

-- | Decode CBOR @MorphismCheckResult@ bytes (the @pp_gat_check_morphism@
-- output shape).
decodeMorphismCheckResult :: LBS.ByteString -> Either String MorphismCheckResult
decodeMorphismCheckResult = runDecoder morphismCheckResultDecoder "morphism check result"

-- | Decode CBOR @HashMap\<String, Vec\<ModelValue\>\>@ bytes (the
-- @pp_gat_migrate_model@ output shape) into a 'Model' for the named
-- theory.
decodeModelSortInterp :: Text -> LBS.ByteString -> Either String Model
decodeModelSortInterp theoryName' =
    runDecoder (modelFor <$> sortInterpDecoder) "model sort interpretations"
  where
    modelFor si = Model {theory = theoryName', sortInterp = si}

-- | Decode a top-level CBOR 'ModelValue' (the @pp_expr_eval_gat@ output
-- shape).
decodeModelValueBytes :: LBS.ByteString -> Either String ModelValue
decodeModelValueBytes = runDecoder decodeModelValue "model value"

-- | Decode CBOR @CheckOutput@ bytes (the @pp_expr_check@ output shape:
-- @{ well_formed, output_sort, error }@) into a 'TypecheckResult'.
decodeTypecheckResult :: LBS.ByteString -> Either String TypecheckResult
decodeTypecheckResult = runDecoder typecheckResultDecoder "typecheck result"

runDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

-- Built positionally rather than via record update: the @name@ field is
-- shared with 'SortExpr', so a record update @acc {name = v}@ is
-- ambiguous under 'DuplicateRecordFields'. Threading a tuple
-- accumulator sidesteps that while tolerating field reordering and
-- unknown fields.
theoryDecoder :: Decoder s Theory
theoryDecoder = decodeFields (T.empty, [], [], [], [], [], []) build handler
  where
    build (n, ex, ss, os, es, des, ps) = Theory n ex ss os es des ps
    handler acc@(n, ex, ss, os, es, des, ps) key = case key of
        "name" -> (\v -> (v, ex, ss, os, es, des, ps)) <$> Dec.decodeString
        "extends" -> (\v -> (n, v, ss, os, es, des, ps)) <$> decodeListOf Dec.decodeString
        "sorts" -> (\v -> (n, ex, v, os, es, des, ps)) <$> decodeListOf decodeSort
        "ops" -> (\v -> (n, ex, ss, v, es, des, ps)) <$> decodeListOf decodeOperation
        "eqs" -> (\v -> (n, ex, ss, os, v, des, ps)) <$> decodeListOf decodeEquation
        "directed_eqs" -> (\v -> (n, ex, ss, os, es, v, ps)) <$> decodeListOf valueDecoder
        "policies" -> (\v -> (n, ex, ss, os, es, des, v)) <$> decodeListOf valueDecoder
        _ -> skipTerm >> pure acc

morphismDecoder :: Decoder s TheoryMorphism
morphismDecoder = decodeMapWith (emptyMorphism T.empty T.empty T.empty) onKey
  where
    onKey acc key = case key of
        "name" -> (\v -> acc {morphismName = v}) <$> Dec.decodeString
        "domain" -> (\v -> acc {domain = v}) <$> Dec.decodeString
        "codomain" -> (\v -> acc {codomain = v}) <$> Dec.decodeString
        "sort_map" -> (\v -> acc {sortMap = v}) <$> decodeTextMap Dec.decodeString
        "op_map" -> (\v -> acc {opMap = v}) <$> decodeTextMap Dec.decodeString
        _ -> skipTerm >> pure acc

morphismCheckResultDecoder :: Decoder s MorphismCheckResult
morphismCheckResultDecoder = decodeFields (False, Nothing) build handler
  where
    build (v, e) = MorphismCheckResult v e
    handler acc@(v, e) key = case key of
        "valid" -> (\x -> (x, e)) <$> Dec.decodeBool
        "error" -> (\x -> (v, x)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

typecheckResultDecoder :: Decoder s TypecheckResult
typecheckResultDecoder = decodeFields (False, Nothing, Nothing) build handler
  where
    build (wf, os, e) = TypecheckResult wf os e
    handler acc@(wf, os, e) key = case key of
        "well_formed" -> (\x -> (x, os, e)) <$> Dec.decodeBool
        "output_sort" -> (\x -> (wf, x, e)) <$> decodeMaybeText
        "error" -> (\x -> (wf, os, x)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

sortInterpDecoder :: Decoder s (HashMap Text [ModelValue])
sortInterpDecoder = decodeTextMap (decodeListOf decodeModelValue)

-- The struct decoders below build positionally rather than via record
-- update because several share field names ('name', 'args', 'output',
-- etc.) across datatypes; threading a tuple accumulator and applying the
-- constructor at the end sidesteps any ambiguity while tolerating field
-- reordering and unknown fields.

decodeSort :: Decoder s Sort
decodeSort = decodeFields (T.empty, [], Structural, Open) build handler
  where
    build (n, ps, k, c) = Sort n ps k c
    handler acc@(n, ps, k, c) key = case key of
        "name" -> (\v -> (v, ps, k, c)) <$> Dec.decodeString
        "params" -> (\v -> (n, v, k, c)) <$> decodeListOf decodeSortParam
        "kind" -> (\v -> (n, ps, v, c)) <$> decodeSortKind
        "closure" -> (\v -> (n, ps, k, v)) <$> decodeSortClosure
        _ -> skipTerm >> pure acc

decodeSortParam :: Decoder s SortParam
decodeSortParam = decodeFields (T.empty, SortName T.empty) build handler
  where
    build (n, s) = SortParam n s
    handler acc@(n, s) key = case key of
        "name" -> (\v -> (v, s)) <$> Dec.decodeString
        "sort" -> (\v -> (n, v)) <$> decodeSortExpr
        _ -> skipTerm >> pure acc

-- | Decode a @#[serde(untagged)]@ 'SortExpr': a bare string is a
-- 'SortName', a map is a @{ name, args }@ 'SortApp'. An @App@ with an
-- empty argument list decodes to 'SortName', matching the Rust
-- normalization invariant.
decodeSortExpr :: Decoder s SortExpr
decodeSortExpr = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> SortName <$> Dec.decodeString
        _ -> decodeFields (T.empty, []) build handler
  where
    build (n, as) = if null as then SortName n else SortApp n as
    handler acc@(n, as) key = case key of
        "name" -> (\v -> (v, as)) <$> Dec.decodeString
        "args" -> (\v -> (n, v)) <$> decodeListOf decodeTerm
        _ -> skipTerm >> pure acc

decodeSortKind :: Decoder s SortKind
decodeSortKind = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            s <- Dec.decodeString
            case s of
                "Structural" -> pure Structural
                other -> fail ("decodeSortKind: unknown unit variant " <> T.unpack other)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            k <- Dec.decodeString
            case k of
                "Val" -> Val <$> decodeValueKind
                "Merger" -> Merger <$> decodeValueKind
                "Coercion" -> decodeCoercion
                other -> fail ("decodeSortKind: unknown variant " <> T.unpack other)

decodeCoercion :: Decoder s SortKind
decodeCoercion = decodeFields (Any, Any, Iso) build handler
  where
    build (f, tk, c) = Coercion f tk c
    handler acc@(f, tk, c) key = case key of
        "from" -> (\v -> (v, tk, c)) <$> decodeValueKind
        "to" -> (\v -> (f, v, c)) <$> decodeValueKind
        "class" -> (\v -> (f, tk, v)) <$> decodeCoercionClass
        _ -> skipTerm >> pure acc

decodeSortClosure :: Decoder s SortClosure
decodeSortClosure = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            s <- Dec.decodeString
            case s of
                "Open" -> pure Open
                other -> fail ("decodeSortClosure: unknown unit variant " <> T.unpack other)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            k <- Dec.decodeString
            case k of
                "Closed" -> Closed <$> decodeListOf Dec.decodeString
                other -> fail ("decodeSortClosure: unknown variant " <> T.unpack other)

decodeValueKind :: Decoder s ValueKind
decodeValueKind = do
    s <- Dec.decodeString
    case lookup s valueKindTags of
        Just vk -> pure vk
        Nothing -> fail ("decodeValueKind: unknown value kind " <> T.unpack s)

decodeCoercionClass :: Decoder s CoercionClass
decodeCoercionClass = do
    s <- Dec.decodeString
    case lookup s coercionClassTags of
        Just c -> pure c
        Nothing -> fail ("decodeCoercionClass: unknown coercion class " <> T.unpack s)

decodeOperation :: Decoder s Operation
decodeOperation = decodeFields (T.empty, [], SortName T.empty) build handler
  where
    build (n, is, o) = Operation n is o
    handler acc@(n, is, o) key = case key of
        "name" -> (\v -> (v, is, o)) <$> Dec.decodeString
        "inputs" -> (\v -> (n, v, o)) <$> decodeListOf decodeInput
        "output" -> (\v -> (n, is, v)) <$> decodeSortExpr
        _ -> skipTerm >> pure acc

decodeInput :: Decoder s (Text, SortExpr, Implicit)
decodeInput = do
    _ <- Dec.decodeListLenOrIndef
    pn <- Dec.decodeString
    se <- decodeSortExpr
    imp <- decodeImplicit
    pure (pn, se, imp)

decodeImplicit :: Decoder s Implicit
decodeImplicit = do
    s <- Dec.decodeString
    case s of
        "No" -> pure ExplicitParam
        "Yes" -> pure ImplicitParam
        other -> fail ("decodeImplicit: unknown variant " <> T.unpack other)

decodeEquation :: Decoder s Equation
decodeEquation = decodeFields (T.empty, Var T.empty, Var T.empty) build handler
  where
    build (n, l, r) = Equation n l r
    handler acc@(n, l, r) key = case key of
        "name" -> (\v -> (v, l, r)) <$> Dec.decodeString
        "lhs" -> (\v -> (n, v, r)) <$> decodeTerm
        "rhs" -> (\v -> (n, l, v)) <$> decodeTerm
        _ -> skipTerm >> pure acc

-- | Decode an externally-tagged 'Term': a single-key map dispatched on
-- the variant tag.
decodeTerm :: Decoder s Term
decodeTerm = do
    _ <- Dec.decodeMapLenOrIndef
    k <- Dec.decodeString
    case k of
        "Var" -> Var <$> Dec.decodeString
        "App" -> decodeApp
        "Case" -> decodeCase
        "Hole" -> decodeHole
        "Let" -> decodeLet
        other -> fail ("decodeTerm: unknown variant " <> T.unpack other)

decodeApp :: Decoder s Term
decodeApp = decodeFields (T.empty, []) build handler
  where
    build (o, as) = App o as
    handler acc@(o, as) key = case key of
        "op" -> (\v -> (v, as)) <$> Dec.decodeString
        "args" -> (\v -> (o, v)) <$> decodeListOf decodeTerm
        _ -> skipTerm >> pure acc

decodeCase :: Decoder s Term
decodeCase = decodeFields (Var T.empty, []) build handler
  where
    build (sc, bs) = Case sc bs
    handler acc@(sc, bs) key = case key of
        "scrutinee" -> (\v -> (v, bs)) <$> decodeTerm
        "branches" -> (\v -> (sc, v)) <$> decodeListOf decodeCaseBranch
        _ -> skipTerm >> pure acc

decodeHole :: Decoder s Term
decodeHole = decodeFields Nothing build handler
  where
    build mn = Hole mn
    handler acc key = case key of
        "name" -> decodeMaybeText
        _ -> skipTerm >> pure acc

decodeLet :: Decoder s Term
decodeLet = decodeFields (T.empty, Var T.empty, Var T.empty) build handler
  where
    build (n, b, bd) = Let n b bd
    handler acc@(n, b, bd) key = case key of
        "name" -> (\v -> (v, b, bd)) <$> Dec.decodeString
        "bound" -> (\v -> (n, v, bd)) <$> decodeTerm
        "body" -> (\v -> (n, b, v)) <$> decodeTerm
        _ -> skipTerm >> pure acc

decodeCaseBranch :: Decoder s CaseBranch
decodeCaseBranch = decodeFields (T.empty, [], Var T.empty) build handler
  where
    build (c, bs, bd) = CaseBranch c bs bd
    handler acc@(c, bs, bd) key = case key of
        "constructor" -> (\v -> (v, bs, bd)) <$> Dec.decodeString
        "binders" -> (\v -> (c, v, bd)) <$> decodeListOf Dec.decodeString
        "body" -> (\v -> (c, bs, v)) <$> decodeTerm
        _ -> skipTerm >> pure acc

-- | Decode an externally-tagged 'ModelValue': a bare string for
-- 'MVNull', a single-key map for the rest.
decodeModelValue :: Decoder s ModelValue
decodeModelValue = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeString -> do
            s <- Dec.decodeString
            case s of
                "Null" -> pure MVNull
                other -> fail ("decodeModelValue: unknown unit variant " <> T.unpack other)
        _ -> do
            _ <- Dec.decodeMapLenOrIndef
            k <- Dec.decodeString
            case k of
                "Str" -> MVStr <$> Dec.decodeString
                "Int" -> MVInt <$> Dec.decodeInt64
                "Bool" -> MVBool <$> Dec.decodeBool
                "List" -> MVList <$> decodeListOf decodeModelValue
                "Map" -> MVMap <$> decodeTextMap decodeModelValue
                "Null" -> pure MVNull
                other -> fail ("decodeModelValue: unknown variant " <> T.unpack other)

-- | Decode a CBOR map, threading a tuple accumulator through an entry
-- handler and applying a constructor at the end.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = build <$> decodeMapWith initial onKey

-- | Fold over a CBOR map's entries (definite or indefinite length),
-- dispatching each key through the handler.
decodeMapWith :: acc -> (acc -> Text -> Decoder s acc) -> Decoder s acc
decodeMapWith initial onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = do
        k <- Dec.decodeString
        acc' <- onKey acc k
        goN (n - 1 :: Int) acc'
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else do
                k <- Dec.decodeString
                acc' <- onKey acc k
                goIndef acc'

-- | Decode a CBOR map with text keys into a 'HashMap'.
decodeTextMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeTextMap decV = HM.fromList <$> decodeMapPairs Dec.decodeString decV

-- | Decode a CBOR map's key/value pairs (definite or indefinite) into an
-- association list.
decodeMapPairs :: Decoder s k -> Decoder s v -> Decoder s [(k, v)]
decodeMapPairs decK decV = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = do
        k <- decK
        v <- decV
        ((k, v) :) <$> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                k <- decK
                v <- decV
                ((k, v) :) <$> goIndef

-- | Decode a CBOR list (definite or indefinite).
decodeListOf :: Decoder s a -> Decoder s [a]
decodeListOf decA = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = (:) <$> decA <*> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> decA <*> goIndef

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> Dec.decodeString

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown or index-cache fields.
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
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> Dec.decodeStringIndef >> skipUntilBreakStrings
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> Dec.decodeBytesIndef >> skipUntilBreakBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        _ -> fail "decodeTheory: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs
    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes
    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings

-- ---------------------------------------------------------------------------
-- Tag tables

-- | The @serde@ string tag for each 'ValueKind', matching the Rust
-- @ValueKind@ variant names (the @as_str@ method returns schema-level
-- kind names, but @serde@ uses the variant identifiers).
valueKindTag :: ValueKind -> Text
valueKindTag = \case
    Bool -> "Bool"
    Int -> "Int"
    Float -> "Float"
    Str -> "Str"
    Bytes -> "Bytes"
    Token -> "Token"
    Null -> "Null"
    Any -> "Any"

valueKindTags :: [(Text, ValueKind)]
valueKindTags = [(valueKindTag k, k) | k <- [minBound .. maxBound]]

coercionClassTag :: CoercionClass -> Text
coercionClassTag = \case
    Iso -> "Iso"
    Retraction -> "Retraction"
    Projection -> "Projection"
    Opaque -> "Opaque"

coercionClassTags :: [(Text, CoercionClass)]
coercionClassTags = [(coercionClassTag c, c) | c <- [minBound .. maxBound]]
