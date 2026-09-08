# Random numbers

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


## Simple interface

### RandomSeed(seed)

seed the global pseudo-random generator

> **See also:** [Random](random.md#random)


### Random()

generate pseudo-random number

[Random](random.md#random) generates a uniformly-distributed pseudo-random number
between 0 and 1.

> **See also:** [RandomSeed](random.md#randomseedseed)


## Advanced interface

### RngCreate()

           RngCreate(seed)
           RngCreate([seed=seed], [engine=engine], [dist=dist])
           RngCreate({seed, engine, dist})

create a pseudo-random generator

[RngCreate](random.md#rngcreate) returns a list which is a well-formed RNG object.  Its
value should be saved in a variable and used to call [Rng](random.md#rngr) and
[RngSeed](random.md#rngseedr-seed).

Engines:

 * `default`
 * `advanced`

Distributions:

 * `default` (the same as `flat`)
 * `flat` (uniform)
 * `gauss` (normal)

> **See also:** [Rng](random.md#rngr), [RngSeed](random.md#rngseedr-seed)

### RngSeed(r, seed)

(re)seed pseudo-random number generator

[RngSeed](random.md#rngseedr-seed) re-initializes the RNG object `r` with the seed value
`seed`. The seed value should be a positive integer.

> **See also:** [RngCreate](random.md#rngcreate), [Rng](random.md#rngr), [RandomSeed](random.md#randomseedseed)

### Rng(r)

generate pseudo-random number

[Rng](random.md#rngr) returns a floating-point random number between 0 and 1 and
updates the RNG object `r`.  (Currently, the Gaussian option makes a RNG
return a *complex* random number instead of a real random number.)

> **See also:** [RngCreate](random.md#rngcreate), [RngSeed](random.md#rngseedr-seed), [Random](random.md#random)


## Auxilliary functions

### RandomIntegerMatrix(rows,cols,from,to)

generate a matrix of random integers

This function generates a `rows x cols` matrix of random integers.
All  entries lie between `from` and `to`, including the boundaries,
and are uniformly distributed in this interval.

**Example:**

```
In> PrettyForm( RandomIntegerMatrix(5,5,-2^10,2^10) )
/                                               \
| ( -506 ) ( 749 )  ( -574 ) ( -674 ) ( -106 )  |
|                                               |
| ( 301 )  ( 151 )  ( -326 ) ( -56 )  ( -277 )  |
|                                               |
| ( 777 )  ( -761 ) ( -161 ) ( -918 ) ( -417 )  |
|                                               |
| ( -518 ) ( 127 )  ( 136 )  ( 797 )  ( -406 )  |
|                                               |
| ( 679 )  ( 854 )  ( -78 )  ( 503 )  ( 772 )   |
\                                               /

```

> **See also:** [RandomIntegerVector](random.md#randomintegervectorn-from-to), [RandomPoly](random.md#randompolyvardegcoefmincoefmax)


### RandomIntegerVector(n, from, to)

generate a vector of random integers

This function generates a list with `n` random integers. All
entries lie between `from` and `to`, including the boundaries, and
are uniformly distributed in this interval.

**Example:**

```
In> RandomIntegerVector(4,-3,3)
Out> {0,3,2,-2};

```

> **See also:** [Random](random.md#random), [RandomPoly](random.md#randompolyvardegcoefmincoefmax)


### RandomPoly(var,deg,coefmin,coefmax)

construct a random polynomial

[RandomPoly](random.md#randompolyvardegcoefmincoefmax) generates a random polynomial in the variable `var`, of
degree `deg`, with integer coefficients ranging from `coefmin` to
`coefmax` (inclusive). The coefficients are uniformly distributed in  this
interval, and are independent of each other.

**Example:**

```
In> RandomPoly(x,3,-10,10)
Out> 3*x^3+10*x^2-4*x-6;
In> RandomPoly(x,3,-10,10)
Out> -2*x^3-8*x^2+8;

```

> **See also:** [Random](random.md#random), [RandomIntegerVector](random.md#randomintegervectorn-from-to)

