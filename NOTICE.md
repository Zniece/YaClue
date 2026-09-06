# Third-party notices

## Yacas (script library lineage)

- The directory `yacas/` contains **yacas-rs**, a general-purpose CAS engine
  forked from Yacas 1.9.x by Ayal Pinkus and contributors, licensed LGPL-2.1+
  for its script-library lineage (see `yacas/COPYING` and `yacas/AUTHORS`).
  YaClue itself is an application built on top of this engine.
- `yacas/scripts/` and `yacas/tests/` are derived from the upstream library
  and remain LGPL-2.1+. The single semantic modification is documented in the
  root README ("Relationship to upstream").
- `yacas/yacas-rs/` is an independent Rust implementation, licensed MIT. It
  is not a derivative of the upstream C++ code; it pins upstream *behavior*
  through a black-box conformance suite.

## Reference sources

Behavioral questions about the script library's original semantics are
settled against the upstream Yacas sources
(<https://github.com/grzegorzmanowski/yacas>), which are not distributed
with this repository.

## Frontend

- The app uses Tauri (MIT OR Apache-2.0) and KaTeX (MIT) via npm/cargo
  dependencies; see the lockfiles for exact versions.
