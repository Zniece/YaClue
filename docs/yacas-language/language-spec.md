# Yacas Scripting Language Specification

This document defines the Yacas scripting language maintained by YaClue. “Must” denotes behavior on which implementations and scripts may rely. Historical entries not covered here or by tests do not automatically become core language requirements.

## 1. Programs and statements

A Yacas source file consists of expressions terminated by semicolons. The loader parses and evaluates them in order, so an earlier statement may declare operators, variables, and rules used later.

```ys
x := 2;
f(t) := t^2;
f(x);
```

`//` starts a line comment and `/* ... */` encloses a block comment. Identifiers are case-sensitive. Product input fields commonly accept one expression only; this product contract does not restrict `.ys` files to one statement.

## 2. Object model

Language values form expression trees:

- **Numbers:** arbitrary-precision integers and decimal numbers;
- **Symbols:** variables, constants, or function names such as `x`, `Pi`, and `Sin`;
- **Strings:** text enclosed in double quotes;
- **Calls:** a head and arguments, such as `f(x,y)`;
- **Lists:** `{a,b}` is surface syntax for `List(a,b)`;
- **Containers:** arrays and associations created by core commands.

An unknown symbol is a valid value. An unknown function does not cause an “undeclared function” error: its arguments are evaluated and the call is retained. Expressions can therefore serve as both data and programs.

## 3. Surface syntax

A function call is written as `head(arg1,arg2,...)`. Prefix, postfix, infix, and bodied operators are determined by the environment's operator table.

```ys
a+b*c;
-x;
n!;
D(x) Sin(x);
```

`Infix`, `Prefix`, `Postfix`, `Bodied`, `RightAssociative`, and the precedence commands can modify that table. A declaration affects only text parsed afterward. `[a; b; c;]` is a sequential block equivalent to `Prog(a,b,c)` and evaluates to its final expression.

## 4. Evaluation model

Evaluation takes an environment and an expression tree and returns either an expression tree or a structured error.

### Atoms

Numbers and strings evaluate to themselves. Symbol lookup checks local bindings before global bindings and returns the symbol itself when it is unbound. Reading an ordinary binding does not repeatedly reevaluate its stored expression. A lazy global binding is evaluated and cached on first access.

### Calls

A call is processed in this order:

1. Find a core command with the same name.
2. Find a rule base with the same name and arity, loading it through `.def` when necessary.
3. If no implementation applies, evaluate the arguments and retain the original head.

A core command controls how its arguments are evaluated. Rule bases evaluate arguments by default; `HoldArg` disables pre-evaluation for selected arguments. Priority and predicates select a rule, and the rule body's result re-enters evaluation.

### Held and unfinished computations

`Hold(expr)` prevents further evaluation of `expr` at the current boundary; `Eval(expr)` explicitly requests evaluation. Nested holds, macro arguments, and backquoting can change evaluation counts, so rules must not assume that every argument is evaluated exactly once.

When symbolic computation cannot proceed, it normally returns an unevaluated expression. This is a valid result. Type errors, invalid arguments, parse failures, exhausted depth, and interruption are returned as errors.

## 5. Variables and scope

`:=` is definition syntax supplied by the standard scripts and ultimately uses core assignment and rule commands. `Local` and `MacroLocal` declare names in the current frame; local bindings shadow global bindings.

```ys
f(x) := [
    Local(y);
    y := x+1;
    y^2;
];
```

Use `LocalSymbols` or `TemplateFunction` when constructing expressions for later evaluation so formal parameters cannot collide with names in the caller. `Protect`, `UnProtect`, and `IsProtected` manage protected symbols.

## 6. Rule system

A function is identified by name and arity. `RuleBase` declares an ordinary rule base and `MacroRuleBase` declares a macro rule base. Listed variants collect arguments into a list.

```ys
RuleBase("double", {x});
Rule("double", 1, 10, True) 2*x;
```

Common definition and rule syntax expands to core commands:

```ys
double(x) := 2*x;
10 # double(_x) <-- 2*x;
```

Pattern parameters, predicates, and priority together determine applicability. `RulePattern` accepts a prebuilt pattern, and `Retract(name,arity)` removes a rule base. A call remains unevaluated when no rule matches. Recursive rules must reduce the problem or enforce an explicit bound.

## 7. Core and standard scripts

Rust registers core commands through `commands::register_core_commands`. They provide assignment and scope, control flow and error handling, rules and operators, basic data and numeric operations, loading and controlled I/O, debugging hooks, assumptions, and bounded evaluation.

Standard scripts under `yacas/scripts` implement higher-level arithmetic normalization, elementary functions, calculus, equations, ODEs, and linear algebra. Script functions and core commands share call syntax but differ in implementation and stability. The Rust registry is authoritative for core commands.

## 8. Scripts and deferred loading

After creating an `Environment`, a host configures the script directory and loads `yacasinit.ys`. The startup file loads basic syntax and definitions; other packages register names through `.def` files and load on first use.

```ys
DefaultDirectory("/path/to/scripts/");
Load("yacasinit.ys");
```

A `.def` file is an index, not an implementation. Packages commonly use `name.rep/code.ys` with a corresponding `.def`. `Load` loads a specified file, while `Use` avoids duplicate loading.

## 9. Numbers, errors, and resource limits

Each environment stores the current decimal precision. Rust represents exact integers and arbitrary-precision decimals; standard scripts decide when to preserve a symbolic form or request an approximation. `N` is the standard numeric entry point. Lower-level algorithms can use `Math*`, `Fast*`, and precision commands.

The parser, core commands, and evaluator return `YacasError`. Scripts can report errors with `Check` and handle them with `TrapError`, `GetCoreError`, or standard-library utilities. Rust manages error propagation and evaluation frames.

A host can set a deadline, which the evaluator and long-running core loops check. Expiration is reported as `UserInterrupt`. A maximum evaluation depth bounds recursion. File and process commands are additionally constrained by the active security scope.

## 10. Conformance

Executable conformance evidence consists of:

- `yacas/yacas-rs/tests` for the parser, evaluator, and core commands;
- `yacas/tests/*.yts` for standard-script behavior;
- Rust module tests for data structures, numbers, and resource limits.

When documentation and tests conflict, determine whether the implementation or the documentation is stale and update both in the same fix.
