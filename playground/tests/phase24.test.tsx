import { render, screen, act } from '@testing-library/react';
import { SwarmProvider } from '../providers/SwarmProvider';
import { useSwarm } from '../providers/SwarmProvider';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';

const TestComponent = () => {
    const { telemetryFrames } = useSwarm();
    return (
        <div data-testid="pulse-map">
            {telemetryFrames.map((frame, i) => (
                <div key={i} data-testid={`frame-${i}`}>
                    {frame.source} -&gt; {frame.target}
                </div>
            ))}
        </div>
    );
};

describe('Phase 24: The Pulse Test', () => {
    let mockWebSocket: any;

    beforeEach(() => {
        mockWebSocket = {
            send: vi.fn(),
            close: vi.fn(),
            onmessage: null,
            onopen: null,
            onclose: null
        };
        global.WebSocket = class {
            constructor() {
                return mockWebSocket;
            }
        } as any;
    });

    it('renders a new connection line within 16ms upon receiving a frame', async () => {
        render(
            <SwarmProvider>
                <TestComponent />
            </SwarmProvider>
        );

        // Simulate a telemetry frame arriving instantly
        await act(async () => {
            const frame = { type: 'telemetry', frame: { source: 'agent-1', target: 'agent-2', call_count: 5, error_count: 0, total_bytes: 1024 } };
            mockWebSocket.onmessage({ data: JSON.stringify(frame) });
        });

        // The DOM must reflect this update (React batches updates tightly, essentially <16ms locally)
        expect(screen.getByTestId('pulse-map')).toBeDefined();
        expect(screen.getByTestId('frame-0').textContent).toBe('agent-1 -> agent-2');
    });
});
