# Contributing

Development uses stable Rust and Node.js 22. Run commands from the repository
root unless a command says otherwise.

```bash
cargo fmt --all -- --check
cargo clippy -p yacas-rs -p processing --all-targets -- -D warnings
cargo test -p yacas-rs --lib
cargo test -p processing --lib -- --test-threads=1 --skip ode::tests::
node --check app/src/main.js
```

Install frontend dependencies with `npm ci` in `app/`. The reviewed step
language suite is opt-in because it is slower:

```bash
cargo test -p processing --test steps_yts -- --test-threads=1
```

Only regenerate golden output while intentionally changing expected steps.
Review every resulting diff before committing:

```bash
UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden
```

The `CI` workflow applies fast checks to pushes and pull requests. Full
workspace tests run before prerelease builds and use one test thread because
the symbolic ODE cases enforce request deadlines. The `Prerelease` workflow
can be started manually to verify packages. Alpha, beta, and release-candidate
tags such as `v0.1.0-alpha.1` publish unsigned Linux, macOS Apple Silicon, and
Windows packages to GitHub Releases.
