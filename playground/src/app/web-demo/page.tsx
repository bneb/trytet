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
    fuel: number;
    lastMove: Move | null;
    moveHistory: Move[];
    eliminated: boolean;
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
        wins: 0, losses: 0, draws: 0, fuel: 100,
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
            // Route through the REAL Wasm bincode serializer
            const t0 = performance.now();
            try {
                bincodeBlob = wasmEngine.snapshot_state(stateBytes);
                serializeMs = performance.now() - t0;
                blobSize = bincodeBlob!.length;
                blobPreview = toHex(bincodeBlob!);
            } catch (e) {
                console.error('Wasm snapshot failed:', e);
            }
        }

        const snap: Snapshot = {
            id, round: currentRound,
            agents: currentAgents.map(a => ({ ...a, moveHistory: [...a.moveHistory] })),
            label, timestamp: Date.now(),
            blobSize, serializeMs, blobPreview,
        };

        // Stash the real bincode blob on the snapshot object for restore
        (snap as any)._blob = bincodeBlob;

        setSnapshots(prev => [...prev, snap]);
        return snap;
    }, []);

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

                    agentA.fuel = Math.max(0, agentA.fuel - 3);
                    agentB.fuel = Math.max(0, agentB.fuel - 3);
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

    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <Navbar />

            <div className="w-full flex justify-center">
                <header className="pt-20 pb-10 px-10 w-full max-w-[1000px]">
                    <div className="flex items-center gap-3 mb-4 flex-wrap">
                        <div className="w-2 h-2 rounded-full animate-pulse" style={{ background: running ? '#34C759' : '#8E8E93' }} />
                        <span className="font-mono text-xs text-[var(--text-sub)] tracking-widest uppercase">
                            {running ? 'Tournament Active' : activeSnapshotId !== null ? `Restored → Snapshot #${activeSnapshotId}` : 'Ready'}
                        </span>
                        {branchCount > 0 && (
                            <span className="font-mono text-xs px-2 py-0.5 rounded" style={{ background: 'rgba(0,122,255,0.15)', color: '#007AFF' }}>
                                Branch #{branchCount}
                            </span>
                        )}
                        <span className="font-mono text-xs px-2 py-0.5 rounded ml-auto" style={{
                            background: wasmReady ? 'rgba(52,199,89,0.12)' : 'rgba(255,69,58,0.12)',
                            color: wasmReady ? '#34C759' : '#FF453A',
                        }}>
                            {wasmReady ? '● Wasm + Bincode Active' : '○ Wasm Loading...'}
                        </span>
                    </div>
                    <h1 className="text-[clamp(28px,5vw,48px)] font-bold tracking-tight mb-3 leading-[1.1]">
                        Git for running processes.
                    </h1>
                    <p className="text-[16px] text-[var(--text-sub)] max-w-[620px] mb-6">
                        8 agents compete in a live RPS tournament. Every 5 rounds, Trytet serializes their state
                        into a portable <strong style={{ color: 'var(--text-main)' }}>bincode snapshot</strong> via
                        real WebAssembly. Click any snapshot to <strong style={{ color: 'var(--text-main)' }}>rewind and fork</strong> —
                        the tournament resumes from that exact binary state, diverging into a new timeline.
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
                                                {agent.eliminated && <span style={{ fontSize: 10, color: '#FF453A' }}>☠</span>}
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
                                                <div style={{ width: 40, height: 4, borderRadius: 2, background: 'var(--code-bg)', overflow: 'hidden' }}>
                                                    <div style={{ width: `${agent.fuel}%`, height: '100%', background: agent.fuel < 20 ? '#FF453A' : '#007AFF', transition: 'width 0.3s ease' }} />
                                                </div>
                                                <span style={{ color: agent.fuel < 20 ? '#FF453A' : 'var(--text-sub)', fontSize: 11 }}>{agent.fuel}%</span>
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

            {/* State Timeline + Snapshot Inspector */}
            <div className="w-full flex justify-center">
                <div className="px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    <div className="card" style={{ padding: '20px 24px' }}>
                        <div className="label" style={{ marginBottom: 16 }}>State Timeline — Snapshot · Rewind · Fork</div>

                        {snapshots.length === 0 ? (
                            <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 12, color: 'var(--text-sub)' }}>
                                Snapshots appear every 5 rounds. Each one is a real bincode-serialized SnapshotPayload{wasmReady ? ' via WebAssembly' : ''}.
                            </div>
                        ) : (
                            <div>
                                {/* Timeline bar */}
                                <div style={{ display: 'flex', alignItems: 'center', position: 'relative', marginBottom: 16 }}>
                                    <div style={{ position: 'absolute', top: '50%', left: 0, right: 0, height: 2, background: 'var(--card-border)', transform: 'translateY(-50%)' }} />
                                    {snapshots.map((snap) => (
                                        <button key={snap.id} onClick={() => restoreSnapshot(snap)}
                                            title={`Restore to ${snap.label} (${snap.blobSize} bytes, ${snap.serializeMs.toFixed(2)}ms)`}
                                            style={{
                                                position: 'relative', flex: 1, display: 'flex', flexDirection: 'column',
                                                alignItems: 'center', background: 'none', border: 'none', cursor: 'pointer', padding: '8px 0', zIndex: 2,
                                            }}>
                                            <div style={{
                                                width: activeSnapshotId === snap.id ? 16 : 12,
                                                height: activeSnapshotId === snap.id ? 16 : 12,
                                                borderRadius: '50%',
                                                background: activeSnapshotId === snap.id ? '#007AFF' : 'var(--text-sub)',
                                                border: activeSnapshotId === snap.id ? '3px solid rgba(0,122,255,0.3)' : '2px solid var(--bg-primary)',
                                                transition: 'all 0.3s cubic-bezier(0.16, 1, 0.3, 1)',
                                                boxShadow: activeSnapshotId === snap.id ? '0 0 12px rgba(0,122,255,0.4)' : 'none',
                                            }} />
                                            <span style={{
                                                fontFamily: "'JetBrains Mono', monospace", fontSize: 10,
                                                color: activeSnapshotId === snap.id ? '#007AFF' : 'var(--text-sub)',
                                                marginTop: 6, whiteSpace: 'nowrap',
                                                fontWeight: activeSnapshotId === snap.id ? 600 : 400,
                                            }}>{snap.label}</span>
                                            <span style={{
                                                fontFamily: "'JetBrains Mono', monospace", fontSize: 9,
                                                color: 'var(--text-sub)', opacity: 0.6, marginTop: 2,
                                            }}>{snap.blobSize}B</span>
                                        </button>
                                    ))}
                                </div>

                                {/* Snapshot Inspector */}
                                {selectedSnapshot && (
                                    <div style={{
                                        background: 'var(--code-bg)', borderRadius: 8, padding: 16, marginBottom: 12,
                                        border: '1px solid var(--card-border)', fontFamily: "'JetBrains Mono', monospace", fontSize: 11,
                                    }}>
                                        <div style={{ color: '#007AFF', fontWeight: 600, marginBottom: 8, fontSize: 12 }}>
                                            Snapshot #{selectedSnapshot.id} — {selectedSnapshot.label}
                                        </div>
                                        <div style={{ color: 'var(--text-sub)', lineHeight: 1.8 }}>
                                            <div>Format: <span style={{ color: 'var(--text-main)' }}>bincode (SnapshotPayload)</span></div>
                                            <div>Blob size: <span style={{ color: 'var(--text-main)' }}>{selectedSnapshot.blobSize} bytes</span></div>
                                            <div>Serialize: <span style={{ color: '#34C759' }}>{selectedSnapshot.serializeMs.toFixed(3)}ms</span></div>
                                            <div style={{ marginTop: 8, wordBreak: 'break-all', opacity: 0.7, fontSize: 10 }}>
                                                {selectedSnapshot.blobPreview}
                                            </div>
                                            <div style={{ marginTop: 8, color: 'var(--text-sub)', fontSize: 10, fontStyle: 'italic' }}>
                                                This blob is byte-compatible with POST /v1/tet/import on any Trytet node.
                                            </div>
                                        </div>
                                    </div>
                                )}

                                <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 11, color: 'var(--text-sub)' }}>
                                    {snapshots.length} snapshot{snapshots.length > 1 ? 's' : ''} ·
                                    Click any point to rewind and fork into an alternate timeline
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
