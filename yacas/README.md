# yacas — the CAS kernel tree

This directory is **yacas-rs**: a general-purpose computer algebra system —
a fork of [Yacas](https://github.com/grzegorzmazur/yacas) (1.9.x), kept in
the same layout as upstream — with one deliberate substitution: **the C++
engine has been replaced by a Rust engine**. It is an independent component:
it computes, it knows nothing about YaClue's step-by-step product layer.

```
yacas-rs/    The engine, in Rust (MIT). Independent implementation; behavior
             covered by its Rust conformance test suite.
scripts/     The standard script library (LGPL-2.1+). Semantics follow
             upstream; the fork applies its own maintenance fixes.
tests/       .yts behavior specs for the script library (LGPL-2.1+).
COPYING      LGPL-2.1+ license text (upstream, kept verbatim).
AUTHORS      Upstream authors (kept verbatim).
```

What is *not* here anymore: the upstream C++ core (`cyacas/`), the upstream
GUI, docs tree and build system. The C++ sources remain available upstream;
YaClue does not build or ship them.

License: `scripts/` and `tests/` are derived from the upstream library and
remain LGPL-2.1+; the engine `yacas-rs/` is MIT and never embeds script
contents. See the root README for the full license map.

## Script maintenance

The library retains its upstream origin and applies local maintenance fixes.
Maintained areas currently include `scripts/integrate.rep/code.ys`,
`scripts/stdarith.ys`, and `scripts/newly.rep/code.ys`; review their Git history
for changes. Upstream behavior is a compatibility reference. Intentional
improvements are defined by this project's tests and documented contracts.

## Rust engine extensions

Each `Environment` owns an isolated assumption context. Rust callers can use
`Environment::assume`, `is_assumed`, and `clear_assumptions`; the language
entry points are `Assume(symbol, fact)`, `IsAssumed(symbol, fact)`, and
`ClearAssumptions()`. The initial facts are `Real`, `Integer`, `Positive`,
`Negative`, and `NonZero`. Safe implications are recorded automatically:
integers are real, while positive and negative symbols are real and nonzero.
Contradictory signs are rejected.

Script rules use `IsAssumedValue(parameter, fact)`, which resolves a locally
bound pattern parameter before querying it. `IsAssumed` deliberately holds the
public symbol name, so assigning a value to that symbol does not silently make
its stored assumptions inaccessible.

These facts are queried explicitly. Legacy value predicates such as
`IsInteger(n)` still mean that the evaluated node is a concrete integer; they
do not consume assumptions. Mathematical rules must opt in through
`IsAssumed` so they cannot accidentally perform numeric operations on a
symbolic value.

Assumption-aware rules keep their dependency in the expression as
`ConditionalValue(value, {{symbol, fact}, ...})`. Product adapters may unwrap
this internal carrier for display while retaining the condition metadata. The
first consumer is the positive/negative parameter branch of
`Limit(x, Infinity) x^n/Ln(x)`.

The script library exposes proof-oriented predicates `IsKnownInteger`,
`IsKnownReal`, `IsKnownPositive`, `IsKnownNegative`, and `IsKnownNonZero`.
They combine exact values and assumptions through safe rules for negation,
sums, products, quotients, integer powers, `Exp`, `Ln`, `Sqrt`, and `Abs`.
`False` means the property was not proved; it does not prove the opposite.
