# YaClue mathematical input

This page explains the **mathematical input expressions** used in YaClue's expression field. They are not the [Yacas scripting language](yacas-language/README.md) used in `.ys` files.

## One expression per request

Input must contain one non-empty expression. YaClue input does not accept statement terminators (`;`), newlines or multi-statement programs, definitions or assignments such as `f(x):=x^2`, strings, or colon-led script constructs. Write those forms in Yacas `.ys` scripts; the calculator field is not a script console.

## Basic notation and composition

Numbers, symbols, function calls, parentheses, lists, matrices, and ordinary mathematical operators are accepted: `2*x^2-3*x+1`, `Sin(x)^2+Cos(x)^2`, `f(x+1)`, `{1,2,3}`, `{{1,2},{3,4}}`, and `x^2-5*x+6==0`. `==` constructs an equation; use `Solve` to solve it. Names begin with an ASCII letter and may continue with ASCII letters or digits. Names are case-sensitive: `mass` and `Mass`, or `foo(x)` and `Foo(x)`, are distinct.

## One-shot assumptions

A single top-level semicolon may attach assumptions to one calculation, as in `Sqrt(x^2);x>0`. Separate multiple assumptions with commas: `x/y;x>0,y!=0`. The accepted forms are `var>0`, `var<0`, `var!=0`, and their equivalent reversed comparisons. The UI renders `!=` as `≠`. These assumptions are scoped to that calculation and are restored after either success or failure; they do not become session state.

Expressions compose inside-out, preserving the structured inner result:

```text
D(x)Integrate(t,0,x)Sin(t^2)
Integrate(x)Taylor(Exp(x),0,2)
N(Determinant({{1,2},{3,4}}))
```

## Elementary, special, and vector functions

In addition to the product operations below, a mathematical expression may call
the bundled evaluator's pure mathematical functions. The following common
forms are part of the calculator input surface (names are case-sensitive):

| Family | Forms |
|---|---|
| Elementary functions | `Sin(x)`, `Cos(x)`, `Tan(x)`, `ArcSin(x)`, `ArcCos(x)`, `ArcTan(x)`, `Exp(x)`, `Ln(x)`, `Sqrt(x)`, `Abs(x)`, `Sign(x)` |
| Integer and scalar arithmetic | `Div(a,b)`, `Mod(a,b)`, `Gcd(a,b)`, `Lcm(a,b)`, `Floor(x)`, `Ceil(x)`, `Round(x)`, `Min(a,b)`, `Max(a,b)`, `Numer(expr)`, `Denom(expr)` |
| Special functions | `Gamma(x)`, `Zeta(x)`, `Bernoulli(n)`, `Euler(n)`, `LambertW(x)` |
| Vectors | `Norm(v)`, `PNorm(v,p)`, `Normalize(v)`, `Dot(u,v)`, `CrossProduct(u,v)`, `Outer(u,v)` |
| Common matrix helpers | `Trace(matrix)`, `MatrixPower(matrix,n)`, `Diagonal(matrix)`, `DiagonalMatrix(vector)`, `Identity(n)`, `ZeroMatrix(n)` |

Vectors use list notation, for example `Norm({3,4})` evaluates to `5` and
`Norm({x,y})` to `Sqrt(x^2+y^2)`. `Norm(v)` is the Euclidean (2-)norm;
`PNorm(v,p)` is the p-norm. `Abs(x)` is absolute value. The calculator's
mathematical input uses these function names, rather than treating visual
absolute-value or norm bars as independently executable source syntax.

The bundled evaluator contains a broader script-library API. A function being
available to that library does not automatically make it a documented
YaClue-specific structured operation or guarantee step-by-step explanation.

## Core operator forms

Here `expr` is an expression, `var` a symbol, `a` and `b` points or bounds, `n` a non-negative integer order, and `dir` a direction.

