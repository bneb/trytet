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
        <div className="w-full flex items-center space-x-4 bg-transparent p-2 rounded-xl">
            <span className="text-teal-400 text-xs font-bold uppercase tracking-widest w-24">FuelVoucher</span>
            <div className="flex-grow h-3 bg-zinc-800/50 rounded-full relative overflow-hidden ring-1 ring-white/10 shadow-inner">
                <motion.div
                    className={`absolute top-0 left-0 h-full rounded-full transition-colors ${isLow ? 'bg-rose-500 shadow-[0_0_10px_#f43f5e]' : 'bg-teal-400 shadow-[0_0_10px_#2dd4bf]'}`}
                    initial={{ width: '100%' }}
                    animate={{ width: `${fuel}%` }}
                    transition={{ ease: "linear", duration: 0.2 }}
                />
            </div>
            <span className={`text-xs font-mono font-bold w-12 text-right ${isLow ? 'text-rose-500' : 'text-teal-400'}`}>
                {fuel.toFixed(1)}%
            </span>
        </div>
    );
}
