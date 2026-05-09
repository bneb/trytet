import { NextResponse } from 'next/server';
import vm from 'vm';

export async function POST(request: Request) {
    try {
        const { snippets } = await request.json();

        if (!Array.isArray(snippets)) {
            return NextResponse.json({ error: 'Expected an array of code snippets' }, { status: 400 });
        }

        const results = [];
        const startTime = Date.now();

        // Evaluate sequentially to accurately simulate wall-clock timeout cost
        for (const code of snippets) {
            const startSnippet = Date.now();
            try {
                // E2B and standard containers use wall-clock timeouts.
                // We set a 1000ms timeout to catch the infinite loops.
                const result = vm.runInNewContext(code, {}, { timeout: 1000 });
                results.push({
                    status: 'Success',
                    output: String(result),
                    duration_ms: Date.now() - startSnippet
                });
            } catch (err: any) {
                if (err.message.includes('Script execution timed out')) {
                    results.push({
                        status: 'Timeout',
                        output: 'Script execution timed out after 1000ms',
                        duration_ms: Date.now() - startSnippet
                    });
                } else {
                    results.push({
                        status: 'Error',
                        output: err.message,
                        duration_ms: Date.now() - startSnippet
                    });
                }
            }
        }

        const totalDuration = Date.now() - startTime;

        return NextResponse.json({
            total_duration_ms: totalDuration,
            results
        });
    } catch (e: any) {
        return NextResponse.json({ error: e.message }, { status: 500 });
    }
}
