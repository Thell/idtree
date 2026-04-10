// This is primarily an ad-hoc profiling build.
//
#![allow(dead_code)]
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use hdrhistogram::Histogram;
use idtree::IDTree;
use nohash_hasher::{IntMap, IntSet};
use rand::SeedableRng;
use rand::prelude::SliceRandom;
use rand::prelude::*;
use rand::rngs::StdRng;
use statrs::statistics::Statistics;

// MARK: Tasks

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TaskVariant {
    IDTree,
}

impl TaskVariant {
    fn name(&self) -> &'static str {
        match self {
            TaskVariant::IDTree => "IDTree",
        }
    }
}

#[derive(Clone, Copy)]
struct Task {
    variant: TaskVariant,
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.variant.name())
    }
}

impl Task {
    fn new(variant: TaskVariant) -> Task {
        Task { variant }
    }
}

const TASKS: [Task; 1] = [Task {
    variant: TaskVariant::IDTree,
}];

// MARK: Reporting

#[derive(Debug, Clone, Copy)]
enum OpType {
    Insertion,
    QueryCold,
    QueryWarm,
    Deletion,
}

impl fmt::Display for OpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpType::Insertion => write!(f, "INSERTION"),
            OpType::QueryCold => write!(f, "QUERY (COLD)"),
            OpType::QueryWarm => write!(f, "QUERY (WARM)"),
            OpType::Deletion => write!(f, "DELETION"),
        }
    }
}

fn report(task: Task, sample_data: Vec<Vec<Vec<(i32, usize)>>>) {
    println!("\nTASK: {}", task);
    println!(
        "{:<24} | {:<8} | {:<10} | {:<10} | {:<8} | {:<8} | {:<8}",
        "Result Type", "Count", "Mean (ns)", "StdDev", "P50", "P99", "Max"
    );

    let op_types = [
        OpType::Insertion,
        OpType::QueryCold,
        OpType::QueryWarm,
        OpType::Deletion,
    ];

    for (op_idx, op_type) in op_types.iter().enumerate() {
        let mut raw_nanos_map: IntMap<i32, Vec<usize>> = IntMap::default();

        for sample in &sample_data {
            if let Some(trace) = sample.get(op_idx) {
                for &(code, nanos) in trace {
                    raw_nanos_map.entry(code).or_default().push(nanos);
                }
            }
        }

        if raw_nanos_map.is_empty() {
            continue;
        }

        println!("{}", "-".repeat(90));
        println!("--- {} ---", op_type);

        let mut sorted_codes: Vec<_> = raw_nanos_map.keys().collect();
        sorted_codes.sort();

        for &code in sorted_codes {
            let nanos_vec = &raw_nanos_map[&code];

            let mut hist = Histogram::<u64>::new_with_bounds(1, 100_000_000, 3).unwrap();
            let mut f64_samples: Vec<f64> = Vec::with_capacity(nanos_vec.len());

            for &n in nanos_vec {
                let _ = hist.record(n as u64);
                f64_samples.push(n as f64);
            }

            let mean = f64_samples.as_slice().mean();
            let std_dev = f64_samples.as_slice().std_dev();

            // Map integer codes to descriptive labels based on OpType
            let label = match op_type {
                OpType::Insertion => match code {
                    0 => "Non-Tree Edge",
                    1 => "Tree Edge",
                    2 => "Non-Tree Reroot",
                    3 => "Tree Reroot",
                    _ => "Invalid/Other",
                },
                OpType::Deletion => match code {
                    0 => "Non-Tree Edge",
                    1 => "Tree Edge (Split)",
                    2 => "Tree Edge (Replaced)",
                    _ => "Invalid/Other",
                },
                OpType::QueryCold | OpType::QueryWarm => match code {
                    0 => "Disconnected",
                    1 => "Connected",
                    _ => "Invalid/Other",
                },
            };

            println!(
                "{:<24} | {:<8} | {:<10.2} | {:<10.2} | {:<8} | {:<8} | {:<8}",
                label,
                nanos_vec.len(),
                mean,
                std_dev,
                hist.value_at_quantile(0.5),
                hist.value_at_quantile(0.99),
                hist.max()
            );
        }
    }
}

// MARK: Data Preparation
struct BenchData {
    all_edges: Vec<(usize, usize)>,
    empty_map: IntMap<usize, IntSet<usize>>,
    id_tree: IDTree,
    query_id_tree: IDTree,
    query_edges: Vec<(usize, usize)>,
}

