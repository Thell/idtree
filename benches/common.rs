use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use hdrhistogram::Histogram;
use nohash_hasher::{IntMap, IntSet};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use statrs::statistics::Statistics;

// MARK: Data Preparation
#[allow(unused)]
pub struct BenchData {
    pub n: usize,
    pub edges: Vec<(usize, usize)>,
    pub del_edges: Vec<(usize, usize)>,
    pub query_edges: Vec<(usize, usize)>,
    pub adj_dict: IntMap<usize, IntSet<usize>>,
}

impl BenchData {
    pub fn new(filename: &str) -> Self {
        let mut edges = load_graph_as_edges(filename);
        let adj_dict = graph_edges_to_adj_dict(&edges);
        let n = adj_dict.len();

        let mut rng = StdRng::seed_from_u64(12345);
        edges.shuffle(&mut rng);

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
        let del_count = edges.len() / 5;
        let del_edges = edges.iter().take(del_count).cloned().collect::<Vec<_>>();

        BenchData {
            n,
            edges,
            del_edges,
            query_edges,
            adj_dict,
        }
    }
}

pub fn load_graph_as_edges(filename: &str) -> Vec<(usize, usize)> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mtx_path = PathBuf::from(manifest_dir)
        .join("benches")
        .join("data")
        .join(filename);
    let reader = BufReader::new(File::open(mtx_path).expect("MTX file missing"));

    let mut edges: Vec<(usize, usize)> = Vec::new();
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
            let mut u: usize = parts[0].parse().unwrap();
            let mut v: usize = parts[1].parse().unwrap();
            // Canonicalize
            if u > v {
                std::mem::swap(&mut u, &mut v);
            }
            // MTX is 1-based; decrement to 0-based.
            // Only add once as the graph is undirected and insert_edge handles symmetry.
            edges.push((u - 1, v - 1));
        }
    }
    edges
}

pub fn graph_edges_to_adj_dict(edges: &[(usize, usize)]) -> IntMap<usize, IntSet<usize>> {
    let mut adj_dict: IntMap<usize, IntSet<usize>> = IntMap::default();
    for &(u, v) in edges.iter() {
        adj_dict.entry(u).or_default();
        adj_dict.entry(v).or_default();
        adj_dict.get_mut(&u).unwrap().insert(v);
        adj_dict.get_mut(&v).unwrap().insert(u);
    }
    adj_dict
}

// MARK: Tasks

#[allow(unused)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskVariant {
    CPPDNDTree,
    CPPIDTree,
    IDTree,
    DNDTree,
}

impl TaskVariant {
    pub fn name(&self) -> &'static str {
        match self {
            TaskVariant::CPPDNDTree => "CPPDNDTree",
            TaskVariant::CPPIDTree => "CPPIDTree",
            TaskVariant::IDTree => "IDTree",
            TaskVariant::DNDTree => "DNDTree",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Task {
    pub variant: TaskVariant,
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.variant.name())
    }
}

// MARK: Reporting

#[derive(Debug, Clone, Copy)]
enum OpType {
    Creation,
    Insertion,
    QueryCold,
    QueryWarm,
    Deletion,
}

impl fmt::Display for OpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpType::Creation => write!(f, "CREATION"),
            OpType::Insertion => write!(f, "INSERTION"),
            OpType::QueryCold => write!(f, "QUERY (COLD)"),
            OpType::QueryWarm => write!(f, "QUERY (WARM)"),
            OpType::Deletion => write!(f, "DELETION"),
        }
    }
}

pub fn report(task: Task, sample_data: Vec<Vec<Vec<(i32, usize)>>>) {
    println!("\nTASK: {}", task);
    println!(
        "{:<24} | {:<8} | {:<10} | {:<10} | {:<8} | {:<8} | {:<8}",
        "Result Type", "Count", "Mean (ns)", "StdDev", "P50", "P99", "Max"
    );

    let op_types = [
        OpType::Creation,
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
                OpType::Creation => match code {
                    0 => "n nodes (no edges)",
                    1 => "n nodes + Insertion",
                    2 => "from_edges",
                    3 => "from_adj",
                    4 => "reset_all_edges",
                    5 => "Reset + Insertion",
                    6 => "reset_all_edges_to_edges",
                    7 => "reset_all_edges_to_adj",
                    8 => "Build Adj From Edges",
                    _ => "invalid/other",
                },
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
