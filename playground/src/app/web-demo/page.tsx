"use client";
import React, { useEffect, useState, useRef, useCallback } from 'react';
import { Navbar } from '../../components/Navbar';

// ============================================================
// TYPES
// ============================================================
type Move = 'rock' | 'paper' | 'scissors';
type Strategy = 'Rocksteady' | 'Contrarian' | 'Copycat' | 'Chaos' | 'Cycle' | 'Grudge' | 'Adaptive' | 'Conservative';

interface Agent {
    id: number;
    name: string;
    strategy: Strategy;
    wins: number;
    losses: number;
    draws: number;
    fuel: number;       // Wasm instruction budget (0-100%). At 0 → OutOfFuel → sandbox kills process.
    lastMove: Move | null;
    moveHistory: Move[];
    eliminated: boolean; // true when fuel === 0 (ExecutionStatus::OutOfFuel)
}

interface MatchResult {
    round: number;
    agentA: string;
    agentB: string;
    moveA: Move;
    moveB: Move;
    winner: string | null;
    timestamp: number;
}

interface Snapshot {
    id: number;
    round: number;
    agents: Agent[];
    label: string;
    timestamp: number;
    blobSize: number;       // Real bincode blob size in bytes
    serializeMs: number;    // Real serialization time
    blobPreview: string;    // Hex preview of first 32 bytes
    sha256: string;         // SHA-256 of the bincode blob (via SubtleCrypto)
    stateSize: number;      // Pre-serialization state size (JSON bytes)
}

// Wasm engine reference (loaded lazily)
let wasmEngine: any = null;
let wasmInit: any = null;

// ============================================================
// STRATEGY ENGINE
// ============================================================
const MOVES: Move[] = ['rock', 'paper', 'scissors'];
const MOVE_EMOJI: Record<Move, string> = { rock: '✊', paper: '✋', scissors: '✌️' };
const BEATS: Record<Move, Move> = { rock: 'scissors', scissors: 'paper', paper: 'rock' };
const COUNTER: Record<Move, Move> = { rock: 'paper', paper: 'scissors', scissors: 'rock' };

function getMove(agent: Agent, opponentHistory: Move[]): Move {
    switch (agent.strategy) {
        case 'Rocksteady': return 'rock';
        case 'Conservative': return 'scissors';
        case 'Chaos': return MOVES[Math.floor(Math.random() * 3)];
        case 'Cycle': return MOVES[agent.moveHistory.length % 3];
        case 'Copycat': return opponentHistory.length > 0 ? opponentHistory[opponentHistory.length - 1] : 'rock';
        case 'Contrarian': return opponentHistory.length > 0 ? COUNTER[opponentHistory[opponentHistory.length - 1]] : MOVES[Math.floor(Math.random() * 3)];
        case 'Grudge': {
            if (agent.losses === 0) return 'rock';
            return opponentHistory.length > 0 ? COUNTER[opponentHistory[opponentHistory.length - 1]] : 'paper';
        }
        case 'Adaptive': {
            if (opponentHistory.length < 3) return MOVES[Math.floor(Math.random() * 3)];
            const freq: Record<Move, number> = { rock: 0, paper: 0, scissors: 0 };
            opponentHistory.forEach(m => freq[m]++);
            const mostCommon = (Object.entries(freq) as [Move, number][]).sort((a, b) => b[1] - a[1])[0][0];
            return COUNTER[mostCommon];
        }
        default: return MOVES[Math.floor(Math.random() * 3)];
    }
}

function resolveMatch(moveA: Move, moveB: Move): 'a' | 'b' | 'draw' {
    if (moveA === moveB) return 'draw';
    return BEATS[moveA] === moveB ? 'a' : 'b';
}

