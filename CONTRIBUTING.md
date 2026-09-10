# Contributing

Development uses stable Rust and Node.js 22. Run commands from the repository
root unless a command says otherwise.

Use the smallest gate that covers the change:

```bash
# Normal edit/commit gate (target: under 30 seconds on a warm build).
./scripts/test-gate.sh fast

# Focused domain regression (target: under two minutes).
./scripts/test-gate.sh domain engine
./scripts/test-gate.sh domain processing
./scripts/test-gate.sh domain ode
./scripts/test-gate.sh domain steps
./scripts/test-gate.sh domain app

# Complete prerelease gate; intentionally takes several minutes.
./scripts/test-gate.sh release
```

The fast gate checks formatting, lint, the engine and processing library fast
paths, and frontend JavaScript syntax. Run the relevant domain gate while
working in that area. The release gate owns the complete workspace suite,
including ODE, golden, YTS, acceptance, and other integration tests. Do not use
it as the default edit-test loop.

Install frontend dependencies with `npm ci` in `app/`. To isolate the reviewed
Yacas step-language protocol suite:

```bash
cargo test -p processing --test steps_yts -- --test-threads=1
```

Only regenerate golden output while intentionally changing expected steps.
Review every resulting diff before committing:

```bash
UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden
```

The `CI` workflow runs the fast gate on pushes and pull requests. Full
workspace tests run only in the prerelease gate and use one test thread because
the symbolic ODE cases enforce request deadlines. The `Prerelease` workflow
can be started manually to verify packages. Alpha, beta, and release-candidate
tags such as `v0.1.0-alpha.1` publish unsigned Linux, macOS Apple Silicon, and
Windows packages to GitHub Releases.