impl BenchData {
    fn new(filename: &str) -> Self {
        let adj_list = BenchData::load_graph(filename);
        let mut adj_dict: IntMap<usize, IntSet<usize>> = IntMap::default();
        for &u in adj_list.keys() {
            adj_dict.entry(u as usize).or_default();
            for &v in adj_list.get(&u).unwrap() {
                adj_dict.entry(v as usize).or_default();
                adj_dict.get_mut(&(u as usize)).unwrap().insert(v as usize);
                adj_dict.get_mut(&(v as usize)).unwrap().insert(u as usize);
            }
        }

        let mut empty_map: IntMap<usize, IntSet<usize>> = IntMap::default();
        for &u in adj_list.keys() {
            empty_map.entry(u as usize).or_default();
            for &v in adj_list.get(&u).unwrap() {
                empty_map.entry(v as usize).or_default();
            }
        }

        let mut all_edges: Vec<(usize, usize)> = adj_list
            .iter()
            .flat_map(|(&u, neighbors)| neighbors.iter().map(move |&v| (u as usize, v as usize)))
            .collect();

        let mut rng = StdRng::seed_from_u64(12345);
        all_edges.shuffle(&mut rng);

        let id_tree = IDTree::from_adj(&adj_dict);

        let n = adj_dict.len();
        let mut query_edges = Vec::new();
        for _ in 0..1000 {
            let u = rng.random_range(0..n);
            let v = rng.random_range(0..n);
            if u == v {
                continue;
            }
            query_edges.push((u, v));
        }

        // Delete some of the edges to create cold trees
        let deleted_count = all_edges.len() / 5;
        let del_edges = all_edges.iter().take(deleted_count).collect::<Vec<_>>();

        let mut query_id_tree = id_tree.clone();
        for &(u, v) in del_edges.iter() {
            query_id_tree.delete_edge(*u, *v);
        }

        BenchData {
            all_edges,
            empty_map,
            id_tree,
            query_id_tree,
            query_edges,
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

fn trace_insertion(_task: Task, data: &BenchData) -> Vec<(i32, usize)> {
    let mut tree = IDTree::from_adj(&data.empty_map);
    let mut trace = Vec::with_capacity(data.all_edges.len());

    for &(u, v) in data.all_edges.iter() {
        let time = Instant::now();
        let res = tree.insert_edge(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res, elapsed));
    }
    trace
}

fn trace_query_cold(task: Task, data: &BenchData) -> Vec<(i32, usize)> {
    let tree = match task.variant {
        TaskVariant::IDTree => data.query_id_tree.clone(),
    };
    let mut trace = Vec::with_capacity(data.query_edges.len());

    for &(u, v) in data.query_edges.iter() {
        let time = Instant::now();
        let res = tree.query(u, v);
        let elapsed = time.elapsed().as_nanos() as usize;
        trace.push((res as i32, elapsed));
    }
    trace
}

fn trace_query_warm(task: Task, data: &BenchData) -> Vec<(i32, usize)> {
    let tree = match task.variant {
        TaskVariant::IDTree => data.query_id_tree.clone(),
    };
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

fn trace_delete(task: Task, data: &BenchData) -> Vec<(i32, usize)> {
    let mut tree = match task.variant {
        TaskVariant::IDTree => data.id_tree.clone(),
    };
    let mut trace = Vec::with_capacity(data.all_edges.len());

    for &(u, v) in data.all_edges.iter() {
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
            trace_insertion(task, data),
            trace_query_cold(task, data),
            trace_query_warm(task, data),
            trace_delete(task, data),
        ];
        sample_data.push(traces);
    }

    report(task, sample_data);
}

fn profile(n: usize, filename: &str) {
    println!("\nPreparing {} data...", filename);

    let start_time = Instant::now();
    let bench_data = BenchData::new(filename);
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    let task = Task::new(TaskVariant::IDTree);
    bench_task(task, &bench_data, n as u32);
}

// Take argv arguments for n and mtx data file name
fn main() {
    use std::env;
    let args: Vec<String> = env::args().collect();

    let n: usize = args[1].parse().unwrap();
    let filename: &str = &args[2];

    let start_time = std::time::Instant::now();
    profile(n, filename);
    let elapsed = start_time.elapsed();

    println!("{} took {} microseconds", n, elapsed.as_micros());
}
