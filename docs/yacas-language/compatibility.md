# Compatibility and Stability

Yacas retains the basic Yacas rule-language and standard-script model, while this project owns its Rust engine, script revisions, resource limits, and product interfaces. Compatibility keeps scripts maintainable while allowing the implementation to evolve.

## Stability levels

| Layer | Commitment |
|---|---|
| Language syntax and evaluation | Highest; changes require specification and language-test updates |
| Public standard-script entries | Preserve verified entries; prefer new rules or versioned entries for extensions |
| Core commands | Preserve semantics required by standard scripts; internal Rust organization may change |
| Low-level Rust modules | Repository-internal interfaces pending the high-level facade |
| Processing and GUI | Product APIs outside the Yacas scripting language standard |

## Order of behavioral authority

1. Behavior explicitly documented and tested by this project;
2. the [language specification](language-spec.md);
3. the current Rust core and standard scripts;
4. migration descriptions in the detailed reference.

A discrepancy qualifies as a compatibility defect when a complete, stable reproducer demonstrates a violation of the current contract. Older implementations remain useful evidence during that investigation.

## Permitted improvements

Compatibility permits fixing definite errors, nontermination, and state leaks; adding resource limits; improving numeric representation and error classification; optimizing rule dispatch; adding assumptions, tracing, and host APIs; and adding richer entries while old public entries remain available.

For an intentional observable change, document the trigger, old and new behavior, rationale, and a regression test.

## Maintaining standard scripts

Standard scripts retain public functions and executable rules. New algorithms should normally enter a clearly sourced extension package. Tests define the maintained behavior and protect unrelated scripts during refactoring.

Move a script algorithm into Rust when maintainability, measured performance, rule extensibility, and licensing together support that placement.

## Sources and attribution

The programming guide, function reference, and algorithm notes originated in Yacas documentation; their licenses and contributor records remain here. They are now this project's maintenance baseline. Tests and product contracts identify supported functions. The repository's `oracle` supplies optional evidence for compatibility investigations, while the Rust implementation drives builds, tests, and releases.
