use idtree::IdTree;
use nohash_hasher::{IntMap, IntSet};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

fn make_adj_i32(n: usize, edges: &[(usize, usize)]) -> IntMap<usize, IntSet<usize>> {
    let mut adj: IntMap<usize, IntSet<usize>> = IntMap::default();
    for i in 0..n {
        adj.insert(i, IntSet::default());
    }
    for &(u, v) in edges {
        adj.get_mut(&(u)).unwrap().insert(v);
        adj.get_mut(&(v)).unwrap().insert(u);
    }
    adj
}

fn make_adj(n: usize) -> IntMap<usize, IntSet<usize>> {
    let mut adj: IntMap<usize, IntSet<usize>> = IntMap::default();
    for i in 0..n {
        adj.insert(i, IntSet::default());
    }
    adj
}

fn make_edges_for_nodes(node_count: usize, edge_count: usize) -> Vec<(usize, usize)> {
    let mut edges = Vec::with_capacity(edge_count);
    for i in 0..edge_count {
        let u = i % node_count;
        let v = (i * 7 + 13) % node_count;
        if u != v {
            edges.push((u, v));
        }
    }
    edges
}

fn make_caterpillar_graph(
    n: usize,
    spine_length_ratio: f64, // 0.1 = short spine, 0.5 = half nodes on spine
    extra_edges_ratio: f64,  // how many additional random chords
) -> IntMap<usize, IntSet<usize>> {
    let mut adj: IntMap<usize, IntSet<usize>> = IntMap::default();
    for i in 0..n {
        adj.insert(i, IntSet::default());
    }

    let mut rng = StdRng::seed_from_u64(42);

    // 1. Create the long spine (backbone path)
    let spine_len = (n as f64 * spine_length_ratio).max(10.0).min(n as f64) as usize;
    let mut spine = Vec::with_capacity(spine_len);
    for i in 0..spine_len {
        spine.push(i);
        if i > 0 {
            let prev = spine[i - 1];
            adj.get_mut(&prev).unwrap().insert(i);
            adj.get_mut(&(i)).unwrap().insert(prev);
        }
    }

    // 2. Attach remaining nodes as leaves or small trees to the spine
    let mut next_node = spine_len;
    while next_node < n {
        // Pick random spine node to attach to
        let attach_to = spine[rng.random_range(0..spine.len())];

        // Attach a small chain (1–4 nodes) to make subtrees deeper
        let chain_len = rng.random_range(1..=4);
        let mut prev = attach_to;
        for _ in 0..chain_len {
            if next_node >= n {
                break;
            }
            adj.get_mut(&prev).unwrap().insert(next_node);
            adj.get_mut(&next_node).unwrap().insert(prev);
            prev = next_node;
            next_node += 1;
        }
    }

    // 3. Add a few random chords (keep connectivity high but allow splits)
    let extra_count = (n as f64 * extra_edges_ratio) as usize;
    for _ in 0..extra_count {
        let u = rng.random_range(0..n);
        let v = rng.random_range(0..n);
        if u != v && !adj.get(&u).unwrap().contains(&v) {
            adj.get_mut(&u).unwrap().insert(v);
            adj.get_mut(&v).unwrap().insert(u);
        }
    }

    adj
}

mod no_dsu_no_compress {
    use super::*;

    #[test]
    fn test_basic_insert_delete_query() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IdTree::new(&adj);

        assert!(t.query(0, 3), "query 1");
        t.delete_edge(1, 2);
        assert!(!t.query(0, 3), "query 2");
        t.insert_edge(1, 2);
        assert!(t.query(0, 3), "query 3");
    }

    #[test]
    fn test_unlink_splits_correctly() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IdTree::new(&adj);

        t.delete_edge(1, 2);
        assert!(t.query(0, 1));
        assert!(!t.query(0, 3));
        assert!(t.query(2, 3));
    }

    #[test]
    fn test_replacement_edge_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3), (0, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IdTree::new(&adj);

        let r = t.delete_edge(1, 2);
        assert_eq!(r, 1);
        assert!(t.query(1, 2));
        assert!(t.query(0, 3));
    }

    #[test]
    fn test_replacement_edge_not_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IdTree::new(&adj);

        let r = t.delete_edge(1, 2);
        assert_eq!(r, 2);
        assert!(!t.query(0, 3));
    }

    #[test]
    fn test_mixed_ops_query_heavy() {
        use rand::SeedableRng;
        use rand::prelude::SliceRandom;
        use rand::rngs::StdRng;

        let n = 1_000;
        let query_factor = 10;

        let mut edges = make_edges_for_nodes(n, n * 2);
        let mut rng = StdRng::seed_from_u64(12345);
        edges.shuffle(&mut rng);

        let (present_edges, absent_edges) = edges.split_at(n);

        let mut adj = make_adj(n);
        for &(u, v) in present_edges.iter() {
            adj.get_mut(&(u)).unwrap().insert(v);
            adj.get_mut(&(v)).unwrap().insert(u);
        }

        let mut tree = IdTree::new(&adj);

        let mut present: Vec<usize> = (0..n).collect();
        let mut absent: Vec<usize> = (0..n).collect();

        for i in 0..n {
            let pi = present[i];
            let (du, dv) = present_edges[pi];
            tree.delete_edge(du, dv);

            for q in 0..query_factor {
                let qi = present[(i + q) % n];
                let (qu, qv) = present_edges[qi];
                let _ = tree.query(qu, qv);
            }

            let ai = absent[i];
            let (iu, iv) = absent_edges[ai];
            tree.insert_edge(iu, iv);

            present[i] = ai;
            absent[i] = pi;
        }
    }

    #[test]
    fn mixed_ops_query_heavy_catgraph() {
        const QUERY_FACTOR: f64 = 0.05;

        use rand::SeedableRng;
        use rand::prelude::SliceRandom;
        use rand::rngs::StdRng;

        let n = 20_000;
        let mut edges = make_edges_for_nodes(n, n * 2);
        let mut rng = StdRng::seed_from_u64(12345);
        edges.shuffle(&mut rng);

        let (present_edges, absent_edges) = edges.split_at(n);

        let adj = make_caterpillar_graph(n, 0.3, 0.05); // spine ~30% of nodes, 5% extra chords

        // Pre-select random endpoint pairs (not edges)
        let num_query_pairs = (QUERY_FACTOR * n as f64) as usize;
        let mut query_pairs = Vec::with_capacity(num_query_pairs);
        for _ in 0..num_query_pairs {
            let qu = rng.random_range(0..n);
            let qv = rng.random_range(0..n);
            query_pairs.push((qu, qv));
        }

        let mut tree = IdTree::new(&adj);

        let mut present: Vec<usize> = (0..n).collect();
        let mut absent: Vec<usize> = (0..n).collect();

        for i in 0..n {
            let pi = present[i];
            let (du, dv) = present_edges[pi];
            tree.delete_edge(du, dv);

            for &(qu, qv) in &query_pairs {
                let _ = tree.query(qu, qv);
            }

            let ai = absent[i];
            let (iu, iv) = absent_edges[ai];
            tree.insert_edge(iu, iv);

            present[i] = ai;
            absent[i] = pi;
        }
    }
}
