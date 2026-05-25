// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod extraction;
pub mod proof_txs;

/// Shared benchmark runner for compliance benchmarks.
///
/// Provides timing, statistics, CSV output, and table formatting.
/// Each benchmark binary collects `BenchResult`s and writes a CSV.
pub mod bench_runner {
    use once_cell::sync::Lazy;
    use std::cmp::Ordering;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Instant;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Circuit metrics extracted from constraint system synthesis.
    #[derive(Clone, Default)]
    pub struct CircuitMetrics {
        pub constraints: usize,
        pub public_inputs: usize,
        pub witness_vars: usize,
    }

    /// A single benchmark measurement with structured dimensions.
    #[derive(Clone)]
    pub struct BenchResult {
        pub version: String,
        /// Ordered key-value pairs describing what was measured.
        /// e.g. [("scenario", "1S1O"), ("stage", "build"), ("mode", "serial")]
        pub dimensions: Vec<(String, String)>,
        pub mean_ms: f64,
        pub median_ms: f64,
        pub samples: usize,
        pub metrics: Option<CircuitMetrics>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BenchProfile {
        Quick,
        Deep,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BenchReport {
        Summary,
        Raw,
        Both,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BenchSuite {
        Complete,
        Regression,
    }

    fn usize_env(var: &str) -> Option<usize> {
        std::env::var(var).ok().and_then(|s| s.parse().ok())
    }

    fn default_warmup_count() -> usize {
        if is_regression_suite() {
            1
        } else {
            match bench_profile() {
                BenchProfile::Quick => 1,
                BenchProfile::Deep => 2,
            }
        }
    }

    fn default_sample_count() -> usize {
        if is_regression_suite() {
            5
        } else {
            match bench_profile() {
                BenchProfile::Quick => 5,
                BenchProfile::Deep => 10,
            }
        }
    }

    pub fn bench_profile() -> BenchProfile {
        match std::env::var("BENCH_PROFILE")
            .unwrap_or_else(|_| "quick".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "deep" => BenchProfile::Deep,
            _ => BenchProfile::Quick,
        }
    }

    pub fn bench_profile_label() -> &'static str {
        match bench_profile() {
            BenchProfile::Quick => "quick",
            BenchProfile::Deep => "deep",
        }
    }

    pub fn is_quick_profile() -> bool {
        bench_profile() == BenchProfile::Quick
    }

    pub fn bench_report() -> BenchReport {
        match std::env::var("BENCH_REPORT")
            .unwrap_or_else(|_| "both".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "summary" => BenchReport::Summary,
            "raw" => BenchReport::Raw,
            _ => BenchReport::Both,
        }
    }

    pub fn bench_suite() -> BenchSuite {
        match std::env::var("BENCH_SUITE")
            .unwrap_or_else(|_| "complete".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "regression" => BenchSuite::Regression,
            "full" | "complete" => BenchSuite::Complete,
            _ => BenchSuite::Complete,
        }
    }

    pub fn is_regression_suite() -> bool {
        bench_suite() == BenchSuite::Regression
    }

    pub fn should_write_summary() -> bool {
        matches!(bench_report(), BenchReport::Summary | BenchReport::Both)
    }

    pub fn should_write_raw() -> bool {
        matches!(bench_report(), BenchReport::Raw | BenchReport::Both)
    }

    static RUN_ID: Lazy<String> = Lazy::new(|| {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("run-{ts}-{}", std::process::id())
    });

    fn host_label() -> String {
        std::env::var("BENCH_HOST_LABEL")
            .or_else(|_| std::env::var("HOSTNAME"))
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown-host".to_string())
    }

    fn git_rev() -> String {
        std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string())
    }

    pub fn annotate_raw_results(results: &mut [BenchResult]) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let profile = bench_profile_label().to_string();
        let run_id = RUN_ID.clone();
        let git_rev = git_rev();
        let host_label = host_label();

        for r in results {
            let mut add_dim = |k: &str, v: &str| {
                if !r.dimensions.iter().any(|(dk, _)| dk == k) {
                    r.dimensions.push((k.to_string(), v.to_string()));
                }
            };
            add_dim("profile", &profile);
            add_dim("run_id", &run_id);
            add_dim("timestamp", &ts);
            add_dim("git_rev", &git_rev);
            add_dim("host_label", &host_label);
        }
    }

