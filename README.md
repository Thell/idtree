# DND-Tree

A Rust implementation of the **ID‑Tree** dynamic connectivity data structure from:

**_Constant-time Connectivity Querying in Dynamic Graphs_**,  
Proceedings of the ACM on Management of Data, Volume 2, Issue 6  
Article No.: 230, Pages 1 - 23  
<https://dl.acm.org/doi/abs/10.1145/3698805>

The Improved D-Tree (ID-Tree) is an improvement on the D-Tree data structure from:

**_Dynamic Spanning Trees for Connectivity Queries on Fully-dynamic Undirected Graphs._**,
Proc. VLDB Endow. 15, 11 (2022), 3263–3276
<https://www.vldb.org/pvldb/vol15/p3263-chen.pdf>

*This is essentially the DNDTree data structure with the disjoint set tree removed.*

## Algorithmic Complexity

| Operation          | DND‑Tree    | D‑Tree                                |
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

# Variants

The full reference C++ implementation has buffered tree operations in-place
which the paper utilizes for temporal capabilities. The Rust implementations
do not have this capability. A part of the buffered operations includes dedup
of tree operations which the Rust implementations also do the remaining
overhead of the buffered operations is minor but measurable.

## C++
CPPDNDTree => Reference implementation accessed via ffi

## Rust
IDTree => Dedicated ID-Tree only build  
DNDTree- => DSU implemented as array back doubly linked list

# Benches

The expensive DSU maintenance operations are avoided by the ID-Tree but it pays
by having to traverse the spanning tree for each query.


## road-usroads-48.mtx

This is a medium sized (126k nodes) graph with average degree 2.
```
                         | CPPDNDTree |  IDTree    | DNDTree
Result Type              | Mean (ns)  | Mean (ns)  | Mean (ns)  
-----------------------------------------------------------------
--- INSERTION ---                                               
Non-Tree Edge            | 2590.14    | 2474.09    | 1699.68    
Tree Edge                | 480.23     | 251.95     | 234.50     
Non-Tree Reroot          | 745.96     | 392.92     | 391.72     
Tree Reroot              | 426.55     | 146.56     | 181.44     
-----------------------------------------------------------------
--- QUERY (COLD) ---                                            
Disconnected             | 120.87     | 2499.36    | 111.04     
Connected                | 75.03      | 4215.66    | 116.51     
-----------------------------------------------------------------
--- QUERY (WARM) ---                                            
Disconnected             | 36.25      | 1716.17    | 30.28      
Connected                | 33.66      | 3291.43    | 28.46      
-----------------------------------------------------------------
--- DELETION ---                                                
Non-Tree Edge            | 215.90     | 74.85      | 81.31      
Tree Edge (Split)        | 5209.38    | 3072.58    | 3120.77    
Tree Edge (Replaced)     | 846.16     | 244.82     | 390.12     
```

## bdo_exploration_graph.mtx

This is a small planar graph (~1k nodes) with average 2.6 degrees.
```
                         | CPPDNDTree | IDTree     | DNDTree
Result Type              | Mean (ns)  | Mean (ns)  | Mean (ns)  
----------------------------------------------------------------
--- INSERTION ---                                         
Non-Tree Edge            | 269.37     | 147.24     | 123.83      
Tree Edge                | 143.41     | 52.24      | 56.48       
Non-Tree Reroot          | 335.36     | 168.10     | 153.10      
Tree Reroot              | 158.55     | 53.06      | 64.88       
-----------------------------------------------------------------
--- QUERY (COLD) ---                                             
Disconnected             | 35.10      | 43.85      | 30.57       
Connected                | 36.26      | 49.94      | 31.21       
-----------------------------------------------------------------
--- QUERY (WARM) ---                                             
Disconnected             | 31.58      | 43.38      | 28.94       
Connected                | 31.65      | 49.28      | 29.01       
-----------------------------------------------------------------
--- DELETION ---                                                 
Non-Tree Edge            | 95.94      | 39.69      | 38.73       
Tree Edge (Split)        | 544.92     | 182.60     | 179.81      
Tree Edge (Replaced)     | 217.16     | 61.39      | 79.31       
```

# Features

## Core ID‑Tree Operations
- Dynamic insertion and deletion of undirected edges.
- Amortized‑efficient connectivity queries.  
  (For a truly constant query time see the DS-Tree variant from the same paper.)
- Balanced rerooting and centroid maintenance following the original algorithm.

## Graph Utilities
Additional helpers built on top of the ID‑Tree adjacency graph:
- Shortest‑path queries (BFS).
- Fundamental cycle‑basis extraction.
- Connected‑component enumeration.
- Active‑node tracking and filtering.
- Subset‑betweenness computations for specialized workloads.

## Optional Python Bindings
Enable the `python` feature to expose the API to Python via PyO3.

```toml
[features]
python = ["pyo3"]
```