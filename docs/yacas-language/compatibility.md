# Compatibility and Stability

Yacas retains the basic Yacas rule-language and standard-script model, while this project owns its Rust engine, script revisions, resource limits, and product interfaces. Compatibility keeps scripts maintainable; it does not freeze historical implementation details.

## Stability levels

| Layer | Commitment |
|---|---|
| Language syntax and evaluation | Highest; changes require specification and language-test updates |
| Public standard-script entries | Preserve verified entries; prefer new rules or versioned entries for extensions |
| Core commands | Preserve semantics required by standard scripts; internal Rust organization may change |
| Low-level Rust modules | Primarily internal; no stable facade is promised yet |
| Processing and GUI | Product APIs outside the Yacas scripting language standard |

## Order of behavioral authority

1. Behavior explicitly documented and tested by this project;
2. the [language specification](language-spec.md);
3. the current Rust core and standard scripts;
4. historical descriptions in the detailed reference.

Output from an older implementation does not override current tests by itself. Treat a discrepancy as a compatibility defect only when it has a complete, stable reproducer and violates the current contract.

## Permitted improvements

Compatibility permits fixing definite errors, nontermination, and state leaks; adding resource limits; improving numeric representation and error classification; optimizing rule dispatch; adding assumptions, tracing, and host APIs; and adding richer entries while old public entries remain available.

For an intentional observable change, document the trigger, old and new behavior, rationale, and a regression test.

## Maintaining standard scripts

Standard scripts retain public functions and executable rules. New algorithms should normally enter a clearly sourced extension package. Incremental maintenance neither preserves defects nor forbids refactoring; it requires tests and avoids accidental breakage of unrelated scripts.

Before moving a script algorithm into Rust, consider maintainability, performance, rule extensibility, and licensing. The fact that Rust can express the same algorithm is not sufficient reason to migrate it.

## Sources and attribution

The programming guide, function reference, and algorithm notes originated in Yacas documentation; their licenses and contributor records remain here. They are now this project's maintenance baseline, but historical functions do not imply complete product support. The repository's `oracle` may be consulted during compatibility investigations; normal builds, tests, and releases do not depend on it.
