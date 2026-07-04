use std::collections::HashMap;

use minijinja::{Environment, Value};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::project::{Model, ModelLanguage, Project};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledModel {
    pub name: String,
    pub compiled_sql: String,
    pub compiled_python: String,
    pub language: String,
    pub config: HashMap<String, serde_json::Value>,
    pub materialization: String,
    pub relation_name: String,
    pub depends_on: Vec<String>,
}

pub struct Engine {
    // Cache compiled results keyed by (model_name, full_refresh)
    cache: std::sync::Mutex<HashMap<(String, bool), CompiledModel>>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn compile(&self, model: &Model, full_refresh: bool) -> anyhow::Result<CompiledModel> {
        let project = Project::current()?;

        let target = &project.profile.outputs[&project.profile.target];
        let catalog = target.catalog.as_deref().unwrap_or("openpipe");
        let schema = target.schema.as_deref().unwrap_or("analytics");

        let alias = model.config.alias.as_deref().unwrap_or(&model.name);
        let relation_name = format!("{}.{}.{}", catalog, schema, alias);

        // Extract config
        let mut merged_config: HashMap<String, serde_json::Value> = HashMap::new();
        if let Some(mat) = &model.config.materialized {
            merged_config.insert("materialized".into(), serde_json::Value::String(mat.clone()));
        }
        if let Some(uk) = &model.config.unique_key {
            merged_config.insert("unique_key".into(), serde_json::Value::String(uk.clone()));
        }
        if model.language == ModelLanguage::Sql {
            let config_from_jinja = self.extract_config(&model.raw_sql);
            merged_config.extend(config_from_jinja);
        }

        let materialization = merged_config
            .get("materialized")
            .and_then(|v| v.as_str())
            .unwrap_or("view")
            .to_string();

        let depends_on: Vec<String> = model.refs.iter()
            .map(|r| resolve_ref_name(r, &project))
            .collect();

        // Handle Python models
        if model.language == ModelLanguage::Python {
            let compiled_python = self.compile_python(model, &relation_name)?;
            return Ok(CompiledModel {
                name: model.name.clone(),
                compiled_sql: String::new(),
                compiled_python,
                language: "python".to_string(),
                config: merged_config,
                materialization,
                relation_name,
                depends_on,
            });
        }

        // Evaluate Jinja2 template
        let compiled_sql = self.evaluate_jinja(&model.raw_sql, &model.name, &relation_name, full_refresh)?;

        let compiled = CompiledModel {
            name: model.name.clone(),
            compiled_sql,
            compiled_python: String::new(),
            language: "sql".to_string(),
            config: merged_config,
            materialization,
            relation_name,
            depends_on,
        };

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert((model.name.clone(), full_refresh), compiled.clone());
        }

