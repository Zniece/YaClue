//! Error taxonomy. The variants correspond to the `KLispErr*` constants of
//! the upstream engine; see `cyacas/libyacas/src/errors.cpp` for the message
//! texts.
//!
//! Errors propagate through `Result` throughout the engine (contract §3).

/// Evaluation/command-layer errors.
#[derive(Debug)]
pub enum YacasError {
    /// Unknown symbol (internal interpreter error; same family as the
    /// parser's `TokenError`, reclassified in evaluation context).
    None,
    /// Division by zero.
    DivideByZero,
    /// A numeric result would exceed the engine's bounded exact-work limit.
    NumericOverflow,
    /// Invalid argument (the common check in most commands).
    InvalidArg,
    /// Not an integer (`Mod`, bit operations, etc. require integral operands).
    NotInteger,
    /// Not a string.
    NotString,
    /// Not a list.
    NotList,
    /// Argument count mismatch.
    WrongNumberOfArgs,
    /// List not long enough (`Nth` / iteration past the end of the chain).
    ListNotLongEnough,
    /// Invalid stack (`GetVariable` etc. with no frame).
    InvalidStack,
    /// Symbol is protected (assignment/overwrite forbidden).
    SymbolProtected,
    /// Security check failed (`Secure` mode).
    SecurityBreach,
    /// Failed to create a user function (a formal parameter is not an atom).
    CreatingUserFunction,
    /// Failed to create a rule.
    CreatingRule,
    /// Arity already taken (redefining `RuleBase` with same name/arity).
    ArityAlreadyDefined,
    /// Not a boolean predicate (a pattern predicate evaluated to neither
    /// `True` nor `False`).
    NonBooleanPredicateInPattern,
    /// Maximum recursion depth reached.
    MaxRecurseDepthReached,
    /// User interrupt (depth +20 without convergence).
    UserInterrupt,
    /// File not found (loading family).
    FileNotFound,
    /// Failed to read file.
    ReadingFile,
    /// Library not found (`Use`).
    LibraryNotFound,
    /// `.def` file already bound (`DefLoad` conflict).
    DefFileAlreadyChosen,
    /// Not an operator (table lookup failed).
    NotAnInfixOperator,
    /// Not an infix/prefix/postfix/bodied operator (`IsInfix` family).
    IsNotInFix,
    /// Unprintable token (printer met a `Generic` without a `TypeName`).
    UnprintableToken,
    /// Generic format error.
    GenericFormat,
    /// Generic error with custom text.
    Generic(String),
    /// Parse error (catch-all; see `parser::ParseError`).
    Parse(crate::parser::ParseError),
}

impl YacasError {
    pub fn generic<T: Into<String>>(msg: T) -> Self {
        YacasError::Generic(msg.into())
    }
}

impl From<crate::parser::ParseError> for YacasError {
    fn from(e: crate::parser::ParseError) -> Self {
        YacasError::Parse(e)
    }
}
