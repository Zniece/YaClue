# Errors, Diagnostics, and Source Locations
> **Scope:** This page distinguishes Rust core commands, standard scripts, and historical compatibility entries. A discoverable name does not imply that every argument boundary has been validated. See the [availability audit](availability.md).

## Error reporting

This chapter contains commands useful for reporting errors to the user.

{Check}, {TrapError}, and {GetCoreError} are Rust core commands. The
error-tableau functions in the following sections are supplied by
`io.rep/errors.ys` and therefore depend on that script package being loaded.

### Check(predicate,"error text")

report "hard" errors

### TrapError(expression,errorHandler)

trap "hard" errors

### GetCoreError()

get "hard" error string

{predicate} -- expression returning `True` or `False`
{"error text"} -- string to print on error
{expression} -- expression to evaluate (causing potential error)
{errorHandler} -- expression to be called to handle error

If {predicate} does not evaluate to `True`, evaluation stops and the Rust
engine returns a structured {YacasError::Generic} error containing
{"error text"} to its host. A console host may print it; embedded callers
receive it through the Rust result value. This facility guards conditions
that must hold during evaluation.

A "soft" error reporting facility that does not stop the execution
is provided by the function {Assert}.

For example, `[Check(1=0,"bad value"); Echo(OK);]` stops at {Check};
{Echo(OK)} is not evaluated.

TrapError evaluates its argument {expression}, returning the result
of evaluating {expression}. If an error occurs, {errorHandler} is
evaluated, returning its return value in stead.

GetCoreError returns a string describing the core error.  TrapError
and GetCoreError can be used in combination to write a custom error
handler.

> **See also:** [Assert](errors.md#assertpred-str-expr)


### Assert(pred, str, expr)

           Assert(pred, str) pred
           Assert(pred)

signal "soft" custom error

Precedence:
EVAL OpPrecedence("Assert")

{pred} -- predicate to check
{"str"} -- string to classify the error
{expr} -- expression, error object

{Assert} is a global error reporting mechanism. It can be used to
check for errors and report them. An error is considered to occur
when the predicate {pred} evaluates to anything except `True`. In
this case, the function returns `False` and an error object is
created and posted to the global error tableau.  Otherwise the
function returns `True`.

Unlike the "hard" error function {Check}, the function {Assert}
does not stop the execution of the program.

The error object consists of the string {"str"} and an arbitrary
expression {expr}. The string should be used to classify the kind
of error that has occurred, for example "domain" or "format". The
error object can be any expression that might be useful for
handling the error later; for example, a list of erroneous values
and explanations.  The association list of error objects is
currently obtainable through the function {GetErrorTableau()}.

If the parameter {expr} is missing, {Assert} substitutes `True`. If
both optional parameters {"str"} and {expr} are missing, {Assert}
creates an error of class {"generic"}.

Errors can be handled by a custom error handler in the portion of
the code that is able to handle a certain class of errors. The
functions {IsError}, {GetError} and {ClearError} can be used.

The error tableau is script-level state. A caller must inspect, clear, or
render it explicitly with the functions below; embedding the Rust engine
does not imply a particular terminal or screen.

**Example:**

```
In> Assert("bad value", "must be zero") 1=0
Out> False;
In> Assert("bad value", "must be one") 1=1
Out> True;
In> IsError()
Out> True;
In> IsError("bad value")
Out> True;
In> IsError("bad file")
Out> False;
In> GetError("bad value");
Out> "must be zero";
In> DumpErrors()
Error: bad value: must be zero
Out> True;

```

No more errors left:

  In> IsError()
  Out> False;
  In> DumpErrors()
  Out> True;

> **See also:** [IsError](errors.md#iserror), [DumpErrors](errors.md#dumperrors), [Check](errors.md#checkpredicateerror-text),

             [GetError](errors.md#geterrorstr), [ClearError](errors.md#clearerrorstr),
             [ClearErrors](errors.md#clearerrors), [GetErrorTableau](errors.md#geterrortableau)

### DumpErrors()

simple error handlers

### ClearErrors()

simple error handlers

{DumpErrors} is a simple error handler for the global error
reporting mechanism. It prints all errors posted using {Assert} and
clears the error tableau.

{ClearErrors} is a trivial error handler that does nothing except
it clears the tableau.

> **See also:** [Assert](errors.md#assertpred-str-expr), [IsError](errors.md#iserror)


### IsError()

           IsError(str)

check for custom error

{"str"} -- string to classify the error

{IsError()} returns `True` if any custom errors have been reported
using {Assert}.  The second form takes a parameter {"str"} that
designates the class of the error we are interested in. It returns
`True` if any errors of the given class {"str"} have been reported.

> **See also:** [GetError](errors.md#geterrorstr), [ClearError](errors.md#clearerrorstr), [Assert](errors.md#assertpred-str-expr),

             [Check](errors.md#checkpredicateerror-text)

### GetError(str)

> **Current status:** `GetError` is supplied by `io.rep/errors.ys` but has no independent `.def` entry. Load its package through a public I/O error entry; do not treat it as an independently stable entry point.

custom errors handlers

### ClearError(str)

custom errors handlers

### GetErrorTableau()

custom errors handlers

{"str"} -- string to classify the error

These functions can be used to create a custom error handler.

{GetError} returns the error object if a custom error of class
{"str"} has been reported using {Assert}, or `False` if no errors
of this class have been reported.

{ClearError("str")} deletes the same error object that is returned
by {GetError("str")}. It deletes at most one error object. It
returns `True` if an object was found and deleted, and `False`
otherwise.

{GetErrorTableau()} returns the entire association list of
currently reported errors.

**Example:**

```
In> x:=1
Out> 1;
In> Assert("bad value", {x,"must be zero"}) x=0
Out> False;
In> GetError("bad value")
Out> {1, "must be zero"};
In> ClearError("bad value");
Out> True;
In> IsError()
Out> False;

```

> **See also:** [IsError](errors.md#iserror), [Assert](errors.md#assertpred-str-expr), [Check](errors.md#checkpredicateerror-text),

             [ClearErrors](errors.md#clearerrors)

### CurrentFile()

return current input file

### CurrentLine()

return current line number on input

The functions {CurrentFile} and {CurrentLine} return a string
with the file name of the current file and the current line
of input respectively.

These functions are most useful in batch file calculations, where
there is a need to determine at which line an error occurred.
One can define a function:

  tst() := Echo({CurrentFile(),CurrentLine()});

which can then be inserted into the input file at various places,
to see how far the interpreter reaches before an error occurs.

> **See also:** [Echo](io.md#echoitem)
