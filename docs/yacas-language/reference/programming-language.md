# Rules and Language Core
> **Scope:** This page classifies low-level entries as Rust core commands, standard-script adapters, or migration entries. The [availability audit](availability.md) records validation evidence.

## Programming

This chapter describes functions useful for writing Yacas scripts.

```
/* --- Start of comment
*/ --- end of comment
// --- Beginning of one-line comment

 /* comment */
 // comment

```

Introduce a comment block in a source file, similar to C++ comments.
`//` makes everything until the end of the line a comment, while `/*`
and `*/` may delimit a multi-line comment.

**Example:**

```
a+b; // get result
a + /* add them */ b;

```

<a id="progexpr1-expr2"></a>

### Prog(expr1, expr2, ...)

block of statements

The [Prog](programming-language.md#progexpr1-expr2) and the `[ ... ]` construct have the same effect: they
evaluate all arguments in order and return the result of the last evaluated
expression.

`Prog(a,b);` is the same as typing `[a;b;];` and is very useful for
writing out function bodies. The `[ ... ]` construct is a syntactically
nicer version of the [Prog](programming-language.md#progexpr1-expr2) call; it is converted into `Prog(...)`
during the parsing stage.

### Bodied(op, precedence)

declare `op` as [bodied function](../glossary.md#bodied-function)

Declares a special syntax for the function to be parsed as a
[bodied function](../glossary.md#bodied-function). For example:

  For(pre, condition, post) statement;

Here the function `For` has 4 arguments and the last argument is placed
outside the parentheses.

The `precedence` of a [bodied function](../glossary.md#bodied-function) refers to how tightly the
last argument is bound to the parentheses.  This makes a difference when the
last argument contains other operators. For example, when taking the
derivative `D(x) Sin(x)+Cos(x)` both [Sin](elementary.md#sinx) and [Cos](elementary.md#cosx) are under
the derivative because the bodied function `D` binds less tightly than the
infix operator `+`.

> **See also:** [IsBodied](programming-language.md#isbodiedop), [OpPrecedence](programming-language.md#opprecedenceop)


### Infix(op[, precedence])

declare `op` as infix operator

Declares a special syntax for the function `op` to be parsed as an infix
operator. Precedence is optional (will be set to 0 by default).

Infix functions must have two arguments and are syntactically placed between
their arguments.  Names of infix functions can be arbitrary, although for
reasons of readability they are usually made of non-alphabetic characters.

> **See also:** [IsBodied](programming-language.md#isbodiedop), [OpPrecedence](programming-language.md#opprecedenceop)


### Postfix(op[, precedence])

declare `op` as postfix operator

Declares a special syntax for the function `op` to be parsed as a  postfix
operator. Precedence is optional (will be set to 0 by default).

Postfix functions must have one argument and are syntactically placed after
their argument.

> **See also:** [IsBodied](programming-language.md#isbodiedop), [OpPrecedence](programming-language.md#opprecedenceop)


### Prefix(op[, precedence])

declare `op` as prefix operator

Declares a special syntax for the function `op` to be parsed as a prefix
operator. Precedence is optional (will be set to 0 by default).

Prefix functions must have one argument and are syntactically placed before
their argument. Function name can be any string but meaningful usage and
readability would require it to be either made up entirely of letters or
entirely of non-letter characters (such as "+", ":" etc.).

**Example:**

```
In> YY x := x+1;
CommandLine(1) : Error parsing expression

In> Prefix("YY", 2)
Out> True;
In> YY x := x+1;
Out> True;
In> YY YY 2*3
Out> 12;
In> Infix("##", 5)
Out> True;
In> a ## b ## c
Out> a##b##c;

```

Note that, due to a current parser limitation, a function atom that
is declared prefix cannot be used by itself as an argument. :

  In> YY
  CommandLine(1) : Error parsing expression

> **See also:** [IsBodied](programming-language.md#isbodiedop), [OpPrecedence](programming-language.md#opprecedenceop)


### IsBodied(op)

check for function syntax

**param op:**string, the name of a function

Check whether the function with given name {"op"} has been declared as a
"bodied", infix, postfix, or prefix operator, and  return `True` or `False`.

### IsInfix(op)

check for function syntax

**param op:**string, the name of a function

Check whether the function with given name {"op"} has been declared as a
"bodied", infix, postfix, or prefix operator, and  return `True` or `False`.

### IsPostfix(op)

check for function syntax

**param op:**string, the name of a function

Check whether the function with given name {"op"} has been declared as a
"bodied", infix, postfix, or prefix operator, and  return `True` or `False`.

### IsPrefix(op)

check for function syntax

**param op:**string, the name of a function

Check whether the function with given name {"op"} has been declared as a
"bodied", infix, postfix, or prefix operator, and  return `True` or `False`.

**Example:**

```
In> IsInfix("+");
Out> True;
In> IsBodied("While");
Out> True;
In> IsBodied("Sin");
Out> False;
In> IsPostfix("!");
Out> True;

```

> **See also:** [Bodied](programming-language.md#bodiedop-precedence), [OpPrecedence](programming-language.md#opprecedenceop)


### OpPrecedence(op)

get operator precedence

**param op:**string, the name of a function

Returns the precedence of the function named "op" which should have
been declared as a bodied function or an infix, postfix, or prefix
operator. Generates an error message if the string str does not
represent a type of function that can have precedence.

For infix operators, right precedence can differ from left
precedence. Bodied functions and prefix operators cannot have left
precedence, while postfix operators cannot have right precedence;
for these operators, there is only one value of precedence.

### OpLeftPrecedence(op)

get operator precedence

**param op:**string, the name of a function

Returns the precedence of the function named "op" which should have
been declared as a bodied function or an infix, postfix, or prefix
operator. Generates an error message if the string str does not
represent a type of function that can have precedence.

For infix operators, right precedence can differ from left
precedence. Bodied functions and prefix operators cannot have left
precedence, while postfix operators cannot have right precedence;
for these operators, there is only one value of precedence.

### OpRightPrecedence(op)

get operator precedence

**param string op:**name of a function

Returns the precedence of the function named "op" which should have
been declared as a bodied function or an infix, postfix, or prefix
operator. Generates an error message if the string str does not
represent a type of function that can have precedence.

For infix operators, right precedence can differ from left
precedence. Bodied functions and prefix operators cannot have left
precedence, while postfix operators cannot have right precedence;
for these operators, there is only one value of precedence.

**Example:**

```
In> OpPrecedence("+")
Out> 6;
In> OpLeftPrecedence("!")
Out> 0;

```

### RightAssociative(op)

declare associativity

**param op:**string, the name of a function

This makes the operator right-associative. For example: :

  RightAssociative("*")

would make multiplication right-associative. Take care not to abuse
this function, because the reverse, making an infix operator
left-associative, is not implemented. (All infix operators are by
default left-associative until they are declared to be
right-associative.)

> **See also:** [OpPrecedence](programming-language.md#opprecedenceop)


### LeftPrecedence(op, precedence)

set operator precedence

**param op:**string, the name of a function

**param precedence:**nonnegative integer

{"op"} should be an infix operator. This function call tells the
infix expression printer to bracket the left or right hand side of
the expression if its precedence is larger than precedence.

This functionality was required in order to display expressions
like {a-(b-c)} correctly. Thus, {a+b+c} is the same as {a+(b+c)},
but {a-(b-c)} is not the same as {a-b-c}.

Note that the left and right precedence of an infix operator does
not affect the way Yacas interprets expressions typed by the
user. You cannot make Yacas parse {a-b-c} as {a-(b-c)} unless you
declare the operator "{-}" to be right-associative.

> **See also:** [OpPrecedence](programming-language.md#opprecedenceop), [OpLeftPrecedence](programming-language.md#opleftprecedenceop),

             [OpRightPrecedence](programming-language.md#oprightprecedenceop), [RightAssociative](programming-language.md#rightassociativeop)

### RightPrecedence

set operator precedence(op, precedence)

**param op:**string, the name of a function

**param precedence:**nonnegative integer

{"op"} should be an infix operator. This function call tells the
infix expression printer to bracket the left or right hand side of
the expression if its precedence is larger than precedence.

This functionality was required in order to display expressions
like {a-(b-c)} correctly. Thus, {a+b+c} is the same as {a+(b+c)},
but {a-(b-c)} is not the same as {a-b-c}.

Note that the left and right precedence of an infix operator does
not affect the way Yacas interprets expressions typed by the
user. You cannot make Yacas parse {a-b-c} as {a-(b-c)} unless you
declare the operator "{-}" to be right-associative.

> **See also:** [OpPrecedence](programming-language.md#opprecedenceop), [OpLeftPrecedence](programming-language.md#opleftprecedenceop),

             [OpRightPrecedence](programming-language.md#oprightprecedenceop), [RightAssociative](programming-language.md#rightassociativeop)

### RuleBase(name, params)

define function with a fixed number of arguments

**param name:**string, name of function

**param params:**list of arguments to function

Define a new rules table entry for a function "name", with {params}
as the parameter list. Name can be either a string or simple atom.

In the context of the transformation rule declaration facilities
this is a useful function in that it allows the stating of argument
names that can he used with HoldArg.

Functions can be overloaded: the same function can be defined with
different number of arguments.

> **See also:** [MacroRuleBase](programming-language.md#macrorulebase), [RuleBaseListed](programming-language.md#rulebaselistedname-params),

             [MacroRuleBaseListed](programming-language.md#macrorulebaselisted), [HoldArg](programming-language.md#holdargoperator-parameters),
             [Retract](programming-language.md#retractfunction-arity)

### RuleBaseListed(name, params)

define function with variable number of arguments

**param name:**string, name of function

**param params:**list of arguments to function

The command {RuleBaseListed} defines a new function. It essentially
works the same way as {RuleBase}, except that it declares a new
function with a variable number of arguments. The list of
parameters {params} determines the smallest number of arguments
that the new function will accept. If the number of arguments
passed to the new function is larger than the number of parameters
in {params}, then the last argument actually passed to the new
function will be a list containing all the remaining arguments.

A function defined using {RuleBaseListed} will appear to have the
arity equal to the number of parameters in the {param} list, and it
can accept any number of arguments greater or equal than that. As a
consequence, it will be impossible to define a new function with
the same name and with a greater arity.

The function body will know that the function is passed more
arguments than the length of the {param} list, because the last
argument will then be a list. The rest then works like a
{RuleBase}-defined function with a fixed number of
arguments. Transformation rules can be defined for the new function
as usual.

**Example:**

The definitions :

  RuleBaseListed("f",{a,b,c})
  10 # f(_a,_b,{_c,_d}) <--
    Echo({"four args",a,b,c,d});
  20 # f(_a,_b,c_IsList) <--
    Echo({"more than four args",a,b,c});
  30 # f(_a,_b,_c) <-- Echo({"three args",a,b,c});

give the following interaction: :

  In> f(A)
  Out> f(A);
  In> f(A,B)
  Out> f(A,B);
  In> f(A,B,C)
  three args A B C
  Out> True;
  In> f(A,B,C,D)
  four args A B C D
  Out> True;
  In> f(A,B,C,D,E)
  more than four args A B {C,D,E}
  Out> True;
  In> f(A,B,C,D,E,E)
  more than four args A B {C,D,E,E}
  Out> True;

The function {f} now appears to occupy all arities greater than 3: :

  In> RuleBase("f", {x,y,z,t});
  CommandLine(1) : Rule base with this arity already defined

> **See also:** [RuleBase](programming-language.md#rulebasename-params), [Retract](programming-language.md#retractfunction-arity), [Echo](io.md#echoitem)


### bodied Rule(body, operator, arity, precedence, predicate)

define a rewrite rule

**param "operator":**string, name of function

**param arity:**

**param precedence:**integers

**param predicate:**function returning boolean

**param body:**expression, body of rule

Define a rule for the function "operator" with "arity",
"precedence", "predicate" and "body". The "precedence" goes from
low to high: rules with low precedence will be applied first.

The arity for a rules database equals the number of
arguments. Different rules data bases can be built for functions
with the same name but with a different number of arguments.

Rules with a low precedence value will be tried before rules with a
high value, so a rule with precedence 0 will be tried before a rule
with precedence 1.

### HoldArg(operator, parameters)

mark argument as not evaluated

{"operator"} -- string, name of a function
{parameter} -- atom, symbolic name of parameter

Specify that parameter should not be evaluated before used. This
will be declared for all arities of "operator", at the moment this
function is called, so it is best called after all {RuleBase} calls
for this operator.  "operator" can be a string or atom specifying
the function name.

The {parameter} must be an atom from the list of symbolic arguments
used when calling {RuleBase}.

> **See also:** [RuleBase](programming-language.md#rulebasename-params), [HoldArgNr](programming-language.md#holdargnrfunction-arity-argnum),

             [RuleBaseArgList](programming-language.md#rulebasearglistoperator-arity)

### Retract(function, arity)

erase rules for a function

{"function"} -- string, name of function
{arity} -- positive integer

Remove a rulebase for the function named {"function"} with the
specific {arity}, if it exists at all. This will make Yacas forget
all rules defined for a given function. Rules for functions with
the same name but different arities are not affected.

Assignment {:=} of a function does this to the function being
(re)defined.

> **See also:** [RuleBaseArgList](programming-language.md#rulebasearglistoperator-arity), [RuleBase](programming-language.md#rulebasename-params), `:=`


### UnFence(operator, arity)

change local variable scope for a function

{"operator"} -- string, name of function
{arity} -- positive integers

When applied to a user function, the bodies defined for the rules
for "operator" with given arity can see the local variables from
the calling function. This is useful for defining macro-like
procedures (looping and such).

The standard library functions {For} and {ForEach} use {UnFence}.

### HoldArgNr(function, arity, argNum)

specify argument as not evaluated

{"function"} -- string, function name
{arity}, {argNum} -- positive integers

Declares the argument numbered {argNum} of the function named
{"function"} with specified {arity} to be unevaluated
("held"). Useful if you don't know symbolic names of parameters,
for instance, when the function was not declared using an explicit
{RuleBase} call. Otherwise you could use {HoldArg}.

> **See also:** [HoldArg](programming-language.md#holdargoperator-parameters), [RuleBase](programming-language.md#rulebasename-params)


### RuleBaseArgList(operator, arity)

obtain list of arguments

{"operator"} -- string, name of function
{arity} -- integer

Returns a list of atoms, symbolic parameters specified in the
{RuleBase} call for the function named {"operator"} with the
specific {arity}.

> **See also:** [RuleBase](programming-language.md#rulebasename-params), [HoldArgNr](programming-language.md#holdargnrfunction-arity-argnum), [HoldArg](programming-language.md#holdargoperator-parameters)


### MacroSet

define rules in functions

### MacroClear

define rules in functions

### MacroLocal

define rules in functions

### MacroRuleBase

define rules in functions

### MacroRuleBaseListed

define rules in functions

### MacroRule

define rules in functions

These functions have the same effect as their non-macro
counterparts, except that their arguments are evaluated before the
required action is performed.  This is useful in macro-like
procedures or in functions that need to define new rules based on
parameters.

Make sure that the arguments of {Macro}... commands evaluate to
expressions that would normally be used in the non-macro versions!

> **See also:** [Set](vars.md#setvar-exp), [Clear](vars.md#clearvar), [Local](vars.md#localvar),

             [RuleBase](programming-language.md#rulebasename-params), `Rule`, [Backquoting](programming-language.md#backquoting)

### Backquoting

macro expansion (LISP-style backquoting)

{expression} -- expression containing "{@var}" combinations to substitute the value of variable "{var}"

Backquoting is a macro substitution mechanism. A backquoted
{expression} is evaluated in two stages: first, variables prefixed
by {@} are evaluated inside an expression, and second, the new
expression is evaluated.

To invoke this functionality, a backquote {`} needs to be placed in front of an expression. Parentheses around the expression are needed because the backquote binds tighter than other operators.

The expression should contain some variables (assigned atoms) with
the special prefix operator {@}. Variables prefixed by {@} will be
evaluated even if they are inside function arguments that are
normally not evaluated (e.g. functions declared with {HoldArg}). If
the {@var} pair is in place of a function name, e.g. "{@f(x)}",
then at the first stage of evaluation the function name itself is
replaced, not the return value of the function (see example); so at
the second stage of evaluation, a new function may be called.

One way to view backquoting is to view it as a parametric
expression generator. {@var} pairs get substituted with the value
of the variable {var} even in contexts where nothing would be
evaluated. This effect can be also achieved using {UnList} and
{Hold} but the resulting code is much more difficult to read and
maintain.

Backquoting builds a substituted expression before evaluating it. Use it
when that construction is part of the intended semantics; no fixed
performance claim is made without measuring the actual rule workload.

**Example:**

This example defines a function that automatically evaluates to
a number as soon as the argument is a number (a lot of functions
do this only when inside a {N(...)} section). :

  In> Decl(f1,f2) := \
  In>   `(@f1(x_IsNumber) <-- N(@f2(x))); Out> True; In> Decl(nSin,Sin) Out> True; In> Sin(1) Out> Sin(1); In> nSin(1) Out> 0.8414709848;

This example assigns the expression {func(value)} to variable
{var}. Normally the first argument of {Set} would be unevaluated. :

  In> SetF(var,func,value) := \
  In>     `(Set(@var,@func(@value))); Out> True; In> SetF(a,Sin,x) Out> True; In> a Out> Sin(x);

> **See also:** [MacroSet](programming-language.md#macroset), [MacroLocal](programming-language.md#macrolocal),

             [MacroRuleBase](programming-language.md#macrorulebase), [Hold](controlflow.md#holdexpr), [HoldArg](programming-language.md#holdargoperator-parameters),
             [DefMacroRuleBase](programming-language.md#defmacrorulebasenameparams)

### DefMacroRuleBase(name,params)

define a function as a macro

{name} -- string, name of a function
{params} -- list of arguments

{DefMacroRuleBase} is similar to {RuleBase}, with the difference
that it declares a macro, instead of a function.  After this call,
rules can be defined for the function "{name}", but their
interpretation will be different.

With the usual functions, the evaluation model is that of the
*applicative-order model of substitution*, meaning that first
the arguments are evaluated, and then the function is applied to
the result of evaluating these arguments. The function is entered,
and the code inside the function can not access local variables
outside of its own local variables.

With macros, the evaluation model is that of the *normal-order
model of substitution*, meaning that all occurrences of
variables in an expression are first substituted into the body of
the macro, and only then is the resulting expression evaluated
*in its calling environment*. This is important, because then
in principle a macro body can access the local variables from the
calling environment, whereas functions can not do that.

As an example, suppose there is a function {square}, which squares
its argument, and a function {add}, which adds its
arguments. Suppose the definitions of these functions are:

  add(x,y) <-- x+y;

and :

  square(x) <-- x*x;

In applicative-order mode (the usual way functions are evaluated),
in the following expression :

  add(square(2),square(3))

first the arguments to {add} get evaluated. So, first {square(2)}
is evaluated.  To evaluate this, first {2} is evaluated, but this
evaluates to itself. Then the {square} function is applied to it,
{2*2}, which returns 4. The same is done for {square(3)}, resulting
in {9}. Only then, after evaluating these two arguments, {add} is
applied to them, which is equivalent to `add(4,9)` resulting in
calling {4+9}, which in turn results in {13}.

In contrast, when {add} is a macro, the arguments to {add} are first
expanded. So :

  add(square(2),square(3))

first expands to :

  square(2) + square(3)

and then this expression is evaluated, as if the user had written
it directly.  In other words, {square(2)} is not evaluated before
the macro has been fully expanded.

Macros are useful for customizing syntax, and compilers can
potentially greatly optimize macros, as they can be inlined in the
calling environment, and optimized accordingly.

Macro expansion creates a substituted expression at runtime. Also, when
one parameter occurs more than once in the macro body, the expanded
expression may evaluate it more than once. Whether this matters should be
measured for the concrete rule workload.

When defining transformation rules for macros, the variables to be
substituted need to be preceded by the {@} operator, similar to the
back-quoting mechanism.  Apart from that, the two are similar, and
all transformation rules can also be applied to macros.

Macros can co-exist with functions with the same name but different
arity.  For instance, one can have a function {foo(a,b)} with two
arguments, and a macro {foo(a,b,c)} with three arguments.

**Example:**

   The following example defines a function {myfor}, and shows one
use, referencing a variable {a} from the calling environment. :

     In> DefMacroRuleBase("myfor",{init,pred,inc,body})
     Out> True;
     In> myfor(_init,_pred,_inc,_body)<--[@init;While(@pred)[@body;@inc;];True;];
     Out> True;
     In> a:=10
     Out> 10;
     In> myfor(i:=1,i<10,i++,Echo(a*i))
     10
     20
     30
     40
     50
     60
     70
     80
     90
     Out> True;
     In> i
     Out> 10;

> **See also:** [RuleBase](programming-language.md#rulebasename-params), [Backquoting](programming-language.md#backquoting),

             [DefMacroRuleBaseListed](programming-language.md#defmacrorulebaselistedname-params)

### DefMacroRuleBaseListed(name, params)

define macro with variable number of arguments

{"name"} -- string, name of function
{params} -- list of arguments to function

This does the same as {DefMacroRuleBase} (define a macro), but with a variable
number of arguments, similar to {RuleBaseListed}.

> **See also:** [RuleBase](programming-language.md#rulebasename-params), [RuleBaseListed](programming-language.md#rulebaselistedname-params),

             [Backquoting](programming-language.md#backquoting), [DefMacroRuleBase](programming-language.md#defmacrorulebasenameparams)

### GarbageCollect()

compatibility no-op

The Rust engine releases expression nodes through `Rc` ownership and interns
symbols in an environment table. It has no tracing garbage collector.
`GarbageCollect()` is retained as a zero-argument compatibility command and
returns `True`; scripts must not rely on it reducing memory use.

### FindFunction(function)

find the library file where a function is defined

{function} -- string, the name of a function

This function is useful for quickly finding the file where a
standard library function is defined. It is likely to only be
useful for developers. The function {FindFunction} scans the {.def}
files that were loaded at start-up.  This means that functions that
are not listed in {.def} files will not be found with
{FindFunction}.

**Example:**

```
In> FindFunction("Sum")
Out> "sums.rep/code.ys";
In> FindFunction("Integrate")
Out> "integrate.rep/code.ys";

```

> **See also:** `Vi`


### Secure(body)

guard the host OS

{body} -- expression

{Secure} evaluates {body} in a "safe" environment, where files
cannot be opened and system calls are not allowed. This can help
protect the system when e.g. a script is sent over the Internet to
be evaluated on a remote computer, which is potentially unsafe.

> **See also:** [SystemCall](misc.md#systemcallstr)
