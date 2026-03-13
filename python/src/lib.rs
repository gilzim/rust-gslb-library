use pyo3::prelude::*;
use gslb_core::{CoreResolver, UNHEALTHY_LATENCY}; 
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::sleep;
use tokio::net::TcpStream;
use reqwest::Client;

#[derive(FromPyObject)]
pub enum EndpointConfig {
    Urls(Vec<String>),
    WeightedUrls(Vec<(String, usize)>),
}

#[pyclass]
pub struct GslbResolver {
    inner: CoreResolver,
    stop_signal: Arc<AtomicBool>,
}

#[pymethods]
impl GslbResolver {
    #[new]
    #[pyo3(signature = (nodes, interval_secs=5, latency_margin_ms=20))]
    fn new(nodes: EndpointConfig, interval_secs: u64, latency_margin_ms: u64) -> Self {
        let config_iter = match nodes {
            EndpointConfig::Urls(urls) => urls.into_iter().map(|u| (u, 1)).collect(),
            EndpointConfig::WeightedUrls(weighted) => weighted,
        };
        Self {
            inner: CoreResolver::new(config_iter, interval_secs, latency_margin_ms),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_monitor(&self) {
        let core = self.inner.clone();
        let stop_signal = self.stop_signal.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(async {
                let client = Client::builder().timeout(Duration::from_secs(2)).build().unwrap();
                while !stop_signal.load(Ordering::Relaxed) {
                    let mut min_latency = UNHEALTHY_LATENCY;
                    {
                        let mut current_stats = core.stats.write().unwrap_or_else(|e| e.into_inner());
                        for endpoint in current_stats.iter_mut() {
                            let start = Instant::now();
                            let mut is_healthy = false;

                            // --- UNIVERSAL CHECK LOGIC ---
                            if endpoint.url.starts_with("tcp://") {
                                let host_port = endpoint.url.trim_start_matches("tcp://");
                                let tcp_check = tokio::time::timeout(
                                    Duration::from_secs(2),
                                    TcpStream::connect(host_port)
                                ).await;

                                if let Ok(Ok(_)) = tcp_check {
                                    is_healthy = true;
                                }
                            } else {
                                if let Ok(resp) = client.head(&endpoint.url).send().await {
                                    if resp.status().is_success() {
                                        is_healthy = true;
                                    }
                                }
                            }
                            // -----------------------------

                            if is_healthy {
                                endpoint.latency_ms = start.elapsed().as_millis() as u64;
                                endpoint.is_healthy = true;
                                if endpoint.latency_ms < min_latency { min_latency = endpoint.latency_ms; }
                            } else {
                                endpoint.latency_ms = UNHEALTHY_LATENCY;
                                endpoint.is_healthy = false;
                            }
                        }
                    } 
                    core.rebuild_pool(min_latency);
                    sleep(Duration::from_secs(core.interval_secs)).await;
                }
            });
        });
    }

    fn stop_monitor(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }

    fn get_endpoint(&self) -> String { self.inner.get_endpoint() }
    fn get_host_port(&self) -> String { self.inner.get_host_port() }
    fn report_failure(&self, failed_url: String) { self.inner.report_failure(&failed_url) }
    fn get_report(&self) -> std::collections::HashMap<String, u64> { self.inner.get_report() }
}

#[pymodule]
fn gslb_rust(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<GslbResolver>()?;
    Ok(())
}