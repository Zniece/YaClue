# Number theory algorithms

> **Implementation scope:** This page explains algorithms behind the standard scripts and may contain historical approaches or old measurements. Rust and `.yts` tests determine current behavior; performance claims require new measurements on the current implementation.


This chapter describes the algorithms used for computing various
number-theoretic functions.  We call "number-theoretic" any function
that takes integer arguments, produces integer values, and is of
interest to number theory.

## Euclidean GCD algorithms

The main algorithm for the calculation of the GCD of two integers is
the binary Euclidean algorithm.  It is based on the following
identities: $\gcd(a,b) = \gcd(b,a)$, $\gcd(a,b) = \gcd(a-b,b)$,
and for odd $b$, $\gcd(2a,b) = \gcd(a,b)$. Thus we can produce
a sequence of pairs with the same GCD as the original two numbers, and each
pair will be at most half the size of the previous pair. The number of
steps is logarithmic in the number of digits in $a$, $b$. The only
operations needed for this algorithm are binary shifts and
subtractions (no modular division is necessary).  The low-level
function for this is [MathGcd](../reference/core-functions.md#mathgcd).

To speed up the calculation when one of the numbers is much larger
than another, one could use the property $\gcd(a,b)=\gcd(a,a \bmod b)$.
This will introduce an additional modular division into the algorithm;
this is a slow operation when the numbers are large.

## Prime numbers: the Miller-Rabin test and its improvements

Small prime numbers are simply stored in a precomputed table as an array of
bits; the bits corresponding to prime numbers are set to 1.  This makes
primality testing on small numbers very quick.  This is implemented by the
function `FastIsPrime`.

Primality of larger numbers is tested by the function [IsPrime](../reference/number-theory.md#isprimen) that
uses the Miller-Rabin algorithm. [#miller-rabin]_ This algorithm is
deterministic (guaranteed correct within a certain running time) for
small numbers $n<3.4\cdot10^{13}$ and probabilistic (correct with high
probability, but not guaranteed) for larger numbers.  In other words,
the Miller-Rabin test could sometimes flag a large number $n$ as prime
when in fact $n$ is composite; but the probability for this to happen
can be made extremely small. The basic reference is
[rabin1980:probabilistic_algorithm_testing](references.md).  We also implemented some
of the improvements suggested in [davenport1992:primality_test_revisited](references.md).

The idea of the Miller-Rabin algorithm is to improve on the Fermat
primality test. If $n$ is prime, then for any $x$ we have
$\gcd(n,x)=1$. Then by [Fermat's little theorem](https://en.wikipedia.org/wiki/Fermat%27s_little_theorem),
$x^{n-1}:=1 \bmod n$. (This is really a simple statement; if $n$ is
prime, then $n-1$ nonzero remainders modulo $n$: $1, 2, \ldots, n-1$ form a cyclic multiplicative group.) Therefore we pick some
"base" integer $x$ and compute $x^{n-1} \bmod n$; this is a quick
computation even if $n$ is large. If this value is not equal to 1 for
some base $x$, then $n$ is definitely not prime.  However, we
cannot test *every* base $x<n$; instead we test only some $x$, so
it may happen that we miss the right values of $x$ that would expose the
non-primality of $n$.  So Fermat's test sometimes fails, i.e. says
that $n$ is a prime when $n$ is in fact not a prime.  Also there
are infinitely many integers called [Carmichael numbers](https://en.wikipedia.org/wiki/Carmichael_number) which are not
prime but pass the Fermat test for every base.

The Miller-Rabin algorithm improves on this by using the property that
for prime $n$ there are no nontrivial square roots of unity in the
ring of integers modulo $n$ (this is Lagrange's theorem). In other
words, if $x^2:=1 \bmod n$ for some $x$, then $x$ must
be equal to 1 or -1 modulo $n$. (Since $n-1$ is equal to -1
modulo $n$, we have $n-1$ as a trivial square root of unity
modulo $n$.  Note that even if $n$ is prime there may be
nontrivial divisors of 1, for example, $2\dot49:=1 \bmod 97$.)

We can check that $n$ is odd before applying any primality test. (A
test $n^2:=1 \bmod 24$ guarantees that $n$ is not divisible by
2 or 3.  For large $n$ it is faster to first compute $n \bmod 24$
rather than $n^2$, or test $n$ directly.)  Then we note that in
Fermat's test the number $n-1$ is certainly a composite number because
$n-1$ is even. So if we first find the largest power of 2 in $n-1$
and decompose $n-1=2^rq$ with $q$ odd, then
$x^{n-1}:=a^{2^r}\bmod n$ where $a:=x^q \bmod n$. (Here
$r\ge 1$ since $n$ is odd.) In other words,
the number $x^{n-1}\bmod n$ is obtained by repeated squaring of the
number $a$.  We get a sequence of $r$ repeated squares:
$a, a^2,\ldots,a^{2^r}$.  The last element of this sequence must be 1 if
$n$ passes the Fermat test.  (If it does not pass, $n$ is
definitely a composite number.)  If $n$ passes the Fermat test, the
last-but-one element $a^{2^{r-1}}$ of the sequence of squares is a
square root of unity modulo $n$.  We can check whether this square
root is non-trivial (i.e. not equal to 1 or -1 modulo $n$). If it is
non-trivial, then $n$ definitely cannot be a prime. If it is trivial
and equal to 1, we can check the preceding element, and so on. If an
element is equal to -1, we cannot say anything, i.e. the test passes
($n$ is "probably a prime").

This procedure can be summarized like this:

1. Find the largest power of 2 in $n-1$ and an odd number $q$
   such that $n-1=2^rq$.
2. Select the "base number" $x<n$. Compute the sequence
   $a:=x^q \bmod n$, $a^2, a^4,\ldots, a^{2^r}$ by repeated
   squaring modulo $n$. This sequence contains at least two elements
   since $r\ge 1$.
3. If $a=1$ or $a=n-1$, the test passes on the base number
   $x$. Otherwise, the test passes if at least one of the elements of
   the sequence is equal to $n-1$ and fails if none of them are equal
   to $n-1$. This simplified procedure works because the first element
   that is equal to 1 *must* be preceded by a -1, or else we would find a
   nontrivial root of unity.

Here is a more formal definition. An odd integer $n$ is called
*strongly-probably-prime* for base $b$ if $b^q:=1 \bmod n$ or
$b^{q2^i}:=n-1 \bmod n$ for some $i$ such that $0\le i < r$,
where $q$ and $r$ are such that $q$ is odd and
$n-1 = q2^r$.

A practical application of this procedure needs to select particular
base numbers.  It is advantageous (according to
[pomerance1980:pseudoprimes](references.md) to choose *prime* numbers $b$
as bases, because for a composite base $b=pq$, if $n$ is
a strong pseudoprime for both $p$ and $q$, then it is very
probable that $n$ is a strong pseudoprime also for $b$,
so composite bases rarely give new information.

An additional check suggested by [davenport1992:primality_test_revisited](references.md)
is activated if $r>2$ (i.e. if $n:=1\bmod 8$ which is true for
only 1/4 of all odd numbers).  If $i\ge 1$ is found such that
$b^{q2^i}:=n-1\bmod n$, then $b^{q2^{i-1}}$ is a square root
of -1 modulo $n$. If $n$ is prime, there may be only two different
square roots of -1.  Therefore we should store the set of found values
of roots of -1; if there are more than two such roots, then we will find
some roots $s_1$, $s_2$ of -1 such that
$s_1+s_2\ne 0 \bmod n$. But $s_1^2-s_2^2:=0 \bmod n$.
Therefore $n$ is definitely composite, e.g. $\gcd(s_1+s_2,n)>1$.
This check costs very little computational effort but guards against some
strong pseudoprimes.

Yet another small improvement comes from [damgard1993:average_case_error](references.md).
They found that the strong primality test sometimes (rarely) passes on
composite numbers $n$ for more than 1/8 of all bases $x<n$
if $n$ is such that either $3n+1$ or $8n+1$ is a perfect square, or if $n$ is a Carmichael number. Checking Carmichael numbers is slow, but it is easy to show that if $n$ is a large enough prime number, then neither $3n+1$, nor $8n+1$, nor any $sn+1$ with small integer $s$ can be a perfect square.  [If $sn+1=r^2$, then $sn=(r-1)(r+1)$.]  Testing for a perfect square is quick and does not slow down the algorithm. This is however not implemented in Yacas because it seems that perfect squares are too rare for this improvement to be significant.

If an integer is not "strongly-probably-prime" for a given base $b$,
then it is a composite number.  However, the converse statement is
false, i.e. "strongly-probably-prime" numbers can actually be
composite.  Composite strongly-probably-prime numbers for base $b$ are
called *strong pseudoprimes* for base $b$. There is a theorem
that if $n$ is composite, then among all numbers $b$ such that
$1 < b < n$, at most one fourth are such that $n$ is a strong
pseudoprime for base $b$.  Therefore if $n$ is
strongly-probably-prime for many bases, then the probability for $n$
to be composite is very small.

For numbers less than $B=34155071728321$, exhaustive [#exhaustive]_
computations have shown that there are no strong
pseudoprimes simultaneously for bases 2, 3, 5, 7, 11, 13 and 17. This
gives a simple and reliable primality test for integers below $B$.
If $n\ge B$, the Rabin-Miller method consists of checking if
$n$ is strongly-probably-prime for $k$ base numbers $b$.
The base numbers are chosen to be consecutive "weak pseudoprimes" that are
easy to generate (see below the function `NextPseudoPrime`).

In the implemented routine `RabinMiller`, the number of bases $k$
is chosen to make the probability of erroneously passing the test
$p < 10^{-25}$. (Note that this is *not* the same as the probability
to give an incorrect answer, because all numbers that do not pass the
test are definitely composite.) The probability for the test to pass
mistakenly on a given number is found as follows.  Suppose the number
of bases $k$ is fixed. Then the probability for a given composite
number to pass the test is less than $p_f=4^{-k}$. The probability
for a given number $n$ to be prime is roughly
$p_p=\frac{1}{\ln{n}}$ and to be composite
$p_c=1-\frac{1}{\ln{n}}$. Prime numbers never fail the test.
Therefore, the probability for the test to pass is $p_fp_c+p_p$
and the probability to pass erroneously is

$$

$$

To make $p<\epsilon$, it is enough to select
$k=\frac{1}{\ln{4}(\ln{n}-\ln{\epsilon})}$.

Before calling `MillerRabin`, the function [IsPrime](../reference/number-theory.md#isprimen) performs two
quick checks: first, for $n\ge4$ it checks that $n$ is not divisible by
2 or 3 (all primes larger than 4 must satisfy this); second, for
$n>257$, it checks that $n$ does not contain small prime factors
$p\le257$.  This is checked by evaluating the GCD of $n$ with the
precomputed product of all primes up to 257.  The computation of the
GCD is quick and saves time in case a small prime factor is present.

There is also a function [NextPrime](../reference/number-theory.md#nextprimei) that returns the smallest
prime number larger than $n$.  This function uses a sequence
$5,7,11,13,\ldots$ generated by the function `NextPseudoPrime`.
This sequence contains numbers not divisible by 2 or 3 (but perhaps
divisible by 5,7,...). The function `NextPseudoPrime` is very fast
because it does not perform a full primality test.

The function [NextPrime](../reference/number-theory.md#nextprimei) however does check each of these pseudoprimes
using [IsPrime](../reference/number-theory.md#isprimen) and finds the first prime number.

## Factorization of integers

When we find from the primality test that an integer $n$ is composite,
we usually do not obtain any factors of $n$.  Factorization is
implemented by functions [Factor](../reference/number-theory.md#factorx) and [Factors](../reference/number-theory.md#factorsx).  Both functions
use the same algorithms to find all prime factors of a given integer $n$.
(Before doing this, the primality checking algorithm is used to detect
whether $n$ is a prime number.)  Factorization consists of repeatedly
finding a factor, i.e. an integer $f$ such that $n\bmod f=0$, and
dividing $n$ by $f$.  (Of course, each fastor $f$ needs to be
factorized too.)

### small prime factors

First we determine whether the number $n$ contains "small" prime
factors $p\le257$. A quick test is to find the GCD of $n$ and the
product of all primes up to 257: if the GCD is greater than 1, then
$n$ has at least one small prime factor. (The product of primes is
precomputed.) If this is the case, the trial division algorithm is
used: $n$ is divided by all prime numbers $p\le257$ until a factor is
found. `NextPseudoPrime` is used to generate the sequence of candidate
divisors $p$.

### checking for prime powers

After separating small prime factors, we test whether the number $n$
is an integer power of a prime number, i.e. whether $n=p^s$ for some
prime number $p$ and an integer $s\ge1$. This is tested by the
following algorithm. We already know that $n$ is not prime and that
$n$ does not contain any small prime factors up to 257. Therefore if
$n=p^s$, then $p>257$ and $2\le s<s_0=\frac{\ln{n}}{\ln{257}}$.
In other words, we only need to look for powers not greater than $s_0.$
This number can be approximated by the "integer logarithm" of $n$ in
base 257 (routine `IntLog(n, 257)`).

Now we need to check whether $n$ is of the form $p^s$ for
$s=2,3,\ldots,s_0$. Note that if for example $n=p^{24}$ for some
$p$, then the square root of $n$ will already be an integer,
$n^\frac{1}{2}=p^{12}$. Therefore it is enough to test whether
$n^\frac{1}{s}$ is an integer for all *prime* values of $s$ up to
$s_0$, and then we will definitely discover whether $n$ is a power
of some other integer. The testing is performed using the integer $n$-th
root function [IntNthRoot](../reference/numeric-programming.md#intnthrootx-n) which quickly computes the integer part of
$n$-th root of an integer number. If we discover that $n$ has an
integer root $p$ of order $s$, we have to check that $p$
itself is a prime power (we use the same algorithm recursively). The number
$n$ is a prime power if and only if $p$ is itself a prime power.
If we find no integer roots of orders $s\le s_0$, then $n$ is not
a prime power.

### Pollard's rho algorithm

If the number $n$ is not a prime power, the [Pollard's rho algorithm](https://en.wikipedia.org/wiki/Pollard%27s_rho_algorithm)
is applied [pollard1975:theory_algebraic_numbers](references.md). The Pollard rho
algorithm takes an irreducible polynomial, e.g. $p(x)=x^2+1$ and builds
a sequence of integers $x_{k+1}:=p(x_k)\bmod n$, starting from
$x_0=2$. For each $k$, the value $x_{2k}-x_k$ is attempted
as possibly containing a common factor with $n$. The GCD of
$x_{2k}-x_k$ with $n$ is computed, and if
$\gcd(x_{2k}-x_k,n)>1$, then that GCD value divides $n$.

The idea behind the rho algorithm is to generate an effectively
random sequence of trial numbers $t_k$ that may have a common factor
with $n$. The efficiency of this algorithm is determined by the size
of the smallest factor $p$ of $n$. Suppose $p$ is the
smallest prime factor of $n$ and suppose we generate a random sequence
of integers $t_k$ such that $1\leq t_k<n$. It is clear that, on
the average, a fraction $\frac{1}{p}$ of these integers will be divisible
by $p$. Therefore (if $t_k$ are truly random) we should need on
the average $p$ tries until we find $t_k$ which is accidentally
divisible by $p$. In practice, of course, we do not use a truly random
sequence and the number of tries before we find a factor $p$ may be
significantly different from $p$. The quadratic polynomial seems to
help reduce the number of tries in most cases.

But the Pollard "rho" algorithm may actually enter an infinite loop
when the sequence $x_k$ repeats itself without giving any factors of
$n$. For example, the unmodified rho algorithm starting from
$x_0=2$ loops on the number 703. The loop is detected by comparing
$x_{2k}$ and $x_k$. When these two quantities become equal to each
other for the first time, the loop may not yet have occurred so the
value of GCD is set to 1 and the sequence is continued. But when the
equality of $x_{2k}$ and $x_k$ occurs many times, it indicates that
the algorithm has entered a loop. A solution is to randomly choose a
different starting number $x_0$ when a loop occurs and try factoring
again, and keep trying new random starting numbers between 1 and $n$
until a non-looping sequence is found. The current implementation
stops after 100 restart attempts and prints an error message, "failed
to factorize number".

A better (and faster) integer factoring algorithm needs to be
implemented in Yacas.

### overview of algorithms

Modern factoring algorithms are all probabilistic (i.e. they do not
guarantee a particular finishing time) and fall into three categories:

1. Methods that work well (i.e. quickly) if there is a relatively
   small factor $p$ of $n$ (even if $n$ itself is large).
   Pollard's rho algorithm belongs to this category. The fastest in this
   category is [Lenstra's elliptic curves method](https://en.wikipedia.org/wiki/Lenstra_elliptic_curve_factorization) (ECM).
2. Methods that work equally quickly regardless of the size of
   factors (but slower with larger $n$). These are the continued
   fractions method and the various sieve methods. The current best
   is the [General Number Field Sieve](https://en.wikipedia.org/wiki/General_number_field_sieve) (GNFS) but it is quite a
   complicated algorithm requiring operations with high-order algebraic
   numbers. The next best one is the [Multiple Polynomial Quadratic Sieve](http://www.mersennewiki.org/index.php/Multiple_polynomial_quadratic_sieve) (MPQS).
3. Methods that are suitable only for numbers of special
   interesting form, e.g. Fermat numbers $2^{2^k}-1$ or generally
   numbers of the form $r^s+a$ where $s$ is large but $r$
   and $a$ are very small integers. The best method seems to be the
   [Special Number Field Sieve](https://en.wikipedia.org/wiki/Special_number_field_sieve) which is a faster variant of the GNFS adapted
   to the problem.

There is ample literature describing these algorithms.

## The Jacobi symbol

A number $m$ is a *quadratic residue modulo* $n$ if there exists a
number $k$ such that $k^2:=m\bmod n$.

The [Legendre symbol](https://en.wikipedia.org/wiki/Legendre_symbol) $(\frac{m}{n})$ is defined as $+1$ if
$m$ is a quadratic residue modulo $n$ and $-1$ if it is a
non-residue. The Legendre symbol is equal to $0$ if $\frac{m}{n}$
is an integer.

The `Jacobi symbol` $(\frac{m}{n})$ is defined as the product of the
Legendre symbols of the prime factors $f_i$ of
$n=f_1^{p_1}\ldots f_s^{p_s}$

$$

$$

(Here we used the same notation $(\frac{a}{b})$ for the Legendre and the
Jacobi symbols; this is confusing but seems to be the current practice.)  The
Jacobi symbol is equal to $0$ if $m$, $n$ are not mutually
prime (have a common factor). The Jacobi symbol and the Legendre symbol have
values $+1$, $-1$ or $0$.

The Jacobi symbol can be efficiently computed without knowing the full
factorization of the number $n$.  The currently used method is based
on the following identities for the Jacobi symbol:

1. $(\frac{a}{1}) = 1$,
2. $(\frac{2}{b}) = (-1)^{\frac{b^2-1}{8}}$,
3. $(\frac{ab}{c}) = (\frac{a}{c})(\frac{b}{c})$,
4. If $a:=b\bmod c$, then $(\frac{a}{c})=(\frac{b}{c})$,
5. If $a$, $b$ are both odd, then
   $(\frac{a}{b})=(\frac{b}{a}) (-1)^\frac{(a-1)(b-1)}{4}$.

Using these identities, we can recursively reduce the computation of
the Jacobi symbol $(\frac{a}{b})$ to the computation of the Jacobi
symbol for numbers that are on the average half as large.  This is similar
to the fast binary Euclidean algorithm for the computation of the GCD.  The
number of levels of recursion is logarithmic in the arguments $a$,
$b$.

More formally, Jacobi symbol $(\frac{a}{b})$ is computed by the following
algorithm.  (The number $b$ must be an odd positive integer, otherwise
the result is undefined.)

1. If $b=1$, return $1$ and stop. If $a=0$, return $0$
   and stop. Otherwise, replace $(\frac{a}{b})$ by
   $(\frac{a\bmod b}{b})$ (identity 4).
2. Find the largest power of $2$ that divides $a$. Say,
   $a=2^{sc}$ where $c$ is odd.  Replace $(\frac{a}{b})$ by
   $(\frac{c}{b})(-1)^\frac{s(b^2-1)}{8}$
   (identities 2 and 3).
3. Now that $c<b$, replace $(\frac{c}{b})$ by
   $(\frac{b}{c})(-1)^\frac{(b-1)(c-1)}{4}$ (identity 5).
4. Continue to step 1.

Note that the arguments $a$, $b$ may be very large integers and we
should avoid performing multiplications of these numbers.  We can
compute $(-1)^\frac{(b-1)(c-1)}{4}$ without multiplications. This
expression is equal to $1$ if either $b$ or $c$ is equal to 1
mod 4; it is equal to $-1$ only if both $b$ and $c$ are equal
to 3 mod 4. Also, $(-1)^\frac{b^2-1}{8}$ is equal to $1$ if either
$b=1$ or $b=7$ mod 8, and it is equal to $-1$ if $b=3$
or $b=5$ mod 8.  Of course, if $s$ is even, none of this needs
to be computed.

## Integer partitions

### partitions of an integer

A partition of an integer $n$ is a way of writing $n$ as the sum of
positive integers, where the order of these integers is unimportant.
For example, there are 3 ways to write the number 3 in this way:
$3=1+1+1$, $3=1+2$, $3=3$.  The function `PartitionsP`
counts the number of such partitions.

#### partitions of an integer by Rademacher-Hardy-Ramanujan series

Large $n$

The first algorithm used to compute this function uses the
Rademacher-Hardy-Ramanujan (RHR) theorem and is efficient for large
$n$.  (See for example [Ahlgren <i>et al.</i> 2001].)  The number of
partitions $P(n)$ is equal to an infinite sum:

$$

$$

where the functions $A$ and $S$ are defined as follows:

$$

$$

$$

$$

where $\delta_{x,y}$ is the Kronecker delta function (so that the
summation goes only over integers $l$ which are mutually prime with
$k$) and $B$ is defined by

$$

$$

The first term of the series gives, at large $n$, the Hardy-Ramanujan
asymptotic estimate,

$$

$$

The absolute value of each term decays quickly, so after $O(\sqrt{n})$
terms the series gives an answer that is very close to the integer result.

There exist estimates of the error of this series, but they are
complicated.  The series is sufficiently well-behaved and it is easier
to determine the truncation point heuristically.  Each term of the
series is either 0 (when all terms in $A(k,n)$ happen to cancel) or
has a magnitude which is not very much larger than the magnitude of
the previous nonzero term.  (But the series is not actually
monotonic.)  In the current implementation, the series is truncated
when $|A(k,n)S(n)\sqrt{k}|$ becomes smaller than $0.1$ for the
first time; in any case, the maximum number of calculated terms is
$5+\frac{\sqrt{n}}{2}$.  One can show that asymptotically for large
$n$, the required number of terms is less than
$\frac{\mu}{\ln{\mu}}$, where $\mu:=\pi\sqrt{\frac{2n}{3}}$.

[Ahlgren <i>et al.</i> 2001] mention that there exist explicit
constants $B_1$ and $B_2$ such that

$$

$$

The floating-point precision necessary to obtain the integer result
must be at least the number of digits in the first term $P_0(n)$, i.e.

$$

$$

However, `Yacas` currently uses the fixed-point precision model.
Therefore, the current implementation divides the series by $P_0(n)$
and computes all terms to $Prec$ digits.

The RHR algorithm requires $O\left(\left(\frac{n}{\ln(n)}\right)^\frac{3}{2}\right)$
operations, of which $O(\frac{n}{\ln(n)})$ are long multiplications at
precision $Prec\sim O(\sqrt{n})$ digits.  The computational cost is
therefore $O(\frac{n}{\ln(n)}M(\sqrt{n}))$.

#### partitions of an integer by recurrence relation

Small $n$

The second, simpler algorithm involves a recurrence relation

$$

$$

The sum can be written out as

$$

$$

where $1, 2, 5, 7, \ldots$ is the [generalized pentagonal sequence](https://oeis.org/A001318)
generated by the pairs $\frac{k(3k-1)}{2}$, $\frac{k(3k+1)}{2}$
for $k=1,2,\ldots$. The recurrence starts from $P_0=1$,
$P_1=1$.  (This is implemented as `PartitionsP'recur`.)

The sum is actually not over all $k$ up to $n$ but is truncated when
the pentagonal sequence grows above $n$.  Therefore, it contains only
$O(\sqrt{n})$ terms.  However, computing $P_n$ using the recurrence
relation requires computing and storing $P_k$ for all
$1\le k\le n$. No long multiplications are necessary, but the number
of long additions of numbers with $Prec\sim O(\sqrt{n})$ digits is
$O(n^\frac{3}{2})$. Therefore the computational cost is $O(n^2)$.
This is asymptotically slower than the RHR algorithm even if a slow
$O(n^2)$ multiplication is used. With internal Yacas math, the recurrence
relation is faster for $n<300$ or so, and for larger $n$ the RHR
algorithm is faster.

## Miscellaneous functions

### divisors

The function [Divisors](../reference/number-theory.md#divisorsn) currently returns the number of divisors of
integer, while [DivisorsSum](../reference/number-theory.md#divisorssumn) returns the sum of these divisors.  (The
current algorithms need to factor the number.) The following theorem
is used:

Let $p_1^{k_1}\ldots p_r^{k_r}$ be the prime factorization of $n$,
where $r$ is the number of prime factors and $k_i$ is the
multiplicity of the $i$-th factor. Then

$$

\mathrm{Divisors}(n) =(k_1+1)\ldots(k_r+1)

$$

$$
\mathrm{DivisorsSum}(n) = \frac{p_1^{k_1+1} -1}{p_1-1}\ldots\frac{p_r^{k_r+1} -1}{p_r-1}

$$

#### proper divisors

The functions [ProperDivisors](../reference/number-theory.md#properdivisorsn) and [ProperDivisorsSum](../reference/number-theory.md#properdivisorssumn) are
functions that do the same as the above functions, except they do not consider
the number $n$ as a divisor for itself.  These functions are defined
by:

$$

\mathrm{ProperDivisors}(n) := \mathrm{Divisors}(n) - 1;

$$

$$
\mathrm{ProperDivisorsSum}(n) := \mathrm{DivisorsSum}(n) - n;

$$

Another number-theoretic function is [Moebius](../reference/number-theory.md#moebiusn), defined as follows:
$\mathrm{Moebius}(n)=(-1)^r$ if no factors of $n$ are repeated,
$\mathrm{Moebius}(n)=0$ if some factors are repeated, and
$\mathrm{Moebius}(n)=1$ if $n = 1$. This again requires to factor
the number $n$ completely and investigate the properties of its prime
factors. From the definition, it can be seen that if $n$ is prime,
then $\mathrm{Moebius}(n) = -1$. The predicate [IsSquareFree](../reference/number-theory.md#issquarefreen) then
reducess to $\mathrm{Moebius}(n)\ne0$, which means that no factors of
$n$ are repeated.

## Gaussian integers

A *Gaussian integer* is a complex number of the form $z = a+b*\imath$, where $a$ and $b$ are ordinary (rational) integers.
[#rational_integers]_ The ring of Gaussian integers is usually denoted by
$\mathbb{Z}[\imath]$ in the mathematical literature. It is an example
of a ring of algebraic integers.

   integers, the ordinary integers (with no imaginary part) are called
   *rational integers*.

The function [GaussianNorm](../reference/number-theory.md#gaussiannormz) computes the norm $N(z)=a^2+b^2$ of
$z$. The norm plays a fundamental role in the arithmetic of Gaussian
integers, since it has the multiplicative property

$$

$$

A unit of a ring is an element that divides any other element of the
ring.  There are four units in the Gaussian integers: $1$, $-1$,
$\imath$, $-\imath$. They are exactly the Gaussian integers whose
norm is $1$. The predicate [IsGaussianUnit](../reference/number-theory.md#isgaussianunitz) tests for a Gaussian
unit.

Two Gaussian integers $z$ and $w$ are *associated* is
$\frac{z}{w}$ is a unit. For example, $2+\imath$ and
$-1+2\imath$ are associated.

A Gaussian integer is called *prime* if it is only divisible by the
units and by its associates. It can be shown that the primes in the
ring of Gaussian integers are:

1. $1+\imath$ and its associates.
2. The rational (ordinary) primes of the form $4n+3$.
3. The factors $a+b\imath$ of rational primes $p$ of the form
   $p=4n+1$, whose norm is $p=a^2+b^2$.

For example, $7$ is prime as a Gaussian integer, while $5$ is not,
since $5 = (2+\imath)(2-\imath)$.  Here $2+\imath$ is a Gaussian
prime.

### Factors

The ring of Gaussian integers is an example of an Euclidean ring,
i.e. a ring where there is a division algorithm.  This makes it
possible to compute the greatest common divisor using Euclid's
algorithm. This is what the function [GaussianGcd](../reference/number-theory.md#gaussiangcdzw) computes.

As a consequence, one can prove a version of the fundamental theorem
of arithmetic for this ring: The expression of a Gaussian integer as a
product of primes is unique, apart from the order of primes, the
presence of units, and the ambiguities between associated primes.

The function [GaussianFactors](../reference/number-theory.md#gaussianfactorsz) finds this expression of a Gaussian
integer $z$ as the product of Gaussian primes, and returns the result
as a list of pairs $(p,e)$, where $p$ is a Gaussian prime and
$e$ is the corresponding exponent.  To do that, an auxiliary function
called `GaussianFactorPrime` is used. This function finds a factor of a
rational prime of the form $4n+1$. We compute $a := (2n)!\bmod p$.
By Wilson's theorem $a^2$ is congruent to $-1$ (mod $p$),
and it follows that $p$ divides $(a+\imath)(a-\imath)=a^2+1$ in
the Gaussian integers. The desired factor is then the [GaussianGcd](../reference/number-theory.md#gaussiangcdzw) of
$a+\imath$ and $p$. If the result is $a+b\imath$, then
$p=a^2+b^2$.

If $z$ is a rational (i.e. real) integer, we factor $z$ in the
Gaussian integers by first factoring it in the rational integers, and
after that by factoring each of the integer prime factors in the
Gaussian integers.

If $z$ is not a rational integer, we find its possible Gaussian prime
factors by first factoring its norm $N(z)$ and then computing the
exponent of each of the factors of $N(z)$ in the decomposition of
$z$.

## A simple factorization algorithm for univariate polynomials

This section discusses factoring polynomials using arithmetic modulo
prime numbers. Information was used from
[knuth1997:acp_seminumerical_algorithms](references.md) and
[davenport1988:computer_algebra](references.md).

A simple factorization algorithm is developed for univariate
polynomials. This algorithm is implemented as the function
`BinaryFactors`. The algorithm was named the binary factoring
algorithm since it determines factors to a polynomial modulo $2^n$ for
successive values of $n$, effectively adding one binary digit to the
solution in each iteration. No reference to this algorithm has been
found so far in literature.

Berlekamp showed that polynomials can be efficiently factored when
arithmetic is done modulo a prime. The [Berlekamp algorithm](https://en.wikipedia.org/wiki/Berlekamp%27s_algorithm) is only
efficient for small primes, but after that [Hensel lifting](https://en.wikipedia.org/wiki/Hensel%27s_lemma) can be used
to determine the factors modulo larger numbers.

The algorithm presented here is similar in approach to applying the
Berlekamp algorithm to factor modulo a small prime, and then factoring
modulo powers of this prime (using the solutions found modulo the
small prime by the Berlekamp algorithm) by applying Hensel lifting.
However it is simpler in set up. It factors modulo 2, by trying all
possible factors modulo 2 (two possibilities, if the polynomial is
monic). This performs the same action usually left to the Berlekamp
step. After that, given a solution modulo $2^n$, it will test for a
solution $f_i$ modulo $2^n$ if $f_i$ or $f_i + 2^n$
are a solution modulo $2^{n+1}$.

This scheme raises the precision of the solution with one digit in
binary representation. This is similar to the linear Hensel lifting
algorithm, which factors modulo $p^n$ for some prime $p$, where
$n$ increases by one after each iteration. There is also a quadratic
version of Hensel lifting which factors modulo $p^{2^n}$, in effect
doubling the number of digits (in $p$-adic expansion) of the solution
after each iteration. However, according to Davenport, the quadratic
algorithm is not necessarily faster.

The algorithm here thus should be equivalent in complexity to Hensel
lifting linear version. This has not been verified yet.

## Modular arithmetic

This section copies some definitions and rules from <I>The Art of
Computer Programming, Volume 1, Fundamental Algorithms </I> regarding
arithmetic modulo an integer.

Arithmetic modulo an integer $p$ requires performing the arithmetic
operation and afterwards determining that integer modulo $p$. A number
$x$ can be written as

$$

$$

where $q$ is called the quotient, and $r$ remainder.  There is some
liberty in the range one chooses $r$ to be in. If $r$ is an integer
in the range $0,1,\ldots,p-1$ then it is the *modulo*,
$r = x \bmod p$.

When $x\bmod p = y\bmod p$, the notation $x=y\pmod p$ is used. All
arithmetic calculations are done modulo an integer $p$ in that case.

For calculations modulo some $p$ the following rules hold:

* If $a=b\pmod p$ and $x=y\pmod p$, then $ax=by\pmod p$,
  $a+x=b+y\pmod p$, and $a-x=b-y\pmod p$.  This means that for
  instance also $x^n\bmod p = (x\bmod p)^n\bmod p$.
* Two numbers $x$ and $y$ are *relatively prime* if they don't
  share a common factor, that is, if their greatest common denominator
  is one, $\gcd(x,y)=1$.
* If $ax=by\pmod p$ and if $a=b\pmod p$, and if $a$ and
  $p$ are relatively prime, then $x=y\pmod p$.  This is useful
  for dividing out common factors.
* $a=b\pmod p$ if and only if $an=bn\pmod np$ when $n\ne0$.
  Also, if $r$ and $s$ are relatively prime, then
  $a=b\pmod rs$ only if $a=b\pmod r$ and $a=b\pmod s$.
  These rules are useful when the modulus is changed.

For polynomials $v_1(x)$ and $v_2(x)$ it further holds that

$$

$$

This follows by writing out the expression, noting that the binomial
coefficients that result are multiples of $p$, and thus their value
modulo $p$ is zero ($p$ divides these coefficients), so only the
two terms on the right hand side remain.

### Some corollaries

One corollary of the rules for calculations modulo an integer is
[Fermat's little theorem](https://en.wikipedia.org/wiki/Fermat%27s_little_theorem): if $p$ is a prime number then
$a^p=a\pmod p$ for all integers $a$ (for a proof, see Knuth).

An interesting corollary to this is that, for some prime integer $p$:

$$

$$

This follows from writing it out and using Fermat's little theorem to replace
$a^p$ with $a$ where appropriate (the coefficients to the
polynomial when written out, on the left hand side).

### Factoring using modular arithmetic

The task is to factor a polynomial

$$

$$

into a form

Where $f_i(x)$ are irreducible polynomials of the form:

$$

$$

The part that could not be factorized is returned as $g(x)$,
with a possible constant factor $C$.

The factors $f_i(x)$ and $g(x)$ are determined uniquely by
requiring them to be monic. The constant $C$ accounts for a common factor.

The $c_i$ constants in the resulting solutions $f_i(x)$ can be
rational numbers (or even complex numbers, if Gaussian integers
are used).

### Preparing the polynomial for factorization

The final factoring algorithm needs the input polynomial to be monic
with integer coefficients (a polynomial is monic if its leading
coefficient is one). Given a non-monic polynomial with rational
coefficients, the following steps are performed:

Convert polynomial with rational coefficients to polynomial with integer coefficients

First the least common multiple $lcm$ of the denominators of the
coefficients $p(x)$ has to be found, and the polynomial is multiplied
by this number.  Afterwards, the $C$ constant in the result should
have a factor $\frac{1}{lcm}$.

The polynomial now only has integer coefficients.

### Convert polynomial to a monic polynomial

The next step is to convert the polynomial to one where the leading
coefficient is one. In order to do so, following "Davenport", the
following steps have to be taken:

1. Multiply the polynomial by $a_n^{n-1}$
2. Perform the substitution $x=\frac{y}{a_n}$

The polynomial is now a monic polynomial in $y$.

After factoring, the irreducible factors of $p(x)$  can be obtained by
multiplying $C$ with $\frac{1}{a_n^{n-1}}$, and replacing
$y$ with $a_nx$. The irreducible solutions $a_nx+c_i$
can be replaced by $x+\frac{c_i}{a_i}$ after multiplying $C$
by $a_n$, converting the factors to monic factors.

After the steps described here the polynomial is now monic with
integer coefficients, and the factorization of this polynomial can be
used to determine the factors of the original polynomial $p(x)$.

### Definition of division of polynomials

To factor a polynomial a division operation for polynomials modulo
some integer is needed. This algorithm needs to return a quotient
$q(x)$ and remainder $r(x)$ such that:

$$

$$

for some polymomial $d(x)$ to be divided by, modulo some integer
$p$. $d(x)$ is said to divide $p(x)$ (modulo $p$)
if $r(x)$ is zero.  It is then a factor modulo $p$.

For binary factoring algorithm it is important that if some monic
$d(x)$ divides $p(x)$, then it also divides $p(x)$ modulo
some integer $p$.

Define $\mathrm{deg}(f(x))$ to be the [degree](https://en.wikipedia.org/wiki/Degree_of_a_polynomial) of $f(x)$ and
$\mathrm{lc}(f(x))$ to be the leading coefficient of $f(x)$.
Then, if $\mathrm{deg}(p(x))\ge \mathrm{deg}(d(x))$, one
can compute an integer $s$ such that

$$

$$

If $p$ is prime, then

$$

$$

Because $a^{p-1} = 1\pmod p$ for any $a$. If $p$ is not
prime but $d(x)$ is monic (and thus $\mathrm{lc}(d(x)) = 1$,

$$

$$

This identity can also be used when dividing in general (not modulo
some integer), since the divisor is monic.

The quotient can then be updated by adding a term:

$$

$$

and updating the polynomial to be divided, $p(x)$, by subtracting
$d(x)term$. The resulting polynomial to be divided now has a degree
one smaller than the previous.

When the degree of $p(x)$ is less than the degree of $d(x)$ it is
returned as the remainder.

A full division algorithm for arbitrary integer $p>1$ with
$\mathrm{lc}(d(x)) = 1$ would thus look like:

	divide(p(x),d(x),p)
	   q(x) = 0
	   r(x) = p(x)
	   while (deg(r(x)) >= deg(d(x)))
	      s = lc(r(x))
	      term = s*x^(deg(r(x))-deg(d(x)))
	      q(x) = q(x) + term
	      r(x) = r(x) - term*d(x) mod p
	   return (q(x),r(x))

The reason we can get away with factoring modulo $2^n$ as opposed to
factoring modulo some prime $p$ in later sections is that the divisor
$d(x)$ is monic. Its leading coefficient is one and thus $q(x)$ and
$r(x)$ can be uniquely determined. If $p$ is not prime and
$\mathrm{lc}(d(x))$ is not equal to one, there might be multiple
combinations for which $p(x) = q(x)d(x)+r(x)$, and we are interested
in the combinations where $r(x)$ is zero. This can be costly to determine
unless $(q(x),r(x))$ is unique.  This is the case here because we are
factoring a monic polynomial, and are thus only interested in cases where
$\mathrm{lc}(d(x)) = 1$.

### Determining possible factors modulo 2

We start with a polynomial $p(x)$ which is monic and has integer
coefficients.

It will be factored into a form:

$$

$$

where all factors $f_i(x)$ are monic also.

The algorithm starts by setting up a test polynomial, $p_{test}(x)$
which divides $p(x)$, but has the property that

$$

$$

Such a polynomial is said to be *square-free*.  It has the same
factors as the original polynomial, but the original might have
multiple of each factor, where $p_{test}(x)$ does not.

The square-free part of a polynomial can be obtained as follows:

$$

$$

It can be seen by simply writing this out that $p(x)$ and
$\frac{d}{dx}p(x)$ will have factors $f_i(x)^{p_i-1}$ in common.
these can thus be divided out.

It is not a requirement of the algorithm that the algorithm being
worked with is square-free, but it speeds up computations to work with
the square-free part of the polynomial if the only thing sought after
is the set of factors. The multiplicity of the factors can be
determined using the original $p(x)$.

Binary factoring then proceeds by trying to find potential solutions
modulo $p=2$ first. There can only be two such solutions: $x+0$
and $x+1$.

A list of possible solutions $L$ is set up with potential solutions.

### Determining factors modulo $2^n$ given a factorization modulo 2

At this point there is a list $L$ with solutions modulo $2^n$ for
some $n$. The solutions will be of the form: $x+a$. The first step
is to determine if any of the elements in $L$ divides $p(x)$ (not
modulo any integer).  Since $x+a$ divides $p_{test}(x)$ modulo
$2^n$, both $x+a$ and $x+a-2^n$ have to be checked.

If an element in $L$ divides $p_{test}(x)$, $p_{test}(x)$
is divided by it, and a loop is entered to test how often it divides
$p(x)$ to determine the multiplicity $p_i$ of the factor.
The found factor $f_i(x) = x+c_i$ is added as a combination
$(x+c_i, p_i)$. $p(x)$ is divided by $f_i(x)^p_i$.

At this point there is a list $L$ of factors that divide
$p_{test}(x)$ modulo $2^n$. This implies that for each of the
elements $u$ in $L$, either $u$ or $u+2^n$ should
divide $p_{test}(x)$ modulo $2^{n+1}$. The following step is thus
to set up a new list with new elements that divide $p_{test}(x)$
modulo $2^{n+1}$.

The loop is re-entered, this time doing the calculation modulo
$2^{n+1}$ instead of modulo $2^n$.

The loop is terminated if the number of factors found equals
$\mathrm{deg}(p_{test}(x))$, or if $2^n$ is larger than the
smallest non-zero coefficient of $p_test(x)$ as this smallest non-zero
coefficient is the product of all the smallest non-zero coefficients of the
factors, or if the list of potential factors is zero.

The polynomial $p(x)$ can not be factored any further, and is added as
a factor $(p(x), 1)$.

The function `BinaryFactors`, yields the following interaction in Yacas:

  In> BinaryFactors((x+1)^4*(x-3)^2)
  Out> {{x-3,2},{x+1,4}}
  In> BinaryFactors((x-1/5)*(2*x+1/3))
  Out> {{2,1},{x-1/5,1},{x+1/6,1}}
  In> BinaryFactors((x-1123125)*(2*x+123233))
  Out> {{2,1},{x-1123125,1},{x+123233/2,1}}

The binary factoring algorithm starts with a factorization modulo 2,
and then each time tries to guess the next bit of the solution,
maintaining a list of potential solutions.  This list can grow
exponentially in certain instances.  For instance, factoring
$(x-a)(x-2a)(x-3a)\ldots(x-na)$ implies a that the roots have common
factors. There are inputs where the number of potential solutions
(almost) doubles with each iteration.  For these inputs the algorithm
becomes exponential. The worst-case performance is therefore
exponential. The list of potential solutions while iterating will
contain a lot of false roots in that case.

### Efficiently deciding if a polynomial divides another

Given the polynomial $p(x)$, and a potential divisor

$$

$$

modulo some $q=2^n$ an expression for the remainder after division
is

$$

$$

For the initial solutions modulo 2, where the possible solutions are
$x$ and $x-1$. For $p=0$, $\mathrm{rem}(0) = a_0$.
For $p=1$, $\mathrm{rem}(1) = \sum_{i=0}^na_i$.

Given a solution $x-p$ modulo $q=2^n$, we consider the possible
solutions $x-p\bmod 2^{n+1}$ and $x-(p+2^n)\bmod 2^{n+1}$.

$x-p$ is a possible solution if $\mathrm{rem}(p)\bmod 2^{n+1} = 0$.

$x-(p+q)$ is a possible solution if
$\mathrm{rem}(p+q)\bmod 2^{n+1} = 0$. Expanding
$\mathrm{rem}(p+q)\bmod 2q$ yields:

$$

$$

When expanding this expression, some terms grouped under
$\mathrm{extra}(p,q)$ have factors like $2q$ or $q^2$.
Since $q=2^n$, these terms vanish if the calculation is done modulo
$2^{n+1}$.

The expression for $\mathrm{extra}(p,q)$ then becomes

$$

$$

An efficient approach to determining if $x-p$ or $x-(p+q)$ divides
$p(x)$ modulo $2^{n+1}$ is then to first calculate
$\mathrm{rem}(p)\bmod 2q$. If this is zero, $x-p$ divides
$p(x)$. In addition, if
$\mathrm{rem}(p)+\mathrm{extra}(p,q)\bmod 2q$ is zero, $x-(p+q)$
is a potential candidate.

Other efficiencies are derived from the fact that the operations are
done in binary. Eg. if $q=2^n$, then $q_{next}=2^{n+1} = 2q = q<<1$
is used in the next iteration. Also, calculations modulo $2^n$ are
equivalent to performing a bitwise and with $2^n-1$. These operations
can in general be performed efficiently on todays hardware which is
based on binary representations.

### Extending the algorithm

Only univariate polynomials with rational coefficients have been
considered so far. This could be extended to allow for roots that are
complex numbers $a+b\imath$ where both $a$ and $b$ are
rational numbers.

For this to work the division algorithm would have to be extended to
handle complex numbers with integer $a$ and $b$ modulo some
integer, and the initial setup of the potential solutions would have to be
extended to try $x+1+\imath$ and $x+\imath$ also. The step
where new potential solutions modulo $2^{n+1}$ are determined should
then also test for $x+2^n\imath$ and $x+2^n+2^n\imath$.

The same extension could be made for multivariate polynomials,
although setting up the initial irreducible polynomials that divide
$p_{test}(x)$ modulo 2 might become expensive if done on a polynomial
with many variables ($2^{2^m-1}$ trials for $m$ variables).

Lastly, polynomials with real-valued coefficients *could* be
factored, if the coefficients were first converted to rational
numbers. However, for real-valued coefficients there exist other
methods (Sturm sequences).

### Newton iteration

What the `BinaryFactor` algorithm effectively does is finding a set of
potential solutions modulo $2^{n+1}$ when given a set of potential
solutions modulo $2^n$.  There is a better algorithm that does
something similar: Hensel lifting. Hensel lifting is a generalized
form of Newton iteration, where given a factorization modulo $p$, each
iteration returns a factorization modulo $p^2$.

Newton iteration is based on the following idea: when one takes a
Taylor series expansion of a function:

$$

$$

Newton iteration then proceeds by taking only the first two terms in
this series, the constant plus the constant times $dx$. Given some
good initial value $x_0$, the function will is assumed to be close to
a root, and the function is assumed to be almost linear, hence this
approximation.  Under these assumptions, if we want $f(x_0+dx)$ to be
zero,

$$

$$

This yields:

$$

$$

And thus a next, better, approximation for the root is

$$

$$

or more general:

$$

$$

If the root has multiplicity one, a Newton iteration can converge
*quadratically*, meaning the number of decimals precision for
each iteration doubles.

As an example, we can try to find a root of $\sin x$ near
$3$, which should converge to $\pi$.

Setting precision to 30 digits,:

  In> Builtin'Precision'Set(30)
  Out> True;

We first set up a function $dx(x)$:

  In> dx(x):=Eval(-Sin(x)/(D(x)Sin(x)))
  Out> True;

And we start with a good initial approximation to $\pi$, namely
$3$. Note we should set `x` *after* we set `dx(x)`, as the right
hand side of the function definition is evaluated. We could also have
used a different parameter name for the definition of the function
$dx(x)$:

  In> x:=3
  Out> 3;

We can now start the iteration:

  In> x:=N(x+dx(x))
  Out> 3.142546543074277805295635410534;
  In> x:=N(x+dx(x))
  Out> 3.14159265330047681544988577172;
  In> x:=N(x+dx(x))
  Out> 3.141592653589793238462643383287;
  In> x:=N(x+dx(x))
  Out> 3.14159265358979323846264338328;
  In> x:=N(x+dx(x))
  Out> 3.14159265358979323846264338328;

As shown, in this example the iteration converges quite quickly.

### Finding roots of multiple equations in multiple variables using Newton iteration

One generalization, mentioned in W.H. Press et al., <i>NUMERICAL
RECIPES in C, The Art of Scientific computing</i> is finding roots for
multiple functions in multiple variables.

Given $N$ functions in $N$ variables, we want to solve

$$

$$

for $i = 1,\ldots N$. If de denote by $X$ the vector

$$

$$

and by $dX$ the delta vector, then one can write

$$

$$

Setting $f_i(X+dX)$ to zero, one obtains

\sum_{j=1}^Na_{ij}dx_j = b_i

where

$$

$$

and

$$

$$

So the generalization is to first initialize $X$ to a good initial
value, calculate the matrix elements $a_{ij}$ and the vector $b_i$,
and then to proceed to calculate $dX$ by solving the matrix equation,
and calculating

$$

$$

In the case of one function with one variable, the summation reduces
to one term, so this linear set of equations was a lot simpler in that
case. In this case we will have to solve this set of linear equations
in each iteration.

As an example, suppose we want to find the zeroes for the following
two functions:

$$

$$

and

$$

$$

It is clear that the solution to this is $a=2$ and
$x:=N\frac{\pi}{2}$ for any integer value $N$.

We will do calculations with precision 30:

  In> Builtin'Precision'Set(30)
  Out> True;

And set up a vector of functions $(f_1(X),f_2(X))$
where $X:=(a,x)$:

  In> f(a,x):={Sin(a*x),a-2}
  Out> True;

Now we set up a function `matrix(a,x)` which returns the
matrix $a_{ij}$:

  In> matrix(a,x):=Eval({D(a)f(a,x),D(x)f(a,x)})
  Out> True;

We now set up some initial values:

  In> {a,x}:={1.5,1.5}
  Out> {1.5,1.5};

The iteration converges a lot slower for this example, so we
will loop 100 times:

  In> For(ii:=1,ii<100,ii++)[{a,x}:={a,x}+\
        N(SolveMatrix(matrix(a,x),-f(a,x)));]
  Out> True;
  In> {a,x}
  Out> {2.,0.059667311457823162437151576236};

The value for $a$ has already been found. Iterating a
few more times:

  In> For(ii:=1,ii<100,ii++)[{a,x}:={a,x}+\
        N(SolveMatrix(matrix(a,x),-f(a,x)));]
  Out> True;
  In> {a,x}
  Out> {2.,-0.042792753588155918852832259721};
  In> For(ii:=1,ii<100,ii++)[{a,x}:={a,x}+\
	   N(SolveMatrix(matrix(a,x),-f(a,x)));]
  Out> True;
  In> {a,x}
  Out> {2.,0.035119151349413516969586788023};

the value for $x$ converges a lot slower this time, and to the
uninteresting value of zero (a rather trivial zero of this set of
functions).  In fact for all integer values $N$ the value
$\frac{N\pi}{2}$ is a solution.  Trying various initial values will
find them.

### Newton iteration on polynomials

von zur Gathen *et al.*, [gathen1999:modern_computer_algebra](references.md) discusses
taking the inverse of a polynomial using Newton iteration.  The task is,
given a polynomial $f(x)$, to find a polynomial $g(x)$ such that
$f(x) = \frac{1}{g(x)}$, modulo some power in $x$.  This implies
that we want to find a polynomial $g$ for which:

$$

$$

Applying a Newton iteration step $g_{i+1} = g_i - \frac{h(g_i)}{\frac{d}{dg}h(g_i)}$ to this expression yields:

$$

$$

von zur Gathen then proves by induction that for $f(x)$ monic, and
thus $f(0)=1$, given initial value $g_0(x) = 1$, that

$$

$$

Example:

suppose we want to find the polynomial $g(x)$ up to the 7-th degree
for which $f(x)g(x) = 1\pmod{x^8}$, for the function

$$

$$

First we define the function f:

  In> f:=1+x+x^2/2+x^3/6+x^4/24
  Out> x+x^2/2+x^3/6+x^4/24+1

And initialize $g$ and $i$:

  In> g:=1
  Out> 1
  In> i:=0
  Out> 0

Now we iterate, increasing $i$, and replacing $g$ with the
new value for $g$:

  In> [i++;g:=BigOh(2*g-f*g^2,x,2^i);]
  Out> 1-x;
  In> [i++;g:=BigOh(2*g-f*g^2,x,2^i);]
  Out> x^2/2-x^3/6-x+1;
  In> [i++;g:=BigOh(2*g-f*g^2,x,2^i);]
  Out> x^7/72-x^6/72+x^4/24-x^3/6+x^2/2-x+1;

The resulting expression must thus be:

$$

$$

We can easily verify this:

  In> Expand(f*g)
  Out> x^11/1728+x^10/576+x^9/216+(5*x^8)/576+1

This expression is 1 modulo $x^8$, as can easily be shown:

  In> BigOh(%,x,8)
  Out> 1;
