# List operations

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


Most objects that can be of variable size are represented as lists
(linked lists internally). Yacas does implement arrays, which are
faster when the number of elements in a collection of objects doesn't
change. Operations on lists have better support in the current system.

### Head(list)

returns the first element of a list

This function returns the first element of a list. If it is applied
to a general expression, it returns the first operand. An error is
returned if `list` is an atom.

**Example:**

```
In> Head({a,b,c})
Out> a;
In> Head(f(a,b,c));
Out> a;

```

> **See also:** [Tail](lists.md#taillist), [Length](lists.md#lengthlist)


### Tail(list)

returns a list without its first element

**Example:**

```
In> Tail({a,b,c})
Out> {b,c};

```

> **See also:** [Head](lists.md#headlist), [Length](lists.md#lengthlist)


### Length(list)

           Length(string)

The length of a list or string

**Example:**

```
In> Length({a,b,c})
Out> 3;
In> Length("abcdef");
Out> 6;

```

> **See also:** [Head](lists.md#headlist), [Tail](lists.md#taillist), [Nth](lists.md#nthlist-n), [Count](lists.md#countlist-expr)


### Map(fn, list)

apply an *n*-ary function to all entries in a list

This function applies `fn` to every list of arguments to be found
in `list`. So the first entry of `list` should be a list
containing the first, second, third, ... argument to `fn`, and
the same goes for the other entries of `list`. The function can
either be given as a string or as a pure function (see [Apply](controlflow.md#applyfn-arglist) for
more information on pure functions).

**Example:**

```
In> Map("+",{{a,b},{c,d}});
Out> {a+c,b+d};

```

> **See also:** [MapSingle](lists.md#mapsinglefn-list), [MapArgs](controlflow.md#mapargsexpr-fn), [Apply](controlflow.md#applyfn-arglist)


### MapSingle(fn, list)

apply a unary function to all entries in a list

The function `fn` is successively applied to all entries in
`list`, and a list containing the respective results is
returned. The function can be given either as a string or as a pure
function (see [Apply](controlflow.md#applyfn-arglist) for more information on pure functions).

The `/@` operator provides a shorthand for [MapSingle](lists.md#mapsinglefn-list).

**Example:**

```
In> MapSingle("Sin",{a,b,c});
Out> {Sin(a),Sin(b),Sin(c)};
In> MapSingle({{x},x^2}, {a,2,c});
Out> {a^2,4,c^2};

```

> **See also:** [Map](lists.md#mapfn-list), [MapArgs](controlflow.md#mapargsexpr-fn), `/@`, [Apply](controlflow.md#applyfn-arglist)


### MakeVector(var,n)

vector of uniquely numbered variable names

A list of length `n` is generated. The first entry contains the
identifier `var` with the number 1 appended to it, the second entry
contains `var` with the suffix 2, and so on until the last entry
which contains `var` with the number `n` appended to it.

**Example:**

```
In> MakeVector(a,3)
Out> {a1,a2,a3};

```

> **See also:** [RandomIntegerVector](random.md#randomintegervectorn-from-to), [ZeroVector](linear-algebra.md#zerovectorn)


### Select(pred, list)

select entries satisfying some predicate

[Select](lists.md#selectpred-list) returns a sublist of `list` which contains all the
entries for which the predicate `pred` returns `True` when
applied to this entry.

**Example:**

```
In> Select("IsInteger",{a,b,2,c,3,d,4,e,f})
Out> {2,3,4};

```

> **See also:** [Length](lists.md#lengthlist), [Find](lists.md#findlist-expr), [Count](lists.md#countlist-expr)


### Nth(list, n)

return the `n`-th element of a list

The entry with index `n` from `list` is returned. The first
entry has index 1. It is possible to pick several entries of the
list by taking `n` to be a list of indices.

More generally, `Nth` returns the `n`-th operand of the
expression passed as first argument.

An alternative but equivalent form of `Nth(list, n)` is
`list[n]`.

**Example:**

```
In> lst := {a,b,c,13,19};
Out> {a,b,c,13,19};
In> Nth(lst, 3);
Out> c;
In> lst[3];
Out> c;
In> Nth(lst, {3,4,1});
Out> {c,13,a};
In> Nth(b*(a+c), 2);
Out> a+c;

```

> **See also:** [Select](lists.md#selectpred-list), [Nth](lists.md#nthlist-n)


### Reverse(list)

return the reversed list (without touching the original)

**param list:**list to reverse

This function returns a list reversed, without changing the
original list. It is similar to [DestructiveReverse](lists.md#destructivereverselist), but safer and
slower.

**Example:**

```
In> lst:={a,b,c,13,19}
Out> {a,b,c,13,19};
In> revlst:=Reverse(lst)
Out> {19,13,c,b,a};
In> lst
Out> {a,b,c,13,19};

```

> **See also:** [FlatCopy](lists.md#flatcopylist), [DestructiveReverse](lists.md#destructivereverselist)


<a id="listexpr1-expr2"></a>

### List(expr1, expr2, ...)

construct a list

A list is constructed whose first entry is `expr1`, the second
entry is `expr2`, and so on. This command is equivalent to the
expression `{expr1, expr2, ...}`.

**Example:**

```
In> List();
Out> {};
In> List(a,b);
Out> {a,b};
In> List(a,{1,2},d);
Out> {a,{1,2},d};

```

> **See also:** [UnList](lists.md#unlistlist), [Listify](lists.md#listifyexpr)


### UnList(list)

convert a list to a function application

This command converts a list to a function application. The first
entry of `list` is treated as a function atom, and the following
entries are the arguments to this function. So the function
referred to in the first element of `list` is applied to the other
elements.

Note that `list` is evaluated before the function application is
formed, but the resulting expression is left unevaluated. The
functions [UnList](lists.md#unlistlist) and [Hold](controlflow.md#holdexpr) both stop the process of
evaluation.

**Example:**

```
In> UnList({Cos, x});
Out> Cos(x);
In> UnList({f});
Out> f();
In> UnList({Taylor,x,0,5,Cos(x)});
Out> Taylor(x,0,5)Cos(x);
In> Eval(%);
Out> 1-x^2/2+x^4/24;

```

> **See also:** [List](lists.md#listexpr1-expr2), [Listify](lists.md#listifyexpr), [Hold](controlflow.md#holdexpr)


### Listify(expr)

convert a function application to a list

The parameter `expr` is expected to be a compound object, i.e. not
an atom. It is evaluated and then converted to a list. The first
entry in the list is the top-level operator in the evaluated
expression and the other entries are the arguments to this
operator. Finally, the list is returned.

**Example:**

```
In> Listify(Cos(x));
Out> {Cos,x};
In> Listify(3*a);
Out> {*,3,a};

```

> **See also:** [List](lists.md#listexpr1-expr2), [UnList](lists.md#unlistlist), [IsAtom](predicates.md#isatomexpr)


<a id="concatlist1-list2"></a>

### Concat(list1, list2, ...)

concatenate lists

The lists `list1`, `list2`, ... are evaluated and concatenated. The
resulting big list is returned.

**Example:**

```
In> Concat({a,b}, {c,d});
Out> {a,b,c,d};
In> Concat({5}, {a,b,c}, {{f(x)}});
Out> {5,a,b,c,{f(x)}};

```

> **See also:** [ConcatStrings](strings.md#concatstringsstrings), `:`, [Insert](lists.md#insertlist-n-expr)


### Delete(list, n)

delete an element from a list

This command deletes the `n`-th element from `list`. The first
parameter should be a list, while `n` should be a positive integer
less than or equal to the length of `list`. The entry with index
`n` is removed (the first entry has index 1), and the resulting
list is returned.

**Example:**

```
In> Delete({a,b,c,d,e,f}, 4);
Out> {a,b,c,e,f};

```

> **See also:** [DestructiveDelete](lists.md#destructivedeletelist-n), [Insert](lists.md#insertlist-n-expr), [Replace](lists.md#replacelist-n-expr)


### Insert(list, n, expr)

insert an element into a list

The expression `expr` is inserted just before the `n`-th entry in
`list`. The first parameter `list` should be a list, while `n`
should be a positive integer less than or equal to the length of
`list` plus one. The expression `expr` is placed between the
entries in `list` with indices `n-1` and `n`. There are two border
line cases: if `n` is 1, the expression `expr` is placed in front
of the list (just as by the `:` operator); if `n` equals the length
of `list` plus one, the expression `expr` is placed at the end of
the list (just as by [Append](lists.md#appendlist-expr)). In any case, the resulting list is
returned.

**Example:**

```
In> Insert({a,b,c,d}, 4, x);
Out> {a,b,c,x,d};
In> Insert({a,b,c,d}, 5, x);
Out> {a,b,c,d,x};
In> Insert({a,b,c,d}, 1, x);
Out> {x,a,b,c,d};

```

> **See also:** [DestructiveInsert](lists.md#destructiveinsertlist-n-expr), `:`, [Append](lists.md#appendlist-expr), [Delete](lists.md#deletelist-n)


### Replace(list, n, expr)

replace an entry in a list

The `n`-th entry of `list` is replaced by the expression
`expr`. This is equivalent to calling [Delete](lists.md#deletelist-n) and [Insert](lists.md#insertlist-n-expr) in
sequence. To be precise, the expression `Replace(list, n, expr)`
has the same result as the expression ``Insert(Delete(list, n), n,
expr)``.

**Example:**

```
In> Replace({a,b,c,d,e,f}, 4, x);
Out> {a,b,c,x,e,f};

```

> **See also:** [Delete](lists.md#deletelist-n), [Insert](lists.md#insertlist-n-expr), [DestructiveReplace](lists.md#destructivereplacelist-n-expr)


### FlatCopy(list)

copy the top level of a list

A copy of `list` is made and returned. The list is not recursed
into, only the first level is copied. This is useful in combination
with the destructive commands that actually modify lists in place
(for efficiency).

The following shows a possible way to define a command that
reverses a list nondestructively.

**Example:**

```
In> reverse(l_IsList) <-- DestructiveReverse(FlatCopy(l));
Out> True;
In> lst := {a,b,c,d,e};
Out> {a,b,c,d,e};
In> reverse(lst);
Out> {e,d,c,b,a};
In> lst;
Out> {a,b,c,d,e};

```

### Contains(list, expr)

test whether a list contains a certain element

This command tests whether `list` contains the expression `expr` as an
entry. It returns `True` if it does and `False` otherwise. Only
the top level of `list` is examined. The parameter `list` may also be a
general expression, in that case the top-level operands are tested for the
occurrence of `expr`.

**Example:**

```
In> Contains({a,b,c,d}, b);
Out> True;
In> Contains({a,b,c,d}, x);
Out> False;
In> Contains({a,{1,2,3},z}, 1);
Out> False;
In> Contains(a*b, b);
Out> True;

```

> **See also:** [Find](lists.md#findlist-expr), [Count](lists.md#countlist-expr)


### Find(list, expr)

get the index at which a certain element occurs

This commands returns the index at which the expression `expr`
occurs in `list`. If `expr` occurs more than once, the lowest
index is returned. If `expr` does not occur at all, -1 is
returned.

**Example:**

```
In> Find({a,b,c,d,e,f}, d);
Out> 4;
In> Find({1,2,3,2,1}, 2);
Out> 2;
In> Find({1,2,3,2,1}, 4);
Out> -1;

```

> **See also:** [Contains](lists.md#containslist-expr)


### Append(list, expr)

append an entry at the end of a list

The expression `expr` is appended at the end of `list` and the
resulting list is returned.

Note that due to the underlying data structure, the time it takes
to append an entry at the end of a list grows linearly with the
length of the list, while the time for prepending an entry at the
beginning is constant.

**Example:**

```
In> Append({a,b,c,d}, 1);
Out> {a,b,c,d,1};

```

> **See also:** [Concat](lists.md#concatlist1-list2), `:`, [DestructiveAppend](lists.md#destructiveappendlist-expr)


### RemoveDuplicates(list)

remove any duplicates from a list

This command removes all duplicate elements from a given list and
returns the resulting list.  To be precise, the second occurrence
of any entry is deleted, as are the third, the fourth, etc.

**Example:**

```
In> RemoveDuplicates({1,2,3,2,1});
Out> {1,2,3};
In> RemoveDuplicates({a,1,b,1,c,1});
Out> {a,1,b,c};

```

### Swap(list, i1, i2)

swap two elements in a list

This command swaps the pair of entries with entries `i1` and
`i2` in `list`. So the element at index `i1` ends up at index
`i2` and the entry at `i2` is put at index `i1`. Both indices
should be valid to address elements in the list. Then the updated
list is returned.  [Swap](lists.md#swaplist-i1-i2) works also on generic arrays.

**Example:**

```
In> lst := {a,b,c,d,e,f};
Out> {a,b,c,d,e,f};
In> Swap(lst, 2, 4);
Out> {a,d,c,b,e,f};

```

> **See also:** [Replace](lists.md#replacelist-n-expr), [DestructiveReplace](lists.md#destructivereplacelist-n-expr), [Array'Create](containers.md#arraycreatesize-init)


### Count(list, expr)

count the number of occurrences of an expression

This command counts the number of times that the expression
`expr` occurs in `list` and returns this number.

**Example:**

```
In> lst := {a,b,c,b,a};
Out> {a,b,c,b,a};
In> Count(lst, a);
Out> 2;
In> Count(lst, c);
Out> 1;
In> Count(lst, x);
Out> 0;

```

> **See also:** [Length](lists.md#lengthlist), [Select](lists.md#selectpred-list), [Contains](lists.md#containslist-expr)


### FillList(expr, n)

fill a list with a certain expression

This command creates a list of length `n` in which all slots
contain the expression `expr` and returns this list.

**Example:**

```
In> FillList(x, 5);
Out> {x,x,x,x,x};

```

> **See also:** [MakeVector](lists.md#makevectorvarn), [ZeroVector](linear-algebra.md#zerovectorn), [RandomIntegerVector](random.md#randomintegervectorn-from-to)


### Drop(list, n)

           Drop(list, -n)
           Drop(list, {m, n})

drop a range of elements from a list

This command removes a sublist of `list` and returns a list
containing the remaining entries. The first calling sequence drops
the first `n` entries in `list`. The second form drops the last
`n` entries. The last invocation drops the elements with indices
`m` through `n`.

**Example:**

```
In> lst := {a,b,c,d,e,f,g};
Out> {a,b,c,d,e,f,g};
In> Drop(lst, 2);
Out> {c,d,e,f,g};
In> Drop(lst, -3);
Out> {a,b,c,d};
In> Drop(lst, {2,4});
Out> {a,e,f,g};

```

> **See also:** [Take](lists.md#takelist-n), [Select](lists.md#selectpred-list)


### Take(list, n)

           Take(list, -n)
           Take(list, {m,n})

take a sublist from a list, dropping the rest

This command takes a sublist of `list`, drops the rest, and
returns the selected sublist. The first calling sequence selects
the first `n` entries in `list`. The second form takes the last
`n` entries. The last invocation selects the sublist beginning
with entry number `m` and ending with the `n`-th entry.

**Example:**

```
In> lst := {a,b,c,d,e,f,g};
Out> {a,b,c,d,e,f,g};
In> Take(lst, 2);
Out> {a,b};
In> Take(lst, -3);
Out> {e,f,g};
In> Take(lst, {2,4});
Out> {b,c,d};

```

> **See also:** [Drop](lists.md#droplist-n), [Select](lists.md#selectpred-list)


### Partition(list, n)

partition a list in sublists of equal length

This command partitions `list` into non-overlapping sublists of
length `n` and returns a list of these sublists. The first `n`
entries in `list` form the first partition, the entries from
position `n+1` up to `2n` form the second partition, and so
on. If `n` does not divide the length of `list`, the remaining
entries will be thrown away. If `n` equals zero, an empty list is
returned.

**Example:**

```
In> Partition({a,b,c,d,e,f,}, 2);
Out> {{a,b},{c,d},{e,f}};
In> Partition(1 .. 11, 3);
Out> {{1,2,3},{4,5,6},{7,8,9}};

```

> **See also:** [Take](lists.md#takelist-n), [Permutations](calc.md#permutationslist)


### Flatten(expression,operator)

flatten expression w.r.t. some operator

[Flatten](lists.md#flattenexpressionoperator) flattens an expression with respect to a specific operator,
converting the result into a list.  This is useful for unnesting an
expression. [Flatten](lists.md#flattenexpressionoperator) is typically used in simple simplification
schemes.

**Example:**

```
In> Flatten(a+b*c+d, "+");
Out> {a,b*c,d};
In> Flatten({a,{b,c},d}, "List");
Out> {a,b,c,d};

```

> **See also:** [UnFlatten](lists.md#unflattenlistoperatoridentity)


### UnFlatten(list,operator,identity)

inverse operation of [Flatten](lists.md#flattenexpressionoperator)

[UnFlatten](lists.md#unflattenlistoperatoridentity) is the inverse operation of [Flatten](lists.md#flattenexpressionoperator). Given a list,
it can be turned into an expression representing for instance the addition of
these elements by calling [UnFlatten](lists.md#unflattenlistoperatoridentity) with `+` as argument to
operator, and 0 as argument to identity (0 is the identity for addition,
since a+0=a). For multiplication the identity element would be 1.

**Example:**

```
In> UnFlatten({a,b,c},"+",0)
Out> a+b+c;
In> UnFlatten({a,b,c},"*",1)
Out> a*b*c;

```

> **See also:** [Flatten](lists.md#flattenexpressionoperator)


### Type(expr)

return the type of an expression

The type of the expression `expr` is represented as a string and
returned. So, if `expr` is a list, the string `"List"` is
returned. In general, the top-level operator of `expr` is
returned. If the argument `expr` is an atom, the result is the
empty string `""`.

**Example:**

```
In> Type({a,b,c});
Out> "List";
In> Type(a*(b+c));
Out> "*";
In> Type(123);
Out> "";

```

> **See also:** [IsAtom](predicates.md#isatomexpr), [NrArgs](lists.md#nrargsexpr)


### NrArgs(expr)

return number of top-level arguments

This function evaluates to the number of top-level arguments of the
expression `expr`. The argument `expr` may not be an atom,
since that would lead to an error.

**Example:**

```
In> NrArgs(f(a,b,c))
Out> 3;
In> NrArgs(Sin(x));
Out> 1;
In> NrArgs(a*(b+c));
Out> 2;

```

> **See also:** [Type](lists.md#typeexpr), [Length](lists.md#lengthlist)


### VarList(expr)

           VarListArith(expr)
           VarListSome(expr, list)

list of variables appearing in an expression

The command [VarList](lists.md#varlistexpr) returns a list of all variables that
appear in the expression `expr`. The expression is traversed
recursively.

The command `VarListSome` looks only at arguments of functions in the
`list`. All other functions are considered opaque (as if they do not
contain any variables) and their arguments are not checked.  For example,
`VarListSome(a + Sin(b-c))` will return `{a, b, c}`, but
`VarListSome(a*Sin(b-c), {*})` will not look at arguments of [Sin](elementary.md#sinx)
and will return `{a,Sin(b-c)}`. Here `Sin(b-c)` is considered a
variable because the function [Sin](elementary.md#sinx) does not belong to `list`.

The command "func:`VarListArith` returns a list of all variables that appear
arithmetically in the expression `expr`. This is implemented through
`VarListSome` by restricting to the arithmetic functions `+`, `-`,
`*`, `/`.  Arguments of other functions are not checked.

Note that since the operators `+` and `-` are prefix as well as infix
operators, it is currently required to use `Atom("+")` to obtain the
unevaluated atom `+`.

**Example:**

```
In> VarList(Sin(x))
Out> {x};
In> VarList(x+a*y)
Out> {x,a,y};
In> VarListSome(x+a*y, {Atom("+")})
Out> {x,a*y};
In> VarListArith(x+y*Cos(Ln(x)/x))
Out> {x,y,Cos(Ln(x)/x)}
In> VarListArith(x+a*y^2-1)
Out> {x,a,y^2};

```

> **See also:** [IsFreeOf](predicates.md#isfreeofvar-expr), `IsVariable`, [FuncList](lists.md#funclistexpr), [HasExpr](predicates.md#hasexprexpr-x), [HasFunc](predicates.md#hasfuncexpr-func)


### FuncList(expr)

list of functions used in an expression

The command [FuncList](lists.md#funclistexpr) returns a list of all function atoms that appear
in the expression `expr`. The expression is recursively traversed.

**Example:**

```
In> FuncList(x+y*Cos(Ln(x)/x))
Out> {+,*,Cos,/,Ln};

```

> **See also:** [VarList](lists.md#varlistexpr), [HasExpr](predicates.md#hasexprexpr-x), [HasFunc](predicates.md#hasfuncexpr-func)


### FuncListArith(expr)

list of functions used in an expression

[FuncListArith](lists.md#funclistarithexpr) is defined through [FuncListSome](lists.md#funclistsomeexpr-list) to look only
at arithmetic operations `+`, `-`, `*`, `/`.

**Example:**

```
In> FuncListArith(x+y*Cos(Ln(x)/x))
Out> {+,*,Cos};

```

> **See also:** [VarList](lists.md#varlistexpr), [HasExpr](predicates.md#hasexprexpr-x), [HasFunc](predicates.md#hasfuncexpr-func)


### FuncListSome(expr, list)

list of functions used in an expression

The command [FuncListSome](lists.md#funclistsomeexpr-list) does the same as [FuncList](lists.md#funclistexpr), except it
only looks at arguments of a given `list` of functions. All other functions
become opaque (as if they do not contain any other functions).  For example,
`FuncList(a + Sin(b-c))` will see that the expression has a `{-}`
operation and return {{+,Sin,-}}, but `FuncListSome(a + Sin(b-c), {+})`
will not look at arguments of [Sin](elementary.md#sinx) and will return `{+,Sin}`.

Note that since the operators `+` and `-` are prefix as
well as infix operators, it is currently required to use
`Atom("+")` to obtain the unevaluated atom `+`.

**Example:**

```
In> FuncListSome({a+b*2,c/d},{List})
Out> {List,+,/};

```

> **See also:** [VarList](lists.md#varlistexpr), [HasExpr](predicates.md#hasexprexpr-x), [HasFunc](predicates.md#hasfuncexpr-func)


### PrintList(list [, padding])

print list with padding

Prints `list` and inserts the `padding` string between each
pair of items of the list. Items of the list which are strings are
printed without quotes, unlike [Write](io.md#writeexpr). Items of the list which
are themselves lists are printed inside braces `{}`. If padding
is not specified, standard one is used ", " (comma, space).

**Example:**

```
In> PrintList({a,b,{c, d}}, `` .. ``)
Out> `` a ..  b .. { c ..  d}``;

```

> **See also:** [Write](io.md#writeexpr), [WriteString](io.md#writestringstring)


### Table(body, var, from, to, step)

evaluate while some variable ranges over interval

This command generates a list of values from `body`, by assigning
variable `var` values from `from` up to `to`, incrementing
`step` each time. So, the variable `var` first gets the value
`from`, and the expression `body` is evaluated. Then the value
`from`+`step` is assigned to `var` and the expression
`body` is again evaluated. This continues, incrementing `var`
with `step` on every iteration, until `var` exceeds `to`. At
that moment, all the results are assembled in a list and this list
is returned.

**Example:**

```
In> Table(i!, i, 1, 9, 1);
Out> {1,2,6,24,120,720,5040,40320,362880};
In> Table(i, i, 3, 16, 4);
Out> {3,7,11,15};
In> Table(i^2, i, 10, 1, -1);
Out> {100,81,64,49,36,25,16,9,4,1};

```

> **See also:** `For`, [MapSingle](lists.md#mapsinglefn-list), `..`:, [TableForm](lists.md#tableformlist)


### TableForm(list)

print each entry in a list on a line

This functions writes out the list `list` in a better readable
form, by printing every element in the list on a separate line.

**Example:**

```
In> TableForm(Table(i!, i, 1, 10, 1));

1
2
6
24
120
720
5040
40320
362880
3628800
Out> True;

```

> **See also:** [PrettyForm](io.md#prettyformexpr), [Echo](io.md#echoitem), [Table](lists.md#tablebody-var-from-to-step)


## Destructive operations

Destructive commands run faster than their nondestructive counterparts because
the latter copy the list before they alter it.

### DestructiveAppend(list, expr)

destructively append an entry to a list

This is the destructive counterpart of [Append](lists.md#appendlist-expr). This command yields
the same result as the corresponding call to [Append](lists.md#appendlist-expr), but the original
list is modified. So if a variable is bound to `list`, it will now be bound
to the list with the expression `expr` inserted.

**Example:**

```
In> lst := {a,b,c,d};
Out> {a,b,c,d};
In> Append(lst, 1);
Out> {a,b,c,d,1};
In> lst
Out> {a,b,c,d};
In> DestructiveAppend(lst, 1);
Out> {a,b,c,d,1};
In> lst;
Out> {a,b,c,d,1};

```

> **See also:** [Concat](lists.md#concatlist1-list2), `:`, [Append](lists.md#appendlist-expr)


### DestructiveDelete(list, n)

delete an element destructively from a list

This is the destructive counterpart of :func`Delete`. This command yields the
same result as the corresponding call to [Delete](lists.md#deletelist-n), but the original
list is modified. So if a variable is bound to `list`, it will now be bound
to the list with the `n`-th entry removed.

**Example:**

```
In> lst := {a,b,c,d,e,f};
Out> {a,b,c,d,e,f};
In> Delete(lst, 4);
Out> {a,b,c,e,f};
In> lst;
Out> {a,b,c,d,e,f};
In> DestructiveDelete(lst, 4);
Out> {a,b,c,e,f};
In> lst;
Out> {a,b,c,e,f};

```

> **See also:** [Delete](lists.md#deletelist-n), [DestructiveInsert](lists.md#destructiveinsertlist-n-expr), [DestructiveReplace](lists.md#destructivereplacelist-n-expr)


### DestructiveInsert(list, n, expr)

insert an element destructively into a list

This is the destructive counterpart of [Insert](lists.md#insertlist-n-expr). This command
yields the same result as the corresponding call to [Insert](lists.md#insertlist-n-expr), but
the original list is modified. So if a variable is bound to
`list`, it will now be bound to the list with the expression
`expr` inserted.

**Example:**

```
In> lst := {a,b,c,d};
Out> {a,b,c,d};
In> Insert(lst, 2, x);
Out> {a,x,b,c,d};
In> lst;
Out> {a,b,c,d};
In> DestructiveInsert(lst, 2, x);
Out> {a,x,b,c,d};
In> lst;
Out> {a,x,b,c,d};

```

> **See also:** [Insert](lists.md#insertlist-n-expr), [DestructiveDelete](lists.md#destructivedeletelist-n), [DestructiveReplace](lists.md#destructivereplacelist-n-expr)


### DestructiveReplace(list, n, expr)

replace an entry destructively in a list

**param list:**list of which an entry should be replaced

**param n:**index of entry to replace

**param expr:**expression to replace the `n`-th entry with

This is the destructive counterpart of [Replace](lists.md#replacelist-n-expr). This command
yields the same result as the corresponding call to [Replace](lists.md#replacelist-n-expr), but
the original list is modified. So if a variable is bound to
`list`, it will now be bound to the list with the expression
`expr` inserted.

**Example:**

```
In> lst := {a,b,c,d,e,f};
Out> {a,b,c,d,e,f};
In> Replace(lst, 4, x);
Out> {a,b,c,x,e,f};
In> lst;
Out> {a,b,c,d,e,f};
In> DestructiveReplace(lst, 4, x);
Out> {a,b,c,x,e,f};
In> lst;
Out> {a,b,c,x,e,f};

```

> **See also:** [Replace](lists.md#replacelist-n-expr), [DestructiveDelete](lists.md#destructivedeletelist-n), [DestructiveInsert](lists.md#destructiveinsertlist-n-expr)


### DestructiveReverse(list)

reverse a list destructively

This command reverses `list` in place, so that the original is
destroyed. This means that any variable bound to `list` will now
have an undefined content, and should not be used any more.  The
reversed list is returned.

**Example:**

```
In> lst := {a,b,c,13,19};
Out> {a,b,c,13,19};
In> revlst := DestructiveReverse(lst);
Out> {19,13,c,b,a};
In> lst;
Out> {a};

```

> **See also:** [FlatCopy](lists.md#flatcopylist), [Reverse](lists.md#reverselist)


## Set operations

### Intersection(l1, l2)

return the intersection of two lists

The intersection of the lists `l1` and `l2` is determined and
returned. The intersection contains all elements that occur in both
lists. The entries in the result are listed in the same order as in
`l1`. If an expression occurs multiple times in both `l1` and
`l2`, then it will occur the same number of times in the result.

**Example:**

```
In> Intersection({a,b,c}, {b,c,d});
Out> {b,c};
In> Intersection({a,e,i,o,u}, {f,o,u,r,t,e,e,n});
Out> {e,o,u};
In> Intersection({1,2,2,3,3,3}, {1,1,2,2,3,3});
Out> {1,2,2,3,3};

```

> **See also:** [Union](lists.md#unionl1-l2), [Difference](lists.md#differencel1-l2)


### Union(l1, l2)

return the union of two lists

The union of the lists `l1` and `l2` is determined and
returned. The union contains all elements that occur in one or both
of the lists. In the resulting list, any element will occur only
once.

**Example:**

```
In> Union({a,b,c}, {b,c,d});
Out> {a,b,c,d};
In> Union({a,e,i,o,u}, {f,o,u,r,t,e,e,n});
Out> {a,e,i,o,u,f,r,t,n};
In> Union({1,2,2,3,3,3}, {2,2,3,3,4,4});
Out> {1,2,3,4};

```

> **See also:** [Intersection](lists.md#intersectionl1-l2), [Difference](lists.md#differencel1-l2)


### Difference(l1, l2)

return the difference of two lists

The difference of the lists `l1` and `l2` is determined and
returned. The difference contains all elements that occur in `l1`
but not in `l2`. The order of elements in `l1` is preserved. If
a certain expression occurs `n1` times in the first list and
`n2` times in the second list, it will occur `n1-n2` times in
the result if `n1` is greater than `n2` and not at all
otherwise.

**Example:**

```
In> Difference({a,b,c}, {b,c,d});
Out> {a};
In> Difference({a,e,i,o,u}, {f,o,u,r,t,e,e,n});
Out> {a,i};
In> Difference({1,2,2,3,3,3}, {2,2,3,4,4});
Out> {1,3,3};

```

> **See also:** [Intersection](lists.md#intersectionl1-l2), [Union](lists.md#unionl1-l2)


## Associative map

### Assoc(key, alist)

return element stored in association list

The association list `alist` is searched for an entry stored with
index `key`. If such an entry is found, it is returned. Otherwise
the atom `Empty` is returned.

Association lists are represented as a list of two-entry lists. The
first element in the two-entry list is the key, the second element
is the value stored under this key.

The call `Assoc(key, alist)` can (probably more intuitively) be
accessed as `alist[key]`.

**Example:**

```
In> writer := {};
Out> {};
In> writer[``Iliad``] := ``Homer``;
Out> True;
In> writer[``Henry IV``] := ``Shakespeare``;
Out> True;
In> writer[``Ulysses``] := ``James Joyce``;
Out> True;
In> Assoc(``Henry IV``, writer);
Out> {``Henry IV``,``Shakespeare``};
In> Assoc(``War and Peace``, writer);
Out> Empty;

```

> **See also:** [AssocIndices](lists.md#associndicesalist), `[]`, `:=`, [AssocDelete](lists.md#assocdeletealist-key)


### AssocIndices(alist)

return the keys in an association list

All the keys in the association list `alist` are assembled in a
list and this list is returned.

**Example:**

```
In> writer := {};
Out> {};
In> writer[``Iliad``] := ``Homer``;
Out> True;
In> writer[``Henry IV``] := ``Shakespeare``;
Out> True;
In> writer[``Ulysses``] := ``James Joyce``;
Out> True;
In> AssocIndices(writer);
Out> {``Iliad``,``Henry IV``,``Ulysses``};

```

> **See also:** [Assoc](lists.md#assockey-alist), [AssocDelete](lists.md#assocdeletealist-key)


### AssocDelete(alist, key)

           AssocDelete(alist, {key, value})

delete an entry in an association list

The key {`key`} in the association list `alist` is deleted. (The
list itself is modified.) If the key was found and successfully
deleted, returns `True`, otherwise if the given key was not found,
the function returns `False`.

The second, longer form of the function deletes the entry that has both the
specified key and the specified value. It can be used for two purposes:

* to make sure that we are deleting the right value;
* if several values are stored on the same key, to delete the specified entry
  (see the last example).

At most one entry is deleted.

**Example:**

```
In> writer := {};
Out> {};
In> writer[``Iliad``] := ``Homer``;
Out> True;
In> writer[``Henry IV``] := ``Shakespeare``;
Out> True;
In> writer[``Ulysses``] := ``James Joyce``;
Out> True;
In> AssocDelete(writer, ``Henry IV``)
Out> True;
In> AssocDelete(writer, ``Henry XII``)
Out> False;
In> writer
Out> {{``Ulysses``,``James Joyce``},
{``Iliad``,``Homer``}};
In> DestructiveAppend(writer,
{``Ulysses``, ``Dublin``});
Out> {{``Iliad``,``Homer``},{``Ulysses``,``James Joyce``},
{``Ulysses``,``Dublin``}};
In> writer[``Ulysses``];
Out> ``James Joyce``;
In> AssocDelete(writer,{``Ulysses``,``James Joyce``});
Out> True;
In> writer
Out> {{``Iliad``,``Homer``},{``Ulysses``,``Dublin``}};

```

> **See also:** [Assoc](lists.md#assockey-alist), [AssocIndices](lists.md#associndicesalist)


## Sorting

### BubbleSort(list, compare)

sort a list

This command returns `list` after it is sorted using `compare` to compare
elements. The function `compare` should accept two arguments, which will be
elements of `list`, and compare them. It should return `True` if in the
sorted list the second argument should come after the first one, and
`False` otherwise.

The function [BubbleSort](lists.md#bubblesortlist-compare) uses the so-called [bubble sort](http://en.wikipedia.org/wiki/Bubble_sort) algorithm to do the sorting by
swapping elements that are out of order. This algorithm is easy to implement,
though it is not particularly fast. The sorting time is proportional to
$n^2$ where $n$ is the length of the list.

**Example:**

```
In> BubbleSort({4,7,23,53,-2,1}, "<");
Out> {-2,1,4,7,23,53};

```

> **See also:** [HeapSort](lists.md#heapsortlist-compare)


### HeapSort(list, compare)

sort a list

This command returns `list` after it is sorted using `compare` to compare
elements. The function `compare` should accept two arguments, which will be
elements of `list`, and compare them. It should return `True` if in the
sorted list the second argument should come after the first one, and
`False` otherwise.

The function [HeapSort](lists.md#heapsortlist-compare) uses the `heapsort algorithm <http://en.wikipedia.org/wiki/Heapsort>` and is much faster for large lists.
The sorting time is proportional to $n\ln(n)$ where $n$ is the
length of the list.

**Example:**

```
In> HeapSort({4,7,23,53,-2,1}, ``>``);
Out> {53,23,7,4,1,-2};

```

> **See also:** [BubbleSort](lists.md#bubblesortlist-compare)


## Stack and queue operations

### Push(stack, expr)

add an element on top of a stack

This is part of a simple implementation of a stack, internally
represented as a list. This command pushes the expression `expr`
on top of the stack, and returns the stack afterwards.

**Example:**

```
In> stack := {};
Out> {};
In> Push(stack, x);
Out> {x};
In> Push(stack, x2);
Out> {x2,x};
In> PopFront(stack);
Out> x2;

```

> **See also:** [Pop](lists.md#popstack-n), [PopFront](lists.md#popfrontstack), [PopBack](lists.md#popbackstack)


### Pop(stack, n)

remove an element from a stack

This is part of a simple implementation of a stack, internally
represented as a list. This command removes the element with index
`n` from the stack and returns this element. The top of the stack
is represented by the index 1. Invalid indices, for example indices
greater than the number of element on the stack, lead to an error.

**Example:**

```
In> stack := {};
Out> {};
In> Push(stack, x);
Out> {x};
In> Push(stack, x2);
Out> {x2,x};
In> Push(stack, x3);
Out> {x3,x2,x};
In> Pop(stack, 2);
Out> x2;
In> stack;
Out> {x3,x};

```

> **See also:** [Push](lists.md#pushstack-expr), [PopFront](lists.md#popfrontstack), [PopBack](lists.md#popbackstack)


### PopFront(stack)

remove an element from the top of a stack

This is part of a simple implementation of a stack, internally
represented as a list. This command removes the element on the top
of the stack and returns it. This is the last element that is
pushed onto the stack.

**Example:**

```
In> stack := {};
Out> {};
In> Push(stack, x);
Out> {x};
In> Push(stack, x2);
Out> {x2,x};
In> Push(stack, x3);
Out> {x3,x2,x};
In> PopFront(stack);
Out> x3;
In> stack;
Out> {x2,x};

```

> **See also:** [Push](lists.md#pushstack-expr), [Pop](lists.md#popstack-n), [PopBack](lists.md#popbackstack)


### PopBack(stack)

remove an element from the bottom of a stack

This is part of a simple implementation of a stack, internally
represented as a list. This command removes the element at the
bottom of the stack and returns this element. Of course, the stack
should not be empty.

**Example:**

```
In> stack := {};
Out> {};
In> Push(stack, x);
Out> {x};
In> Push(stack, x2);
Out> {x2,x};
In> Push(stack, x3);
Out> {x3,x2,x};
In> PopBack(stack);
Out> x;
In> stack;
Out> {x3,x2};

```

> **See also:** [Push](lists.md#pushstack-expr), [Pop](lists.md#popstack-n), [PopFront](lists.md#popfrontstack)


### Global stack

The functions below operate on a global stack, currently implemented as a list
that is not accessible externally (it is protected through
`LocalSymbols`).

### GlobalPop()

           GlobalPop(var)

restore variables using a global stack

[GlobalPop](lists.md#globalpop) removes the last pushed value from the stack. If a variable
name is given, the variable is assigned, otherwise the popped value is
returned. If the global stack is empty, an error message is printed.

> **See also:** [GlobalPush](lists.md#globalpushexpr), [Pop](lists.md#popstack-n), [PopFront](lists.md#popfrontstack)


### GlobalPush(expr)

save variables using a global stack

**Example:**

```
In> GlobalPush(3)
Out> 3;
In> GlobalPush(Sin(x))
Out> Sin(x);
In> GlobalPop(x)
Out> Sin(x);
In> GlobalPop(x)
Out> 3;
In> x
Out> 3;

```

> **See also:** [GlobalPop](lists.md#globalpop), [Push](lists.md#pushstack-expr), [PopFront](lists.md#popfrontstack)

