import { describe, it, expect, vi } from 'vitest';

function mockWorker() {
    let onmessage: ((e: MessageEvent) => void) | null = null;
    return {
        postMessage: (msg: { type: string; payload?: unknown }) => {
            if (msg.type === 'HYDRATE') {
                if (onmessage) {
                    onmessage({ data: { type: 'LOG', data: 'RESURRECTED: agent pre-migration state' } } as MessageEvent);
                }
            }
        },
        set onmessage(fn: (e: MessageEvent) => void) {
            onmessage = fn;
        },
        terminate: vi.fn(),
    };
}

describe('Phase 25: The Resurrection', () => {
    it('passes a valid snapshot to WebWorker and returns hydration logs', async () => {
        const worker = mockWorker();
        let loggedMessage = '';

        worker.onmessage = (e: MessageEvent) => {
            if (e.data.type === 'LOG') {
                loggedMessage = e.data.data;
            }
        };

        const mockPayload = new Uint8Array([1, 2, 3, 4]); // Valid dummy
        worker.postMessage({ type: 'HYDRATE', payload: mockPayload });

        expect(loggedMessage).toBe('RESURRECTED: agent pre-migration state');
    });
});
