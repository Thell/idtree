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
    pub neighbors: SmallVec<[usize; 4]>,
}

impl Node {
    fn new() -> Self {
        Node {
            parent: SENTINEL,
            subtree_size: 1,
            neighbors: SmallVec::new(),
        }
    }

    fn insert_neighbor(&mut self, u: usize) -> i32 {
        if !self.neighbors.contains(&(u as usize)) {
            self.neighbors.push(u as usize);
            // // Sorting is for use during the development cycle for divergence testing of op logic
            // self.neighbors.sort();
            return 0;
        }
        1
    }

    fn delete_neighbor(&mut self, u: usize) -> i32 {
        if let Some(i) = self.neighbors.iter().position(|&x| x == u as usize) {
            self.neighbors.swap_remove(i);
            // // Sorting is for use during the development cycle for divergence testing of op logic
            // self.neighbors.sort();
            return 0;
        }
        1
    }
}

/// DNDTree
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

    vec_scratch_nodes: Vec<usize>,
    vec_scratch_stack: Vec<usize>,
    generation: u32,
    generations: Vec<u32>,

    distances: Vec<i32>,               // (used for betweenness)
    deque_scratch: VecDeque<usize>,    // scratch area (used by shortest path)
    node_bitset_scratch0: FixedBitSet, // |nodes| len scratch area
    node_bitset_scratch1: FixedBitSet, // |nodes| len scratch area
    node_bitset_scratch2: FixedBitSet, // |nodes| len scratch area
}

impl IDTree {
    /// Create a new DNDTree
    pub fn new(adj_dict: &IntMap<usize, IntSet<usize>>) -> Self {
        let mut instance = Self::setup(&adj_dict);
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
        let res = self.insert_edge_balanced(u, v);
        res
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
        let res = self.delete_edge_balanced(u, v);
        res
    }

    /// Query if u and v are in the same connected component
    pub fn query(&self, u: usize, v: usize) -> bool {
        if u >= self.n || v >= self.n {
            return false;
        }
        self.get_tree_root(u) == self.get_tree_root(v)
    }

    /// Get parent of node
    pub fn get_parent(&self, u: usize) -> usize {
        self.nodes[u].parent
    }

    /// TODO: Remove after debugging
    pub fn get_node_data(&self, u: usize) -> Node {
        self.nodes[u].clone()
    }
}

impl IDTree {
    // NOTE: After setup completes all node, neighbor and lnode entries are
    //       guaranteed to be within range 0..self.n
    // SAFETY: No function should be added to the struct that allows direct modification
    //         of any of these fields
    #[inline(always)]
    fn setup(adj_dict: &IntMap<usize, IntSet<usize>>) -> Self {
        let n = adj_dict.len();
        let nodes: Vec<Node> = (0..n)
            .map(|i| {
                let mut node = Node::new();
                for &j in adj_dict.get(&(i)).unwrap_or(&IntSet::default()) {
                    assert!(j < n, "invalid neighbor {} of {}", j, adj_dict.len());
                    node.insert_neighbor(j);
                }
                node
            })
            .collect();

        Self {
            n,
            nodes,
            vec_scratch_nodes: vec![],
            vec_scratch_stack: vec![],
            generation: 1,
            generations: vec![0; n],

            distances: vec![0; n],
            deque_scratch: VecDeque::with_capacity(n),
            node_bitset_scratch0: FixedBitSet::with_capacity(n),
            node_bitset_scratch1: FixedBitSet::with_capacity(n),
            node_bitset_scratch2: FixedBitSet::with_capacity(n),
        }
    }

    #[inline(always)]
    fn initialize(&mut self) {
        self.generation = self.generation.wrapping_add(1);

        let sorted_nodes = self.sort_nodes_by_degree();

        for &node in sorted_nodes.iter() {
            if self.generations[node] == self.generation {
                continue;
            }

            // NOTE: each subtree is setup in the scratch collection which is reused
            //       to find the centroid
            self.bfs_setup_subtrees(node);
            if let Some(centroid) = self.find_centroid_in_q() {
                self.reroot(centroid, node);
            }
        }
    }

    #[inline(always)]
    fn sort_nodes_by_degree(&self) -> Vec<usize> {
        let mut node_indices: Vec<usize> = (0..self.n).collect();
        node_indices.sort_unstable_by(|&a, &b| {
            self.nodes[b]
                .neighbors
                .len()
                .cmp(&self.nodes[a].neighbors.len())
        });
        node_indices
    }

