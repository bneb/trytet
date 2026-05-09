use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::mesh::TetMesh;
use tet_core::models::TetExecutionRequest;
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::economy::VoucherManager;
use tet_core::hive::HivePeers;

#[tokio::test]
async fn test_red_team_fuel_stealing() {
    let peers = HivePeers::new();
    let (mesh, _rx) = TetMesh::new(100, peers);
    let vm = Arc::new(VoucherManager::new("test".to_string()));
    let sandbox = Arc::new(
        WasmtimeSandbox::new(mesh.clone(), vm, false, "test_node".to_string()).unwrap(),
    );

    // This agent invokes a cartridge with 50,000 fuel.
    // The cartridge does almost nothing and returns an Err("logical error").
    // We expect the parent agent to be refunded the ~49,900 unused fuel.
    
    // First, let's precompile the cartridge
    let cartridge_wat = r#"
    (component
      (core module $m
        (memory (export "memory") 1)
        (global $bump (mut i32) (i32.const 4096))
        (func (export "cabi_realloc") (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32) (result i32)
            (local $ptr i32)
            (local.set $ptr (global.get $bump))
            (global.set $bump (i32.add (global.get $bump) (local.get $new_size)))
            (local.get $ptr)
        )
        (func (export "execute") (param $ptr i32) (param $len i32) (result i32)
          ;; Return Err("Fast Fail")
          (i32.store8 (i32.const 2048) (i32.const 1)) ;; Discriminant 1 = Err
          (i32.store (i32.const 2052) (i32.const 8192))
          (i32.store (i32.const 2056) (i32.const 9)) ;; "Fast Fail"
          (i32.const 2048)
        )
        (data (i32.const 8192) "Fast Fail")
      )
      (core instance $i (instantiate $m))
      (func $execute (param "input" string) (result (result string (error string)))
          (canon lift (core func $i "execute") (memory $i "memory") (realloc (func $i "cabi_realloc")))
      )
      (export "execute" (func $execute))
    )
    "#;
    let cartridge_wasm = wat::parse_str(cartridge_wat).unwrap();
    sandbox.cartridge_manager.precompile("quick-fail", &cartridge_wasm).unwrap();

    // Now the parent agent
    let parent_wat = r#"
    (module
        (import "trytet" "invoke_component" (func $invoke (param i32 i32 i32 i32 i64 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
            ;; Invoke quick-fail with 50,000 fuel
            (call $invoke
                (i32.const 1024) (i32.const 10) ;; cid "quick-fail"
                (i32.const 2048) (i32.const 2)  ;; payload "{}"
                (i64.const 50000)               ;; 50,000 fuel
                (i32.const 4096)                ;; out_ptr
                (i32.const 4092)                ;; out_len_ptr
            )
            drop
        )
        (data (i32.const 1024) "quick-fail")
        (data (i32.const 2048) "{}")
    )
    "#;
    let parent_wasm = wat::parse_str(parent_wat).unwrap();

    let req = TetExecutionRequest {
        payload: Some(parent_wasm),
        alias: Some("parent".to_string()),
        allocated_fuel: 60000,
        max_memory_mb: 10,
        env: Default::default(),
        injected_files: Default::default(),
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
    };

    let res = sandbox.execute(req).await.unwrap();
    println!("Fuel consumed: {}", res.fuel_consumed);
    
    // If fuel_consumed is > 50,000, it means the 50,000 fuel allocated to the child was NOT refunded
    // even though the child barely executed anything!
    assert!(res.fuel_consumed < 10000, "Red Team Alert: Cartridge execution errors steal all allocated fuel without refunding!");
}
