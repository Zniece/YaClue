# Calculus

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


In this chapter, some facilities for doing calculus are
described. These include functions implementing differentiation,
integration, calculating limits etc.

### bodied D(expression, variable[,n=1])

derivative

**param variable:**variable

**param expression:**expression to take derivatives of

**param n:**order

**returns:**`n`-th derivative of `expression` with respect to `variable`

### bodied D(expression, variable)

derivative

**param variable:**variable

**param list:**a list of variables

**param expression:**expression to take derivatives of

**param n:**order of derivative

**returns:**derivative of `expression` with respect to `variable`

This function calculates the derivative of the expression {expr}
with  respect to the variable {var} and returns it. If the third
calling  format is used, the {n}-th derivative is determined. Yacas
knows  how to differentiate standard functions such as {Ln}  and
{Sin}.    The {D} operator is threaded in both {var} and  {expr}.
This means that if either of them is a list, the function is
applied to each entry in the list. The results are collected in
another list which is returned. If both {var} and {expr} are a
list, their lengths should be equal. In this case, the first entry
in  the list {expr} is differentiated with respect to the first
entry in  the list {var}, the second entry in {expr} is
differentiated with  respect to the second entry in {var}, and so
on.    The {D} operator returns the original function if $n=0$, a
common  mathematical idiom that simplifies many formulae.

**Example:**

```
In> D(x)Sin(x*y)
Out> y*Cos(x*y);
In> D({x,y,z})Sin(x*y)
Out> {y*Cos(x*y),x*Cos(x*y),0};
In> D(x,2)Sin(x*y)
Out> -Sin(x*y)*y^2;
In> D(x){Sin(x),Cos(x)}
Out> {Cos(x),-Sin(x)};

```

