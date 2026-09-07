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
