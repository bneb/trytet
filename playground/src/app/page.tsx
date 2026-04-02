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
                        <span className="text-transparent bg-clip-text bg-gradient-to-br from-[var(--text-main)] from-70% to-[rgba(52,199,89,0.6)]">Zero-trust substrate.</span>
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
                            <strong className="text-[var(--text-main)]">Context Limits & Crashes:</strong> Agents halt. APIs rate-limit. Trytet drops execution context into a deterministic <code>.tet</code> binary. Snapshot them instantly, hibernate mid-thought, and safely fork states without re-running initialization.
                        </p>
                        <p className="text-[18px] text-[var(--text-sub)] leading-relaxed">
                            <strong className="text-[var(--text-main)]">Untrusted Execution:</strong> Giving an LLM unverified host execution breaks security. We evaluate volatile code in an embedded, sub-millisecond sandbox wrapped with cryptographic fuel accounting.
                        </p>
                        <p className="text-[18px] text-[var(--text-sub)] leading-relaxed">
                            <strong className="text-[var(--text-main)]">Edge Swarming:</strong> Data is heavy. Compute is light. By serializing active Wasm memory, you can migrate live executing agents directly to edge nodes hosting your vector databases to eliminate latency.
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
