# Yacas Scripting Language Programming Function Reference

The programming interfaces are divided by implementation responsibility. Script authors should usually begin with the rule language and error handling; low-level numeric and core functions are mainly for standard-library work and engine embedding.

- [Rules and language core](programming-language.md): operators, rule bases, macros, scope, and secure evaluation.
- [Arbitrary-precision numeric programming](numeric-programming.md): numeric mode, precision, roots, and series helpers.
- [Errors, diagnostics, and source locations](errors.md): hard errors, the script error tableau, and source positions.
- [Rust core functions](core-functions.md): low-level commands registered by `yacas-rs` and historical gaps.
- [Engine containers](containers.md): arrays, associations, and generic compatibility names.
- [Yacas test tools](testing.md): `.yts` verification functions and known-failure conventions.

The [language specification](../language-spec.md) defines public semantics. See the [function availability audit](availability.md) for coverage.
