# Rust Engine and Embedding

`yacas-rs` is the current Yacas engine. It parses and evaluates expressions, matches rules, performs basic numeric operations, and loads scripts. External standard scripts provide most mathematical algorithms.

## Components

| Module | Responsibility |
|---|---|
| `tokenizer.rs`, `parser.rs` | Lexing, surface syntax, and internal prefix syntax |
| `value.rs`, `symtab.rs` | Expression nodes, atoms, and symbol interning |
| `env.rs` | Variables, rules, operators, precision, loading state, and limits |
| `evaluator.rs` | Core commands, rules, deferred loading, and unevaluated fallback |
| `userfunc.rs`, `pattern.rs` | User rules, pattern matching, and priority |
| `commands/` | Rust core commands callable from Yacas |
| `number/` | Arbitrary-precision integers and decimal numbers |
| `loader.rs`, `standard.rs` | `.def` indices, file location, and shared runtime support |

The source tree and the single command registry are authoritative for the core interface.

## Creating an environment

`Environment::new()` creates an isolated core environment, registers standard operators and commands, creates Boolean values, protects basic symbols, and establishes a local frame. The host then configures the standard-script directory.

```rust
use yacas_rs::{env::Environment, evaluator, parser};

let mut env = Environment::new();
let tree = parser::parse_expression(&mut env, "1+2;")?
    .expect("one expression");
let value = evaluator::eval(&mut env, &tree)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Applications should configure the script directory and load `yacasinit.ys` before relying on symbolic arithmetic. The processing engine adapter demonstrates the current product setup.

## Environments, parsing, and threads

Variables, rules, assumptions, precision, loading state, and operators belong to an `Environment`. Each environment owns its mutable state. Expression nodes use `Rc`; cross-thread applications keep the environment on one worker and serialize requests through message passing.

`parse_expression` parses one semicolon-terminated expression. `parse_one` reads expressions successively from a stream. Parsing uses the current operator table. `evaluator::eval` returns an expression tree for direct structured consumption by the host.

## Boundary for core extensions

A new Rust core command should serve engine internals, low-level numeric access, controlled I/O, or behavior whose reliable implementation requires the core. It must define argument evaluation and scope, return structured `YacasError` values, check deadlines in long loops, use the appropriate `commands` module and central registry, and have core and dependent-script tests.

Readable symbolic identities and transformations normally belong in `.ys` scripts. Rust suits basic mechanisms, numeric hot paths, resource limits, and host interfaces. Processing suits product orchestration, verification, and teaching events.

## Resources and security

`Environment::set_eval_timeout` sets a request deadline, while `max_eval_depth` bounds recursion. Hosts should clear deadlines at request completion and reclaim temporary unique symbols according to session policy.

Yacas includes file, loading, and system-call capabilities. Products expose a validated single-expression interface to users and reserve full script execution for trusted packages or explicit development interfaces.

## Rust API stability

The crate currently exposes low-level modules for repository-internal use. A future stable CAS facade will wrap sessions, evaluation, batched numeric work, and errors; `Rc<LispObject>`, `Environment` fields, and thread protocols remain implementation details.
