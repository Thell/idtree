use idtree::IdTree;
use nohash_hasher::{IntMap, IntSet};
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

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

fn connected_idtree(tree: &mut IdTree, u: usize, v: usize) -> bool {
    tree.query(u, v)
}

#[test]
fn test_basic_insert_delete_query() {
    let edges = vec![(0, 1), (1, 2), (2, 3)];
    let adj = make_adj_usize(4, &edges);
    let mut t = IdTree::new(&adj);

    assert!(t.query(0, 3), "initial query (0, 3) failed");
    t.delete_edge(1, 2);
    assert!(!t.query(0, 3), "query (0, 3) after delete failed");
    t.insert_edge(1, 2);
    assert!(t.query(0, 3), "query (0, 3) after insert failed");
}

#[test]
fn test_unlink_splits_correctly() {
    let edges = vec![(0, 1), (1, 2), (2, 3)];
    let adj = make_adj_usize(4, &edges);
    let mut t = IdTree::new(&adj);

    t.delete_edge(1, 2);
    assert!(t.query(0, 1));
    assert!(!t.query(0, 3));
    assert!(t.query(2, 3));
}

#[test]
fn test_replacement_edge_found() {
    let edges = vec![(0, 1), (1, 2), (2, 3), (0, 3)];
    let adj = make_adj_usize(4, &edges);
    let mut t = IdTree::new(&adj);

    let r = t.delete_edge(1, 2);
    assert_eq!(r, 1);
    assert!(t.query(1, 2));
    assert!(t.query(0, 3));
}

#[test]
fn test_replacement_edge_not_found() {
    let edges = vec![(0, 1), (1, 2), (2, 3)];
    let adj = make_adj_usize(4, &edges);
    let mut t = IdTree::new(&adj);

    let r = t.delete_edge(1, 2);
    assert_eq!(r, 2);
    assert!(!t.query(0, 3));
}

#[test]
fn test_creation_via_adj_matches_insertion() {
    let mut rng = StdRng::seed_from_u64(99999);
    let n = 50;
    let mut edges = vec![];

    while edges.len() < 100 {
        let u = rng.random_range(0..n);
        let v = rng.random_range(0..n);
        if u != v {
            edges.push((u, v));
        }
    }

    let adj_id = make_adj_usize(n, &edges);
    let mut idt1 = IdTree::new(&adj_id);

    let empty_adj = make_adj_usize(n, &[]);
    let mut idt2 = IdTree::new(&empty_adj);
    for &(u, v) in &edges {
        idt2.insert_edge(u, v);
    }

    for u in 0..n {
        for v in 0..n {
            assert_eq!(
                connected_idtree(&mut idt1, u, v),
                connected_idtree(&mut idt2, u, v)
            );
        }
    }
}
