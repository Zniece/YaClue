# Debugging and Tracing

> **Scope:** This page describes tracing implemented by the current Rust engine. Interactive commands from the historical script debugger depend on unavailable console-input hooks and are not stable entries.

## TraceExp(expr)

`TraceExp` uses `CustomEval` to emit `TrEnter` and `TrLeave` around each subexpression. Rust end-to-end tests cover this path. Complex expressions can produce large amounts of text, so it is best suited to small reproducers.

```ys
ToString()[TraceExp(1+1);];
```

Hosts should capture the engine output stream when consuming a trace. Trace text is not currently a stable structured product protocol.

## TraceRule(template, expr)

`TraceRule` traces only script rule bases whose function head and arity match `template`, then restores the trace flag after evaluating `expr`.

```ys
f(x) := x+1;
TraceRule(f(_x), f(3));
```

The Rust implementation emits `TrEnter` and `TrLeave` for rule calls. It applies to script functions, not core commands. The actual core interface is the two-argument `TraceRule(template, expr)`, despite historical documentation showing bodied syntax.

## TraceStack(expr)

The name `TraceStack` remains for script compatibility. The Rust evaluator does not retain a displayable historical call stack, so this command evaluates and returns `expr` without printing old-style frames. Do not rely on it to diagnose recursion failures until structured frame recording exists.

## CustomEval

`CustomEval(enter, leave, error, expression)` is the available low-level debugging mechanism. Its callbacks can read:

- `CustomEval'Expression()`: the current expression;
- `CustomEval'Result()`: the current result;
- `CustomEval'Locals()`: currently visible local names;
- `CustomEval'Stop()`: stop custom evaluation.

Debug state is reset when callbacks finish. See the [Yacas programming guide](../programming/index.md#custom-evaluation-facilities) for the detailed model.

## Profiling

`Profile` is a standard-script call profiler built on `CustomEval` and adds substantial evaluation overhead. Measure engine hot paths, rule attempts, and numeric performance with Rust benchmarks, `YACAS_RULE_STATS`, and representative persistent sessions. Do not use trace output as a performance benchmark.
