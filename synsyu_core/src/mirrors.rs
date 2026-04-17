/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::mirrors
  Etiquette: Synavera Script Etiquette - Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Discover, probe, and rank pacman mirror candidates for the
    Bash orchestrator to consume during bounded repo failover.

  Security / Safety Notes:
    This module does not install packages and does not edit pacman
    configuration. It only reads configured mirror sources and performs
    bounded HTTP probes.
============================================================*/

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{SecondsFormat, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::MirrorConfig;
use crate::logger::Logger;

const UNKNOWN_FRESHNESS_SCORE_PENALTY: u64 = 1_000_000;

/// Structured mirror subsystem state embedded in manifests and status output.
#[derive(Debug, Serialize, Clone)]
pub struct MirrorState {
    pub enabled: bool,
    pub status: String,
    pub generated_at: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub pacman_conf_path: String,
    pub probe_enabled: bool,
    pub timeout_seconds: u64,
    pub max_candidates: usize,
    pub max_failovers: usize,
    pub retry_delay_seconds: u64,
    pub max_sync_age_hours: u64,
    pub cache_path: String,
    pub cache_used: bool,
    pub cache_ttl_hours: u64,
    pub candidate_count: usize,
    pub usable_count: usize,
    pub candidates: Vec<MirrorCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One mirror candidate with enough state for explainable orchestration.
#[derive(Debug, Serialize, Clone)]
pub struct MirrorCandidate {
    pub rank: usize,
    pub server: String,
    pub probe_url: String,
    pub status: String,
    pub outcome: String,
    pub freshness: String,
    pub usable: bool,
    pub score: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastsync_age_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MirrorProbeCache {
    version: u8,
    generated_at: String,
    entries: BTreeMap<String, CachedMirrorOutcome>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CachedMirrorOutcome {
    server: String,
    outcome: String,
    freshness: String,
    usable: bool,
    score: u64,
    latency_ms: Option<u64>,
    lastsync_age_seconds: Option<u64>,
    observed_at_epoch: u64,
}

impl MirrorState {
    fn from_config(config: &MirrorConfig, status: &str, reason: Option<String>) -> Self {
        Self {
            enabled: config.enabled,
            status: status.to_string(),
            generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            source: if config.servers.is_empty() {
                "mirrorlist".to_string()
            } else {
                "config.mirrors.servers".to_string()
            },
            source_path: if config.servers.is_empty() {
                Some(config.mirrorlist_path.clone())
            } else {
                None
            },
            pacman_conf_path: config.pacman_conf_path.clone(),
            probe_enabled: config.probe,
            timeout_seconds: config.probe_timeout_seconds,
            max_candidates: config.max_candidates,
            max_failovers: config.max_failovers,
            retry_delay_seconds: config.retry_delay_seconds,
            max_sync_age_hours: config.max_sync_age_hours,
            cache_path: mirror_cache_path(config).display().to_string(),
            cache_used: false,
            cache_ttl_hours: config.cache_ttl_hours,
            candidate_count: 0,
            usable_count: 0,
            candidates: Vec::new(),
            reason,
        }
    }
}

/// Collect mirror state using configured bounds. Failures are recorded in state
/// and logs rather than aborting manifest generation.
pub async fn collect_mirror_state(
    config: &MirrorConfig,
    logger: &Logger,
    offline_or_repo_disabled: bool,
) -> MirrorState {
    if !config.enabled {
        logger.info("MIRROR", "Mirror failover disabled by configuration");
        return MirrorState::from_config(config, "disabled", Some("disabled by config".into()));
    }
    if offline_or_repo_disabled {
        logger.info(
            "MIRROR",
            "Mirror probing skipped because repo network operations are disabled",
        );
        return MirrorState::from_config(
            config,
            "skipped",
            Some("offline mode or repo operations disabled".into()),
        );
    }

    let servers = match discover_mirror_servers(config) {
        Ok(servers) => servers,
        Err(reason) => {
            logger.warn("MIRROR", format!("Mirror discovery failed: {reason}"));
            return MirrorState::from_config(config, "error", Some(reason));
        }
    };

    let cache_path = mirror_cache_path(config);
    let cache = read_probe_cache(&cache_path, config.cache_ttl_hours);

    let mut state = MirrorState::from_config(config, "ready", None);
    state.cache_used = !cache.entries.is_empty();
    state.candidate_count = servers.len();
    if servers.is_empty() {
        state.status = "empty".to_string();
        state.reason = Some("no active mirror candidates discovered".to_string());
        logger.warn("MIRROR", "No active mirror candidates discovered");
        return state;
    }

    let arch = pacman_arch();
    let ordered_servers = order_servers_with_cache(servers, &cache);
    let limited: Vec<String> = ordered_servers
        .into_iter()
        .take(config.max_candidates)
        .collect();
    let candidates = if config.probe {
        probe_candidates(&limited, config, &arch, logger).await
    } else {
        limited
            .iter()
            .enumerate()
            .map(|(idx, server)| cached_or_unprobed_candidate(idx, server, &arch, &cache))
            .collect()
    };

    state.candidates = rank_candidates(candidates);
    state.usable_count = state.candidates.iter().filter(|c| c.usable).count();
    state.candidate_count = state.candidates.len();

    if state.usable_count == 0 {
        state.status = "exhausted".to_string();
        state.reason = Some("no usable mirror candidates after probing".to_string());
        logger.warn(
            "MIRROR",
            "Mirror probing found no usable candidates; Bash layer will use safe fallback",
        );
    } else {
        logger.info(
            "MIRROR",
            format!(
                "Mirror candidates ready: usable={} total={}",
                state.usable_count, state.candidate_count
            ),
        );
    }

    if config.probe {
        write_probe_cache(&cache_path, &state.candidates, logger);
    }

    state
}

fn discover_mirror_servers(config: &MirrorConfig) -> std::result::Result<Vec<String>, String> {
    if !config.servers.is_empty() {
        return Ok(dedupe_servers(config.servers.clone()));
    }

    let contents = fs::read_to_string(&config.mirrorlist_path)
        .map_err(|err| format!("failed to read {}: {err}", config.mirrorlist_path))?;
    Ok(parse_mirrorlist_contents(&contents))
}

fn dedupe_servers(servers: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for server in servers {
        let trimmed = server.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn parse_mirrorlist_contents(contents: &str) -> Vec<String> {
    let mut servers = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("Server") {
            continue;
        }
        let mut server = value.trim().to_string();
        if let Some((before_comment, _)) = server.split_once(" #") {
            server = before_comment.trim().to_string();
        }
        servers.push(server);
    }
    dedupe_servers(servers)
}

async fn probe_candidates(
    servers: &[String],
    config: &MirrorConfig,
    arch: &str,
    logger: &Logger,
) -> Vec<MirrorCandidate> {
    let timeout = Duration::from_secs(config.probe_timeout_seconds.max(1));
    let client = match Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(err) => {
            logger.warn("MIRROR", format!("Unable to build HTTP client: {err}"));
            return servers
                .iter()
                .enumerate()
                .map(|(idx, server)| {
                    failed_candidate_with_outcome(
                        idx,
                        server,
                        arch,
                        "probe_failed",
                        "http client error",
                    )
                })
                .collect();
        }
    };

    let mut candidates = Vec::new();
    for (idx, server) in servers.iter().enumerate() {
        candidates.push(probe_one(&client, idx, server, config, arch, logger).await);
    }
    candidates
}

async fn probe_one(
    client: &Client,
    idx: usize,
    server: &str,
    config: &MirrorConfig,
    arch: &str,
    logger: &Logger,
) -> MirrorCandidate {
    let probe_url = repo_probe_url(server, "core", arch);
    let start = Instant::now();
    let response = client.head(&probe_url).send().await;
    let elapsed_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;

    let mut candidate = match response {
        Ok(resp) if resp.status().is_success() => MirrorCandidate {
            rank: idx + 1,
            server: server.to_string(),
            probe_url,
            status: "ready".to_string(),
            outcome: "ready".to_string(),
            freshness: "unknown".to_string(),
            usable: true,
            score: UNKNOWN_FRESHNESS_SCORE_PENALTY
                .saturating_add(elapsed_ms)
                .saturating_add(idx as u64),
            latency_ms: Some(elapsed_ms),
            lastsync_age_seconds: None,
            reason: Some("lastsync unavailable; freshness unknown".to_string()),
        },
        Ok(resp) => {
            let reason = format!("HTTP probe returned {}", resp.status());
            logger.debug("MIRROR", format!("{server}: {reason}"));
            return failed_candidate_with_outcome(idx, server, arch, "http_error", &reason);
        }
        Err(err) => {
            let reason = err.to_string();
            let outcome = if err.is_timeout() {
                "timeout"
            } else if err.is_connect() {
                "connect_failed"
            } else {
                "probe_failed"
            };
            logger.debug("MIRROR", format!("{server}: {reason}"));
            return failed_candidate_with_outcome(idx, server, arch, outcome, &reason);
        }
    };

    if let Some(age) = probe_lastsync_age(client, server, arch).await {
        candidate.lastsync_age_seconds = Some(age);
        let max_age = config.max_sync_age_hours.saturating_mul(3600);
        if max_age > 0 && age > max_age {
            candidate.status = "stale".to_string();
            candidate.outcome = "stale".to_string();
            candidate.freshness = "stale".to_string();
            candidate.usable = false;
            candidate.score = u64::MAX.saturating_sub(1);
            candidate.reason = Some(format!(
                "lastsync age {}s exceeds configured limit {}s",
                age, max_age
            ));
        } else {
            candidate.freshness = "fresh".to_string();
            candidate.score = elapsed_ms
                .saturating_add(age / 60)
                .saturating_add(idx as u64);
            candidate.reason = None;
        }
    }

    logger.debug(
        "MIRROR",
        format!(
            "Probe {} status={} latency={}ms",
            candidate.server,
            candidate.status,
            candidate.latency_ms.unwrap_or(0)
        ),
    );
    candidate
}

async fn probe_lastsync_age(client: &Client, server: &str, arch: &str) -> Option<u64> {
    let url = lastsync_url(server, arch)?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().await.ok()?;
    let stamp = text.trim().parse::<u64>().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    now.checked_sub(stamp)
}

fn failed_candidate_with_outcome(
    idx: usize,
    server: &str,
    arch: &str,
    outcome: &str,
    reason: &str,
) -> MirrorCandidate {
    MirrorCandidate {
        rank: idx + 1,
        server: server.to_string(),
        probe_url: repo_probe_url(server, "core", arch),
        status: "failed".to_string(),
        outcome: outcome.to_string(),
        freshness: "unknown".to_string(),
        usable: false,
        score: u64::MAX,
        latency_ms: None,
        lastsync_age_seconds: None,
        reason: Some(reason.to_string()),
    }
}

fn order_servers_with_cache(mut servers: Vec<String>, cache: &MirrorProbeCache) -> Vec<String> {
    servers.sort_by(|a, b| {
        cached_server_rank(a, cache)
            .cmp(&cached_server_rank(b, cache))
            .then_with(|| a.cmp(b))
    });
    servers
}

fn cached_server_rank(server: &str, cache: &MirrorProbeCache) -> (u8, u8, u64) {
    let Some(entry) = cache.entries.get(server) else {
        return (1, 1, UNKNOWN_FRESHNESS_SCORE_PENALTY);
    };
    let usability = if entry.usable { 0 } else { 2 };
    let freshness = freshness_rank(&entry.freshness);
    (usability, freshness, entry.score)
}

fn cached_or_unprobed_candidate(
    idx: usize,
    server: &str,
    arch: &str,
    cache: &MirrorProbeCache,
) -> MirrorCandidate {
    if let Some(entry) = cache.entries.get(server) {
        if entry.usable {
            return MirrorCandidate {
                rank: idx + 1,
                server: server.to_string(),
                probe_url: repo_probe_url(server, "core", arch),
                status: "cached".to_string(),
                outcome: format!("cached_{}", entry.outcome),
                freshness: entry.freshness.clone(),
                usable: true,
                score: entry.score,
                latency_ms: entry.latency_ms,
                lastsync_age_seconds: entry.lastsync_age_seconds,
                reason: Some("using last-known probe outcome; probing disabled".to_string()),
            };
        }
    }

    MirrorCandidate {
        rank: idx + 1,
        server: server.to_string(),
        probe_url: repo_probe_url(server, "core", arch),
        status: "unprobed".to_string(),
        outcome: "unprobed".to_string(),
        freshness: "unknown".to_string(),
        usable: true,
        score: UNKNOWN_FRESHNESS_SCORE_PENALTY.saturating_add(idx as u64),
        latency_ms: None,
        lastsync_age_seconds: None,
        reason: Some("probing disabled".to_string()),
    }
}

fn rank_candidates(mut candidates: Vec<MirrorCandidate>) -> Vec<MirrorCandidate> {
    candidates.sort_by(|a, b| {
        b.usable
            .cmp(&a.usable)
            .then_with(|| freshness_rank(&a.freshness).cmp(&freshness_rank(&b.freshness)))
            .then_with(|| a.score.cmp(&b.score))
            .then_with(|| a.server.cmp(&b.server))
    });
    for (idx, candidate) in candidates.iter_mut().enumerate() {
        candidate.rank = idx + 1;
    }
    candidates
}

fn freshness_rank(value: &str) -> u8 {
    match value {
        "fresh" => 0,
        "unknown" => 1,
        "stale" => 2,
        _ => 3,
    }
}

fn read_probe_cache(path: &PathBuf, ttl_hours: u64) -> MirrorProbeCache {
    let Ok(contents) = fs::read_to_string(path) else {
        return MirrorProbeCache::default();
    };
    let Ok(mut cache) = serde_json::from_str::<MirrorProbeCache>(&contents) else {
        return MirrorProbeCache::default();
    };
    if ttl_hours == 0 {
        cache.entries.clear();
        return cache;
    }
    let max_age = ttl_hours.saturating_mul(3600);
    let now = epoch_seconds();
    cache
        .entries
        .retain(|_, entry| now.saturating_sub(entry.observed_at_epoch) <= max_age);
    cache
}

fn write_probe_cache(path: &PathBuf, candidates: &[MirrorCandidate], logger: &Logger) {
    let now = epoch_seconds();
    let mut entries = BTreeMap::new();
    for candidate in candidates {
        entries.insert(
            candidate.server.clone(),
            CachedMirrorOutcome {
                server: candidate.server.clone(),
                outcome: candidate
                    .outcome
                    .strip_prefix("cached_")
                    .unwrap_or(&candidate.outcome)
                    .to_string(),
                freshness: candidate.freshness.clone(),
                usable: candidate.usable,
                score: candidate.score,
                latency_ms: candidate.latency_ms,
                lastsync_age_seconds: candidate.lastsync_age_seconds,
                observed_at_epoch: now,
            },
        );
    }
    let cache = MirrorProbeCache {
        version: 1,
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        entries,
    };
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            logger.warn(
                "MIRROR",
                format!("Unable to create mirror probe cache directory: {err}"),
            );
            return;
        }
    }
    match serde_json::to_string_pretty(&cache) {
        Ok(json) => {
            if let Err(err) = fs::write(path, json) {
                logger.warn(
                    "MIRROR",
                    format!("Unable to write mirror probe cache: {err}"),
                );
            }
        }
        Err(err) => logger.warn(
            "MIRROR",
            format!("Unable to serialize mirror probe cache: {err}"),
        ),
    }
}

fn mirror_cache_path(config: &MirrorConfig) -> PathBuf {
    if let Some(path) = &config.cache_path {
        return expand_tilde(path);
    }
    dirs::cache_dir()
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache")
        })
        .join("syn-syu")
        .join("mirror-probes.json")
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(rest);
    }
    PathBuf::from(path)
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn repo_probe_url(server: &str, repo: &str, arch: &str) -> String {
    let replaced = server
        .trim()
        .trim_end_matches('/')
        .replace("$repo", repo)
        .replace("$arch", arch);
    format!("{replaced}/{repo}.db")
}