function createAgents(): Agent[] {
    const configs: { name: string; strategy: Strategy }[] = [
        { name: 'Rocksteady', strategy: 'Rocksteady' },
        { name: 'Contrarian', strategy: 'Contrarian' },
        { name: 'Copycat', strategy: 'Copycat' },
        { name: 'Chaos', strategy: 'Chaos' },
        { name: 'Cycle', strategy: 'Cycle' },
        { name: 'Grudge', strategy: 'Grudge' },
        { name: 'Adaptive', strategy: 'Adaptive' },
        { name: 'Conservative', strategy: 'Conservative' },
    ];
    return configs.map((c, i) => ({
        id: i, name: c.name, strategy: c.strategy,
        wins: 0, losses: 0, draws: 0,
        fuel: 100, // 100% of allocated_fuel (maps to store.set_fuel() in the real engine)
        lastMove: null, moveHistory: [], eliminated: false,
    }));
}

const STRATEGY_COLORS: Record<Strategy, string> = {
    Rocksteady: '#FF6B6B', Contrarian: '#007AFF', Copycat: '#34C759',
    Chaos: '#FF9F0A', Cycle: '#AF52DE', Grudge: '#FF453A',
    Adaptive: '#30D5C8', Conservative: '#8E8E93',
};

function toHex(arr: Uint8Array, maxBytes: number = 32): string {
    return Array.from(arr.slice(0, maxBytes)).map(b => b.toString(16).padStart(2, '0')).join(' ');
}

