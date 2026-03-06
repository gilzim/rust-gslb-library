use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

#[derive(Clone)]
pub struct EndpointStats {
    pub url: String,
    pub weight: usize,
    pub latency_ms: u64,
    pub is_healthy: bool,
}

#[derive(Clone)]
pub struct CoreResolver {
    pub stats: Arc<RwLock<Vec<EndpointStats>>>,
    pub active_pool: Arc<RwLock<Vec<String>>>,
    pub counter: Arc<AtomicUsize>,
    pub interval_secs: u64,
    pub latency_margin_ms: u64,
}

impl CoreResolver {
    pub fn new(config: Vec<(String, usize)>, interval_secs: u64, latency_margin_ms: u64) -> Self {
        let mut stats = Vec::new();
        let mut initial_pool = Vec::new();

        for (raw_url, weight) in config {
            // New logic: Automatically default to tcp:// if no protocol is provided
            let url = if raw_url.starts_with("http://") || raw_url.starts_with("https://") || raw_url.starts_with("tcp://") {
                raw_url
            } else {
                format!("tcp://{}", raw_url)
            };

            for _ in 0..weight {
                initial_pool.push(url.clone());
            }
            stats.push(EndpointStats {
                url, weight, latency_ms: u64::MAX, is_healthy: true,
            });
        }
        if initial_pool.is_empty() { initial_pool.push(String::new()); }

        Self {
            stats: Arc::new(RwLock::new(stats)),
            active_pool: Arc::new(RwLock::new(initial_pool)),
            counter: Arc::new(AtomicUsize::new(0)),
            interval_secs,
            latency_margin_ms,
        }
    }

    pub fn get_endpoint(&self) -> String {
        let pool = self.active_pool.read().unwrap();
        if pool.is_empty() { return String::new(); }
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        pool[count % pool.len()].clone()
    }

    /// Helper to extract host:port from a URL string
    fn extract_host_port(url: &str) -> String {
        let stripped = if let Some(s) = url.strip_prefix("tcp://") {
            s
        } else if let Some(s) = url.strip_prefix("http://") {
            s
        } else if let Some(s) = url.strip_prefix("https://") {
            s
        } else {
            url
        };
        stripped.split('/').next().unwrap_or(stripped).to_string()
    }

    pub fn get_host_port(&self) -> String {
        let endpoint = self.get_endpoint();
        if endpoint.is_empty() { return String::new(); }
        Self::extract_host_port(&endpoint)
    }

    pub fn report_failure(&self, identifier: &str) {
        let mut stats = self.stats.write().unwrap();
        let mut min_latency = u64::MAX;
        let mut changed = false;

        for endpoint in stats.iter_mut() {
            let is_match = endpoint.url == identifier || Self::extract_host_port(&endpoint.url) == identifier;
            if is_match {
                endpoint.is_healthy = false;
                endpoint.latency_ms = u64::MAX;
                changed = true;
            } else if endpoint.is_healthy && endpoint.latency_ms < min_latency {
                min_latency = endpoint.latency_ms;
            }
        }
        if changed { drop(stats); self.rebuild_pool(min_latency); }
    }

    pub fn get_report(&self) -> HashMap<String, u64> {
        self.stats.read().unwrap().iter().filter(|e| e.is_healthy)
            .map(|e| (e.url.clone(), e.latency_ms))
            .collect()
    }

    pub fn rebuild_pool(&self, min_latency: u64) {
        let stats = self.stats.read().unwrap();
        let mut new_pool = Vec::new();
        let threshold = min_latency.saturating_add(self.latency_margin_ms);

        for e in stats.iter() {
            if e.is_healthy && e.latency_ms <= threshold {
                for _ in 0..e.weight { new_pool.push(e.url.clone()); }
            }
        }
        if new_pool.is_empty() {
            for e in stats.iter() { new_pool.push(e.url.clone()); }
        }
        *self.active_pool.write().unwrap() = new_pool;
    }
}