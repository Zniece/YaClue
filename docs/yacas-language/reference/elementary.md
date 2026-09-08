# Elementary functions

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### Sin(x)

trigonometric sine function

**Example:**

```
In> Sin(1)
Out> Sin(1);
In> N(Sin(1),20)
Out> 0.84147098480789650665;
In> Sin(Pi/4)
Out> Sqrt(2)/2;

```

> **See also:** [Cos](elementary.md#cosx), [Tan](elementary.md#tanx), [ArcSin](elementary.md#arcsinx), [ArcCos](elementary.md#arccosx), [ArcTan](elementary.md#arctanx), `Pi`


### Cos(x)

trigonometric cosine function

**Example:**

```
In> Cos(1)
Out> Cos(1);
In> N(Cos(1),20)
Out> 0.5403023058681397174;
In> Cos(Pi/4)
Out> Sqrt(1/2);

```

> **See also:** [Sin](elementary.md#sinx), [Tan](elementary.md#tanx), [ArcSin](elementary.md#arcsinx), [ArcCos](elementary.md#arccosx), [ArcTan](elementary.md#arctanx), `Pi`


### Tan(x)

trigonometric tangent function

**Example:**

```
In> Tan(1)
Out> Tan(1);
In> N(Tan(1),20)
Out> 1.5574077246549022305;
In> Tan(Pi/4)
Out> 1;

```

> **See also:** [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [ArcSin](elementary.md#arcsinx), [ArcCos](elementary.md#arccosx), [ArcTan](elementary.md#arctanx), `Pi`


### ArcSin(x)

inverse trigonometric function arc-sine

**Example:**

```
In> ArcSin(1)
Out> Pi/2;
In> ArcSin(1/3)
Out> ArcSin(1/3);
In> Sin(ArcSin(1/3))
Out> 1/3;
In> x:=N(ArcSin(0.75))
Out> 0.848062;
In> N(Sin(x))
Out> 0.7499999477;

```

> **See also:** [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx), `Pi`, [Ln](elementary.md#lnx), [ArcCos](elementary.md#arccosx), [ArcTan](elementary.md#arctanx)


### ArcCos(x)

inverse trigonometric function arc-cosine

**Example:**

```
In> ArcCos(0)
Out> Pi/2
In> ArcCos(1/3)
Out> ArcCos(1/3)
In> Cos(ArcCos(1/3))
Out> 1/3
In> x:=N(ArcCos(0.75))
Out> 0.7227342478
In> N(Cos(x))
Out> 0.75

```

> **See also:** [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx), `Pi`, [Ln](elementary.md#lnx), [ArcSin](elementary.md#arcsinx), [ArcTan](elementary.md#arctanx)


### ArcTan(x)

inverse trigonometric function arc-tangent

**Example:**

```
In> ArcTan(1)
Out> Pi/4
In> ArcTan(1/3)
Out> ArcTan(1/3)
In> Tan(ArcTan(1/3))
Out> 1/3
In> x:=N(ArcTan(0.75))
Out> 0.643501108793285592213351264945231378078460693359375
In> N(Tan(x))
Out> 0.75

```

> **See also:** [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx), `Pi`, [Ln](elementary.md#lnx), [ArcSin](elementary.md#arcsinx), [ArcCos](elementary.md#arccosx)


### Exp(x)

exponential function

**Example:**

```
In> Exp(0)
Out> 1;
In> Exp(I*Pi)
Out> -1;
In> N(Exp(1))
Out> 2.7182818284;

```

> **See also:** [Ln](elementary.md#lnx), [Sin](elementary.md#sinx), [Cos](elementary.md#cosx), [Tan](elementary.md#tanx)


### Ln(x)

natural logarithm

**Example:**

```
In> Ln(1)
Out> 0;
In> Ln(Exp(x))
Out> x;
In> D(x) Ln(x)
Out> 1/x;

```

> **See also:** [Exp](elementary.md#expx), `Arg`


### Sqrt(x)

square root

**Example:**

```
In> Sqrt(16)
Out> 4;
In> Sqrt(15)
Out> Sqrt(15);
In> N(Sqrt(15))
Out> 3.8729833462;
In> Sqrt(4/9)
Out> 2/3;
In> Sqrt(-1)
Out> Complex(0,1);

```

> **See also:** [Exp](elementary.md#expx), `^`


### Abs(x)

absolute value or modulus of complex number

**Example:**

```
In> Abs(2);
Out> 2;
In> Abs(-1/2);
Out> 1/2;
In> Abs(3+4*I);
Out> 5;

```

> **See also:** [Sign](elementary.md#signx), `Arg`


### Sign(x)

sign of a number

**Example:**

```
In> Sign(2)
Out> 1;
In> Sign(-3)
Out> -1;
In> Sign(0)
Out> 1;
In> Sign(-3) * Abs(-3)
Out> -3;

```

> **See also:** `Arg`, [Abs](elementary.md#absx)

