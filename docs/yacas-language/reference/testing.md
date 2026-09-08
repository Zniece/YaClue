# Yacas Test Tools
> **Scope:** This page distinguishes Rust core commands, standard scripts, and historical compatibility entries. A discoverable name does not imply that every argument boundary has been validated. See the [availability audit](availability.md).

## The Yacas test suite

This chapter describes commands used to verify Yacas expressions. The standard
script tests are in `yacas/tests/`; `.yts` is the test-script extension.
Run the Rust engine and script conformance tests from `prj` with:

```bash
cargo test -p yacas-rs
```

The project uses three complementary levels of verification:

- Rust unit and integration tests cover parser, evaluator, numeric, and core
  command behavior.
- `.yts` files cover standard-script behavior through the initialized engine.
- `processing` tests cover product APIs and step output; its reviewed golden
  snapshots belong to the processing layer rather than the language standard.

A defect should remain on the project todo list only while it has a stable
reproducer. {KnownFailure} remains available for inherited script suites, but
new regressions should normally receive an executable test with an explicit
expected result.

The verification commands described in this chapter only  display the
expressions that do not evaluate correctly. Errors do not terminate the
execution of the Yacas script that uses these testing commands, since they are
meant to be used in test scripts.

### Verify(question,answer)

verifying equivalence of two expressions

### TestYacas(question,answer)

verifying equivalence of two expressions

### LogicVerify(question,answer)

verifying equivalence of two expressions

### LogicTest(variables,expr1,expr2)

verifying equivalence of two expressions

{question} -- expression to check for
{answer} -- expected result after evaluation
{variables} -- list of variables
{exprN} -- Some boolean expression

The commands {Verify}, {TestYacas}, {LogicVerify} and {LogicTest}
can be used to verify that an expression is <I>equivalent</I> to a
correct answer after evaluation. All three commands return `True`
or `False`.

For some calculations, the demand that two expressions are
*identical* syntactically is too stringent. The Yacas system
might change at various places in the future, but $ 1+x$ would
still be equivalent, from a mathematical point of view, to $x+1$.

The general problem of deciding that two expressions $a$ and
$b$ are equivalent, which is the same as saying that $a-b=0$,
is generally hard to decide on. The following commands solve this
problem by having domain-specific comparisons.

The comparison commands do the following comparison types:

* [Verify](testing.md#verifyquestionanswer) -- verify for literal equality.
  This is the fastest and simplest comparison, and can be
  used, for example, to test that an expression evaluates to $2$.
* [TestYacas](testing.md#testyacasquestionanswer) -- compare two expressions after simplification as
  multivariate polynomials. If the two arguments are equivalent
  multivariate polynomials, this test succeeds. [TestYacas](testing.md#testyacasquestionanswer) uses
  [Simplify](simplify.md#simplifyexpr). Note: [TestYacas](testing.md#testyacasquestionanswer) currently should not be used to
  test equality of lists.
* [LogicVerify](testing.md#logicverifyquestionanswer) -- Perform a test by using [CanProve](logic.md#canproveproposition) to verify
  that from `question` the expression `answer` follows. This test
  command is used for testing the logic theorem prover in Yacas.
* [LogicTest](testing.md#logictestvariablesexpr1expr2) -- Generate a truth table for the two expressions and
  compare these two tables. They should be the same if the two
  expressions are logically the same.

**Example:**

```
In> Verify(1+2,3)
Out> True;
In> Verify(x*(1+x),x^2+x)
******************
x*(x+1) evaluates to x*(x+1) which differs
  from x^2+x
******************
Out> False;
In> TestYacas(x*(1+x),x^2+x)
Out> True;
In> Verify(a And c Or b And Not c,a Or b)
******************
 a And c Or b And Not c evaluates to  a And c
  Or b And Not c which differs from  a Or b
******************
Out> False;
In> LogicVerify(a And c Or b And Not c,a Or b)
Out> True;
In> LogicVerify(a And c Or b And Not c,b Or a)
Out> True;
In> LogicTest({A,B,C},Not((Not A) And (Not B)),A Or B)
Out> True
In> LogicTest({A,B,C},Not((Not A) And (Not B)),A Or C)
******************
CommandLine: 1

TrueFalse4({A,B,C},Not(Not A And Not B))
 evaluates to
{{{False,False},{True,True}},{{True,True},{True,True}}}
 which differs from
{{{False,True},{False,True}},{{True,True},{True,True}}}
******************
Out> False

```

> **See also:** [Simplify](simplify.md#simplifyexpr), [CanProve](logic.md#canproveproposition),

             [KnownFailure](testing.md#knownfailuretest)

### KnownFailure(test)

Mark a test as a known failure

{test} -- expression that should return `False` on failure

The command {KnownFailure} marks a test as known to fail and emits a
diagnostic through the active output sink.

This may be used to record a deliberately acknowledged failure while keeping
the remaining `.yts` file runnable. Current project policy prefers a tracked,
reproducible regression test over silently accumulating known failures.

**Example:**

```
In> KnownFailure(Verify(1,2))
Known failure:
******************
1 evaluates to  1 which differs from  2
******************
Out> False;
In> KnownFailure(Verify(1,1))
Known failure:
Failure resolved!
Out> True;

```

> **See also:** [Verify](testing.md#verifyquestionanswer), [TestYacas](testing.md#testyacasquestionanswer), [LogicVerify](testing.md#logicverifyquestionanswer)


### RoundTo(number,precision)

Round a real-valued result to a set number of digits

{number} -- number to round off
{precision} -- precision to use for round-off

The function {RoundTo} rounds a floating point number to a
specified precision, allowing for testing for correctness using the
{Verify} command.

**Example:**

```
In> N(RoundTo(Exp(1),30),30)
Out> 2.71828182110230114951959786552;
In> N(RoundTo(Exp(1),20),20)
Out> 2.71828182796964237096;

```

> **See also:** [Verify](testing.md#verifyquestionanswer), [VerifyArithmetic](testing.md#verifyarithmeticxnm), [VerifyDiv](testing.md#verifydivuv)


### VerifyArithmetic(x,n,m)

Special purpose arithmetic verifiers

### RandVerifyArithmetic(n)

Special purpose arithmetic verifiers

### VerifyDiv(u,v)

Special purpose arithmetic verifiers

{x}, {n}, {m}, {u}, {v} -- integer arguments

The commands {VerifyArithmetic} and {VerifyDiv} test a mathematic
equality which should hold, testing that the result returned by the
system is mathematically correct according to a mathematically
provable theorem.

{VerifyArithmetic} verifies for an arbitrary set of numbers
$x$, $n$ and $m$ that
$(x^n-1)*(x^m-1) = x^(n+m)-(x^n)-(x^m)+1$.

The left and right side represent two ways to arrive at the
same result, and so an arithmetic module actually doing the
calculation does the calculation in two different ways.
The results should be exactly equal.

{RandVerifyArithmetic(n)} calls {VerifyArithmetic} with
random values, {n} times.

{VerifyDiv(u,v)} checks that
$u = v*Div(u,v) + Mod(u,v)$.

**Example:**

```
In> VerifyArithmetic(100,50,60)
Out> True;
In> RandVerifyArithmetic(4)
Out> True;
In> VerifyDiv(x^2+2*x+3,x+1)
Out> True;
In> VerifyDiv(3,2)
Out> True;

```

> **See also:** [Verify](testing.md#verifyquestionanswer)
