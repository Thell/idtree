# id-tree

A Rust implementation of the **ID‑Tree** dynamic connectivity data structure from:

**_Constant-time Connectivity Querying in Dynamic Graphs_**,  
Proceedings of the ACM on Management of Data, Volume 2, Issue 6  
Article No.: 230, Pages 1 - 23  
<https://dl.acm.org/doi/abs/10.1145/3698805>

The Improved D-Tree (ID-Tree) is an improvement on the D-Tree data structure from:

**_Dynamic Spanning Trees for Connectivity Queries on Fully-dynamic Undirected Graphs._**,
Proc. VLDB Endow. 15, 11 (2022), 3263–3276
<https://www.vldb.org/pvldb/vol15/p3263-chen.pdf>

The implementation is fully safe Rust.

## Algorithmic Complexity

| Operation          | ID‑Tree     | D‑Tree                                |
|--------------------|-------------|---------------------------------------|
| Query processing   | $O(\alpha)$ | $O(h)$                                |
| Edge insertion     | $O(h)$      | $O(h \cdot \text{nbr}_\text{update})$ |
| Edge deletion      | $O(h)$      | $O(h^2 \cdot \text{nbr}_\text{scan})$ |

Where:

- $\alpha$ is the inverse Ackermann function, a small constant ($\alpha$ < 5)
- $h$ is the average vertex depth in the spanning tree.
- $\text{nbr}_\text{update}$ is the time to insert a vertex into neighbors of a vertex or to
 delete a vertex from neighbors of a vertex.
- $\text{nbr}_\text{scan}$ is the time to scan all neighbors of a vertex.

## Performance Characteristics

```
Timer precision: 100 ns
bench            fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ bench_build_from_adj¹       │               │               │               │         │
│  ├─ 10000      388.6 µs      │ 866.1 µs      │ 431.4 µs      │ 465.9 µs      │ 100     │ 100
│  ├─ 100000     4.962 ms      │ 9.332 ms      │ 5.439 ms      │ 5.711 ms      │ 100     │ 100
│  ╰─ 500000     38.89 ms      │ 47.92 ms      │ 42.48 ms      │ 42.49 ms      │ 100     │ 100
├─ bench_delete                │               │               │               │         │
│  ├─ 10000      563.3 µs      │ 4.344 ms      │ 678.8 µs      │ 874.4 µs      │ 100     │ 100
│  ├─ 100000     21.35 ms      │ 61.54 ms      │ 24.03 ms      │ 26.84 ms      │ 100     │ 100
│  ╰─ 500000     509.7 ms      │ 680.9 ms      │ 540.6 ms      │ 548.3 ms      │ 100     │ 100
├─ bench_insert                │               │               │               │         │
│  ├─ 10000      367.2 µs      │ 703.9 µs      │ 373.4 µs      │ 391.7 µs      │ 100     │ 100
│  ├─ 100000     10.75 ms      │ 22.04 ms      │ 11.07 ms      │ 12.07 ms      │ 100     │ 100
│  ╰─ 500000     214 ms        │ 332.5 ms      │ 234.9 ms      │ 245.4 ms      │ 100     │ 100
╰─ bench_query                 │               │               │               │         │
   ├─ 10000      1.403 ms      │ 2.333 ms      │ 1.43 ms       │ 1.487 ms      │ 100     │ 100
   ├─ 100000     354.7 ms      │ 780.2 ms      │ 369.8 ms      │ 392.2 ms      │ 100     │ 100
   ╰─ 500000     32.15 s       │ 36.61 s       │ 33.2 s        │ 34.28 s       │ 100     │ 100
```
¹ Creates the same graph as 'bench_insert' but uses a pre-populated adj map.

The ID-Tree, which differs from the DS-Tree in that it does not have a disjoint
set to update on insert or delete, does much less work on insert than it does on
delete, since the insert only needs to check for re-balancing of the spanning
tree. Delete needs to check for a replacement edge. The query needs to walk the
tree to a common parent, which doesn't need to be done in the DS-Tree variant,
for each query since it doesn't have the constant lookup of the disjoint set.

_The primary use case for the ID-Tree compared to the DS-Tree (or a D-Tree) is
when many insert/delete actions are done per query._

## Features

### Core ID‑Tree Operations
- Dynamic insertion and deletion of undirected edges.
- Amortized‑efficient connectivity queries.  
  (For a truly constant query time see the DS-Tree variant from the same paper.)
- Balanced rerooting and centroid maintenance following the original algorithm.

### Graph Utilities
Additional helpers built on top of the ID‑Tree adjacency graph:
- Shortest‑path queries (BFS).
- Fundamental cycle‑basis extraction.
- Connected‑component enumeration.
- Active‑node tracking and filtering.
- Subset‑betweenness computations for specialized workloads.

### Optional Python Bindings
Enable the `python` feature to expose the API to Python via PyO3.

```toml
[features]
python = ["pyo3"]
