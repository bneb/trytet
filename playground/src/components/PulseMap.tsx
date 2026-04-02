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
        return { x: col * 60 + 40, y: row * 60 + 40 };
    };

    const agents = Array.from({ length: AGENT_COUNT }).map((_, i) => getPosition(i));

    const parseAliasIndex = (alias: string) => {
        // Simple mock extraction: 'agent-42' -> 42
        const match = alias.match(/\d+/);
        return match ? parseInt(match[0], 10) % AGENT_COUNT : 0;
    };

    return (
        <div className="w-full h-full relative overflow-hidden bg-transparent font-mono p-4" data-testid="pulse-map">
            <h3 className="absolute top-4 left-4 text-teal-400 text-xs uppercase tracking-[0.2em] font-bold z-10 flex items-center space-x-2 drop-shadow-md">
                <span className="w-1.5 h-1.5 rounded-full bg-teal-400 animate-pulse" />
                <span>Sovereign Network Topology [51 Nodes]</span>
            </h3>
            
            <svg className="w-full h-full" viewBox="0 0 540 420">
                {/* Render nodes */}
                {agents.map((pos, i) => (
                    <circle
                        key={`node-${i}`}
                        cx={pos.x}
                        cy={pos.y}
                        r={4}
                        fill="#3f3f46"
                        className="transition-colors duration-500 hover:fill-teal-400 cursor-crosshair"
                    />
                ))}

                {/* Render pulsing connections */}
                <AnimatePresence>
                    {telemetryFrames.map((frame, i) => {
                        const from = parseAliasIndex(frame.source);
                        const to = parseAliasIndex(frame.target);
                        const posFrom = agents[from];
                        const posTo = agents[to];
                        
                        return (
                            <motion.line
                                key={`${frame.source}-${frame.target}-${i}`}
                                data-testid={`frame-${i}`}
                                x1={posFrom.x}
                                y1={posFrom.y}
                                x2={posTo.x}
                                y2={posTo.y}
                                stroke={frame.error_count > 0 ? "#f43f5e" : "#2dd4bf"}
                                strokeWidth={Math.min(frame.call_count, 4)}
                                filter="drop-shadow(0px 0px 4px rgba(45, 212, 191, 0.4))"
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
    );
}
