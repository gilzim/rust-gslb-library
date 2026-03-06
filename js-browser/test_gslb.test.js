// Testing the js-browser WASM wrapper
const { TextEncoder, TextDecoder } = require('util');
const fetch = require('node-fetch');
const crypto = require('crypto');

global.TextEncoder = TextEncoder;
global.TextDecoder = TextDecoder;
global.fetch = fetch;
global.Headers = fetch.Headers;
global.Request = fetch.Request;
global.Response = fetch.Response;
global.crypto = crypto.webcrypto;

describe('GslbResolver Browser Binding', () => {
    let GslbResolver;
    let resolvers = [];

    beforeAll(() => {
        try {
            // For Jest running in a Node environment against the nodejs target
            const mod = require('./pkg-node/gslb_browser.js');
            GslbResolver = mod.GslbResolver;
        } catch (e) {
            try {
                // Fallback to standard pkg if built for bundler/web 
                const mod = require('./pkg/gslb_browser.js');
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
        resolvers.push(resolver);
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
        resolvers.push(resolver);
        
        resolver.report_failure("https://bad.api.com");
        
        const endpoint = resolver.get_endpoint();
        expect(endpoint).toBe("https://good.api.com");
    });

    it('returns the host port correctly', () => {
        const nodes = ["https://my-api.com:8080"];
        const resolver = new GslbResolver(nodes, 5, 20);
        resolvers.push(resolver);
        const hostPort = resolver.get_host_port();
        expect(hostPort).toBe("my-api.com:8080");
    });

    it('reports failure by host_port identifier', () => {
        const nodes = ["https://fail-me.com:9000", "https://stay-alive.com:9000"];
        const resolver = new GslbResolver(nodes, 5, 20);
        resolvers.push(resolver);
        
        resolver.report_failure("fail-me.com:9000");
        
        const endpoint = resolver.get_endpoint();
        expect(endpoint).toBe("https://stay-alive.com:9000");
    });

    // Wait a brief moment to allow the WASM monitor threads to spin down
    // to prevent memory access violations on process exit.
    afterAll((done) => {
        resolvers.forEach(r => {
            try { r.stop_monitor(); } catch (e) {}
        });
        setTimeout(done, 100);
    });
});

