# Rust Core Functions
> **Scope:** This page distinguishes Rust core commands, standard scripts, and historical compatibility entries. A discoverable name does not imply that every argument boundary has been validated. See the [availability audit](availability.md).

## Interface layers

This page collects low-level functions used directly by standard-script authors. Rust does not register all of them as core commands. They fall into these groups:

- **Rust core commands:** logic, comparisons, integers, basic arithmetic, `Fast*`, and shifts;
- **standard-script numeric adapters:** `MathExp`, `MathLog`, `MathPower`, trigonometric and inverse-trigonometric functions, and `MathSqrt`;
- **historical entries:** unavailable hyperbolic `Math*`, console timing, and prompt functions are listed only in the [availability audit](availability.md).

Core commands are implemented under `yacas/yacas-rs/src/commands` and entered in the single `commands::register_core_commands` registry. Rules can extend or replace standard-script adapters. The `Math` prefix alone does not identify an implementation layer, and core commands are not automatically faster than script functions.

## Rust logic, comparison, and integer commands

### MathNot(expression)

built-in logical "not"

Returns "False" if "expression" evaluates to "True", and vice
versa.

### MathAnd()

built-in logical "and"

Lazy logical {And}: returns `True` if all args evaluate to `True`,
and does this by looking at first, and then at the second argument,
until one is `False`.  If one of the arguments is `False`, {And}
immediately returns `False` without evaluating the rest. This is
faster, but also means that none of the arguments should cause side
effects when they are evaluated.

### MathOr()

built-in logical "or"

{MathOr} is the basic logical "or" function. Similarly to {And}, it
is lazy-evaluated. {And(...)} and {Or(...)} do also exist, defined
in the script library. You can redefine them as infix operators
yourself, so you have the choice of precedence. In the standard
scripts they are in fact declared as infix operators, so you can
write {expr1 And expr}.

### BitAnd(n,m)

bitwise and operation

### BitOr(n,m)

bitwise or operation

### BitXor(n,m)

bitwise xor operation

These functions return bitwise "and", "or" and "xor" of two
numbers.

### Equals(a,b)

check equality

Compares evaluated {a} and {b} recursively (stepping into
expressions). So "Equals(a,b)" returns "True" if the expressions
would be printed exactly the same, and "False" otherwise.

### GreaterThan(a,b)

comparison predicate

### LessThan(a,b)

comparison predicate

{a}, {b} -- numbers or strings

Comparing numbers or strings (lexicographically).

**Example:**

```
In> LessThan(1,1)
Out> False;
In> LessThan("a","b")
Out> True;

```

## Basic arithmetic core and numeric adapters

<a id="mathgcd"></a>
### MathGcd(n,m)

Greatest Common Divisor

<a id="mathadd"></a>
### MathAdd(x,y)

(add two numbers)

### MathSubtract(x,y)

(subtract two numbers)

<a id="mathmultiply"></a>
### MathMultiply(x,y)

(multiply two numbers)

<a id="mathdivide"></a>
### MathDivide(x,y)

(divide two numbers)

### MathSqrt(x)

(square root, must be x>=0)

### MathFloor(x)

(largest integer not larger than x)

### MathCeil(x)

(smallest integer not smaller than x)

### MathAbs(x)

(absolute value of x, or `|x|` )

### MathExp(x)

(exponential, base 2.718...)

### MathLog(x)

(natural logarithm, for x>0)

<a id="mathpower"></a>
### MathPower(x,y)

(power, x ^ y)

<a id="mathsin"></a>
### MathSin(x)

(sine)

### MathCos(x)

(cosine)

### MathTan(x)

(tangent)

### MathArcSin(x)

(inverse sine)

### MathArcCos(x)

(inverse cosine)

### MathArcTan(x)

(inverse tangent)

### MathDiv(x,y)

(integer division, result is an integer)

### MathMod(x,y)

(remainder of division, or x mod y)

These commands perform the calculation of elementary mathematical
functions.  The arguments *must* be numbers.  The reason for
the prefix {Math} is that the library needs to define equivalent
non-numerical functions for symbolic computations, such as {Exp},
{Sin} and so on.

Note that all functions, such as the {MathPower}, {MathSqrt},
{MathAdd} etc., accept integers as well as floating-point numbers.
The resulting values may be integers or floats.  If the
mathematical result is an exact integer, then the integer is
returned.  For example, {MathSqrt(25)} returns the integer {5}, and
{MathPower(2,3)} returns the integer {8}.  In such cases, the
integer result is returned even if the calculation requires more
digits than set by {Builtin'Precision'Set}.  However, when the
result is mathematically not an integer, the functions return a
floating-point result which is correct only to the current
precision.

**Example:**

```
In> Builtin'Precision'Set(10)
Out> True
In> Sqrt(10)
Out> Sqrt(10)
In> MathSqrt(10)
Out> 3.16227766
In> MathSqrt(490000*2^150)
Out> 26445252304070013196697600
In> MathSqrt(490000*2^150+1)
Out> 0.264452523e26
In> MathPower(2,3)
Out> 8
In> MathPower(2,-3)
Out> 0.125

```

## Bounded floating-point and shift core

### FastLog(x)

(natural logarithm),

### FastPower(x,y)

### FastArcSin(x)

double-precision math functions

These are bounded floating-point primitives implemented by the Rust numeric
core. They are intended for algorithms that explicitly request fast approximate
arithmetic; they do not replace arbitrary-precision operations.

### ShiftLeft(expr, bits)

built-in bitwise shift left operation

### ShiftRight(expr, bits)

built-in bitwise shift right operation

ShiftLeft(expr,bits)
ShiftRight(expr,bits)

Shift bits to the left or to the right.
