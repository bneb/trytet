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
      className="fixed inset-0 z-[1000] flex items-center justify-center bg-[var(--bg)]/80 backdrop-blur-md p-4"
    >
      <motion.div 
        initial={{ scale: 0.95, opacity: 0, y: 20 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        transition={{ delay: 0.1, duration: 0.4, type: "spring", bounce: 0.25 }}
        className="card max-w-2xl w-full !p-0 overflow-hidden relative shadow-2xl"
      >
        {/* Intrinsic Brand Glows */}
        <div className="absolute -top-32 -left-32 w-64 h-64 rounded-full blur-[80px] opacity-40 bg-[var(--electric-blue)]" />
        <div className="absolute -bottom-32 -right-32 w-64 h-64 rounded-full blur-[80px] opacity-30 bg-[var(--magenta-teleport)]" />

        <div className="relative p-8 md:p-12 border-b border-[var(--card-border)]">
          <div className="flex items-center space-x-3 mb-6">
            <div className="p-2 rounded-lg border border-[var(--card-border)]" style={{ background: 'rgba(0,122,255,0.1)' }}>
              <Zap className="w-6 h-6 text-[var(--electric-blue)]" />
            </div>
            <h2 className="text-3xl font-bold tracking-tight text-[var(--text-main)]">Trytet Playground</h2>
          </div>
          
          <p className="text-[var(--text-sub)] text-lg leading-relaxed mb-6 font-light">
            Welcome. The moment you loaded this page, an active AI swarm was snapshotted from the Trytet backend and seamlessly handed off to your browser's WebAssembly sandbox.
          </p>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
            <div className="p-4 rounded-xl flex items-start space-x-3 border border-[var(--card-border)] bg-[var(--code-bg)]">
              <Cpu className="w-5 h-5 text-[var(--electric-blue)] mt-0.5" />
              <div>
                <h3 className="text-[var(--text-main)] font-medium mb-1 text-sm">Zero Cloud Execution</h3>
                <p className="text-[var(--text-sub)] text-xs">Agents run autonomously in your local isolated Wasm runtime.</p>
              </div>
            </div>
            <div className="p-4 rounded-xl flex items-start space-x-3 border border-[var(--card-border)] bg-[var(--code-bg)]">
              <Activity className="w-5 h-5 text-[var(--magenta-teleport)] mt-0.5" />
              <div>
                <h3 className="text-[var(--text-main)] font-medium mb-1 text-sm">Sub-Millisecond Cold State</h3>
                <p className="text-[var(--text-sub)] text-xs">Experience perfectly linear, state-hydrated execution speed.</p>
              </div>
            </div>
          </div>
        </div>

        <div className="relative p-6 flex flex-col sm:flex-row justify-between items-center bg-[var(--code-bg)]">
          <div className="text-[var(--text-sub)] text-[11px] uppercase tracking-widest font-mono mb-4 sm:mb-0">
            [Awaiting Hydration Signal]
          </div>
          <button 
            onClick={onHydrate}
            className="group relative px-6 py-3 transition-colors rounded-lg font-[600] text-[14px] uppercase tracking-wider overflow-hidden w-full sm:w-auto"
            style={{ 
              background: 'var(--text-main)', 
              color: 'var(--bg)',
              boxShadow: '0 0 20px rgba(0,122,255,0.2)'
            }}
          >
            <span className="relative z-10 flex items-center justify-center space-x-2">
              <Zap className="w-4 h-4" />
              <span>Hydrate Swarm State</span>
            </span>
            <div className="absolute inset-0 h-full w-full bg-gradient-to-r from-transparent via-black/10 to-transparent -translate-x-full group-hover:animate-[shimmer_1.5s_infinite]" />
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
