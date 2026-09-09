# Function Availability Audit

This page classifies detailed function-reference entries as core commands, standard scripts, package helpers, or migration entries. The audited counts describe documentation structure.

## Structural audit: 2026-09-09

The audit created an `Environment` with current `yacas-rs`, loaded `yacasinit.ys`, and compared reference headings with the core registry, startup rule bases, and deferred-load `.def` tables.

| Item | Count |
|---|---:|
| Level-three reference headings | 525 |
| Headings recognizable as functions or operators | 509 |
| Names directly discoverable after full startup | 505 |
| Names available after package loading | 4 |

Four package-local names become available when their packages load.

## Migration entries

- `Factorize`: use the standard-script `Factor` or a specific polynomial interface.
- `ExtraInfo'Set`: migrate object metadata to structures supported by the current expression model.
- `MathSinh`, `MathCosh`, `MathTanh`, and the three hyperbolic `MathArc*` names: use their public standard-script functions.
- `IsPromptShown` and `ReadCmdLineString`: the Rust host supplies console interaction.
- `GetTime`: Rust benchmarks and host timing provide elapsed-time measurement.

These names provide migration guidance for scripts written against earlier interfaces.

## Names available after package loading

- The graph operator `->` is declared by the graph package.
- `OrthoPoly` and `OrthoPolySum` are internal orthogonal-polynomial helpers.
- `GetError` loads with the I/O error script through that package's public entries.

Load the package through a registered public entry before checking its internal functions.

## Module comparison

“Test mentions” counts textual occurrences in current `yacas/tests/*.yts`. Dedicated behavior tests record argument and boundary coverage. The count helps prioritize later audits.

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

Pages with fewer functions follow the same rules. Behavioral evidence promotes a description to a stable commitment.

## Evidence levels

The structural audit records names registered by the core or scripts. `.yts`, Rust, and product regression tests supply evidence for arguments and results. Documentation uses these levels:

1. **Language core:** covered by Rust core tests;
2. **Standard-script entry:** exposed through `.def` and expected to have `.yts` behavior tests;
3. **Package helper:** guaranteed only as needed by its public package entry;
4. **Migration entry:** guidance for earlier scripts.

Future audits record test coverage and known boundaries on each relevant page, keeping support claims tied to executable evidence.
