# Miscellaneous

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### Time(expr)

measure the time taken by a function

**param expr:**any expression

The function [Time](misc.md#timeexpr) evaluates the expression `expr` and prints the
time in seconds needed for the evaluation. The time is printed to the current
output stream. Measure elapsed time in the Rust host or benchmark harness;
the historical {GetTime} interpreter command is not part of the current core.

The result is the "user time" as reported by the OS, not the real ("wall
clock") time. Therefore, any CPU-intensive processes running alongside Yacas
will not significantly affect the result of [Time](misc.md#timeexpr).

**Example:**

```
In> Time(N(MathLog(1000),40))
0.34 seconds taken
Out> 6.9077552789821370520539743640530926228033;

```



### SystemCall(str)

pass a command to the shell

The command contained in the string `str` is executed by the underlying
operating system. The return value of [SystemCall](misc.md#systemcallstr) is `True` or
`False` according to the exit code of the command.

The [SystemCall](misc.md#systemcallstr) function is not allowed in the body of the
[Secure](programming-language.md#securebody) command.

In a UNIX environment, the command `SystemCall("ls")` would print
the contents of the current directory:

   In> SystemCall("ls")
   AUTHORS
   COPYING
   ChangeLog
   ... (truncated to save space)
   Out> True;

The standard UNIX command `test` returns success or failure
depending on conditions.  For example, the following command will
check if a directory exists:

   In> SystemCall("test -d scripts/")
   Out> True;

Check that a file exists:

   In> SystemCall("test -f COPYING")
   Out> True;
   In> SystemCall("test -f nosuchfile.txt")
   Out> False;

> **See also:** [Secure](programming-language.md#securebody)
