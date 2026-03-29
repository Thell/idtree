/*
   The API being benched is:

   /// Creating a new Tree
   ///   using an adjacency list
   ///   inserting edge by edge into an empty tree

   /// Inserting an edge
   /// Returns:
   ///   -1 if the edge is invalid
   ///   0 if the edge inserted was a non-tree edge
   ///   1 if the edge inserted was a tree edge
   ///   2 if the edge inserted was a non-tree edge triggering a reroot
   ///   3 if the edge inserted was a tree edge triggering a reroot

   /// Deleting an edge
   /// Returns:
   ///   -1 if the edge is invalid
   ///   0 if the edge deleted was a non-tree edge
   ///   1 if the edge deleted was a tree edge (a new component is created)
   ///   2 if the edge deleted was a tree edge and a replacement edge was found

   /// Methodology (Insertion benching):
   Testing the insertion of the edges poses a bit of a challenge since each insertion modifies
   the state of the tree and benching a single insertion is not sufficient. To isolate the edges
   to use during the bench the full graph is generated and each edge insertion is classified by
   its type (tree, non-tree, reroot tree, reroot non-tree).

   These edges are then deleted and classified by their deletion type one at a time from a clone
   of the tree thereby identifying isolated edges that have a low probability of interacting with
   other edges used for the bench.

   Lastly, for each vector of classified deleted edges, these edges are again deleted one at a time
   from a single clone of the tree and the ones that retain the same classification are used for
   the bench by starting with a fresh tree and then deleting the edges outside of the benching
   loop and then inserting the edges inside the benching loop.

   /// Methodology (Deletion benching):
   The same initial methodology is used as for the insertion benching except that the final step
   of the benching loop is to delete the edges inside the benching loop and skip the insertion.

   /// Querying an edge
   /// NOTES:
   ///   ID-Tree: always traverses parents to find the roots
   ///   DND-Tree: traverses roots when the path has not been compressed
   ///   DND-Tree: with link compression traverses the roots when the path has not been compressed
   ///            and compresses the DSU link list as well.
   /// Returns:
   ///   True if the edge ends are connected
   ///   False if the edge ends are not connected
*/
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use hdrhistogram::Histogram;
use idtree::IdTree;
use nohash_hasher::{IntMap, IntSet};
use rand::prelude::*;
use statrs::statistics::Statistics;

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

        println!("--- {} ---", op_type);
        println!(
            "{:<24} | {:<8} | {:<10} | {:<10} | {:<8} | {:<8} | {:<8}",
            "Result Type", "Count", "Mean (ns)", "StdDev", "P50", "P99", "Max"
        );
        println!("{}", "-".repeat(90));

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
    id_tree: IdTree,
    query_id_tree: IdTree,
    query_edges: Vec<(usize, usize)>,
}

impl BenchData {
    fn new(filename: &str) -> Self {
        let adj_list = BenchData::load_graph(filename);
        let mut adj_dict: IntMap<usize, IntSet<usize>> = IntMap::default();
        for &u in adj_list.keys() {
            adj_dict.entry(u).or_default();
            for &v in adj_list.get(&u).unwrap() {
                adj_dict.entry(v).or_default();
                adj_dict.get_mut(&u).unwrap().insert(v);
                adj_dict.get_mut(&v).unwrap().insert(u);
            }
        }

        let mut empty_map: IntMap<usize, IntSet<usize>> = IntMap::default();
        for &u in adj_list.keys() {
            empty_map.entry(u).or_default();
            for &v in adj_list.get(&u).unwrap() {
                empty_map.entry(v).or_default();
            }
        }

        let mut all_edges: Vec<(usize, usize)> = adj_list
            .iter()
            .flat_map(|(&u, neighbors)| neighbors.iter().map(move |&v| (u as usize, v as usize)))
            .collect();

        let mut rng = StdRng::seed_from_u64(42);
        all_edges.shuffle(&mut rng);

        let id_tree = IdTree::new(&adj_dict);

        let mut rng = StdRng::seed_from_u64(12345);
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

        let mut query_id_tree = IdTree::new(&adj_dict);
        for &(u, v) in all_edges.iter().take(deleted_count) {
            query_id_tree.delete_edge(u, v);
        }

        BenchData {
            all_edges,
            empty_map,
            id_tree,
            query_id_tree,
            query_edges,
        }
    }

    fn load_graph(filename: &str) -> IntMap<usize, Vec<usize>> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let mtx_path = PathBuf::from(manifest_dir)
            .join("benches")
            .join("data")
            .join(filename);
        let reader = BufReader::new(File::open(mtx_path).expect("MTX file missing"));

        let mut adj_list: IntMap<usize, Vec<usize>> = IntMap::default();
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
                adj_list.entry(u - 1).or_default().push(v - 1);
            }
        }
        adj_list
    }
}

fn trace_insertion(_task: Task, data: &BenchData) -> Vec<(i32, usize)> {
    let mut tree = IdTree::new(&data.empty_map);
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

fn main() {
    println!("Preparing road-usroads-48.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("road-usroads-48.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 100);
    }

    println!("Preparing bdo_exploration_graph.mtx data...");
    let start_time = Instant::now();
    let bench_data = BenchData::new("bdo_exploration_graph.mtx");
    println!("Prepared in {} ms", start_time.elapsed().as_millis());

    for task in TASKS {
        bench_task(task, &bench_data, 1000);
    }
}
