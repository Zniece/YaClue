# Number theory

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


This chapter describes functions that are of interest in number
theory.  These functions typically operate on integers.  Some of these
functions work quite slowly.

### IsPrime(n)

test for a prime number

**param n:**integer to test

### IsComposite(n)

test for a composite number

**param n:**positive integer

### IsCoprime(m,n)

test if integers are coprime

**param m:**positive integer

**param n:**positive integer

**param list:**list of positive integers

### IsSquareFree(n)

test for a square-free number

**param n:**positive integer

### IsPrimePower(n)

test for a power of a prime number

**param n:**integer to test

### NextPrime(i)

generate a prime following a number

**param i:**integer value

### IsTwinPrime(n)

test for a twin prime

**param n:**positive integer

### IsIrregularPrime(n)

test for an irregular prime

**param n:**positive integer

### IsCarmichaelNumber(n)

test for a Carmichael number

**param n:**positive integer

### Factors(x)

factorization

**param x:**integer or univariate polynomial

### IsAmicablePair(m,n)

test for a pair of amicable numbers

**param m:**positive integer

**param n:**positive integer

### Factor(x)

factorization, in pretty form

**param x:**integer or univariate polynomial

### Divisors(n)

number of divisors

**param n:**positive integer

### DivisorsSum(n)

the sum of  divisors

**param n:**positive integer

### ProperDivisors(n)

the number of proper divisors

**param n:**positive integer

### ProperDivisorsSum(n)

the sum of proper divisors

**param n:**positive integer

### Moebius(n)

the Moebius function

**param n:**positive integer

### CatalanNumber(n)

return the `n`-th Catalan Number

**param n:**positive integer

### FermatNumber(n)

return the `n`-th Fermat Number

**param n:**positive integer

### HarmonicNumber(n)

return the `n`-th Harmonic Number

**param n:**positive integer

**param r:**positive integer

### StirlingNumber1(n,m)

return the `n,m`-th Stirling Number of the first kind

**param n:**positive integers

**param m:**positive integers

### StirlingNumber1(n,m)

return the `n,m`-th Stirling Number of the second kind

**param n:**positive integer

**param m:**positive integer

### DivisorsList(n)

the list of divisors

**param n:**positive integer

### SquareFreeDivisorsList(n)

the list of square-free divisors

**param n:**positive integer

### MoebiusDivisorsList(n)

the list of divisors and Moebius values

**param n:**positive integer

### SumForDivisors(var,n,expr)

loop over divisors

**param var:**atom, variable name

**param n:**positive integer

**param expr:**expression depending on `var`

### RamanujanSum(k,n)

compute the Ramanujan's sum

**param k:**positive integer

**param n:**positive integer

This function computes the Ramanujan's sum, i.e. the sum of the
`n`-th powers of the `k`-th primitive roots of the unit:


where $l$ runs thought the integers between 1 and `k-1`
that are coprime to $l$. The computation is done by using the
formula in T. M. Apostol, <i>Introduction to Analytic Theory</i>
(Springer-Verlag), Theorem 8.6.


### PAdicExpand(n, p)

p-adic expansion

**param n:**number or polynomial to expand

**param p:**base to expand in

### IsQuadraticResidue(m,n)

functions related to finite groups

**param m:**integer

**param n:**odd positive integer

### GaussianFactors(z)

factorization in Gaussian integers

**param z:**Gaussian integer

### GaussianNorm(z)

norm of a Gaussian integer

**param z:**Gaussian integer

### IsGaussianUnit(z)

test for a Gaussian unit

**param z:**a Gaussian integer

### IsGaussianPrime(z)

test for a Gaussian prime

**param z:**a complex or real number

### GaussianGcd(z,w)

greatest common divisor in Gaussian integers

**param z:**Gaussian integer

**param w:**Gaussian integer

