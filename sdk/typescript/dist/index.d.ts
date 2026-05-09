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
export interface TetExecutionResult {
    tet_id: string;
    status: 'Success' | 'OutOfFuel' | 'MemoryExceeded' | 'Migrated' | 'Suspended' | {
        Crash: {
            error_type: string;
            message: string;
            instruction_offset?: number;
        };
    };
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
export declare class TrytetClient {
    private baseUrl;
    constructor(options?: {
        baseUrl?: string;
    });
    /**
     * Executes a WebAssembly payload or forks from an existing snapshot.
     */
    execute(request: TetExecutionRequest): Promise<TetExecutionResult>;
    /**
     * Captures the live memory and VFS state of a completed Wasm execution.
     */
    snapshot(tetId: string): Promise<SnapshotResponse>;
    /**
     * Forks a new execution instance from a pre-existing snapshot.
     */
    fork(snapshotId: string, request: TetExecutionRequest): Promise<TetExecutionResult>;
    /**
     * Teleports an agent to another node in the hive.
     */
    teleport(alias: string, targetNode: string): Promise<void>;
    /**
     * Retrieves the live hive topology (connected peers).
     */
    getTopology(): Promise<any>;
}
