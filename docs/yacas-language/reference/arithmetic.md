# Arithmetic and other operations on numbers

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### infix +(x,y)

addition

Addition can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
Addition is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> 2+3
Out> 5

```

### prefix -(x)

negation

Negation can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
Negation is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> - 3
Out> -3

```

### infix -(x,y)

subtraction

Subtraction can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
Subtraction is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> 2-3
Out> -1

```

### infix *(x,y)

multiplication

Multiplication can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
In the case of matrices, multiplication is defined in
terms of standard matrix product.

```

```
Multiplication is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> 2*3
Out> 6

```

### infix /(x,y)

division

Division can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
For matrices division is element-wise.

```

```
Division is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> 6/2
Out> 3

```

### infix ^(x,y)

exponentiation

Exponentiation can work on integers, rational numbers,
complex numbers, vectors, matrices and lists.

```
In the case of matrices, exponentiation is defined in
terms of standard matrix product.

```

```
Exponentiation is implemented in the standard math library (as
opposed to being built-in). This means that it can be extended by the user.

```

**Example:**

```
In> 2^3
Out> 8

```

### Div(x,y)

 determine divisor

[Div](arithmetic.md#divxy) performs integer division.
If `Div(x,y)` returns `a` and `Mod(x,y)` equals `b`, then these
numbers satisfy $x =ay + b$ and $0 \leq b < y$.

**Example:**

```
In> Div(5,3)
Out> 1

```

> **See also:** [Mod](arithmetic.md#modxy), [Gcd](arithmetic.md#gcdnm), [Lcm](arithmetic.md#lcmnm)


### Mod(x,y)

 determine remainder

[Mod](arithmetic.md#modxy) returns the division remainder.
If `Div(x,y)` returns `a` and `Mod(x,y)` equals `b`, then these
numbers satisfy $x =ay + b$ and $0 \leq b < y$.

**Example:**

```
In> Div(5,3)
Out> 1
In> Mod(5,3)
Out> 2

```

> **See also:** [Div](arithmetic.md#divxy), [Gcd](arithmetic.md#gcdnm), [Lcm](arithmetic.md#lcmnm)


### Gcd(n,m)

            Gcd(list)

greatest common divisor

This function returns the [greatest common divisor](https://en.wikipedia.org/wiki/Greatest_common_divisor) of `n` and `m`
or of all elements of `list`.

> **See also:** [Lcm](arithmetic.md#lcmnm)


### Lcm(n,m)

            Lcm(list)

least common multiple

This command returns the [least common multiple](https://en.wikipedia.org/wiki/Least_common_multiple) of `n` and `m` or
of all elements of `list`.

**Example:**

```
In> Lcm(60,24)
Out> 120
In> Lcm({3,5,7,9})
Out> 315

```

> **See also:** [Gcd](arithmetic.md#gcdnm)


### infix <<(n, m)

            infix >>(n, m)

binary shift operators

These operators shift integers to the left or to the right.  They
are similar to the C shift operators. These are sign-extended
shifts, so they act as multiplication or division by powers of 2.

**Example:**

```
In> 1 << 10
Out> 1024
In> -1024 >> 10
Out> -1

```

### FromBase(base,"string")

conversion of a number from non-decimal base to decimal base

**param base:**integer, base to convert to/from

**param number:**integer, number to write out in a different base

**param "string":**string representing a number in a different base

In Yacas, all numbers are written in decimal notation (base 10).
The two functions {FromBase}, {ToBase} convert numbers between base
10 and a different base.  Numbers in non-decimal notation are
represented by strings.    {FromBase} converts an integer, written
as a string in base  {base}, to base 10. {ToBase} converts
{number},  written in base 10, to base {base}.

### N(expression)

try determine numerical approximation of expression

**param expression:**expression to evaluate

**param precision:**integer, precision to use

The function [N](arithmetic.md#nexpression) instructs Yacas to try to coerce an expression
in to a numerical approximation to the  expression `expr`, using
`prec` digits precision if the second calling  sequence is used,
and the default precision otherwise. This overrides the normal
behaviour, in which expressions are kept in symbolic form (eg.
`Sqrt(2)` instead of `1.41421`). Application of the [N](arithmetic.md#nexpression) operator
will make Yacas  calculate floating point representations of
functions whenever  possible. In addition, the variable `Pi` is
bound to  the value of $\pi$ calculated at the current precision.

```
`N` is a macro. Its argument ``expr`` will only be evaluated after
switching to numeric mode.

