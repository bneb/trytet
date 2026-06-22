// Trytet TypeScript SDK — @trytet/client
// Full-featured client for the Trytet Engine API, MCP, WebSocket telemetry, and cartridge management.

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

export interface AgentManifest {
  metadata: {
    name: string;
    version: string;
    author_pubkey?: string;
  };
  constraints: {
    max_memory_pages: number;
    fuel_limit: number;
    max_egress_bytes: number;
  };
  permissions: {
    can_egress: string[];
    can_persist: boolean;
    can_teleport: boolean;
    is_genesis_factory: boolean;
    can_fork: boolean;
  };
}

export interface FuelVoucher {
  tet_id: string;
  fuel_limit: number;
  nonce: number;
  signature: number[];
}

export interface EgressPolicy {
  allowed_domains: string[];
  max_daily_bytes: number;
  require_https: boolean;
}

export interface TetExecutionRequest {
  payload?: number[];
  alias?: string;
  env?: Record<string, string>;
  injected_files?: Record<string, string>;
  allocated_fuel?: number;
  max_memory_mb?: number;
  parent_snapshot_id?: string;
  target_function?: string;
  call_depth?: number;
  voucher?: FuelVoucher;
  manifest?: AgentManifest;
  egress_policy?: EgressPolicy;
}

export interface StructuredTelemetry {
  stdout_lines: string[];
  stderr_lines: string[];
  memory_used_kb: number;
}

export type ExecutionStatus =
  | 'Success'
  | 'OutOfFuel'
  | 'MemoryExceeded'
  | 'Migrated'
  | 'Suspended'
  | { Crash: CrashReport };

export interface CrashReport {
  error_type: string;
  message: string;
  instruction_offset?: number;
}

export interface TetExecutionResult {
  tet_id: string;
  status: ExecutionStatus;
  telemetry: StructuredTelemetry;
  execution_duration_us: number;
  fuel_consumed: number;
  mutated_files: Record<string, string>;
  migrated_to?: string;
}

export interface SnapshotResponse {
  snapshot_id: string;
  size_bytes: number;
}

// ---------------------------------------------------------------------------
// MCP Types
// ---------------------------------------------------------------------------

export interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number | string;
  method: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: number | string;
  result: unknown;
}

export interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface McpResource {
  uri: string;
  name: string;
  description?: string;
  mimeType?: string;
}

export interface McpPrompt {
  name: string;
  description?: string;
  arguments?: McpPromptArgument[];
}

export interface McpPromptArgument {
  name: string;
  description?: string;
  required?: boolean;
}

// ---------------------------------------------------------------------------
// Cartridge Types
// ---------------------------------------------------------------------------

export interface CartridgeInvocation {
  component_id: string;
  payload: string;
  fuel: number;
  max_memory_mb?: number;
}

export interface CartridgeResult {
  output: string;
  fuel_consumed: number;
  duration_us: number;
}

// ---------------------------------------------------------------------------
// Swarm / Topology Types
// ---------------------------------------------------------------------------

export interface TopologyEdge {
  source: string;
  target: string;
  latency_us: number;
  bytes_transferred: number;
}

export interface HiveNode {
  node_id: string;
  public_addr: string;
  available_fuel: number;
  total_memory_mb: number;
  price_per_million_fuel: number;
}

// ---------------------------------------------------------------------------
// Northstar Benchmark Types
// ---------------------------------------------------------------------------

export interface NorthstarReport {
  teleport_warp_us: number;
  mitosis_constant_us: number;
  oracle_fidelity_us: number;
  market_evacuation_us: number;
  cartridge_spinup_us: number;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Retry Configuration
// ---------------------------------------------------------------------------

export interface RetryConfig {
  maxRetries: number;
  initialDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}

const DEFAULT_RETRY: RetryConfig = {
  maxRetries: 3,
  initialDelayMs: 100,
  maxDelayMs: 5000,
  backoffMultiplier: 2,
};

// ---------------------------------------------------------------------------
// TrytetClient
// ---------------------------------------------------------------------------

export class TrytetClient {
  private baseUrl: string;
  private retryConfig: RetryConfig;

  constructor(options?: { baseUrl?: string; retry?: Partial<RetryConfig> }) {
    this.baseUrl = options?.baseUrl || 'http://localhost:3000';
    this.retryConfig = { ...DEFAULT_RETRY, ...options?.retry };
  }

  // -- low-level fetch with retry -----------------------------------------

