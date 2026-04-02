"use client";
import { SwarmProvider } from '../providers/SwarmProvider';
import { useSwarm } from '../providers/SwarmProvider';
import { useTetEngine } from '../hooks/useTetEngine';
import { useEffect, useState } from 'react';

import { PulseMap } from '../components/PulseMap';
import { LedgerTerm } from '../components/LedgerTerm';
import { FuelGauge } from '../components/FuelGauge';
import { OnboardingModal } from '../components/OnboardingModal';
import { Navbar } from '../components/Navbar';
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
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <AnimatePresence>
                {showModal && <OnboardingModal onHydrate={handleHydrate} />}
            </AnimatePresence>

            <Navbar />

            <div className="w-full flex justify-center">
                <header className="pt-24 pb-16 px-10 w-full max-w-[1000px]">
                    <h1 className="text-[clamp(40px,8vw,72px)] font-bold tracking-tight mb-6 leading-[1.1]">
                        Autonomous execution. <br/>
                        <span className="text-[var(--text-sub)]">Zero-trust substrate.</span>
                    </h1>
                    <p className="text-[20px] text-[var(--text-sub)] max-w-[600px] mb-10">
                        Trytet is an ephemeral, sub-millisecond runtime where AI agents seamlessly migrate, execute, and scale across native boundaries.
                    </p>
                    
                    <div className="flex items-center gap-4 text-sm font-mono text-[var(--text-sub)]">
                        STATUS: <span className={connected ? "text-[var(--mint-success)] font-bold" : "text-[var(--text-sub)]"}>
                            {connected ? 'LINK ESTABLISHED' : 'OFFLINE'}
                        </span>
                        {!engine.initialized && !showModal && <span className="text-red-500 bg-red-500/10 px-2 py-1 rounded ml-auto">WASM Offline</span>}
                    </div>
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="grid grid-cols-12 gap-6 px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    
                    {/* Fuel Gauge Card */}
                    <div className="card col-span-12 md:col-span-6 relative">
                        <div className="label">System Budget (Fuel)</div>
                        <FuelGauge />
                    </div>

                    {/* Pulse Map Card */}
                    <div className="card col-span-12 md:col-span-6 relative z-0">
                        <div className="label">State Migration Latency</div>
                        <PulseMap />
                    </div>

                    {/* Code Block / Ledger Term */}
                    <div className="code-block col-span-12 relative min-h-[300px]">
                        <LedgerTerm logs={engine.logs} />
                    </div>

                </main>
            </div>
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
