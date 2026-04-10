use std::{collections::VecDeque, vec};

use fixedbitset::FixedBitSet;
use nohash_hasher::{BuildNoHashHasher, IntMap, IntSet};
use rapidhash::RapidHashSet;
use smallvec::SmallVec;

const MAX_DEPTH: usize = 32767;
const SENTINEL: usize = usize::MAX;

// MARK: Node

#[derive(Clone, Debug)]
pub struct Node {
    /// The parent of this node in the id-tree
    pub parent: usize,

    /// Subtree cardinality in normal operation. During rotations this field is
    /// temporarily used to store signed size deltas (child_size - parent_size)
    /// as part of the O(height) subtree-size transfer algorithm. The value is
    /// guaranteed to be >= 1 except while a rotation is actively in progress.
    pub subtree_size: i32,

    /// The adjacent neighbors of this node
    pub neighbors: SmallVec<[u32; 8]>,
}

impl Node {
    fn new() -> Self {
        Node {
            parent: SENTINEL,
            subtree_size: 1,
            neighbors: SmallVec::new(),
        }
    }

    fn insert_neighbor(&mut self, u: u32) -> i32 {
        if !self.neighbors.contains(&u) {
            self.neighbors.push(u);

            // Sorting is for use during the development cycle for divergence testing of op logic
            #[cfg(feature = "cpp")]
            self.neighbors.sort();

            return 0;
        }
        1
    }

    fn delete_neighbor(&mut self, u: u32) -> i32 {
        if let Some(i) = self.neighbors.iter().position(|&x| x == u) {
            self.neighbors.swap_remove(i);

            // Sorting is for use during the development cycle for divergence testing of op logic
            #[cfg(feature = "cpp")]
            self.neighbors.sort();

            return 0;
        }
        1
    }
}

/// MARK: DNDTree
//
// NOTE: After setup completes all node, neighbor and link entries are
//       guaranteed to be within range 0..self.n
// SAFETY: No function should be added to the struct that allows direct modification
//         of any of these fields and all public functions must check the invariants.
//         ( 0 <= u < self.n, 0 <= v < self.n, 0 <= u < self.n, 0 <= v < self.n )
#[derive(Clone, Debug)]
pub struct IDTree {
    n: usize,

    nodes: Vec<Node>,
    generation: u16,
    generations: Vec<u16>,
    vec_scratch_nodes: Vec<usize>,

    distances: Vec<i32>,               // (used for betweenness)
    deque_scratch: VecDeque<usize>,    // scratch area (used by shortest path)
    vec_scratch_stack: Vec<usize>,     // scratch area
    node_bitset_scratch0: FixedBitSet, // |nodes| len scratch area
    node_bitset_scratch1: FixedBitSet, // |nodes| len scratch area
    node_bitset_scratch2: FixedBitSet, // |nodes| len scratch area
}

impl IDTree {
    /// Create a new IDTree with n isolated nodes.
    ///
    /// NOTE: new nodes are never added to the tree post setup.
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "must have at least one node");
        let nodes = vec![Node::new(); n];
        Self::_new(nodes)
    }

    /// Create a new DNDTree with the given adjacency list where there are adj_dict.len() nodes
    /// and all keys are in range 0..adj_dict.len().
    ///
    /// NOTE: new nodes are never added to the tree post setup.
    pub fn from_adj(adj_dict: &IntMap<usize, IntSet<usize>>) -> Self {
        let n = adj_dict.len();
        assert!(n > 0, "adjacency map must have at least one entry");
        let nodes = Self::nodes_from_map(n, adj_dict);
        let mut instance = Self::_new(nodes);
        instance.initialize();
        instance
    }

    /// Create a new DNDTree with the given edges where there are n nodes and all edge
    /// endpoints are in range 0..n
    ///
    /// NOTE: new nodes are never added to the tree post setup.
    pub fn from_edges(n: usize, edges: &[(usize, usize)]) -> Self {
        assert!(n > 0, "must have at least one node");
        let nodes = Self::nodes_from_edges(n, edges);
        let mut instance = Self::_new(nodes);
        instance.initialize();
        instance
    }

    /// Insert an undirected edge
    ///
    /// Returns:
    ///   -1 if the edge is invalid
    ///   0 if the edge inserted was a non-tree edge
    ///   1 if the edge inserted was a tree edge
    ///   2 if the edge inserted was a non-tree edge triggering a reroot
    ///   3 if the edge inserted was a tree edge triggering a reroot
    pub fn insert_edge(&mut self, u: usize, v: usize) -> i32 {
        if u >= self.n || v >= self.n || u == v || !self.insert_edge_in_graph(u, v) {
            return -1;
        }
        self.insert_edge_balanced(u, v)
    }

    /// Delete an undirected edge
    ///
    /// Returns:
    ///   -1 if the edge is invalid
    ///   0 if the edge deleted was a non-tree edge
    ///   1 if the edge deleted was a tree edge
    ///   2 if the edge deleted was a tree edge and a replacement edge was found
    pub fn delete_edge(&mut self, u: usize, v: usize) -> i32 {
        if u >= self.n || v >= self.n || u == v || !self.delete_edge_in_graph(u, v) {
            return -1;
        }
        self.delete_edge_balanced(u, v)
    }

    /// Query if u and v are in the same connected component
    //
    // NOTE: mut is required for DSU path and link compression
    pub fn query(&self, u: usize, v: usize) -> bool {
        if u >= self.n || v >= self.n {
            return false;
        }
        self.get_tree_root(u) == self.get_tree_root(v)
    }

    /// Reset the graph to contain zero edges (n isolated nodes).
    ///
    /// NOTE: The number of nodes is left unchanged.
    pub fn reset_all_edges(&mut self) {
        for node in &mut self.nodes {
            node.neighbors.clear();
            node.parent = SENTINEL;
            node.subtree_size = 1;
        }
    }

    /// Reset the graph to contain edges given in edge_list.
    ///
    /// NOTE: The number of nodes is left unchanged.
    ///
    /// NOTE: Assumes all endpoints are in range 0..self.n
    pub fn reset_all_edges_to_edges(&mut self, edges: &[(usize, usize)]) {
        self.reset_all_edges();
        edges.iter().for_each(|(u, v)| {
            self.insert_edge_in_graph(*u, *v);
        });
        self.initialize();
    }

    /// Reset the graph to contain edges given in adj_dict.
    ///
    /// NOTE: The number of nodes is left unchanged.
    ///
    /// NOTE: Assumes adj_dict contains the full undirected graph with all nodes represented
    ///       as keys and all endpoint indices are in range 0..self.n.
    pub fn reset_all_edges_to_adj(&mut self, adj_dict: &IntMap<usize, IntSet<usize>>) {
        let n = adj_dict.len();
        assert_eq!(n, self.n, "adjacency size must match existing tree size");
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.parent = SENTINEL;
            node.subtree_size = 1;

            let neighbors = adj_dict.get(&i).unwrap();
            node.neighbors.clear();
            node.neighbors.extend(neighbors.iter().map(|&j| j as u32));
        }
        self.initialize();
    }

    /// For tests
    pub fn get_node_data(&self, u: usize) -> Node {
        self.nodes[u].clone()
    }

    /// For tests
    pub fn get_parent(&self, u: usize) -> usize {
        self.nodes[u].parent
    }
}

