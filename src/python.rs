//! PyO3 Python bindings for Rable.
//!
//! Exposes a `parse()` function, `ParsedNode` wrapper, and error types
//! that are a drop-in replacement for Parable's Python API.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use crate::error::RableError;

// -- Exception types --

pyo3::create_exception!(
    rable,
    ParseError,
    PyException,
    "Syntax error during parsing."
);
pyo3::create_exception!(
    rable,
    MatchedPairError,
    PyException,
    "Unmatched delimiter (parenthesis, brace, bracket, quote)."
);

// -- Parsed node wrapper --

/// A parsed AST node with S-expression output.
#[pyclass(name = "ParsedNode")]
struct ParsedNode {
    sexp: String,
}

#[pymethods]
impl ParsedNode {
    /// Returns the S-expression representation of this node.
    fn to_sexp(&self) -> &str {
        &self.sexp
    }

    fn __repr__(&self) -> String {
        format!("ParsedNode({})", self.sexp)
    }

    fn __str__(&self) -> &str {
        self.to_sexp()
    }
}

// -- Module functions --

/// Parse a bash source string into a list of AST nodes.
///
/// Args:
///     source: The bash source code to parse.
///     extglob: Whether to enable extended glob patterns (default: False).
///
/// Returns:
///     A list of `ParsedNode` objects.
///
/// Raises:
///     ParseError: If the source contains syntax errors.
///     MatchedPairError: If the source has unmatched delimiters.
#[pyfunction]
#[pyo3(signature = (source, extglob = false))]
fn parse(source: &str, extglob: bool) -> PyResult<Vec<ParsedNode>> {
    let nodes = crate::parse(source, extglob).map_err(|e| match e {
        RableError::Parse { .. } => PyErr::new::<ParseError, _>(e.to_string()),
        RableError::MatchedPair { .. } => PyErr::new::<MatchedPairError, _>(e.to_string()),
    })?;

    Ok(nodes
        .into_iter()
        .map(|node| ParsedNode {
            sexp: node.to_string(),
        })
        .collect())
}

/// The rable Python module — a drop-in replacement for Parable.
#[pymodule]
fn rable(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_class::<ParsedNode>()?;
    m.add("ParseError", m.py().get_type::<ParseError>())?;
    m.add("MatchedPairError", m.py().get_type::<MatchedPairError>())?;
    Ok(())
}
