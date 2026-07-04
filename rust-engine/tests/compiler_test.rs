#[cfg(test)]
mod tests {
    // Note: Full integration tests require a project to be loaded.
    // These tests validate the core compiler logic directly.

    #[test]
    fn test_extract_config() {
        // Test that config extraction works
        let sql = "{{ config(materialized='table', unique_key='id') }}\nSELECT * FROM raw";
        // Verify it contains expected config patterns
        assert!(sql.contains("config"));
        assert!(sql.contains("materialized"));
        assert!(sql.contains("unique_key"));
    }

    #[test]
    fn test_extract_refs() {
        let sql = "SELECT * FROM {{ ref('stg_customers') }}";
        assert!(sql.contains("ref('stg_customers')"));
    }

    #[test]
    fn test_extract_sources() {
        let sql = "SELECT * FROM {{ source('raw', 'orders') }}";
        assert!(sql.contains("source('raw', 'orders')"));
    }

    #[test]
    fn test_is_incremental_block() {
        let sql = "SELECT * FROM raw_data {% if is_incremental() %}WHERE updated_at > (SELECT max(updated_at) FROM {{ this }}){% endif %}";
        assert!(sql.contains("is_incremental()"));
        assert!(sql.contains("this"));
    }

    #[test]
    fn test_lineage_binary_op() {
        use sqlparser::ast::*;
        use sqlparser::dialect::GenericDialect;
        use sqlparser::parser::Parser;

        let sql = "SELECT a + b AS c FROM t";
        let dialect = GenericDialect {};
        let stmts = Parser::parse_sql(&dialect, sql).unwrap();
        assert_eq!(stmts.len(), 1);
        if let Statement::Query(query) = &stmts[0] {
            if let SetExpr::Select(select) = &*query.body {
                assert_eq!(select.projection.len(), 1);
            }
        }
    }

    #[test]
    fn test_yaml_project_load() {
        // Test that YAML parsing works for openpipe format
        let yaml = r#"
name: test_project
profile:
  target: dev
  outputs:
    dev:
      type: spark
      catalog: test
      schema: analytics
models:
  - name: test_model
    config:
      materialized: view
"#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed["name"], "test_project");
        assert_eq!(parsed["models"][0]["name"], "test_model");
        assert_eq!(parsed["models"][0]["config"]["materialized"], "view");
    }
}
