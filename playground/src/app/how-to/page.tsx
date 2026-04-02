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
                        Unleashing Autonomous Agents
                    </h1>
                    <p className="text-[18px] text-[var(--text-sub)] max-w-[700px]">
                        Trytet isn't just an execution sandbox; it is an embeddable compute substrate designed to solve the hardest problems in agentic workflows: context limits, untrusted code execution, and geographic latency.
                    </p>
                </header>
            </div>

            <div className="w-full flex justify-center">
                <main className="grid grid-cols-12 gap-10 px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    
                    {/* Use Case 1 */}
                    <section className="col-span-12 grid grid-cols-1 md:grid-cols-2 gap-8 items-start">
                        <div>
                            <h2 className="text-2xl font-bold tracking-tight mb-4">1. The "Indestructible" AI Agent <br/><span className="text-[var(--text-sub)] text-xl font-medium">(LangChain Integration)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                When running long-lived autonomous agents (like Devin or AutoGPT), system crashes, network failures, or API context limits usually mean starting from scratch. Trytet compiles agents into a persistent <code>.tet</code> footprint.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Zero-Overhead Snapshots:</strong> Agents can be snapshotted instantly without losing their thread or scratchpad memory.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Hibernation:</strong> Pause execution while waiting on external API results and awake exactly where you left off.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Seamless Resumption:</strong> Clone an agent mid-thought to try different decision branches simultaneously.</span>
                                </li>
                            </ul>
                        </div>
                        <div className="code-block w-full">
<pre><code><span className="comment text-[var(--code-comment)]">// Wrapping an autonomous agent in Trytet</span><br/>
<span className="keyword text-[var(--electric-blue)]">import</span> {'{'} TrytetRuntime {'}'} <span className="keyword text-[var(--electric-blue)]">from</span> <span className="string text-[var(--mint-success)]">'@trytet/sdk'</span>;<br/>
<br/>
<span className="keyword text-[var(--electric-blue)]">const</span> agent = <span className="keyword text-[var(--electric-blue)]">new</span> TrytetRuntime({'{'}<br/>
    wasmBinary: <span className="string text-[var(--mint-success)]">'./agent-core.tet'</span>,<br/>
    maxFuel: <span className="string text-[var(--mint-success)]">50000</span>, <span className="comment text-[var(--code-comment)]">// deterministic budget</span><br/>
    onTimeout: <span className="string text-[var(--mint-success)]">'checkpoint'</span><br/>
{'}'});<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Run the agent dynamically</span><br/>
<span className="keyword text-[var(--electric-blue)]">const</span> execution = <span className="keyword text-[var(--electric-blue)]">await</span> agent.<span className="function text-[var(--magenta-teleport)]">run</span>({'{'} <br/>
    task: <span className="string text-[var(--mint-success)]">"Refactor this monolithic repo"</span> <br/>
{'}'});<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Forking the agent exactly at crash state</span><br/>
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
<span className="comment text-[var(--code-comment)]"># Define strict boundaries before execution</span><br/>
env = trytet.<span className="function text-[var(--magenta-teleport)]">Environment</span>(<br/>
    max_memory_mb=<span className="string text-[var(--mint-success)]">128</span>,<br/>
    vfs_mounts={<span className="string text-[var(--mint-success)]">{'{'}"/workspace": "./isolated"{'}'}</span>},<br/>
    network_egress=<span className="keyword text-[var(--electric-blue)]">False</span><br/>
)<br/>
<br/>
<span className="comment text-[var(--code-comment)]"># Untrusted LLM generated string</span><br/>
untrusted_code = <span className="string text-[var(--mint-success)]">"import os; os.system('rm -rf /')"</span><br/>
<br/>
<span className="comment text-[var(--code-comment)]"># Try to run it in the ephemeral sandbox</span><br/>
<span className="keyword text-[var(--electric-blue)]">try</span>:<br/>
    result = trytet.<span className="function text-[var(--magenta-teleport)]">spawn</span>(code=untrusted_code, env=env)<br/>
