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
        <div className="w-full flex items-center space-x-4 border border-[#333] p-2 bg-[#050505]">
            <span className="text-[#00ff9d] text-xs font-bold uppercase w-24">FuelVoucher</span>
            <div className="flex-grow h-4 bg-[#111] relative overflow-hidden ring-1 ring-[#333]">
                <motion.div
                    className={`absolute top-0 left-0 h-full ${isLow ? 'bg-[#fe2c55]' : 'bg-[#00ff9d]'}`}
                    initial={{ width: '100%' }}
                    animate={{ width: `${fuel}%` }}
                    transition={{ ease: "linear", duration: 0.2 }}
                />
            </div>
            <span className={`text-xs font-mono font-bold w-12 text-right ${isLow ? 'text-[#fe2c55]' : 'text-[#00ff9d]'}`}>
                {fuel.toFixed(1)}%
            </span>
        </div>
    );
}
