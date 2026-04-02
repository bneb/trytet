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
                    
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    <div className="flex flex-col gap-6 max-w-[800px]">
                        <p className="text-[18px] text-[var(--text-sub)] leading-relaxed">
                            <strong className="text-[var(--text-main)]">Context Limits & Crashes:</strong> Traditional AI agents lose their memory when the system crashes or API limits are hit. Trytet compiles agents into a <code>.tet</code> footprint, allowing zero-overhead snapshots, hibernation mid-thought, and seamless state resumption.
                        </p>
                        <p className="text-[18px] text-[var(--text-sub)] leading-relaxed">
                            <strong className="text-[var(--text-main)]">Untrusted Execution:</strong> Don't give an LLM root access to your machine. Trytet acts as an embedded zero-trust sandbox that spins up in sub-milliseconds with strict fuel accounting, isolating machine-generated code safely.
                        </p>
                        <p className="text-[18px] text-[var(--text-sub)] leading-relaxed">
                            <strong className="text-[var(--text-main)]">Edge Swarming:</strong> Compute should follow the data. Instantly migrate an active agent's linear memory across the globe via P2P primitives to query massive databases with zero latency.
                        </p>
                        
                        <div className="flex items-center gap-4 mt-6">
                            <Link href="/how-to">
                                <button className="btn">View Use Cases</button>
                            </Link>
                            <a href="https://github.com/bneb/trytet" target="_blank" rel="noreferrer">
                                <button className="btn" style={{ background: 'transparent', border: '1px solid var(--card-border)', color: 'var(--text-main)' }}>GitHub</button>
                            </a>
                        </div>
                    </div>
                </main>
            </div>
        </div>
    );
}
