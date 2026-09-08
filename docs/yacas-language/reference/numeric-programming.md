# Arbitrary-Precision Numeric Programming
> **Scope:** This page distinguishes Rust core commands, standard scripts, and historical compatibility entries. A discoverable name does not imply that every argument boundary has been validated. See the [availability audit](availability.md).

## Arbitrary-precision numerical programming

This chapter contains functions that help programming numerical
calculations with arbitrary precision.

### MultiplyNum(x,y[,...])

optimized numerical multiplication

{x}, {y}, {z} -- integer, rational or floating-point numbers to multiply

The function {MultiplyNum} is used to speed up multiplication of
floating-point numbers with rational numbers. Suppose we need to
compute $(p/q)*x$ where $p$, $q$ are integers and $x$ is a
floating-point number. At high precision, it is faster to multiply
$x$ by an integer $p$ and divide by an integer $q$ than to compute
$p/q$ to high precision and then multiply by $x$. The function
{MultiplyNum} performs this optimization.

The function accepts any number of arguments (not less than two) or
a list of numbers. The result is always a floating-point number
(even if {InNumericMode()} returns False).

> **See also:** [MathMultiply](core-functions.md#mathmultiply)


### CachedConstant(cache, Cname, Cfunc)

precompute multiple-precision constants

{cache} -- atom, name of the cache
{Cname} -- atom, name of the constant
{Cfunc} -- expression that evaluates the constant

This function is used to create precomputed multiple-precision
values of constants. Caching these values will save time if they
are frequently used.

The call to {CachedConstant} defines a new function named {Cname()}
that returns the value of the constant at given precision. If the
precision is increased, the value will be recalculated as
necessary, otherwise calling {Cname()} will take very little time.

The parameter {Cfunc} must be an expression that can be evaluated
and returns the value of the desired constant at the current
precision. (Most arbitrary-precision mathematical functions do this
by default.)

The associative list {cache} contains elements of the form {{Cname,
prec, value}}, as illustrated in the example. If this list does not
exist, it will be created.

This mechanism is currently used by {N()} to precompute the values
of $Pi$ and $gamma$ (and the golden ratio through {GoldenRatio},
and {Catalan}).  The name of the cache for {N()} is
{CacheOfConstantsN}.  The code in the function {N()} assigns
unevaluated calls to {Internal'Pi()} and {Internal'gamma()} to the
atoms {Pi} and {gamma} and declares them to be lazy global
variables through {SetGlobalLazyVariable} (with equivalent
functions assigned to other constants that are added to the list of
cached constants).

The result is that the constants will be recalculated only when
they are used in the expression under {N()}.  In other words, the
code in {N()} does the equivalent of :

  SetGlobalLazyVariable(mypi,Hold(Internal'Pi()));
  SetGlobalLazyVariable(mygamma,Hold(Internal'gamma()));

After this, evaluating an expression such as {1/2+gamma} will call
the function {Internal'gamma()} but not the function
{Internal'Pi()}.

**Example:**

```
In> CachedConstant(my'cache, Ln2, Internal'LnNum(2))
Out> True;
In> Ln2()
Out> 0.6931471806;
In> V(N(Ln2(),20))
CachedConstant: Info: constant Ln2 is being recalculated at precision 20
Out> 0.69314718055994530942;
In> my'cache
Out> {{"Ln2",20,0.69314718055994530942}};

```

> **See also:** [N](arithmetic.md#nexpression), [Builtin'Precision'Set](numeric-programming.md#builtinprecisionsetn), [Pi](consts.md#pi),

             [GoldenRatio](consts.md#goldenratio), [Catalan](consts.md#catalan), [gamma](consts.md#gamma)

### NewtonNum(func, x0[, prec0[, order]])

low-level optimized Newton's iterations

{func} -- a function specifying the iteration sequence
{x0} -- initial value (must be close enough to the root)
{prec0} -- initial precision (at least 4, default 5)
{order} -- convergence order (typically 2 or 3, default 2)

This function is an optimized interface for computing Newton's
iteration sequences for numerical solution of equations in
arbitrary precision.

{NewtonNum} will iterate the given function starting from the
initial value, until the sequence converges within current
precision.  Initially, up to 5 iterations at the initial precision
{prec0} is performed (the low precision is set for speed). The
initial value {x0} must be close enough to the root so that the
initial iterations converge. If the sequence does not produce even
a single correct digit of the root after these initial iterations,
an error message is printed. The default value of the initial
precision is 5.

The {order} parameter should give the convergence order of the
scheme.  Normally, Newton iteration converges quadratically (so the
default value is {order}=2) but some schemes converge faster and
you can speed up this function by specifying the correct
order. (Caution: if you give {order}=3 but the sequence is actually
quadratic, the result will be silently incorrect. It is safe to use
{order}=2.)

The verbose option {V} can be used to monitor the convergence. The
achieved exact digits should roughly form a geometric progression.

**Example:**

```
In> Builtin'Precision'Set(20)
Out> True;
In> NewtonNum({{x}, x+Sin(x)}, 3, 5, 3)
Out> 3.14159265358979323846;

```

> **See also:** [Newton](solvers.md#newtonexpr-var-initial-accuracy)


### SumTaylorNum

optimized numerical evaluation of Taylor series

SumTaylorNum(x, NthTerm, order)
SumTaylorNum(x, NthTerm, TermFactor, order)
SumTaylorNum(x, ZerothTerm, TermFactor, order)

{NthTerm} -- a function specifying $n$-th coefficient of the series
{ZerothTerm} -- value of the $0$-th coefficient of the series
{x} -- number, value of the expansion variable
{TermFactor} -- a function specifying the ratio of $n$-th term to the previous one
{order} -- power of $x$ in the last term

{SumTaylorNum} computes a Taylor series $Sum(k,0,n,a[k]*x^k)$
numerically. This function allows very efficient computations of
functions given by Taylor series, although some tweaking of the
parameters is required for good results.

The coefficients $a_k$ of the Taylor series are given as functions
of one integer variable ($k$). It is convenient to pass them to
{SumTaylorNum} as closures.  For example, if a function {a(k)} is
defined, then :

 SumTaylorNum(x, {{k}, a(k)}, n)

computes the series $Sum(k, 0, n, a(k)*x^k)$.

Often a simple relation between successive coefficients $a_{k-1}$,
$a{k}$ of the series is available; usually they are related by a
rational factor. In this case, the second form of {SumTaylorNum}
should be used because it will compute the series faster. The
function {TermFactor} applied to an integer $k>=1$ must return the
ratio $a_k/a_{k-1}$. (If possible, the function {TermFactor}
should return a rational number and not a floating-point number.)
The function {NthTerm} may also be given, but the current
implementation only calls {NthTerm(0)} and obtains all other
coefficients by using {TermFactor}.  Instead of the function
{NthTerm}, a number giving the $0$-th term can be given.

The algorithm is described elsewhere in the documentation.  The
number of terms {order}+1 must be specified and a sufficiently high
precision must be preset in advance to achieve the desired
accuracy.  (The function {SumTaylorNum} does not change the current
precision.)

**Example:**

To compute 20 digits of $Exp(1)$ using the Taylor series, one needs
21 digits of working precision and 21 terms of the series. :

  In> Builtin'Precision'Set(21)
  Out> True;
  In> SumTaylorNum(1, {{k},1/k!}, 21)
  Out> 2.718281828459045235351;
  In> SumTaylorNum(1, 1, {{k},1/k}, 21)
  Out> 2.71828182845904523535;
  In> SumTaylorNum(1, {{k},1/k!}, {{k},1/k}, 21)
  Out> 2.71828182845904523535;
  In> RoundTo(N(Ln(%)),20)
  Out> 1;

> **See also:** [Taylor](calc.md#taylorvar-at-order-expr)


### IntPowerNum(x, n, mult, unity)

optimized computation of integer powers

{x} -- a number or an expression
{n} -- a non-negative integer (power to raise {x} to)
{mult} -- a function that performs one multiplication
{unity} -- value of the unity with respect to that multiplication

{IntPowerNum} computes the power $x^n$ using the fast binary
algorithm.  It can compute integer powers with $n>=0$ in any ring
where multiplication with unity is defined.  The multiplication
function and the unity element must be specified.  The number of
multiplications is no more than $2*Ln(n)/Ln(2)$.

Mathematically, this function is a generalization of {MathPower} to
rings other than that of real numbers.

In the current implementation, the {unity} argument is only used
when the given power {n} is zero.

**Example:**

For efficient numerical calculations, the {MathMultiply} function can be passed: :

  In> IntPowerNum(3, 3, MathMultiply,1)
  Out> 27;

Otherwise, the usual {*} operator suffices: :

  In> IntPowerNum(3+4*I, 3, *,1)
  Out> Complex(-117,44);
  In> IntPowerNum(HilbertMatrix(2), 4, *, Identity(2))
  Out> {{289/144,29/27},{29/27,745/1296}};

Compute $Mod(3^100,7)$: :

  In> IntPowerNum(3,100,{{x,y},Mod(x*y,7)},1)
  Out> 4;

> **See also:** [MultiplyNum](numeric-programming.md#multiplynumxy), [MathPower](core-functions.md#mathpower),

             [MatrixPower](linear-algebra.md#matrixpowermatn)

### BinSplitNum(n1, n2, a, b, c, d)

computations of series by the binary splitting method

### BinSplitData(n1,n2, a, b, c, d)

computations of series by the binary splitting method

### BinSplitFinal({P,Q,B,T})

computations of series by the binary splitting method

{n1}, {n2} -- integers, initial and final indices for summation
{a}, {b}, {c}, {d} -- functions of one argument, coefficients of the series
{P}, {Q}, {B}, {T} -- numbers, intermediate data as returned by {BinSplitData}

The binary splitting method is an efficient way to evaluate many
series when fast multiplication is available and when the series
contains only rational numbers.  The function {BinSplitNum}
evaluates a series of the form $S(n[1],n[2])=Sum(k,n[1],n[2], a(k)/b(k)*(p(0)/q(0)) * ... * p(k)/q(k))$.  Most series for
elementary and special functions at rational points are of this
form when the functions $a(k)$, $b(k)$, $p(k)$, $q(k)$ are chosen
appropriately.

The last four arguments of {BinSplitNum} are functions of one
argument that give the coefficients $a(k)$, $b(k)$, $p(k)$, $q(k)$.
In most cases these will be short integers that are simple to
determine.  The binary splitting method will work also for
non-integer coefficients, but the calculation will take much longer
in that case.

Note: the binary splitting method outperforms the straightforward
summation only if the multiplication of integers is faster than
quadratic in the number of digits.  See <*the algorithm
documentation|yacasdoc://Algo/3/14/*> for more information.

The two other functions are low-level functions that allow a finer
control over the calculation.  The use of the low-level routines
allows checkpointing or parallelization of a binary splitting
calculation.

The binary splitting method recursively reduces the calculation of
$S(n[1],n[2])$ to the same calculation for the two halves of the
interval $[n_1, n_2]$.  The intermediate results of a binary
splitting calculation are returned by {BinSplitData} and consist of
four integers $P$, $Q$, $B$, $T$.  These four integers are
converted into the final answer $S$ by the routine {BinSplitFinal}
using the relation $S = T / (B*Q)$.

**Example:**

Compute the series for $e=Exp(1)$ using binary splitting.
(We start from $n=1$ to simplify the coefficient functions.):

  In> Builtin'Precision'Set(21)
  Out> True;
  In>  BinSplitNum(1,21, {{k},1}, {{k},1},{{k},1},{{k},k})
  Out> 1.718281828459045235359;
  In> N(Exp(1)-1)
  Out> 1.71828182845904523536;
  In>  BinSplitData(1,21, {{k},1}, {{k},1},{{k},1},{{k},k})
  Out> {1,51090942171709440000,1, 87788637532500240022};
  In> BinSplitFinal(%)
  Out> 1.718281828459045235359;

> **See also:** [SumTaylorNum](numeric-programming.md#sumtaylornum)


### MathGetExactBits(x)

manipulate precision of floating-point numbers

### MathSetExactBits(x,bits)

manipulate precision of floating-point numbers

{x} -- an expression evaluating to a floating-point number
{bits} -- integer, number of bits

The Rust engine derives this value from the stored number rather than
maintaining an error interval through every arithmetic operation.
For a floating-point value, {MathGetExactBits(x)} converts its stored
decimal digit count to a binary digit count. {MathSetExactBits(x,bits)}
rounds the fractional part to the corresponding decimal capacity and
returns a floating-point value. A negative {bits} value is treated as
zero capacity. The setter does not attach a persistent precision flag,
so a later {MathGetExactBits} call is not an inverse operation.

These functions are only meaningful for floating-point numbers.
(All integers are always exact.)  For integer {x}, the function
{MathGetExactBits} returns the bit count of {x} and the function
{MathSetExactBits} returns the unmodified integer {x}.


**Example:**

The exact result depends on the precision at which the literal was
created. For example, a value created at 10 decimal digits occupies
about 34 binary digits in the current representation:

  In> MathGetExactBits(1000.123)
  Out> 34;
  In> x:=MathSetExactBits(10., 20)
  Out> 10.;

Negative capacities are clamped to zero:

  In> x:=MathSetExactBits(0., -2)
  Out> 0.;

> **See also:** [Builtin'Precision'Set](numeric-programming.md#builtinprecisionsetn), [Builtin'Precision'Get](numeric-programming.md#builtinprecisionget)


### InNumericMode()

determine if currently in numeric mode

### NonN(expr)

calculate part in non-numeric mode

{expr} -- expression to evaluate
{prec} -- integer, precision to use

When in numeric mode, [InNumericMode](numeric-programming.md#innumericmode) will return `True`, else it
will return `False`. Yacas is in numeric mode when evaluating an
expression with the function [N](arithmetic.md#nexpression). Thus when calling `N(expr)`,
[InNumericMode](numeric-programming.md#innumericmode) will return `True` while `expr` is being
evaluated.

[InNumericMode](numeric-programming.md#innumericmode) would typically be used to define a
transformation rule that defines how to get a numeric approximation
of some expression. One could define a transformation rule:

  f(_x)_InNumericMode() <- [... some code to get a numeric approximation of f(x) ... ];

[InNumericMode](numeric-programming.md#innumericmode) usually returns `False`, so transformation rules
that check for this predicate are usually left alone.

When in numeric mode, [NonN](numeric-programming.md#nonnexpr) can be called to switch back to non-numeric
mode temporarily.

[NonN](numeric-programming.md#nonnexpr) is a macro. Its argument `expr` will only be evaluated after
the numeric mode has been set appropriately.

**Example:**

```
In> InNumericMode()
Out> False
In> N(InNumericMode())
Out> True
In> N(NonN(InNumericMode()))
Out> False

```

> **See also:** [N](arithmetic.md#nexpression), [Builtin'Precision'Set](numeric-programming.md#builtinprecisionsetn),

             [Builtin'Precision'Get](numeric-programming.md#builtinprecisionget), [Pi](consts.md#pi),
             [CachedConstant](numeric-programming.md#cachedconstantcache-cname-cfunc)

### IntLog(n, base)

integer part of logarithm

{n}, {base} -- positive integers

{IntLog} calculates the integer part of the logarithm of {n} in
base {base}. The algorithm uses only integer math and may be faster
than computing $Ln(n)/Ln(base)$ with multiple precision
floating-point math and rounding off to get the integer part.

This function can also be used to quickly count the digits in a
given number.

**Example:**

Count the number of bits:

  In> IntLog(257^8, 2)
  Out> 64;

Count the number of decimal digits:

  In> IntLog(321^321, 10)
  Out> 804;

> **See also:** [IntNthRoot](numeric-programming.md#intnthrootx-n), [Div](arithmetic.md#divxy), [Mod](arithmetic.md#modxy),

             [Ln](elementary.md#lnx)

### IntNthRoot(x, n)

integer part of $n$-th root

{x}, {n} -- positive integers

{IntNthRoot} calculates the integer part of the $n$-th root of
$x$. The algorithm uses only integer math and may be faster than
computing $x^(1/n)$ with floating-point and rounding.

This function is used to test numbers for prime powers.

**Example:**

```
In> IntNthRoot(65537^111, 37)
Out> 281487861809153;

```

> **See also:** [IntLog](numeric-programming.md#intlogn-base), [MathPower](core-functions.md#mathpower), [IsPrimePower](number-theory.md#isprimepowern)


### NthRoot(m,n)

calculate/simplify nth root of an integer

{m} -- a non-negative integer (`m>0`)
{n} -- a positive integer greater than 1 (`n>1`)

{NthRoot(m,n)} calculates the integer part of the $n$-th root
$m^(1/n)$ and returns a list {{f,r}}. {f} and {r} are both positive
integers that satisfy $f^nr=m$.  In other words, $f$ is the
largest integer such that $m$ divides $f^n$ and $r$ is the
remaining factor.

For large {m} and small {n} {NthRoot} may work quite slowly. Every
result {{f,r}} for given {m}, {n} is saved in a lookup table, thus
subsequent calls to {NthRoot} with the same values {m}, {n} will be
executed quite fast.

**Example:**

```
In> NthRoot(12,2)
Out> {2,3};
In> NthRoot(81,3)
Out> {3,3};
In> NthRoot(3255552,2)
Out> {144,157};
In> NthRoot(3255552,3)
Out> {12,1884};

```

> **See also:** [IntNthRoot](numeric-programming.md#intnthrootx-n), [Factors](number-theory.md#factorsx), [MathPower](core-functions.md#mathpower)


### ContFracList(frac[,depth])

manipulate continued fractions

### ContFracEval(list[,rest])

manipulate continued fractions

{frac} -- a number to be expanded
{depth} -- desired number of terms
{list} -- a list of coefficients
{rest} -- expression to put at the end of the continued fraction

The function {ContFracList} computes terms of the continued
fraction representation of a rational number {frac}.  It returns a
list of terms of length {depth}. If {depth} is not specified, it
returns all terms.

The function {ContFracEval} converts a list of coefficients into a
continued fraction expression. The optional parameter {rest}
specifies the symbol to put at the end of the expansion. If it is
not given, the result is the same as if {rest=0}.

**Example:**

```
In> A:=ContFracList(33/7 + 0.000001)
Out> {4,1,2,1,1,20409,2,1,13,2,1,4,1,1,3,3,2};
In> ContFracEval(Take(A, 5))
Out> 33/7;
In> ContFracEval(Take(A,3), remainder)
Out> 1/(1/(remainder+2)+1)+4;

```

> **See also:** [ContFrac](arithmetic.md#contfracxdepth6), [GuessRational](numeric-programming.md#guessrationalxdigits)


### GuessRational(x[,digits])

find optimal rational approximations

### NearRational(x,[digits])

find optimal rational approximations

### BracketRational(x,eps)

find optimal rational approximations

{x} -- a number to be approximated (must be already evaluated to floating-point)
{digits} -- desired number of decimal digits (integer)
{eps} -- desired precision

The functions {GuessRational(x)} and {NearRational(x)} attempt to
find "optimal" rational approximations to a given value {x}. The
approximations are "optimal" in the sense of having smallest
numerators and denominators among all rational numbers close to
{x}. This is done by computing a continued fraction representation
of {x} and truncating it at a suitably chosen term.  Both functions
return a rational number which is an approximation of {x}.

Unlike the function {Rationalize()} which converts floating-point
numbers to rationals without loss of precision, the functions
{GuessRational()} and {NearRational()} are intended to find the
best rational that is *approximately* equal to a given value.

The function {GuessRational()} is useful if you have obtained a
floating-point representation of a rational number and you know
approximately how many digits its exact representation should
contain.  This function takes an optional second parameter {digits}
which limits the number of decimal digits in the denominator of the
resulting rational number. If this parameter is not given, it
defaults to half the current precision. This function truncates the
continuous fraction expansion when it encounters an unusually large
value (see example).  This procedure does not always give the
"correct" rational number; a rule of thumb is that the
floating-point number should have at least as many digits as the
combined number of digits in the numerator and the denominator of
the correct rational number.

The function {NearRational(x)} is useful if one needs to
approximate a given value, i.e. to find an "optimal" rational
number that lies in a certain small interval around a certain value
{x}. This function takes an optional second parameter {digits}
which has slightly different meaning: it specifies the number of
digits of precision of the approximation; in other words, the
difference between {x} and the resulting rational number should be
at most one digit of that precision. The parameter {digits} also
defaults to half of the current precision.

The function {BracketRational(x,eps)} can be used to find
approximations with a given relative precision from above and from
below.  This function returns a list of two rational numbers
{{r1,r2}} such that $r1<x<r2$ and $Abs(r2-r1)<Abs(x*eps)$.  The
argument {x} must be already evaluated to enough precision so that
this approximation can be meaningfully found.  If the approximation
with the desired precision cannot be found, the function returns an
empty list.

**Example:**

Start with a rational number and obtain a floating-point approximation:

  In> x:=N(956/1013)
  Out> 0.9437314906
  In> Rationalize(x)
  Out> 4718657453/5000000000;
  In> V(GuessRational(x))
  GuessRational: using 10 terms of the continued fraction
  Out> 956/1013;
  In> ContFracList(x)
  Out> {0,1,16,1,3,2,1,1,1,1,508848,3,1,2,1,2,2};

The first 10 terms of this continued fraction correspond to the
correct continued fraction for the original rational number:

  In> NearRational(x)
  Out> 218/231;

This function found a different rational number closeby because the
precision was not high enough:

  In> NearRational(x, 10)
  Out> 956/1013;

Find an approximation to $Ln(10)$ good to 8 digits:

  In> BracketRational(N(Ln(10)), 10^(-8))
  Out> {12381/5377,41062/17833};

> **See also:** [ContFrac](arithmetic.md#contfracxdepth6), [ContFracList](numeric-programming.md#contfraclistfracdepth),

             [Rationalize](arithmetic.md#rationalizeexpr)

### TruncRadian(r)

remainder modulo $2*Pi$

{r} -- a number

{TruncRadian} calculates $Mod(r,2*Pi)$, returning a value between
$0$ and $2*Pi$. This function is used in the trigonometry
functions, just before doing a numerical calculation using a Taylor
series. It greatly speeds up the calculation if the value passed is
a large number.

The library uses the formula $TruncRadian(r) = r - Floor( r/(2*Pi) )*2*Pi$, where $r$ and $2*Pi$ are calculated with twice the
precision used in the environment to make sure there is no rounding
error in the significant digits.

**Example:**

```
In> 2*Internal'Pi()
Out> 6.283185307;
In> TruncRadian(6.28)
Out> 6.28;
In> TruncRadian(6.29)
Out> 0.0068146929;

```

> **See also:** [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx)


### Builtin'Precision'Set(n)

set the precision

{n} -- integer, new value of precision

This command sets the number of decimal digits to be used in
calculations.  All subsequent floating point operations will allow
for at least {n} digits of mantissa.

This is not the number of digits after the decimal point.  For
example, {123.456} has 3 digits after the decimal point and 6
digits of mantissa.  The number {123.456} is adequately computed by
specifying {Builtin'Precision'Set(6)}.

The call {Builtin'Precision'Set(n)} will not guarantee that all
results are precise to {n} digits.

When the precision is changed, all variables containing previously
calculated values remain unchanged.  The {Builtin'Precision'Set}
function only makes all further calculations proceed with a
different precision.

Also, when typing floating-point numbers, the current value of
{Builtin'Precision'Set} is used to implicitly determine the number
of precise digits in the number.

**Example:**

```
In> Builtin'Precision'Set(10)
Out> True;
In> N(Sin(1))
Out> 0.8414709848;
In> Builtin'Precision'Set(20)
Out> True;
In> x:=N(Sin(1))
Out> 0.84147098480789650665;

```

The value {x} is not changed by a {Builtin'Precision'Set()} call:

   In> [ Builtin'Precision'Set(10); x; ]
   Out> 0.84147098480789650665;

The value {x} is rounded off to 10 digits after an arithmetic
operation:

  In> x+0.
  Out> 0.8414709848;

In the above operation, {0.} was interpreted as a number which is
precise to 10 digits (the user does not need to type {0.0000000000}
for this to happen).  So the result of {x+0.} is precise only to 10
digits.

> **See also:** [Builtin'Precision'Get](numeric-programming.md#builtinprecisionget), [N](arithmetic.md#nexpression)


### Builtin'Precision'Get()

get the current precision

This command returns the current precision, as set by
{Builtin'Precision'Set}.

**Example:**

```
In> Builtin'Precision'Get();
Out> 10;
In> Builtin'Precision'Set(20);
Out> True;
In> Builtin'Precision'Get();
Out> 20;

```

> **See also:** [Builtin'Precision'Set](numeric-programming.md#builtinprecisionsetn), [N](arithmetic.md#nexpression)
