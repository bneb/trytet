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

        // Try to get computed styles for exact hex values, fallback to approximate if SSR
        const isClient = typeof window !== 'undefined';
        const getVar = (name: string, fallback: string) => 
            isClient ? getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback : fallback;

        // Note: xterm.js theme requires actual hex/rgba strings, CSS variables often don't parse well directly in canvas
        const term = new Terminal({
            theme: {
                background: 'transparent',
                // Map to var(--text-sub) or default
                foreground: '#8E8E93',
                // Map to var(--magenta-teleport) or default
                cursor: '#AF52DE',
            },
            fontFamily: 'var(--font-mono), monospace',
            convertEol: true,
            fontSize: 13,
            lineHeight: 1.4,
        });

        const fitAddon = new FitAddon();
        term.loadAddon(fitAddon);

        term.open(termRef.current);
        fitAddon.fit();

        // Print initial mock syntax-highlighted block
        term.writeln('\x1b[38;2;99;99;102m// Substrate link verified. Listening for telemetry...\x1b[0m');
        term.writeln('\x1b[38;2;0;122;255mconst\x1b[0m engine = \x1b[38;2;0;122;255mawait\x1b[0m Trytet.\x1b[38;2;175;82;222mconnect\x1b[0m();\r\n');

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
            // Format incoming string like a telemetry string log
            xtermRef.current.writeln(`\x1b[38;2;52;199;89m"\x1b[0m\x1b[38;2;142;142;147m[SOVEREIGN] ${lastLog}\x1b[0m\x1b[38;2;52;199;89m"\x1b[0m`);
        }
    }, [logs]);

    return (
        <div className="w-full h-full flex flex-col pt-2">
            <div ref={termRef} className="flex-grow w-full" />
        </div>
    );
}
