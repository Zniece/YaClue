# Operations on polynomials

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


This chapter contains commands to manipulate polynomials. This
includes functions for constructing and evaluating orthogonal
polynomials.

### Expand(expr)

           Expand(expr,var)
           Expand(expr,varlist)

transform a polynomial to an expanded form

**param expr:**a polynomial expression

**param var:**a variable

**param varlist:**a list of variables

This command brings a polynomial in expanded form, in which
polynomials are represented in the form   $c_0 + c_1x + c_2x^2 + ... + c_nx^n$. In this form, it is   easier to test whether a
polynomial is zero, namely by testing  whether all coefficients are
zero.    If the polynomial {expr} contains only one variable, the
first  calling sequence can be used. Otherwise, the second form
should be  used which explicitly mentions that {expr} should be
considered as a  polynomial in the variable {var}. The third
calling form can be used  for multivariate polynomials. Firstly,
the polynomial {expr} is  expanded with respect to the first
variable in {varlist}. Then the  coefficients are all expanded with
respect to the second variable, and  so on.

**Example:**

```
In> Expand((1+x)^5)
Out> x^5+5*x^4+10*x^3+10*x^2+5*x+1
In> Expand((1+x-y)^2, x);
Out> x^2+2*(1-y)*x+(1-y)^2
In> Expand((1+x-y)^2, {x,y})
Out> x^2+((-2)*y+2)*x+y^2-2*y+1

```

