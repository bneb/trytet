"use client";
import { SwarmProvider } from '../providers/SwarmProvider';
import { useSwarm } from '../providers/SwarmProvider';
import { useTetEngine } from '../hooks/useTetEngine';
import { useEffect } from 'react';

import { PulseMap } from '../components/PulseMap';
import { LedgerTerm } from '../components/LedgerTerm';
import { FuelGauge } from '../components/FuelGauge';

function Dashboard() {
    const { connected, triggerTeleport, lastSnapshotPayload, telemetryFrames } = useSwarm();
    const engine = useTetEngine();

    useEffect(() => {
        if (lastSnapshotPayload && engine.initialized) {
            console.log("Hydrating downloaded snapshot payload! Size: ", lastSnapshotPayload.byteLength);
            engine.hydrateSnapshot(lastSnapshotPayload);
        }
    }, [lastSnapshotPayload, engine.initialized, engine.hydrateSnapshot]);

    return (
        <div className="flex flex-col h-screen bg-black text-[#00ff9d] p-8 font-mono">
            <header className="flex justify-between items-center mb-6 border-b border-[#fe2c55] pb-4 flex-shrink-0">
                <h1 className="text-3xl font-bold tracking-widest uppercase">Sovereign Playground</h1>
                <div className="flex items-center space-x-4">
                    <span className="text-sm">STATUS: {connected ? 'LINK ESTABLISHED' : 'OFFLINE'}</span>
                    <button 
                        onClick={() => triggerTeleport("agent-1")}
                        className="bg-[#fe2c55] text-white px-6 py-2 rounded-sm font-bold uppercase hover:bg-[#fe2c55]/80 transition-colors"
                    >
                        Initiate Teleport
                    </button>
                    {!engine.initialized && <span className="text-xs text-red-500">WASM Offline</span>}
                </div>
            </header>

            <main className="flex-grow grid grid-cols-3 gap-8 overflow-hidden mb-6">
                {/* 1. Swarm Pulse (Center) */}
                <div className="col-span-2 relative h-full flex flex-col">
                    <PulseMap />
                </div>

                {/* 2. Ledger Term (Right) */}
                <div className="h-full relative flex flex-col">
                    <LedgerTerm logs={engine.logs} />
                </div>
            </main>

            {/* 3. Fuel Gauge (Bottom) */}
            <footer className="flex-shrink-0">
                <FuelGauge />
            </footer>
        </div>
    );
}

export default function Page() {
    return (
        <SwarmProvider>
            <Dashboard />
        </SwarmProvider>
    );
}
