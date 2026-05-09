// src/index.ts

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
  payload?: number[]; // Wasm binary bytes as array
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
  status: 'Success' | 'OutOfFuel' | 'MemoryExceeded' | 'Migrated' | 'Suspended' | { Crash: { error_type: string; message: string; instruction_offset?: number } };
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

export class TrytetClient {
  private baseUrl: string;

  constructor(options?: { baseUrl?: string }) {
    this.baseUrl = options?.baseUrl || 'http://localhost:3000';
  }

  /**
   * Executes a WebAssembly payload or forks from an existing snapshot.
   */
  async execute(request: TetExecutionRequest): Promise<TetExecutionResult> {
    const res = await fetch(`${this.baseUrl}/v1/tet/execute`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Execution failed (${res.status}): ${errorText}`);
    }

    return res.json();
  }

  /**
   * Captures the live memory and VFS state of a completed Wasm execution.
   */
  async snapshot(tetId: string): Promise<SnapshotResponse> {
    const res = await fetch(`${this.baseUrl}/v1/tet/snapshot/${tetId}`, {
      method: 'POST',
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Snapshot failed (${res.status}): ${errorText}`);
    }

    return res.json();
  }

  /**
   * Forks a new execution instance from a pre-existing snapshot.
   */
  async fork(snapshotId: string, request: TetExecutionRequest): Promise<TetExecutionResult> {
    const res = await fetch(`${this.baseUrl}/v1/tet/fork/${snapshotId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Fork failed (${res.status}): ${errorText}`);
    }

    return res.json();
  }

  /**
   * Teleports an agent to another node in the hive.
   */
  async teleport(alias: string, targetNode: string): Promise<void> {
    const res = await fetch(`${this.baseUrl}/v1/tet/teleport/${alias}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(targetNode),
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Teleport failed (${res.status}): ${errorText}`);
    }
  }

  /**
   * Retrieves the live hive topology (connected peers).
   */
  async getTopology(): Promise<any> {
    const res = await fetch(`${this.baseUrl}/v1/topology`, {
      method: 'GET',
    });

    if (!res.ok) {
      throw new Error(`Failed to fetch topology`);
    }

    return res.json();
  }
}