```

**Example:**

```
In> 1/2
Out> 1/2;
In> N(1/2)
Out> 0.5;
In> Sin(1)
Out> Sin(1);
In> N(Sin(1),10)
Out> 0.8414709848;
In> Pi
Out> Pi;
In> N(Pi,20)
Out> 3.14159265358979323846;

```

> **See also:** [Pi](consts.md#pi)


### Rationalize(expr)

convert floating point numbers to fractions

**param expr:**an expression containing real numbers

This command converts every real number in the expression "expr"
into a rational number. This is useful when a calculation needs to
be  done on floating point numbers and the algorithm is unstable.
Converting the floating point numbers to rational numbers will
force  calculations to be done with infinite precision (by using
rational  numbers as representations).    It does this by finding
the smallest integer $n$ such that multiplying  the number with
$10^n$ is an integer. Then it divides by $10^n$ again,  depending
on the internal gcd calculation to reduce the resulting  division
of integers.

**Example:**

```
In> {1.2,3.123,4.5}
Out> {1.2,3.123,4.5};
In> Rationalize(%)
Out> {6/5,3123/1000,9/2};

```

> **See also:** [IsRational](arithmetic.md#isrationalexpr)


### ContFrac(x[,depth=6])

continued fraction expansion

**param x:**number or polynomial to expand in continued fractions

**param depth:**positive integer, maximum required depth

This command returns the [continued fraction](https://en.wikipedia.org/wiki/Continued_fraction) expansion of `x`,
which should be either a floating point number or a polynomial. The remainder
is denoted by `rest`.  This is especially useful for polynomials, since
series expansions that converge slowly will typically converge a lot faster
if calculated using a continued fraction expansion.

**Example:**

```
In> PrettyForm(ContFrac(N(Pi)))
              1
--------------------------- + 3
           1
----------------------- + 7
        1
------------------ + 15
     1
-------------- + 1
   1
-------- + 292
rest + 1
Out> True;
In> PrettyForm(ContFrac(x^2+x+1, 3))
x
---------------- + 1
x
1 - ------------
x
-------- + 1
rest + 1
Out> True;

```

> **See also:** [PAdicExpand](number-theory.md#padicexpandn-p), [N](arithmetic.md#nexpression)


### Decimal(frac)

decimal representation of a rational

**param frac:**a rational number

This function returns the infinite decimal representation of a
rational number {frac}.  It returns a list, with the first element
being the number before the decimal point and the last element the
sequence of digits that will repeat forever. All the intermediate
list  elements are the initial digits before the period sets in.

**Example:**

```
In> Decimal(1/22)
Out> {0,0,{4,5}};
In> N(1/22,30)
Out> 0.045454545454545454545454545454;

```

> **See also:** [N](arithmetic.md#nexpression)


### Floor(x)

round a number downwards

**param x:**a number

This function returns $\left \lfloor{x}\right \rfloor$, the largest
integer smaller than or equal to `x`.

**Example:**

```
In> Floor(1.1)
Out> 1;
In> Floor(-1.1)
Out> -2;

```

> **See also:** [Ceil](arithmetic.md#ceilx), [Round](arithmetic.md#roundx)


### Ceil(x)

round a number upwards

**param x:**a number

This function returns $\left \lceil{x}\right \rceil$, the smallest
integer larger than or equal to `x`.

**Example:**

```
In> Ceil(1.1)
Out> 2;
In> Ceil(-1.1)
Out> -1;

