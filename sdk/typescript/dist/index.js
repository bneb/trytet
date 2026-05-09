"use strict";
// src/index.ts
Object.defineProperty(exports, "__esModule", { value: true });
exports.TrytetClient = void 0;
class TrytetClient {
    baseUrl;
    constructor(options) {
        this.baseUrl = options?.baseUrl || 'http://localhost:3000';
    }
    /**
     * Executes a WebAssembly payload or forks from an existing snapshot.
     */
    async execute(request) {
        const res = await fetch(`${this.baseUrl}/v1/tet/execute`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(request),
        });
        if (!res.ok) {
            const errorText = await res.text();
            throw new Error(`Execution failed (${res.status}): ${errorText}`);
        }
        return res.json();
    }
    /**
     * Captures the live memory and VFS state of a completed Wasm execution.
     */
    async snapshot(tetId) {
        const res = await fetch(`${this.baseUrl}/v1/tet/snapshot/${tetId}`, {
            method: 'POST',
        });
        if (!res.ok) {
            const errorText = await res.text();
            throw new Error(`Snapshot failed (${res.status}): ${errorText}`);
        }
        return res.json();
    }
    /**
     * Forks a new execution instance from a pre-existing snapshot.
     */
    async fork(snapshotId, request) {
        const res = await fetch(`${this.baseUrl}/v1/tet/fork/${snapshotId}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(request),
        });
        if (!res.ok) {
            const errorText = await res.text();
            throw new Error(`Fork failed (${res.status}): ${errorText}`);
        }
        return res.json();
    }
    /**
     * Teleports an agent to another node in the hive.
     */
    async teleport(alias, targetNode) {
        const res = await fetch(`${this.baseUrl}/v1/tet/teleport/${alias}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(targetNode),
        });
        if (!res.ok) {
            const errorText = await res.text();
            throw new Error(`Teleport failed (${res.status}): ${errorText}`);
        }
    }
    /**
     * Retrieves the live hive topology (connected peers).
     */
    async getTopology() {
        const res = await fetch(`${this.baseUrl}/v1/topology`, {
            method: 'GET',
        });
        if (!res.ok) {
            throw new Error(`Failed to fetch topology`);
        }
        return res.json();
    }
}
exports.TrytetClient = TrytetClient;
