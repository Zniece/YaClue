# Simplification of expressions

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


Simplification of expression is a big and non-trivial
subject. Simplification implies that there is a preferred form. In
practice the preferred form depends on the calculation at hand. This
chapter describes the functions offered that allow simplification of
expressions.

### Simplify(expr)

try to simplify an expression

This function tries to simplify the expression `expr` as much  as
possible. It does this by grouping powers within terms, and then
grouping similar terms.

**Example:**

```
In> a*b*a^2/b-a^3
Out> (b*a^3)/b-a^3;
In> Simplify(a*b*a^2/b-a^3)
Out> 0;

```

> **See also:** [FactorialSimplify](simplify.md#factorialsimplifyexpression), [LnCombine](simplify.md#lncombineexpr), [LnExpand](simplify.md#lnexpandexpr), [RadSimp](simplify.md#radsimpexpr), [TrigSimpCombine](simplify.md#trigsimpcombineexpr)


### RadSimp(expr)

simplify expression with nested radicals

This function tries to write the expression `expr` as a sum of roots  of
integers: $\sqrt{e_1} + \sqrt{e_2} + ...$, where $e_1,e_2$ and so
on are natural numbers. The expression `expr` may not contain  free
variables.

It does this by trying all possible combinations for $e_1,e_2,\ldots$.
Every possibility is numerically evaluated using [N](arithmetic.md#nexpression) and compared with
the numerical evaluation of `expr`. If the approximations are equal (up to
a certain margin), this possibility is returned. Otherwise, the expression is
returned unevaluated.

```
Due to the use of numerical approximations, there is a small chance that
the expression returned by `RadSimp` is close but not equal to
``expr``::

 In> RadSimp(Sqrt(1+10^(-6)))
 Out> 1;

```

```
If the numerical value of ``expr`` is large, the
number of possibilities becomes exorbitantly big so the evaluation may take
very long.

```

**Example:**

```
In> RadSimp(Sqrt(9+4*Sqrt(2)))
Out> Sqrt(8)+1;
In> RadSimp(Sqrt(5+2*Sqrt(6)) + Sqrt(5-2*Sqrt(6)))
Out> Sqrt(12);
In> RadSimp(Sqrt(14+3*Sqrt(3+2*Sqrt(5-12*Sqrt(3-2*Sqrt(2))))))
Out> Sqrt(2)+3;

```

> **See also:** [Simplify](simplify.md#simplifyexpr), [N](arithmetic.md#nexpression), [Sqrt](elementary.md#sqrtx)


### FactorialSimplify(expression)

simplify hypergeometric expressions containing factorials

[FactorialSimplify](simplify.md#factorialsimplifyexpression) takes an expression that may contain factorials,
and tries to simplify it. An expression like $\frac{(n+1)!}{n!}$ would
simplify to $(n+1)$.

> **See also:** [Simplify](simplify.md#simplifyexpr), `!`


### LnExpand(expr)

expand a logarithmic expression using standard logarithm rules

[LnExpand](simplify.md#lnexpandexpr) takes an expression of the form $\ln(expr)$, and
applies logarithm  rules to expand this into multiple [Ln](elementary.md#lnx) expressions
where possible.  An  expression like $\ln(ab^n)$ would be expanded to
$\ln(a)+n\ln(b)$. If the logarithm of an integer is discovered, it is
factorised using [Factors](number-theory.md#factorsx) and expanded as though [LnExpand](simplify.md#lnexpandexpr) had
been given the factorised form.  So $\ln(18)$ goes to
$\ln(2)+2\ln(3)$.

> **See also:** [LnCombine](simplify.md#lncombineexpr), [Simplify](simplify.md#simplifyexpr), [Ln](elementary.md#lnx), [Expand](univariate-polynomials.md#expandexpr)


### LnCombine(expr)

combine logarithmic expressions using standard logarithm rules

[LnCombine](simplify.md#lncombineexpr) finds [Ln](elementary.md#lnx) terms in the expression it is given, and
combines them  using logarithm rules.  It is intended to be the converse of
[LnExpand](simplify.md#lnexpandexpr).

> **See also:** [LnExpand](simplify.md#lnexpandexpr), [Simplify](simplify.md#simplifyexpr), [Ln](elementary.md#lnx)


### TrigSimpCombine(expr)

combine products of trigonometric functions

This function applies the product rules of trigonometry, e.g.
$\cos{u}\sin{v} = \frac{1}{2}(\sin(v-u) + \sin(v+u))$. As a result, all
products of the trigonometric functions [Cos](elementary.md#cosx) and [Sin](elementary.md#sinx)
disappear. The function also tries to simplify the resulting expression as
much as  possible by combining all similar terms. This function is used in
for instance `Integrate`, to bring down the expression into a simpler
form that hopefully can be  integrated easily.

**Example:**

```
In> PrettyPrinter'Set("PrettyForm");
True
In> TrigSimpCombine(Cos(a)^2+Sin(a)^2)
1
In> TrigSimpCombine(Cos(a)^2-Sin(a)^2)
Cos( -2 * a )
Out>
In> TrigSimpCombine(Cos(a)^2*Sin(b))
Sin( b )   Sin( -2 * a + b )
-------- + -----------------
   2               4
  Sin( -2 * a - b )
- -----------------
         4

```

> **See also:** [Simplify](simplify.md#simplifyexpr), `Integrate`, [Expand](univariate-polynomials.md#expandexpr), [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx)