impl IDTree {
    // NOTE: After setup completes all node, neighbor and lnode entries are
    //       guaranteed to be within range 0..self.n
    // SAFETY: No function should be added to the struct that allows direct modification
    //         of any of these fields
    fn _new(nodes: Vec<Node>) -> Self {
        let n = nodes.len();
        Self {
            n,
            nodes,
            generation: 1,
            generations: vec![0; n],
            vec_scratch_nodes: Vec::with_capacity(n),

            distances: vec![0; n],
            deque_scratch: VecDeque::with_capacity(n),
            vec_scratch_stack: vec![],
            node_bitset_scratch0: FixedBitSet::with_capacity(n),
            node_bitset_scratch1: FixedBitSet::with_capacity(n),
            node_bitset_scratch2: FixedBitSet::with_capacity(n),
        }
    }

    fn nodes_from_map(n: usize, adj_dict: &IntMap<usize, IntSet<usize>>) -> Vec<Node> {
        (0..n)
            .map(|i| {
                let mut node = Node::new();
                for &j in adj_dict.get(&i).unwrap_or(&IntSet::default()) {
                    assert!(j != i, "invalid self loop");
                    assert!(j < n, "invalid neighbor {} of {}", j, i);
                    node.insert_neighbor(j as u32);
                }
                node
            })
            .collect()
    }

    fn nodes_from_edges(n: usize, edges: &[(usize, usize)]) -> Vec<Node> {
        let mut nodes = vec![Node::new(); n];
        for &(j, k) in edges {
            assert!(j != k, "invalid self loop");
            assert!(j < n, "invalid endpoint {}", j,);
            assert!(k < n, "invalid endpoint {}", k,);
            nodes[j].insert_neighbor(k as u32);
            nodes[k].insert_neighbor(j as u32);
        }
        nodes
    }

    fn initialize(&mut self) {
        assert!(self.nodes.len() == self.generations.len());

        let cur_generation = self.next_generation();

        for node in 0..self.nodes.len() {
            // Skip isolated nodes
            if self.nodes[node].neighbors.is_empty() {
                continue;
            }
            if self.generations[node] == cur_generation {
                continue;
            }

            // NOTE: each subtree is setup in the scratch collection which is reused
            //       to find the centroid
            self.bfs_setup_subtrees(node);
            if let Some(centroid) = self.find_centroid_in_q() {
                self.reroot(centroid, node);
            }
        }
        self.vec_scratch_nodes.clear();
    }

    fn bfs_setup_subtrees(&mut self, root: usize) {
        let deque = &mut self.deque_scratch;
        deque.clear();
        deque.push_back(root);

        self.vec_scratch_nodes.clear();
        self.vec_scratch_nodes.push(root);

        let cur_generation = self.generation;
        self.generations[root] = cur_generation;

        while let Some(p) = deque.pop_front() {
            for j in 0..self.nodes[p].neighbors.len() {
                let neighbor = self.nodes[p].neighbors[j] as usize;

                if self.generations[neighbor] != cur_generation {
                    self.generations[neighbor] = cur_generation;

                    self.nodes[neighbor].parent = p;
                    self.vec_scratch_nodes.push(neighbor);
                    deque.push_back(neighbor);
                }
            }
        }

        for &q in self.vec_scratch_nodes.iter().skip(1).rev() {
            let p = self.nodes[q].parent;
            self.nodes[p].subtree_size += self.nodes[q].subtree_size;
        }
    }

