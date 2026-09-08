# Debugging and Tracing

> **Scope:** This page describes the tracing facilities implemented by the current Rust engine.

## TraceExp(expr)

`TraceExp` uses `CustomEval` to emit `TrEnter` and `TrLeave` around each subexpression. Rust end-to-end tests cover this path. Complex expressions can produce large amounts of text, so it is best suited to small reproducers.

```ys
ToString()[TraceExp(1+1);];
```

Hosts capture trace diagnostics from the engine output stream. Product integrations use their own structured event protocols.

## TraceRule(template, expr)

`TraceRule` traces only script rule bases whose function head and arity match `template`, then restores the trace flag after evaluating `expr`.

```ys
f(x) := x+1;
TraceRule(f(_x), f(3));
```

The Rust implementation emits `TrEnter` and `TrLeave` for script-function rule calls. Its core interface is `TraceRule(template, expr)`.

## TraceStack(expr)

`TraceStack(expr)` currently evaluates and returns `expr`. Structured frame recording is tracked as a future tracing capability.

## CustomEval

`CustomEval(enter, leave, error, expression)` is the available low-level debugging mechanism. Its callbacks can read:

- `CustomEval'Expression()`: the current expression;
- `CustomEval'Result()`: the current result;
- `CustomEval'Locals()`: currently visible local names;
- `CustomEval'Stop()`: stop custom evaluation.

Debug state is reset when callbacks finish. See the [Yacas programming guide](../programming/index.md#custom-evaluation-facilities) for the detailed model.

## Profiling

`Profile` is a standard-script call profiler built on `CustomEval` and adds substantial evaluation overhead. Rust benchmarks, `YACAS_RULE_STATS`, and representative persistent sessions provide performance measurements for engine hot paths, rule attempts, and numeric work.
