# Engine Containers
> **Scope:** This page distinguishes Rust core commands, standard scripts, and historical compatibility entries. A discoverable name does not imply that every argument boundary has been validated. See the [availability audit](availability.md).

## Engine containers

Arrays and associations are environment-owned containers implemented by the
Rust engine. Historical predicate names use the term “generic” for script
compatibility.

{IsGeneric}, {GenericTypeName}, the four basic {Array'} operations, and the
{Association'} operations documented below are Rust core commands.
{Array'CreateFromList} and {Array'ToList} are convenience functions from
`array.rep/code.ys`.

### IsGeneric(object)

check for generic object

Returns `True` if an object is of a generic object type.

### GenericTypeName(object)

get type name

Returns a string representation of the name of a generic object.

**Example:**

```
In> GenericTypeName(Array'Create(10,1))
Out> "Array";

```

### Array'Create(size, init)

create array

**param size:**size of the array

**param init:**initial value

Creates an array with `size` elements, all initialized to the
value `init`.

### Array'Size(array)

array size

**param array:**an array

**returns:**array size (number of elements in the array)

### Array'Get(array,index)

fetch array element

**param array:**an array

**param index:**an index

**returns:**the element of `array` at position `index`


   Array indices are one-based, which means that the first element
   is indexed by 1.

Arrays can also be accessed through the `[]` operators. So
`array[index]` would return the same as ``Array'Get(array,
index)``.

### Array'Set(array,index,element)

set array element

Sets the element at position index in the array passed to the value
passed in as argument to element. Arrays are treated as base-one,
so {index} set to 1 would set first element.

Arrays can also be accessed through the {[]} operators. So
{array[index] := element} would do the same as {Array'Set(array,
index,element)}.

### Array'CreateFromList(list)

convert list to array

Creates an array from the contents of the list passed in.

### Array'ToList(array)

convert array to list

Creates a list from the contents of the array passed in.

### Association'Create()

Create an empty association.

### Association'Size(association)

Return the number of key-value pairs.

### Association'Contains(association,key)

Return whether {key} is present.

### Association'Get(association,key)

Return the associated value, or {Undefined} when the key is absent.

### Association'Set(association,key,value)

Insert or replace a key-value pair and return {True}.

### Association'Drop(association,key)

Remove a key. The result says whether the key existed.

### Association'Keys(association)

Return the keys in the engine's stable total order.

### Association'ToList(association)

Return ordered `{key,value}` pairs.

### Association'Head(association)

Return the first ordered `{key,value}` pair. An empty association is an
argument error.

`Association'CreateFromList(list)` is a standard-script convenience function
from `assoc.rep/code.ys`; it builds an association from `{key,value}` pairs.
