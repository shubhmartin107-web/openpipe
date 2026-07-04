use serde::{Deserialize, Serialize};

use crate::compiler::resolve_ref_name;
use crate::project::{Project, Test, TestType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_name: String,
    pub model_name: String,
    pub test_type: String,
    pub column_name: Option<String>,
    pub status: TestStatus,
    pub error_message: Option<String>,
    pub execution_sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Pass,
    Fail,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteResult {
    pub results: Vec<TestResult>,
    pub summary: TestSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
    pub skipped: usize,
    pub pass_rate: f64,
}

pub fn compile_test(test: &Test, project: &Project) -> anyhow::Result<String> {
    let test_type = test.test_type()
        .ok_or_else(|| anyhow::anyhow!("Unknown test type for '{}'", test.name))?;

    let relation = resolve_model_table(&test.model_name, project);
    let col = test.column_name.as_deref().unwrap_or("id");

    let sql = match &test_type {
        TestType::NotNull => {
            format!("SELECT COUNT(*) AS failures FROM {} WHERE {} IS NULL", relation, col)
        }
        TestType::Unique => {
            format!("SELECT COUNT(*) AS failures FROM (SELECT {} FROM {} GROUP BY {} HAVING COUNT(*) > 1) sub", col, relation, col)
        }
        TestType::AcceptedValues { values } => {
            let vals: Vec<String> = values.iter().map(|v| format!("'{}'", v.replace('\'', "''"))).collect();
            format!("SELECT COUNT(*) AS failures FROM {} WHERE {} NOT IN ({})", relation, col, vals.join(", "))
        }
        TestType::Relationships { to, field } => {
            let to_rel = resolve_model_table(to, project);
            format!(
                "SELECT COUNT(*) AS failures FROM {} t LEFT JOIN {} ref ON t.{} = ref.{} WHERE ref.{} IS NULL AND t.{} IS NOT NULL",
                relation, to_rel, col, field, field, col
            )
        }
        TestType::Custom { sql } => {
            format!("SELECT COUNT(*) AS failures FROM ({}) sub", sql)
        }
    };

    Ok(sql)
}

fn resolve_model_table(model_name: &str, project: &Project) -> String {
    if model_name.starts_with("source:") {
        let parts: Vec<&str> = model_name.trim_start_matches("source:").split('.').collect();
        if parts.len() == 2 {
            for source in &project.sources {
                if source.name == parts[0] {
                    let src_schema = source.schema.as_deref().unwrap_or(&source.name);
                    for table in &source.tables {
                        if table.name == parts[1] {
                            let ident = table.identifier.as_deref().unwrap_or(&table.name);
                            return format!("{}.{}", src_schema, ident);
                        }
                    }
                    return format!("{}.{}", src_schema, parts[1]);
                }
            }
        }
        return model_name.to_string();
    }

    resolve_ref_name(model_name, project)
}

impl TestSuiteResult {
    pub fn new(results: Vec<TestResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.status == TestStatus::Pass).count();
        let failed = results.iter().filter(|r| r.status == TestStatus::Fail).count();
        let errors = results.iter().filter(|r| r.status == TestStatus::Error).count();
        let skipped = results.iter().filter(|r| r.status == TestStatus::Skipped).count();
        let pass_rate = if total > 0 { passed as f64 / total as f64 * 100.0 } else { 100.0 };

        TestSuiteResult {
            summary: TestSummary { total, passed, failed, errors, skipped, pass_rate },
            results,
        }
    }
}
