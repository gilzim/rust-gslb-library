// Testing the js-browser WASM wrapper

describe('GslbResolver Browser Binding', () => {
    let GslbResolver;

    beforeAll(() => {
        try {
            // For Jest running in a Node environment against the nodejs target
            const mod = require('./pkg-node/gslb_browser_source');
            GslbResolver = mod.GslbResolver;
        } catch (e) {
            try {
                // Fallback to standard pkg if built for bundler/web 
                const mod = require('./pkg/gslb_browser_source');
                GslbResolver = mod.GslbResolver;
            } catch (err) {
                console.error("Could not load WASM module, ensure 'npm run build:nodejs' has been executed.");
                throw err;
            }
        }
    });

    it('initializes and returns an expected endpoint', () => {
        const nodes = [
            "https://server1.api.com",
            "https://server2.api.com"
        ];
        
        const resolver = new GslbResolver(nodes, 5, 20);
        expect(resolver).toBeDefined();

        resolver.spawn_monitor();

        const endpoint = resolver.get_endpoint();
        expect(nodes).toContain(endpoint);
    });

    it('reports failure and removes node from active pool', () => {
        const nodes = [
            "https://good.api.com",
            "https://bad.api.com"
        ];
        
        const resolver = new GslbResolver(nodes, 5, 20);
        
        resolver.report_failure("https://bad.api.com");
        
        const endpoint = resolver.get_endpoint();
        expect(endpoint).toBe("https://good.api.com");
    });

    it('returns the host port correctly', () => {
        const nodes = ["https://my-api.com:8080"];
        const resolver = new GslbResolver(nodes, 5, 20);
        const hostPort = resolver.get_host_port();
        expect(hostPort).toBe("my-api.com:8080");
    });
});
