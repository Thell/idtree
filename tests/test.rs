// NOTE: This testing setup is the same as used for the DNDTree project.

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;

use idtree::IDTree;
use nohash_hasher::{IntMap, IntSet};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

use idtree::bridge::ffi;

// MARK: MTX handling
struct MtxData {
    all_edges: Vec<(usize, usize)>,
    empty_map_i32: IntMap<i32, IntSet<i32>>,
    empty_map_usize: IntMap<usize, IntSet<usize>>,
}

impl MtxData {
    fn new(filename: &str) -> Self {
        let adj_list = MtxData::load_graph(filename);
        let mut adj_dict: IntMap<i32, IntSet<i32>> = IntMap::default();
        for &u in adj_list.keys() {
            adj_dict.entry(u).or_default();
            for &v in adj_list.get(&u).unwrap() {
                adj_dict.entry(v).or_default();
                adj_dict.get_mut(&u).unwrap().insert(v);
                adj_dict.get_mut(&v).unwrap().insert(u);
            }
        }

        let mut empty_map_i32: IntMap<i32, IntSet<i32>> = IntMap::default();
        for &u in adj_list.keys() {
            empty_map_i32.entry(u).or_default();
            for &v in adj_list.get(&u).unwrap() {
                empty_map_i32.entry(v).or_default();
            }
        }

        let mut empty_map_usize: IntMap<usize, IntSet<usize>> = IntMap::default();
        for &u in adj_list.keys() {
            empty_map_usize.entry(u as usize).or_default();
            for &v in adj_list.get(&u).unwrap() {
                empty_map_usize.entry(v as usize).or_default();
            }
        }

        let all_edges: Vec<(usize, usize)> = adj_list
            .iter()
            .flat_map(|(&u, neighbors)| neighbors.iter().map(move |&v| (u as usize, v as usize)))
            .collect();

        MtxData {
            all_edges,
            empty_map_i32,
            empty_map_usize,
        }
    }

    fn load_graph(filename: &str) -> IntMap<i32, Vec<i32>> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let mtx_path = PathBuf::from(manifest_dir)
            .join("benches")
            .join("data")
            .join(filename);
        let reader = BufReader::new(File::open(mtx_path).expect("MTX file missing"));

        let mut adj_list: IntMap<i32, Vec<i32>> = IntMap::default();
        let mut data_started = false;

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('%') {
                continue;
            }
            if !data_started {
                data_started = true;
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let mut u: i32 = parts[0].parse().unwrap();
                let mut v: i32 = parts[1].parse().unwrap();
                // Canonicalize
                if u > v {
                    std::mem::swap(&mut u, &mut v);
                }
                // MTX is 1-based; decrement to 0-based.
                // Only add once as the graph is undirected and insert_edge handles symmetry.
                adj_list.entry(u - 1).or_default().push(v - 1);
            }
        }
        adj_list
    }
}

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
        let mut t = IDTree::new(&adj);

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
        let mut t = IDTree::new(&adj);

        t.delete_edge(1, 2);
        assert!(t.query(0, 1));
        assert!(!t.query(0, 3));
        assert!(t.query(2, 3));
    }

    #[test]
    fn test_replacement_edge_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3), (0, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IDTree::new(&adj);

        let r = t.delete_edge(1, 2);
        assert_eq!(r, 1);
        assert!(t.query(1, 2));
        assert!(t.query(0, 3));
    }

    #[test]
    fn test_replacement_edge_not_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let adj = make_adj_i32(4, &edges);
        let mut t = IDTree::new(&adj);

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

        let mut tree = IDTree::new(&adj);

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

        let mut tree = IDTree::new(&adj);

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

fn make_adj_usize(n: usize, edges: &[(usize, usize)]) -> IntMap<usize, IntSet<usize>> {
    let mut adj: IntMap<usize, IntSet<usize>> = IntMap::default();
    for i in 0..n {
        adj.insert(i, IntSet::default());
    }
    for &(u, v) in edges {
        adj.get_mut(&u).unwrap().insert(v);
        adj.get_mut(&v).unwrap().insert(u);
    }
    adj
}

fn setup_cpp_tree(
    n: usize,
    edges: &[(i32, i32)],
    use_dsu: bool,
) -> cxx::UniquePtr<ffi::CPPDNDTree> {
    let mut adj = vec![vec![]; n];
    for &(u, v) in edges {
        if u >= 0 && u < n as i32 && v >= 0 && v < n as i32 {
            adj[u as usize].push(v);
            adj[v as usize].push(u);
        }
    }

    let mut degrees = Vec::with_capacity(n);
    let mut flat_neighbors = Vec::new();
    for neighbors in &adj {
        degrees.push(neighbors.len() as i32);
        for &v in neighbors {
            flat_neighbors.push(v);
        }
    }

    ffi::new_cpp_dndtree_from_flat_adj(n as i32, &degrees, &flat_neighbors, use_dsu)
}

