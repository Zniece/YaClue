# Propositional logic theorem prover

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### CanProve(proposition)

try to prove statement

**param proposition:**an expression with logical operations

Yacas has a small built-in propositional logic theorem prover. It can be
invoked with a call to [CanProve](logic.md#canproveproposition). An example of a proposition is: "if
a implies b and b implies c then a implies c". Yacas supports the following
logical operations:

 `Not`
    negation, read as "not"

 `And`
     conjunction, read as "and"

 `Or`
     disjunction, read as "or"
 `=>`
     implication, read as "implies"

The abovementioned proposition would be represented by the following
expression:

     ( (a=>b) And (b=>c) ) => (a=>c)

Yacas can prove that is correct by applying [CanProve](logic.md#canproveproposition) to it:

     In> CanProve(( (a=>b) And (b=>c) ) => (a=>c))
     Out> True;

It does this in the following way: in order to prove a proposition $p$,
it suffices to prove that $\neg p$ is false. It continues to simplify
$\neg p$ using the rules:

 * $\neg\neg x \to x$ (eliminate double negation),
 * $x\Rightarrow y \to \neg x \vee y$ (eliminate implication),
 * $\neg (x \wedge y) \to \neg x \vee \neg y$ ([De Morgan's law](https://en.wikipedia.org/wiki/De_Morgan%27s_laws)),
 * $\neg (x \vee y) \to \neg x \wedge \neg y$ ([De Morgan's law](https://en.wikipedia.org/wiki/De_Morgan%27s_laws)),
 * $(x \wedge y) \vee z \to (x \vee z) \wedge (y \vee z)$ (distribution),
 * $x \vee (y \wedge z) \to (x \vee y) \wedge (x \vee z)$ (distribution),

and the obvious other rules, such as, $1 \vee x \to 1$ etc. The above
rules will translate a proposition into a form:

     (p1 Or p2 Or ...) And (q1 Or q2 Or ...) And ...

If any of the clauses is false, the entire expression will be false. In the
next step, clauses are scanned for situations of the form: $(p \vee Y) \wedge (\neg p \vee Z) \to (Y \vee Z)$. If this combination $(Y\vee Z)$
is empty, it is false, and thus the entire proposition is false. As a last
step, the algorithm negates the result again. This has the added advantage of
simplifying the expression further.

**Example:**

```
In> CanProve(a Or Not a)
Out> True;
In> CanProve(True Or a)
Out> True;
In> CanProve(False Or a)
Out> a;
In> CanProve(a And Not a)
Out> False;
In> CanProve(a Or b Or (a And b))
Out> a Or b;

```

> **See also:** [True](consts.md#true), `False`, `And`, `Or`, [Not](predicates.md#notexpr)


