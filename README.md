# gslb_rust

A blazing‑fast, minimal‑footprint Global Server Load Balancing (GSLB) client library for Python, powered by a native Rust engine. The current release is **1.0.0** and the package supports CPython **3.7 through 3.12**.

`gslb_rust` lets Python applications perform client‑side load balancing with
intelligent latency‑based routing, Weighted Round Robin (WRR), and instant
failover—all without blocking the Python Global Interpreter Lock (GIL) or
consuming significant CPU/memory.

## 🚀 Features

* **Zero Python Overhead:** Background health checks and latency monitoring run in a native Rust OS thread using tokio.

* **Latency-Aware Routing:** Automatically routes traffic to the fastest responding endpoint.

* **Tolerance Margins:** Group nodes with similar latencies (e.g., within 30ms of each other) to prevent traffic flapping.

* **Weighted Round Robin (WRR):** Distribute traffic proportionally among optimal endpoints (e.g., 75% to US-East, 25% to US-West).

* **Instant Failover:** Manually report failures mid-flight to instantly strike a node from the routing pool.

* **No External Dependencies:** Statically linked (rustls), requiring no system-level OpenSSL installations.

## 📦 Installation

The project ships pre‑built binary wheels for each supported platform and
Python minor version.  Wheels are attached to the GitHub release for the
corresponding tag; names are of the form
`gslb_rust-<version>-cp3XY-cp3XY-<platform>.whl` (e.g. `cp312` for Python 3.12).

To install a wheel manually:

```bash
pip install gslb_rust-<version>-<platform>.whl
```

If you prefer, you can also install directly from a GitHub release URL or,
once published to PyPI, simply `pip install gslb_rust`.


## 💻 Quick Start

The simplest way to use `gslb_rust` is to provide a list of URLs. The
library will continuously monitor them in the background and always return
the fastest healthy node.

```python
import gslb_rust
import requests
import time

# 1. Define your endpoints
nodes = [
    "https://us-east.api.com",
    "https://eu-west.api.com",
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

By default, each healthy endpoint contributes an equal share of the
routing pool.  When you are balancing among clusters with differing
capacity you can supply a *weight* for each URL.  The resolver will
first filter nodes by latency margin and then apply WRR to the remaining
set, distributing traffic proportionally to the weights.

Weights are arbitrary positive integers; `2` and `4` are equivalent to
`1` and `2` (i.e. only the relative ratio matters).

Here’s a simple example – the first call uses equal weight, the second
uses a 3 : 1 ratio:

```python
# equal-weight usage (default behavior)
nodes = [
    "https://fast.api.com",
    "https://slow.api.com",
]
resolver = gslb_rust.GslbResolver(nodes, interval_secs=5, latency_margin_ms=20)
resolver.spawn_monitor()

# > roughly 50% of calls go to each endpoint

# weighted usage (75% / 25%)
weighted_nodes = [
    ("https://fast.api.com", 3),
    ("https://slow.api.com", 1),
]
resolver = gslb_rust.GslbResolver(weighted_nodes, interval_secs=5, latency_margin_ms=20)
resolver.spawn_monitor()

# > approximately three quarters of calls will hit the fast endpoint
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

To build locally (for development or to support a new Python version) you'll
need the Rust toolchain and `maturin`.

1. Install Rust: 
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. Install maturin in your Python environment:
   ```bash
   python -m pip install maturin
   ```
3. When checked out, build and install the package into a virtualenv:
   ```bash
   python -m venv .venv
   .venv/bin/python -m pip install --upgrade pip
   .venv/bin/python -m pip install maturin
   .venv/bin/python -m maturin develop --release
   ```
4. Run the test suite:
   ```bash
   .venv/bin/python -m pytest tests/test_gslb.py
   ```
