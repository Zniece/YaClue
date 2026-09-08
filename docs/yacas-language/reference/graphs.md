# Graphs

> **Scope:** This page retains detailed function documentation. Inclusion in the manual does not imply validation against the current Rust implementation. See the [availability audit](availability.md) for the boundaries between core commands, standard scripts, and historical entries.


### Graph(edges)

            Graph(vertices, edges)

construct a graph

> **See also:** `->`, `<->`


### infix -> (vertex1, vertex2)

> **Current status:** A package-local operator declared after the graph package is loaded. It is not part of the startup core syntax. Calling a public entry such as `Graph` loads the corresponding script package.

            infix <-> (vertex1, vertex2)

construct an edge

### Vertices(g)

return list of graph vertices

> **See also:** [Edges](graphs.md#edgesg), [Graph](graphs.md#graphedges)


### Edges(g)

return list of graph edges

> **See also:** [Vertices](graphs.md#verticesg), [Graph](graphs.md#graphedges)


### AdjacencyList(g)

adjacency list

**param g:**graph

Return [adjacency list](https://en.wikipedia.org/wiki/Adjacency_list)
of graph `g`.

> **See also:** [AdjacencyMatrix](graphs.md#adjacencymatrixg), [Graph](graphs.md#graphedges)


### AdjacencyMatrix(g)

adjacency matrix

**param g:**graph

Return [adjacency matrix](https://en.wikipedia.org/wiki/Adjacency_matrix)
of graph `g`.

> **See also:** [AdjacencyList](graphs.md#adjacencylistg), [Graph](graphs.md#graphedges)


### BFS(g, f)

            BFS(g, v, f)

traverse graph in breadth-first order


Traverse graph `g` in [breadth-first](https://en.wikipedia.org/wiki/Breadth-first_search) order, starting from
`v` if provided, or from the first vertex. `f` is called for every
visited vertex.

> **See also:** [DFS](graphs.md#dfsg-f), [Graph](graphs.md#graphedges)


### DFS(g, f)

            DFS(g, v, f)

traverse graph in depth-first order


Traverse graph `g` in [depth-first](https://en.wikipedia.org/wiki/Depth-first_search) order, starting from
`v` if provided, or from the first vertex. `f` is called for every
visited vertex.

> **See also:** [BFS](graphs.md#bfsg-f), [Graph](graphs.md#graphedges)