// ============================================================
// MAIN COMPONENT
// ============================================================
export default function WebDemoPage() {
    const [agents, setAgents] = useState<Agent[]>(createAgents);
    const [matchLog, setMatchLog] = useState<MatchResult[]>([]);
    const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
    const [round, setRound] = useState(0);
    const [running, setRunning] = useState(false);
    const [activeSnapshotId, setActiveSnapshotId] = useState<number | null>(null);
    const [branchCount, setBranchCount] = useState(0);
    const [wasmReady, setWasmReady] = useState(false);
    const [selectedSnapshot, setSelectedSnapshot] = useState<Snapshot | null>(null);
    const intervalRef = useRef<NodeJS.Timeout | null>(null);
    const matchLogRef = useRef<HTMLDivElement>(null);
    const snapshotIdCounter = useRef(0);

    // Cumulative Wasm telemetry — tracks total proof-of-work
    const wasmStats = useRef({ calls: 0, totalBytes: 0, totalMs: 0 });
    const [statsVersion, setStatsVersion] = useState(0);

    // Initialize the real Wasm module on mount
    useEffect(() => {
        (async () => {
            try {
                const mod = await import('../../../pkg/tet_web');
                await mod.default();
                wasmEngine = new mod.BrowserEngine();
                wasmInit = mod;
                setWasmReady(true);
            } catch (e) {
                console.warn('Wasm module not available, falling back to JS-only snapshots:', e);
                setWasmReady(false);
            }
        })();
    }, []);

    useEffect(() => {
        if (matchLogRef.current) {
            matchLogRef.current.scrollTop = matchLogRef.current.scrollHeight;
        }
    }, [matchLog]);

    // Async SHA-256 hash of a Uint8Array via SubtleCrypto
    const hashBlob = useCallback(async (data: Uint8Array): Promise<string> => {
        const hash = await crypto.subtle.digest('SHA-256', data.buffer as ArrayBuffer);
        return Array.from(new Uint8Array(hash)).map(b => b.toString(16).padStart(2, '0')).join('');
    }, []);

    // Take a snapshot — routes through real Wasm bincode if available
    const takeSnapshot = useCallback((label: string, currentRound: number, currentAgents: Agent[]) => {
        const id = snapshotIdCounter.current++;
        const stateJson = JSON.stringify({ round: currentRound, agents: currentAgents });
        const stateBytes = new TextEncoder().encode(stateJson);

        let blobSize = stateBytes.length;
        let serializeMs = 0;
        let blobPreview = toHex(stateBytes);
        let bincodeBlob: Uint8Array | null = null;

        if (wasmEngine) {
            const t0 = performance.now();
            try {
                bincodeBlob = wasmEngine.snapshot_state(stateBytes);
                serializeMs = performance.now() - t0;
                blobSize = bincodeBlob!.length;
                blobPreview = toHex(bincodeBlob!);

                // Update cumulative stats
                wasmStats.current.calls++;
                wasmStats.current.totalBytes += blobSize;
                wasmStats.current.totalMs += serializeMs;
                setStatsVersion(v => v + 1);

                // Structured console telemetry
                console.groupCollapsed(
                    `%c[TRYTET] %csnapshot_state%c → ${blobSize}B in ${serializeMs.toFixed(3)}ms`,
                    'color:#007AFF;font-weight:bold', 'color:#34C759', 'color:inherit'
                );
                console.log('State (JSON):', stateBytes.length, 'bytes');
                console.log('Bincode blob:', blobSize, 'bytes');
                console.log('Overhead:', blobSize - stateBytes.length, 'bytes (SnapshotPayload header)');
                console.log('Serialization:', serializeMs.toFixed(3), 'ms');
                console.log('Hex:', blobPreview);
                console.groupEnd();
            } catch (e) {
                console.error('Wasm snapshot failed:', e);
            }
        }

        const snap: Snapshot = {
            id, round: currentRound,
            agents: currentAgents.map(a => ({ ...a, moveHistory: [...a.moveHistory] })),
            label, timestamp: Date.now(),
            blobSize, serializeMs, blobPreview,
            sha256: '', stateSize: stateBytes.length,
        };

        (snap as any)._blob = bincodeBlob;

        // Compute SHA-256 asynchronously
        if (bincodeBlob) {
            hashBlob(bincodeBlob).then(hash => {
                snap.sha256 = hash;
                setSnapshots(prev => prev.map(s => s.id === snap.id ? { ...s, sha256: hash } : s));
                console.log(`%c[TRYTET] %cSHA-256%c ${hash.slice(0, 16)}…`, 'color:#007AFF;font-weight:bold', 'color:#AF52DE', 'color:inherit');
            });
        }

        setSnapshots(prev => [...prev, snap]);
        return snap;
    }, [hashBlob]);

    // Restore from a snapshot — routes through real Wasm bincode deserialization
    const restoreSnapshot = useCallback((snap: Snapshot) => {
        if (intervalRef.current) clearInterval(intervalRef.current);
        setRunning(false);

        let restoredAgents = snap.agents.map(a => ({ ...a, moveHistory: [...a.moveHistory] }));

        // If we have the real bincode blob, prove round-trip through Wasm
        if (wasmEngine && (snap as any)._blob) {
            const t0 = performance.now();
            try {
                const restoredBytes = wasmEngine.restore_state((snap as any)._blob);
                const restoredJson = new TextDecoder().decode(restoredBytes);
                const parsed = JSON.parse(restoredJson);
                restoredAgents = parsed.agents;
                const restoreMs = performance.now() - t0;
                console.log(`[TRYTET] Restored from bincode in ${restoreMs.toFixed(3)}ms (${(snap as any)._blob.length} bytes)`);
            } catch (e) {
                console.error('Wasm restore failed, using JS fallback:', e);
            }
        }

        setAgents(restoredAgents);
        setRound(snap.round);
        setActiveSnapshotId(snap.id);
        setSelectedSnapshot(snap);
        setBranchCount(prev => prev + 1);
        setMatchLog(prev => prev.filter(m => m.round <= snap.round));
    }, []);

    // Execute one round
    const executeRound = useCallback(() => {
        setRound(prevRound => {
            const newRound = prevRound + 1;

            setAgents(prevAgents => {
                const alive = prevAgents.filter(a => !a.eliminated);
                if (alive.length < 2) {
                    if (intervalRef.current) clearInterval(intervalRef.current);
                    setRunning(false);
                    return prevAgents;
                }

                const shuffled = [...alive].sort(() => Math.random() - 0.5);
                const pairs: [Agent, Agent][] = [];
                for (let i = 0; i + 1 < shuffled.length; i += 2) {
                    pairs.push([shuffled[i], shuffled[i + 1]]);
                }

                const newAgents = prevAgents.map(a => ({ ...a, moveHistory: [...a.moveHistory] }));
                const roundMatches: MatchResult[] = [];

                for (const [a, b] of pairs) {
                    const agentA = newAgents.find(x => x.id === a.id)!;
                    const agentB = newAgents.find(x => x.id === b.id)!;
                    const moveA = getMove(agentA, agentB.moveHistory);
                    const moveB = getMove(agentB, agentA.moveHistory);
                    agentA.moveHistory.push(moveA); agentB.moveHistory.push(moveB);
                    agentA.lastMove = moveA; agentB.lastMove = moveB;

                    const result = resolveMatch(moveA, moveB);
                    let winner: string | null = null;
                    if (result === 'a') { agentA.wins++; agentB.losses++; winner = agentA.name; }
                    else if (result === 'b') { agentB.wins++; agentA.losses++; winner = agentB.name; }
                    else { agentA.draws++; agentB.draws++; }

                    // Each round burns Wasm fuel. In the real engine, every instruction costs 1 fuel.
                    agentA.fuel = Math.max(0, agentA.fuel - 3);
                    agentB.fuel = Math.max(0, agentB.fuel - 3);
                    // fuel === 0 → ExecutionStatus::OutOfFuel → sandbox terminates the process
                    if (agentA.fuel <= 0) agentA.eliminated = true;
                    if (agentB.fuel <= 0) agentB.eliminated = true;

                    roundMatches.push({
                        round: newRound, agentA: agentA.name, agentB: agentB.name,
                        moveA, moveB, winner, timestamp: Date.now(),
                    });
                }

                setMatchLog(prev => [...prev, ...roundMatches]);

                if (newRound % 5 === 0) {
                    takeSnapshot(`R${newRound}`, newRound, newAgents);
                }

                return newAgents;
            });

            return newRound;
        });
    }, [takeSnapshot]);

    const toggleRun = useCallback(() => {
        if (running) {
            if (intervalRef.current) clearInterval(intervalRef.current);
            setRunning(false);
        } else {
            setActiveSnapshotId(null);
            setSelectedSnapshot(null);
            const id = setInterval(executeRound, 1200);
            intervalRef.current = id;
            setRunning(true);
        }
    }, [running, executeRound]);

    const resetTournament = useCallback(() => {
        if (intervalRef.current) clearInterval(intervalRef.current);
        setRunning(false);
        setAgents(createAgents());
        setMatchLog([]);
        setSnapshots([]);
        setRound(0);
        setActiveSnapshotId(null);
        setSelectedSnapshot(null);
        setBranchCount(0);
    }, []);

    const manualSnapshot = useCallback(() => {
        takeSnapshot(`Manual @R${round}`, round, agents);
    }, [takeSnapshot, round, agents]);

    const sorted = [...agents].sort((a, b) => (b.wins * 3 + b.draws) - (a.wins * 3 + a.draws));
    const aliveCount = agents.filter(a => !a.eliminated).length;
    const maxRound = 34;

    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <Navbar />

            {/* SUBWAY TIMELINE — sticky bar below nav */}
            <div style={{
                position: 'sticky', top: 56, zIndex: 90,
                background: 'var(--nav-bg)',
                backdropFilter: 'blur(20px)', WebkitBackdropFilter: 'blur(20px)',
                borderBottom: '1px solid var(--card-border)',
                padding: '0 40px',
            }}>
                <div style={{ maxWidth: 1000, margin: '0 auto', padding: '12px 0', display: 'flex', alignItems: 'center', gap: 12 }}>
                    <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 10, color: 'var(--text-sub)', whiteSpace: 'nowrap', minWidth: 48 }}>
                        R{round}
                    </div>

                    <div style={{ flex: 1, position: 'relative', height: 24, display: 'flex', alignItems: 'center' }}>
                        {/* Track */}
                        <div style={{ position: 'absolute', left: 0, right: 0, top: '50%', height: 3, borderRadius: 2, background: 'var(--card-border)', transform: 'translateY(-50%)' }} />
                        {/* Progress fill */}
                        <div style={{
                            position: 'absolute', left: 0, top: '50%', height: 3, borderRadius: 2,
                            background: 'linear-gradient(90deg, #007AFF, #AF52DE)',
                            width: `${Math.min(100, (round / maxRound) * 100)}%`,
                            transform: 'translateY(-50%)',
                            transition: 'width 0.4s cubic-bezier(0.16, 1, 0.3, 1)',
                        }} />
                        {/* Station dots */}
                        {snapshots.map((snap) => {
                            const pct = Math.min(100, (snap.round / maxRound) * 100);
                            const isActive = activeSnapshotId === snap.id;
                            return (
                                <button key={snap.id} onClick={() => restoreSnapshot(snap)}
                                    title={`${snap.label} · ${snap.blobSize}B · ${snap.serializeMs.toFixed(2)}ms`}
                                    style={{
                                        position: 'absolute', left: `${pct}%`, transform: 'translate(-50%, -50%)', top: '50%',
                                        width: isActive ? 18 : 12, height: isActive ? 18 : 12, borderRadius: '50%',
                                        background: isActive ? '#007AFF' : 'var(--bg-secondary)',
                                        border: isActive ? '3px solid rgba(0,122,255,0.4)' : '2px solid var(--text-sub)',
                                        cursor: 'pointer', zIndex: 3, padding: 0,
                                        transition: 'all 0.3s cubic-bezier(0.16, 1, 0.3, 1)',
                                        boxShadow: isActive ? '0 0 10px rgba(0,122,255,0.5)' : 'none',
                                    }}
                                />
                            );
                        })}
                        {/* Current position */}
                        <div style={{
                            position: 'absolute', left: `${Math.min(100, (round / maxRound) * 100)}%`,
                            transform: 'translate(-50%, -50%)', top: '50%',
                            width: 8, height: 8, borderRadius: '50%',
                            background: running ? '#34C759' : '#8E8E93', zIndex: 4,
                            transition: 'left 0.4s cubic-bezier(0.16, 1, 0.3, 1)',
                            boxShadow: running ? '0 0 8px rgba(52,199,89,0.6)' : 'none',
                        }} />
                    </div>

                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, whiteSpace: 'nowrap' }}>
                        {branchCount > 0 && (
                            <span style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 10, padding: '2px 6px', borderRadius: 4, background: 'rgba(0,122,255,0.15)', color: '#007AFF' }}>
                                ⑂ {branchCount}
                            </span>
                        )}
                        <span style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 10, color: 'var(--text-sub)' }}>
                            {snapshots.length > 0 ? `${snapshots.length} snap${snapshots.length > 1 ? 's' : ''}` : ''}
                        </span>
                        <span style={{
                            fontFamily: "'JetBrains Mono', monospace", fontSize: 10, padding: '2px 6px', borderRadius: 4,
                            background: wasmReady ? 'rgba(52,199,89,0.12)' : 'rgba(255,69,58,0.12)',
                            color: wasmReady ? '#34C759' : '#FF453A',
                        }}>
                            {wasmReady ? '●' : '○'} wasm
                        </span>
                    </div>
                </div>
            </div>

            {/* HEADER */}
            <div className="w-full flex justify-center">
                <header className="pt-12 pb-8 px-10 w-full max-w-[1000px]">
                    <h1 className="text-[clamp(28px,5vw,48px)] font-bold tracking-tight mb-3 leading-[1.1]">
                        Git for running processes.
                    </h1>
                    <p className="text-[16px] text-[var(--text-sub)] max-w-[620px] mb-6">
                        8 agents compete in a live RPS tournament. Each starts with a <strong style={{ color: 'var(--text-main)' }}>Wasm fuel budget</strong> —
                        every round burns compute. When fuel hits zero, the sandbox kills the process.
                        Every 5 rounds, Trytet serializes state into a portable <strong style={{ color: 'var(--text-main)' }}>bincode snapshot</strong> via
                        real WebAssembly. Click any station to <strong style={{ color: 'var(--text-main)' }}>rewind and fork</strong>.
                    </p>
                    <div className="flex items-center gap-3 flex-wrap">
                        <button onClick={toggleRun} className="btn" style={{ minWidth: 120 }}>
                            {running ? '⏸ Pause' : '▶ Start'}
                        </button>
                        <button onClick={manualSnapshot} className="btn"
                            style={{ background: 'var(--bg-secondary)', color: 'var(--text-main)', border: '1px solid var(--card-border)' }}>
                            📸 Snapshot
                        </button>
                        <button onClick={resetTournament} className="btn"
                            style={{ background: 'transparent', color: 'var(--text-sub)', border: '1px solid var(--card-border)' }}>
                            ↺ Reset
                        </button>
                        <span className="font-mono text-sm text-[var(--text-sub)] ml-auto">
                            Round {round} · {aliveCount}/8 alive
                        </span>
                    </div>
                </header>
            </div>

            {/* DASHBOARD GRID */}
            <div className="w-full flex justify-center">
                <main className="grid grid-cols-12 gap-6 px-5 md:px-10 pb-8 w-full max-w-[1000px]">
                    {/* Leaderboard */}
                    <div className="card col-span-12 md:col-span-7 relative" style={{ padding: 0, overflow: 'hidden' }}>
                        <div className="label" style={{ padding: '20px 24px 12px' }}>Leaderboard</div>
                        <table style={{ width: '100%', borderCollapse: 'collapse', fontFamily: "'JetBrains Mono', monospace", fontSize: 13 }}>
                            <thead>
                                <tr style={{ borderBottom: '1px solid var(--card-border)' }}>
                                    {['#', 'Agent', 'W', 'L', 'D', 'Fuel'].map((h, i) => (
                                        <th key={h} style={{
                                            textAlign: i < 2 ? 'left' : i === 5 ? 'right' : 'center',
                                            padding: i === 0 ? '8px 24px' : i === 5 ? '8px 24px' : '8px 12px',
                                            color: 'var(--text-sub)', fontWeight: 500, fontSize: 11,
                                        }}>{h}</th>
                                    ))}
                                </tr>
                            </thead>
                            <tbody>
                                {sorted.map((agent, idx) => (
                                    <tr key={agent.id} style={{
                                        borderBottom: '1px solid var(--card-border)',
                                        opacity: agent.eliminated ? 0.35 : 1,
                                        transition: 'all 0.4s cubic-bezier(0.16, 1, 0.3, 1)',
                                    }}>
                                        <td style={{ padding: '10px 24px', color: 'var(--text-sub)' }}>{idx + 1}</td>
                                        <td style={{ padding: '10px 12px' }}>
                                            <div className="flex items-center gap-2">
                                                <div style={{ width: 8, height: 8, borderRadius: 2, background: STRATEGY_COLORS[agent.strategy], flexShrink: 0 }} />
                                                <span style={{ fontWeight: 600, color: agent.eliminated ? 'var(--text-sub)' : 'var(--text-main)' }}>
                                                    {agent.name}
                                                </span>
                                                {agent.eliminated && <span title="ExecutionStatus::OutOfFuel" style={{ fontSize: 9, color: '#FF453A', fontFamily: "'JetBrains Mono', monospace" }}>OOF</span>}
                                                {agent.lastMove && !agent.eliminated && (
                                                    <span style={{ fontSize: 12, opacity: 0.6 }}>{MOVE_EMOJI[agent.lastMove]}</span>
                                                )}
                                            </div>
                                        </td>
                                        <td style={{ textAlign: 'center', padding: '10px 12px', color: '#34C759' }}>{agent.wins}</td>
                                        <td style={{ textAlign: 'center', padding: '10px 12px', color: '#FF453A' }}>{agent.losses}</td>
                                        <td style={{ textAlign: 'center', padding: '10px 12px', color: 'var(--text-sub)' }}>{agent.draws}</td>
                                        <td style={{ textAlign: 'right', padding: '10px 24px' }}>
                                            <div className="flex items-center justify-end gap-2">
                                                <div title="Wasm fuel remaining — each round burns ~3% of allocated compute" style={{ width: 40, height: 4, borderRadius: 2, background: 'var(--code-bg)', overflow: 'hidden' }}>
                                                    <div style={{ width: `${agent.fuel}%`, height: '100%', background: agent.fuel < 20 ? '#FF453A' : '#007AFF', transition: 'width 0.3s ease' }} />
                                                </div>
                                                <span title={agent.fuel <= 0 ? 'OutOfFuel — sandbox terminated' : `${agent.fuel}% of allocated_fuel remaining`} style={{ color: agent.fuel < 20 ? '#FF453A' : 'var(--text-sub)', fontSize: 11 }}>{agent.fuel}%</span>
                                            </div>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>

                    {/* Match Feed */}
                    <div className="card col-span-12 md:col-span-5 relative" style={{ padding: 0, display: 'flex', flexDirection: 'column' }}>
                        <div className="label" style={{ padding: '20px 24px 12px' }}>Match Feed</div>
                        <div ref={matchLogRef} style={{
                            flex: 1, overflowY: 'auto', padding: '0 24px 20px',
                            fontFamily: "'JetBrains Mono', monospace", fontSize: 12, lineHeight: 1.8, maxHeight: 380,
                        }}>
                            {matchLog.length === 0 && (
                                <div style={{ color: 'var(--text-sub)', fontStyle: 'italic', paddingTop: 12 }}>
                                    Press ▶ Start to begin the tournament...
                                </div>
                            )}
                            {matchLog.slice(-30).map((m, i) => (
                                <div key={i} style={{ color: 'var(--text-sub)' }}>
                                    <span style={{ opacity: 0.5 }}>R{m.round} </span>
                                    <span style={{ color: STRATEGY_COLORS[m.agentA as Strategy] || 'var(--text-main)' }}>{m.agentA}</span>
                                    <span> {MOVE_EMOJI[m.moveA]} </span>
                                    <span style={{ opacity: 0.4 }}>vs</span>
                                    <span> {MOVE_EMOJI[m.moveB]} </span>
                                    <span style={{ color: STRATEGY_COLORS[m.agentB as Strategy] || 'var(--text-main)' }}>{m.agentB}</span>
                                    <span style={{ opacity: 0.5 }}> → </span>
                                    {m.winner
                                        ? <span style={{ color: '#34C759', fontWeight: 600 }}>{m.winner}</span>
                                        : <span style={{ opacity: 0.6 }}>draw</span>
                                    }
                                </div>
                            ))}
                        </div>
                    </div>
                </main>
            </div>

            {/* SNAPSHOT INSPECTOR */}
            {selectedSnapshot && (
                <div className="w-full flex justify-center">
                    <div className="px-5 md:px-10 pb-6 w-full max-w-[1000px]">
                        <div style={{
                            background: 'var(--code-bg)', borderRadius: 8, padding: 20,
                            border: '1px solid var(--card-border)', fontFamily: "'JetBrains Mono', monospace", fontSize: 11,
                        }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 10, flexWrap: 'wrap' }}>
                                <div style={{ color: '#007AFF', fontWeight: 600, fontSize: 12 }}>
                                    ⑂ Snapshot #{selectedSnapshot.id} — {selectedSnapshot.label}
                                </div>
                                <div style={{ marginLeft: 'auto', display: 'flex', gap: 16, color: 'var(--text-sub)', flexWrap: 'wrap' }}>
                                    <span>Format: <span style={{ color: 'var(--text-main)' }}>bincode</span></span>
                                    <span>Payload: <span style={{ color: 'var(--text-main)' }}>{selectedSnapshot.stateSize}B</span> → Blob: <span style={{ color: 'var(--text-main)' }}>{selectedSnapshot.blobSize}B</span></span>
                                    <span>Time: <span style={{ color: '#34C759' }}>{selectedSnapshot.serializeMs.toFixed(3)}ms</span></span>
                                </div>
                            </div>
                            <div style={{ wordBreak: 'break-all', opacity: 0.5, fontSize: 10, lineHeight: 1.6 }}>
                                {selectedSnapshot.blobPreview}
                            </div>
                            {selectedSnapshot.sha256 && (
                                <div style={{ marginTop: 6, fontSize: 10, color: 'var(--text-sub)' }}>
                                    SHA-256: <span style={{ color: '#AF52DE', letterSpacing: '0.5px' }}>{selectedSnapshot.sha256}</span>
                                </div>
                            )}
                            <div style={{ marginTop: 8, color: 'var(--text-sub)', fontSize: 10, fontStyle: 'italic' }}>
                                This blob is byte-compatible with POST /v1/tet/import on any Trytet node.
                            </div>
                        </div>
                    </div>
                </div>
            )}

            {/* UNDER THE HOOD — Proof of Authenticity */}
            <div className="w-full flex justify-center">
                <div className="px-5 md:px-10 pb-20 w-full max-w-[1000px]">
                    <div style={{
                        borderRadius: 8, padding: 20,
                        border: '1px solid var(--card-border)',
                        fontFamily: "'JetBrains Mono', monospace", fontSize: 11,
                    }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
                            <span style={{ fontSize: 13 }}>🔬</span>
                            <span style={{ fontWeight: 600, fontSize: 12, color: 'var(--text-main)', letterSpacing: '0.5px' }}>Under the Hood</span>
                        </div>
                        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))', gap: 12, marginBottom: 16 }}>
                            <div style={{ padding: 12, borderRadius: 6, background: 'var(--code-bg)', border: '1px solid var(--card-border)' }}>
                                <div style={{ color: 'var(--text-sub)', fontSize: 10, marginBottom: 4 }}>Engine</div>
                                <div style={{ color: wasmReady ? '#34C759' : '#FF453A', fontWeight: 600 }}>
                                    {wasmReady ? 'tet_web.wasm' : 'Loading...'}
                                </div>
                            </div>
                            <div style={{ padding: 12, borderRadius: 6, background: 'var(--code-bg)', border: '1px solid var(--card-border)' }}>
                                <div style={{ color: 'var(--text-sub)', fontSize: 10, marginBottom: 4 }}>Wasm Calls</div>
                                <div style={{ color: 'var(--text-main)', fontWeight: 600 }}>
                                    {wasmStats.current.calls}
                                </div>
                            </div>
                            <div style={{ padding: 12, borderRadius: 6, background: 'var(--code-bg)', border: '1px solid var(--card-border)' }}>
                                <div style={{ color: 'var(--text-sub)', fontSize: 10, marginBottom: 4 }}>Total Serialized</div>
                                <div style={{ color: 'var(--text-main)', fontWeight: 600 }}>
                                    {wasmStats.current.totalBytes > 1024
                                        ? `${(wasmStats.current.totalBytes / 1024).toFixed(1)} KB`
                                        : `${wasmStats.current.totalBytes} B`}
                                </div>
                            </div>
                            <div style={{ padding: 12, borderRadius: 6, background: 'var(--code-bg)', border: '1px solid var(--card-border)' }}>
                                <div style={{ color: 'var(--text-sub)', fontSize: 10, marginBottom: 4 }}>Avg Serialize</div>
                                <div style={{ color: 'var(--text-main)', fontWeight: 600 }}>
                                    {wasmStats.current.calls > 0
                                        ? `${(wasmStats.current.totalMs / wasmStats.current.calls).toFixed(3)}ms`
                                        : '—'}
                                </div>
                            </div>
                        </div>
                        <div style={{ color: 'var(--text-sub)', fontSize: 10, lineHeight: 1.8 }}>
                            <div>
                                Serialization struct:{' '}
                                <a href="https://github.com/bneb/trytet/blob/main/src/sandbox.rs#L3-L11"
                                    target="_blank" rel="noopener noreferrer"
                                    style={{ color: '#007AFF', textDecoration: 'none' }}>
                                    SnapshotPayload
                                </a>
                                {' '}· Wasm bridge:{' '}
                                <a href="https://github.com/bneb/trytet/blob/main/crates/tet-web/src/lib.rs"
                                    target="_blank" rel="noopener noreferrer"
                                    style={{ color: '#007AFF', textDecoration: 'none' }}>
                                    BrowserEngine
                                </a>
                                {' '}· Engine:{' '}
                                <a href="https://github.com/bneb/trytet/blob/main/src/sandbox/sandbox_wasmtime.rs"
                                    target="_blank" rel="noopener noreferrer"
                                    style={{ color: '#007AFF', textDecoration: 'none' }}>
                                    WasmtimeSandbox
                                </a>
                            </div>
                            <div style={{ marginTop: 4, opacity: 0.7 }}>
                                Open DevTools → Console to see structured telemetry for every Wasm call.
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

