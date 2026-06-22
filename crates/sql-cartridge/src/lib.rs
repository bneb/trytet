//! SQL Cartridge — in-memory SQLite for AI agents.
//!
//! Agents send SQL queries and get back JSON results.
//! Uses rusqlite with bundled SQLite compiled to wasm32-wasip1.

wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use rusqlite::Connection;

struct SqlCartridge;

impl Guest for SqlCartridge {
    fn execute(input: String) -> Result<String, String> {
        let parsed: serde_json::Value =
            serde_json::from_str(&input).map_err(|e| format!("Invalid input: {}", e))?;

        let sql = parsed["sql"].as_str().ok_or("Missing 'sql' field")?;

        let conn = Connection::open_in_memory().map_err(|e| format!("SQLite error: {}", e))?;

        // Execute each statement
        let mut results = Vec::new();
        for stmt_text in sql.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let mut stmt = conn
                .prepare(stmt_text)
                .map_err(|e| format!("SQL error: {}", e))?;
            let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
            let rows = stmt
                .query_map([], |row| {
                    let mut map = serde_json::Map::new();
                    for (i, col) in cols.iter().enumerate() {
                        let val: String = row.get::<_, String>(i).unwrap_or_else(|_| "NULL".into());
                        map.insert(col.clone(), serde_json::Value::String(val));
                    }
                    Ok(serde_json::Value::Object(map))
                })
                .map_err(|e| format!("Query error: {}", e))?;

            let row_values: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).collect();
            results.push(serde_json::json!({
                "columns": cols,
                "rows": row_values,
                "row_count": row_values.len(),
            }));
        }

        Ok(serde_json::json!({"results": results}).to_string())
    }
}

export!(SqlCartridge);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_sql_create_and_select() {
        // Use TEXT columns since the cartridge reads all values as String
        let input =
            r#"{"sql":"CREATE TABLE t (x TEXT); INSERT INTO t VALUES ('42'); SELECT x FROM t"}"#;
        let result = SqlCartridge::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let results = resp["results"].as_array().unwrap();
        assert_eq!(results.len(), 3, "Expected 3 statement results");
        // The SELECT result
        let select_result = &results[2];
        assert_eq!(select_result["columns"], serde_json::json!(["x"]));
        assert_eq!(select_result["row_count"], 1);
        assert_eq!(select_result["rows"][0]["x"], "42");
    }

    #[test]
    fn test_sql_invalid_json() {
        let result = SqlCartridge::execute("not-json".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_sql_malformed_sql() {
        let input = r#"{"sql":"CREATE TBLE wat"}"#;
        let result = SqlCartridge::execute(input.into());
        assert!(result.is_err());
    }

    #[test]
    fn test_sql_empty_sql() {
        let input = r#"{"sql":""}"#;
        let result = SqlCartridge::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let results = resp["results"].as_array().unwrap();
        assert!(results.is_empty(), "Expected no results for empty SQL");
    }

    #[test]
    fn test_sql_no_rows() {
        let input = r#"{"sql":"CREATE TABLE t (x TEXT); SELECT x FROM t WHERE x = '99'"}"#;
        let result = SqlCartridge::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let results = resp["results"].as_array().unwrap();
        let select_result = &results[1];
        assert_eq!(select_result["row_count"], 0);
        assert!(select_result["rows"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_sql_where_clause() {
        // Use TEXT columns since the cartridge reads all values as String
        let input = r#"{"sql":"CREATE TABLE t (a TEXT, b TEXT); INSERT INTO t VALUES ('x','1'),('y','2'); SELECT b FROM t WHERE a = 'y'"}"#;
        let result = SqlCartridge::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let results = resp["results"].as_array().unwrap();
        let select_result = &results[2];
        assert_eq!(select_result["row_count"], 1);
        assert_eq!(select_result["rows"][0]["b"], "2");
    }
}