    // NOTE: Uses pre-populated self.vec_scratch_nodes from bfs_setup_subtrees.
    fn find_centroid_in_q(&self) -> Option<usize> {
        let num_nodes = self.vec_scratch_nodes.len();
        let half_num_nodes = (num_nodes / 2) as i32;

        self.vec_scratch_nodes.iter().rev().find_map(|&i| {
            if self.nodes[i].subtree_size > half_num_nodes {
                Some(i)
            } else {
                None
            }
        })
    }
}

impl IDTree {
    // MARK: Accessors
    // SAFETY: Unchecked access is safe because all public functions check invariants
    //         and after setup completes all entries are within range 0..self.n with
    //         proper invariants and all node accesses are within range 0..self.n.
    // NOTE: Sentinel value of usize::MAX is reserved for NULL for parent usage only
    // TODO: Switch to NonMax type once stable https://github.com/rust-lang/rust/issues/151435
    fn node(&self, i: usize) -> &Node {
        debug_assert!(i < self.n);
        unsafe { self.nodes.get_unchecked(i) }
    }

    fn next_generation(&mut self) -> u16 {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
            self.generations.fill(0);
        }
        self.generation
    }
}

// MARK: Base functions

impl IDTree {
    fn delete_edge_in_graph(&mut self, u: usize, v: usize) -> bool {
        self.nodes[u].delete_neighbor(v as u32) == 0 && self.nodes[v].delete_neighbor(u as u32) == 0
    }

    fn delete_edge_balanced(&mut self, mut u: usize, mut v: usize) -> i32 {
        if (self.nodes[u].parent != v && self.nodes[v].parent != u) || u == v {
            return 0;
        }

        if self.nodes[v].parent == u {
            std::mem::swap(&mut u, &mut v);
        }

        let (p, subtree_u_size) = self.unlink(u, v);
        let (small_node, large_node): (usize, usize) =
            if self.nodes[p].subtree_size < subtree_u_size {
                (p, u)
            } else {
                (u, p)
            };

        // NOTE: Populates self.vec_scratch_nodes for potential re-use by remove_subtree_union_find
        if self.find_replacement(small_node, large_node) {
            return 1;
        }
        2
    }

    fn insert_edge_in_graph(&mut self, u: usize, v: usize) -> bool {
        self.nodes[u].insert_neighbor(v as u32) == 0 && self.nodes[v].insert_neighbor(u as u32) == 0
    }

    fn insert_edge_balanced(&mut self, u: usize, v: usize) -> i32 {
        let (fu, fv) = (self.get_tree_root(u), self.get_tree_root(v));
        if fu == fv {
            self.insert_non_tree_edge_balanced(u, v, fu)
        } else {
            self.insert_tree_edge_balanced(u, v, fu, fv)
        }
    }

    /// Handles insertion of a non‑tree edge (u, v) when both endpoints are in the
    /// same component. This performs the depth‑imbalance check, identifies the
    /// centroid of the deeper side, detaches and reroots the smaller subtree, and
    /// rebalances the component around the centroid if required.
    ///
    /// Arguments:
    /// - `u`, `v`: original edge endpoints
    /// - `f`: the component root (tree‑root or DSU‑root), used to compute the
    ///   target half‑subtree size during rebalancing
    fn insert_non_tree_edge_balanced(&mut self, u: usize, v: usize, f: usize) -> i32 {
        let (reshape, small_node, large_node, small_p, _large_p) =
            self.detect_depth_imbalance(u, v);

        if !reshape {
            return 0;
        }

        // Node at which the subtree should be detached and rerooted.
        let p = self.find_imbalance_centroid(small_node, small_p);

        // Remove the subtree rooted at the detach point from its ancestors.
        self.adjust_subtree_sizes(p, -self.nodes[p].subtree_size);

        // Reroot the smaller subtree under the larger side.
        self.nodes[p].parent = SENTINEL;
        self.reroot(small_node, SENTINEL);
        self.nodes[small_node].parent = large_node;

        // Recompute subtree sizes upward from the attach point and detect the new root centroid.
        let new_root = self.rebalance_tree(small_node, large_node, f);

        if let Some(new_root) = new_root
            && new_root != f
        {
            self.reroot(new_root, f);
            return 2;
        }

        0
    }

    /// Handles insertion of a tree edge (u, v) connecting two different components.
    /// Ensures the smaller component attaches under the larger one, rotating the
    /// tree so that `u` becomes the root of its component, fixes subtree sizes
    /// along the reversed path, and rebalances the merged tree.
    ///
    /// Arguments:
    /// - `u`, `v`: edge endpoints
    /// - `fu`: root of u’s component
    /// - `fv`: root of v’s component
    fn insert_tree_edge_balanced(
        &mut self,
        mut u: usize,
        mut v: usize,
        mut fu: usize,
        mut fv: usize,
    ) -> i32 {
        // Ensure fu is the root of the smaller component.
        if self.nodes[fu].subtree_size > self.nodes[fv].subtree_size {
            std::mem::swap(&mut u, &mut v);
            std::mem::swap(&mut fu, &mut fv);
        }

        let u = self.rotate_tree(u, v);

        // Attach smaller component under larger.
        let new_root = self.rebalance_tree(fu, v, fv);

        self.fix_rotated_subtree_sizes(u, v);

        if let Some(new_root) = new_root
            && new_root != fv
        {
            self.reroot(new_root, fv);
            return 3;
        }

        1
    }

    fn get_tree_root(&self, u: usize) -> usize {
        let mut root = u;
        while self.node(root).parent != SENTINEL {
            root = self.nodes[root].parent;
        }
        root
    }
}

// MARK: Support functions

