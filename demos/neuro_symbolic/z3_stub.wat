;; Z3 Solver Stub Cartridge — Neuro-Symbolic Demo
;;
;; This is a synthetic Wasm Component that simulates a Z3 constraint solver.
;; It returns a hardcoded satisfiable schedule as JSON.
;;
;; In production, this would be a real Z3 binding compiled to Wasm.
;; For the demo, the mechanism is what matters — the host's ability to
;; fuel-bound, instantiate, invoke, and reclaim this component.
;;
;; Interface: trytet:component/cartridge-v1
;;   execute(input: string) -> result<string, string>

(component
    (core module $m
        (memory (export "memory") 1)

        ;; Pre-baked JSON response: a satisfiable weekly schedule
        ;; This is what a real Z3 solver would produce given calendar constraints
        (data (i32.const 8192)
            "{\"status\":\"sat\",\"solver\":\"z3-stub\",\"model\":{\"schedule\":[{\"day\":\"Mon\",\"time\":\"09:00\",\"event\":\"Team Standup\",\"room\":\"A1\"},{\"day\":\"Mon\",\"time\":\"14:00\",\"event\":\"Design Review\",\"room\":\"B3\"},{\"day\":\"Tue\",\"time\":\"10:00\",\"event\":\"Sprint Planning\",\"room\":\"A1\"},{\"day\":\"Wed\",\"time\":\"11:00\",\"event\":\"1:1 with Manager\",\"room\":\"C2\"},{\"day\":\"Thu\",\"time\":\"15:00\",\"event\":\"Demo Prep\",\"room\":\"A1\"}],\"conflicts\":0,\"solve_time_us\":42}}"
        )
        ;; JSON length: 389 bytes

        ;; Bump allocator for canonical ABI string passing
        (global $bump (mut i32) (i32.const 4096))
        (func (export "cabi_realloc") (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32) (result i32)
            (local $ptr i32)
            (local.set $ptr (global.get $bump))
            (global.set $bump (i32.add (global.get $bump) (local.get $new_size)))
            (local.get $ptr)
        )

        ;; execute: ignores input, returns the pre-baked schedule as Ok(json)
        ;; Canonical ABI return layout for result<string, string>:
        ;;   offset+0: discriminant (0 = Ok, 1 = Err)
        ;;   offset+4: string pointer
        ;;   offset+8: string length
        (func (export "execute") (param $input_ptr i32) (param $input_len i32) (result i32)
            ;; Write return struct at offset 2048
            (i32.store8 (i32.const 2048) (i32.const 0))    ;; Ok discriminant
            (i32.store (i32.const 2052) (i32.const 8192))   ;; ptr to JSON data
            (i32.store (i32.const 2056) (i32.const 389))    ;; JSON length
            (i32.const 2048)                                 ;; return pointer to ret area
        )
    )
    (core instance $i (instantiate $m))

    ;; Lift to component model types
    (func $execute (param "input" string) (result (result string (error string)))
        (canon lift (core func $i "execute")
            (memory $i "memory")
            (realloc (func $i "cabi_realloc"))
        )
    )

    (export "execute" (func $execute))
)