```

> **See also:** [Floor](arithmetic.md#floorx), [Round](arithmetic.md#roundx)


### Round(x)

round a number to the nearest integer

**param x:**a number

This function returns the integer closest to $x$. Half-integers
(i.e. numbers of the form $n + 0.5$, with $n$ an integer) are
rounded upwards.

**Example:**

```
In> Round(1.49)
Out> 1;
In> Round(1.51)
Out> 2;
In> Round(-1.49)
Out> -1;
In> Round(-1.51)
Out> -2;

```

> **See also:** [Floor](arithmetic.md#floorx), [Ceil](arithmetic.md#ceilx)


### Min(x,y)

           Min(list)

minimum of a number of values

This function returns the minimum value of its argument(s). If the
first calling sequence is used, the smaller of `x` and `y` is
returned. If one uses the second form, the smallest of the entries
in `list` is returned. In both cases, this function can only be
used  with numerical values and not with symbolic arguments.

**Example:**

```
In> Min(2,3)
Out> 2
In> Min({5,8,4})
Out> 4
In> Min(Pi, Exp(1))
Out> Exp(1)

```

> **See also:** [Max](arithmetic.md#maxxy), [Sum](calc.md#sumvar-from-to-body)


### Max(x,y)

           Max(list)

maximum of a number of values

This function returns the maximum value of its argument(s). If the
first calling sequence is used, the larger of `x` and `y` is
returned. If one uses the second form, the largest of the entries
in  `list` is returned. In both cases, this function can only be
used  with numerical values and not with symbolic arguments.

**Example:**

```
In> Max(2,3);
Out> 3;
In> Max({5,8,4});
Out> 8;

```

> **See also:** [Min](arithmetic.md#minxy), [Sum](calc.md#sumvar-from-to-body)


### Numer(expr)

numerator of an expression

This function determines the numerator of the rational expression
`expr` and returns it. As a special case, if its argument is
numeric  but not rational, it returns this number. If `expr` is
neither  rational nor numeric, the function returns unevaluated.

**Example:**

```
In> Numer(2/7)
Out> 2;
In> Numer(a / x^2)
Out> a;
In> Numer(5)
Out> 5;

```

> **See also:** [Denom](arithmetic.md#denomexpr), [IsRational](arithmetic.md#isrationalexpr), [IsNumber](predicates.md#isnumberexpr)


### Denom(expr)

denominator of an expression

This function determines the denominator of the rational expression
`expr` and returns it. As a special case, if its argument is
numeric but not rational, it returns `1`. If `expr` is  neither
rational nor numeric, the function returns unevaluated.

**Example:**

```
In> Denom(2/7)
Out> 7;
In> Denom(a / x^2)
Out> x^2;
In> Denom(5)
Out> 1;

```

> **See also:** [Numer](arithmetic.md#numerexpr), [IsRational](arithmetic.md#isrationalexpr), [IsNumber](predicates.md#isnumberexpr)


### Pslq(xlist[,precision=6])

search for integer relations between reals

**param xlist:**list of numbers

**param precision:**required number of digits precision of calculation

This function is an integer relation detection algorithm. This
means  that, given the numbers $x_i$ in the list `xlist`, it tries
to find integer coefficients
$a_i$ such that  $a_1*x_1+\ldots+a_n*x_n = 0$. The list of
integer coefficients is returned.
The numbers in "xlist" must evaluate to floating point numbers when
the [N](arithmetic.md#nexpression) operator is applied to them.

### infix <(e1, e2)

test for "less than"

**param e1:**expression to be compared

**param e2:**expression to be compared

The two expression are evaluated. If both results are numeric, they
are compared. If the first expression is smaller than the second
one,  the result is `True` and it is `False` otherwise. If either
of the expression is not numeric, after  evaluation, the expression
is returned with evaluated arguments.    The word "numeric" in the
previous paragraph has the following  meaning. An expression is
numeric if it is either a number (i.e. {IsNumber} returns `True`),
or the  quotient of two numbers, or an infinity (i.e. {IsInfinity}
returns `True`). Yacas will try to   coerce the arguments passed to
this comparison operator to a real value before making the
comparison.

**Example:**

```
In> 2 < 5;
Out> True;
In> Cos(1) < 5;
Out> True;

