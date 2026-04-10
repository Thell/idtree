# API Benching

## Creating a new Tree
  -  using an adjacency list
  -  inserting edge by edge into an empty tree

## Insert edges
```
Returns:
    -1 if the edge is invalid
    0 if the edge inserted was a non-tree edge
    1 if the edge inserted was a tree edge
    2 if the edge inserted was a non-tree edge triggering a reroot
    3 if the edge inserted was a tree edge triggering a reroot
```

## Delete edges
```
Returns:
  -1 if the edge is invalid
  0 if the edge deleted was a non-tree edge
  1 if the edge deleted was a tree edge (a new component is created)
  2 if the edge deleted was a tree edge and a replacement edge was found
```

## Query (cold and warm)

- ID-Tree always traverses parents to find the roots
- DND-Tree traverses roots when cold

```
Returns:
  True if the edge ends are connected
  False if the edge ends are not connected
```