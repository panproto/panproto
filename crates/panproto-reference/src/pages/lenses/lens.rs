//! The graph lens: get is restrict with its complement; put restores a
//! source from a view and a complement on the put domain.

pub use super::domain::{PutDomainError, ViewRef};
pub use super::graph::{
    GraphLens, IdentityError, IdentityStatus, LensError, graph_lens_identity, present,
};
pub use super::laws::{
    BasisCase, CongruenceCase, Divergence, DivergenceKind, DomainExpectation, EqualityClass,
    FusionCheck, FusionObservation, GateReport, GraphShape, LawFailure, LawGate, LawGateStatus,
    LawObstruction, LawOperation, LawReport, LawShape, LensShape, LocalityReport, Mutation,
    OutcomeClass, PutGetMode, ReferenceLawError, SynthIndex, TableShape, TreeLawComplement,
    TreeShape, check_graph_laws, check_laws_report, check_locality_report, check_tree_laws,
    compare_fusion, put,
};
pub use super::protolens::{
    InstantiatedLens, Protolens, ProtolensApplication, ProtolensError, SchemaPrecondition, get,
    protolens_instance, protolens_schema,
};
pub use super::squares::{
    GraphSquareObservation, SquareError, SquareResult, SquareRouteObservation,
    TableSquareObservation, graph_put_square, graph_square, table_put_square, table_square,
};
pub use super::tree::{TreeOperationalError, tree_check_put_domain, tree_get, tree_put};
