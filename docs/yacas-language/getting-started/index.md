# Getting Started

The current Yacas engine is implemented in Rust and its standard scripts live in the project. Building and testing requires neither C++, CMake, Java, nor a separately installed Yacas.

## Build and test

From the product repository, use stable Rust:

```bash
cd prj
cargo test -p yacas-rs
cargo test -p processing
cargo fmt --all -- --check
```

`Cargo.lock` is the shared workspace lockfile.

## Run the desktop test bench

The desktop application also requires Node.js. Tauri CLI is installed locally in the project.

```bash
cd app
npm install
npm run tauri dev
```

`cargo build -p app` builds the Rust application member directly. The current interface is a backend integration test bench, not the final GUI. Ordinary mathematical fields accept a single-expression Yacas subset, and equation systems are split by line. The direct Yacas evaluation entry uses the persistent engine session and is intended for trusted development use.

## Initialize standard scripts

A bare `Environment::new()` registers only the language core. A complete CAS configures `yacas/scripts` as its script directory and loads:

```ys
Load("yacasinit.ys");
```

Processing performs this initialization and then loads teaching-step scripts. Embedded applications should locate packaged resources explicitly instead of relying on the working directory.

## Write scripts

Yacas files use the `.ys` extension and terminate every expression with a semicolon:

```ys
square(x) := x*x;
square(5);
```

Packages commonly place code in `name.rep/code.ys` and register deferred functions through `.def`. See the [language specification](../language-spec.md) and [programming guide](../programming/index.md).

The repository's `oracle` is used only for specific compatibility investigations. It does not participate in normal builds or determine whether intentional improvements should be reverted.