mod cpp_tests {
    use super::*;
    use cxx::UniquePtr;
    use idtree::bridge::ffi;
    use log::debug;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    // Switch this when tracing
    pub const USE_DSU: bool = false; // Both DSU and no DSU expect to pass all tests

    #[test]
    fn test_basic_insert_delete_query() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let t = setup_cpp_tree(4, &edges, USE_DSU);

        assert!(t.query(0, 3), "query 1");
        t.delete_edge(1, 2);
        assert!(!t.query(0, 3), "query 2");
        t.insert_edge(1, 2);
        assert!(t.query(0, 3), "query 3");
    }

    #[test]
    fn test_unlink_splits_correctly() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let t = setup_cpp_tree(4, &edges, USE_DSU);

        t.delete_edge(1, 2);
        assert!(t.query(0, 1));
        assert!(!t.query(0, 3));
        assert!(t.query(2, 3));
    }

    #[test]
    fn test_replacement_edge_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3), (0, 3)];
        let t = setup_cpp_tree(4, &edges, USE_DSU);

        let r = t.delete_edge(1, 2);
        assert_eq!(r, 1);
        assert!(t.query(1, 2));
        assert!(t.query(0, 3));
    }

    #[test]
    fn test_replacement_edge_not_found() {
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let t = setup_cpp_tree(4, &edges, USE_DSU);

        let r = t.delete_edge(1, 2);
        assert_eq!(r, 2);
        assert!(!t.query(0, 3));
    }

    #[test]
    fn test_dndtree_matches_idtree() {
        let mut rng = StdRng::seed_from_u64(99999);
        let n = 50;
        let mut edges = vec![];

        for _ in 0..100 {
            let u = rng.random_range(0..n);
            let v = rng.random_range(0..n);
            if u != v {
                edges.push((u, v));
            }
        }

        let adj_id = make_adj_usize(n, &edges);
        let cpp_edges: Vec<(i32, i32)> = edges.iter().map(|&(u, v)| (u as i32, v as i32)).collect();

        let cpp = setup_cpp_tree(n, &cpp_edges, USE_DSU);
        let mut idt = IDTree::new(&adj_id);

        for _ in 0..200 {
            let u = rng.random_range(0..n);
            let v = rng.random_range(0..n);

            let op = rng.random_range(0..3);
            match op {
                0 => {
                    cpp.insert_edge(u as i32, v as i32);
                    idt.insert_edge(u, v);
                }
                1 => {
                    cpp.delete_edge(u as i32, v as i32);
                    idt.delete_edge(u, v);
                }
                _ => {}
            }

            for _ in 0..20 {
                let a = rng.random_range(0..n);
                let b = rng.random_range(0..n);
                assert_eq!(cpp.query(a as i32, b as i32), idt.query(a, b));
            }
        }
    }

    #[test]
    fn test_mixed_ops_query_heavy() {
        let n = 1_000;
        let query_factor = 10;

        let edges = make_edges_for_nodes(n, n * 2);
        let (present_edges, absent_edges) = edges.split_at(n);

        let cpp_init: Vec<(i32, i32)> = present_edges
            .iter()
            .map(|&(u, v)| (u as i32, v as i32))
            .collect();

        let mut max_node_id = 0;
        for &(u, v) in present_edges.iter() {
            max_node_id = max_node_id.max(u);
            max_node_id = max_node_id.max(v);
        }
        for &(u, v) in absent_edges.iter() {
            max_node_id = max_node_id.max(u);
            max_node_id = max_node_id.max(v);
        }
        assert!(max_node_id <= n);
        debug!("max_node_id = {}, n = {}", max_node_id, n);
        let tree = setup_cpp_tree(max_node_id, &cpp_init, USE_DSU);

        let mut present: Vec<usize> = (0..n).collect();
        let mut absent: Vec<usize> = (0..n).collect();

        debug!("begin");
        for i in 0..n {
            let pi = present[i];
            let (du, dv) = present_edges[pi];
            debug!("  delete edge ({}, {})", du, dv);
            tree.delete_edge(du as i32, dv as i32);

            for q in 0..query_factor {
                let qi = present[(i + q) % n];
                let (qu, qv) = present_edges[qi];
                debug!("  query ({}, {})", qu, qv);
                let _ = tree.query(qu as i32, qv as i32);
            }

            let ai = absent[i];
            let (iu, iv) = absent_edges[ai];
            debug!("  insert edge ({}, {})", iu, iv);
            tree.insert_edge(iu as i32, iv as i32);

            present[i] = ai;
            absent[i] = pi;
        }
    }

    #[test]
    fn mixed_ops_query_heavy_catgraph() {
        const QUERY_FACTOR: f64 = 0.05;
        let n = 20_000;
        let mut rng = StdRng::seed_from_u64(12345);

        let edges = make_edges_for_nodes(n, n * 2);
        let (present_edges, absent_edges) = edges.split_at(n);

        let adj = make_caterpillar_graph(n, 0.3, 0.05);
        let mut init_edges = vec![];
        for (u, neighbors) in adj.iter() {
            for &v in neighbors {
                if (*u as i32) < v as i32 {
                    init_edges.push((*u as i32, v as i32));
                }
            }
        }

        let tree = setup_cpp_tree(n, &init_edges, USE_DSU);

        let num_query_pairs = (QUERY_FACTOR * n as f64) as usize;
        let mut query_pairs = Vec::with_capacity(num_query_pairs);
        for _ in 0..num_query_pairs {
            query_pairs.push((rng.random_range(0..n), rng.random_range(0..n)));
        }

        let mut present: Vec<usize> = (0..n).collect();
        let mut absent: Vec<usize> = (0..n).collect();

        for i in 0..n {
            let pi = present[i];
            let (du, dv) = present_edges[pi];
            tree.delete_edge(du as i32, dv as i32);

            for &(qu, qv) in &query_pairs {
                let _ = tree.query(qu as i32, qv as i32);
            }

            let ai = absent[i];
            let (iu, iv) = absent_edges[ai];
            tree.insert_edge(iu as i32, iv as i32);

            present[i] = ai;
            absent[i] = pi;
        }
    }

    fn verify_idtree_topology_sync(cpp: &UniquePtr<ffi::CPPDNDTree>, idt: &IDTree, n: usize) {
        for i in 0..n {
            let cpp_p = cpp.get_tree_parent(i as i32);
            let idt_p = idt.get_parent(i);

            if cpp_p != idt_p as i32 {
                panic!(
                    "TOPOLOGY DRIFT at Node {}: CPP parent = {}, IDT parent = {}",
                    i, cpp_p, idt_p
                );
            }
        }
    }

    // This test will fail without sorting of neighbors
    #[test]
    #[ignore]
    fn test_dndtree_matches_idtree_mtx() {
        use std::io::Write; // Required for flushing

        // let _ = simple_logger::SimpleLogger::new()
        //     .with_level(log::LevelFilter::Debug)
        //     .init();

        // dndtree::bridge::ffi::set_cpp_trace(true);

        let mtx_data = MtxData::new("bdo_exploration_graph.mtx");
        let all_nodes = mtx_data.empty_map_i32.keys().collect::<Vec<_>>();
        let node_count = all_nodes.len();

        let cpp = setup_cpp_tree(all_nodes.len(), &[], USE_DSU);
        let mut idt = IDTree::new(&mtx_data.empty_map_usize);

        // Insertion Phase
        for &(u, v) in mtx_data.all_edges.iter() {
            debug!(
                "----------------------------------------------\n inserting edge u = {u}, v = {v}\n----------------------------------------------"
            );
            let _ = std::io::stdout().flush();

            debug!("-----CPPDNDTree");
            let _ = std::io::stdout().flush();

            let cpp_res = cpp.insert_edge(u as i32, v as i32);

            debug!("\n-----IDTree");
            let _ = std::io::stdout().flush();

            let idt_res = idt.insert_edge(u as usize, v as usize);

            verify_idtree_topology_sync(&cpp, &idt, node_count);

            if cpp_res != idt_res as i32 {
                panic!(
                    "Insert results don't match u = {u}, v = {v}, cpp_res = {cpp_res}, idt_res = {idt_res}"
                );
            }
        }

        // Deletion Phase with Connectivity Checks
        for &(u, v) in mtx_data.all_edges.iter() {
            debug!(
                "----------------------------------------------\n deleting edge u = {u}, v = {v}\n----------------------------------------------"
            );
            debug!("-----CPPDNDTree");
            let _ = std::io::stdout().flush();

            let cpp_res = cpp.delete_edge(u as i32, v as i32);

            debug!("\n-----IDTree");
            let _ = std::io::stdout().flush();

            // Perform the operation that is likely causing the trace cutoff
            let idt_res = idt.delete_edge(u as usize, v as usize);

            verify_idtree_topology_sync(&cpp, &idt, node_count);

            // Critical flush: If idt.delete_edge panicked or corrupted the state,
            // we want the internal logger to have pushed its buffer out.
            let _ = std::io::stdout().flush();

            if cpp_res != idt_res as i32 {
                debug!(
                    "Delete results don't match u = {u}, v = {v}, cpp_res = {cpp_res}, idt_res = {idt_res}"
                );
                let _ = std::io::stdout().flush();
                panic!("CPPDNDTree and IDTree diverged!");
            }

            let cpp_conn = cpp.query(u as i32, v as i32);
            let idt_conn = idt.query(u as usize, v as usize);
            if cpp_conn != idt_conn {
                debug!(
                    "Query results don't match after delete u = {u}, v = {v}, cpp_res = {cpp_conn}, idt_res = {idt_conn}"
                );
                let _ = std::io::stdout().flush();
                panic!("CPPDNDTree and IDTree diverged!");
            }
        }

        // Final Connectivity Sweep
        for &(u, v) in mtx_data.all_edges.iter() {
            assert_eq!(
                cpp.query(u as i32, v as i32),
                idt.query(u as usize, v as usize),
                "Final connectivity mismatch for edge ({}, {})",
                u,
                v
            );
        }
    }
}
