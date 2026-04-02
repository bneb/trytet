"use client";

import React, { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { useSwarm } from '../providers/SwarmProvider';

export function FuelGauge() {
    const { telemetryFrames } = useSwarm();
    const [fuel, setFuel] = useState(100);

    // Mock fuel consumption based on telemetry calls
    useEffect(() => {
        if (telemetryFrames.length > 0) {
            const lastFrame = telemetryFrames[telemetryFrames.length - 1];
            setFuel((prev) => Math.max(0, prev - (lastFrame.call_count * 0.1)));
        }
    }, [telemetryFrames]);

    const isLow = fuel < 20;

    return (
        <div className="w-full flex flex-col justify-center h-full">
            <div className="font-mono text-[32px] mb-2">{fuel.toFixed(2)}%</div>
            <div className="fuel-wrap w-full">
                <div className="fuel-bar w-full">
                    <motion.div
                        className={`fuel-fill absolute top-0 left-0 transition-colors ${isLow ? '!bg-[var(--magenta-teleport)] !shadow-[0_0_15px_rgba(175,82,222,0.3)]' : ''}`}
                        initial={{ width: '100%' }}
                        animate={{ width: `${fuel}%` }}
                        transition={{ ease: "linear", duration: 0.2 }}
                    />
                </div>
            </div>
            <div className="flex justify-between text-xs text-[var(--text-sub)] mt-3">
                <span>Process ID: trytet-worker-01</span>
                <span className={isLow ? 'text-[var(--magenta-teleport)]' : 'text-[var(--mint-success)]'}>
                    {isLow ? 'CRITICAL' : 'ACTIVE'}
                </span>
            </div>
        </div>
    );
}
