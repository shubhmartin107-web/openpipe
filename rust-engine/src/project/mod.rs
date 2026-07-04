use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub profile: Profile,
    pub models: Vec<Model>,
    pub sources: Vec<Source>,
    pub tests: Vec<Test>,
    pub model_map: HashMap<String, usize>,
    pub source_map: HashMap<String, usize>,
    pub project_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub target: String,
    pub outputs: HashMap<String, TargetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(rename = "type")]
    pub engine_type: String,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub file_path: PathBuf,
    pub raw_sql: String,
    pub raw_python: String,
    pub language: ModelLanguage,
    pub config: ModelConfig,
    pub refs: Vec<String>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModelLanguage {
    Sql,
    Python,
}

impl Default for ModelLanguage {
    fn default() -> Self { ModelLanguage::Sql }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    pub materialized: Option<String>,
    pub unique_key: Option<String>,
    pub schema: Option<String>,
    pub alias: Option<String>,
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub tables: Vec<SourceTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTable {
    pub name: String,
    pub identifier: Option<String>,
    pub columns: Option<Vec<SourceColumn>>,
    pub freshness: Option<Freshness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceColumn {
    pub name: String,
    pub data_type: Option<String>,
    pub description: Option<String>,
    pub tests: Vec<TestDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Freshness {
    pub warn_after: Option<FreshnessThreshold>,
    pub error_after: Option<FreshnessThreshold>,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessThreshold {
    pub count: u32,
    pub period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Test {
    pub name: String,
    #[serde(rename = "model_name")]
    pub model_name: String,
    pub column_name: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub type_specific: HashMap<String, serde_json::Value>,
}

impl Test {
    fn from_value(v: &serde_json::Value) -> anyhow::Result<Self> {
        let obj = v.as_object().ok_or_else(|| anyhow::anyhow!("Test must be an object"))?;
        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Test missing 'name'"))?
            .to_string();
        let model_name = obj.get("model_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Test missing 'model_name'"))?
            .to_string();
        let column_name = obj.get("column_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut type_specific = HashMap::new();
        for (k, v) in obj {
            match k.as_str() {
                "name" | "model_name" | "column_name" => {}
                _ => { type_specific.insert(k.clone(), v.clone()); }
            }
        }

        Ok(Test { name, model_name, column_name, type_specific })
    }
}

impl Test {
    pub fn test_type(&self) -> Option<TestType> {
        let t = self.type_specific.get("type")?.as_str()?;
        match t {
            "not_null" => Some(TestType::NotNull),
            "unique" => Some(TestType::Unique),
            "accepted_values" => {
                let values = self.type_specific.get("values")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                Some(TestType::AcceptedValues { values })
            }
            "relationships" => {
                let to = self.type_specific.get("to")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                let field = self.type_specific.get("field")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                Some(TestType::Relationships { to, field })
            }
            "custom" => {
                let sql = self.type_specific.get("sql")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                Some(TestType::Custom { sql })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    NotNull,
    Unique,
    AcceptedValues { values: Vec<String> },
    Relationships {
        to: String,
        field: String,
    },
    Custom { sql: String },
}

pub type TestDef = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

static CURRENT_PROJECT: std::sync::Mutex<Option<Project>> = std::sync::Mutex::new(None);

impl Project {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let project_file = find_project_file(path)?;
        let project_dir = project_file.parent().unwrap_or(path);

        let content = std::fs::read_to_string(&project_file)?;
        let raw: RawProject = if project_file.extension().map_or(false, |e| e == "yml" || e == "yaml") {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };

        let target = raw.profile.target.clone();
        let profile = Profile {
            target: target.clone(),
            outputs: raw.profile.outputs,
        };

        let mut model_map = HashMap::new();
        let mut source_map = HashMap::new();

        let models = raw.models.unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                let sql_path = project_dir.join(format!("models/{}.sql", m.name));
                let py_path = project_dir.join(format!("models/{}.py", m.name));
                let (raw_sql, raw_python, language) = if py_path.exists() {
                    (String::new(), std::fs::read_to_string(&py_path).unwrap_or_default(), ModelLanguage::Python)
                } else if sql_path.exists() {
                    (std::fs::read_to_string(&sql_path).unwrap_or_default(), String::new(), ModelLanguage::Sql)
                } else {
                    (String::new(), String::new(), ModelLanguage::Sql)
                };
                let (refs, sources) = if language == ModelLanguage::Sql {
                    extract_refs_and_sources(&raw_sql)
                } else {
                    extract_refs_and_sources_python(&raw_python)
                };
                model_map.insert(m.name.clone(), i);
                Model {
                    name: m.name,
                    file_path: if language == ModelLanguage::Python { py_path } else { sql_path },
                    raw_sql,
                    raw_python,
                    language,
                    config: m.config.unwrap_or_default(),
                    refs,
                    sources,
                }
            })
            .collect();

        let sources: Vec<Source> = raw.sources.unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                source_map.insert(s.name.clone(), i);
                s
            })
            .collect();

        let tests: Vec<Test> = raw.tests.clone().unwrap_or_default()
            .into_iter()
            .map(|v| Test::from_value(&v))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut project = Project {
            name: raw.name.unwrap_or_else(|| "openpipe_project".to_string()),
            profile,
            models,
            sources,
            tests,
            model_map,
            source_map,
            project_path: project_dir.to_path_buf(),
        };

        project.extract_tests_from_sources();

        let mut guard = CURRENT_PROJECT.lock().unwrap();
        *guard = Some(project.clone());

        Ok(project)
    }

    pub fn current() -> anyhow::Result<Self> {
        let guard = CURRENT_PROJECT.lock().unwrap();
        guard.clone()
            .ok_or_else(|| anyhow::anyhow!("No project loaded. Call POST /api/v1/project/load first."))
    }

    pub fn resolve_refs(&self) -> Vec<Edge> {
        let mut edges = Vec::new();
        for model in &self.models {
            for dep in &model.refs {
                if self.model_map.contains_key(dep) {
                    edges.push(Edge {
                        from: dep.clone(),
                        to: model.name.clone(),
                        edge_type: "ref".to_string(),
                    });
                }
            }
            for src in &model.sources {
                if self.source_map.contains_key(src) {
                    edges.push(Edge {
                        from: format!("source:{}", src),
                        to: model.name.clone(),
                        edge_type: "source".to_string(),
                    });
                }
            }
        }
        edges
    }

    fn extract_tests_from_sources(&mut self) {
        let mut new_tests = Vec::new();
        for source in &self.sources {
            for table in &source.tables {
                if let Some(ref columns) = table.columns {
                    for col in columns {
                        for test_def in &col.tests {
                            for (test_name, config_val) in test_def {
                                let mut config = HashMap::new();
                                config.insert("type".to_string(), serde_json::Value::String(test_name.clone()));
                                if let Some(cfg) = config_val.as_object() {
                                    for (k, v) in cfg {
                                        config.insert(k.clone(), v.clone());
                                    }
                                }
                                let test = Test {
                                    name: format!("{}_{}_{}", source.name, table.name, test_name),
                                    model_name: format!("source:{}.{}", source.name, table.name),
                                    column_name: Some(col.name.clone()),
                                    type_specific: config,
                                };
                                new_tests.push(test);
                            }
                        }
                    }
                }
            }
        }
        self.tests.extend(new_tests);
    }
}

fn find_project_file(path: &Path) -> anyhow::Result<PathBuf> {
    let candidates = ["openpipe.yml", "openpipe.yaml", "openpipe.json"];
    for candidate in &candidates {
        let full = path.join(candidate);
        if full.exists() {
            return Ok(full);
        }
    }
    Err(anyhow::anyhow!("No openpipe.yml/yaml/json found in {}. Create one with 'name', 'profile', and optionally 'models'/'sources'/'tests'.", path.display()))
}

#[derive(Debug, Deserialize)]
struct RawProject {
    name: Option<String>,
    profile: RawProfile,
    models: Option<Vec<RawModel>>,
    sources: Option<Vec<Source>>,
    #[serde(default)]
    tests: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct RawProfile {
    target: String,
    outputs: HashMap<String, TargetConfig>,
}

#[derive(Debug, Deserialize)]
struct RawModel {
    name: String,
    config: Option<ModelConfig>,
}

fn extract_refs_and_sources_python(py: &str) -> (Vec<String>, Vec<String>) {
    let mut refs = Vec::new();
    let mut sources = Vec::new();

    // dbt.ref('name') or dbt.source('src', 'tbl')
    let ref_re = Regex::new(r#"dbt\.ref\(['"]([^'"]+)['"]\)"#).unwrap();
    for cap in ref_re.captures_iter(py) {
        if let Some(m) = cap.get(1) {
            refs.push(m.as_str().to_string());
        }
    }

    let source_re = Regex::new(r#"dbt\.source\(['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]\)"#).unwrap();
    for cap in source_re.captures_iter(py) {
        if let Some(m) = cap.get(1) {
            sources.push(m.as_str().to_string());
        }
    }

    (refs, sources)
}

fn extract_refs_and_sources(sql: &str) -> (Vec<String>, Vec<String>) {
    let mut refs = Vec::new();
    let mut sources = Vec::new();

    let ref_re = Regex::new(r#"\{\{\s*ref\s*\(\s*['"]([^'"]+)['"]\s*\)\s*\}\}"#).unwrap();
    for cap in ref_re.captures_iter(sql) {
        if let Some(m) = cap.get(1) {
            refs.push(m.as_str().to_string());
        }
    }

    let source_re = Regex::new(
        r#"\{\{\s*source\s*\(\s*['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]\s*\)\s*\}\}"#,
    )
    .unwrap();
    for cap in source_re.captures_iter(sql) {
        if let (Some(src), Some(_tbl)) = (cap.get(1), cap.get(2)) {
            sources.push(src.as_str().to_string());
        }
    }

    (refs, sources)
}
