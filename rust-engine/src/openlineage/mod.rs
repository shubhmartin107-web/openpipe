use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compiler::Engine;
use crate::lineage;
use crate::project::Project;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageEvents {
    pub events: Vec<OpenLineageEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageEvent {
    #[serde(rename = "eventType")]
    pub event_type: String,
    #[serde(rename = "eventTime")]
    pub event_time: String,
    pub producer: String,
    pub schema_url: String,
    pub job: OpenLineageJob,
    pub run: OpenLineageRun,
    pub inputs: Vec<OpenLineageDataset>,
    pub outputs: Vec<OpenLineageDataset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageRun {
    #[serde(rename = "runId")]
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageJob {
    pub namespace: String,
    pub name: String,
    pub facets: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageDataset {
    pub namespace: String,
    pub name: String,
    pub facets: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLineageFacet {
    pub schema: Option<String>,
    pub fields: HashMap<String, ColumnLineageField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLineageField {
    #[serde(rename = "inputFields")]
    pub input_fields: Vec<ColumnLineageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLineageInput {
    pub namespace: String,
    pub name: String,
    pub field: String,
    pub transformations: Vec<ColumnLineageTransformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLineageTransformation {
    #[serde(rename = "type")]
    pub trans_type: String,
    pub subtype: String,
    pub description: Option<String>,
    pub masking: Option<bool>,
}

const OPENLINEAGE_SCHEMA: &str = "https://openlineage.io/spec/1-1-0/OpenLineage.json";

pub fn generate_events(project: &Project, engine: &Engine) -> anyhow::Result<OpenLineageEvents> {
    let mut events = Vec::new();

    for model in &project.models {
        let compiled = engine.compile(model, false)?;
        let model_lineage = lineage::analyze_model_lineage(model, &compiled.compiled_sql)?;

        let run_id = Uuid::now_v7().to_string();

        let target = &project.profile.outputs[&project.profile.target];
        let namespace = format!("{}::{}", target.engine_type, target.catalog.as_deref().unwrap_or("lakehouse"));

        // Build input datasets from lineage
        let mut inputs = Vec::new();
        for input_ds in &model_lineage.input_datasets {
            let dataset = OpenLineageDataset {
                namespace: namespace.clone(),
                name: input_ds.clone(),
                facets: None,
            };
            inputs.push(dataset);
        }

        // Build output dataset with column lineage facet
        let mut columns_facet = ColumnLineageFacet {
            schema: None,
            fields: HashMap::new(),
        };

        for (col_name, col_lineage) in &model_lineage.columns {
            let mut input_fields = Vec::new();
            for input_field in &col_lineage.input_fields {
                let ol_input = ColumnLineageInput {
                    namespace: namespace.clone(),
                    name: input_field.dataset.clone(),
                    field: input_field.field.clone(),
                    transformations: input_field.transformations.iter().map(|t| {
                        ColumnLineageTransformation {
                            trans_type: t.trans_type.clone(),
                            subtype: t.subtype.clone(),
                            description: t.description.clone(),
                            masking: t.masking,
                        }
                    }).collect(),
                };
                input_fields.push(ol_input);
            }

            columns_facet.fields.insert(col_name.clone(), ColumnLineageField {
                input_fields,
            });
        }

        let mut output_facets = HashMap::new();
        output_facets.insert(
            "columnLineage".to_string(),
            serde_json::to_value(columns_facet)?,
        );

        let output = OpenLineageDataset {
            namespace: namespace.clone(),
            name: model_lineage.output_dataset.clone(),
            facets: Some(output_facets),
        };

        // Remove duplicate inputs (same namespace/name)
        inputs.sort_by(|a, b| a.name.cmp(&b.name));
        inputs.dedup_by(|a, b| a.name == b.name);

        // If no inputs found, create a placeholder
        if inputs.is_empty() {
            inputs.push(OpenLineageDataset {
                namespace: namespace.clone(),
                name: "unknown_input".to_string(),
                facets: None,
            });
        }

        let event = OpenLineageEvent {
            event_type: "COMPLETE".to_string(),
            event_time: chrono::Utc::now().to_rfc3339(),
            producer: "https://github.com/openingest/openpipe".to_string(),
            schema_url: OPENLINEAGE_SCHEMA.to_string(),
            job: OpenLineageJob {
                namespace: "openpipe".to_string(),
                name: compiled.relation_name.clone(),
                facets: Some(HashMap::from([
                    ("sql".to_string(), serde_json::json!({"query": compiled.compiled_sql})),
                ])),
            },
            run: OpenLineageRun {
                run_id: run_id.clone(),
            },
            inputs,
            outputs: vec![output],
        };

        events.push(event);
    }

    Ok(OpenLineageEvents { events })
}
