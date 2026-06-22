"use strict";
// Trytet TypeScript SDK — @trytet/client
// Full-featured client for the Trytet Engine API, MCP, WebSocket telemetry, and cartridge management.
Object.defineProperty(exports, "__esModule", { value: true });
exports.TelemetryStream = exports.TrytetClient = void 0;
const DEFAULT_RETRY = {
    maxRetries: 3,
    initialDelayMs: 100,
    maxDelayMs: 5000,
    backoffMultiplier: 2,
};
// ---------------------------------------------------------------------------
// TrytetClient
// ---------------------------------------------------------------------------
class TrytetClient {
    baseUrl;
    retryConfig;
    constructor(options) {
        this.baseUrl = options?.baseUrl || 'http://localhost:3000';
        this.retryConfig = { ...DEFAULT_RETRY, ...options?.retry };
    }
    // -- low-level fetch with retry -----------------------------------------
    async fetchWithRetry(path, init, retryOnStatus) {
        const statuses = retryOnStatus || [502, 503, 504];
        let lastError;
        let delay = this.retryConfig.initialDelayMs;
        for (let attempt = 0; attempt <= this.retryConfig.maxRetries; attempt++) {
            try {
                const res = await fetch(`${this.baseUrl}${path}`, init);
                if (!res.ok && statuses.includes(res.status) && attempt < this.retryConfig.maxRetries) {
                    throw new Error(`Retryable status ${res.status}`);
                }
                return res;
            }
            catch (err) {
                lastError = err instanceof Error ? err : new Error(String(err));
                if (attempt < this.retryConfig.maxRetries) {
                    await this.sleep(delay);
                    delay = Math.min(delay * this.retryConfig.backoffMultiplier, this.retryConfig.maxDelayMs);
                }
            }
        }
        throw lastError;
    }
    sleep(ms) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }
    async checkResponse(res) {
        if (!res.ok) {
            const text = await res.text().catch(() => 'Unknown error');
            throw new Error(`Request failed (${res.status}): ${text}`);
        }
        return res.json();
    }
    // -- Agent execution ----------------------------------------------------
    async execute(request) {
        const res = await this.fetchWithRetry('/v1/tet/execute', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(request),
        });
        return this.checkResponse(res);
    }
    async snapshot(tetId) {
        const res = await this.fetchWithRetry(`/v1/tet/snapshot/${tetId}`, {
            method: 'POST',
        });
        return this.checkResponse(res);
    }
    async fork(snapshotId, request) {
        const res = await this.fetchWithRetry(`/v1/tet/fork/${snapshotId}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(request),
        });
        return this.checkResponse(res);
    }
    async teleport(alias, targetNode) {
        const res = await this.fetchWithRetry(`/v1/tet/teleport/${alias}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(targetNode),
        });
        await this.checkResponse(res);
    }
    async getTopology() {
        const res = await this.fetchWithRetry('/v1/topology', { method: 'GET' });
        return this.checkResponse(res);
    }
    async getSwarmMetrics() {
        const res = await this.fetchWithRetry('/v1/swarm/metrics', { method: 'GET' });
        return this.checkResponse(res);
    }
    async getHealth() {
        const res = await this.fetchWithRetry('/health', { method: 'GET' });
        return this.checkResponse(res);
    }
    // -- Cartridge management -----------------------------------------------
    async invokeCartridge(invocation) {
        const res = await this.fetchWithRetry('/v1/cartridge/invoke', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(invocation),
        });
        return this.checkResponse(res);
    }
    // -- MCP -----------------------------------------------------------------
    async mcpCall(method, params) {
        const request = {
            jsonrpc: '2.0',
            id: Date.now(),
            method,
            params,
        };
        const res = await this.fetchWithRetry('/v1/mcp', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(request),
        });
        const rpcResponse = await this.checkResponse(res);
        return rpcResponse.result;
    }
    async listTools() {
        const result = await this.mcpCall('tools/list');
        const obj = result;
        return obj.tools || [];
    }
    async callTool(name, args) {
        return this.mcpCall('tools/call', { name, arguments: args });
    }
    async listResources() {
        const result = await this.mcpCall('resources/list');
        const obj = result;
        return obj.resources || [];
    }
    async listPrompts() {
        const result = await this.mcpCall('prompts/list');
        const obj = result;
        return obj.prompts || [];
    }
    // -- WebSocket telemetry ------------------------------------------------
    createTelemetryStream() {
        const wsUrl = this.baseUrl.replace(/^http/, 'ws') + '/v1/swarm/stream';
        return new TelemetryStream(wsUrl);
    }
}
exports.TrytetClient = TrytetClient;
class TelemetryStream {
    ws = null;
    listeners = new Set();
    reconnectDelay = 1000;
    maxReconnectDelay = 30000;
    url;
    closed = false;
    constructor(url) {
        this.url = url;
    }
    connect() {
        if (this.closed)
            return;
        this.ws = new WebSocket(this.url);
        this.ws.onmessage = (event) => {
            try {
                const parsed = JSON.parse(event.data);
                this.emit(parsed);
            }
            catch {
                // Skip unparseable messages
            }
        };
        this.ws.onclose = () => {
            if (!this.closed) {
                setTimeout(() => {
                    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay);
                    this.connect();
                }, this.reconnectDelay);
            }
        };
        this.ws.onerror = () => {
            this.ws?.close();
        };
    }
    onEvent(callback) {
        this.listeners.add(callback);
        return () => this.listeners.delete(callback);
    }
    close() {
        this.closed = true;
        this.ws?.close();
        this.listeners.clear();
    }
    emit(event) {
        for (const listener of this.listeners) {
            try {
                listener(event);
            }
            catch {
                // Isolate listener failures
            }
        }
    }
}
exports.TelemetryStream = TelemetryStream;
