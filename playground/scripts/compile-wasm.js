const wabt = require('wabt')();
const fs = require('fs');
const path = require('path');

async function compileWat(filename, watString) {
  try {
    const wabtModule = await wabt;
    const myModule = wabtModule.parseWat(filename, watString);
    myModule.resolveNames();
    myModule.validate();
    const binaryOutput = myModule.toBinary({ log: true, write_debug_names: true });
    
    const outPath = path.join(__dirname, '..', 'public', filename);
    fs.writeFileSync(outPath, Buffer.from(binaryOutput.buffer));
    console.log(`Successfully wrote ${filename}`);
  } catch (e) {
    console.error(`Failed to compile ${filename}:`, e);
  }
}

const infiniteLoopWat = `
(module
  (memory (export "memory") 1)
  (func (export "_start")
    (loop $inf (br $inf))
  )
)
`;

const fibonacciWat = `
(module
  (memory (export "memory") 1)
  (func $fib (param $n i32) (result i32)
    (if (result i32)
      (i32.lt_s (local.get $n) (i32.const 2))
      (then (local.get $n))
      (else
        (i32.add
          (call $fib (i32.sub (local.get $n) (i32.const 1)))
          (call $fib (i32.sub (local.get $n) (i32.const 2)))
        )
      )
    )
  )
  (func (export "_start")
    (call $fib (i32.const 30))
    drop
  )
)
`;

async function main() {
    await compileWat('infinite-loop.wasm', infiniteLoopWat);
    await compileWat('fibonacci.wasm', fibonacciWat);
}

main();
