# YaClue

[![CI](https://github.com/Zniece/YaClue/actions/workflows/ci.yml/badge.svg)](https://github.com/Zniece/YaClue/actions/workflows/ci.yml)

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
docs/                 English and Chinese Yacas scripting language documentation
```

The kernel (`yacas/`) is self-contained and could be used by any application;
YaClue is the first one.

## Symbol-safety boundary

User expressions embedded in dynamically generated Yacas scopes must never
share fixed local names with the surrounding command. Processing code uses
`fresh_internal_symbols` over every user-controlled fragment before emitting
such a scope; `LocalSymbols` is not a substitute because it can rename the
inserted user tree together with the command template. AST substitution and
alpha-renaming remain the responsibility of `processing::binding`, while ODE
display names are assigned only after computation. New structured entry
points must include a regression where a user parameter matches each former
scratch-name class, without adding a CAS round trip to the result path.

## License map

| Path | License |
|---|---|
| `yacas/scripts/`, `yacas/tests/` | LGPL-2.1+ (upstream lineage; see `yacas/COPYING`) |
| `yacas/yacas-rs/` | MIT |
| `processing/` | MIT |
| `app/` | MIT |
| `docs/yacas-language/`, `docs/yacas-language-zh/` | GFDL-1.1 (adapted Yacas documentation, project additions, and translations) |

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

The product path is implemented in Rust.

### Desktop

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

### Android

The Android shell is included in the repository. Building it requires a JDK,
the Android SDK, Android NDK 27, and the Rust target for the device. For a
typical 64-bit ARM phone or ARM-based emulator:

```bash
rustup target add aarch64-linux-android

export ANDROID_HOME="$HOME/Library/Android/sdk" # use your SDK location
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="/path/to/your/jdk"

cd app
npm ci
npm run tauri -- android build --debug --target aarch64 --apk --ci
```

The APK is written to
`app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/` and can be
installed on a connected device or emulator with `adb install -r <apk>`.
Use `npm run tauri -- android dev --target aarch64` for development against a
running device or emulator.

The build packages the Yacas and processing scripts as Android assets. On
first launch, YaClue copies them into its private application storage so the
CAS can load them as ordinary files. Android removes that storage when the
application is uninstalled.

The Android port is currently intended for development testing. A distributable
release APK requires a persistent signing key; the repository does not contain
one.

Contributor checks and CI/release-build details are documented in
[CONTRIBUTING.md](CONTRIBUTING.md). GitHub Actions runs formatting, Clippy,
fast engine and processing tests, and the frontend JavaScript check on every
push and pull request. Prerelease tags currently build bundled Linux, macOS,
and Windows packages after the complete test suite passes.

The engine boots the script library through a `DefaultDirectory` +
`Load("yacasinit.ys")` sequence (see `processing/src/engine/rust.rs`); the step
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
baseline. Run those explicitly with
`cargo test -p processing engine::tests -- --ignored --test-threads=1`;
without the binary they fail instead of being reported as passed.

This workspace uses the root `Cargo.lock`; Rust workspace members do not
maintain separate lockfiles. The independently built Tauri application keeps
its own `app/src-tauri/Cargo.lock`.

The low-level Yacas tokenizer accepts Unicode letters. Product-facing
`processing` APIs intentionally restrict variable and parameter identifiers to
ASCII letters, ASCII digits after the first character, and apostrophes until
all domain scripts and TeX rendering support Unicode identifiers consistently.