  private async fetchWithRetry(
    path: string,
    init: RequestInit,
    retryOnStatus?: number[],
  ): Promise<Response> {
    const statuses = retryOnStatus || [502, 503, 504];
    let lastError: Error | undefined;
    let delay = this.retryConfig.initialDelayMs;

    for (let attempt = 0; attempt <= this.retryConfig.maxRetries; attempt++) {
      try {
        const res = await fetch(`${this.baseUrl}${path}`, init);
        if (!res.ok && statuses.includes(res.status) && attempt < this.retryConfig.maxRetries) {
          throw new Error(`Retryable status ${res.status}`);
        }
        return res;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
        if (attempt < this.retryConfig.maxRetries) {
          await this.sleep(delay);
          delay = Math.min(delay * this.retryConfig.backoffMultiplier, this.retryConfig.maxDelayMs);
        }
      }
    }
    throw lastError!;
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  private async checkResponse<T>(res: Response): Promise<T> {
    if (!res.ok) {
      const text = await res.text().catch(() => 'Unknown error');
      throw new Error(`Request failed (${res.status}): ${text}`);
    }
    return res.json() as Promise<T>;
  }

  // -- Agent execution ----------------------------------------------------

  async execute(request: TetExecutionRequest): Promise<TetExecutionResult> {
    const res = await this.fetchWithRetry('/v1/tet/execute', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    return this.checkResponse<TetExecutionResult>(res);
  }

  async snapshot(tetId: string): Promise<SnapshotResponse> {
    const res = await this.fetchWithRetry(`/v1/tet/snapshot/${tetId}`, {
      method: 'POST',
    });
    return this.checkResponse<SnapshotResponse>(res);
  }

  async fork(snapshotId: string, request: TetExecutionRequest): Promise<TetExecutionResult> {
    const res = await this.fetchWithRetry(`/v1/tet/fork/${snapshotId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    return this.checkResponse<TetExecutionResult>(res);
  }

  async teleport(alias: string, targetNode: string): Promise<void> {
    const res = await this.fetchWithRetry(`/v1/tet/teleport/${alias}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(targetNode),
    });
    await this.checkResponse(res);
  }

  async getTopology(): Promise<TopologyEdge[]> {
    const res = await this.fetchWithRetry('/v1/topology', { method: 'GET' });
    return this.checkResponse<TopologyEdge[]>(res);
  }

  async getSwarmMetrics(): Promise<NorthstarReport> {
    const res = await this.fetchWithRetry('/v1/swarm/metrics', { method: 'GET' });
    return this.checkResponse<NorthstarReport>(res);
  }

  async getHealth(): Promise<{ status: string }> {
    const res = await this.fetchWithRetry('/health', { method: 'GET' });
    return this.checkResponse<{ status: string }>(res);
  }

  // -- Cartridge management -----------------------------------------------

  async invokeCartridge(invocation: CartridgeInvocation): Promise<CartridgeResult> {
    const res = await this.fetchWithRetry('/v1/cartridge/invoke', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(invocation),
    });
    return this.checkResponse<CartridgeResult>(res);
  }

  // -- MCP -----------------------------------------------------------------

  async mcpCall(method: string, params?: Record<string, unknown>): Promise<unknown> {
    const request: JsonRpcRequest = {
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
    const rpcResponse = await this.checkResponse<JsonRpcResponse>(res);
    return rpcResponse.result;
  }

  async listTools(): Promise<McpTool[]> {
    const result = await this.mcpCall('tools/list');
    const obj = result as { tools: McpTool[] };
    return obj.tools || [];
  }

  async callTool(name: string, args: Record<string, unknown>): Promise<unknown> {
    return this.mcpCall('tools/call', { name, arguments: args });
  }

  async listResources(): Promise<McpResource[]> {
    const result = await this.mcpCall('resources/list');
    const obj = result as { resources: McpResource[] };
    return obj.resources || [];
  }

  async listPrompts(): Promise<McpPrompt[]> {
    const result = await this.mcpCall('prompts/list');
    const obj = result as { prompts: McpPrompt[] };
    return obj.prompts || [];
  }

  // -- WebSocket telemetry ------------------------------------------------

  createTelemetryStream(): TelemetryStream {
    const wsUrl = this.baseUrl.replace(/^http/, 'ws') + '/v1/swarm/stream';
    return new TelemetryStream(wsUrl);
  }
}

// ---------------------------------------------------------------------------
// TelemetryStream — real-time WebSocket telemetry
// ---------------------------------------------------------------------------

export type TelemetryEventCallback = (event: TelemetryEvent) => void;

export interface TelemetryEvent {
  event_type: string;
  tet_id: string;
  timestamp_us: number;
  data: Record<string, unknown>;
}

export class TelemetryStream {
  private ws: WebSocket | null = null;
  private listeners: Set<TelemetryEventCallback> = new Set();
  private reconnectDelay = 1000;
  private maxReconnectDelay = 30000;
  private url: string;
  private closed = false;

  constructor(url: string) {
    this.url = url;
  }

  connect(): void {
    if (this.closed) return;
    this.ws = new WebSocket(this.url);

    this.ws.onmessage = (event: MessageEvent) => {
      try {
        const parsed: TelemetryEvent = JSON.parse(event.data as string);
        this.emit(parsed);
      } catch {
        // Skip unparseable messages
      }
    };

    this.ws.onclose = () => {
      if (!this.closed) {
        setTimeout(() => {
          this.reconnectDelay = Math.min(
            this.reconnectDelay * 2,
            this.maxReconnectDelay,
          );
          this.connect();
        }, this.reconnectDelay);
      }
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };
  }

  onEvent(callback: TelemetryEventCallback): () => void {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  }

  close(): void {
    this.closed = true;
    this.ws?.close();
    this.listeners.clear();
  }

  private emit(event: TelemetryEvent): void {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch {
        // Isolate listener failures
      }
    }
  }
}
