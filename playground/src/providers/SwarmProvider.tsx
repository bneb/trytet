"use client";
import React, { createContext, useContext, useEffect, useState, ReactNode } from 'react';

export type TelemetryFrame = {
    source: string;
    target: string;
    call_count: number;
    error_count: number;
    total_bytes: number;
};

export type SwarmContextType = {
    connected: boolean;
    telemetryFrames: TelemetryFrame[];
    triggerTeleport: (alias: string) => void;
    lastSnapshotPayload: Uint8Array | null;
};

const SwarmContext = createContext<SwarmContextType | undefined>(undefined);

export const SwarmProvider = ({ children }: { children: ReactNode }) => {
    const [ws, setWs] = useState<WebSocket | null>(null);
    const [connected, setConnected] = useState(false);
    const [telemetryFrames, setTelemetryFrames] = useState<TelemetryFrame[]>([]);
    const [lastSnapshotPayload, setLastSnapshotPayload] = useState<Uint8Array | null>(null);

    useEffect(() => {
        // Connect to Fly.io Tet Backend (Assumed local 3000 for development)
        const socket = new WebSocket('ws://127.0.0.1:3000/v1/swarm/stream');

        socket.onopen = () => setConnected(true);
        socket.onclose = () => setConnected(false);

        socket.onmessage = async (event) => {
            if (event.data instanceof Blob) {
                // Incoming Snapshot Payload
                const buffer = await event.data.arrayBuffer();
                setLastSnapshotPayload(new Uint8Array(buffer));
            } else if (typeof event.data === 'string') {
                try {
                    const data = JSON.parse(event.data);
                    if (data.type === 'telemetry') {
                        setTelemetryFrames(prev => [...prev.slice(-100), data.frame as TelemetryFrame]);
                    }
                } catch (e) {
                    console.error("Failed to parse socket message", e);
                }
            }
        };

        setWs(socket);

        return () => {
            socket.close();
        };
    }, []);

    const triggerTeleport = (alias: string) => {
        if (ws && connected) {
            ws.send(JSON.stringify({ type: 'request_teleport', alias }));
        }
    };

    return (
        <SwarmContext.Provider value={{ connected, telemetryFrames, triggerTeleport, lastSnapshotPayload }}>
            {children}
        </SwarmContext.Provider>
    );
};

export const useSwarm = () => {
    const ctx = useContext(SwarmContext);
    if (!ctx) throw new Error("useSwarm must be used within SwarmProvider");
    return ctx;
};
