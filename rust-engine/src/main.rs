use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod compiler;
mod lineage;
mod openlineage;
mod project;
mod test_runner;

#[derive(Clone)]
struct AppState {
    engine: Arc<compiler::Engine>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into()))
        .init();

    let engine = Arc::new(compiler::Engine::new());

    let state = AppState { engine };

    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/project/load", post(load_project))
        .route("/api/v1/compile", post(compile))
        .route("/api/v1/lineage", post(get_lineage))
        .route("/api/v1/lineage/openlineage", post(get_openlineage))
        .route("/api/v1/tests/compile", post(compile_tests))
        .route("/api/v1/tests/suite", post(get_test_suite))
        .route("/api/v1/models/types", get(get_model_types))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 9090));
    tracing::info!("OpenPipe engine listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Deserialize)]
struct LoadProjectParams {
    path: PathBuf,
}

async fn load_project(
    State(_state): State<AppState>,
    Json(params): Json<LoadProjectParams>,
) -> Result<Json<project::Project>, (StatusCode, String)> {
    let result = project::Project::load(&params.path)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CompileParams {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    full_refresh: bool,
}

async fn compile(
    State(state): State<AppState>,
    Json(params): Json<CompileParams>,
) -> Result<Json<CompileResponse>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let models = if let Some(name) = &params.model {
        vec![project.models.iter()
            .find(|m| m.name == *name)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Model '{}' not found", name)))?
            .clone()]
    } else {
        project.models.clone()
    };

    let mut compiled = Vec::new();
    for model in &models {
        let result = state.engine.compile(model, params.full_refresh)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Compile error for '{}': {}", model.name, e)))?;
        compiled.push(result);
    }

    Ok(Json(CompileResponse {
        models: compiled,
        dag_edges: project.resolve_refs(),
    }))
}

async fn get_lineage(
    State(state): State<AppState>,
) -> Result<Json<lineage::LineageResult>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let mut lineage_result = lineage::LineageResult::new();
    for model in &project.models {
        let compiled = state.engine.compile(model, false)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let model_lineage = lineage::analyze_model_lineage(model, &compiled.compiled_sql)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        lineage_result.models.push(model_lineage);
    }

    Ok(Json(lineage_result))
}

async fn get_openlineage(
    State(state): State<AppState>,
) -> Result<Json<openlineage::OpenLineageEvents>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let events = openlineage::generate_events(&project, &state.engine)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(events))
}

#[derive(Serialize)]
struct CompileResponse {
    models: Vec<compiler::CompiledModel>,
    dag_edges: Vec<project::Edge>,
}

#[derive(Deserialize)]
struct TestCompileParams {
    #[serde(default)]
    test: Option<String>,
}

#[derive(Serialize)]
struct TestCompileResponse {
    tests: Vec<TestCompileEntry>,
}

#[derive(Serialize)]
struct TestCompileEntry {
    name: String,
    test_type: String,
    compiled_sql: String,
    model_name: String,
}

#[derive(Serialize)]
struct ModelTypesResponse {
    models: Vec<ModelTypeEntry>,
}

#[derive(Serialize)]
struct ModelTypeEntry {
    name: String,
    language: String,
    materialization: String,
}

async fn compile_tests(
    State(_state): State<AppState>,
    Json(params): Json<TestCompileParams>,
) -> Result<Json<TestCompileResponse>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let tests: Vec<&project::Test> = if let Some(ref name) = params.test {
        project.tests.iter().filter(|t| t.name == *name).collect()
    } else {
        project.tests.iter().collect()
    };

    let mut entries = Vec::new();
    for test in tests {
        let compiled = test_runner::compile_test(test, &project)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Test '{}' error: {}", test.name, e)))?;
        let test_type_str = test.type_specific.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        entries.push(TestCompileEntry {
            name: test.name.clone(),
            test_type: test_type_str,
            compiled_sql: compiled,
            model_name: test.model_name.clone(),
        });
    }

    Ok(Json(TestCompileResponse { tests: entries }))
}

async fn get_test_suite(
    State(_state): State<AppState>,
) -> Result<Json<test_runner::TestSuiteResult>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let mut results = Vec::new();
    for test in &project.tests {
        let compiled = match test_runner::compile_test(test, &project) {
            Ok(sql) => sql,
            Err(e) => {
                results.push(test_runner::TestResult {
                    test_name: test.name.clone(),
                    model_name: test.model_name.clone(),
                    test_type: test.type_specific.get("type")
                        .and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    column_name: test.column_name.clone(),
                    status: test_runner::TestStatus::Error,
                    error_message: Some(format!("Compilation error: {}", e)),
                    execution_sql: None,
                });
                continue;
            }
        };
        results.push(test_runner::TestResult {
            test_name: test.name.clone(),
            model_name: test.model_name.clone(),
            test_type: test.type_specific.get("type")
                .and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            column_name: test.column_name.clone(),
            status: test_runner::TestStatus::Pass,
            error_message: None,
            execution_sql: Some(compiled),
        });
    }

    Ok(Json(test_runner::TestSuiteResult::new(results)))
}

async fn get_model_types(
    State(_state): State<AppState>,
) -> Result<Json<ModelTypesResponse>, (StatusCode, String)> {
    let project = project::Project::current()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let entries = project.models.iter().map(|m| ModelTypeEntry {
        name: m.name.clone(),
        language: match m.language {
            project::ModelLanguage::Sql => "sql".to_string(),
            project::ModelLanguage::Python => "python".to_string(),
        },
        materialization: m.config.materialized.as_deref().unwrap_or("view").to_string(),
    }).collect();

    Ok(Json(ModelTypesResponse { models: entries }))
}
