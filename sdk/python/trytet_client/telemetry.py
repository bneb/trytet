"""WebSocket telemetry stream for real-time Trytet observability."""

import asyncio
import json
from typing import Any, Callable, Dict, Set

import websockets
from websockets.asyncio.client import ClientConnection

from .models import TelemetryEvent

TelemetryCallback = Callable[[TelemetryEvent], None]


class TelemetryStream:
    """Async WebSocket client for the Trytet telemetry stream."""

    def __init__(self, ws_url: str):
        self._url = ws_url
        self._ws: ClientConnection | None = None
        self._listeners: Set[TelemetryCallback] = set()
        self._closed = False
        self._reconnect_delay = 1.0

    async def connect(self) -> None:
        """Connect and begin receiving telemetry events."""
        while not self._closed:
            try:
                async with websockets.connect(self._url) as ws:
                    self._ws = ws
                    self._reconnect_delay = 1.0
                    await self._read_loop(ws)
            except Exception:
                if not self._closed:
                    await asyncio.sleep(self._reconnect_delay)
                    self._reconnect_delay = min(self._reconnect_delay * 2, 30.0)

    async def _read_loop(self, ws: ClientConnection) -> None:
        async for message in ws:
            try:
                data = json.loads(message)
                event = TelemetryEvent(**data)
                await self._emit(event)
            except Exception:
                pass

    async def _emit(self, event: TelemetryEvent) -> None:
        for listener in list(self._listeners):
            try:
                listener(event)
            except Exception:
                pass

    def on_event(self, callback: TelemetryCallback) -> Callable[[], None]:
        """Register a callback. Returns an unregister function."""
        self._listeners.add(callback)
        return lambda: self._listeners.discard(callback)

    def close(self) -> None:
        """Close the stream and stop reconnecting."""
        self._closed = True
        self._listeners.clear()