    #[inline(always)]
    fn bfs_setup_subtrees(&mut self, root: usize) {
        use std::collections::VecDeque;
        let mut deque = VecDeque::new();
        deque.push_back(root);

        self.vec_scratch_nodes.clear();
        self.vec_scratch_nodes.push(root);

        self.generations[root] = self.generation;

        while let Some(p) = deque.pop_front() {
            for j in 0..self.nodes[p].neighbors.len() {
                let neighbor = self.nodes[p].neighbors[j] as usize;

                if self.generations[neighbor] != self.generation {
                    self.generations[neighbor] = self.generation;

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

    #[inline(always)]
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
    // SAFETY: Unchecked access is safe because all public functions check range
    //         and after setup completes all entries are within range 0..self.n
    //         and all node accesses are within range 0..self.n.
    // NOTE: Sentinel value of usize::MAX is reserved for NULL for parent usage only
    // TODO: Switch to NonMax type once stable https://github.com/rust-lang/rust/issues/151435
    #[inline(always)]
    fn node(&self, i: usize) -> &Node {
        debug_assert!(i < self.n);
        unsafe { self.nodes.get_unchecked(i) }
    }

    #[inline(always)]
    fn node_mut(&mut self, i: usize) -> &mut Node {
        debug_assert!(i < self.n);
        unsafe { self.nodes.get_unchecked_mut(i) }
    }
}

// MARK: Base functions

impl IDTree {
    #[inline(always)]
    fn delete_edge_in_graph(&mut self, u: usize, v: usize) -> bool {
        self.nodes[u].delete_neighbor(v) == 0 && self.nodes[v].delete_neighbor(u) == 0
    }

    #[inline(always)]
    fn delete_edge_balanced(&mut self, mut u: usize, mut v: usize) -> i32 {
        if (self.node(u).parent != v && self.node(v).parent != u) || u == v {
            return 0;
        }

        if self.nodes[v].parent == u {
            std::mem::swap(&mut u, &mut v);
        }

        let (p, subtree_u_size) = self.unlink(u, v);
        let (small_node, large_node): (usize, usize) = if self.node(p).subtree_size < subtree_u_size
        {
            (p, u)
        } else {
            (u, p)
        };

        if self.find_replacement(small_node, large_node) {
            return 1;
        }
        2
    }

    #[inline(always)]
    fn insert_edge_in_graph(&mut self, u: usize, v: usize) -> bool {
        self.node_mut(u).insert_neighbor(v) == 0 && self.node_mut(v).insert_neighbor(u) == 0
    }

    #[inline(always)]
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
    ///        target half‑subtree size during rebalancing
    #[inline(always)]
    fn insert_non_tree_edge_balanced(&mut self, u: usize, v: usize, f: usize) -> i32 {
        let (reshape, small_node, large_node, small_p, _large_p) =
            self.detect_depth_imbalance(u, v);

        if !reshape {
            return 0;
        }

        // Node at which the subtree should be detached and rerooted.
        let p = self.find_imbalance_centroid(small_node, small_p);

        // Remove the subtree rooted at the detach point from its ancestors.
        self.adjust_subtree_sizes(p, -self.node(p).subtree_size);

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
    #[inline(always)]
    fn insert_tree_edge_balanced(
        &mut self,
        mut u: usize,
        mut v: usize,
        mut fu: usize,
        mut fv: usize,
    ) -> i32 {
        // Ensure fu is the root of the smaller component.
        if self.node(fu).subtree_size > self.node(fv).subtree_size {
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

    #[inline(always)]
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
    #[inline(always)]
    fn detect_depth_imbalance(
        &self,
        mut u: usize,
        mut v: usize,
    ) -> (bool, usize, usize, usize, usize) {
        let mut reshape = false;
        let mut depth = 0;

        let mut pu = self.node(u).parent;
        let mut pv = self.node(v).parent;

        while depth < MAX_DEPTH {
            if pu == SENTINEL {
                if pv != SENTINEL && self.node(pv).parent != SENTINEL {
                    reshape = true;
                    std::mem::swap(&mut u, &mut v);
                    std::mem::swap(&mut pu, &mut pv);
                }
                break;
            } else if pv == SENTINEL {
                if pu != SENTINEL && self.node(pu).parent != SENTINEL {
                    reshape = true;
                }
                break;
            }

            pu = self.node(pu).parent;
            pv = self.node(pv).parent;
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
    #[inline(always)]
    fn find_imbalance_centroid(&self, small_node: usize, small_p: usize) -> usize {
        let mut depth_imbalance = 0;
        let mut p = small_p;

        while p != SENTINEL {
            depth_imbalance += 1;
            p = self.node(p).parent;
        }

        depth_imbalance = depth_imbalance / 2 - 1;

        let mut cur = small_node;
        while depth_imbalance > 0 {
            cur = self.node(cur).parent;
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
    #[inline(always)]
    fn adjust_subtree_sizes(&mut self, start_node: usize, delta: i32) -> usize {
        let mut root_v = start_node;
        let mut p = self.node(start_node).parent;
        while p != SENTINEL {
            self.node_mut(p).subtree_size += delta;
            root_v = p;
            p = self.nodes[p].parent;
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
    #[inline(always)]
    fn rebalance_tree(&mut self, u: usize, v: usize, f: usize) -> Option<usize> {
        let s = (self.node(f).subtree_size + self.node(u).subtree_size) / 2;

        let mut new_root = None;
        let mut p = v;

        while p != SENTINEL {
            self.node_mut(p).subtree_size += self.node(u).subtree_size;
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
    /// - `f`: the root of the other component
    ///
    /// Returns:
    /// - `true` if a replacement edge was found and the tree structure was rebuilt
    #[inline(always)]
    fn find_replacement(&mut self, u: usize, f: usize) -> bool {
        assert_eq!(self.nodes.len(), self.generations.len());

        self.vec_scratch_nodes.clear();

        self.generation = self.generation.wrapping_add(1);
        let cur_gen = self.generation;

        self.vec_scratch_nodes.push(u);
        self.generations[u] = cur_gen;

        let mut i = 0;
        while i < self.vec_scratch_nodes.len() {
            let node = self.vec_scratch_nodes[i];
            let parent = self.node(node).parent;
            i += 1;

            'neighbors: for n_idx in 0..self.node(node).neighbors.len() {
                let neighbor = self.node(node).neighbors[n_idx] as usize;
                if neighbor == parent {
                    continue;
                }

                if node == self.node(neighbor).parent && self.generations[neighbor] != cur_gen {
                    self.vec_scratch_nodes.push(neighbor);
                    self.generations[neighbor] = cur_gen;
                    continue;
                }

                // Is path clear?
                let mut w = neighbor;
                while w != SENTINEL {
                    if self.generations[w] == cur_gen {
                        continue 'neighbors;
                    }
                    self.generations[w] = cur_gen;
                    w = self.node(w).parent;
                }

                let u = self.rotate_tree(node, neighbor);
                let new_root = self.rebalance_tree(u, neighbor, f);
                self.fix_rotated_subtree_sizes(u, neighbor);

                if let Some(new_root) = new_root
                    && new_root != f
                {
                    self.reroot(new_root, f);
                }
                return true;
            }
        }
        false
    }

    /// Reroots the tree by moving the subtree of `u` to `f`.
    #[inline(always)]
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
    #[inline(always)]
    fn rotate_tree(&mut self, start_node: usize, stop_node: usize) -> usize {
        self._rotate_tree(start_node, stop_node)
    }

    /// Rotates the parent pointers along the branch from `start_node` to the root,
    /// so that `start_node` becomes the root of its component.
    ///
    /// Arguments:
    /// - `start_node`: node whose component is being rerooted
    #[inline(always)]
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
    #[inline(always)]
    fn _rotate_tree(&mut self, mut u: usize, new_parent: usize) -> usize {
        let mut p = self.node(u).parent;
        self.nodes[u].parent = new_parent;
        while p < self.nodes.len() {
            let next = self.node(p).parent;
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
    #[inline(always)]
    fn fix_rotated_subtree_sizes(&mut self, start_node: usize, stop_node: usize) {
        self._fix_rotated_subtree_sizes(start_node, stop_node);
    }

    /// After a rotation updates the parent chain of a component, this restores
    /// correct subtree sizes along the affected branch until reaching the root.
    ///
    /// Arguments:
    /// - `start_node`: the node where the updated branch begins
    #[inline(always)]
    fn fix_rotated_subtree_sizes_until_root(&mut self, start_node: usize) {
        self._fix_rotated_subtree_sizes(start_node, SENTINEL);
    }

    /// After a rotation updates the parent chain of a component, this restores
    /// correct subtree sizes along the affected branch until reaching `stop_parent`.
    ///
    /// Arguments:
    /// - `start_node`: the node where the updated branch begins
    /// - `stop_parent`: the parent value at which to stop adjusting
    #[inline(always)]
    fn _fix_rotated_subtree_sizes(&mut self, mut u: usize, stop_parent: usize) {
        let mut p = self.node(u).parent;
        while p != stop_parent {
            self.node_mut(u).subtree_size -= self.node(p).subtree_size;
            self.nodes[p].subtree_size += self.nodes[u].subtree_size;
            u = p;
            p = self.nodes[p].parent;
        }
    }

    /// Unlinks the subtree of `u` from the tree, returning the root of the subtree
    /// and the subtree size of `u`.
    #[inline(always)]
    fn unlink(&mut self, u: usize, v: usize) -> (usize, i32) {
        let subtree_u_size = self.node(u).subtree_size;

        let mut root_v = 0;
        let mut p = v;
        while p != SENTINEL {
            self.node_mut(p).subtree_size -= subtree_u_size;
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
                        a = self.nodes[a].parent as usize;
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
                        b = self.nodes[b].parent as usize;
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
                if visited.insert(neighbor) {
                    stack.push(neighbor);
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
                    .filter(|&v| !visited.put(*v))
                    .copied(),
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
            self.delete_edge(v, *neighbor);
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
        self.nodes[v].neighbors.iter().cloned().collect()
    }

    /// Returns the neighbors of the node.
    pub fn neighbors_smallvec(&mut self, v: usize) -> SmallVec<[usize; 4]> {
        self.nodes[v].neighbors.clone()
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
                if visited[v] != self.generation {
                    visited[v] = self.generation;
                    parents[v] = u;
                    queue.push_back(v);
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
                if self.generations[v] != self.generation {
                    self.generations[v] = self.generation;
                    self.distances[v] = distance_to_u + 1;
                    queue.push_back(v);
                }
            }
        }
    }
}
