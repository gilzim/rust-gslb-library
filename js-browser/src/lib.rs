use wasm_bindgen::prelude::*;
use gslb_core::CoreResolver; 
use reqwest::Client;
use gloo_timers::future::sleep;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[wasm_bindgen]
pub struct GslbResolver {
    inner: CoreResolver,
    stop_signal: Arc<AtomicBool>,
}

#[wasm_bindgen]
impl GslbResolver {
    #[wasm_bindgen(constructor)]
    pub fn new(urls: js_sys::Array, interval_secs: u32, latency_margin_ms: u32) -> Self {
        let mut config_iter = Vec::new();
        for i in 0..urls.length() {
            if let Some(url) = urls.get(i).as_string() {
                config_iter.push((url, 1)); 
            }
        }
        Self { 
            inner: CoreResolver::new(config_iter, interval_secs as u64, latency_margin_ms as u64),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    #[wasm_bindgen]
    pub fn spawn_monitor(&self) {
        let core = self.inner.clone();
        let stop_signal = self.stop_signal.clone();
        
        // Spawns asynchronously on the Browser Event Loop
        wasm_bindgen_futures::spawn_local(async move {
            let client = Client::new();
            while !stop_signal.load(Ordering::Relaxed) {
                let mut min_latency = u64::MAX;
                {
                    let mut current_stats = core.stats.write().unwrap();
                    for endpoint in current_stats.iter_mut() {
                        let start = js_sys::Date::now();
                        let mut is_healthy = false;

                        // Wasm has no TCP sockets. Ignore tcp:// endpoints.
                        if endpoint.url.starts_with("tcp://") {
                            is_healthy = false; 
                        } else {
                            if let Ok(resp) = client.head(&endpoint.url).send().await {
                                if resp.status().is_success() { is_healthy = true; }
                            }
                        }

                        if is_healthy {
                            endpoint.latency_ms = (js_sys::Date::now() - start) as u64;
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
    }

    #[wasm_bindgen]
    pub fn get_endpoint(&self) -> String { self.inner.get_endpoint() }
    
    #[wasm_bindgen]
    pub fn get_host_port(&self) -> String { self.inner.get_host_port() }
    
    
    #[wasm_bindgen]
    pub fn report_failure(&self, failed_url: String) { self.inner.report_failure(&failed_url) }

    #[wasm_bindgen]
    pub fn stop_monitor(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}