```

> **See also:** [IsNumber](predicates.md#isnumberexpr), [IsInfinity](predicates.md#isinfinityexpr), [N](arithmetic.md#nexpression)


### infix >(e1, e2)

test for "greater than"

**param e1:**expression to be compared

**param e2:**expression to be compared

The two expression are evaluated. If both results are numeric, they
are compared. If the first expression is larger than the second
one,  the result is `True` and it is `False` otherwise. If either
of the expression is not numeric, after  evaluation, the expression
is returned with evaluated arguments.    The word "numeric" in the
previous paragraph has the following  meaning. An expression is
numeric if it is either a number (i.e. {IsNumber} returns `True`),
or the  quotient of two numbers, or an infinity (i.e. {IsInfinity}
returns `True`). Yacas will try to   coerce the arguments passed to
this comparison operator to a real value before making the
comparison.

**Example:**

```
In> 2 > 5;
Out> False;
In> Cos(1) > 5;
Out> False

```

> **See also:** [IsNumber](predicates.md#isnumberexpr), [IsInfinity](predicates.md#isinfinityexpr), [N](arithmetic.md#nexpression)


### infix <=(e1, e2)

test for "less or equal"

**param e1:**expression to be compared

**param e2:**expression to be compared

The two expression are evaluated. If both results are numeric, they
are compared. If the first expression is smaller than or equals the
second one, the result is `True` and it is `False` otherwise. If
either of the expression is not  numeric, after evaluation, the
expression is returned with evaluated  arguments.    The word
"numeric" in the previous paragraph has the following  meaning. An
expression is numeric if it is either a number (i.e. {IsNumber}
returns `True`), or the  quotient of two numbers, or an infinity
(i.e. {IsInfinity} returns `True`). Yacas will try to   coerce the
arguments passed to this comparison operator to a real value before
making the comparison.

**Example:**

```
In> 2 <= 5;
Out> True;
In> Cos(1) <= 5;
Out> True

```

> **See also:** [IsNumber](predicates.md#isnumberexpr), [IsInfinity](predicates.md#isinfinityexpr), [N](arithmetic.md#nexpression)


### infix >=(e1, e2)

test for "greater or equal"

**param e1:**expression to be compared

**param e2:**expression to be compared

The two expression are evaluated. If both results are numeric, they
are compared. If the first expression is larger than or equals the
second one, the result is `True` and it is `False` otherwise. If
either of the expression is not  numeric, after evaluation, the
expression is returned with evaluated  arguments.    The word
"numeric" in the previous paragraph has the following  meaning. An
expression is numeric if it is either a number (i.e. {IsNumber}
returns `True`), or the  quotient of two numbers, or an infinity
(i.e. {IsInfinity} returns `True`). Yacas will try to   coerce the
arguments passed to this comparison operator to a real value before
making the comparison.

**Example:**

```
In> 2 >= 5;
Out> False;
In> Cos(1) >= 5;
Out> False

```

> **See also:** [IsNumber](predicates.md#isnumberexpr), [IsInfinity](predicates.md#isinfinityexpr), [N](arithmetic.md#nexpression)


### IsZero(n)

test whether argument is zero

**param n:**number to test

`IsZero(n)` evaluates to `True` if  `n` is zero. In case `n` is
not a number, the function returns  `False`.

**Example:**

```
In> IsZero(3.25)
Out> False;
In> IsZero(0)
Out> True;
In> IsZero(x)
Out> False;

```

> **See also:** [IsNumber](predicates.md#isnumberexpr), [IsNotZero](predicates.md#isnotzeron)


### IsRational(expr)

test whether argument is a rational

**param expr:**expression to test

This commands tests whether the expression "expr" is a rational
number, i.e. an integer or a fraction of integers.

**Example:**

```
In> IsRational(5)
Out> False;
In> IsRational(2/7)
Out> True;
In> IsRational(0.5)
Out> False;
In> IsRational(a/b)
Out> False;
In> IsRational(x + 1/x)
Out> False;

```

> **See also:** [Numer](arithmetic.md#numerexpr), [Denom](arithmetic.md#denomexpr)


