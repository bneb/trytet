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
export type ExecutionStatus = 'Success' | 'OutOfFuel' | 'MemoryExceeded' | 'Migrated' | 'Suspended' | {
    Crash: CrashReport;
};
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
export interface NorthstarReport {
    teleport_warp_us: number;
    mitosis_constant_us: number;
    oracle_fidelity_us: number;
    market_evacuation_us: number;
    cartridge_spinup_us: number;
    timestamp: string;
}
export interface RetryConfig {
    maxRetries: number;
    initialDelayMs: number;
    maxDelayMs: number;
    backoffMultiplier: number;
}
export declare class TrytetClient {
    private baseUrl;
    private retryConfig;
    constructor(options?: {
        baseUrl?: string;
        retry?: Partial<RetryConfig>;
    });
    private fetchWithRetry;
    private sleep;
    private checkResponse;
    execute(request: TetExecutionRequest): Promise<TetExecutionResult>;
    snapshot(tetId: string): Promise<SnapshotResponse>;
    fork(snapshotId: string, request: TetExecutionRequest): Promise<TetExecutionResult>;
    teleport(alias: string, targetNode: string): Promise<void>;
    getTopology(): Promise<TopologyEdge[]>;
    getSwarmMetrics(): Promise<NorthstarReport>;
    getHealth(): Promise<{
        status: string;
    }>;
    invokeCartridge(invocation: CartridgeInvocation): Promise<CartridgeResult>;
    mcpCall(method: string, params?: Record<string, unknown>): Promise<unknown>;
    listTools(): Promise<McpTool[]>;
    callTool(name: string, args: Record<string, unknown>): Promise<unknown>;
    listResources(): Promise<McpResource[]>;
    listPrompts(): Promise<McpPrompt[]>;
    createTelemetryStream(): TelemetryStream;
}
export type TelemetryEventCallback = (event: TelemetryEvent) => void;
export interface TelemetryEvent {
    event_type: string;
    tet_id: string;
    timestamp_us: number;
    data: Record<string, unknown>;
}
export declare class TelemetryStream {
    private ws;
    private listeners;
    private reconnectDelay;
    private maxReconnectDelay;
    private url;
    private closed;
    constructor(url: string);
    connect(): void;
    onEvent(callback: TelemetryEventCallback): () => void;
    close(): void;
    private emit;
}