        Ok(compiled)
    }

    fn compile_python(&self, model: &Model, _relation_name: &str) -> anyhow::Result<String> {
        let mut code = model.raw_python.clone();

        // Replace dbt.ref('name') with Spark DataFrame read
        for r in &model.refs {
            let project = Project::current()?;
            let resolved = resolve_ref_name(r, &project);
            let pattern = format!("dbt.ref('{}')", r);
            code = code.replace(&pattern, &format!("spark.table(\"{}\")", resolved));
        }

        // Replace dbt.source('src', 'tbl') with Spark DataFrame read
        let project = Project::current()?;
        for source in &project.sources {
            for table in &source.tables {
                let ident = table.identifier.as_deref().unwrap_or(&table.name);
                let src_schema = source.schema.as_deref().unwrap_or(&source.name);
                let pattern = format!("dbt.source('{}', '{}')", source.name, table.name);
                code = code.replace(&pattern, &format!("spark.table(\"{}.{}\")", src_schema, ident));
            }
        }

        // Wrap in PySpark boilerplate if not already
        if !code.contains("def model(") {
            code = format!(
                "def model(dbt, session):\n    import pyspark.sql.functions as F\n    spark = session.spark\n    {}\n    return result",
                code.replace("\n", "\n    ")
            );
        }

        Ok(code)
    }

    fn extract_config(&self, raw_sql: &str) -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();

        for line in raw_sql.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("{{ config(") || trimmed.starts_with("{%- raw -%}{{ config(") {
                let inner = trimmed
                    .trim_start_matches("{{ config(")
                    .trim_end_matches(") }}")
                    .trim_start_matches("{%- raw -%}{{ config(")
                    .trim_end_matches(") }}");
                for pair in inner.split(',') {
                    let parts: Vec<&str> = pair.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        let key = parts[0].trim().trim_matches('\'').trim_matches('"').to_string();
                        let val = parts[1].trim().trim_matches('\'').trim_matches('"').to_string();
                        config.insert(key, serde_json::Value::String(val));
                    }
                }
            }
        }

        config
    }

    fn evaluate_jinja(&self, raw_sql: &str, model_name: &str, relation_name: &str, full_refresh: bool) -> anyhow::Result<String> {
        let mut env = Environment::new();

        // Add dbt-compatible functions
        env.add_function("ref", {
            move |name: String| -> String {
                let project = Project::current().ok();
                if let Some(ref project) = project {
                    if let Some(idx) = project.model_map.get(&name) {
                        if let Some(dep_model) = project.models.get(*idx) {
                            let target = &project.profile.outputs[&project.profile.target];
                            let catalog = target.catalog.as_deref().unwrap_or("openpipe");
                            let schema = target.schema.as_deref().unwrap_or("analytics");
                            let alias = dep_model.config.alias.as_deref().unwrap_or(&dep_model.name);
                            return format!("{}.{}.{}", catalog, schema, alias);
                        }
                    }
                }
                name
            }
        });

        env.add_function("source", {
            move |source_name: String, table_name: String| -> String {
                let project = Project::current().ok();
                if let Some(ref project) = project {
                    for source in &project.sources {
                        if source.name == source_name {
                            let src_schema = source.schema.as_deref().unwrap_or(&source_name);
                            for table in &source.tables {
                                if table.name == table_name {
                                    let ident = table.identifier.as_deref().unwrap_or(&table_name);
                                    return format!("{}.{}", src_schema, ident);
                                }
                            }
                        }
                    }
                }
                format!("{}.{}", source_name, table_name)
            }
        });

        env.add_function("config", {
            move |_key: String, _value: Value| -> String { String::new() }
        });

        env.add_function("is_incremental", {
            let full_refresh = full_refresh;
            move || -> bool { !full_refresh }
        });

        // Add global variables
        let mut globals = HashMap::new();
        globals.insert("this".to_string(), Value::from(relation_name.to_string()));
        globals.insert("model_name".to_string(), Value::from(model_name.to_string()));
        globals.insert("run_started_at".to_string(), Value::from(
            chrono::Utc::now().to_rfc3339()
        ));
        env.add_global("this", Value::from(relation_name.to_string()));
        env.add_global("model_name", Value::from(model_name.to_string()));

        // Render template
        let template = match env.template_from_str(raw_sql) {
            Ok(t) => t,
            Err(e) => {
                // If Jinja fails, try simple text replacement
                tracing::warn!("Jinja eval failed for '{}', falling back to string replacement: {}", model_name, e);
                return Ok(self.fallback_compile(raw_sql, model_name, relation_name));
            }
        };

        let result = template
            .render(&globals)
            .map_err(|e| anyhow::anyhow!("Template render error for '{}': {}", model_name, e))?;

        // Cleanup: remove config directives that remain
        let config_re = Regex::new(r"\{\{ config\([^}]+\) \}\}").unwrap();
        let result = config_re.replace_all(&result, "").to_string();

        Ok(result.trim().to_string())
    }

    fn fallback_compile(&self, raw_sql: &str, _model_name: &str, relation_name: &str) -> String {
        let project = Project::current().ok();
        let mut sql = raw_sql.to_string();

        // Replace {{ config(...) }}
        let config_re = Regex::new(r"\{\{ config\([^}]+\) \}\}").unwrap();
        sql = config_re.replace_all(&sql, "").to_string();

        // Replace {{ ref('name') }}
        if let Some(ref project) = project {
            for model in &project.models {
                let ref_expr = format!("{{{{ ref('{}') }}}}", model.name);
                let target = &project.profile.outputs[&project.profile.target];
                let catalog = target.catalog.as_deref().unwrap_or("openpipe");
                let schema = target.schema.as_deref().unwrap_or("analytics");
                let alias = model.config.alias.as_deref().unwrap_or(&model.name);
                let rel = format!("{}.{}.{}", catalog, schema, alias);
                sql = sql.replace(&ref_expr, &rel);
            }
        }

        // Replace {{ source('name', 'table') }}
        if let Some(ref project) = project {
            for source in &project.sources {
                let src_schema = source.schema.as_deref().unwrap_or(&source.name);
                for table in &source.tables {
                    let ident = table.identifier.as_deref().unwrap_or(&table.name);
                    let rel = format!("{}.{}", src_schema, ident);
                    let source_expr = format!("{{{{ source('{}', '{}') }}}}", source.name, table.name);
                    sql = sql.replace(&source_expr, &rel);
                }
            }
        }

        // Replace {{ this }}
        sql = sql.replace("{{ this }}", relation_name);
        sql = sql.replace("{{this}}", relation_name);

        // Handle is_incremental blocks (fallback: full refresh, so remove them)
        let re = Regex::new(
            r"\{%\s*if\s+is_incremental\(\s*\)\s*%\}(.*?)\{%\s*endif\s*%\}"
        ).unwrap();
        sql = re.replace_all(&sql, "").to_string();

        sql.trim().to_string()
    }

    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
}

pub fn resolve_ref_name(name: &str, project: &Project) -> String {
    project.models.get(
        project.model_map.get(name).copied().unwrap_or(0)
    ).map(|m| {
        let target = &project.profile.outputs[&project.profile.target];
        let dep_catalog = target.catalog.as_deref().unwrap_or("openpipe");
        let dep_schema = target.schema.as_deref().unwrap_or("analytics");
        let dep_alias = m.config.alias.as_deref().unwrap_or(&m.name);
        format!("{}.{}.{}", dep_catalog, dep_schema, dep_alias)
    }).unwrap_or_else(|| name.to_string())
}