> **See also:** [ExpandBrackets](univariate-polynomials.md#expandbracketsexpr)


### Degree(expr,[var])

degree of a polynomial

**param expr:**a polynomial

**param var:**a variable occurring in {expr}

This command returns the
[degree](http://en.wikipedia.org/wiki/Degree_of_a_polynomial) of the
polynomial `expr` with respect to the variable `var`. If only one
variable occurs in `expr`, the first calling sequence can be used.
Otherwise the user should use the second form in which the variable is
explicitly mentioned.

**Example:**

```
In> Degree(x^5+x-1);
Out> 5;
In> Degree(a+b*x^3, a);
Out> 1;
In> Degree(a+b*x^3, x);
Out> 3;

```

> **See also:** [Expand](univariate-polynomials.md#expandexpr), [Coef](univariate-polynomials.md#coefexpr-var-order)


### Coef(expr, var, order)

coefficient of a polynomial

**param expr:**a polynomial

**param var:**a variable occurring in {expr}

**param order:**integer or list of integers

This command returns the coefficient of {var} to the power {order}
in the polynomial {expr}. The parameter {order} can also be a list
of integers, in which case this function returns a list of
coefficients.

**Example:**

```
In> e := Expand((a+x)^4,x)
Out> x^4+4*a*x^3+(a^2+(2*a)^2+a^2)*x^2+
(a^2*2*a+2*a^3)*x+a^4;
In> Coef(e,a,2)
Out> 6*x^2;
In> Coef(e,a,0 .. 4)
Out> {x^4,4*x^3,6*x^2,4*x,1};

```

> **See also:** [Expand](univariate-polynomials.md#expandexpr), [Degree](univariate-polynomials.md#degreeexprvar), [LeadingCoef](univariate-polynomials.md#leadingcoefpoly)


### Content(expr)

content of a univariate polynomial

**param expr:**univariate polynomial

This command determines the
[content](https://en.wikipedia.org/wiki/Primitive_part_and_content)
of a univariate polynomial.

**Example:**

```
In> poly := 2*x^2 + 4*x;
Out> 2*x^2+4*x;
In> c := Content(poly);
Out> 2*x;
In> pp := PrimitivePart(poly);
Out> x+2;
In> Expand(pp*c);
Out> 2*x^2+4*x;

```

> **See also:** [PrimitivePart](univariate-polynomials.md#primitivepartexpr), [Gcd](arithmetic.md#gcdnm)


### PrimitivePart(expr)

primitive part of a univariate polynomial

**param expr:**univariate polynomial

This command determines the [primitive part](https://en.wikipedia.org/wiki/Primitive_part_and_content) of a univariate
polynomial. The primitive part is what remains after the content is divided
out. So the  product of the content and the primitive part equals the
original  polynomial.

**Example:**

```
In> poly := 2*x^2 + 4*x;
Out> 2*x^2+4*x;
In> c := Content(poly);
Out> 2*x;
In> pp := PrimitivePart(poly);
Out> x+2;
In> Expand(pp*c);
Out> 2*x^2+4*x;

```

> **See also:** [Content](univariate-polynomials.md#contentexpr)


### LeadingCoef(poly)

           LeadingCoef(poly, var)

leading coefficient of a polynomial

This function returns the [leading coefficient](https://en.wikipedia.org/wiki/Coefficient) of `poly`, regarded as  a
polynomial in the variable `var`. The leading coefficient is the
coefficient of the term of highest degree. If only one variable  appears in
the expression `poly`, it is obvious that it should be  regarded as a
polynomial in this variable and the first calling  sequence may be used.

**Example:**

```
In> poly := 2*x^2 + 4*x;
Out> 2*x^2+4*x;
In> lc := LeadingCoef(poly);
Out> 2;
In> m := Monic(poly);
Out> x^2+2*x;
In> Expand(lc*m);
Out> 2*x^2+4*x;
In> LeadingCoef(2*a^2 + 3*a*b^2 + 5, a);
Out> 2;
In> LeadingCoef(2*a^2 + 3*a*b^2 + 5, b);
Out> 3*a;

```

> **See also:** [Coef](univariate-polynomials.md#coefexpr-var-order), [Monic](univariate-polynomials.md#monicpoly)


### Monic(poly)

           Monic(poly, var)

monic part of a polynomial

This function returns the monic part of `poly`, regarded as a polynomial in
the variable `var`. The monic part of a polynomial is the quotient of this
polynomial by its leading coefficient. So the leading coefficient of the
monic part is always one. If only one variable appears in the expression
`poly`, it is obvious that it should be regarded as a polynomial in this
variable and the first calling sequence may be used.

**Example:**

```
In> poly := 2*x^2 + 4*x;
Out> 2*x^2+4*x;
In> lc := LeadingCoef(poly);
Out> 2;
In> m := Monic(poly);
Out> x^2+2*x;
In> Expand(lc*m);
Out> 2*x^2+4*x;
In> Monic(2*a^2 + 3*a*b^2 + 5, a);
Out> a^2+(a*3*b^2)/2+5/2;
In> Monic(2*a^2 + 3*a*b^2 + 5, b);
Out> b^2+(2*a^2+5)/(3*a);

```

> **See also:** [LeadingCoef](univariate-polynomials.md#leadingcoefpoly)


### SquareFree(p)

return the square-free part of polynomial

**param p:**a polynomial in {x}

Given a polynomial $p = p_1^{n_1}\ldots p_m^{n_m}$  with irreducible
polynomials $p_i$,  return the square-free version part (with all the
factors having multiplicity 1):  $p_1\ldots p_m$

**Example:**

```
In> Expand((x+1)^5)
Out> x^5+5*x^4+10*x^3+10*x^2+5*x+1;
In> SquareFree(%)
Out> (x+1)/5;
In> Monic(%)
Out> x+1;

```

> **See also:** [FindRealRoots](solvers.md#findrealrootsp), [NumRealRoots](solvers.md#numrealrootsp), [MinimumBound](solvers.md#minimumboundp), `MaximumBound`, [Factor](number-theory.md#factorx)


### SquareFreeFactorize(p,x)

return square-free decomposition of polynomial

**param p:**a polynomial in {x}

Given a polynomial $p$ having square-free decomposition $p = p_1^{n_1}\ldots p_m^{n_m}$  where $p_i$ are square-free and
$n_{i+1}>n_i$,  return the list of pairs ($p_i$, $n_i$)

**Example:**

```
In> Expand((x+1)^5)
Out> x^5+5*x^4+10*x^3+10*x^2+5*x+1
In> SquareFreeFactorize(%,x)
Out> {{x+1,5}}

```

> **See also:** [Factor](number-theory.md#factorx)


### Horner(expr, var)

convert a polynomial into the Horner form

This command turns the polynomial `expr`, considered as a univariate
polynomial in `var`, into the Horner form. A polynomial in normal form  is
an expression such as  $c_0 + c_1x + \ldots + c_nx^n$. If one converts
this polynomial into Horner form, one gets the  equivalent expression
$(\ldots( c_nx + c_{n-1}) x + \ldots  + c_1)x + c_0$. Both expression
are equal, but the latter form gives a more  efficient way to evaluate the
polynomial as  the powers have  disappeared.

**Example:**

```
In> expr1:=Expand((1+x)^4)
Out> x^4+4*x^3+6*x^2+4*x+1;
In> Horner(expr1,x)
Out> (((x+4)*x+6)*x+4)*x+1;

```

> **See also:** [Expand](univariate-polynomials.md#expandexpr), [ExpandBrackets](univariate-polynomials.md#expandbracketsexpr), [EvaluateHornerScheme](univariate-polynomials.md#evaluatehornerschemecoeffsx)


### ExpandBrackets(expr)

expand all brackets

This command tries to expand all the brackets by repeatedly using the
distributive laws $a(b+c) = ab + ac$ and $(a+b)c = ac + bc$. It
goes further than [Expand](univariate-polynomials.md#expandexpr), in that it expands all brackets.

**Example:**

```
In> Expand((a-x)*(b-x),x)
Out> x^2-(b+a)*x+a*b;
In> Expand((a-x)*(b-x),{x,a,b})
Out> x^2-(b+a)*x+b*a;
In> ExpandBrackets((a-x)*(b-x))
Out> a*b-x*b+x^2-a*x;

```

> **See also:** [Expand](univariate-polynomials.md#expandexpr)


### EvaluateHornerScheme(coeffs,x)

fast evaluation of polynomials

This function evaluates a polynomial given as a list of its coefficients,
using the [Horner scheme](https://en.wikipedia.org/wiki/Horner%27s_method).
The list of coefficients starts with the lowest order. For example:

```text
`OrthoLSum(c, a, x) = c[1] L[0](a,x) + c[2] L[1](a,x) + ... + c[N] L[N-1](a,x)`
```

See pages for specific orthogonal polynomials for more details on the
parameters of the polynomials.  Most of the work is performed by the internal
function [OrthoPolySum](univariate-polynomials.md#orthopolysumname-c-params-x). The individual polynomials entering the series
are not computed, only the sum of the series.

**Example:**

```
In> Expand(OrthoPSum({1,0,0,1/7,1/8}, 3/2, 2/3, x));
Out> (7068985*x^4)/3981312+(1648577*x^3)/995328+
(-3502049*x^2)/4644864+(-4372969*x)/6967296
+28292143/27869184

```

> **See also:** [OrthoP](univariate-polynomials.md#orthopn-x), [OrthoG](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoH](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoL](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoT](univariate-polynomials.md#orthopolyname-n-params-x), `OrthoU`, [OrthoPolySum](univariate-polynomials.md#orthopolysumname-c-params-x)


<a id="orthopn-x"></a>
<a id="orthogn-a-x"></a>
<a id="orthohn-x"></a>
<a id="ortholn-a-x"></a>
<a id="orthotn-x"></a>

### OrthoPoly(name, n, params, x)

> **Current status:** An internal helper in the orthogonal-polynomial package. It is defined after loading public entries such as `OrthoP` and is not registered as an independent deferred-load entry.

internal function for constructing orthogonal polynomials

This function is used internally to construct orthogonal polynomials. It
returns the `n`-th polynomial from the family `name` with parameters
`params` at the point `x`. All known families are stored in the
association list returned by the function `KnownOrthoPoly`. The
`name` serves as key.

At the moment the following names are known to Yacas: `"Jacobi"`,
`"Gegenbauer"`, `"Laguerre"`, `"Hermite"`, `"Tscheb1"`, and
`"Tscheb2"`. The value associated to the key is a pure function that takes
two arguments: the order `n` and the  extra parameters `p`, and returns a
list of two lists: the first list  contains the coefficients `{A,B}` of the
`n=1` polynomial, i.e. $A+Bx$;  the second list contains the
coefficients `{A,B,C}` in the recurrence  relation, i.e. $P_n = (A+Bx)P_{n-1}+CP_{n-2}$.

```
There are only 3 coefficients in the second list, because none of the
considered polynomials use $C+Dx$ instead of $C$ in the
recurrence relation. This is assumed in the implementation.

```

If the argument `x` is numerical, the function `OrthoPolyNumeric` is
called. Otherwise, the function `OrthoPolyCoeffs` computes a list of
coefficients, and [EvaluateHornerScheme](univariate-polynomials.md#evaluatehornerschemecoeffsx) converts this list into a
polynomial expression.

> **See also:** [OrthoP](univariate-polynomials.md#orthopn-x), [OrthoG](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoH](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoL](univariate-polynomials.md#orthopolyname-n-params-x), [OrthoT](univariate-polynomials.md#orthopolyname-n-params-x), `OrthoU`, [OrthoPolySum](univariate-polynomials.md#orthopolysumname-c-params-x)


<a id="orthopsumc-x"></a>

### OrthoPolySum(name, c, params, x)

> **Current status:** An internal helper in the orthogonal-polynomial package. It is defined after loading public entries such as `OrthoPSum` and is not registered as an independent deferred-load entry.

internal function for computing series of orthogonal polynomials

This function is used internally to compute series of orthogonal polynomials.
It is similar to the function [OrthoPoly](univariate-polynomials.md#orthopolyname-n-params-x) and returns the result of the
summation of series of polynomials from the family `name` with parameters
`params`  at the point `x`, where `c` is the list of coefficients of
the series. The algorithm used to compute the series without first computing
the individual polynomials is the Clenshaw-Smith recurrence scheme.  (See the
algorithms book for explanations.)

If the argument `x` is numerical, the function `OrthoPolySumNumeric`
is called. Otherwise, the function `OrthoPolySumCoeffs` computes the
list of coefficients  of the resulting polynomial, and
[EvaluateHornerScheme](univariate-polynomials.md#evaluatehornerschemecoeffsx) converts this list into a polynomial expression.

> **See also:** [OrthoPSum](univariate-polynomials.md#orthopolysumname-c-params-x), `OrthoGSum`, `OrthoHSum`, `OrthoLSum`, `OrthoTSum`, `OrthoUSum`, [OrthoPoly](univariate-polynomials.md#orthopolyname-n-params-x)

