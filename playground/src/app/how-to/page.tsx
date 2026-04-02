"use client";
import React from 'react';
import { Navbar } from '../../components/Navbar';

export default function HowToPage() {
    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <Navbar />

            <div className="w-full flex justify-center">
                <header className="pt-24 pb-12 px-10 w-full max-w-[1000px]">
                    <h1 className="text-[clamp(32px,6vw,56px)] font-bold tracking-tight mb-4 leading-[1.1]">
                        Architecting stateful agents.
                    </h1>
                    <p className="text-[18px] text-[var(--text-sub)] max-w-[700px]">
                        Trytet is an embeddable Wasm substrate. It handles the heavily-constrained problems of agent architecture natively: resolving context crashes, isolating untrusted code, and serializing state for geographic transit.
                    </p>
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="grid grid-cols-12 gap-10 px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    
                    {/* Use Case 1 */}
                    <section className="col-span-12 grid grid-cols-1 md:grid-cols-2 gap-8 items-start">
                        <div>
                            <h2 className="text-2xl font-bold tracking-tight mb-4">1. Persistent Agent Memory <br/><span className="text-[var(--text-sub)] text-xl font-medium">(LangChain Integration)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                Agents crash. APIs rate-limit. Compiling the workflow into a deterministic <code>.tet</code> binary solves state retention.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Determinism:</strong> Snapshot linear memory at zero-overhead without dropping execution context.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Suspension:</strong> Hibernate memory instantly when hitting network await limits. Awaken cleanly.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Forking:</strong> If an LLM hallucinates a bad trajectory, fork its timeline back to the exact byte before the decision point without re-running initialization.</span>
                                </li>
                            </ul>
                        </div>
                        <div className="code-block w-full">
<pre><code><span className="comment text-[var(--code-comment)]">// Wrap the agent in a deterministic runtime</span><br/>
<span className="keyword text-[var(--electric-blue)]">import</span> {'{'} TrytetRuntime {'}'} <span className="keyword text-[var(--electric-blue)]">from</span> <span className="string text-[var(--mint-success)]">'@trytet/sdk'</span>;<br/>
<br/>
<span className="keyword text-[var(--electric-blue)]">const</span> agent = <span className="keyword text-[var(--electric-blue)]">new</span> TrytetRuntime({'{'}<br/>
    wasmBinary: <span className="string text-[var(--mint-success)]">'./agent-core.tet'</span>,<br/>
    maxFuel: <span className="string text-[var(--mint-success)]">50000</span>, <span className="comment text-[var(--code-comment)]">// bounded cyclic limits</span><br/>
    onTimeout: <span className="string text-[var(--mint-success)]">'checkpoint'</span><br/>
{'}'});<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Execute with an ephemeral boundary</span><br/>
<span className="keyword text-[var(--electric-blue)]">const</span> execution = <span className="keyword text-[var(--electric-blue)]">await</span> agent.<span className="function text-[var(--magenta-teleport)]">run</span>({'{'} <br/>
    task: <span className="string text-[var(--mint-success)]">"Refactor this monolithic repo"</span> <br/>
{'}'});<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Branch execution timeline deterministically</span><br/>
<span className="keyword text-[var(--electric-blue)]">if</span> (execution.error) {'{'}<br/>
    <span className="keyword text-[var(--electric-blue)]">const</span> clone = <span className="keyword text-[var(--electric-blue)]">await</span> TrytetRuntime.<span className="function text-[var(--magenta-teleport)]">fork</span>(<br/>
        execution.snapshot_id<br/>
    );<br/>
    <span className="keyword text-[var(--electric-blue)]">await</span> clone.<span className="function text-[var(--magenta-teleport)]">continue</span>();<br/>
{'}'}</code></pre>
                        </div>
                    </section>

                    {/* Use Case 2 */}
                    <section className="col-span-12 grid grid-cols-1 md:grid-cols-2 gap-8 items-start mt-12">
                        <div className="code-block w-full order-2 md:order-1">
<pre><code><span className="keyword text-[var(--electric-blue)]">import</span> trytet<br/>
<br/>
<span className="comment text-[var(--code-comment)]"># Force mathematical bounds on the generator</span><br/>
env = trytet.<span className="function text-[var(--magenta-teleport)]">Environment</span>(<br/>
    max_memory_mb=<span className="string text-[var(--mint-success)]">128</span>,<br/>
    vfs_mounts={<span className="string text-[var(--mint-success)]">{'{'}"/workspace": "./isolated"{'}'}</span>},<br/>
    network_egress=<span className="keyword text-[var(--electric-blue)]">False</span><br/>
)<br/>
<br/>
<span className="comment text-[var(--code-comment)]"># Arbitrary volatile string</span><br/>
untrusted_code = <span className="string text-[var(--mint-success)]">"import os; os.system('rm -rf /')"</span><br/>
<br/>
<span className="comment text-[var(--code-comment)]"># Sandbox prevents hypervisor faults natively</span><br/>
<span className="keyword text-[var(--electric-blue)]">try</span>:<br/>
    result = trytet.<span className="function text-[var(--magenta-teleport)]">spawn</span>(code=untrusted_code, env=env)<br/>
<span className="keyword text-[var(--electric-blue)]">except</span> trytet.SecurityViolation <span className="keyword text-[var(--electric-blue)]">as</span> e:<br/>
    <span className="function text-[var(--magenta-teleport)]">print</span>(f<span className="string text-[var(--mint-success)]">"Blocked: {'{'}e.reason{'}'}"</span>) <br/>
    <span className="comment text-[var(--code-comment)]"># Host maintains system integrity</span></code></pre>
                        </div>
                        <div className="order-1 md:order-2">
                            <h2 className="text-2xl font-bold tracking-tight mb-4">2. Untrusted Code Isolation <br/><span className="text-[var(--text-sub)] text-xl font-medium">(Zero-Trust Ephemerals)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                Letting an LLM read and write directly against a host VFS is fundamentally unsafe. Trytet avoids this by defaulting to a zero-trust perimeter block.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Instant Spin-Up:</strong> Sub-millisecond initialization maps tightly to dynamic evaluation loops where Docker latency breaks UX.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Algorithmic Fuel:</strong> Strict cyclic unit limitation prevents intentionally generated OOMs and infinite infinite loops.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Defensive Boundary:</strong> Run unsafe generated python output instantly safely mapped to virtualized geometry, not local drives.</span>
                                </li>
                            </ul>
                        </div>
                    </section>

                    {/* Use Case 3 */}
                    <section className="col-span-12 grid grid-cols-1 md:grid-cols-2 gap-8 items-start mt-12">
                        <div>
                            <h2 className="text-2xl font-bold tracking-tight mb-4">3. P2P Compute Topology <br/><span className="text-[var(--text-sub)] text-xl font-medium">(Zero-Latency Swarming)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                Data is heavy. Compute is light. Rather than streaming multi-gigabyte vector payloads over standard HTTP sockets to a remote evaluator, serialize the engine state directly.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Spatial Continuity:</strong> Transit the linear Wasm memory of a live <code>.tet</code> footprint onto an edge datacenter node locally hosting the vector stores.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Asymmetric Routing:</strong> Launch duplicated state copies through WebRTC primitives for distributed brute-force cognitive resolution.</span>
                                </li>
                            </ul>
                        </div>
                        <div className="code-block w-full">
<pre><code><span className="keyword text-[var(--electric-blue)]">use</span> trytet::mesh::{'{'}MeshNode, TeleportRequest{'}'};<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Bound the target edge node</span><br/>
<span className="keyword text-[var(--electric-blue)]">let</span> eu_target = <span className="string text-[var(--mint-success)]">"edge_frankfurt_01"</span>;<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// O(1) linear memory and VFS freeze</span><br/>
<span className="keyword text-[var(--electric-blue)]">let</span> snapshot = local_engine.<span className="function text-[var(--magenta-teleport)]">take_snapshot</span>();<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// P2P transmission vector via raw byte chunks</span><br/>
node.<span className="function text-[var(--magenta-teleport)]">teleport</span>(TeleportRequest {'{'}<br/>
    target: eu_target,<br/>
    payload: snapshot,<br/>
    <span className="comment text-[var(--code-comment)]">// Resumes natively on the target hardware without</span><br/>
    <span className="comment text-[var(--code-comment)]">// resetting context windows.</span><br/>
{'}'}).<span className="keyword text-[var(--electric-blue)]">await</span>?;</code></pre>
                        </div>
                    </section>

                </main>
            </div>
        </div>
    );
}
