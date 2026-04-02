"use client";
import { SwarmProvider } from '../providers/SwarmProvider';
import { useSwarm } from '../providers/SwarmProvider';
import { useTetEngine } from '../hooks/useTetEngine';
import { useEffect, useState } from 'react';

import { PulseMap } from '../components/PulseMap';
import { LedgerTerm } from '../components/LedgerTerm';
import { FuelGauge } from '../components/FuelGauge';
import { OnboardingModal } from '../components/OnboardingModal';
import { AnimatePresence } from 'framer-motion';

function Dashboard() {
    const { connected, triggerTeleport, lastSnapshotPayload, telemetryFrames } = useSwarm();
    const engine = useTetEngine();
    const [showModal, setShowModal] = useState(true);

    useEffect(() => {
        if (lastSnapshotPayload && engine.initialized) {
            console.log("Hydrating downloaded snapshot payload! Size: ", lastSnapshotPayload.byteLength);
            engine.hydrateSnapshot(lastSnapshotPayload);
        }
    }, [lastSnapshotPayload, engine.initialized, engine.hydrateSnapshot]);

    const handleHydrate = () => {
        triggerTeleport("agent-1");
        setShowModal(false);
    };

    return (
        <div className="flex flex-col h-screen text-zinc-100 p-4 md:p-8 font-mono relative overflow-hidden">
            <AnimatePresence>
                {showModal && <OnboardingModal onHydrate={handleHydrate} />}
            </AnimatePresence>

            <header className="glass-panel rounded-2xl flex justify-between items-center mb-6 px-6 py-4 flex-shrink-0 relative z-10 w-full">
                <div className="flex items-center space-x-3">
                    <div className="w-8 h-8 rounded-full bg-teal-500/20 flex items-center justify-center border border-teal-500/30">
                        <div className={`w-3 h-3 rounded-full ${connected ? 'bg-teal-400 shadow-[0_0_10px_#2dd4bf]' : 'bg-zinc-600'}`} />
                    </div>
                    <h1 className="text-2xl font-bold tracking-widest uppercase text-white drop-shadow-md">Sovereign Playground</h1>
                </div>
                <div className="flex items-center space-x-4">
                    <span className="text-sm text-zinc-400 font-semibold tracking-wider">
                        STATUS: <span className={connected ? "text-teal-400" : "text-zinc-500"}>{connected ? 'LINK ESTABLISHED' : 'OFFLINE'}</span>
                    </span>
                    {!engine.initialized && !showModal && <span className="text-xs text-red-500 border border-red-500/30 px-2 py-1 rounded bg-red-500/10">WASM Offline</span>}
                </div>
            </header>

            <main className="flex-grow grid grid-cols-1 lg:grid-cols-3 gap-6 overflow-hidden mb-6 relative z-10">
                {/* 1. Swarm Pulse (Center) */}
                <div className="glass-panel p-1 rounded-2xl lg:col-span-2 relative h-full flex flex-col">
                    <PulseMap />
                </div>

                {/* 2. Ledger Term (Right) */}
                <div className="glass-panel p-1 rounded-2xl h-full relative flex flex-col">
                    <LedgerTerm logs={engine.logs} />
                </div>
            </main>

            {/* 3. Fuel Gauge (Bottom) */}
            <footer className="glass-panel p-4 rounded-2xl flex-shrink-0 relative z-10">
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
