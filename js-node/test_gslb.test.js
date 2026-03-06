const { GslbResolver } = require('./index');

describe('GslbResolver Node.js Binding', () => {
    it('initializes and returns an expected endpoint', () => {
        const nodes = [
            "https://server1.api.com",
            "https://server2.api.com"
        ];
        
        // Instantiate the resolver
        const resolver = new GslbResolver(nodes, 5, 20);
        expect(resolver).toBeDefined();

        // Spawn background monitor
        resolver.spawnMonitor();

        // Verify that the retrieved endpoint is one of the initial nodes
        const endpoint = resolver.getEndpoint();
        expect(nodes).toContain(endpoint);
    });

    it('reports failure and removes node from active pool', () => {
        const nodes = [
            "https://good.api.com",
            "https://bad.api.com"
        ];
        
        const resolver = new GslbResolver(nodes, 5, 20);
        
        // Explicitly fail the bad node
        resolver.reportFailure("https://bad.api.com");
        
        // The endpoint should fallback to the remaining healthy node
        const endpoint = resolver.getEndpoint();
        expect(endpoint).toBe("https://good.api.com");
    });

    it('returns the host port correctly', () => {
        const nodes = ["https://my-api.com:8080"];
        const resolver = new GslbResolver(nodes, 5, 20);
        const hostPort = resolver.getHostPort();
        expect(hostPort).toBe("my-api.com:8080");
    });

    // Wait a brief moment to allow the tokio monitor thread to spin down
    // before Jest aggressively exits the native process, to prevent segfaults.
    afterAll((done) => {
        setTimeout(done, 100);
    });
});