impl IDTree {
    /// Determines whether the paths from u and v to the root differ enough to
    /// require a reshape. Walks both parent chains upward in lockstep until one
    /// reaches the root. If the other still has depth remaining, a reshape is
    /// required.
    ///
    /// Returns:
    /// - `reshape`: whether a rebalance is needed
    /// - `small_node`: the side that reached the root first (after swap)
    /// - `large_node`: the deeper side (after swap)
    /// - `small_p`: parent pointer at the divergence point for the shallow side
    /// - `large_p`: parent pointer at the divergence point for the deep side
    fn detect_depth_imbalance(
        &self,
        mut u: usize,
        mut v: usize,
    ) -> (bool, usize, usize, usize, usize) {
        let mut reshape = false;
        let mut depth = 0;

        let mut pu = self.nodes[u].parent;
        let mut pv = self.nodes[v].parent;

        while depth < MAX_DEPTH {
            if pu == SENTINEL {
                if pv != SENTINEL && self.nodes[pv].parent != SENTINEL {
                    reshape = true;
                    std::mem::swap(&mut u, &mut v);
                    std::mem::swap(&mut pu, &mut pv);
                }
                break;
            } else if pv == SENTINEL {
                if pu != SENTINEL && self.nodes[pu].parent != SENTINEL {
                    reshape = true;
                }
                break;
            }

            pu = self.nodes[pu].parent;
            pv = self.nodes[pv].parent;
            depth += 1;
        }

        (reshape, u, v, pu, pv)
    }

    /// Given the shallow side (`small_node`) and the parent pointer at the
    /// divergence point (`small_p`), computes the centroid of the deeper side.
    /// This is done by measuring the remaining depth to the root and walking
    /// halfway up.
    ///
    /// Arguments:
    /// - `small_node`: the node on the shallow side
    /// - `small_p`: parent pointer where the shallow side stopped
    ///
    /// Returns:
    /// - the centroid node index
    fn find_imbalance_centroid(&self, small_node: usize, small_p: usize) -> usize {
        let mut depth_imbalance = 0;
        let mut p = small_p;

        while p != SENTINEL {
            depth_imbalance += 1;
            p = self.nodes[p].parent;
        }

        depth_imbalance = depth_imbalance / 2 - 1;

        let mut cur = small_node;
        while depth_imbalance > 0 {
            cur = self.nodes[cur].parent;
            depth_imbalance -= 1;
        }

        cur
    }

    /// Applies a constant subtree‑size adjustment to all ancestors of `start_node`.
    /// Used both for subtracting the detached subtree and for adding the attached
    /// subtree during rebalancing.
    ///
    /// Arguments:
    /// - `start_node`: the node whose subtree size is being propagated upward
    /// - `delta`: signed adjustment applied to each ancestor’s subtree_size
    ///
    /// Returns:
    /// - the last node whose subtree size was adjusted (the root)
    fn adjust_subtree_sizes(&mut self, start_node: usize, delta: i32) -> usize {
        let mut root_v = start_node;
        let mut w = self.nodes[start_node].parent;
        while w != SENTINEL {
            self.nodes[w].subtree_size += delta;
            root_v = w;
            w = self.nodes[w].parent;
        }

        root_v
    }

    /// After attaching subtree `u` under node `v`, this propagates the subtree size
    /// of `u` upward through the ancestors of `v` and identifies the centroid of
    /// the merged component.
    ///
    /// Arguments:
    /// - `u`: root of the newly attached subtree
    /// - `v`: attach point in the larger component
    /// - `f`: root of the larger component (used to compute the half‑size threshold)
    ///
    /// Returns:
    /// - `Some(new_root)` if a centroid different from `f` is found
    /// - `None` if no rebalance is needed
    fn rebalance_tree(&mut self, u: usize, v: usize, f: usize) -> Option<usize> {
        let s = (self.nodes[f].subtree_size + self.nodes[u].subtree_size) / 2;

        let mut new_root = None;
        let mut p = v;

        while p != SENTINEL {
            self.nodes[p].subtree_size += self.nodes[u].subtree_size;
            if new_root.is_none() && self.nodes[p].subtree_size > s {
                new_root = Some(p);
            }
            p = self.nodes[p].parent;
        }

        new_root
    }

