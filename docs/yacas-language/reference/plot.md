# Plotting

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.
>
> **Current boundary:** `Plot2D` and `Plot3DS` are historical standard-script plotting interfaces. Their default backend attempts to invoke external gnuplot; only `output="data"` returns samples without doing so. YaClue uses the Rust batch sampler in `processing::plot`, invokes none of these backends, and writes no temporary plot file. These interfaces have separate performance and output contracts.


<a id="plot2dexprx0x1optionvalue"></a>

### Plot2D(expr,[x0:x1],[option=value, [...]])

           Plot2D({expr1, expr2, ..., exprn},[x0:x1],[option=value,[...]])

adaptive two-dimensional plotting

**param f(x):**unevaluated expression containing one variables (function to be plotted)

**param list:**list of functions to plot

**param a}, {b:**numbers, plotting range in the $x$ coordinate

**param option:**atom, option name

**param value:**atom, number or string (value of option)

The routine [Plot2D](plot.md#plot2dexprx0x1optionvalue) performs adaptive plotting of one or several
functions  of one variable in the specified range.  The result is presented
as a line given by the equation $y=f(x)$.  Several functions can be
plotted at once.  Various plotting options can be specified.  Output can be
directed to a plotting program (the default is to use  {data}) to a list of
values.    The function parameter {f(x)} must evaluate to a Yacas expression
containing  at most one variable. (The variable does not have to be called
{x}.) Also, {N(f(x))} must evaluate to a real (not complex) numerical value
when given a numerical value of the argument {x}.  If the function {f(x)}
does not satisfy these requirements, an error is raised.    Several functions
may be specified as a list and they do not have to depend on the same
variable, for example, {{f(x), g(y)}}.  The functions will be plotted on the
same graph using the same coordinate ranges.    If you have defined a
function which accepts a number but does not  accept an undefined variable,
{Plot2D} will fail to plot it.  Use {NFunction} to overcome this difficulty.
File-producing backends use `TmpFile()` or the explicit `filename` option.
Use `output="data"` when only coordinate data is required. Verbose mode may
print backend diagnostics.    The current
algorithm uses Newton-Cotes quadratures and some heuristics for error
estimation (see <*yacasdoc://Algo/3/1/*>).  The initial grid of {points+1}
points is refined between any grid points $x0$, $x1$ if the
integral $Integrate(x,a,b)f(x)$ is not approximated to the given
precision by  the existing grid.    Default plotting range is {-5:5}. Range
can also be specified as {x= -5:5} (note the mandatory space separating "{=}"
and "{-}");  currently the variable name {x} is ignored in this case.
Options are of the form {option=value}. Currently supported option names
are: "points", "precision", "depth", "output", "filename", "yrange". Option
values  are either numbers or special unevaluated atoms such as {data}.  If
you need to use the names of these atoms  in your script, strings can be
used. Several option/value pairs may be specified (the function {Plot2D} has
a variable number of arguments).

* {yrange}: the range of ordinates to use for plotting, e.g.
  {yrange=0:20}. If no range is specified, the default is usually
  to leave the choice to the plotting backend.
* {points}: initial number of points (default 23) -- at least that
  many points will be plotted. The initial grid of this many points
  will be adaptively refined.
* {precision}: graphing precision (default $10^(-6)$). This is
  interpreted as the relative precision of computing the integral
  of $f(x)-Min(f(x))$ using the grid points. For a smooth,
  non-oscillating function this value should be roughly 1/(number
  of screen pixels in the plot).
* {depth}: max. refinement depth, logarithmic (default 5) -- means
  there will be at most $2^depth$ extra points per initial grid
  point.
* {output}: name of the plotting backend. Supported names: {data}
  (default).  The {data} backend will return the data as a list of
  pairs such as {{{x1,y1}, {x2,y2}, ...}}.
* {filename}: specify name of the created data file. For example:
  {filename="data1.txt"}.  The default is the name {"output.data"}.
  Note that if several functions are plotted, the data files will
  have a number appended to the given name, for example
  {data.txt1}, {data.txt2}.

Other options may be supported in the future.

The current implementation can deal with a singularity within the
plotting range only if the function {f(x)} returns {Infinity},
{-Infinity} or {Undefined} at the singularity.  If the function
{f(x)} generates a numerical error and fails at a singularity,
{Plot2D} will fail if one of the grid points falls on thez
singularity.  (All grid points are generated by bisection so in
principle the endpoints and the {points} parameter could be chosen
to avoid numerical singularities.)

> **See also:** [V](io.md#vexpression), [NFunction](functional.md#nfunctionnewname-funcname-arglist), [Plot3DS](plot.md#plot3dsexproptionvalue)

<a id="plot3dsexproptionvalue"></a>

### Plot3DS(expr,[option=value, [...]])

           Plot3DS({expr1, expr2, ..., exprn},[option=value,[...]])

three-dimensional (surface) plotting

The routine [Plot3DS](plot.md#plot3dsexproptionvalue) performs adaptive plotting of a function of two
variables in the specified ranges.  The result is presented as a surface
given by the equation $z=f(x,y)$.  Several functions can be plotted at
once, by giving a list of functions.  Various plotting options can be
specified.  Output can be directed to a plotting program (the default is to
use  {data}), to a list of values.    The function parameter {f(x,y)} must
evaluate to a Yacas expression containing  at most two variables. (The
variables do not have to be called {x} and {y}.)  Also, {N(f(x,y))} must
evaluate to a real (not complex) numerical value when given numerical values
of the arguments {x}, {y}.  If the function {f(x,y)} does not satisfy these
requirements, an error is raised.    Several functions may be specified as a
list but they have to depend on the same symbolic variables, for example,
{{f(x,y), g(y,x)}}, but not {{f(x,y), g(a,b)}}.  The functions will be
plotted on the same graph using the same coordinate ranges.    If you have
defined a function which accepts a number but does not  accept an undefined
variable, {Plot3DS} will fail to plot it.  Use {NFunction} to overcome this
difficulty.    File-producing backends use `TmpFile()` or the explicit `filename` option.
Use `output="data"` when only coordinate data is required. Verbose mode may
print backend diagnostics.    The current algorithm uses Newton-Cotes cubatures and some
heuristics for error estimation (see <*yacasdoc://Algo/3/1/*>).  The initial
rectangular grid of {xpoints+1}*{ypoints+1} points is refined within any
rectangle where the integral  of $f(x,y)$ is not approximated to the
given precision by  the existing grid.    Default plotting range is {-5:5} in
both coordinates.  A range can also be specified with a variable name, e.g.
{x= -5:5} (note the mandatory space separating "{=}" and "{-}").  The
variable name {x} should be the same as that used in the function {f(x,y)}.
If ranges are not given with variable names, the first variable encountered
in the function {f(x,y)} is associated with the first of the two ranges.
Options are of the form {option=value}. Currently supported option names are
"points", "xpoints", "ypoints", "precision", "depth", "output", "filename",
"xrange", "yrange", "zrange". Option values  are either numbers or special
unevaluated atoms such as {data}.  If you need to use the names of these
atoms  in your script, strings can be used (e.g. {output="data"}). Several
option/value pairs may be specified (the function {Plot3DS} has a variable
number of arguments).

* {xrange}, {yrange}: optionally override coordinate ranges. Note
  that {xrange} is always the first variable and {yrange} the
  second variable, regardless of the actual variable names.
* {zrange}: the range of the $z$ axis to use for plotting, e.g.
  {zrange=0:20}. If no range is specified, the default is usually
  to leave the choice to the plotting backend. Automatic choice
  based on actual values may give visually inadequate plots if the
  function has a singularity.
* {points}, {xpoints}, {ypoints}: initial number of points (default
  10 each) -- at least that many points will be plotted in each
  coordinate.  The initial grid of this many points will be
  adaptively refined.  If {points} is specified, it serves as a
  default for both {xpoints} and {ypoints}; this value may be
  overridden by {xpoints} and {ypoints} values.
* {precision}: graphing precision (default $0.01$). This is
  interpreted as the relative precision of computing the integral
  of $f(x,y)-Min(f(x,y))$ using the grid points. For a smooth,
  non-oscillating function this value should be roughly 1/(number
  of screen pixels in the plot).
* {depth}: max. refinement depth, logarithmic (default 3) -- means
  there will be at most $2^depth$ extra points per initial grid
  point (in each coordinate).
* {output}: name of the plotting backend. Supported names: {data}
  (default). The {data} backend will return the data as a list of
  triples such as {{{x1, y1, z1}, {x2, y2, z2}, ...}}.

Other options may be supported in the future.

The current implementation can deal with a singularity within the
plotting range only if the function {f(x,y)} returns {Infinity},
{-Infinity} or {Undefined} at the singularity.  If the function
{f(x,y)} generates a numerical error and fails at a singularity,
{Plot3DS} will fail only if one of the grid points falls on the
singularity.  (All grid points are generated by bisection so in
principle the endpoints and the {xpoints}, {ypoints} parameters
could be chosen to avoid numerical singularities.)

The {filename} option is optional if using graphical backends, but
can be used to specify the location of the created data file.

**Example:**

```
In> Plot3DS(a*b^2)
Out> True;
In> V(Plot3DS(Sin(x)*Cos(y),x=0:20, y=0:20,depth=3))
CachedConstant: Info: constant Pi is being
recalculated at precision 10
CachedConstant: Info: constant Pi is being
recalculated at precision 11
Plot3DS: using 1699  points for function Sin(x)*Cos(y)
Plot3DS: max. used 8 subdivisions for Sin(x)*Cos(y)
Plot3DS'datafile: created file '/path/to/output.data1'
Out> True;

```

> **See also:** [V](io.md#vexpression), [NFunction](functional.md#nfunctionnewname-funcname-arglist), [Plot2D](plot.md#plot2dexprx0x1optionvalue)
