# Numerical methods

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### bodied NIntegrate(expr, x, x0, x1)

numerical integration

**param x:**integration variable

**param x0:**lower integration limit

**param x1:**upper integration limit

**param expr:**integrand

Numerically integrate `expr` over `x` from `x0` to `x1`.

> **See also:** `Integrate`

