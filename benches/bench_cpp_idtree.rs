// bench_cpp_idtree.rs
// cargo bench --features cpp
mod common;

use std::time::Instant;

use crate::common::BenchData;
use crate::common::Task;
use crate::common::TaskVariant;
use crate::common::report;

const TASKS: [Task; 1] = [Task {
    variant: TaskVariant::CPPIDTree,
}];

// MARK: Data Preparation

#[cfg(feature = "cpp")]
fn setup_cpp_tree(
    n: usize,
    edges: &[(usize, usize)],
    use_dsu: bool,
) -> cxx::UniquePtr<idtree::bridge::ffi::CPPDNDTree> {
    use idtree::bridge::ffi;

    let mut adj = vec![vec![]; n];
    for &(u, v) in edges {
        if u < n && v < n {
            adj[u as usize].push(v);
            adj[v as usize].push(u);
        }
    }

    let mut degrees = Vec::with_capacity(n);
    let mut flat_neighbors = Vec::new();
    for neighbors in &adj {
        degrees.push(neighbors.len() as i32);
        for &v in neighbors {
            flat_neighbors.push(v as i32);
        }
    }

    ffi::new_cpp_dndtree_from_flat_adj(n as i32, &degrees, &flat_neighbors, use_dsu)
}

// MARK: Benchmarks

#[cfg(feature = "cpp")]
fn trace_creation(data: &BenchData) -> Vec<(i32, usize)> {
    let mut trace = Vec::with_capacity(data.edges.len());

    let time = Instant::now();
    let _tree = setup_cpp_tree(data.n, &[], true);
    let elapsed = time.elapsed().as_nanos() as usize;
    trace.push((2, elapsed));

    trace
}

#[cfg(feature = "cpp")]
fn trace_insertion(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = setup_cpp_tree(data.n, &[], true);
    let mut trace = Vec::with_capacity(data.edges.len());

    for &(u, v) in data.edges.iter() {
        let time = Instant::now();
        let res = tree.insert_edge(u as i32, v as i32);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res, elapsed));
    }

    trace
}

#[cfg(feature = "cpp")]
fn trace_query_cold(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = setup_cpp_tree(data.n, &data.edges, true);
    let mut trace = Vec::with_capacity(data.edges.len());

    for &(u, v) in data.del_edges.iter() {
        tree.delete_edge(u as i32, v as i32);
    }

    for &(u, v) in data.query_edges.iter() {
        let time = Instant::now();
        let res = tree.query(u as i32, v as i32);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }

    trace
}

#[cfg(feature = "cpp")]
fn trace_query_warm(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = setup_cpp_tree(data.n, &data.edges, true);
    for &(u, v) in data.del_edges.iter() {
        tree.delete_edge(u as i32, v as i32);
    }

    // run through one time to warm up
    for &(u, v) in data.query_edges.iter() {
        tree.query(u as i32, v as i32);
    }

    let mut trace = Vec::with_capacity(data.query_edges.len());

    for &(u, v) in data.query_edges.iter() {
        let time = Instant::now();
        let res = tree.query(u as i32, v as i32);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }

    trace
}

#[cfg(feature = "cpp")]
fn trace_delete(data: &BenchData) -> Vec<(i32, usize)> {
    let tree = setup_cpp_tree(data.n, &data.edges, true);
    let mut trace = Vec::with_capacity(data.edges.len());

    for &(u, v) in data.edges.iter() {
        let time = Instant::now();
        let res = tree.delete_edge(u as i32, v as i32);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }

    trace
}

#[cfg(feature = "cpp")]
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

#[cfg(feature = "cpp")]
fn main() {
    println!("\nPreparing road-usroads-48.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("road-usroads-48.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 10);
    }

    println!("\nPreparing bdo_exploration_graph.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("bdo_exploration_graph.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 1000);
    }
}

#[cfg(not(feature = "cpp"))]
fn main() {}
