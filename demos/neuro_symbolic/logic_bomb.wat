;; Logic Bomb Cartridge — Fuel Exhaustion Demo
;;
;; This component enters an infinite loop the moment execute() is called.
;; It simulates a solver hitting an unsatisfiable problem space or a
;; pathological input that causes exponential blowup.
;;
;; When invoked via CartridgeManager with a finite fuel budget,
;; wasmtime will trap with FuelExhausted — proving the host survives.
;;
;; Interface: trytet:component/cartridge-v1
;;   execute(input: string) -> result<string, string>

(component
    (core module $m
        (memory (export "memory") 1)

        (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
            i32.const 512
        )

        ;; The infinite loop. This is the "Z3 logic bomb."
        ;; In a real scenario, this could be a SAT solver on an unsatisfiable
        ;; formula with millions of clauses — computationally identical to
        ;; an infinite loop from the host's perspective.
        (func (export "execute") (param i32 i32) (result i32)
            (loop $spin
                (br $spin)
            )
            unreachable
        )
    )
    (core instance $i (instantiate $m))

    (func $execute (param "input" string) (result (result string (error string)))
        (canon lift (core func $i "execute")
            (memory $i "memory")
            (realloc (func $i "cabi_realloc"))
        )
    )

    (export "execute" (func $execute))
)
