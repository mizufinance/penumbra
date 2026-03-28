use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::tps::sender::SubmitMode;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointKind {
    #[default]
    TendermintProxy,
    NodeService,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TpsProfile {
    Regression,
    Ceiling,
    Soak,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StabilityConfig {
    pub max_reject_rate_pct: f64,
    pub max_p95_latency_ms: f64,
    pub max_backlog_growth_pct: f64,
    #[serde(default = "default_min_steady_commits")]
    pub min_steady_commits: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BurstProfile {
    pub burst_tx_count: u64,
    pub burst_duration_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub name: String,
    pub corpus_dir: PathBuf,
    pub offered_tps: Vec<u64>,
    pub repeats: u32,
    #[serde(default = "default_repeat_start")]
    pub repeat_start: u32,
    pub warmup_blocks: u64,
    pub steady_blocks: u64,
    pub submit_workers: usize,
    pub max_inflight: usize,
    #[serde(default)]
    pub submit_mode: SubmitMode,
    #[serde(default = "default_pacer_tick_ms")]
    pub pacer_tick_ms: u64,
    #[serde(default)]
    pub disable_pacer: bool,
    #[serde(default)]
    pub burst_profile: Option<BurstProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TpsConfig {
    pub label: String,
    pub pd_endpoints: Vec<String>,
    pub observer_endpoint: String,
    #[serde(default)]
    pub endpoint_kind: EndpointKind,
    #[serde(default)]
    pub metrics_endpoint: Option<String>,
    #[serde(default)]
    pub comet_metrics_endpoint: Option<String>,
    pub profile: TpsProfile,
    pub target_block_time_ms: u64,
    #[serde(default)]
    pub mempool_checktx_concurrency: Option<usize>,
    pub scenarios: Vec<ScenarioConfig>,
    pub stability: StabilityConfig,
}

#[derive(Clone, Debug)]
pub struct RunCase {
    pub scenario: ScenarioConfig,
    pub offered_tps: u64,
    pub repeat: u32,
}

impl TpsConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed reading TPS config {}", path.display()))?;
        let mut cfg: TpsConfig =
            serde_yaml::from_slice(&bytes).context("failed parsing YAML config")?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        cfg.resolve_paths(base_dir);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.label.trim().is_empty(), "label must not be empty");
        anyhow::ensure!(
            !self.pd_endpoints.is_empty(),
            "pd_endpoints must include at least one endpoint"
        );
        anyhow::ensure!(
            self.observer_endpoint.starts_with("http://")
                || self.observer_endpoint.starts_with("https://"),
            "observer_endpoint must be http(s)"
        );
        anyhow::ensure!(
            self.target_block_time_ms > 0,
            "target_block_time_ms must be > 0"
        );
        if let Some(metrics_endpoint) = &self.metrics_endpoint {
            anyhow::ensure!(
                metrics_endpoint.starts_with("http://") || metrics_endpoint.starts_with("https://"),
                "metrics_endpoint must be http(s)"
            );
        }
        if let Some(comet_metrics_endpoint) = &self.comet_metrics_endpoint {
            anyhow::ensure!(
                comet_metrics_endpoint.starts_with("http://")
                    || comet_metrics_endpoint.starts_with("https://"),
                "comet_metrics_endpoint must be http(s)"
            );
        }
        if let Some(mempool_checktx_concurrency) = self.mempool_checktx_concurrency {
            anyhow::ensure!(
                mempool_checktx_concurrency > 0,
                "mempool_checktx_concurrency must be > 0"
            );
        }
        anyhow::ensure!(!self.scenarios.is_empty(), "scenarios must not be empty");
        anyhow::ensure!(
            self.stability.max_reject_rate_pct >= 0.0,
            "max_reject_rate_pct must be >= 0"
        );
        anyhow::ensure!(
            self.stability.max_p95_latency_ms >= 0.0,
            "max_p95_latency_ms must be >= 0"
        );
        anyhow::ensure!(
            self.stability.max_backlog_growth_pct >= 0.0,
            "max_backlog_growth_pct must be >= 0"
        );

        for endpoint in &self.pd_endpoints {
            anyhow::ensure!(
                endpoint.starts_with("http://") || endpoint.starts_with("https://"),
                "pd endpoint must be http(s): {endpoint}"
            );
        }

        for scenario in &self.scenarios {
            anyhow::ensure!(
                scenario.name == "regulated" || scenario.name == "unregulated",
                "scenario.name must be one of regulated|unregulated (got {})",
                scenario.name
            );
            anyhow::ensure!(
                !scenario.offered_tps.is_empty(),
                "scenario {} must define offered_tps",
                scenario.name
            );
            anyhow::ensure!(
                scenario.repeats > 0,
                "scenario {} repeats must be > 0",
                scenario.name
            );
            anyhow::ensure!(
                scenario.repeat_start > 0,
                "scenario {} repeat_start must be > 0",
                scenario.name
            );
            anyhow::ensure!(
                scenario.steady_blocks > 0,
                "scenario {} steady_blocks must be > 0",
                scenario.name
            );
            anyhow::ensure!(
                scenario.submit_workers > 0,
                "scenario {} submit_workers must be > 0",
                scenario.name
            );
            anyhow::ensure!(
                scenario.max_inflight >= scenario.submit_workers,
                "scenario {} max_inflight must be >= submit_workers",
                scenario.name
            );
            anyhow::ensure!(
                scenario.disable_pacer || scenario.pacer_tick_ms > 0,
                "scenario {} pacer_tick_ms must be > 0 unless pacing is disabled",
                scenario.name
            );
            if let Some(burst_profile) = &scenario.burst_profile {
                anyhow::ensure!(
                    burst_profile.burst_tx_count > 0,
                    "scenario {} burst_tx_count must be > 0",
                    scenario.name
                );
                anyhow::ensure!(
                    burst_profile.burst_duration_ms > 0,
                    "scenario {} burst_duration_ms must be > 0",
                    scenario.name
                );
            }
            anyhow::ensure!(
                scenario.corpus_dir.exists(),
                "scenario {} corpus_dir does not exist: {}",
                scenario.name,
                scenario.corpus_dir.display()
            );
            for &offered_tps in &scenario.offered_tps {
                anyhow::ensure!(
                    offered_tps > 0,
                    "scenario {} offered_tps entries must be > 0",
                    scenario.name
                );
            }
        }

        Ok(())
    }

    pub fn expand_cases(&self) -> Vec<RunCase> {
        let mut out = Vec::new();
        for scenario in &self.scenarios {
            for &offered_tps in &scenario.offered_tps {
                for repeat in scenario.repeat_start..(scenario.repeat_start + scenario.repeats) {
                    out.push(RunCase {
                        scenario: scenario.clone(),
                        offered_tps,
                        repeat,
                    });
                }
            }
        }
        out
    }

    fn resolve_paths(&mut self, base_dir: &Path) {
        for scenario in &mut self.scenarios {
            if scenario.corpus_dir.is_relative() {
                scenario.corpus_dir = base_dir.join(&scenario.corpus_dir);
            }
        }
    }
}

fn default_min_steady_commits() -> u64 {
    1
}

fn default_pacer_tick_ms() -> u64 {
    50
}

fn default_repeat_start() -> u32 {
    1
}
