import os
for f_name in ["tests/api_tests.rs", "src/main.rs"]:
    with open(f_name, "r") as f:
        c = f.read()
    c = c.replace("        hive: None,\n    });", "        hive: None,\n        ingress_routes: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),\n    });")
    c = c.replace("        hive: Some(hive_peers.clone()),\n    });", "        hive: Some(hive_peers.clone()),\n        ingress_routes: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),\n    });")
    with open(f_name, "w") as f:
        f.write(c)
