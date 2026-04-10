// bench_idtree.rs
mod common;

use std::time::Instant;

use idtree::IDTree;
use nohash_hasher::{IntMap, IntSet};

use crate::common::BenchData as CommonBenchData;
use crate::common::Task;
use crate::common::TaskVariant;
use crate::common::report;

const TASKS: [Task; 1] = [Task {
    variant: TaskVariant::IDTree,
}];

// MARK: Data Preparation
struct BenchData {
    n: usize,
    edges: Vec<(usize, usize)>,
    adj_dict: IntMap<usize, IntSet<usize>>,
    id_tree: IDTree,
    query_id_tree: IDTree,
    query_edges: Vec<(usize, usize)>,
}

impl BenchData {
    fn new(filename: &str) -> Self {
        let common_data = CommonBenchData::new(filename);
        let n = common_data.n;
        let edges = common_data.edges;
        let adj_dict = common_data.adj_dict;
        let query_edges = common_data.query_edges;
        let del_edges = common_data.del_edges;

        // NOTE: This is only valid for DNDTree and IDTree;
        //       CPPDNDTree doesn't implement clone and has to be recreated each bench
        let id_tree = IDTree::from_adj(&adj_dict);
        let mut query_id_tree = id_tree.clone();
        for &(u, v) in del_edges.iter() {
            query_id_tree.delete_edge(u, v);
        }

        BenchData {
            n,
            edges,
            adj_dict,
            id_tree,
            query_id_tree,
            query_edges,
        }
    }
}

// MARK: Benchmarks

fn trace_creation(data: &BenchData) -> Vec<(i32, usize)> {
    // Empty tree
    let mut rc = 0;
    let mut trace = Vec::new();
    let time = Instant::now();
    let _tree = IDTree::new(data.n);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Empty tree followed by inserting all edges
    rc += 1;
    let time = Instant::now();
    let mut tree = IDTree::new(data.n);
    for &(u, v) in data.edges.iter() {
        tree.insert_edge(u, v);
    }
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // From a list of edges
    rc += 1;
    let time = Instant::now();
    let _tree = IDTree::from_edges(data.n, &data.edges);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // From an adj map
    rc += 1;
    let time = Instant::now();
    let _tree = IDTree::from_adj(&data.adj_dict);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Resetting all edges
    rc += 1;
    let mut tree = IDTree::from_adj(&data.adj_dict);
    let time = Instant::now();
    tree.reset_all_edges();
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Resetting all edges followed by inserting all edges
    rc += 1;
    let mut tree = IDTree::from_adj(&data.adj_dict);
    let time = Instant::now();
    tree.reset_all_edges();
    for &(u, v) in data.edges.iter() {
        tree.insert_edge(u, v);
    }
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Resetting all edges to edge
    rc += 1;
    let mut tree = IDTree::from_adj(&data.adj_dict);
    let time = Instant::now();
    tree.reset_all_edges_to_edges(&data.edges);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Resetting all edges to adj
    rc += 1;
    let mut tree = IDTree::from_adj(&data.adj_dict);
    let time = Instant::now();
    tree.reset_all_edges_to_adj(&data.adj_dict);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    // Build adj from all_edges
    rc += 1;
    let time = Instant::now();
    let mut adj: IntMap<usize, Vec<usize>> = IntMap::default();
    for &(u, v) in data.edges.iter() {
        adj.entry(u).or_default().push(v);
    }
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((rc, elapsed));

    trace
}

fn trace_insertion(data: &BenchData) -> Vec<(i32, usize)> {
    let mut tree = IDTree::new(data.n);
    let mut trace = Vec::with_capacity(data.edges.len());

    for &(u, v) in data.edges.iter() {
        let time = Instant::now();
        let res = tree.insert_edge(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res, elapsed));
    }
    trace
}

fn trace_query_cold(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = data.query_id_tree.clone();
    let mut trace = Vec::with_capacity(data.query_edges.len());

    for &(u, v) in data.query_edges.iter() {
        let time = Instant::now();
        let res = tree.query(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }
    trace
}

fn trace_query_warm(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = data.query_id_tree.clone();
    let mut trace = Vec::with_capacity(data.query_edges.len());

    for &(u, v) in data.query_edges.iter() {
        tree.query(u, v);
    }

    for &(u, v) in data.query_edges.iter() {
        let time = Instant::now();
        let res = tree.query(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }
    trace
}

fn trace_delete(data: &BenchData) -> Vec<(i32, usize)> {
    let mut tree = data.id_tree.clone();
    let mut trace = Vec::with_capacity(data.edges.len());

    for &(u, v) in data.edges.iter() {
        let time = Instant::now();
        let res = tree.delete_edge(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }
    trace
}

fn bench_task(task: Task, data: &BenchData, sample_count: u32) {
    let mut sample_data = Vec::with_capacity(sample_count as usize);
    for _ in 0..sample_count {
        let traces = vec![
            trace_creation(data),
            trace_insertion(data),
            trace_query_cold(data),
            trace_query_warm(data),
            trace_delete(data),
        ];
        sample_data.push(traces);
    }
    report(task, sample_data);
}

fn main() {
    println!("Preparing road-usroads-48.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("road-usroads-48.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 10);
    }

    println!("Preparing bdo_exploration_graph.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("bdo_exploration_graph.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 1000);
    }
}
