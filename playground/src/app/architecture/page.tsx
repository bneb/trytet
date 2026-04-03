"use client";

import React from 'react';
import { Navbar } from '../../components/Navbar';

export default function ArchitecturePage() {
    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <Navbar />

            <div className="w-full flex justify-center">
                <header className="pt-24 pb-16 px-10 w-full max-w-[1000px]">
                    <h1 className="text-[clamp(40px,8vw,72px)] font-bold tracking-tight mb-6 leading-[1.1]">
                        The Single-Page Universe <br/>
                        <span className="text-transparent bg-clip-text bg-gradient-to-br from-[var(--text-main)] from-70% to-[rgba(175,82,222,0.6)]">64KB Sovereignty.</span>
                    </h1>
                    <p className="text-[20px] text-[var(--text-sub)] max-w-[700px] mb-10">
                        To understand how an agent can shrink to 64KB, we have to stop thinking like "Cloud Engineers" (who think in Gigabytes) and start thinking like "Systems Architects" (who think in Pages).
                    </p>
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    <div className="grid grid-cols-12 gap-8">
                        
                        {/* Section 1: The Wasm Atomic Unit */}
                        <section className="col-span-12 p-8 rounded-2xl bg-[var(--card-bg)] border border-[var(--card-border)] shadow-sm">
                            <div className="text-[11px] font-mono uppercase tracking-[0.1em] text-[var(--text-sub)] mb-6 flex items-center gap-2 after:content-[''] after:flex-1 after:h-[1px] after:bg-[var(--card-border)]">
                                1. The Atomic Unit: The Wasm Page
                            </div>
                            <div className="space-y-6">
                                <p className="text-lg text-[var(--text-sub)] leading-relaxed">
                                    WebAssembly (Wasm) does not allocate memory byte-by-byte like a standard process. It allocates memory in fixed-size blocks called <strong className="text-[var(--text-main)]">Pages</strong>.
                                </p>
                                <div className="inline-block px-4 py-2 bg-[var(--card-border)] rounded-lg font-mono text-[var(--text-main)] text-sm">
                                    1 Wasm Page = Exactly 64 KiB (2<sup>16</sup> bytes)
                                </div>
                                <p className="text-lg text-[var(--text-sub)] leading-relaxed">
                                    In the Trytet ecosystem, 64KB isn't just a random number—it is the fundamental unit of existence. When we say an agent takes up 64KB, we are saying it is living in a <strong className="text-[var(--text-main)]">Single-Page Universe</strong>. Everything the agent "is"—its stack, its small heap, and its current instruction pointer—fits into that one architectural tile.
                                </p>
                            </div>
                        </section>

                        {/* Section 2: Soul vs. Library */}
                        <section className="col-span-12 p-8 rounded-2xl bg-[var(--card-bg)] border border-[var(--card-border)] shadow-sm">
                            <div className="text-[11px] font-mono uppercase tracking-[0.1em] text-[var(--text-sub)] mb-6 flex items-center gap-2 after:content-[''] after:flex-1 after:h-[1px] after:bg-[var(--card-border)]">
                                2. "The Soul vs. The Library"
                            </div>
                            <p className="text-lg text-[var(--text-sub)] leading-relaxed mb-8">
                                The biggest misconception is that the LLM (the model) is inside the 64KB. It isn't. In a Sovereign architecture, we separate the <strong className="text-[var(--text-main)]">Logic</strong> from the <strong className="text-[var(--text-main)]">Weights</strong>:
                            </p>
                            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                                <div className="p-6 rounded-xl bg-[rgba(var(--accent-blue-rgb),0.05)] border border-[var(--card-border)]">
                                    <h3 className="text-[var(--accent-blue)] font-bold mb-3">The Model (Weights)</h3>
                                    <p className="text-sm text-[var(--text-sub)] leading-relaxed">
                                        These are the multi-gigabyte Llama-3 or Mistral files stored on the Host Node. They are shared by all agents.
                                    </p>
                                </div>
                                <div className="p-6 rounded-xl bg-[rgba(var(--accent-magenta-rgb),0.05)] border border-[var(--card-border)]">
                                    <h3 className="text-[var(--accent-magenta)] font-bold mb-3">The Agent (The Tet)</h3>
                                    <p className="text-sm text-[var(--text-sub)] leading-relaxed">
                                        This is just the "Current Thought." It contains the specific prompt, the local variables, and the logic to call <code className="px-1.5 py-0.5 bg-[var(--card-border)] rounded text-[var(--text-main)]">trytet::model_predict</code>.
                                    </p>
                                </div>
                            </div>
                            <p className="text-lg text-[var(--text-sub)] leading-relaxed mt-8">
                                By stripping away the "Body" (the OS, the Model, the Python Runtime), the "Soul" of the agent becomes incredibly light.
                            </p>
                        </section>

                        {/* Section 3: Hard-Tech Constraints */}
                        <section className="col-span-12 p-8 rounded-2xl bg-[var(--card-bg)] border border-[var(--card-border)] shadow-sm">
                            <div className="text-[11px] font-mono uppercase tracking-[0.1em] text-[var(--text-sub)] mb-6 flex items-center gap-2 after:content-[''] after:flex-1 after:h-[1px] after:bg-[var(--card-border)]">
                                3. How We Achieve the "Single-Page" Footprint
                            </div>
                            <p className="text-lg text-[var(--text-sub)] leading-relaxed mb-8">
                                To get a Rust-based agent down to 64KB, we apply three "Hard-Tech" constraints:
                            </p>
                            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                                <div className="space-y-3">
                                    <h4 className="font-bold text-[var(--text-main)]">A. no_std and Tiny Allocators</h4>
                                    <p className="text-sm text-[var(--text-sub)] leading-relaxed">
                                        Standard Rust binaries include "bloat" (panic handling, string formatting). We compile using <code className="px-1.5 py-0.5 bg-[var(--card-border)] rounded text-[var(--text-main)]">#![no_std]</code> and specialized allocators like <code className="px-1.5 py-0.5 bg-[var(--card-border)] rounded text-[var(--text-main)]">wee_alloc</code>.
                                    </p>
                                </div>
                                <div className="space-y-3">
                                    <h4 className="font-bold text-[var(--text-main)]">B. Offloading "Knowledge" to the VFS</h4>
                                    <p className="text-sm text-[var(--text-sub)] leading-relaxed">
                                        An agent doesn't store 10,000 documents in RAM. It stores them in the Tiered LSM-Vector VFS. RAM handles the active thought; VFS handles the library of facts via <code className="px-1.5 py-0.5 bg-[var(--card-border)] rounded text-[var(--text-main)]">trytet::recall</code>.
                                    </p>
                                </div>
                                <div className="space-y-3">
                                    <h4 className="font-bold text-[var(--text-main)]">C. Context Router Pruning</h4>
                                    <p className="text-sm text-[var(--text-sub)] leading-relaxed">
                                        We aggressively prune conversation history. If the "Thought" starts to exceed memory limits, the Router truncates data, mathematically forcing the agent back into its 64KB "Straightjacket."
                                    </p>
                                </div>
                            </div>
                        </section>

                        {/* Section 4: The Physics of Teleportation */}
                        <section className="col-span-12 p-12 rounded-2xl bg-[var(--card-bg)] border border-[var(--card-border)] flex flex-col items-center text-center shadow-md overflow-hidden relative">
                            {/* Accent graphics */}
                            <div className="absolute top-0 right-0 w-64 h-64 bg-[rgba(var(--accent-magenta-rgb),0.03)] blur-3xl rounded-full translate-x-1/2 -translate-y-1/2" />
                            <div className="absolute bottom-0 left-0 w-64 h-64 bg-[rgba(var(--accent-blue-rgb),0.03)] blur-3xl rounded-full -translate-x-1/2 translate-y-1/2" />
                            
                            <div className="text-[11px] font-mono uppercase tracking-[0.1em] text-[var(--text-sub)] mb-8">
                                4. The "Teleportation" Advantage
                            </div>
                            
                            <div className="font-mono text-4xl font-bold tracking-tight mb-4 text-[var(--text-main)] px-6 py-4 bg-[var(--card-border)] rounded-2xl">
                                T<sub>teleport</sub> = Size / Bandwidth
                            </div>
                            <div className="text-xs uppercase tracking-widest text-[var(--text-sub)] mb-10">
                                The Physics of Sovereign Mobility
                            </div>
                            
                            <div className="max-w-[800px] space-y-6">
                                <p className="text-lg text-[var(--text-sub)] leading-relaxed">
                                    If an agent is <strong className="text-[var(--text-main)]">64KB</strong>, it can teleport across a standard 1 Gbps mesh in approximately <strong className="text-[var(--accent-green)]">0.5ms</strong>.
                                </p>
                                <p className="text-lg text-[var(--text-sub)] leading-relaxed">
                                    If an agent is <strong className="text-[var(--text-main)]">1GB</strong> (a standard Docker container), it takes <strong className="text-[var(--accent-magenta)]">8,000ms</strong>.
                                </p>
                                <p className="text-lg text-[var(--text-sub)] leading-relaxed pt-4 border-t border-[var(--card-border)]">
                                    By staying small, the agent becomes Instantaneous. It can hop from your phone to a cloud node to a specialized NPU in the time it takes for a single human eyelid to blink. That is the definition of <strong className="text-[var(--text-main)]">Sovereign Mobility</strong>.
                                </p>
                            </div>
                        </section>

                    </div>
                </main>
            </div>
        </div>
    );
}
