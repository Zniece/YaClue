# Glossary

## Arity

A function is identified by name and argument count, so `f(x)` and `f(x,y)` may have separate rule bases. Listed rules collect extra arguments into a list.

## Array

A fixed-length mutable container managed by the Rust engine through `Array'Create`, `Array'Get`, `Array'Set`, and `Array'Size`.

## Atom

An expression-tree value without children, including symbols, strings, and numbers. Symbols are case-sensitive and an unbound symbol evaluates to itself.

<a id="bodied-function"></a>
## Bodied function

An operator form whose final argument appears outside parentheses, as in `D(x) Sin(x)`. `Bodied` declares this parsing behavior and its precedence.

## CAS

Computer Algebra System. Yacas is a rule language for CAS work; `yacas-rs` evaluates it, and standard scripts provide high-level mathematical algorithms.

<a id="constant"></a>
## Constant

A symbol such as `Pi` whose mathematical meaning is supplied by standard scripts. It can remain symbolic until `N` requests an approximation.

<a id="cached-constant"></a>
## Cached constant

An expensive constant cached at a computed precision. Scripts access this mechanism through [CachedConstant](reference/numeric-programming.md#cachedconstantcache-cname-cfunc).

## Equation

`lhs == rhs` constructs a symbolic equation. Assignment uses `:=`, while `=` performs equality testing in applicable language contexts.

## Function

An expression with a call head and arguments. Rust core commands or Yacas rules may implement it. An unknown function retains its head after evaluating its arguments.

## List

A variable-length sequence written `{a,b,c}`, internally equivalent to `List(a,b,c)`.

## Matrix

A row-major list of lists such as `{{a,b},{c,d}}`. Each linear-algebra function validates shape and element-domain requirements.

## Operator

A function with special surface syntax declared as infix, prefix, postfix, or bodied. Operator declarations affect subsequently parsed text.

## Precedence

A nonnegative integer controlling operator binding; smaller Yacas values bind more tightly. Infix operators can have separate left and right precedence.

## Property

A historical term for expression metadata, retained in migration documentation for scripts that used `ExtraInfo'Set`.

## Rule

An evaluation unit that transforms a function call. A rule has a function name, arity, priority, pattern or predicate, and body. Calls remain unevaluated when no rule applies.

## String

A text atom enclosed in double quotes. Strings and symbols are distinct; `String` and `Atom` provide controlled conversion.

## Syntax

Yacas surface syntax combines calls with configurable operators. The parser uses the current environment's operator table to produce a uniform call tree.

## Threaded function

A function for which standard scripts define element-wise list rules. Each function activates threading through its own rules.

## Variable

A symbol bound to a value. Evaluation checks local bindings before global bindings. Lazy global values are evaluated and cached on first access.

## Warranty

The software is provided as-is. Rust engine and product code use the MIT license; inherited standard scripts and tests use LGPL-2.1-or-later. See the [license](license.md).
