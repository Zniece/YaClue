# String manipulation

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### StringMid'Set(index,substring,string)

change a substring

**param index:**index of substring to get

**param substring:**substring to store

**param string:**string to store substring in

Set (change) a part of a string. It leaves the original alone,
returning a new changed copy.

**Example:**

```
In> StringMid'Set(3,"XY","abcdef")
Out> "abXYef";

```

> **See also:** [StringMid'Get](strings.md#stringmidgetindexlengthstring), [Length](lists.md#lengthlist)


### StringMid'Get(index,length,string)

retrieve a substring

**param index:**index of substring to get

**param length:**length of substring to get

**param string:**string to get substring from

{StringMid'Get} returns a part of a string. Substrings can also be
accessed using the {[]} operator.

**Example:**

```
In> StringMid'Get(3,2,"abcdef")
Out> "cd";
In> "abcdefg"[2 .. 4]
Out> "bcd";

```

> **See also:** [StringMid'Set](strings.md#stringmidsetindexsubstringstring), [Length](lists.md#lengthlist)


### Atom("string")

convert string to atom

**param "string":**a string

Returns an atom with the string representation given as the
evaluated argument. Example: {Atom("foo");} returns {foo}.

**Example:**

```
In> Atom("a")
Out> a;

```

> **See also:** [String](strings.md#stringatom)


### String(atom)

convert atom to string

**param atom:**an atom

{String} is the inverse of {Atom}: turns {atom} into {"atom"}.

**Example:**

```
In> String(a)
Out> "a";

```

> **See also:** [Atom](strings.md#atomstring)


### ConcatStrings(strings)

concatenate strings

**param strings:**one or more strings

Concatenates strings.

**Example:**

```
In> ConcatStrings("a","b","c")
Out> "abc";

```

> **See also:** [Concat](lists.md#concatlist1-list2)


### PatchString(string)

execute commands between `<?` and `?>` in strings

**param string:**a string to patch

This function does the same as [PatchLoad](io.md#patchloadname), but it works on a string
instead of on the contents of a text file. See [PatchLoad](io.md#patchloadname) for more
details.

**Example:**

```
In> PatchString("Two plus three is <? Write(2+3); ?> ");
Out> "Two plus three is 5 ";

```

> **See also:** [PatchLoad](io.md#patchloadname)


