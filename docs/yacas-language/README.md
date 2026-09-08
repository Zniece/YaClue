# Yacas Scripting Language Documentation

**English** | [中文](../yacas-language-zh/README.md)

This directory documents the Yacas scripting language used by YaClue, its Rust evaluator, and the standard script library maintained with the project. The Yacas scripting language is a rule language for symbolic computation; `.ys` is its script extension. The current implementation lives in `yacas/yacas-rs` and provides the engine used by normal builds.

## Documentation layers

Project-maintained documents governed by the current implementation and tests:

1. [Language specification](language-spec.md): syntax, values, evaluation, rules, and loading.
2. [Rust engine](rust-engine.md): architecture, embedding, limits, and extension boundaries.
3. [Compatibility](compatibility.md): stable contracts and intentional differences.
4. [Getting started](getting-started/index.md): building, testing, and running scripts.

Detailed material includes the [programming guide](programming/index.md), [function reference](reference/index.md), [algorithm notes](algorithms/index.md), [tutorial](tutorial/index.md), and [glossary](glossary.md). This material originated in Yacas documentation and is now maintained with this implementation. Tests and the compatibility policy define the supported product capabilities. See the [license](license.md) and [credits](credits.md) for attribution.

## Specification, implementation, and product

| Layer | Location | Responsibility |
|---|---|---|
| Yacas scripting language | `yacas/yacas-rs/src/` | Parsing, values, evaluation, rules, numbers, and loading |
| Standard scripts | `yacas/scripts/` | Symbolic algorithms and user functions written in Yacas |
| Executable contracts | `yacas/tests/`, `yacas/yacas-rs/tests/` | Language and library behavior |
| Teaching steps | `processing/` | Orchestration, verification, numeric methods, and structured steps |
| Desktop application | `app/` | Product interaction built on the language and processing APIs |

The core supplies mechanisms needed to run scripts. Standard scripts provide most capabilities such as `Simplify`, `Solve`, and `Integrate`.

## Maintenance rules

- Derive language semantics from the Rust implementation and executable tests.
- Label core commands separately from script functions.
- Update the specification and regression tests with syntax or semantic changes.
- Reserve language guarantees for stable behavior; keep measurements in development status notes.
- Use historical implementations as sources for attribution and compatibility investigations.

## License

This documentation includes adapted upstream Yacas manuals. This English tree,
including project-maintained additions, is distributed under the GNU Free
Documentation License 1.1. See the [license](license.md) and
[credits](credits.md).
