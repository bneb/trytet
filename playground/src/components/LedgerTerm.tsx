"use client";

import React, { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

export function LedgerTerm({ logs }: { logs: string[] }) {
    const termRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<Terminal | null>(null);

    useEffect(() => {
        if (!termRef.current) return;

        const term = new Terminal({
            theme: {
                background: 'transparent',
                foreground: '#a1a1aa', // zinc-400
                cursor: '#2dd4bf', // teal-400
            },
            fontFamily: 'monospace',
            convertEol: true,
            fontSize: 12,
        });

        const fitAddon = new FitAddon();
        term.loadAddon(fitAddon);

        term.open(termRef.current);
        fitAddon.fit();

        term.writeln('Sovereign Ledger Node Online.');
        term.writeln('Listening for Telemetry...\r\n');

        xtermRef.current = term;

        const resizeObserver = new ResizeObserver(() => fitAddon.fit());
        resizeObserver.observe(termRef.current);

        return () => {
            resizeObserver.disconnect();
            term.dispose();
        };
    }, []);

    // Sync logs to terminal when they update
    useEffect(() => {
        if (xtermRef.current && logs.length > 0) {
            const lastLog = logs[logs.length - 1];
            xtermRef.current.writeln(`[SOVEREIGN] ${lastLog}`);
        }
    }, [logs]);

    return (
        <div className="flex flex-col h-full bg-transparent relative overflow-hidden rounded-xl">
            <h2 className="text-zinc-400 p-2 uppercase text-xs font-bold border-b border-white/5 z-10 sticky top-0 bg-transparent backdrop-blur-md">
                Terminal Ledger
            </h2>
            <div ref={termRef} className="flex-grow p-4" />
        </div>
    );
}