<span className="keyword text-[var(--electric-blue)]">except</span> trytet.SecurityViolation <span className="keyword text-[var(--electric-blue)]">as</span> e:<br/>
    <span className="function text-[var(--magenta-teleport)]">print</span>(f<span className="string text-[var(--mint-success)]">"Blocked: {'{'}e.reason{'}'}"</span>) <br/>
    <span className="comment text-[var(--code-comment)]"># Responds gracefully, host is fully insulated</span></code></pre>
                        </div>
                        <div className="order-1 md:order-2">
                            <h2 className="text-2xl font-bold tracking-tight mb-4">2. Untrusted OS-Level Execution <br/><span className="text-[var(--text-sub)] text-xl font-medium">(The Zero-Trust Sandbox)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                Providing your local machine environment to LLM generators to test scripts exposes you to critical security risks. Trytet solves this entirely by acting as an embedded zero-trust runtime.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Safe Evaluators:</strong> Execute arbitrary, unverified code without spinning up heavy Docker containers.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Instant Isolation:</strong> Sub-millisecond boot times mean your agent can run hundreds of micro-tests locally without performance degradation.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Strict Accounting:</strong> Guaranteed CPU bounds ensure an LLM can't induce an infinite loop computationally stalling your system.</span>
                                </li>
                            </ul>
                        </div>
                    </section>

                    {/* Use Case 3 */}
                    <section className="col-span-12 grid grid-cols-1 md:grid-cols-2 gap-8 items-start mt-12">
                        <div>
                            <h2 className="text-2xl font-bold tracking-tight mb-4">3. Dynamic Multi-Agent Swarming <br/><span className="text-[var(--text-sub)] text-xl font-medium">(Zero-Latency Edge Execution)</span></h2>
                            <p className="text-[var(--text-sub)] leading-relaxed mb-6">
                                Instead of constantly streaming huge vectors of information to a centralized server, send the logic to where the data naturally lives. The engine natively serializes processes across the network.
                            </p>
                            <ul className="space-y-3">
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Compute Follows Data:</strong> Transmit an active <code>.tet</code> instance to an edge node to query its local Vector database, avoiding roundtrip latencies entirely.</span>
                                </li>
                                <li className="flex items-start">
                                    <span className="text-[var(--electric-blue)] font-bold mr-2">→</span>
                                    <span><strong>Swarm Intelligence:</strong> Duplicate an agent across regions from a single orchestrator node utilizing simple P2P primitives.</span>
                                </li>
                            </ul>
                        </div>
                        <div className="code-block w-full">
<pre><code><span className="keyword text-[var(--electric-blue)]">use</span> trytet::mesh::{'{'}MeshNode, TeleportRequest{'}'};<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Agent decides to migrate to Europe edge node</span><br/>
<span className="keyword text-[var(--electric-blue)]">let</span> eu_target = <span className="string text-[var(--mint-success)]">"edge_frankfurt_01"</span>;<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// Instantly serialize Heap/Wasm linear memory</span><br/>
<span className="keyword text-[var(--electric-blue)]">let</span> snapshot = local_engine.<span className="function text-[var(--magenta-teleport)]">take_snapshot</span>();<br/>
<br/>
<span className="comment text-[var(--code-comment)]">// P2P network transit over WebRTC/TCP</span><br/>
node.<span className="function text-[var(--magenta-teleport)]">teleport</span>(TeleportRequest {'{'}<br/>
    target: eu_target,<br/>
    payload: snapshot,<br/>
    <span className="comment text-[var(--code-comment)]">// Re-awakens on the other side exactly</span><br/>
    <span className="comment text-[var(--code-comment)]">// where the network call was initiated</span><br/>
{'}'}).<span className="keyword text-[var(--electric-blue)]">await</span>?;</code></pre>
                        </div>
                    </section>

                </main>
            </div>
        </div>
    );
}
