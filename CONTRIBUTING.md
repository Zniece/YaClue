# Contributing

Development uses stable Rust and Node.js 22. Run commands from the repository
root unless a command says otherwise.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
node --check app/src/main.js
```

Install frontend dependencies with `npm ci` in `app/`. The reviewed step
language suite is opt-in because it is slower:

```bash
cargo test -p processing --test steps_yts -- --ignored
```

Only regenerate golden output while intentionally changing expected steps.
Review every resulting diff before committing:

```bash
UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden
```

The `CI` workflow applies the normal checks to pushes and pull requests. The
tests run serially because the symbolic ODE cases enforce request deadlines
and can otherwise compete for CPU on shared runners. The
`Release build` workflow can be started manually and also runs for alpha, beta,
and release-candidate tags such as `v0.1.0-alpha.1`. It verifies unsigned
Release builds on Linux, macOS Apple Silicon, and Windows. Uploading artifacts,
creating a public GitHub Release, and building signed installers follow the
resource-distribution milestone.