    /// Searches for a non‑tree edge that still connects the two components
    /// created by deleting a tree edge. If such an edge exists, the function
    /// rebuilds the ID‑Tree structure around that edge so that the component
    /// remains a single balanced tree.
    ///
    /// From the IDTree and DSU perspective, the component is already either
    /// connected or disconnected; this function does not determine that fact.
    /// It only determines whether a valid replacement edge exists and, if so,
    /// performs the structural rotations and rebalancing needed to make that
    /// edge the new tree connection.
    ///
    /// Returns `true` if a replacement edge was found and the tree structure
    /// was rebuilt around it; otherwise returns `false`, leaving the two
    /// components permanently separated.
    ///
    /// Arguments:
    /// - `u`: the root of the detached subtree
    /// - `root_v`: the root of the other component
    ///
    /// Returns:
    /// - `true` if a replacement edge was found and the tree structure was rebuilt
    fn find_replacement(&mut self, u: usize, root_v: usize) -> bool {
        self.vec_scratch_nodes.clear();
        let cur_generation = self.next_generation();

        self.vec_scratch_nodes.push(u);
        self.generations[u] = cur_generation;

        // NOTE: Do not use a deque here for the queue since popping from the front removes elements
        //       and when use_union_find is true the scratch vec is used as the subtree to
        //       to remove from the DSU via the remove subtree processing.
        let mut i = 0;
        while i < self.vec_scratch_nodes.len() {
            let node = self.vec_scratch_nodes[i];
            i += 1;

            'neighbors: for n_idx in 0..self.nodes[node].neighbors.len() {
                let neighbor = self.nodes[node].neighbors[n_idx] as usize;
                if neighbor == self.nodes[node].parent {
                    continue;
                }

                // NOTE: It is tempting to short-circuit this loop with
                //         `&& self.generations[neighbor] != cur_generation`
                //       but that can cause improper subtree setup in the scratch collection
                //       (See the with_dsu::test_mixed_ops_query_heavy test case.)
                //       For a non-DSU dedicated build for a specific graph this may be worth the
                //       performance optimization but requires careful analysis.
                if self.nodes[neighbor].parent == node {
                    self.vec_scratch_nodes.push(neighbor);
                    self.generations[neighbor] = cur_generation;
                    continue;
                }

                let mut w = neighbor;
                while w != SENTINEL {
                    if self.generations[w] == cur_generation {
                        continue 'neighbors;
                    }
                    w = self.nodes[w].parent;
                }

                let rotated_u = self.rotate_tree(node, neighbor);
                let new_root = self.rebalance_tree(rotated_u, neighbor, root_v);
                self.fix_rotated_subtree_sizes(rotated_u, neighbor);

                if let Some(new_root) = new_root
                    && new_root != root_v
                {
                    self.reroot(new_root, root_v);
                }
                return true;
            }
        }
        false
    }

    /// Reroots the tree by moving the subtree of `u` to `f`.
    fn reroot(&mut self, u: usize, _f: usize) {
        let old_root = self.rotate_tree_to_root(u);
        self.fix_rotated_subtree_sizes_until_root(old_root);
    }

    /// Rotates the parent pointers along the branch from `start_node` upward so that
    /// `start_node` becomes the root of that branch, then attaches the branch under
    /// `stop_node`.
    ///
    /// Arguments:
    /// - `start_node`: node whose branch is being rotated
    /// - `stop_node`: attach point in the other component
    fn rotate_tree(&mut self, start_node: usize, stop_node: usize) -> usize {
        self._rotate_tree(start_node, stop_node)
    }

    /// Rotates the parent pointers along the branch from `start_node` to the root,
    /// so that `start_node` becomes the root of its component.
    ///
    /// Arguments:
    /// - `start_node`: node whose component is being rerooted
    fn rotate_tree_to_root(&mut self, start_node: usize) -> usize {
        self._rotate_tree(start_node, SENTINEL)
    }

    /// Rotates the parent pointers along the branch from `start_node` upward so that
    /// `start_node` becomes the root of that branch, then attaches the branch under
    /// `new_parent`.
    ///
    /// Arguments:
    /// - `start_node`: node whose branch is being rotated
    /// - `new_parent`: the parent value to attach the rotated branch under
    fn _rotate_tree(&mut self, mut u: usize, new_parent: usize) -> usize {
        let mut p = self.nodes[u].parent;
        self.nodes[u].parent = new_parent;

        while p != SENTINEL {
            let next = self.nodes[p].parent;
            self.nodes[p].parent = u;
            u = p;
            p = next;
        }

        u // old root
    }

    /// After a rotation updates the parent chain of a component, this restores
    /// correct subtree sizes along the affected branch until reaching `stop_node`.
    ///
    /// Arguments:
    /// - `start_node`: the node where the updated branch begins
    /// - `stop_node`: the node at which to stop adjusting (the attach point)
    fn fix_rotated_subtree_sizes(&mut self, start_node: usize, stop_node: usize) {
        self._fix_rotated_subtree_sizes(start_node, stop_node);
    }

    /// After a rotation updates the parent chain of a component, this restores
    /// correct subtree sizes along the affected branch until reaching the root.
    ///
    /// Arguments:
    /// - `start_node`: the node where the updated branch begins
    fn fix_rotated_subtree_sizes_until_root(&mut self, start_node: usize) {
        self._fix_rotated_subtree_sizes(start_node, SENTINEL);
    }

    /// After a rotation updates the parent chain of a component, this restores
    /// correct subtree sizes along the affected branch until reaching `stop_parent`.
    ///
    /// Arguments:
    /// - `start_node`: the node where the updated branch begins
    /// - `stop_parent`: the parent value at which to stop adjusting
    fn _fix_rotated_subtree_sizes(&mut self, mut u: usize, stop_parent: usize) {
        let mut p = self.nodes[u].parent;
        while p != stop_parent {
            self.nodes[u].subtree_size -= self.nodes[p].subtree_size;
            self.nodes[p].subtree_size += self.nodes[u].subtree_size;
            u = p;
            p = self.nodes[p].parent;
        }
    }

    fn unlink(&mut self, u: usize, v: usize) -> (usize, i32) {
        let subtree_u_size = self.nodes[u].subtree_size;

        let mut root_v = 0;
        let mut p = v;
        while p != SENTINEL {
            self.nodes[p].subtree_size -= subtree_u_size;
            root_v = p;
            p = self.nodes[p].parent;
        }
        self.nodes[u].parent = SENTINEL;
        (root_v, subtree_u_size)
    }
}

// MARK: Extensions

