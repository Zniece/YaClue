#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_dir"

usage() {
    echo "usage: $0 fast | domain <engine|processing|ode|steps|app> | release" >&2
    exit 2
}

case "${1:-}" in
    fast)
        cargo fmt --all -- --check
        cargo clippy -p yacas-rs -p processing --all-targets -- -D warnings
        cargo test -p yacas-rs --lib
        cargo test -p processing --lib 'binding::tests::'
        cargo test -p processing --lib 'composition::tests::'
        cargo test -p processing --lib 'engine::tests::'
        cargo test -p processing --lib 'input::tests::'
        cargo test -p processing --lib 'protocol::tests::'
        cargo test -p processing --lib 'semantic::tests::'
        cargo test -p processing --lib 'steps::tests::'
        node --check app/src/main.js
        ;;
    domain)
        case "${2:-}" in
            engine) cargo test -p yacas-rs ;;
            processing) cargo test -p processing -- --test-threads=1 --skip 'ode::tests::' ;;
            ode) cargo test -p processing 'ode::' -- --test-threads=1 ;;
            steps)
                cargo test -p processing --test steps_golden -- --test-threads=1
                cargo test -p processing --test steps_yts -- --test-threads=1
                cargo test -p processing --test step_quality -- --test-threads=1
                ;;
            app) cargo test -p app ;;
            *) usage ;;
        esac
        ;;
    release)
        cargo fmt --all -- --check
        cargo clippy --workspace --all-targets -- -D warnings
        node --check app/src/main.js
        cargo test --workspace -- --test-threads=1
        ;;
    *) usage ;;
esac
