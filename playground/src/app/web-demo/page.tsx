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
    winner: string | null; // null = draw
    timestamp: number;
}

interface Snapshot {
    id: number;
    round: number;
    agents: Agent[];
    label: string;
    timestamp: number;
}

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
            // Start with rock, after any loss switch to whatever beats opponent's last
            const losses = agent.losses;
            if (losses === 0) return 'rock';
            return opponentHistory.length > 0 ? COUNTER[opponentHistory[opponentHistory.length - 1]] : 'paper';
        }
        case 'Adaptive': {
            if (opponentHistory.length < 3) return MOVES[Math.floor(Math.random() * 3)];
            // Count opponent frequencies and play the counter to their most common
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

// ============================================================
// INITIAL STATE
// ============================================================
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
        id: i,
        name: c.name,
        strategy: c.strategy,
        wins: 0, losses: 0, draws: 0,
        fuel: 100,
        lastMove: null,
        moveHistory: [],
        eliminated: false,
    }));
}

// ============================================================
// STRATEGY COLORS (for leaderboard)
// ============================================================
const STRATEGY_COLORS: Record<Strategy, string> = {
    Rocksteady: '#FF6B6B',
    Contrarian: '#007AFF',
    Copycat: '#34C759',
    Chaos: '#FF9F0A',
    Cycle: '#AF52DE',
    Grudge: '#FF453A',
    Adaptive: '#30D5C8',
    Conservative: '#8E8E93',
};

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
    const intervalRef = useRef<NodeJS.Timeout | null>(null);
    const matchLogRef = useRef<HTMLDivElement>(null);
    const snapshotIdCounter = useRef(0);

    // Auto-scroll match log
    useEffect(() => {
        if (matchLogRef.current) {
            matchLogRef.current.scrollTop = matchLogRef.current.scrollHeight;
        }
    }, [matchLog]);

    // Take a snapshot of current state
    const takeSnapshot = useCallback((label: string, currentRound: number, currentAgents: Agent[]) => {
        const id = snapshotIdCounter.current++;
        const snap: Snapshot = {
            id,
            round: currentRound,
            agents: currentAgents.map(a => ({ ...a, moveHistory: [...a.moveHistory] })),
            label,
            timestamp: Date.now(),
        };
        setSnapshots(prev => [...prev, snap]);
        return snap;
    }, []);

    // Restore from a snapshot
    const restoreSnapshot = useCallback((snap: Snapshot) => {
        // Stop the current run
        if (intervalRef.current) clearInterval(intervalRef.current);
        setRunning(false);

        // Deep-copy the snapshot state
        const restoredAgents = snap.agents.map(a => ({ ...a, moveHistory: [...a.moveHistory] }));
        setAgents(restoredAgents);
        setRound(snap.round);
        setActiveSnapshotId(snap.id);
        setBranchCount(prev => prev + 1);

        // Trim match log to this snapshot's round
        setMatchLog(prev => prev.filter(m => m.round <= snap.round));
    }, []);

    // Run one round of matches
    const executeRound = useCallback(() => {
        setAgents(prevAgents => {
            setRound(prevRound => {
                const newRound = prevRound + 1;
                const alive = prevAgents.filter(a => !a.eliminated);
                if (alive.length < 2) {
                    if (intervalRef.current) clearInterval(intervalRef.current);
                    setRunning(false);
                    return newRound;
                }

                // Pair agents randomly
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

                    agentA.moveHistory.push(moveA);
                    agentB.moveHistory.push(moveB);
                    agentA.lastMove = moveA;
                    agentB.lastMove = moveB;

                    const result = resolveMatch(moveA, moveB);
                    let winner: string | null = null;

                    if (result === 'a') {
                        agentA.wins++;
                        agentB.losses++;
                        winner = agentA.name;
                    } else if (result === 'b') {
                        agentB.wins++;
                        agentA.losses++;
                        winner = agentB.name;
                    } else {
                        agentA.draws++;
                        agentB.draws++;
                    }

                    // Fuel cost
                    agentA.fuel = Math.max(0, agentA.fuel - 3);
                    agentB.fuel = Math.max(0, agentB.fuel - 3);

                    if (agentA.fuel <= 0) agentA.eliminated = true;
                    if (agentB.fuel <= 0) agentB.eliminated = true;

                    roundMatches.push({
                        round: newRound,
                        agentA: agentA.name,
                        agentB: agentB.name,
                        moveA, moveB,
                        winner,
                        timestamp: Date.now(),
                    });
                }

                setMatchLog(prev => [...prev, ...roundMatches]);

                // Auto-snapshot every 5 rounds
                if (newRound % 5 === 0) {
                    takeSnapshot(`Round ${newRound}`, newRound, newAgents);
                }

                // Update agents
                setAgents(newAgents);
                return newRound;
            });
            return prevAgents; // setAgents is handled inside
        });
    }, [takeSnapshot]);

    // Start / pause the tournament
    const toggleRun = useCallback(() => {
        if (running) {
            if (intervalRef.current) clearInterval(intervalRef.current);
            setRunning(false);
        } else {
            setActiveSnapshotId(null);
            const id = setInterval(executeRound, 1200);
            intervalRef.current = id;
            setRunning(true);
        }
    }, [running, executeRound]);

    // Reset everything
    const resetTournament = useCallback(() => {
        if (intervalRef.current) clearInterval(intervalRef.current);
        setRunning(false);
        setAgents(createAgents());
        setMatchLog([]);
        setSnapshots([]);
        setRound(0);
        setActiveSnapshotId(null);
        setBranchCount(0);
    }, []);

    // Manual snapshot
    const manualSnapshot = useCallback(() => {
        takeSnapshot(`Manual @R${round}`, round, agents);
    }, [takeSnapshot, round, agents]);

    // Sort agents by wins descending
    const sorted = [...agents].sort((a, b) => {
        const scoreA = a.wins * 3 + a.draws;
        const scoreB = b.wins * 3 + b.draws;
        return scoreB - scoreA;
    });

    const aliveCount = agents.filter(a => !a.eliminated).length;

    return (
        <div className="flex flex-col min-h-screen relative overflow-x-hidden">
            <Navbar />

            <div className="w-full flex justify-center">
                <header className="pt-20 pb-10 px-10 w-full max-w-[1000px]">
                    <div className="flex items-center gap-3 mb-4">
                        <div className="w-2 h-2 rounded-full animate-pulse" style={{ background: running ? '#34C759' : '#8E8E93' }} />
                        <span className="font-mono text-xs text-[var(--text-sub)] tracking-widest uppercase">
                            {running ? 'Tournament Active' : activeSnapshotId !== null ? `Restored from Snapshot #${activeSnapshotId}` : 'Ready'}
                        </span>
                        {branchCount > 0 && (
                            <span className="font-mono text-xs px-2 py-0.5 rounded" style={{ background: 'rgba(0,122,255,0.15)', color: '#007AFF' }}>
                                Branch #{branchCount}
                            </span>
                        )}
                    </div>
                    <h1 className="text-[clamp(28px,5vw,48px)] font-bold tracking-tight mb-3 leading-[1.1]">
                        Git for running processes.
                    </h1>
                    <p className="text-[16px] text-[var(--text-sub)] max-w-[600px] mb-6">
                        Watch 8 agents compete in a live RPS tournament. Trytet snapshots their state every 5 rounds.
                        Click any snapshot to <strong style={{ color: 'var(--text-main)' }}>rewind and fork</strong> — the tournament resumes from that exact state, diverging into a new timeline.
                    </p>

                    {/* Controls */}
                    <div className="flex items-center gap-3 flex-wrap">
                        <button onClick={toggleRun} className="btn" style={{ minWidth: 120 }}>
                            {running ? '⏸ Pause' : '▶ Start'}
                        </button>
                        <button onClick={manualSnapshot} className="btn" style={{ background: 'var(--bg-secondary)', color: 'var(--text-main)', border: '1px solid var(--card-border)' }}>
                            📸 Snapshot
                        </button>
                        <button onClick={resetTournament} className="btn" style={{ background: 'transparent', color: 'var(--text-sub)', border: '1px solid var(--card-border)' }}>
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
                                    <th style={{ textAlign: 'left', padding: '8px 24px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>#</th>
                                    <th style={{ textAlign: 'left', padding: '8px 12px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>Agent</th>
                                    <th style={{ textAlign: 'center', padding: '8px 12px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>W</th>
                                    <th style={{ textAlign: 'center', padding: '8px 12px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>L</th>
                                    <th style={{ textAlign: 'center', padding: '8px 12px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>D</th>
                                    <th style={{ textAlign: 'right', padding: '8px 24px', color: 'var(--text-sub)', fontWeight: 500, fontSize: 11 }}>Fuel</th>
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
                                                <div style={{
                                                    width: 8, height: 8, borderRadius: 2,
                                                    background: STRATEGY_COLORS[agent.strategy],
                                                    flexShrink: 0,
                                                }} />
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
                                                <div style={{
                                                    width: 40, height: 4, borderRadius: 2,
                                                    background: 'var(--code-bg)',
                                                    overflow: 'hidden',
                                                }}>
                                                    <div style={{
                                                        width: `${agent.fuel}%`,
                                                        height: '100%',
                                                        background: agent.fuel < 20 ? '#FF453A' : '#007AFF',
                                                        transition: 'width 0.3s ease',
                                                    }} />
                                                </div>
                                                <span style={{ color: agent.fuel < 20 ? '#FF453A' : 'var(--text-sub)', fontSize: 11 }}>
                                                    {agent.fuel}%
                                                </span>
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
                            fontFamily: "'JetBrains Mono', monospace", fontSize: 12, lineHeight: 1.8,
                            maxHeight: 380,
                        }}>
                            {matchLog.length === 0 && (
                                <div style={{ color: 'var(--text-sub)', fontStyle: 'italic', paddingTop: 12 }}>
                                    Press ▶ Start to begin the tournament...
                                </div>
                            )}
                            {matchLog.slice(-30).map((m, i) => (
                                <div key={i} style={{ color: 'var(--text-sub)' }}>
                                    <span style={{ color: 'var(--text-sub)', opacity: 0.5 }}>R{m.round} </span>
                                    <span style={{ color: STRATEGY_COLORS[m.agentA as Strategy] || 'var(--text-main)' }}>{m.agentA}</span>
                                    <span> {MOVE_EMOJI[m.moveA]} </span>
                                    <span style={{ opacity: 0.4 }}>vs</span>
                                    <span> {MOVE_EMOJI[m.moveB]} </span>
                                    <span style={{ color: STRATEGY_COLORS[m.agentB as Strategy] || 'var(--text-main)' }}>{m.agentB}</span>
                                    <span style={{ opacity: 0.5 }}> → </span>
                                    {m.winner ? (
                                        <span style={{ color: '#34C759', fontWeight: 600 }}>{m.winner}</span>
                                    ) : (
                                        <span style={{ color: 'var(--text-sub)', opacity: 0.6 }}>draw</span>
                                    )}
                                </div>
                            ))}
                        </div>
                    </div>

                </main>
            </div>

            {/* State Timeline */}
            <div className="w-full flex justify-center">
                <div className="px-5 md:px-10 pb-24 w-full max-w-[1000px]">
                    <div className="card" style={{ padding: '20px 24px' }}>
                        <div className="label" style={{ marginBottom: 16 }}>State Timeline — Snapshot & Rewind</div>

                        {snapshots.length === 0 ? (
                            <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 12, color: 'var(--text-sub)' }}>
                                Snapshots will appear here every 5 rounds. You can also take manual snapshots above.
                            </div>
                        ) : (
                            <div>
                                {/* Timeline bar */}
                                <div style={{ display: 'flex', alignItems: 'center', gap: 0, position: 'relative', marginBottom: 16 }}>
                                    <div style={{ position: 'absolute', top: '50%', left: 0, right: 0, height: 2, background: 'var(--card-border)', transform: 'translateY(-50%)' }} />
                                    {snapshots.map((snap) => (
                                        <button
                                            key={snap.id}
                                            onClick={() => restoreSnapshot(snap)}
                                            title={`Restore to ${snap.label}`}
                                            style={{
                                                position: 'relative',
                                                flex: 1,
                                                display: 'flex',
                                                flexDirection: 'column',
                                                alignItems: 'center',
                                                background: 'none',
                                                border: 'none',
                                                cursor: 'pointer',
                                                padding: '8px 0',
                                                zIndex: 2,
                                            }}
                                        >
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
                                                fontFamily: "'JetBrains Mono', monospace",
                                                fontSize: 10,
                                                color: activeSnapshotId === snap.id ? '#007AFF' : 'var(--text-sub)',
                                                marginTop: 6,
                                                whiteSpace: 'nowrap',
                                                fontWeight: activeSnapshotId === snap.id ? 600 : 400,
                                            }}>
                                                {snap.label}
                                            </span>
                                        </button>
                                    ))}
                                </div>

                                <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 11, color: 'var(--text-sub)' }}>
                                    {snapshots.length} snapshot{snapshots.length > 1 ? 's' : ''} · Click any point to rewind and fork into an alternate timeline
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