fn lastsync_url(server: &str, arch: &str) -> Option<String> {
    let replaced = server
        .trim()
        .trim_end_matches('/')
        .replace("$repo", "core")
        .replace("$arch", arch);

    if let Some((base, _)) = replaced.split_once("/core/os/") {
        return Some(format!("{}/lastsync", base.trim_end_matches('/')));
    }
    replaced
        .rsplit_once('/')
        .map(|(base, _)| format!("{}/lastsync", base.trim_end_matches('/')))
}

fn pacman_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64".to_string(),
        "aarch64" => "aarch64".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_servers_and_ignores_comments() {
        let input = r#"
#Server = https://commented.example/$repo/os/$arch
Server = https://one.example/$repo/os/$arch
  Server = http://two.example/archlinux/$repo/os/$arch # nearby
Server = file:///not-supported/$repo/os/$arch
Server = https://one.example/$repo/os/$arch
"#;
        let parsed = parse_mirrorlist_contents(input);
        assert_eq!(
            parsed,
            vec![
                "https://one.example/$repo/os/$arch".to_string(),
                "http://two.example/archlinux/$repo/os/$arch".to_string(),
            ]
        );
    }

    #[test]
    fn builds_core_db_probe_url() {
        let url = repo_probe_url(
            "https://mirror.example/arch/$repo/os/$arch",
            "core",
            "x86_64",
        );
        assert_eq!(url, "https://mirror.example/arch/core/os/x86_64/core.db");
    }

    #[test]
    fn ranks_usable_low_latency_first() {
        let candidates = vec![
            MirrorCandidate {
                rank: 1,
                server: "https://slow.example/$repo/os/$arch".into(),
                probe_url: "".into(),
                status: "ready".into(),
                outcome: "ready".into(),
                freshness: "unknown".into(),
                usable: true,
                score: UNKNOWN_FRESHNESS_SCORE_PENALTY + 200,
                latency_ms: Some(200),
                lastsync_age_seconds: None,
                reason: None,
            },
            MirrorCandidate {
                rank: 2,
                server: "https://failed.example/$repo/os/$arch".into(),
                probe_url: "".into(),
                status: "failed".into(),
                outcome: "timeout".into(),
                freshness: "unknown".into(),
                usable: false,
                score: u64::MAX,
                latency_ms: None,
                lastsync_age_seconds: None,
                reason: Some("timeout".into()),
            },
            MirrorCandidate {
                rank: 3,
                server: "https://fast.example/$repo/os/$arch".into(),
                probe_url: "".into(),
                status: "ready".into(),
                outcome: "ready".into(),
                freshness: "unknown".into(),
                usable: true,
                score: UNKNOWN_FRESHNESS_SCORE_PENALTY + 10,
                latency_ms: Some(10),
                lastsync_age_seconds: None,
                reason: None,
            },
        ];

        let ranked = rank_candidates(candidates);
        assert_eq!(ranked[0].server, "https://fast.example/$repo/os/$arch");
        assert_eq!(ranked[0].rank, 1);
        assert_eq!(ranked[2].status, "failed");
    }

    #[test]
    fn ranks_known_fresh_before_unknown_even_when_slower() {
        let candidates = vec![
            MirrorCandidate {
                rank: 1,
                server: "https://unknown-fast.example/$repo/os/$arch".into(),
                probe_url: "".into(),
                status: "ready".into(),
                outcome: "ready".into(),
                freshness: "unknown".into(),
                usable: true,
                score: UNKNOWN_FRESHNESS_SCORE_PENALTY + 5,
                latency_ms: Some(5),
                lastsync_age_seconds: None,
                reason: Some("lastsync unavailable; freshness unknown".into()),
            },
            MirrorCandidate {
                rank: 2,
                server: "https://fresh-slower.example/$repo/os/$arch".into(),
                probe_url: "".into(),
                status: "ready".into(),
                outcome: "ready".into(),
                freshness: "fresh".into(),
                usable: true,
                score: 500,
                latency_ms: Some(400),
                lastsync_age_seconds: Some(6_000),
                reason: None,
            },
        ];

        let ranked = rank_candidates(candidates);
        assert_eq!(
            ranked[0].server,
            "https://fresh-slower.example/$repo/os/$arch"
        );
    }

    #[test]
    fn cached_outcomes_promote_previous_good_mirror_before_probe_limit() {
        let mut cache = MirrorProbeCache::default();
        cache.entries.insert(
            "https://good.example/$repo/os/$arch".into(),
            CachedMirrorOutcome {
                server: "https://good.example/$repo/os/$arch".into(),
                outcome: "ready".into(),
                freshness: "fresh".into(),
                usable: true,
                score: 20,
                latency_ms: Some(20),
                lastsync_age_seconds: Some(60),
                observed_at_epoch: epoch_seconds(),
            },
        );

        let ordered = order_servers_with_cache(
            vec![
                "https://unknown.example/$repo/os/$arch".into(),
                "https://good.example/$repo/os/$arch".into(),
            ],
            &cache,
        );

        assert_eq!(ordered[0], "https://good.example/$repo/os/$arch");
    }
}
