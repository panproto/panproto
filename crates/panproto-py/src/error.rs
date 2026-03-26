//! Python exception hierarchy for panproto.
//!
//! Maps each panproto sub-crate's error type to a Python exception class.
//! The hierarchy mirrors the existing Python SDK's exception hierarchy so
//! that `except panproto.SchemaValidationError` continues to work.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

// -- Exception hierarchy --

create_exception!(
    panproto._native,
    PanprotoError,
    PyException,
    "Base exception for all panproto errors."
);
create_exception!(
    panproto._native,
    SchemaValidationError,
    PanprotoError,
    "Schema construction or validation failed."
);
create_exception!(
    panproto._native,
    MigrationError,
    PanprotoError,
    "Migration compilation or application failed."
);
create_exception!(
    panproto._native,
    ExistenceCheckError,
    PanprotoError,
    "Existence checking found undefined references."
);
create_exception!(
    panproto._native,
    LensError,
    PanprotoError,
    "Lens construction, get/put, or law check failed."
);
create_exception!(
    panproto._native,
    VcsError,
    PanprotoError,
    "Version control operation failed."
);
create_exception!(
    panproto._native,
    IoError,
    PanprotoError,
    "Instance parse or emit failed."
);
create_exception!(
    panproto._native,
    ExprError,
    PanprotoError,
    "Expression parse or evaluation failed."
);
create_exception!(
    panproto._native,
    GatError,
    PanprotoError,
    "GAT theory operation failed."
);
create_exception!(
    panproto._native,
    CheckError,
    PanprotoError,
    "Diff or compatibility classification failed."
);
create_exception!(
    panproto._native,
    ParseError,
    PanprotoError,
    "Full-AST parsing or emission failed."
);
create_exception!(
    panproto._native,
    ProjectError,
    PanprotoError,
    "Project assembly failed."
);
create_exception!(
    panproto._native,
    GitBridgeError,
    PanprotoError,
    "Git bridge operation failed."
);

// -- Conversion helpers --
//
// We cannot implement `From<ForeignError> for PyErr` due to the orphan
// rule. Instead, provide helper functions and a trait.

use panproto_core::schema::SchemaError;

/// Extension trait to convert panproto errors into Python exceptions.
pub trait IntoPyErr {
    /// Convert this error into a `PyErr`.
    fn into_py_err(self) -> PyErr;
}

impl IntoPyErr for SchemaError {
    fn into_py_err(self) -> PyErr {
        SchemaValidationError::new_err(self.to_string())
    }
}

/// Extension trait on `Result` for ergonomic `?` usage.
pub trait MapPyErr<T> {
    /// Map the error to a `PyErr` using `IntoPyErr`.
    fn map_py_err(self) -> PyResult<T>;
}

impl<T, E: IntoPyErr> MapPyErr<T> for Result<T, E> {
    fn map_py_err(self) -> PyResult<T> {
        self.map_err(IntoPyErr::into_py_err)
    }
}

/// Register all exception classes on the given module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add("PanprotoError", parent.py().get_type::<PanprotoError>())?;
    parent.add(
        "SchemaValidationError",
        parent.py().get_type::<SchemaValidationError>(),
    )?;
    parent.add("MigrationError", parent.py().get_type::<MigrationError>())?;
    parent.add(
        "ExistenceCheckError",
        parent.py().get_type::<ExistenceCheckError>(),
    )?;
    parent.add("LensError", parent.py().get_type::<LensError>())?;
    parent.add("VcsError", parent.py().get_type::<VcsError>())?;
    parent.add("IoError", parent.py().get_type::<IoError>())?;
    parent.add("ExprError", parent.py().get_type::<ExprError>())?;
    parent.add("GatError", parent.py().get_type::<GatError>())?;
    parent.add("CheckError", parent.py().get_type::<CheckError>())?;
    parent.add("ParseError", parent.py().get_type::<ParseError>())?;
    parent.add("ProjectError", parent.py().get_type::<ProjectError>())?;
    parent.add("GitBridgeError", parent.py().get_type::<GitBridgeError>())?;
    Ok(())
}