> **See also:** `Integrate`, [Taylor](calc.md#taylorvar-at-order-expr), [Diverge](calc.md#divergevector-basis), [Curl](calc.md#curlvector-basis)


### Curl(vector, basis)

curl of a vector field

**param vector:**vector field to take the curl of

**param basis:**list of variables forming the basis

This function takes the curl of the vector field `vector` with respect to
the variables `basis`. The curl is defined in the usual way, ``Curl(f,x) =
{D(x[2]) f[3] - D(x[3]) f[2], D(x[3]) f[1] - D(x[1])f[3], D(x[1]) f[2] -
D(x[2]) f[1]}`.  Both `vector` and `basis`` should be lists of length 3.

### Diverge(vector, basis)

divergence of a vector field

**param vector:**vector field to calculate the divergence of

**param basis:**list of variables forming the basis

This function calculates the divergence of the vector field `vector`  with
respect to the variables `basis`. The divergence is defined as
`Diverge(f,x) = D(x[1]) f[1] + ... + D(x[n]) f[n]`,  where `n` is the
length of the lists `vector` and `basis`. These lists should have equal
length.

### HessianMatrix(function,var)

create the Hessian matrix

**param function:**a function in $n$ variables

**param var:**an $n$-dimensional vector of variables

The function [HessianMatrix](calc.md#hessianmatrixfunctionvar) calculates the Hessian matrix of a vector.
If $f(x)$ is a function of an $n$-dimensional vector $x$,
then the $(i,j)$-th element of the Hessian matrix of the function
$f(x)$ is defined as $ Deriv(x[i]) Deriv(x[j]) f(x)$. If the
second order mixed partials are continuous, then the Hessian matrix is
symmetric (a standard theorem of calculus). The Hessian matrix is used in the
second derivative test to discern if a critical point is a local maximum, a
local minimum or a saddle point.

**Example:**

```
In> HessianMatrix(3*x^2-2*x*y+y^2-8*y, {x,y} )
Out> {{6,-2},{-2,2}};
In> PrettyForm(%)
/                \
| ( 6 )  ( -2 )  |
|                |
| ( -2 ) ( 2 )   |
\                /

```

### JacobianMatrix(functions,variables)

calculate the Jacobian matrix of $n$ functions in $n$ variables

**param functions:**an $n$-dimensional vector of functions

**param variables:**an $n$-dimensional vector of variables

The function {JacobianMatrix} calculates the Jacobian matrix  of n
functions in n variables.    The $(i,j)$-th element of the
Jacobian matrix is defined as the derivative  of $i$-th function
with respect to the $j$-th variable.

**Example:**

```
In> JacobianMatrix( {Sin(x),Cos(y)}, {x,y} );
Out> {{Cos(x),0},{0,-Sin(y)}};
In> PrettyForm(%)
/                                 \
| ( Cos( x ) ) ( 0 )              |
|                                 |
| ( 0 )        ( -( Sin( y ) ) )  |
\                                 /

```

### bodied Integrate(expr, var)

           bodied Integrate(expr, var, x1, x2)

integral

**param expr:**expression to integrate

**param var:**atom, variable to integrate over

**param x1:**first point of definite integration

**param x2:**second point of definite integration

This function integrates the expression `expr` with respect to the
variable `var`. In the case of definite integral, the integration
is carried out from $var=x1$ to $var=x2$". Some simple integration
rules have currently been implemented.  Polynomials, some quotients
of polynomials, trigonometric functions and their inverses,
hyperbolic functions and their inverses, {Exp}, and {Ln}, and
products of these functions with polynomials can be integrated.

**Example:**

```
In> Integrate(x,a,b) Cos(x)
Out> Sin(b)-Sin(a);
In> Integrate(x) Cos(x)
Out> Sin(x);

```

> **See also:** `D`, [UniqueConstant](vars.md#uniqueconstant)


### bodied Limit(expr, var, val)

limit of an expression

**param var:**variable

**param val:**number or `Infinity`

**param dir:**direction (`Left` or `Right`)

**param expr:**an expression

This command tries to determine the value that the expression
"expr"  converges to when the variable "var" approaches "val". One
may use  {Infinity} or {-Infinity} for  "val". The result of
{Limit} may be one of the  symbols {Undefined} (meaning that the
limit does not  exist), {Infinity}, or {-Infinity}.    The second
calling sequence is used for unidirectional limits. If one  gives
"dir" the value {Left}, the limit is taken as  "var" approaches
"val" from the positive infinity; and {Right} will take the limit
from the negative infinity.

**Example:**

```
In> Limit(x,0) Sin(x)/x
Out> 1;
In> Limit(x,0) (Sin(x)-Tan(x))/(x^3)
Out> -1/2;
In> Limit(x,0) 1/x
Out> Undefined;
In> Limit(x,0,Left) 1/x
Out> -Infinity;
In> Limit(x,0,Right) 1/x
Out> Infinity;

```

### Add(val1, val2, ...)

           Add(list)

find sum of a list of values

**param val1 val2:**expressions

**param list:**list of expressions to add

This function adds all its arguments and returns their sum. It
accepts any  number of arguments. The arguments can be also passed
as a list.

**Example:**

```
In> Add(1,4,9);
Out> 14;
In> Add(1 .. 10);
Out> 55;

```

### Multiply(val1, val2, ...)

           Multiply(list)

product of a list of values

**param val1 val2:**expressions

**param list:**list of expressions to add

Multiply all arguments and returns their product. It
accepts any  number of arguments. The arguments can be also passed
as a list.

**Example:**

```
In> Multiply(2,3,4);
Out> 24
In> Multiply(1 .. 10)
Out> 3628800

```

### Sum(var, from, to, body)

find sum of a sequence

**param var:**variable to iterate over

**param from:**integer value to iterate from

**param to:**integer value to iterate up to

**param body:**expression to evaluate for each iteration

The command finds the sum of the sequence generated by an iterative
formula.   The expression "body" is  evaluated while the variable
"var" ranges over all integers from  "from" up to "to", and the sum
of all the results is  returned. Obviously, "to" should be greater
than or equal to  "from".    Warning: {Sum} does not evaluate its
arguments {var} and {body} until the actual loop is run.

**Example:**

```
In> Sum(i, 1, 3, i^2);
Out> 14;

```

> **See also:** [Factor](number-theory.md#factorx)

### SeriesConvergence(var, from, term)

analyze a common infinite series

`SeriesConvergence` returns
`{status, method, testValue, conditions, value}`. It recognizes geometric
and p-series directly and applies bounded shape-specific ratio and root
tests to common exponential forms. `status` is one of
`"absolutely_convergent"`, `"divergent"`, `"conditional"`, or
`"inconclusive"`. A symbolic geometric ratio produces the condition
`Abs(r)<1`; an unavailable closed form is represented by `Undefined`.

The analyzer deliberately returns `inconclusive` for unsupported shapes
instead of running an unbounded chain of symbolic limits. Product code can
therefore use it as a predictable companion to `Sum`.

```
In> SeriesConvergence(k, 0, (1/2)^k);
Out> {"absolutely_convergent","geometric",1/2,{},2};
In> SeriesConvergence(k, 1, 1/k);
Out> {"divergent","p_series",1,{},Undefined};
```


### Taylor(var, at, order) expr

univariate Taylor series expansion

**param var:**variable

**param at:**point to get Taylor series around

**param order:**order of approximation

**param expr:**expression to get Taylor series for

This function returns the Taylor series expansion of the expression
"expr" with respect to the variable "var" around "at" up to order
"order". This is a polynomial which agrees with "expr" at the
point "var = at", and furthermore the first "order" derivatives of
the polynomial at this point agree with "expr". Taylor expansions
around removable singularities are correctly handled by taking the
limit as "var" approaches "at".

**Example:**

```
In> PrettyForm(Taylor(x,0,9) Sin(x))
3    5      7       9
x    x      x       x
x - -- + --- - ---- + ------
6    120   5040   362880
Out> True;

```

> **See also:** `D`, [InverseTaylor](calc.md#inversetaylorvar-at-order-expr), [ReversePoly](calc.md#reversepolyf-g-var-newvar-degree), [BigOh](calc.md#bigohpoly-var-degree)


### InverseTaylor(var, at, order) expr

Taylor expansion of inverse

**param var:**variable

**param at:**point to get inverse Taylor series around

**param order:**order of approximation

**param expr:**expression to get inverse Taylor series for

This function builds the Taylor series expansion of the inverse of
the  expression "expr" with respect to the variable "var" around
"at"  up to order "order". It uses the function {ReversePoly} to
perform the task.

**Example:**

```
In> PrettyPrinter'Set("PrettyForm")
True
In> exp1 := Taylor(x,0,7) Sin(x)
3    5      7
x    x      x
x - -- + --- - ----
6    120   5040
In> exp2 := InverseTaylor(x,0,7) ArcSin(x)
5      7     3
x      x     x
--- - ---- - -- + x
120   5040   6
In> Simplify(exp1-exp2)
0

```

> **See also:** [ReversePoly](calc.md#reversepolyf-g-var-newvar-degree), [Taylor](calc.md#taylorvar-at-order-expr), [BigOh](calc.md#bigohpoly-var-degree)


### ReversePoly(f, g, var, newvar, degree)

solve $h(f(x)) = g(x) + O(x^n)$ for $h$

**param f:**function of `var`

**param g:**function of `var`

**param var:**a variable

**param newvar:**a new variable to express the result in

**param degree:**the degree of the required solution

This function returns a polynomial in "newvar", say "h(newvar)",
with the property that "h(f(var))" equals "g(var)" up to order
"degree". The degree of the result will be at most "degree-1". The
only requirement is that the first derivative of "f" should not be
zero.    This function is used to determine the Taylor series
expansion of the  inverse of a function "f": if we take
"g(var)=var", then  "h(f(var))=var" (up to order "degree"), so "h"
will be the  inverse of "f".

**Example:**

```
In> f(x):=Eval(Expand((1+x)^4))
Out> True;
In> g(x) := x^2
Out> True;
In> h(y):=Eval(ReversePoly(f(x),g(x),x,y,8))
Out> True;
In> BigOh(h(f(x)),x,8)
Out> x^2;
In> h(x)
Out> (-2695*(x-1)^7)/131072+(791*(x-1)^6)/32768 +(-119*(x-1)^5)/4096+(37*(x-1)^4)/1024+(-3*(x-1)^3)/64+(x-1)^2/16;

```

> **See also:** [InverseTaylor](calc.md#inversetaylorvar-at-order-expr), [Taylor](calc.md#taylorvar-at-order-expr), [BigOh](calc.md#bigohpoly-var-degree)


### BigOh(poly, var, degree)

drop all terms of a certain order in a polynomial

**param poly:**a univariate polynomial

**param var:**a free variable

**param degree:**positive integer

This function drops all terms of order "degree" or higher in
"poly", which is a polynomial in the variable "var".

**Example:**

```
In> BigOh(1+x+x^2+x^3,x,2)
Out> x+1;

```

> **See also:** [Taylor](calc.md#taylorvar-at-order-expr), [InverseTaylor](calc.md#inversetaylorvar-at-order-expr)


### LagrangeInterpolant(xlist, ylist, var)

polynomial interpolation

**param xlist:**list of argument values

**param ylist:**list of function values

**param var:**free variable for resulting polynomial

This function returns a polynomial in the variable "var" which
interpolates the points "(xlist, ylist)". Specifically, the value
of  the resulting polynomial at "xlist[1]" is "ylist[1]", the value
at  "xlist[2]" is "ylist[2]", etc. The degree of the polynomial is
not  greater than the length of "xlist".    The lists "xlist" and
"ylist" should be of equal  length. Furthermore, the entries of
"xlist" should be all distinct  to ensure that there is one and
only one solution.    This routine uses the Lagrange interpolant
formula to build up the  polynomial.

**Example:**

```
In> f := LagrangeInterpolant({0,1,2}, \
{0,1,1}, x);
Out> (x*(x-1))/2-x*(x-2);
In> Eval(Subst(x,0) f);
Out> 0;
In> Eval(Subst(x,1) f);
Out> 1;
In> Eval(Subst(x,2) f);
Out> 1;
In> PrettyPrinter'Set("PrettyForm");
True
In> LagrangeInterpolant({x1,x2,x3}, {y1,y2,y3}, x)
y1 * ( x - x2 ) * ( x - x3 )
----------------------------
( x1 - x2 ) * ( x1 - x3 )
y2 * ( x - x1 ) * ( x - x3 )
+ ----------------------------
( x2 - x1 ) * ( x2 - x3 )
y3 * ( x - x1 ) * ( x - x2 )
+ ----------------------------
( x3 - x1 ) * ( x3 - x2 )

```

> **See also:** `Subst`


### postfix !(n)

factorial

**param m:**integer

**param n:**integer, half-integer, or list

**param a}, {b:**numbers

The factorial function {n!} calculates the factorial of integer or
half-integer numbers. For  nonnegative integers, $n! := n*(n-1)*(n-2)*...*1$. The factorial of  half-integers is defined
via Euler's Gamma function, $z! := Gamma(z+1)$. If $n=0$ the
function returns $1$.    The "double factorial" function {n!!}
calculates $n*(n-2)*(n-4)*...$. This product terminates either with
$1$ or with $2$ depending on whether $n$ is odd or even. If $n=0$
the function returns $1$.    The "partial factorial" function {a
*** b} calculates the product $a*(a+1)*...$ which is terminated at
the least integer not greater than $b$. The arguments $a$ and $b$
do not have to be integers; for integer arguments, {a *** b} = $b! / (a-1)!$. This function is sometimes a lot faster than evaluating
the two factorials, especially if $a$ and $b$ are close together.
If $a>b$ the function returns $1$.    The {Subfactorial} function
can be interpreted as the  number of permutations of {m} objects in
which no object   appears in its natural place, also called
"derangements."     The factorial functions are threaded, meaning
that if the argument {n} is a  list, the function will be applied
to each element of the list.    Note: For reasons of Yacas syntax,
the factorial sign {!} cannot precede other  non-letter symbols
such as {+} or {*}. Therefore, you should enter a space  after {!}
in expressions such as {x! +1}.    The factorial functions
terminate and print an error message if the arguments are too large
(currently the limit is $n < 65535$) because exact factorials of
such large numbers are computationally expensive and most probably
not useful. One can call {Internal'LnGammaNum()} to evaluate
logarithms of such factorials to desired precision.

**Example:**

```
In> 5!
Out> 120;
In> 1 * 2 * 3 * 4 * 5
Out> 120;
In> (1/2)!
Out> Sqrt(Pi)/2;
In> 7!!;
Out> 105;
In> 1/3 *** 10;
Out> 17041024000/59049;
In> Subfactorial(10)
Out> 1334961;

```

> **See also:** [Bin](calc.md#binn-m), [Factor](number-theory.md#factorx), [Gamma](consts.md#gamma), `!!`, `***`, `Subfactorial`


### postfix !!(n)

double factorial

### infix ***(x,y)

whatever

### Bin(n, m)

binomial coefficients

**param n}, {m:**integers

This function calculates the binomial coefficient "n" above  "m",
which equals $n! / (m! * (n-m)!)$    This is equal to the number
of ways  to choose "m" objects out of a total of "n" objects if
order is  not taken into account. The binomial coefficient is
defined to be zero  if "m" is negative or greater than "n";
{Bin(0,0)}=1.

**Example:**

```
In> Bin(10, 4)
Out> 210;
In> 10! / (4! * 6!)
Out> 210;

```

> **See also:** `!`, [Eulerian](calc.md#euleriannm)


### Eulerian(n,m)

Eulerian numbers

The [Eulerian numbers](https://en.wikipedia.org/wiki/Eulerian_number) can
be viewed as a generalization of the binomial coefficients,  and are given
explicitly by $Sum(j,0,k+1,(-1)^j*Bin(n+1,j)*(k-j+1)^n)$.

**Example:**

```
In> Eulerian(6,2)
Out> 302;
In> Eulerian(10,9)
Out> 1;

```

> **See also:** [Bin](calc.md#binn-m)


### KroneckerDelta(i,j)

           KroneckerDelta({i,j,...})

Kronecker delta

Calculates the [Kronecker delta](https://en.wikipedia.org/wiki/Kronecker_delta), which gives $1$
if all arguments are equal and $0$ otherwise.

### LeviCivita(list)

totally anti-symmetric Levi-Civita symbol

**param list:**a list of integers $1,\ldots,n$ in some order

[LeviCivita](calc.md#levicivitalist) implements the [Levi-Civita symbol](https://en.wikipedia.org/wiki/Levi-Civita_symbol). `list`  should be a
list of integers, and this function returns 1 if the integers are in
successive order,  eg. `LeviCivita({1,2,3,...})`  would return 1. Swapping
two elements of this  list would return -1. So, `LeviCivita({2,1,3})` would
evaluate  to -1.

**Example:**

```
In> LeviCivita({1,2,3})
Out> 1;
In> LeviCivita({2,1,3})
Out> -1;
In> LeviCivita({2,2,3})
Out> 0;

```

> **See also:** [Permutations](calc.md#permutationslist)


### Permutations(list)

get all permutations of a list

**param list:**a list of elements

Permutations returns a list with all the permutations of  the
original list.

**Example:**

```
In> Permutations({a,b,c})
Out> {{a,b,c},{a,c,b},{c,a,b},{b,a,c},
{b,c,a},{c,b,a}};

```

> **See also:** [LeviCivita](calc.md#levicivitalist)


### Fibonacci(n)

Fibonacci sequence

The function returns $n$-th [Fibonacci number](https://en.wikipedia.org/wiki/Fibonacci_number)

**Example:**

```
In> Fibonacci(4)
Out> 3
In> Fibonacci(8)
Out> 21
In> Table(Fibonacci(i), i, 1, 10, 1)
Out> {1,1,2,3,5,8,13,21,34,55}

```
