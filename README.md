# YaClue

[简体中文](README.zh-CN.md)

[![CI](https://github.com/Zniece/YaClue/actions/workflows/ci.yml/badge.svg)](https://github.com/Zniece/YaClue/actions/workflows/ci.yml)

**Yet another clue** — an open-source, local-first mathematics application.
Enter a problem, inspect its structured result, and follow the transformations
and analyses used to obtain it.

Under the hood, YaClue is powered by **yacas-rs**, a general-purpose computer
algebra engine written in Rust — a dialect fork of
[Yacas](https://github.com/grzegorzmazur/yacas) (1.9.x): the script library is maintained locally while the engine itself is a fresh
implementation. YaClue's typed semantic layer turns engine expressions into
composable mathematical objects and keeps equivalent transformations,
auxiliary analyses, terminal conclusions, and UI effects distinct.

Download the current prerelease from
[YaClue 0.1.0-alpha.4](https://github.com/Zniece/YaClue/releases/tag/v0.1.0-alpha.4).
The desktop interface is available in English and Simplified Chinese; it uses
the system language initially and can be switched in the application.

## Mathematical input

YaClue accepts compact mathematical expressions rather than a general-purpose
script language. Operations compose directly, so the result of an inner
operation remains a typed mathematical object for the outer operation.

Enter one mathematical expression at a time. Definitions, assignments, script
statements, and statement terminators are not part of this input field; use
the Yacas scripting language and `.ys` files when writing or extending CAS
scripts.

```text
D(x)Sin(x)^2
Integrate(x,0,Pi)Sin(x)
Limit(Sin(x)/x,0)
Solve(x^2-5*x+6==0,x)
OdeSolve(y'==y+2*x)
Plot(Sin(x),x,-Pi,Pi)
```

`==` constructs an equation; it does not solve it implicitly. Use `Solve` for
algebraic equations and systems, and `OdeSolve` for ordinary differential
equations. Function-style operations can consume expressions or compatible
results from other operations. Unsupported symbolic results remain structured
objects when possible instead of being flattened into display strings.

## Layout

```
yacas/                The CAS (yacas-rs), laid out like its upstream
├── yacas-rs/         The CAS engine, in Rust (upstream has cyacas/ in C++)
├── scripts/          The standard script library
├── tests/            .yts behavior specs for the library
├── COPYING/AUTHORS   Upstream license and authors (LGPL-2.1+)
processing/           Typed semantic objects, operation registry, composition,
                      domain solvers, structured traces, and product projection
app/                  YaClue's GUI shell (Tauri + KaTeX)
docs/                 English and Chinese Yacas scripting language documentation
```

The kernel (`yacas/`) is self-contained and could be used by any application;
YaClue is the first one.

## Architecture

Input is parsed and elaborated into one typed object tree. Operations execute
inside-out through a shared registry, preserving AST identity and semantic
state as results flow into later operations. Yacas-rs remains responsible for
symbolic computation; `processing` owns mathematical types, capabilities,
partial application, normalization, composition, and structured explanations.

Product output separates four concepts instead of presenting every event as an
equation step:

- equivalent transformations of the complete expression;
- mathematical analyses used to choose or verify a method;
- terminal conclusions such as no value or an unresolved object;
- effects such as a plot attached to the normal mathematical result.

The structured wire format is locale-neutral. Result headings use a stable
`title_key`; steps, analyses, conclusions, and user-facing errors use
`message_ref` objects containing a stable `key` and named `args`. Human-readable
text is selected by the frontend locale catalogue rather than serialized by the
mathematics backend.

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

# normal development gate
./scripts/test-gate.sh fast

# focused engine or step-layer regression
./scripts/test-gate.sh domain engine
./scripts/test-gate.sh domain steps
```

### Test stdin/stdout interface

The development-only `yaclue-stdio` binary exposes the same unified input
dispatch as the desktop application. It reads one request per line and writes
one JSON result per line. A plain line is treated as an expression with steps
enabled; JSON lines may set `expression`, `steps`, and `verbosity`.

```bash
printf '%s\n' 'Limit(x,0)' | cargo run -q -p app --features test-cli --bin yaclue-stdio
printf '%s\n' '{"expression":"D(x)x^2","steps":false,"verbosity":"concise"}' \
  | cargo run -q -p app --features test-cli --bin yaclue-stdio
```

The desktop frontend also includes a collapsed command-line test component
for sending multiple lines through this protocol interactively. This interface
is intended for repository testing and is not a stable public API.

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

The Android port is currently intended for development testing. Prereleases
include an optimized Release arm64 APK signed with an ephemeral evaluation key
so it can be installed directly. It is not an app-store package: a store APK or
AAB requires a persistent signing key, and the repository does not contain one.

Contributor checks and CI/release-build details are documented in
[CONTRIBUTING.md](CONTRIBUTING.md). GitHub Actions runs formatting, Clippy,
fast engine and processing tests, and the frontend JavaScript check on every
push and pull request. Prerelease tags build bundled Linux, macOS, and Windows
packages plus an Android arm64 Release APK after the complete test suite passes.

The engine boots the script library through a `DefaultDirectory` +
`Load("yacasinit.ys")` sequence (see `processing/src/engine/rust.rs`); the step
package (`processing/scripts/steps.rep`) is loaded explicitly on top of the
standard library.

## Status

Version `0.1.0-alpha.4` is the first prerelease based on the typed semantic
core. The backend and structured product contract are stable enough to serve
as the baseline for subsequent development; the application remains a
prerelease and its mathematical coverage and presentation will continue to
evolve.

Current product paths include arithmetic and algebraic transformations,
limits, derivatives, symbolic and definite integrals, series, equations and
systems, symbolic and numeric ODEs, numerical evaluation and root finding,
linear algebra, multivariate calculus, and plotting. Supported results compose
through the semantic object pipeline; unsupported or conditional results remain
explicit rather than silently degrading to strings.

This workspace uses the root `Cargo.lock`; Rust workspace members do not
maintain separate lockfiles. The independently built Tauri application keeps
its own `app/src-tauri/Cargo.lock`.

The low-level Yacas tokenizer accepts Unicode letters. Product-facing
`processing` APIs intentionally restrict variable and parameter identifiers to
ASCII letters, ASCII digits after the first character, and apostrophes until
all domain scripts and TeX rendering support Unicode identifiers consistently.
