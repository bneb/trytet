"""Trytet API client with retry, MCP support, and cartridge management."""

import asyncio
from typing import Any, Dict, List, Optional

import httpx

from .models import (
    CartridgeInvocation,
    CartridgeResult,
    McpPrompt,
    McpResource,
    McpTool,
    NorthstarReport,
    SnapshotResponse,
    TetExecutionRequest,
    TetExecutionResult,
    TopologyEdge,
)
from .telemetry import TelemetryStream


class TrytetClient:
    """Async HTTP client for the Trytet Engine API."""

    def __init__(
        self,
        base_url: str = "http://localhost:3000",
        max_retries: int = 3,
        initial_delay_ms: int = 100,
    ):
        self._base = base_url.rstrip("/")
        self._max_retries = max_retries
        self._initial_delay = initial_delay_ms / 1000.0
        self._client = httpx.AsyncClient(timeout=httpx.Timeout(30.0))

    async def __aenter__(self) -> "TrytetClient":
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self.close()

    async def close(self) -> None:
        await self._client.aclose()

    # -- retry helper -------------------------------------------------------

    async def _request(
        self,
        method: str,
        path: str,
        *,
        json_body: Any = None,
        retry_on_status: Optional[List[int]] = None,
    ) -> httpx.Response:
        statuses = retry_on_status or [502, 503, 504]
        delay = self._initial_delay

        for attempt in range(self._max_retries + 1):
            try:
                resp = await self._client.request(
                    method, f"{self._base}{path}", json=json_body
                )
                if resp.status_code in statuses and attempt < self._max_retries:
                    raise httpx.TransportError(f"Retryable status {resp.status_code}")
                return resp
            except (httpx.TransportError, httpx.ConnectError) as e:
                if attempt == self._max_retries:
                    raise
                await asyncio.sleep(delay)
                delay = min(delay * 2, 5.0)

        raise RuntimeError("unreachable")

    async def _post(self, path: str, body: Any = None) -> httpx.Response:
        return await self._request("POST", path, json_body=body)

    async def _get(self, path: str) -> httpx.Response:
        return await self._request("GET", path)

    async def _check(self, resp: httpx.Response) -> Dict[str, Any]:
        if not resp.is_success:
            text = resp.text[:500]
            raise RuntimeError(f"Request failed ({resp.status_code}): {text}")
        return resp.json()

    # -- Agent lifecycle ----------------------------------------------------

    async def execute(self, req: TetExecutionRequest) -> TetExecutionResult:
        resp = await self._post("/v1/tet/execute", req.model_dump(exclude_none=True))
        data = await self._check(resp)
        return TetExecutionResult(**data)

    async def snapshot(self, tet_id: str) -> SnapshotResponse:
        resp = await self._post(f"/v1/tet/snapshot/{tet_id}")
        data = await self._check(resp)
        return SnapshotResponse(**data)

    async def fork(
        self, snapshot_id: str, req: TetExecutionRequest
    ) -> TetExecutionResult:
        resp = await self._post(
            f"/v1/tet/fork/{snapshot_id}", req.model_dump(exclude_none=True)
        )
        data = await self._check(resp)
        return TetExecutionResult(**data)

    async def teleport(self, alias: str, target_node: str) -> None:
        resp = await self._post(f"/v1/tet/teleport/{alias}", target_node)
        await self._check(resp)

    async def get_topology(self) -> List[TopologyEdge]:
        resp = await self._get("/v1/topology")
        data = await self._check(resp)
        return [TopologyEdge(**e) for e in data]

    async def get_swarm_metrics(self) -> NorthstarReport:
        resp = await self._get("/v1/swarm/metrics")
        data = await self._check(resp)
        return NorthstarReport(**data)

    async def get_health(self) -> Dict[str, Any]:
        resp = await self._get("/health")
        return await self._check(resp)

    # -- Cartridge management -----------------------------------------------

    async def invoke_cartridge(self, invocation: CartridgeInvocation) -> CartridgeResult:
        resp = await self._post(
            "/v1/cartridge/invoke", invocation.model_dump()
        )
        data = await self._check(resp)
        return CartridgeResult(**data)

    # -- MCP ----------------------------------------------------------------

    async def _mcp_call(self, method: str, params: Any = None) -> Any:
        import time

        request = {
            "jsonrpc": "2.0",
            "id": int(time.time() * 1000),
            "method": method,
            "params": params,
        }
        resp = await self._post("/v1/mcp", request)
        data = await self._check(resp)
        return data.get("result")

    async def list_tools(self) -> List[McpTool]:
        result = await self._mcp_call("tools/list")
        return [McpTool(**t) for t in (result or {}).get("tools", [])]

    async def call_tool(self, name: str, arguments: Dict[str, Any]) -> Any:
        return await self._mcp_call("tools/call", {"name": name, "arguments": arguments})

    async def list_resources(self) -> List[McpResource]:
        result = await self._mcp_call("resources/list")
        return [McpResource(**r) for r in (result or {}).get("resources", [])]

    async def list_prompts(self) -> List[McpPrompt]:
        result = await self._mcp_call("prompts/list")
        return [McpPrompt(**p) for p in (result or {}).get("prompts", [])]

    # -- Telemetry ----------------------------------------------------------

    def create_telemetry_stream(self) -> TelemetryStream:
        ws_url = self._base.replace("http", "ws") + "/v1/swarm/stream"
        return TelemetryStream(ws_url)
