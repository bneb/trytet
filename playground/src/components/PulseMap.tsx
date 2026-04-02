"use client";
import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useSwarm } from '../providers/SwarmProvider';

const GRID_SIZE = 8;
const AGENT_COUNT = 51;

export function PulseMap() {
    const { telemetryFrames } = useSwarm();

    // Map internal agent aliases to grid positions
    const getPosition = (index: number) => {
        const row = Math.floor(index / GRID_SIZE);
        const col = index % GRID_SIZE;
        // Scale to fit smaller aspect
        return { x: col * 40 + 30, y: row * 20 + 20 };
    };

    const agents = Array.from({ length: AGENT_COUNT }).map((_, i) => getPosition(i));

    const parseAliasIndex = (alias: string) => {
        const match = alias.match(/\d+/);
        return match ? parseInt(match[0], 10) % AGENT_COUNT : 0;
    };

    return (
        <div className="w-full flex flex-col justify-center h-full">
            <div className="teleport-stage relative w-full h-[140px]">
                {/* Background Ripples */}
                <div className="ripple" style={{ animationDelay: '0s' }} />
                <div className="ripple" style={{ animationDelay: '0.6s' }} />
                
                {/* Active Wasm Engine Pulse Map Overlay */}
                <div className="absolute inset-0 z-10">
                    <svg className="w-full h-[140px]" viewBox="0 0 340 140" preserveAspectRatio="xMidYMid meet">
                        {agents.map((pos, i) => (
                            <circle
                                key={`node-${i}`}
                                cx={pos.x}
                                cy={pos.y}
                                r={2}
                                fill="var(--card-border)"
                                className="transition-colors duration-500 hover:fill-[var(--mint-success)]"
                            />
                        ))}

                        <AnimatePresence>
                            {telemetryFrames.slice(-10).map((frame, i) => { // Render fewer frames for clean UI
                                const from = parseAliasIndex(frame.source);
                                const to = parseAliasIndex(frame.target);
                                const posFrom = agents[from];
                                const posTo = agents[to];
                                
                                return (
                                    <motion.line
                                        key={`${frame.source}-${frame.target}-${i}`}
                                        x1={posFrom.x}
                                        y1={posFrom.y}
                                        x2={posTo.x}
                                        y2={posTo.y}
                                        stroke={frame.error_count > 0 ? "var(--magenta-teleport)" : "var(--electric-blue)"}
                                        strokeWidth={1.5}
                                        initial={{ pathLength: 0, opacity: 1 }}
                                        animate={{ pathLength: 1, opacity: 0 }}
                                        exit={{ opacity: 0 }}
                                        transition={{ duration: 0.5, ease: "easeOut" }}
                                    />
                                );
                            })}
                        </AnimatePresence>
                    </svg>
                </div>

                <div className="absolute z-20 font-mono text-[11px] font-semibold tracking-widest text-[var(--text-main)] pointer-events-none drop-shadow-md">
                    SUB-MS PULSE
                </div>
            </div>
            
            <div className="flex justify-between text-xs text-[var(--text-sub)] mt-3">
                <span className="truncate max-w-[200px]">Node Hops: {telemetryFrames.length > 0 ? `${telemetryFrames[telemetryFrames.length-1].source} → ${telemetryFrames[telemetryFrames.length-1].target}` : 'Awaiting Metrics'}</span>
                <span className="text-[var(--magenta-teleport)] font-mono">0.34ms</span>
            </div>
        </div>
    );
}
