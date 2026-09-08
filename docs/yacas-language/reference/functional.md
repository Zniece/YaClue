# Functional operators

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


These operators can help the user to program in the style of
functional programming languages such as Miranda or Haskell.

### infix :(item, list)

           infix :(list, item)
           infix :(list, list)
           infix :(string, string)

prepend or append item to list, or concatenate lists or strings

**Example:**

```
In> a:b:c:{}
Out> {a,b,c};
In> "This":"Is":"A":"String"
Out> "ThisIsAString";

```

> **See also:** [Concat](lists.md#concatlist1-list2), [ConcatStrings](strings.md#concatstringsstrings)


### infix @(fn, arglist)

apply a function

This function is a shorthand for [Apply](controlflow.md#applyfn-arglist). It applies the  function
`fn` to the argument(s) in `arglist` and returns the  result. `fn` can
either be a string containing  the name of a function or a pure function.

**Example:**

```
In> "Sin" @ a
Out> Sin(a);
In> {{a},Sin(a)} @ a
Out> Sin(a);
In> "f" @ {a,b}
Out> f(a,b);

```

> **See also:** [Apply](controlflow.md#applyfn-arglist)


### infix /@(fn, list)

apply a function to all entries in a list

This function is a shorthand for [MapSingle](lists.md#mapsinglefn-list). It successively applies
the function `fn` to all the entries in `list` and returns a list
containing the results. The parameter `fn` can either be a string
containing the name of a function or a pure  function.

**Example:**

```
In> "Sin" /@ {a,b}
Out> {Sin(a),Sin(b)};
In> {{a},Sin(a)*a} /@ {a,b}
Out> {Sin(a)*a,Sin(b)*b};

```

> **See also:** [MapSingle](lists.md#mapsinglefn-list), [Map](lists.md#mapfn-list), [MapArgs](controlflow.md#mapargsexpr-fn)


### infix .. (n, m)

construct a list of consecutive integers

This command returns the list `{n, n+1, n+2, ..., m}`. If `m` is smaller
than `n`, the empty list is returned.

          parser happy. So one should write `1 .. 4` instead of `1..4`.

### NFunction(newname, funcname, arglist)

make wrapper for numeric functions

This function will define a function named `newname`  with the same
arguments as an existing function named `funcname`. The new function
will evaluate and return the expression `funcname(arglist)` only when  all
items in the argument list `arglist` are numbers, and return unevaluated
otherwise. This can be useful e.g. when plotting functions defined through
other Yacas routines that cannot return unevaluated. If the numerical
calculation does not return a number (for example,  it might return the atom
`Infinity` for some arguments),  then the new function will return
`Undefined`.

**Example:**

```
In> f(x) := N(Sin(x));
Out> True;
In> NFunction("f1", "f", {x});
Out> True;
In> f1(a);
Out> f1(a);
In> f1(0);
Out> 0;

```

Suppose we need to define a complicated function `t` which cannot be
evaluated unless the argument is a number:

   In> t(x) := If(x<=0.5, 2*x, 2*(1-x));
   Out> True;
   In> t(0.2);
   Out> 0.4;
   In> t(x);
   In function "If" :
   bad argument number 1 (counting from 1)
   CommandLine(1) : Invalid argument

Then, we can use [NFunction](functional.md#nfunctionnewname-funcname-arglist) to define a wrapper `t1` around
`t` which will not try to evaluate `t` unless the argument is a
number:

   In> NFunction("t1", "t", {x})
   Out> True;
   In> t1(x);
   Out> t1(x);
   In> t1(0.2);
   Out> 0.4;

Now we can plot the function.

   In> Plot2D(t1(x), -0.1: 1.1)
   Out> True;

> **See also:** [MacroRule](programming-language.md#macrorule)


