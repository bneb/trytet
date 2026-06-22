;; Counter module for snapshot/fork demo.
;; Stores a 32-bit counter at linear memory address 0.
;; Each invocation of _start increments the counter by 1.
(module
  (memory (export "memory") 1)
  (func (export "_start")
    ;; Load current counter from address 0
    (i32.store (i32.const 0)
      (i32.add
        (i32.load (i32.const 0))
        (i32.const 1)
      )
    )
  )
)
