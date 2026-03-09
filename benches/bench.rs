use divan::Bencher;
use idtree::IdTree;
use nohash_hasher::{IntMap, IntSet};

const ARGS: &[usize] = &[1_000, 10_000, 100_000];

fn make_adj(n: usize) -> IntMap<usize, IntSet<usize>> {
    let mut adj: IntMap<usize, IntSet<usize>> = IntMap::default();
    for i in 0..n {
        adj.insert(i, IntSet::default());
    }
    adj
}

fn make_edges(n: usize) -> Vec<(usize, usize)> {
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let u = i % n;
        let v = (i * 7 + 13) % n;
        if u != v {
            edges.push((u, v));
        }
    }
    edges
}

#[divan::bench(args = ARGS)]
fn bench_build_from_adj(bencher: Bencher, n: usize) {
    let mut adj = make_adj(n);
    let edges = make_edges(n);

    // populate adjacency list once
    for &(u, v) in &edges {
        adj.get_mut(&u).unwrap().insert(v);
        adj.get_mut(&v).unwrap().insert(u);
    }

    bencher.with_inputs(|| adj.clone()).bench_refs(|adj| {
        let _ = IdTree::new(adj);
    });
}

#[divan::bench(args = ARGS)]
fn bench_insert(bencher: Bencher, n: usize) {
    let adj = make_adj(n);
    let edges = make_edges(n);
    let tree = IdTree::new(&adj);

    bencher
        .with_inputs(|| (edges.clone(), tree.clone()))
        .bench_refs(|(edges, tree)| {
            for (u, v) in edges {
                tree.insert_edge(*u, *v);
            }
        });
}

#[divan::bench(args = ARGS)]
fn bench_query(bencher: Bencher, n: usize) {
    let adj = make_adj(n);
    let mut tree = IdTree::new(&adj);
    let edges = make_edges(n);

    // populate once
    for &(u, v) in &edges {
        tree.insert_edge(u, v);
    }

    bencher.bench_local(move || {
        for &(u, v) in &edges {
            let _ = tree.query(u, v);
        }
    });
}

#[divan::bench(args = ARGS)]
fn bench_delete(bencher: Bencher, n: usize) {
    let adj = make_adj(n);
    let mut tree = IdTree::new(&adj);
    let edges = make_edges(n);

    // populate once
    for &(u, v) in &edges {
        tree.insert_edge(u, v);
    }

    bencher
        .with_inputs(|| (edges.clone(), tree.clone()))
        .bench_refs(|(edges, tree)| {
            for (u, v) in edges {
                tree.delete_edge(*u, *v);
            }
        });
}

fn main() {
    divan::main();
}
