"use client";
import React from 'react';
import { motion } from 'framer-motion';
import { Cpu, Zap, Activity } from 'lucide-react';

interface Props {
  onHydrate: () => void;
}

export function OnboardingModal({ onHydrate }: Props) {
  return (
    <motion.div 
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0, backdropFilter: 'blur(0px)' }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4"
    >
      <motion.div 
        initial={{ scale: 0.95, opacity: 0, y: 20 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        transition={{ delay: 0.1, duration: 0.4, type: "spring", bounce: 0.25 }}
        className="glass-panel max-w-2xl w-full rounded-2xl overflow-hidden relative"
      >
        {/* Glow effect */}
        <div className="absolute -top-32 -left-32 w-64 h-64 bg-teal-500/20 rounded-full blur-3xl opacity-50" />
        <div className="absolute -bottom-32 -right-32 w-64 h-64 bg-emerald-500/10 rounded-full blur-3xl opacity-50" />

        <div className="relative p-8 md:p-12 border-b border-white/5">
          <div className="flex items-center space-x-3 mb-6">
            <div className="bg-teal-500/20 p-2 rounded-lg border border-teal-500/30">
              <Zap className="w-6 h-6 text-teal-400" />
            </div>
            <h2 className="text-3xl font-bold tracking-tight text-white">Sovereign Playground</h2>
          </div>
          
          <p className="text-zinc-300 text-lg leading-relaxed mb-6 font-light">
            Welcome to the front lines. The moment you loaded this page, an active 51-agent AI swarm was snapshotted on a Fly.io server and seamlessly teleported into your browser's WebWorker via a WebAssembly binary.
          </p>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-8">
            <div className="glass-panel p-4 rounded-xl flex items-start space-x-3">
              <Cpu className="w-5 h-5 text-teal-400 mt-0.5" />
              <div>
                <h3 className="text-white font-semibold mb-1">Zero Cloud Execution</h3>
                <p className="text-zinc-400 text-sm">Agents run autonomously in your local isolated Wasm runtime.</p>
              </div>
            </div>
            <div className="glass-panel p-4 rounded-xl flex items-start space-x-3">
              <Activity className="w-5 h-5 text-teal-400 mt-0.5" />
              <div>
                <h3 className="text-white font-semibold mb-1">Sub-Millisecond Cold State</h3>
                <p className="text-zinc-400 text-sm">Experience perfectly linear, state-hydrated execution speed natively.</p>
              </div>
            </div>
          </div>
        </div>

        <div className="relative p-6 bg-white/5 flex flex-col sm:flex-row justify-between items-center border-t border-white/5">
          <div className="text-zinc-400 text-xs uppercase tracking-widest font-mono mb-4 sm:mb-0">
            [Awaiting Hydration Signal]
          </div>
          <button 
            onClick={onHydrate}
            className="group relative px-8 py-3 bg-teal-500 hover:bg-teal-400 transition-colors rounded-lg font-bold text-teal-950 uppercase tracking-widest overflow-hidden shadow-[0_0_20px_rgba(20,184,166,0.3)] w-full sm:w-auto"
          >
            <span className="relative z-10 flex items-center justify-center space-x-2">
              <Zap className="w-4 h-4" />
              <span>Hydrate Swarm State</span>
            </span>
            <div className="absolute inset-0 h-full w-full bg-gradient-to-r from-transparent via-white/40 to-transparent -translate-x-full group-hover:animate-[shimmer_1.5s_infinite]" />
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