impl IDTree {
    /// Rooted Tree-Based Fundamental Cycle Basis
    pub fn cycle_basis(&mut self, root: Option<usize>) -> Vec<Vec<usize>> {
        // Constructs a fundamental cycle basis for the connected component containing `root`,
        // using the ID-Tree structure as its spanning tree. A fundamental cycle is formed
        // each time a non-tree edge is encountered during DFS from the `root`.
        if root.is_none() {
            return vec![];
        }
        let root = root.unwrap();

        let mut cycles = Vec::with_capacity(self.n / 2);

        let stack = &mut self.vec_scratch_stack;
        let in_component = &mut self.node_bitset_scratch0;

        stack.clear();
        in_component.clear();

        stack.push(root);
        in_component.set(root, true);

        while let Some(u) = stack.pop() {
            for &v in &self.nodes[u].neighbors {
                let v = v as usize;
                if !in_component[v] {
                    stack.push(v);
                    in_component.set(v, true);
                }

                let pu = self.nodes[u].parent;
                let pv = self.nodes[v].parent;
                if pu == v || pv == u {
                    continue;
                }

                if u >= v {
                    continue;
                }

                // Found a fundamental cycle via (u, v)
                let mut path_u = Vec::with_capacity(self.n);
                let mut path_v = Vec::with_capacity(self.n);
                path_u.push(u);
                path_v.push(v);

                let visited_u = &mut self.node_bitset_scratch1;
                let visited_v = &mut self.node_bitset_scratch2;
                visited_u.clear();
                visited_v.clear();
                visited_u.set(u, true);
                visited_v.set(v, true);

                let mut a = u;
                let mut b = v;

                while a != b {
                    if self.nodes[a].parent != SENTINEL {
                        a = self.nodes[a].parent;
                        if visited_u[a] {
                            break;
                        }
                        visited_u.set(a, true);

                        path_u.push(a);

                        if visited_v[a] {
                            break;
                        }
                    }
                    if self.nodes[b].parent != SENTINEL && a != b {
                        b = self.nodes[b].parent;
                        if visited_v[b] {
                            break;
                        }
                        visited_v.set(b, true);

                        path_v.push(b);

                        if visited_u[b] {
                            break;
                        }
                    }
                }

                let lca = *path_u.iter().rev().find(|x| path_v.contains(x)).unwrap();
                while path_u.last() != Some(&lca) {
                    path_u.pop();
                }
                while path_v.last() != Some(&lca) {
                    path_v.pop();
                }
                path_v.pop(); // avoid repeating lca

                path_v.reverse();
                path_u.extend(path_v);
                cycles.push(path_u);
            }
        }

        cycles
    }

    /// Return the connected component containing node v.
    pub fn node_connected_component(&mut self, v: usize) -> Vec<usize> {
        let mut stack = vec![v];
        let mut visited = IntSet::from_iter([v]);
        while let Some(node) = stack.pop() {
            for &neighbor in self.nodes[node].neighbors.iter() {
                if visited.insert(neighbor as usize) {
                    stack.push(neighbor as usize);
                }
            }
        }
        visited.into_iter().collect()
    }

    /// Return the connected component containing node v.
    pub fn node_connected_component_bitset(&mut self, v: usize) -> FixedBitSet {
        let stack = &mut self.vec_scratch_stack;
        let visited = &mut self.node_bitset_scratch0;

        stack.clear();
        visited.clear();

        stack.push(v);
        visited.insert(v);

        while let Some(node) = stack.pop() {
            stack.extend(
                self.nodes[node]
                    .neighbors
                    .iter()
                    .filter(|&v| !visited.put(*v as usize))
                    .map(|v| *v as usize),
            )
        }

        visited.clone()
    }

    /// Return the number of connected components.
    pub fn num_connected_components(&mut self) -> usize {
        (0..self.n)
            .filter(|&i| self.nodes[i].parent == SENTINEL && !self.is_isolated(i))
            .count()
    }

    /// Return the connected components.
    pub fn connected_components(&mut self) -> Vec<Vec<usize>> {
        let roots: Vec<_> = (0..self.n)
            .filter(|&i| self.nodes[i].parent == SENTINEL && !self.is_isolated(i))
            .collect();
        roots
            .into_iter()
            .map(|i| self.node_connected_component(i))
            .collect()
    }

    /// Return the active nodes.
    pub fn active_nodes_vec(&mut self) -> Vec<usize> {
        (0..self.n).filter(|&i| !self.is_isolated(i)).collect()
    }

    /// Return the active nodes.
    pub fn active_nodes_set(&mut self) -> IntSet<usize> {
        let mut active_nodes =
            IntSet::with_capacity_and_hasher(self.n, BuildNoHashHasher::default());
        for i in 0..self.n {
            if !self.is_isolated(i) {
                active_nodes.insert(i);
            }
        }
        active_nodes
    }

    /// Return the active nodes.
    pub fn active_nodes_bitset(&mut self) -> FixedBitSet {
        let mut active_nodes = FixedBitSet::with_capacity(self.n);
        for i in 0..self.n {
            if !self.is_isolated(i) {
                active_nodes.insert(i);
            }
        }
        active_nodes
    }

    /// Isolate a single node by removing all incident edges.
    pub fn isolate_node(&mut self, v: usize) {
        self.nodes[v].neighbors.clone().iter().for_each(|neighbor| {
            self.delete_edge(v, *neighbor as usize);
        });
    }

    /// Isolate multiple nodes by removing all incident edges.
    pub fn isolate_nodes(&mut self, nodes: Vec<usize>) {
        nodes.iter().for_each(|&v| self.isolate_node(v));
    }

    /// Returns true if the node is isolated.
    pub fn is_isolated(&mut self, v: usize) -> bool {
        self.nodes[v].neighbors.is_empty()
    }

