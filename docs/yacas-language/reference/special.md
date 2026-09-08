# Special functions

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### Gamma(x)

[Euler](https://en.wikipedia.org/wiki/Leonhard_Euler)'s [Gamma](https://en.wikipedia.org/wiki/Gamma_function) function


**Example:**

```
In> Gamma(1.3)
Out> Gamma(1.3);
In> N(Gamma(1.3),30)
Out> 0.897470696306277188493754954771;
In> Gamma(1.5)
Out> Sqrt(Pi)/2;
In> N(Gamma(1.5),30);
Out> 0.88622692545275801364908374167;

```

> **See also:** `!`, `gamma`


### Zeta(x)

[Riemann](https://en.wikipedia.org/wiki/Bernhard_Riemann)'s [Zeta](https://en.wikipedia.org/wiki/Riemann_zeta_function) function

**Example:**

```
In> Precision(30)
Out> True;
In> Zeta(1)
Out> Infinity;
In> Zeta(1.3)
Out> Zeta(1.3);
In> N(Zeta(1.3))
Out> 3.93194921180954422697490751058798;
In> Zeta(2)
Out> Pi^2/6;
In> N(Zeta(2));
Out> 1.64493406684822643647241516664602;

```

> **See also:** `!`


### Bernoulli(n)

           Bernoulli(n, x)

[Bernoulli numbers](https://en.wikipedia.org/wiki/Bernoulli_number) and [Bernoulli polynomials](https://en.wikipedia.org/wiki/Bernoulli_polynomials)

### Euler(n)

           Euler(n, x)

[Euler numbers](https://en.wikipedia.org/wiki/Euler_number) and [Euler polynomials](http://mathworld.wolfram.com/EulerPolynomial.html)

**Example:**

```
In> Euler(6)
Out> -61;
In> A:=Euler(5,x)
Out> (x-1/2)^5+(-10*(x-1/2)^3)/4+(25*(x-1/2))/16;
In> Simplify(A)
Out> (2*x^5-5*x^4+5*x^2-1)/2;

```

> **See also:** [Bin](calc.md#binn-m)


### LambertW(x)

[Lambert](https://en.wikipedia.org/wiki/Johann_Heinrich_Lambert)'s [W-function](https://en.wikipedia.org/wiki/Lambert_W_function)

**Example:**

```
In> LambertW(0)
Out> 0;
In> N(LambertW(-0.24/Sqrt(3*Pi)))
Out> -0.0851224014;

```

> **See also:** [Exp](elementary.md#expx)


