# YaClue

**Yet another clue** — an open-source, local-first, step-by-step math
application. Enter a problem, watch the solution unfold step by step, and
inspect *why* each step applies. Every step names the rule that produced it.

Under the hood, YaClue is powered by **yacas-rs**, a general-purpose computer
algebra engine written in Rust — a dialect fork of
[Yacas](https://github.com/grzegorzmazur/yacas) (1.9.x): the script library is maintained locally while the engine itself is a fresh
implementation. The step-generation layer and the desktop GUI live outside
the kernel and are what make YaClue a product.

## Layout

```
yacas/                The CAS (yacas-rs), laid out like its upstream
├── yacas-rs/         The CAS engine, in Rust (upstream has cyacas/ in C++)
├── scripts/          The standard script library
├── tests/            .yts behavior specs for the library
├── COPYING/AUTHORS   Upstream license and authors (LGPL-2.1+)
processing/           YaClue's logic layer: engine traits, step generation,
                      plotting support
app/                  YaClue's GUI shell (Tauri + KaTeX)
```

The kernel (`yacas/`) is self-contained and could be used by any application;
YaClue is the first one.

## License map

| Path | License |
|---|---|
| `yacas/scripts/`, `yacas/tests/` | LGPL-2.1+ (upstream lineage; see `yacas/COPYING`) |
| `yacas/yacas-rs/` | MIT |
| `processing/` | MIT |
| `app/` | MIT |

Red lines kept by design:

1. MIT-licensed crates never embed LGPL script contents — scripts stay
   external files loaded at runtime, so LGPL §6 replaceability is natural;
2. modifications to the script library are LGPL (they are);
3. the engine and the script library are released together, versioned as one;
4. if a `.ys` rule is ever rewritten into the Rust engine, that code becomes
   an LGPL derivative — think before moving script logic down.

## yacas-rs's upstream

- yacas-rs's upstream is [Yacas](https://github.com/grzegorzmazur/yacas)
  (LGPL-2.1+): the script library carries that lineage, while Yacas's C++
  engine is not part of this repository.
- The script library receives ongoing maintenance fixes as part of
  yacas-rs.
- The Rust engine is an independent implementation whose supported behavior
  is covered by a conformance suite
  (`yacas/yacas-rs/tests/` — 100+ tests incl. golden files).

## Build & run

The product path is pure Rust — no C++ toolchain needed.

```bash
# run the desktop app
cd app
npm install          # installs the project-local CLI in app/node_modules
npm run tauri dev

# compile the desktop binary without the Node CLI
cd ..
cargo build -p app

# run the engine conformance suite
cargo test -p yacas-rs

# run the step layer tests
cargo test -p processing
```

The engine boots the script library through a `DefaultDirectory` +
`Load("yacasinit.ys")` sequence (see `processing/src/engine.rs`); the step
package (`processing/scripts/steps.rep`) is loaded explicitly on top of the
standard library.

## Status

Work in progress. The desktop experience playground exposes the current
processing APIs for step-by-step derivatives, indefinite and definite
integrals, algebraic transformations, equations and systems, limits,
function plotting, assumptions, and direct engine evaluation. It is an
integration prototype rather than the final GUI design.

## Optional upstream comparison

Normal builds and tests use Rust. For a specific compatibility question,
set `YACAS_BIN` to a separately built Yacas executable to enable the optional
C++ adapter checks. Intentional behavior improvements are governed by this
project's tests; C++ output is not an automatic replacement for a golden
baseline.

This workspace uses the root `Cargo.lock`; member crates do not maintain
separate lockfiles.
