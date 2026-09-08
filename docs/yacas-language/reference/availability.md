# Function Availability Audit

This page explains how to interpret the detailed function reference. The manual contains current core commands and standard-script entries as well as a few historical entries and package helpers. Its length is not a capability count.

## Structural audit: 2026-09-09

The audit created an `Environment` with current `yacas-rs`, loaded `yacasinit.ys`, and compared reference headings with the core registry, startup rule bases, and deferred-load `.def` tables.

| Item | Count |
|---|---:|
| Level-three reference headings | 523 |
| Headings recognizable as functions or operators | 507 |
| Names directly discoverable after full startup | 503 |
| Headings not directly discovered | 4 |

After obsolete interfaces were removed, only four package-local names are not directly discoverable at startup.

## Historical entries not provided

- `Factorize`: use the standard-script `Factor` or a specific polynomial interface.
- `ExtraInfo'Set`: historical object metadata not exposed by the current expression model.
- `MathSinh`, `MathCosh`, `MathTanh`, and the three hyperbolic `MathArc*` functions: use public standard-script functions.
- `IsPromptShown` and `ReadCmdLineString`: historical console hooks; the Rust host now supplies input.
- `GetTime`: historical interpreter timing; use Rust benchmarks or host timing.

These names remain here only as migration guidance and are not required interfaces.

## Names available after package loading

- The graph operator `->` is declared by the graph package.
- `OrthoPoly` and `OrthoPolySum` are internal orthogonal-polynomial helpers.
- `GetError` lives in the I/O error script but has no independent `.def` entry.

These names do not by themselves indicate a startup failure. Load the package through a registered public entry before checking its internal functions.

## Module comparison

“Test mentions” means only that a name occurs in current `yacas/tests/*.yts`; it does not imply full argument or boundary coverage. It helps prioritize later behavioral audits.

| Page | Function headings | Discoverable at startup | `.yts` test mentions |
|---|---:|---:|---:|
| `arithmetic.md` | 30 | 30 | 28 |
| `calc.md` | 25 | 25 | 18 |
| `controlflow.md` | 15 | 15 | 11 |
| `elementary.md` | 11 | 11 | 11 |
| `functional.md` | 5 | 5 | 4 |
| `graphs.md` | 8 | 7 | 6 |
| `io.md` | 34 | 34 | 17 |
| `linear-algebra.md` | 44 | 44 | 36 |
| `lists.md` | 57 | 57 | 34 |
| `number-theory.md` | 34 | 34 | 18 |
| `ode.md` | 5 | 5 | 3 |
| `plot.md` | 2 | 2 | 2 |
| `predicates.md` | 29 | 29 | 13 |
| `probability-and-statistics.md` | 12 | 12 | 2 |
| `programming-language.md` | 35 | 35 | 10 |
| `numeric-programming.md` | 23 | 23 | 11 |
| `errors.md` | 12 | 11 | 8 |
| `core-functions.md` | 34 | 34 | 10 |
| `containers.md` | 17 | 17 | 9 |
| `testing.md` | 9 | 9 | 7 |
| `random.md` | 8 | 8 | 5 |
| `solvers.md` | 12 | 12 | 9 |
| `univariate-polynomials.md` | 14 | 12 | 9 |

Pages not listed contain fewer functions and follow the same rules. Add behavioral evidence before promoting historical descriptions to stable commitments.

## Discoverable does not mean validated

The structural audit proves only that a name is registered by the core or scripts. Its arguments and results still require `.yts`, Rust, or product regression tests. Documentation uses these levels:

1. **Language core:** covered by Rust core tests;
2. **Standard-script entry:** exposed through `.def` and expected to have `.yts` behavior tests;
3. **Package helper:** guaranteed only as needed by its public package entry;
4. **Historical entry:** migration guidance without a compatibility commitment.

Future audits should record test coverage and known boundaries on the relevant page instead of producing a static, implementation-independent “everything supported” list.
