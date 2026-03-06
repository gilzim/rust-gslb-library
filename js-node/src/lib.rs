use napi_derive::napi;
use gslb_core::CoreResolver;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::sleep;
use tokio::net::TcpStream;
use reqwest::Client;

#[napi]
pub struct GslbResolver {
    inner: CoreResolver,
    stop_signal: Arc<AtomicBool>,
}

#[napi]
impl GslbResolver {
    #[napi(constructor)]
    pub fn new(urls: Vec<String>, interval_secs: u32, latency_margin_ms: u32) -> Self {
        let config_iter = urls.into_iter().map(|u| (u, 1)).collect();
        Self { 
            inner: CoreResolver::new(config_iter, interval_secs as u64, latency_margin_ms as u64),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    #[napi]
    pub fn spawn_monitor(&self) {
        let core = self.inner.clone();
        let stop_signal = self.stop_signal.clone();
        
        // Spawns a native OS thread just like Python!
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(async {
                let client = Client::builder().timeout(Duration::from_secs(2)).build().unwrap();
                while !stop_signal.load(Ordering::Relaxed) {
                    let mut min_latency = u64::MAX;
                    {
                        let mut current_stats = core.stats.write().unwrap();
                        for endpoint in current_stats.iter_mut() {
                            let start = Instant::now();
                            let mut is_healthy = false;

                            // --- UNIVERSAL CHECK LOGIC (TCP + HTTP) ---
                            if endpoint.url.starts_with("tcp://") {
                                let host_port = endpoint.url.trim_start_matches("tcp://");
                                let tcp_check = tokio::time::timeout(
                                    Duration::from_secs(2),
                                    TcpStream::connect(host_port)
                                ).await;

                                if let Ok(Ok(_)) = tcp_check { is_healthy = true; }
                            } else {
                                if let Ok(resp) = client.head(&endpoint.url).send().await {
                                    if resp.status().is_success() { is_healthy = true; }
                                }
                            }

                            if is_healthy {
                                endpoint.latency_ms = start.elapsed().as_millis() as u64;
                                endpoint.is_healthy = true;
                                if endpoint.latency_ms < min_latency { min_latency = endpoint.latency_ms; }
                            } else {
                                endpoint.latency_ms = u64::MAX;
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

    #[napi]
    pub fn get_endpoint(&self) -> String { self.inner.get_endpoint() }
    
    #[napi]
    pub fn get_host_port(&self) -> String { self.inner.get_host_port() }
    
    
    #[napi]
    pub fn report_failure(&self, failed_url: String) { self.inner.report_failure(&failed_url) }

    #[napi]
    pub fn stop_monitor(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}