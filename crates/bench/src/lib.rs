// Requires nightly.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

/// Shared benchmark runner for compliance benchmarks.
///
/// Provides timing, statistics, CSV output, and table formatting.
/// Each benchmark binary collects `BenchResult`s and writes a CSV.
pub mod bench_runner {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::time::Instant;

    /// A single benchmark measurement with structured dimensions.
    pub struct BenchResult {
        pub version: String,
        /// Ordered key-value pairs describing what was measured.
        /// e.g. [("scenario", "1S1O"), ("stage", "build"), ("mode", "serial")]
        pub dimensions: Vec<(String, String)>,
        pub mean_ms: f64,
        pub median_ms: f64,
        pub min_ms: f64,
        pub max_ms: f64,
        pub std_dev_ms: f64,
        pub samples: usize,
        pub constraints: Option<usize>,
    }

    /// Run a benchmark: warmup iterations, then timed samples.
    /// Returns raw times in milliseconds.
    pub fn run_bench(warmup: usize, samples: usize, mut f: impl FnMut()) -> Vec<f64> {
        for _ in 0..warmup {
            f();
        }
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let start = Instant::now();
            f();
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times
    }

    /// Compute (mean, median, min, max, std_dev) from raw ms times.
    pub fn stats(times: &[f64]) -> (f64, f64, f64, f64, f64) {
        assert!(!times.is_empty());
        let n = times.len() as f64;
        let mean = times.iter().sum::<f64>() / n;
        let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let mut sorted = times.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        };

        let variance = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        (mean, median, min, max, std_dev)
    }

    /// Build a BenchResult from raw times and structured dimensions.
    pub fn make_result(
        version: &str,
        dims: &[(&str, &str)],
        times: &[f64],
        constraints: Option<usize>,
    ) -> BenchResult {
        let (mean, median, min, max, std_dev) = stats(times);
        BenchResult {
            version: version.to_string(),
            dimensions: dims
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            mean_ms: mean,
            median_ms: median,
            min_ms: min,
            max_ms: max,
            std_dev_ms: std_dev,
            samples: times.len(),
            constraints,
        }
    }

    /// Create a BenchResult with 0ms (for v0 operations that didn't exist).
    pub fn make_zero_result(version: &str, dims: &[(&str, &str)], samples: usize) -> BenchResult {
        BenchResult {
            version: version.to_string(),
            dimensions: dims
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            mean_ms: 0.0,
            median_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            std_dev_ms: 0.0,
            samples,
            constraints: None,
        }
    }

    /// Collect all dimension keys across results, preserving insertion order.
    fn dimension_keys(results: &[BenchResult]) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut keys = Vec::new();
        for r in results {
            for (k, _) in &r.dimensions {
                if seen.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
        }
        keys
    }

    /// Write results to CSV. Creates parent directories if needed.
    /// Columns: version, <dimension keys...>, mean_ms, median_ms, std_dev_ms, samples, constraints
    pub fn write_csv(path: &Path, results: &[BenchResult]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("can create results directory");
        }

        let dim_keys = dimension_keys(results);

        // Header
        let mut csv = String::from("version");
        for k in &dim_keys {
            csv.push(',');
            csv.push_str(k);
        }
        csv.push_str(",mean_ms,median_ms,std_dev_ms,samples,constraints\n");

        // Rows
        for r in results {
            csv.push_str(&r.version);
            for k in &dim_keys {
                csv.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    csv.push_str(v);
                }
            }
            let constraints = r.constraints.map(|c| c.to_string()).unwrap_or_default();
            csv.push_str(&format!(
                ",{:.2},{:.2},{:.2},{},{}\n",
                r.mean_ms, r.median_ms, r.std_dev_ms, r.samples, constraints,
            ));
        }
        fs::write(path, &csv).expect("can write CSV");
        eprintln!("CSV written to {}", path.display());
    }

    /// Print a formatted comparison table to stdout.
    pub fn print_table(results: &[BenchResult]) {
        let dim_keys = dimension_keys(results);

        // Build display label from dimensions
        let label = |r: &BenchResult| -> String {
            r.dimensions
                .iter()
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join("/")
        };

        let ver_w = 8;
        let op_w = results
            .iter()
            .map(|r| label(r).len())
            .max()
            .unwrap_or(10)
            .max(10);
        let num_w = 12;

        // Header
        let dim_header = dim_keys.join("/");
        let header_label = if dim_header.is_empty() {
            "OPERATION".to_string()
        } else {
            dim_header.to_uppercase()
        };
        println!(
            "\n{:<ver_w$}  {:<op_w$}  {:>num_w$}  {:>num_w$}  {:>num_w$}  {:>num_w$}  {:>num_w$}  {:>8}  {:>12}",
            "VERSION", header_label, "MEAN (ms)", "MEDIAN", "MIN", "MAX", "STD DEV", "SAMPLES", "CONSTRAINTS",
        );
        println!("{}", "-".repeat(ver_w + op_w + num_w * 5 + 8 + 12 + 16));

        for r in results {
            let constraints = r
                .constraints
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{:<ver_w$}  {:<op_w$}  {:>num_w$.2}  {:>num_w$.2}  {:>num_w$.2}  {:>num_w$.2}  {:>num_w$.2}  {:>8}  {:>12}",
                r.version,
                label(r),
                r.mean_ms,
                r.median_ms,
                r.min_ms,
                r.max_ms,
                r.std_dev_ms,
                r.samples,
                constraints,
            );
        }
        println!();
    }
}
