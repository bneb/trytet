"use client";
import React from 'react';
import Link from 'next/link';
import { Navbar } from '../components/Navbar';

export default function Page() {
    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
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
                        STATUS: <span className="text-[var(--mint-success)] font-bold">LINK ESTABLISHED</span>
                    </div>
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="grid grid-cols-12 gap-6 px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    
                    {/* Fuel Gauge Card - Static Representation */}
                    <div className="card col-span-12 md:col-span-6 relative">
                        <div className="label">System Budget (Fuel)</div>
                        <div className="w-full flex flex-col justify-center h-full">
                            <div className="font-mono text-[32px] mb-2">72.04%</div>
                            <div className="fuel-wrap w-full">
                                <div className="fuel-bar w-full">
                                    <div className="fuel-fill absolute top-0 left-0 transition-colors w-[72%]" />
                                </div>
                            </div>
                            <div className="flex justify-between text-xs text-[var(--text-sub)] mt-3">
                                <span>Process ID: trytet-worker-01</span>
                                <span className="text-[var(--mint-success)]">ACTIVE</span>
                            </div>
                        </div>
                    </div>

                    {/* Pulse Map Card - Static Representation */}
                    <div className="card col-span-12 md:col-span-6 relative z-0">
                        <div className="label">State Migration Latency</div>
                        <div className="w-full flex flex-col justify-center h-full">
                            <div className="teleport-stage relative w-full h-[140px] border border-[var(--card-border)]">
                                <div className="ripple" style={{ animationDelay: '0s' }} />
                                <div className="ripple" style={{ animationDelay: '0.6s' }} />
                                <div className="absolute z-20 font-mono text-[11px] font-semibold tracking-widest text-[var(--text-main)] pointer-events-none drop-shadow-md">
                                    SUB-MS PULSE
                                </div>
                            </div>
                            <div className="flex justify-between text-xs text-[var(--text-sub)] mt-3">
                                <span>Route: Edge_NYC → Edge_LON</span>
                                <span className="text-[var(--magenta-teleport)] font-mono">0.34ms</span>
                            </div>
                        </div>
                    </div>

                    {/* Code Block */}
                    <div className="code-block col-span-12 relative">
                        <pre><code><span className="comment text-[var(--code-comment)]">// Initialize an ephemeral process with its own budget</span><br/>
<span className="keyword text-[var(--electric-blue)]">const</span> tet = <span className="keyword text-[var(--electric-blue)]">await</span> Trytet.<span className="function text-[var(--magenta-teleport)]">spawn</span>({'{'}<br/>
    brain: <span className="string text-[var(--mint-success)]">"neural-v2"</span>,<br/>
    fuel: <span className="string text-[var(--mint-success)]">5000_UNIT</span><br/>
{'}'});<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Immediate cross-region state migration</span><br/>
<span className="keyword text-[var(--electric-blue)]">await</span> tet.<span className="function text-[var(--magenta-teleport)]">teleport</span>(<span className="string text-[var(--mint-success)]">"global-edge-tokyo"</span>);</code></pre>
                    </div>

                </main>
            </div>
        </div>
    );
}
