"""Pydantic models for all Trytet API types."""

from typing import Optional, Union, List, Dict, Any
from pydantic import BaseModel, Field


class AgentManifestMetadata(BaseModel):
    name: str
    version: str
    author_pubkey: Optional[str] = None


class AgentManifestConstraints(BaseModel):
    max_memory_pages: int
    fuel_limit: int
    max_egress_bytes: int


class AgentManifestPermissions(BaseModel):
    can_egress: List[str] = Field(default_factory=list)
    can_persist: bool = False
    can_teleport: bool = False
    is_genesis_factory: bool = False
    can_fork: bool = False


class AgentManifest(BaseModel):
    metadata: AgentManifestMetadata
    constraints: AgentManifestConstraints
    permissions: AgentManifestPermissions


class FuelVoucher(BaseModel):
    tet_id: str
    fuel_limit: int
    nonce: int
    signature: List[int]


class EgressPolicy(BaseModel):
    allowed_domains: List[str]
    max_daily_bytes: int
    require_https: bool = True


class TetExecutionRequest(BaseModel):
    payload: Optional[List[int]] = None
    alias: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    injected_files: Optional[Dict[str, str]] = None
    allocated_fuel: Optional[int] = None
    max_memory_mb: Optional[int] = None
    parent_snapshot_id: Optional[str] = None
    target_function: Optional[str] = None
    call_depth: int = 0
    voucher: Optional[FuelVoucher] = None
    manifest: Optional[AgentManifest] = None
    egress_policy: Optional[EgressPolicy] = None


class StructuredTelemetry(BaseModel):
    stdout_lines: List[str] = Field(default_factory=list)
    stderr_lines: List[str] = Field(default_factory=list)
    memory_used_kb: int = 0


class CrashReport(BaseModel):
    error_type: str
    message: str
    instruction_offset: Optional[int] = None


ExecutionStatus = Union[
    str, Dict[str, Any]
]  # "Success" | "OutOfFuel" | ... | {"Crash": {...}}


class TetExecutionResult(BaseModel):
    tet_id: str
    status: Any  # ExecutionStatus
    telemetry: StructuredTelemetry
    execution_duration_us: int
    fuel_consumed: int
    mutated_files: Dict[str, str] = Field(default_factory=dict)
    migrated_to: Optional[str] = None


class SnapshotResponse(BaseModel):
    snapshot_id: str
    size_bytes: int


class TopologyEdge(BaseModel):
    source: str
    target: str
    latency_us: int
    bytes_transferred: int


class NorthstarReport(BaseModel):
    teleport_warp_us: int
    mitosis_constant_us: int
    oracle_fidelity_us: int
    market_evacuation_us: int
    cartridge_spinup_us: int
    timestamp: str


class CartridgeInvocation(BaseModel):
    component_id: str
    payload: str
    fuel: int
    max_memory_mb: int = 512


class CartridgeResult(BaseModel):
    output: str
    fuel_consumed: int
    duration_us: int


class McpTool(BaseModel):
    name: str
    description: str
    inputSchema: Dict[str, Any] = Field(default_factory=dict)


class McpResource(BaseModel):
    uri: str
    name: str
    description: Optional[str] = None
    mimeType: Optional[str] = None


class McpPromptArgument(BaseModel):
    name: str
    description: Optional[str] = None
    required: Optional[bool] = None


class McpPrompt(BaseModel):
    name: str
    description: Optional[str] = None
    arguments: Optional[List[McpPromptArgument]] = None


class TelemetryEvent(BaseModel):
    event_type: str
    tet_id: str
    timestamp_us: int
    data: Dict[str, Any] = Field(default_factory=dict)