| Operation | Complete forms |
|---|---|
| Derivative | `D(var)expr`, `D(var,n)expr`; `Deriv` is an alias. |
| Integral | `Integrate(var)expr`, `Integrate(var,a,b)expr`. |
| Limit | `Limit(expr,a)`, `Limit(var,a)expr`, `Limit(var,a,dir)expr`. |
| Taylor series | `Taylor(expr,a,n)`, `Taylor(var,a,n)expr`. |
| Substitution | `Subst(var,replacement)expr`. |
| Equations | `Solve(equation,var)`, `Solve({equation,...},{var,...})`. |
| Algebra | `Factor(expr)`, `Expand(expr)`, `Simplify(expr)`, `Tidy(expr)`, `Apart(expr,var)`. |
| Approximation | `N(expr)`, `N(expr,precision)`, `Approximate(expr[,precision])`. |
| Series / ODE / plot | `Sum(var,a,b,expr)`, `OdeSolve(equation)`, `Plot(expr,var,a,b)`. `Sum(var,a,b)` is a valid partial input. |

The first variable in the bodied derivative, integral, variable-form limit or Taylor, and sum forms is bound in the trailing operand. For example, `x` is local in `D(x)(x*y)`, while `y` remains a free parameter.

## Other registered operations

- Matrix: `Transpose`, `Determinant`, `Inverse`, `Rank`, `RREF`, `RowReduce`, `EigenValues`, `NullSpace`, `ColumnSpace`, `EigenSpaces`, `PLDU`, `Cholesky`, `GramSchmidt`, `OrthogonalBasis`, `OrthonormalBasis`, and `Factors` take one matrix. `MatrixSolve`/`SolveMatrix` take a matrix and vector.
- Defined and multiple integrals: `ImproperIntegral(expr,var,a,b[,options])`, `PrincipalValueIntegral(expr,var,a,b[,options])`, `DoubleIntegral(expr,x,a,b,y,c,d)`, and `PolarIntegral(expr,x,y,r,theta,r0,r1,t0,t1)`.
- Numeric and multivariate: `FindRoot(expr,var,initial)`, `OdeSolveNumeric(expr,var,dependent,x0,y0,x1)`, `Extrema(expr,var1,var2)`, `Lagrange(objective,constraint,var1,var2)`, `Gradient`/`Jacobian`/`Hessian`/`Divergence`/`Curl` (two or three arguments), and `DirectionalDerivative(expr,vars,dir[,assumption[,extra]])`.
- Line and surface integrals: `ScalarLineIntegral` and `VectorLineIntegral` take six arguments; `ScalarSurfaceIntegral` and `VectorSurfaceIntegral` take six or seven.

Syntactic validity does not promise a closed-form answer; domain solvers may impose further mathematical constraints.

## Partial input and currying

Partial input is deliberate, but not universal automatic currying.

- A bodied operation with valid supplied arguments and only its final operand missing becomes a composable partial application: `D(x)` and `Integrate(x)` await an expression, which completes them as `D(x)x^2` and `Integrate(x)x^2`.
- An incomplete prefix that can match more than one signature remains an ambiguous partial application. YaClue preserves every compatible signature rather than guessing; `Limit(t)`, for example, has several possible arities.
- A missing non-final argument, an unregistered signature, or an invalid argument is not promoted to an executable curried call. Filling a variable slot requires a symbol; filling an order slot requires a non-negative integer.

Partial applications are structured held/unresolved mathematical objects, not errors or ordinary values.

## Outcomes

A successfully parsed expression first becomes a structured mathematical object. Its result can be a composable value, a held/unresolved object, a no-value mathematical conclusion (such as absence or divergence), an effect-only result, or an error. `Plot` normally supplies an effect alongside ordinary mathematical output. Conditions travel with transformations; unresolved, no value, no solution, divergence, and input errors are distinct states.

## Script language

YaClue uses yacas-rs for expression parsing and symbolic computation. For rules, assignments, program blocks, and standard-library `.ys` files, see the [Yacas scripting language documentation](yacas-language/README.md).
