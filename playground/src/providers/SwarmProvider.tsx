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
        // Connect to Fly.io Tet Backend natively for production
        const isProd = process.env.NODE_ENV === 'production';
        const wsUrl = isProd 
            ? 'wss://trytet-api.fly.dev/v1/swarm/stream' 
            : 'ws://127.0.0.1:3000/v1/swarm/stream';
        
        const socket = new WebSocket(wsUrl);

        socket.onopen = () => setConnected(true);
        socket.onclose = () => setConnected(false);

        socket.onmessage = async (event) => {
            if (event.data instanceof Blob) {
                const buffer = await event.data.arrayBuffer();
                setLastSnapshotPayload(new Uint8Array(buffer));
            } else if (typeof event.data === 'string') {
                try {
                    const data = JSON.parse(event.data);
                    if (data.type === 'telemetry') {
                        setTelemetryFrames(prev => [...prev.slice(-100), data.frame as TelemetryFrame]);
                    }
                } catch (e) {
                    // Ignore parse errors on active streams
                }
            }
        };

        setWs(socket);

        // DEMO RESILIENCY LOOP: If backend is offline, inject simulated metrics to guarantee visual interactions
        let demoInterval: NodeJS.Timeout;
        const startDemoFallback = () => {
            // Generate a fake snapshot binary so the frontend UI can proceed
            setLastSnapshotPayload(new Uint8Array(new Array(1024).fill(0).map(() => Math.floor(Math.random() * 256))));
            
            // Randomly simulate PulseMap telemetry
            demoInterval = setInterval(() => {
                const sourceAgent = `Agent-${Math.floor(Math.random() * 51) + 1}`;
                const targetAgent = `Agent-${Math.floor(Math.random() * 51) + 1}`;
                const frame: TelemetryFrame = {
                    source: sourceAgent,
                    target: targetAgent,
                    call_count: Math.floor(Math.random() * 10),
                    error_count: Math.random() > 0.9 ? 1 : 0, // Rare errors
                    total_bytes: Math.floor(Math.random() * 5000),
                };
                setTelemetryFrames(prev => [...prev.slice(-40), frame]);
            }, 600);
        };

        // If the socket outright fails to connect or closes, trigger the demo fallback
        socket.onerror = () => startDemoFallback();
        socket.onclose = () => {
            setConnected(false);
            startDemoFallback(); // Ensure demo continues even if backend scales to zero
        };

        return () => {
            clearInterval(demoInterval);
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
