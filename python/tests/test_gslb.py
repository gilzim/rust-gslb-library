import pytest
import threading
import time
from collections import Counter
from http.server import BaseHTTPRequestHandler, HTTPServer
import gslb_rust

# --- 1. Mock Server Setup ---
# We define specific paths to simulate different network conditions
MOCK_STATE = {
    "/fast1": {"delay": 0.0, "status": 200},
    "/fast2": {"delay": 0.0, "status": 200},
    "/slow":  {"delay": 0.1, "status": 200}, # 100ms latency
    "/fail":  {"delay": 0.0, "status": 500},
}

class MockHandler(BaseHTTPRequestHandler):
    def do_HEAD(self):
        state = MOCK_STATE.get(self.path, {"delay": 0.0, "status": 404})
        if state["delay"] > 0:
            time.sleep(state["delay"])
        self.send_response(state["status"])
        self.end_headers()
        
    def log_message(self, format, *args):
        pass # Keep test output clean

@pytest.fixture(scope="module")
def mock_server():
    """Spawns a local HTTP server in a background thread for the tests."""
    server = HTTPServer(('127.0.0.1', 8080), MockHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield "http://127.0.0.1:8080"
    server.shutdown()

# --- 2. Test Cases ---

def test_default_weights_and_failover(mock_server):
    """Tests basic routing and automatic failure exclusion."""
    nodes = [f"{mock_server}/fast1", f"{mock_server}/fail"]
    
    # 1 second interval, 20ms margin
    resolver = gslb_rust.GslbResolver(nodes, 1, 20)
    resolver.spawn_monitor()
    
    # Wait for the first background probe to complete
    time.sleep(1.2)
    
    # /fail should be excluded, leaving only /fast1
    assert resolver.get_endpoint() == f"{mock_server}/fast1"

def test_weighted_round_robin(mock_server):
    """Tests WRR ratio distribution among healthy nodes."""
    nodes = [
        (f"{mock_server}/fast1", 3), # 75% traffic
        (f"{mock_server}/fast2", 1)  # 25% traffic
    ]
    
    resolver = gslb_rust.GslbResolver(nodes, 1, 20)
    resolver.spawn_monitor()
    time.sleep(1.2)
    
    # Pull 100 endpoints
    results = [resolver.get_endpoint() for _ in range(100)]
    counts = Counter(results)
    
    # The atomic counter should perfectly distribute 75/25
    assert counts[f"{mock_server}/fast1"] == 75
    assert counts[f"{mock_server}/fast2"] == 25

def test_latency_margin_exclusion(mock_server):
    """Tests that a node is excluded if it falls outside the latency margin."""
    nodes = [f"{mock_server}/fast1", f"{mock_server}/slow"]
    
    # Margin is 50ms. /slow takes 100ms, /fast1 takes 0ms. 
    # Therefore, /slow should be ignored.
    resolver = gslb_rust.GslbResolver(nodes, 1, 50)
    resolver.spawn_monitor()
    time.sleep(1.2)
    
    results = [resolver.get_endpoint() for _ in range(10)]
    counts = Counter(results)
    
    assert counts[f"{mock_server}/fast1"] == 10
    assert f"{mock_server}/slow" not in counts

def test_manual_failure_override(mock_server):
    """Tests the manual circuit breaker logic."""
    nodes = [f"{mock_server}/fast1", f"{mock_server}/fast2"]
    resolver = gslb_rust.GslbResolver(nodes, 5, 20) # 5 sec interval
    resolver.spawn_monitor()
    time.sleep(1.2)
    
    # Manually declare fast1 as failed
    resolver.report_failure(f"{mock_server}/fast1")
    
    # It should instantly switch to fast2, despite the 5 second probe interval
    assert resolver.get_endpoint() == f"{mock_server}/fast2"