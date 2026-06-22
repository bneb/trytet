import re

with open("src/sandbox/sandbox_wasmtime.rs", "r") as f:
    content = f.read()

start_marker = r'        // Phase 3: Custom Inter-Tet RPC Host Function\n        linker\.func_wrap_async\('
end_marker = r'\)\.map_err\(\|e\| TetError::EngineError\(format!\("Failed to register trytet::invoke_component: \{e:#\}"\)\)\)\?;\n'

start_match = re.search(start_marker, content)
end_match = re.search(end_marker, content)

if not start_match or not end_match:
    print("Markers not found!")
    exit(1)

start_idx = start_match.start()
end_idx = end_match.end()

extracted_code = content[start_idx:end_idx]

header = """use wasmtime::Caller;
use crate::sandbox::sandbox_wasmtime::{TetState, validate_range, validate_range_mut};
use crate::engine::TetError;
use std::time::Instant;
use crate::models::{MeshCallRequest, ExecutionStatus};

pub fn register_host_functions(
    linker: &mut wasmtime::Linker<TetState>,
    source_alias: String
) -> Result<(), TetError> {
"""

footer = """
    Ok(())
}
"""

with open("src/sandbox/host_api.rs", "w") as f:
    f.write(header + extracted_code + footer)

new_content = content[:start_idx] + "        crate::sandbox::host_api::register_host_functions(linker, source_alias)?;\n" + content[end_idx:]

with open("src/sandbox/sandbox_wasmtime.rs", "w") as f:
    f.write(new_content)

print("Extraction and replacement successful.")
