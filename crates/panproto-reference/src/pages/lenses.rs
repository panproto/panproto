//! Complete reference operations assigned to `lenses`.

// Structural alignment of a tree view read from a serialization.
mod alignment;
// The sort coercion protolens's value layer.
mod coercion;
// Fused chains, scoped protolenses and sequential composites.
mod composite;
mod constructions;
// The elementary protolens kinds of the lens DSL's step alphabet.
mod elementary;
// Shared put-domain machinery and graph restoration.
mod domain;
mod graph;
// The stable public graph-lens facade preserves every existing item path.
pub mod lens;
// Path composition and stable free-function operations.
mod laws;
pub mod path;
// Representation bridges and observables.
mod protolens;
mod squares;
// Table planning, complements, get, and put.
pub mod table;
mod tree;

pub use self::alignment::{
    ViewAlignmentRefusal, ViewPosition, align_tree_view, tree_view_positions,
};
pub use self::constructions::{
    ConstructionFeature, ConstructionObstruction, ConstructionShape, construction_obstruction,
    span_has_shared_middle,
};
pub use self::laws::{
    BasisCase, CongruenceCase, Divergence, DivergenceKind, DomainExpectation, EqualityClass,
    FusionCheck, FusionObservation, GateReport, GraphShape, LawFailure, LawGate, LawGateStatus,
    LawObstruction, LawOperation, LawReport, LawShape, LensShape, LocalityReport, Mutation,
    OutcomeClass, PutGetMode, ReferenceLawError, SynthIndex, TableLawError, TableShape,
    TreeLawComplement, TreeShape, check_graph_laws, check_laws_report, check_locality_report,
    check_table_laws, check_tree_laws, compare_fusion,
};
pub use self::lens::{
    ComplementAtom, ComplementComposeRefusal, ComplementModel, GraphLens, GraphSquareObservation,
    IdentityError, IdentityStatus, InstantiatedLens, LensError, Protolens, ProtolensApplication,
    ProtolensError, SchemaPrecondition, SquareError, SquareResult, SquareRouteObservation,
    TableSquareObservation, TreeOperationalError, compose_complement, graph_lens_identity,
    graph_put_square, graph_square, protolens_instance, protolens_schema, put, table_put_square,
    table_square, tree_check_put_domain, tree_get, tree_put,
};
pub use self::path::{
    FusionCandidate, FusionCondition, FusionDecision, FusionInputs, FusionNormalizationError,
    FusionPredicate, FusionSemantics, FusionWitness, LensPath, LocatedFusionObstruction,
    NormalForm, NormalGroup, NormalStageError, PathComplement, PathComplementEntry,
    PathConstructionError, PathError, PathIdentityField, PathStage, PathStageError, Premise,
    SchemaEndpoint, StageIndex, StagePut, TableFusionObstruction, TableFusionPremises,
    classify_table_fusion, declared_graph_source, get_normal_form, get_path, put_normal_form,
    put_path,
};
pub use self::table::{TableLens, get};
