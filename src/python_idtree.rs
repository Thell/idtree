use pyo3::prelude::*;

use nohash_hasher::{IntMap, IntSet};

use crate::idtree::IDTree;

/// Python bindings for the ID‑Tree data structure.
#[pyclass(name = "IDTree", unsendable)]
pub struct PyIDTree {
    inner: IDTree,
}

#[pymethods]
impl PyIDTree {
    /// Construct an IDTree from a Python adjacency dictionary:
    ///
    ///     { 0: {1}, 1: {0,2}, 2: {1} }
    ///
    #[new]
    fn py_new(adj: std::collections::HashMap<usize, Vec<usize>>) -> PyResult<Self> {
        let adj_map: IntMap<usize, IntSet<usize>> = adj
            .into_iter()
            .map(|(k, v)| (k, IntSet::from_iter(v)))
            .collect();

        Ok(Self {
            inner: IDTree::new(&adj_map),
        })
    }

    /// Clone the IDTree.
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Insert an undirected edge (u, v).
    ///
    /// Returns 0 on success, -1 on failure (e.g., out of bounds).
    #[pyo3(name = "insert_edge")]
    fn py_insert_edge(&mut self, u: usize, v: usize) -> PyResult<i32> {
        Ok(self.inner.insert_edge(u, v))
    }

    /// Delete an undirected edge (u, v).
    ///
    /// Returns:
    /// - -1 if the edge is invalid or out of bounds.
    /// - 0 if the edge was removed from the adjacency graph but did not affect the ID-Tree structure.
    /// - 1 if a replacement edge was found to maintain connectivity.
    /// - 2 if no replacement edge was found and the component was split.    #[pyo3(name = "delete_edge")]
    fn py_delete_edge(&mut self, u: usize, v: usize) -> PyResult<i32> {
        Ok(self.inner.delete_edge(u, v))
    }

    /// Connectivity query: returns True if u and v are connected.
    #[pyo3(name = "query")]
    fn py_query(&self, u: usize, v: usize) -> PyResult<bool> {
        Ok(self.inner.query(u, v))
    }

    /// Return the fundamental cycle basis for the component containing `root`.
    #[pyo3(name = "cycle_basis")]
    fn py_cycle_basis(&mut self, root: Option<usize>) -> PyResult<Vec<Vec<usize>>> {
        Ok(self.inner.cycle_basis(root))
    }

    /// Return the connected component containing node v.
    #[pyo3(name = "node_connected_component")]
    fn py_node_connected_component(&mut self, v: usize) -> PyResult<Vec<usize>> {
        Ok(self.inner.node_connected_component(v))
    }

    /// Return the number of connected components.
    #[pyo3(name = "num_connected_components")]
    fn py_num_connected_components(&mut self) -> PyResult<usize> {
        Ok(self.inner.num_connected_components())
    }

    /// Return all connected components.
    #[pyo3(name = "connected_components")]
    fn py_connected_components(&mut self) -> PyResult<Vec<Vec<usize>>> {
        Ok(self.inner.connected_components())
    }

    /// Return all active (non‑isolated) nodes.
    #[pyo3(name = "active_nodes")]
    fn py_active_nodes(&mut self) -> PyResult<Vec<usize>> {
        Ok(self.inner.active_nodes_vec())
    }

    /// Isolate a single node by removing all incident edges.
    #[pyo3(name = "isolate_node")]
    fn py_isolate_node(&mut self, v: usize) -> PyResult<()> {
        self.inner.isolate_node(v);
        Ok(())
    }

    /// Isolate a list of nodes.
    #[pyo3(name = "isolate_nodes")]
    fn py_isolate_nodes(&mut self, nodes: Vec<usize>) -> PyResult<()> {
        self.inner.isolate_nodes(nodes);
        Ok(())
    }

    /// Return True if the node has no neighbors.
    #[pyo3(name = "is_isolated")]
    fn py_is_isolated(&mut self, v: usize) -> PyResult<bool> {
        Ok(self.inner.is_isolated(v))
    }

    /// Return the degree of node v.
    #[pyo3(name = "degree")]
    fn py_degree(&mut self, v: usize) -> PyResult<i32> {
        Ok(self.inner.degree(v))
    }

    /// Return the neighbors of node v.
    #[pyo3(name = "neighbors")]
    fn py_neighbors(&mut self, v: usize) -> PyResult<Vec<usize>> {
        Ok(self.inner.neighbors(v))
    }

    /// Filter a list of nodes, keeping only those that are active.
    #[pyo3(name = "retain_active_nodes_from")]
    fn py_retain_active_nodes_from(&mut self, from: Vec<usize>) -> PyResult<Vec<usize>> {
        Ok(self.inner.retain_active_nodes_from(from))
    }

    /// Shortest path between two nodes (BFS on adjacency graph).
    ///
    /// Returns a list of nodes or None.
    #[pyo3(name = "shortest_path")]
    fn py_shortest_path(&mut self, start: usize, target: usize) -> PyResult<Option<Vec<usize>>> {
        Ok(self.inner.shortest_path(start, target))
    }
}

#[pymodule]
pub fn python_idtree(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyIDTree>()?;
    Ok(())
}
