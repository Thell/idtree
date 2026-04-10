// bridge.rs

/// Bridge between Rust and C++ for the reference implementation.
#[cfg(feature = "cpp")]
#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("dndtree_wrapper.h");

        /// Opaque handle to the C++ CPPDNDTree wrapper class.
        type CPPDNDTree;

        /// Constructs a new CPPDNDTree from a flattened adjacency list representation.
        ///
        /// # Arguments
        /// * `n` - The number of nodes in the graph.
        /// * `degrees` - A slice containing the degree of each node.
        /// * `flat_neighbors` - A flattened list of neighbors for all nodes.
        /// * `use_union_find` - Whether to enable DSU-based connectivity optimizations.
        fn new_cpp_dndtree_from_flat_adj(
            n: i32,
            degrees: &[i32],
            flat_neighbors: &[i32],
            use_union_find: bool,
        ) -> UniquePtr<CPPDNDTree>;

        /// Inserts an edge into the tree and updates the dynamic connectivity state.
        fn insert_edge(&self, u: i32, v: i32) -> i32;

        /// Deletes an edge from the tree and searches for a replacement edge.
        fn delete_edge(&self, u: i32, v: i32) -> i32;

        /// Returns true if nodes u and v are currently connected in the forest.
        fn query(&self, u: i32, v: i32) -> bool;

        /// Returns the DSU root for a given node. Returns -1 if DSU is disabled.
        fn get_dsu_root(&self, u: i32) -> i32;

        /// Returns the current parent of node u in the tree structure.
        fn get_tree_parent(&self, u: i32) -> i32;

        /// Returns the size of the subtree rooted at node u.
        fn get_subtree_size(&self, u: i32) -> i32;

        /// Toggles the global C++ trace output for debugging internal logic state.
        fn set_cpp_trace(enable: bool);
    }
}
