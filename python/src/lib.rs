use pyo3::prelude::*;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio::net::TcpStream;
use reqwest::Client;

#[derive(FromPyObject)]
pub enum EndpointConfig {
    Urls(Vec<String>),
    WeightedUrls(Vec<(String, usize)>),
}

#[derive(Clone)]
struct EndpointStats {
    url: String,
    weight: usize,
    latency_ms: u64,
    is_healthy: bool,
}

#[pyclass]
pub struct GslbResolver {
    stats: Arc<RwLock<Vec<EndpointStats>>>,
    active_pool: Arc<RwLock<Vec<String>>>,
    counter: Arc<AtomicUsize>,
    interval_secs: u64,
    latency_margin_ms: u64,
}

#[pymethods]
impl GslbResolver {
    #[new]
    #[pyo3(signature = (nodes, interval_secs=5, latency_margin_ms=20))]
    fn new(nodes: EndpointConfig, interval_secs: u64, latency_margin_ms: u64) -> Self {
        let mut stats = Vec::new();
        let mut initial_pool = Vec::new();

        let config_iter: Vec<(String, usize)> = match nodes {
            EndpointConfig::Urls(urls) => urls.into_iter().map(|u| (u, 1)).collect(),
            EndpointConfig::WeightedUrls(weighted) => weighted,
        };

        for (raw_url, weight) in config_iter {
            // Automatically default to tcp:// if no protocol is provided
            let url = if raw_url.starts_with("http://") || raw_url.starts_with("https://") || raw_url.starts_with("tcp://") {
                raw_url
            } else {
                format!("tcp://{}", raw_url)
            };

            for _ in 0..weight {
                initial_pool.push(url.clone());
            }
            stats.push(EndpointStats {
                url,
                weight,
                latency_ms: u64::MAX,
                is_healthy: true,
            });
        }

        if initial_pool.is_empty() { initial_pool.push(String::new()); }

        GslbResolver {
            stats: Arc::new(RwLock::new(stats)),
            active_pool: Arc::new(RwLock::new(initial_pool)),
            counter: Arc::new(AtomicUsize::new(0)),
            interval_secs,
            latency_margin_ms,
        }
    }

    fn spawn_monitor(&self) {
        let stats_clone = self.stats.clone();
        let active_pool_clone = self.active_pool.clone();
        let interval = self.interval_secs;
        let margin = self.latency_margin_ms;

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(async {
                let client = Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .unwrap();

                loop {
                    let mut current_stats = stats_clone.write().unwrap();
                    let mut min_latency = u64::MAX;

                    for endpoint in current_stats.iter_mut() {
                        let start = Instant::now();
                        let mut is_healthy = false;

                        // --- UNIVERSAL CHECK LOGIC ---
                        if endpoint.url.starts_with("tcp://") {
                            // Strip prefix to get host:port (e.g., db.com:5432)
                            let host_port = endpoint.url.trim_start_matches("tcp://");
                            
                            // 2-second timeout for the TCP Handshake
                            let tcp_check = tokio::time::timeout(
                                Duration::from_secs(2),
                                TcpStream::connect(host_port)
                            ).await;

                            if let Ok(Ok(_)) = tcp_check {
                                is_healthy = true;
                            }
                        } else {
                            // Standard HTTP/HTTPS HEAD Probe
                            let res = client.head(&endpoint.url).send().await;
                            if let Ok(resp) = res {
                                if resp.status().is_success() {
                                    is_healthy = true;
                                }
                            }
                        }
                        // -----------------------------

                        if is_healthy {
                            endpoint.latency_ms = start.elapsed().as_millis() as u64;
                            endpoint.is_healthy = true;
                            if endpoint.latency_ms < min_latency {
                                min_latency = endpoint.latency_ms;
                            }
                        } else {
                            endpoint.latency_ms = u64::MAX;
                            endpoint.is_healthy = false;
                        }
                    }

                    // Rebuild routing pool with healthy, optimal nodes
                    let mut new_pool = Vec::new();
                    let threshold = min_latency.saturating_add(margin);

                    for e in current_stats.iter() {
                        if e.is_healthy && e.latency_ms <= threshold {
                            for _ in 0..e.weight {
                                new_pool.push(e.url.clone());
                            }
                        }
                    }

                    // Fallback mechanism if everything fails
                    if new_pool.is_empty() {
                        for e in current_stats.iter() { new_pool.push(e.url.clone()); }
                    }

                    *active_pool_clone.write().unwrap() = new_pool;
                    
                    drop(current_stats);
                    sleep(Duration::from_secs(interval)).await;
                }
            });
        });
    }

    fn get_endpoint(&self) -> String {
        let pool = self.active_pool.read().unwrap();
        if pool.is_empty() { return String::new(); }
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        pool[count % pool.len()].clone()
    }

    /// Convenience method to return strictly the host:port.
    /// Strips protocols (tcp://, http://) and any URL paths.
    fn get_host_port(&self) -> String {
        let endpoint = self.get_endpoint();
        if endpoint.is_empty() { return String::new(); }

        let stripped = if let Some(s) = endpoint.strip_prefix("tcp://") {
            s
        } else if let Some(s) = endpoint.strip_prefix("http://") {
            s
        } else if let Some(s) = endpoint.strip_prefix("https://") {
            s
        } else {
            &endpoint
        };

        // Split by '/' to remove any paths (e.g., from HTTP URLs)
        stripped.split('/').next().unwrap_or(stripped).to_string()
    }

    fn report_failure(&self, failed_url: String) {
        let mut stats = self.stats.write().unwrap();
        let mut min_latency = u64::MAX;
        let mut changed = false;

        for endpoint in stats.iter_mut() {
            if endpoint.url == failed_url {
                endpoint.is_healthy = false;
                endpoint.latency_ms = u64::MAX;
                changed = true;
            } else if endpoint.is_healthy && endpoint.latency_ms < min_latency {
                min_latency = endpoint.latency_ms;
            }
        }

        if changed {
            let mut new_pool = Vec::new();
            let threshold = min_latency.saturating_add(self.latency_margin_ms);

            for e in stats.iter() {
                if e.is_healthy && e.latency_ms <= threshold {
                    for _ in 0..e.weight {
                        new_pool.push(e.url.clone());
                    }
                }
            }
            if !new_pool.is_empty() {
                *self.active_pool.write().unwrap() = new_pool;
            }
        }
    }

    fn get_report(&self) -> std::collections::HashMap<String, u64> {
        self.stats.read().unwrap().iter().filter(|e| e.is_healthy)
            .map(|e| (e.url.clone(), e.latency_ms))
            .collect()
    }
}

#[pymodule]
fn gslb_rust(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<GslbResolver>()?;
    Ok(())
}