    /// Returns the degree of the node.
    pub fn degree(&mut self, v: usize) -> i32 {
        self.nodes[v].neighbors.len() as i32
    }

    /// Returns the neighbors of the node.
    pub fn neighbors(&mut self, v: usize) -> Vec<usize> {
        self.nodes[v]
            .neighbors
            .iter()
            .cloned()
            .map(|x| x as usize)
            .collect()
    }

    /// Returns the neighbors of the node.
    pub fn neighbors_smallvec(&mut self, v: usize) -> SmallVec<[usize; 8]> {
        self.nodes[v]
            .neighbors
            .iter()
            .cloned()
            .map(|x| x as usize)
            .collect()
    }

    /// Retain only non-isolated nodes from `from_indices`.
    pub fn retain_active_nodes_from(&mut self, from_indices: Vec<usize>) -> Vec<usize> {
        from_indices
            .into_iter()
            .filter(|&neighbor| !self.is_isolated(neighbor))
            .collect()
    }

    /// Returns the shortest path from `start` to `target` in the undirected graph,
    /// using idtree adjacency graph.
    ///
    /// The path is returned as a vector of node indices from `start` to `target`,
    /// inclusive. If no path exists, returns `None`.
    pub fn shortest_path(&mut self, start: usize, target: usize) -> Option<Vec<usize>> {
        if start >= self.n || target >= self.n {
            return None;
        }
        if start == target {
            return Some(vec![start]);
        }

        let queue = &mut self.deque_scratch;
        queue.clear();

        let parents = &mut self.vec_scratch_nodes;
        // TODO: fix - we dont want to do this on each shortest path call
        parents.resize(self.n, SENTINEL);

        let visited = &mut self.generations;
        self.generation += 1;

        queue.push_back(start);
        visited[start] = self.generation;
        parents[start] = usize::MAX;

        let mut found = false;
        while let Some(u) = queue.pop_front() {
            if u == target {
                found = true;
                break;
            }

            for &v in &self.nodes[u].neighbors {
                if visited[v as usize] != self.generation {
                    visited[v as usize] = self.generation;
                    parents[v as usize] = u;
                    queue.push_back(v as usize);
                }
            }
        }

        if !found {
            return None;
        }

        let mut path = Vec::with_capacity(32);
        let mut current = target;
        while current != usize::MAX {
            path.push(current);
            current = parents[current];
        }
        path.reverse();
        Some(path)
    }

    /// Computes betweenness for candidate nodes via idtree adjacency graph.
    ///
    /// NOTE: This is an undirected, unweighted betweenness result.
    pub fn compute_subset_betweenness(
        &mut self,
        removal_candidates: &[(usize, usize)],
        affected_terminals: &RapidHashSet<(usize, usize)>,
        affected_base_towns: &IntSet<usize>,
        super_root: Option<usize>,
    ) -> IntMap<usize, usize> {
        if removal_candidates.is_empty() || affected_terminals.is_empty() {
            return removal_candidates.iter().map(|&(v, _)| (v, 0)).collect();
        }

        // Group terminals by root
        let mut root_to_terminals: IntMap<usize, SmallVec<[usize; 16]>> = IntMap::default();
        for &(terminal, pair_root) in affected_terminals {
            root_to_terminals
                .entry(pair_root)
                .or_default()
                .push(terminal);
        }

        let num_terminals = affected_terminals.len();
        let num_roots = root_to_terminals.len();
        let num_candidates = removal_candidates.len();

        // Decision: grouped is cheaper if #roots + #candidates < #terminals
        // TODO: Validate this threshold on a larger variety of test instances.
        let use_grouped = (num_roots + num_candidates) < num_terminals;
        if use_grouped {
            if num_terminals < num_candidates {
                self.compute_subset_betweenness_grouped_terminal_centric(
                    removal_candidates,
                    root_to_terminals,
                    affected_base_towns,
                    super_root,
                )
            } else {
                self.compute_subset_betweenness_grouped_candidate_centric(
                    removal_candidates,
                    root_to_terminals,
                    affected_base_towns,
                    super_root,
                )
            }
        } else {
            self.compute_subset_betweenness_pairwise(
                removal_candidates,
                root_to_terminals,
                affected_base_towns,
                super_root,
            )
        }
    }

