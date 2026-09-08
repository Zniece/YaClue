# Transforms

> **Implementation scope:** This page explains algorithms behind the standard scripts and may contain historical approaches or old measurements. Rust and `.yts` tests determine current behavior; performance claims require new measurements on the current implementation.


Currently the only tranform defined is `LaplaceTransform`, which has
the calling convention:

  LaplaceTransform(var1,var2,func)

It has been setup much like the integration algorithm. If the
transformation algorithm cannot perform the transform, the expression
(in theory) is returned unsimplified. Some cases may still erroneously
return `Undefined` or `Infinity`.

## The `LaplaceTransform` algorithm

This section describes the steps taken in doing a Laplace
transform.

### General structure

`LaplaceTransform` is immediately handed off to `LapTran`.  This is
done because if the last `LapTran` rule is met, the Laplace transform
couldn't be found and it can then return `LaplaceTransform`
unevaluated.

### Operational properties

The first rules that are matched against utilize the various
operational properties of `LaplaceTransform`, such as:

* Linearity Properties
* Shift properties, i.e. multiplying the function by an exponential
* $\mathcal{L}\lbrace yx^n\rbrace = (-1)^n \frac{d^n}{dx^n} \mathcal{L}\lbrace y\rbrace$
* $\mathcal{L}\lbrace \frac{y}{x}\rbrace = \int_s^\infty\mathcal{L}\lbrace y\rbrace(\sigma)d\sigma$

The last operational property dealing with integration is not yet
fully bug-tested, it sometimes returns `Undefined` or `Infinity` if
the integral returns such.

### Transform tables

For elementary functions, Yacas uses transform tables. For instance,
the fact that the Laplace transform of $cos(t)$ is
$\frac{s}{s^2+1}$ is declared in a transform table.

For the purpose of setting up the transform table, a few declaration
functions have been defined, which use some generalized pattern
matchers to be more flexible in recognizing expressions that are
transformable.

### Transforming simple functions

For functions like $\sin(t)$ the transform can be declared with the
function `LapTranDef`.

The calling sequence for `LapTranDef` is:

  LapTranDef(in, out)

Currently `in` must be a variable of `_t` and `out` must be a function
of `s`.  For instance, for the function $\cos(t)$ there is a
declaration:

  LapTranDef(Cos(_t), s/(s^2+1));

The fact that the first argument is a pattern means that each
occurrence of the variable to be matched should be referred to as
`_t`, as in the example above.

`LapTranDef` generalizes the transform implicitly, in that it will set
up the system to actually recognize expressions of the form $\cos(at)$
and $\cos(\frac{t}{a})$ , and return the appropriate answer.  The way
this is done is by three separate rules for case of `t` itself, `a*t` and
`t/a`. This is similar to the [MatchLinear](../reference/predicates.md#matchlinearxexpr) function that
`Integrate` uses, except `LaplaceTransforms` must have `b=0`.

### Further Directions

Currenlty $\sin(t)\cos(t)$ cannot be transformed, because it requires
a convolution integral. This will be implemented soon. The inverse
Laplace transform will be implement soon also.