    pub fn raw_csv_path(summary_csv_path: &Path) -> PathBuf {
        let parent = summary_csv_path
            .parent()
            .expect("summary path has a parent directory");
        let stem = summary_csv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("summary path has a valid file stem");
        parent.join("raw").join(format!("{stem}_raw.csv"))
    }

    pub fn compliance_results_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance")
    }

    pub fn flows_overview_csv_path() -> PathBuf {
        compliance_results_root().join("flows.csv")
    }

    pub fn category_csv_path(category: &str) -> PathBuf {
        compliance_results_root()
            .join(category)
            .join(format!("{category}.csv"))
    }

    pub fn category_sections_csv_path(category: &str) -> PathBuf {
        compliance_results_root()
            .join(category)
            .join("sections.csv")
    }

    pub fn section_csv_path(category: &str, section: &str) -> PathBuf {
        compliance_results_root()
            .join(category)
            .join("sections")
            .join(format!("{section}.csv"))
    }

    pub fn subsection_csv_path(category: &str, section: &str, subsection: &str) -> PathBuf {
        compliance_results_root()
            .join(category)
            .join("sections")
            .join(format!("{section}_subsections"))
            .join(format!("{subsection}.csv"))
    }

    fn version_rank(version: &str) -> u8 {
        if version.starts_with("base") {
            0
        } else if version == "dev" {
            1
        } else if version == "local" {
            2
        } else {
            3
        }
    }

    fn value_rank(value: &str) -> (u8, i64, String) {
        match value {
            "serial" => return (0, 0, String::new()),
            "parallel" => return (0, 1, String::new()),
            "concurrent" => return (0, 2, String::new()),
            "per_tx" => return (1, i64::MAX, String::new()),
            _ => {}
        }

        if let Some(pct) = value.strip_suffix('%') {
            if let Ok(n) = pct.parse::<i64>() {
                return (1, n, String::new());
            }
        }
        if let Ok(n) = value.parse::<i64>() {
            return (1, n, String::new());
        }
        (2, 0, value.to_string())
    }

    fn cmp_dims(a: &[(String, String)], b: &[(String, String)]) -> Ordering {
        let mut i = 0usize;
        while i < a.len() && i < b.len() {
            let (ak, av) = &a[i];
            let (bk, bv) = &b[i];
            match ak.cmp(bk) {
                Ordering::Equal => {}
                o => return o,
            }
            match value_rank(av).cmp(&value_rank(bv)) {
                Ordering::Equal => {}
                o => return o,
            }
            i += 1;
        }
        a.len().cmp(&b.len())
    }

    pub fn sort_results(results: &mut [BenchResult]) {
        results.sort_by(|a, b| {
            let dims_cmp = cmp_dims(&a.dimensions, &b.dimensions);
            if dims_cmp != Ordering::Equal {
                return dims_cmp;
            }
            version_rank(&a.version).cmp(&version_rank(&b.version))
        });
    }

    fn cmp_csv_rows(a: &str, b: &str, regular_dim_count: usize) -> Ordering {
        let ac: Vec<&str> = a.split(',').collect();
        let bc: Vec<&str> = b.split(',').collect();

        for i in 1..=regular_dim_count {
            let at = ac.get(i).copied().unwrap_or_default();
            let bt = bc.get(i).copied().unwrap_or_default();
            match value_rank(at).cmp(&value_rank(bt)) {
                Ordering::Equal => {}
                o => return o,
            }
        }

        let av = ac.first().copied().unwrap_or_default();
        let bv = bc.first().copied().unwrap_or_default();
        version_rank(av).cmp(&version_rank(bv))
    }

    /// Default warmup count, overridable via `BENCH_WARMUP` env var.
    pub fn warmup_count() -> usize {
        usize_env("BENCH_WARMUP").unwrap_or_else(default_warmup_count)
    }

    /// Default sample count, overridable via `BENCH_SAMPLES` env var.
    pub fn sample_count() -> usize {
        usize_env("BENCH_SAMPLES").unwrap_or_else(default_sample_count)
    }

    pub fn usize_list_env(var: &str, default: &[usize]) -> Vec<usize> {
        match std::env::var(var) {
            Ok(value) => {
                let parsed = value
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(|entry| {
                        entry.parse::<usize>().unwrap_or_else(|_| {
                            panic!(
                                "{var} must contain comma-separated positive integers, got {value}"
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                assert!(
                    !parsed.is_empty(),
                    "{var} must contain at least one positive integer"
                );
                parsed
            }
            Err(_) => default.to_vec(),
        }
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

    pub fn run_collect<T>(warmup: usize, samples: usize, mut f: impl FnMut() -> T) -> Vec<T> {
        for _ in 0..warmup {
            let _ = f();
        }

        (0..samples).map(|_| f()).collect()
    }

    /// Compute (mean, median) from raw ms times.
    pub fn stats(times: &[f64]) -> (f64, f64) {
        assert!(!times.is_empty());
        let n = times.len() as f64;
        let mean = times.iter().sum::<f64>() / n;

        let mut sorted = times.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let median = if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        };

        (mean, median)
    }

    /// Build a BenchResult from raw times and structured dimensions.
    pub fn make_result(
        version: &str,
        dims: &[(&str, &str)],
        times: &[f64],
        metrics: Option<CircuitMetrics>,
    ) -> BenchResult {
        let (mean, median) = stats(times);
        BenchResult {
            version: version.to_string(),
            dimensions: dims
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            mean_ms: mean,
            median_ms: median,
            samples: times.len(),
            metrics,
        }
    }

    /// Create a BenchResult with 0ms (for base operations that didn't exist).
    pub fn make_zero_result(version: &str, dims: &[(&str, &str)], samples: usize) -> BenchResult {
        BenchResult {
            version: version.to_string(),
            dimensions: dims
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            mean_ms: 0.0,
            median_ms: 0.0,
            samples,
            metrics: None,
        }
    }

    const META_DIM_KEYS: [&str; 5] = ["profile", "run_id", "timestamp", "git_rev", "host_label"];
    const BASE_METRIC_COLUMNS: [&str; 3] = ["mean_ms", "median_ms", "samples"];
    const CIRCUIT_METRIC_COLUMNS: [&str; 3] = ["constraints", "public_inputs", "witness_vars"];

    fn is_meta_dim_key(key: &str) -> bool {
        META_DIM_KEYS.contains(&key)
    }

    /// Collect all non-metadata dimension keys across results, preserving insertion order.
    fn regular_dimension_keys(results: &[BenchResult]) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut keys = Vec::new();
        for r in results {
            for (k, _) in &r.dimensions {
                if !is_meta_dim_key(k) && seen.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
        }
        keys
    }

    /// Metadata keys are emitted at the far right for readability.
    fn metadata_dimension_keys(results: &[BenchResult]) -> Vec<String> {
        META_DIM_KEYS
            .iter()
            .filter(|k| {
                results
                    .iter()
                    .any(|r| r.dimensions.iter().any(|(dk, _)| dk == **k))
            })
            .map(|k| (*k).to_string())
            .collect()
    }

    fn include_circuit_metric_columns(results: &[BenchResult]) -> bool {
        results.iter().any(|r| r.metrics.is_some())
    }

    fn append_metric_columns(header: &mut String, include_circuit_metrics: bool) {
        for k in BASE_METRIC_COLUMNS {
            header.push(',');
            header.push_str(k);
        }
        if include_circuit_metrics {
            for k in CIRCUIT_METRIC_COLUMNS {
                header.push(',');
                header.push_str(k);
            }
        }
    }

    fn append_metric_values(out: &mut String, r: &BenchResult, include_circuit_metrics: bool) {
        out.push_str(&format!(
            "{:.2},{:.2},{}",
            r.mean_ms, r.median_ms, r.samples
        ));
        if include_circuit_metrics {
            let (constraints, public_inputs, witness_vars) = match &r.metrics {
                Some(m) => (
                    m.constraints.to_string(),
                    m.public_inputs.to_string(),
                    m.witness_vars.to_string(),
                ),
                None => (String::new(), String::new(), String::new()),
            };
            out.push(',');
            out.push_str(&constraints);
            out.push(',');
            out.push_str(&public_inputs);
            out.push(',');
            out.push_str(&witness_vars);
        }
    }

    /// Write results to CSV. Creates parent directories if needed.
    pub fn write_csv(path: &Path, results: &[BenchResult]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("can create results directory");
        }

        let mut sorted = results.to_vec();
        sort_results(&mut sorted);
        let regular_dim_keys = regular_dimension_keys(&sorted);
        let meta_dim_keys = metadata_dimension_keys(&sorted);
        let include_circuit_metrics = include_circuit_metric_columns(&sorted);

        let mut csv = String::from("version");
        for k in &regular_dim_keys {
            csv.push(',');
            csv.push_str(k);
        }
        append_metric_columns(&mut csv, include_circuit_metrics);
        for k in &meta_dim_keys {
            csv.push(',');
            csv.push_str(k);
        }
        csv.push('\n');

        for r in &sorted {
            csv.push_str(&r.version);
            for k in &regular_dim_keys {
                csv.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    csv.push_str(v);
                }
            }
            csv.push(',');
            append_metric_values(&mut csv, r, include_circuit_metrics);
            for k in &meta_dim_keys {
                csv.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    csv.push_str(v);
                }
            }
            csv.push('\n');
        }
        fs::write(path, &csv).expect("can write CSV");
        eprintln!("CSV written to {}", path.display());
    }

    /// Get the bench version from `BENCH_VERSION` env var.
    ///
    /// `BENCH_VERSION` is required and must be one of: `base`, `dev`, `local`.
    pub fn bench_version() -> String {
        let value = std::env::var("BENCH_VERSION").unwrap_or_else(|_| {
            panic!("BENCH_VERSION is required (expected one of: base|dev|local)")
        });
        match value.as_str() {
            "base" | "dev" | "local" => value,
            other => panic!("unsupported BENCH_VERSION={other}, expected base|dev|local"),
        }
    }

    /// Append results to an existing CSV, removing any rows with matching versions
    /// (for idempotency). If the file doesn't exist, creates it.
    pub fn append_csv(path: &Path, new_results: &[BenchResult]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("can create results directory");
        }

        let mut sorted_new = new_results.to_vec();
        sort_results(&mut sorted_new);
        let new_versions: BTreeSet<&str> = sorted_new.iter().map(|r| r.version.as_str()).collect();

        let existing = fs::read_to_string(path).unwrap_or_default();
        let mut lines: Vec<&str> = existing.lines().collect();
        let header = if lines.is_empty() {
            None
        } else {
            Some(lines.remove(0))
        };

        let kept: Vec<&str> = lines
            .into_iter()
            .filter(|line| {
                let version = line.split(',').next().unwrap_or("");
                !new_versions.contains(version)
            })
            .collect();

        let regular_dim_keys = regular_dimension_keys(&sorted_new);
        let meta_dim_keys = metadata_dimension_keys(&sorted_new);
        let include_circuit_metrics = include_circuit_metric_columns(&sorted_new);
        let new_header = {
            let mut h = String::from("version");
            for k in &regular_dim_keys {
                h.push(',');
                h.push_str(k);
            }
            append_metric_columns(&mut h, include_circuit_metrics);
            for k in &meta_dim_keys {
                h.push(',');
                h.push_str(k);
            }
            h
        };

        // If the existing schema no longer matches, we cannot safely preserve
        // old rows (their column positions may differ). Rewrite with current rows.
        if let Some(old_header) = header {
            if old_header != new_header {
                eprintln!(
                    "CSV schema changed for {} (rewriting file with latest rows)",
                    path.display()
                );
                write_csv(path, &sorted_new);
                return;
            }
        }

        let mut csv = String::new();
        csv.push_str(&new_header);
        csv.push('\n');
        let mut rows: Vec<String> = Vec::new();
        for row in kept {
            rows.push(row.to_string());
        }
        for r in &sorted_new {
            let mut row = String::new();
            row.push_str(&r.version);
            for k in &regular_dim_keys {
                row.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    row.push_str(v);
                }
            }
            row.push(',');
            append_metric_values(&mut row, r, include_circuit_metrics);
            for k in &meta_dim_keys {
                row.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    row.push_str(v);
                }
            }
            rows.push(row);
        }

        rows.sort_by(|a, b| cmp_csv_rows(a, b, regular_dim_keys.len()));

        for row in rows {
            csv.push_str(&row);
            csv.push('\n');
        }

        fs::write(path, &csv).expect("can write CSV");
        eprintln!("CSV updated at {}", path.display());
    }

    /// Append results while upserting by `(version + scope_dims...)`.
    ///
    /// This is used for shared overview files (for example `flows.csv`) where
    /// different benches append disjoint categories into the same CSV.
    pub fn append_csv_scoped(path: &Path, new_results: &[BenchResult], scope_dims: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("can create results directory");
        }

        let mut sorted_new = new_results.to_vec();
        sort_results(&mut sorted_new);

        let new_scope_keys: BTreeSet<(String, Vec<String>)> = sorted_new
            .iter()
            .map(|r| {
                let scoped_vals = scope_dims
                    .iter()
                    .map(|k| {
                        r.dimensions
                            .iter()
                            .find(|(dk, _)| dk == *k)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                (r.version.clone(), scoped_vals)
            })
            .collect();

        let existing = fs::read_to_string(path).unwrap_or_default();
        let mut lines: Vec<&str> = existing.lines().collect();
        let header = if lines.is_empty() {
            None
        } else {
            Some(lines.remove(0))
        };

        let regular_dim_keys = regular_dimension_keys(&sorted_new);
        let meta_dim_keys = metadata_dimension_keys(&sorted_new);
        let include_circuit_metrics = include_circuit_metric_columns(&sorted_new);
        let new_header = {
            let mut h = String::from("version");
            for k in &regular_dim_keys {
                h.push(',');
                h.push_str(k);
            }
            append_metric_columns(&mut h, include_circuit_metrics);
            for k in &meta_dim_keys {
                h.push(',');
                h.push_str(k);
            }
            h
        };

        // If the existing schema no longer matches, rewrite with latest rows.
        let kept: Vec<&str> = if let Some(old_header) = header {
            if old_header != new_header {
                eprintln!(
                    "CSV schema changed for {} (rewriting file with latest rows)",
                    path.display()
                );
                write_csv(path, &sorted_new);
                return;
            }

            let cols: Vec<&str> = old_header.split(',').collect();
            let scope_indices: Vec<Option<usize>> = scope_dims
                .iter()
                .map(|k| cols.iter().position(|col| col == k))
                .collect();

            lines
                .into_iter()
                .filter(|line| {
                    let row_cols: Vec<&str> = line.split(',').collect();
                    let version = row_cols.first().copied().unwrap_or_default().to_string();
                    let scoped_vals = scope_indices
                        .iter()
                        .map(|idx| {
                            idx.and_then(|i| row_cols.get(i).copied())
                                .unwrap_or_default()
                                .to_string()
                        })
                        .collect::<Vec<_>>();
                    !new_scope_keys.contains(&(version, scoped_vals))
                })
                .collect()
        } else {
            Vec::new()
        };

        let mut csv = String::new();
        csv.push_str(&new_header);
        csv.push('\n');

        let mut rows: Vec<String> = Vec::new();
        for row in kept {
            rows.push(row.to_string());
        }
        for r in &sorted_new {
            let mut row = String::new();
            row.push_str(&r.version);
            for k in &regular_dim_keys {
                row.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    row.push_str(v);
                }
            }
            row.push(',');
            append_metric_values(&mut row, r, include_circuit_metrics);
            for k in &meta_dim_keys {
                row.push(',');
                if let Some((_, v)) = r.dimensions.iter().find(|(dk, _)| dk == k) {
                    row.push_str(v);
                }
            }
            rows.push(row);
        }

        rows.sort_by(|a, b| cmp_csv_rows(a, b, regular_dim_keys.len()));
        for row in rows {
            csv.push_str(&row);
            csv.push('\n');
        }

        fs::write(path, &csv).expect("can write CSV");
        eprintln!("CSV updated at {}", path.display());
    }

    /// Print a formatted comparison table to stdout.
    pub fn print_table(results: &[BenchResult]) {
        let label = |r: &BenchResult| -> String {
            r.dimensions
                .iter()
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join("/")
        };

        let dim_keys = regular_dimension_keys(results);

        let ver_w = 14;
        let op_w = results
            .iter()
            .map(|r| label(r).len())
            .max()
            .unwrap_or(10)
            .max(10);
        let num_w = 12;

        let dim_header = dim_keys.join("/");
        let header_label = if dim_header.is_empty() {
            "OPERATION".to_string()
        } else {
            dim_header.to_uppercase()
        };
        println!(
            "\n{:<ver_w$}  {:<op_w$}  {:>num_w$}  {:>num_w$}  {:>8}  {:>12}  {:>12}  {:>12}",
            "VERSION",
            header_label,
            "MEAN (ms)",
            "MEDIAN",
            "SAMPLES",
            "CONSTRAINTS",
            "PUB_INPUTS",
            "WITNESS",
        );
        println!("{}", "-".repeat(ver_w + op_w + num_w * 2 + 8 + 12 * 3 + 14));

        for r in results {
            let (constraints, public_inputs, witness_vars) = match &r.metrics {
                Some(m) => (
                    m.constraints.to_string(),
                    m.public_inputs.to_string(),
                    m.witness_vars.to_string(),
                ),
                None => ("-".to_string(), "-".to_string(), "-".to_string()),
            };
            println!(
                "{:<ver_w$}  {:<op_w$}  {:>num_w$.2}  {:>num_w$.2}  {:>8}  {:>12}  {:>12}  {:>12}",
                r.version,
                label(r),
                r.mean_ms,
                r.median_ms,
                r.samples,
                constraints,
                public_inputs,
                witness_vars,
            );
        }
        println!();
    }

    /// Print results as NDJSON to stdout (one JSON object per line).
    pub fn print_json(results: &[BenchResult]) {
        for r in results {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "version".to_string(),
                serde_json::Value::String(r.version.clone()),
            );
            for (k, v) in &r.dimensions {
                obj.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            obj.insert("mean_ms".to_string(), serde_json::Value::from(r.mean_ms));
            obj.insert(
                "median_ms".to_string(),
                serde_json::Value::from(r.median_ms),
            );
            obj.insert(
                "samples".to_string(),
                serde_json::Value::from(r.samples as u64),
            );
            match &r.metrics {
                Some(m) => {
                    obj.insert(
                        "constraints".to_string(),
                        serde_json::Value::from(m.constraints as u64),
                    );
                    obj.insert(
                        "public_inputs".to_string(),
                        serde_json::Value::from(m.public_inputs as u64),
                    );
                    obj.insert(
                        "witness_vars".to_string(),
                        serde_json::Value::from(m.witness_vars as u64),
                    );
                }
                None => {
                    obj.insert("constraints".to_string(), serde_json::Value::Null);
                    obj.insert("public_inputs".to_string(), serde_json::Value::Null);
                    obj.insert("witness_vars".to_string(), serde_json::Value::Null);
                }
            }
            match serde_json::to_string(&serde_json::Value::Object(obj)) {
                Ok(line) => println!("{line}"),
                Err(e) => {
                    eprintln!("failed to serialize benchmark result as JSON: {e:#}");
                    std::process::exit(1);
                }
            };
        }
    }

    /// Output results based on `BENCH_OUTPUT` env var: "json" for NDJSON, otherwise table.
    pub fn output_results(results: &[BenchResult]) {
        match std::env::var("BENCH_OUTPUT").as_deref() {
            Ok("json") => print_json(results),
            _ => print_table(results),
        }
    }

    /// Convert category-specific flow rows into a shared `flows.csv` shape.
    ///
    /// Output dimensions are:
    /// - `category`
    /// - `kpi` (derived from the row's non-metadata dimensions)
    /// - metadata keys (`profile`, `run_id`, `timestamp`, `git_rev`, `host_label`) when present
    pub fn to_flow_overview_rows(category: &str, rows: &[BenchResult]) -> Vec<BenchResult> {
        rows.iter()
            .map(|r| {
                let mut kpi_parts = Vec::new();
                let mut metadata_dims = Vec::new();
                for (k, v) in &r.dimensions {
                    if is_meta_dim_key(k) {
                        metadata_dims.push((k.clone(), v.clone()));
                    } else if !v.is_empty() {
                        kpi_parts.push(format!("{k}={v}"));
                    }
                }

                let mut dimensions = vec![
                    ("category".to_string(), category.to_string()),
                    (
                        "kpi".to_string(),
                        if kpi_parts.is_empty() {
                            "default".to_string()
                        } else {
                            kpi_parts.join("|")
                        },
                    ),
                ];
                dimensions.extend(metadata_dims);

                BenchResult {
                    version: r.version.clone(),
                    dimensions,
                    mean_ms: r.mean_ms,
                    median_ms: r.median_ms,
                    samples: r.samples,
                    metrics: r.metrics.clone(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::bench_runner::{
        append_csv, append_csv_scoped, make_zero_result, sort_results, write_csv, BenchResult,
    };

    #[test]
    fn sort_results_orders_versions_and_modes() {
        let mut rows: Vec<BenchResult> = vec![
            make_zero_result("local", &[("batch_size", "100"), ("mode", "parallel")], 1),
            make_zero_result("base", &[("batch_size", "100"), ("mode", "serial")], 1),
            make_zero_result("dev", &[("batch_size", "100"), ("mode", "parallel")], 1),
            make_zero_result("base", &[("batch_size", "100"), ("mode", "parallel")], 1),
            make_zero_result("dev", &[("batch_size", "100"), ("mode", "serial")], 1),
        ];

        sort_results(&mut rows);

        let ordered: Vec<(String, String)> = rows
            .iter()
            .map(|r| {
                let mode = r
                    .dimensions
                    .iter()
                    .find(|(k, _)| k == "mode")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                (r.version.clone(), mode)
            })
            .collect();

        assert_eq!(
            ordered,
            vec![
                ("base".to_string(), "serial".to_string()),
                ("dev".to_string(), "serial".to_string()),
                ("base".to_string(), "parallel".to_string()),
                ("dev".to_string(), "parallel".to_string()),
                ("local".to_string(), "parallel".to_string()),
            ]
        );
    }

    #[test]
    fn append_csv_upserts_and_keeps_stable_order() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "penumbra-bench-runner-test-{}-{}.csv",
            std::process::id(),
            std::thread::current().name().unwrap_or("t")
        ));

        // Initial rows.
        let initial = vec![
            make_zero_result("local", &[("batch_size", "100"), ("mode", "parallel")], 1),
            make_zero_result("dev", &[("batch_size", "100"), ("mode", "serial")], 1),
        ];
        write_csv(&path, &initial);

        // Upsert local + add base.
        let update = vec![
            make_zero_result("local", &[("batch_size", "100"), ("mode", "serial")], 2),
            make_zero_result("base", &[("batch_size", "100"), ("mode", "serial")], 2),
        ];
        append_csv(&path, &update);

        let csv = std::fs::read_to_string(&path).expect("can read csv");
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 4, "expected header + 3 rows, got {lines:?}");
        assert!(
            lines[1].starts_with("base,"),
            "base should be first data row"
        );
        assert!(
            lines[2].starts_with("dev,"),
            "dev should be second data row"
        );
        assert!(
            lines[3].starts_with("local,"),
            "local should be third data row"
        );

        // Ensure previous local row was replaced by the updated one (samples=2).
        assert!(
            lines[3].ends_with(",2"),
            "expected upserted local samples in row: {}",
            lines[3]
        );

        let _ = std::fs::remove_file(PathBuf::from(path));
    }

    #[test]
    fn append_csv_scoped_upserts_only_matching_scope() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "penumbra-bench-runner-scoped-test-{}-{}.csv",
            std::process::id(),
            std::thread::current().name().unwrap_or("t")
        ));

        let initial = vec![
            make_zero_result("local", &[("category", "client"), ("stage", "total")], 1),
            make_zero_result("local", &[("category", "scanner"), ("stage", "full")], 1),
            make_zero_result("dev", &[("category", "client"), ("stage", "total")], 1),
        ];
        write_csv(&path, &initial);

        let update = vec![make_zero_result(
            "local",
            &[("category", "client"), ("stage", "total")],
            2,
        )];
        append_csv_scoped(&path, &update, &["category"]);

        let csv = std::fs::read_to_string(&path).expect("can read csv");
        assert!(
            csv.contains("local,scanner,full,0.00,0.00,1"),
            "expected unrelated local/scanner row to remain"
        );
        assert!(
            csv.contains("local,client,total,0.00,0.00,2"),
            "expected local/client row to be replaced"
        );
        assert!(
            csv.contains("dev,client,total,0.00,0.00,1"),
            "expected dev/client row to remain"
        );

        let _ = std::fs::remove_file(PathBuf::from(path));
    }
}