    /// Betweenness via idtree adjacency graph using BFS per pair.
    fn compute_subset_betweenness_pairwise(
        &mut self,
        removal_candidates: &[(usize, usize)],
        root_to_terminals: IntMap<usize, SmallVec<[usize; 16]>>,
        affected_base_towns: &IntSet<usize>,
        super_root: Option<usize>,
    ) -> IntMap<usize, usize> {
        let mut index_to_betweenness: IntMap<usize, usize> =
            removal_candidates.iter().map(|&(v, _)| (v, 0)).collect();

        if let Some(super_root) = super_root {
            for (pair_root, terminals_for_root) in root_to_terminals {
                if pair_root == super_root {
                    // Accumulate all paths to each base town for the super terminal
                    for terminal in terminals_for_root {
                        for &base_town in affected_base_towns {
                            if let Some(path) = self.shortest_path(terminal, base_town) {
                                for &node in &path {
                                    if let Some(count) = index_to_betweenness.get_mut(&node) {
                                        *count += 1;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    for terminal in terminals_for_root {
                        if let Some(path) = self.shortest_path(pair_root, terminal) {
                            for &node in &path {
                                if let Some(count) = index_to_betweenness.get_mut(&node) {
                                    *count += 1;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            for (pair_root, terminals_for_root) in root_to_terminals {
                for terminal in terminals_for_root {
                    if let Some(path) = self.shortest_path(pair_root, terminal) {
                        for &node in &path {
                            if let Some(count) = index_to_betweenness.get_mut(&node) {
                                *count += 1;
                            }
                        }
                    }
                }
            }
        }

        index_to_betweenness
    }

    /// Betweenness via idtree adjacency graph using triangle equality via BFS per root
    /// and per removal candidate.
    fn compute_subset_betweenness_grouped_candidate_centric(
        &mut self,
        removal_candidates: &[(usize, usize)],
        mut root_to_terminals: IntMap<usize, SmallVec<[usize; 16]>>,
        _affected_base_towns: &IntSet<usize>,
        super_root: Option<usize>,
    ) -> IntMap<usize, usize> {
        let mut betweenness_counts = vec![0usize; self.n];

        let mut candidate_filter = vec![false; self.n];
        for &(candidate_index, _) in removal_candidates {
            candidate_filter[candidate_index] = true;
        }

        if let Some(super_root) = super_root {
            root_to_terminals.remove(&super_root);
        }

        // Phase 1: Cache distances from each root.
        let mut dist_from_root_cache: IntMap<usize, Vec<i32>> = IntMap::default();
        for &pair_root in root_to_terminals.keys() {
            self.compute_distances_from_internal(pair_root);
            dist_from_root_cache.insert(pair_root, self.distances.clone());
        }

        // Phase 2: Triangle Equality Check.
        for &(candidate, _) in removal_candidates {
            self.compute_distances_from_internal(candidate);
            let mut current_candidate_betweenness = 0;

            for (&pair_root, terminals) in &root_to_terminals {
                let distances_from_root = &dist_from_root_cache[&pair_root];
                let distance_root_to_candidate = distances_from_root[candidate];

                // If the root cannot reach the candidate, it cannot be on a path to terminals.
                if distance_root_to_candidate < 0 {
                    continue;
                }

                for &terminal in terminals {
                    let distance_root_to_terminal = distances_from_root[terminal];
                    let distance_candidate_to_terminal = self.distances[terminal];

                    // Check if candidate is on the shortest path between root and terminal.
                    if distance_root_to_terminal >= 0
                        && self.generations[terminal] == self.generation
                        && distance_root_to_terminal
                            == distance_root_to_candidate + distance_candidate_to_terminal
                    {
                        current_candidate_betweenness += 1;
                    }
                }
            }
            betweenness_counts[candidate] = current_candidate_betweenness;
        }

        removal_candidates
            .iter()
            .map(|&(v, _)| (v, betweenness_counts[v]))
            .collect()
    }

    fn compute_subset_betweenness_grouped_terminal_centric(
        &mut self,
        removal_candidates: &[(usize, usize)],
        mut root_to_terminals: IntMap<usize, SmallVec<[usize; 16]>>,
        _affected_base_towns: &IntSet<usize>,
        super_root: Option<usize>,
    ) -> IntMap<usize, usize> {
        let mut betweenness_counts = vec![0usize; self.n];

        let mut candidate_filter = vec![false; self.n];
        for &(candidate_index, _) in removal_candidates {
            candidate_filter[candidate_index] = true;
        }

        if let Some(super_root) = super_root {
            root_to_terminals.remove(&super_root);
        }

        // Phase 1: Cache distances from each root.
        let mut dist_from_root_cache: IntMap<usize, Vec<i32>> = IntMap::default();
        for &pair_root in root_to_terminals.keys() {
            self.compute_distances_from_internal(pair_root);
            dist_from_root_cache.insert(pair_root, self.distances.clone());
        }

        // Phase 2: Triangle Equality Check (Terminal-Centric Inversion).
        // Instead of BFS per candidate, we BFS once per unique terminal.
        for (&pair_root, terminals) in &root_to_terminals {
            let distances_from_root = &dist_from_root_cache[&pair_root];

            for &terminal in terminals {
                let distance_root_to_terminal = distances_from_root[terminal];

                if distance_root_to_terminal < 0 {
                    continue;
                }

                self.compute_distances_from_internal(terminal);

                for &(candidate, _) in removal_candidates {
                    let distance_root_to_candidate = distances_from_root[candidate];
                    let distance_candidate_to_terminal = self.distances[candidate];

                    if distance_root_to_candidate >= 0
                        && self.generations[candidate] == self.generation
                        && distance_root_to_terminal
                            == distance_root_to_candidate + distance_candidate_to_terminal
                    {
                        betweenness_counts[candidate] += 1;
                    }
                }
            }
        }

        removal_candidates
            .iter()
            .map(|&(v, _)| (v, betweenness_counts[v]))
            .collect()
    }

    /// Internal helper to populate distance and generation arrays for a source node.
    fn compute_distances_from_internal(&mut self, source: usize) {
        self.generation += 1;
        if self.generation == 0 {
            self.generations.fill(0);
            self.generation = 1;
        }

        let queue = &mut self.deque_scratch;
        queue.clear();

        self.distances[source] = 0;
        self.generations[source] = self.generation;
        queue.push_back(source);

        while let Some(u) = queue.pop_front() {
            let distance_to_u = self.distances[u];
            for &v in &self.nodes[u].neighbors {
                if self.generations[v as usize] != self.generation {
                    self.generations[v as usize] = self.generation;
                    self.distances[v as usize] = distance_to_u + 1;
                    queue.push_back(v as usize);
                }
            }
        }
    }
}
