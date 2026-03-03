# gslb_rust

A blazing-fast, minimal-footprint Global Server Load Balancing (GSLB) client library for Python, powered by a native Rust engine.

`gslb_rust` allows Python applications to perform client-side load balancing with intelligent latency-based routing, Weighted Round Robin (WRR), and instant failover—all without blocking the Python Global Interpreter Lock (GIL) or consuming significant CPU/Memory resources.

## 🚀 Features

* **Zero Python Overhead:** Background health checks and latency monitoring run in a native Rust OS thread using tokio.

* **Latency-Aware Routing:** Automatically routes traffic to the fastest responding endpoint.

* **Tolerance Margins:** Group nodes with similar latencies (e.g., within 30ms of each other) to prevent traffic flapping.

* **Weighted Round Robin (WRR):** Distribute traffic proportionally among optimal endpoints (e.g., 75% to US-East, 25% to US-West).

* **Instant Failover:** Manually report failures mid-flight to instantly strike a node from the routing pool.

* **No External Dependencies:** Statically linked (rustls), requiring no system-level OpenSSL installations.

## 📦 Installation

Currently, this library is distributed via GitHub Releases.

1. Go to the Releases Page of this repository.

2. Download the `.whl` file that matches your Operating System and Python version.

3. Install it using `pip`:
    ```bash
    pip install gslb_rust-<version>-<platform>.whl
    ```


## 💻 Quick Start

The simplest way to use gslb_rust is to provide a list of URLs. The library will continuously monitor them in the background and always return the fastest healthy node.
```python
import gslb_rust
import requests
import time

# 1. Define your endpoints
nodes = [
    "[https://us-east.api.com](https://us-east.api.com)",
    "[https://eu-west.api.com](https://eu-west.api.com)"
]

# 2. Initialize the resolver (checks every 5 seconds, 20ms latency margin)
resolver = gslb_rust.GslbResolver(nodes, interval_secs=5, latency_margin_ms=20)

# 3. Spawn the Rust background monitor
resolver.spawn_monitor()

# 4. Use it in your application loop
while True:
    # This call takes < 1 microsecond (O(1) lookup in Rust memory)
    best_endpoint = resolver.get_endpoint()
    
    print(f"Routing to: {best_endpoint}")
    # response = requests.get(f"{best_endpoint}/data")
    
    time.sleep(1)
```

## ⚙️ Advanced Usage

### Weighted Round Robin (WRR)

If you have clusters of different sizes, you can pass a list of (`URL, Weight`) tuples. The library will distribute traffic proportionally among the nodes that fall within the latency margin.
```python
# 3-to-1 traffic distribution (75% / 25%)
weighted_nodes = [
    ("[https://massive-cluster.api.com](https://massive-cluster.api.com)", 3),
    ("[https://small-backup.api.com](https://small-backup.api.com)", 1)
]

resolver = gslb_rust.GslbResolver(weighted_nodes, interval_secs=5, latency_margin_ms=50)
resolver.spawn_monitor()

# Subsequent calls to get_endpoint() will respect the 3:1 ratio
```

### The "Latency Margin" Explained

Strict latency routing causes "flapping" (where 100% of traffic wildly swings between two servers that are only 1ms apart).

The `latency_margin_ms` parameter fixes this. If your fastest node responds in 40ms, and your margin is set to 20, any healthy node that responds in **60ms or less** is added to the active routing pool. Traffic is then distributed among that pool using your WRR weights.

### Manual Circuit Breaker

Sometimes a server responds to a background health ping (`200 OK`), but fails during actual application logic (e.g., `500 Internal Server Error` on a specific database query). You can report this to instantly pull the node from rotation.

```python
target = resolver.get_endpoint()

try:
    response = requests.get(f"{target}/complex-query", timeout=2)
    response.raise_for_status()
except requests.exceptions.RequestException:
    print(f"Node {target} failed! Striking from rotation.")
    
    # Instantly removes the node from the active WRR pool
    resolver.report_failure(target)
    
    # Fallback to the next best node immediately
    target = resolver.get_endpoint()
```

### Health Reporting

Want to see the current latencies without parsing logs? Use the built-in report generator:
```python
stats = resolver.get_report()
print(stats) 
# Output: {'[https://us-east.api.com](https://us-east.api.com)': 42, '[https://eu-west.api.com](https://eu-west.api.com)': 115}
```

## 🛠️ Building from Source

If you want to contribute or build the library locally, you will need the Rust toolchain and maturin.

1. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

2. Install Maturin: `pip install maturin`

3. Clone the repo and run:
    ```bash
    maturin develop --release
    ```

4. Run the test suite:
    ```bash
    pytest tests/test_gslb.py
    ```
