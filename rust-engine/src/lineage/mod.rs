use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlparser::ast::*;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::project::{Model, Project};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLineage {
    pub model_name: String,
    pub relation_name: String,
    pub columns: HashMap<String, ColumnLineage>,
    pub input_datasets: Vec<String>,
    pub output_dataset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLineage {
    pub input_fields: Vec<InputField>,
    pub transformation_type: String,
    pub transformation_subtype: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputField {
    pub dataset: String,
    pub field: String,
    pub transformations: Vec<Transformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transformation {
    #[serde(rename = "type")]
    pub trans_type: String,
    pub subtype: String,
    pub description: Option<String>,
    pub masking: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageResult {
    pub models: Vec<ModelLineage>,
}

impl LineageResult {
    pub fn new() -> Self {
        LineageResult { models: Vec::new() }
    }
}

pub fn analyze_model_lineage(model: &Model, compiled_sql: &str) -> anyhow::Result<ModelLineage> {
    let project = Project::current()?;
    let target = &project.profile.outputs[&project.profile.target];
    let catalog = target.catalog.as_deref().unwrap_or("openpipe");
    let schema = target.schema.as_deref().unwrap_or("analytics");
    let alias = model.config.alias.as_deref().unwrap_or(&model.name);
    let relation_name = format!("{}.{}.{}", catalog, schema, alias);

    let dialect = GenericDialect {};
    let statements = Parser::parse_sql(&dialect, compiled_sql)
        .map_err(|e| anyhow::anyhow!("SQL parse error for '{}': {}", model.name, e))?;

    let mut input_datasets: Vec<String> = Vec::new();
    let mut columns: HashMap<String, ColumnLineage> = HashMap::new();

    for stmt in &statements {
        match stmt {
            Statement::Query(query) => {
                analyze_query(&query.body, &mut input_datasets, &mut columns, &project);
            }
            Statement::CreateTable(create) => {
                if let Some(query) = &create.query {
                    analyze_query(&query.body, &mut input_datasets, &mut columns, &project);
                }
            }
            _ => {}
        }
    }

    Ok(ModelLineage {
        model_name: model.name.clone(),
        relation_name: relation_name.clone(),
        columns,
        input_datasets,
        output_dataset: relation_name,
    })
}

fn analyze_query(
    set_expr: &SetExpr,
    input_datasets: &mut Vec<String>,
    columns: &mut HashMap<String, ColumnLineage>,
    project: &Project,
) {
    match set_expr {
        SetExpr::Select(select) => {
            let tables = extract_tables_from_select(select, project);
            input_datasets.extend(tables);

            for item in &select.projection {
                match item {
                    SelectItem::UnnamedExpr(expr) => {
                        let col_name = expr_to_column_name(expr);
                        let lineage = analyze_expression_lineage(expr, input_datasets, project);
                        columns.insert(col_name, lineage);
                    }
                    SelectItem::ExprWithAlias { expr, alias } => {
                        let col_name = alias.value.to_string();
                        let lineage = analyze_expression_lineage(expr, input_datasets, project);
                        columns.insert(col_name, lineage);
                    }
                    SelectItem::Wildcard(wc) => {
                        if let Some(ref except) = wc.opt_except {
                            let mut col_names = vec![except.first_element.value.clone()];
                            for col in &except.additional_elements {
                                col_names.push(col.value.clone());
                            }
                            for col_name in col_names {
                                let lineage = input_datasets.last().map(|ds| ColumnLineage {
                                    input_fields: vec![InputField {
                                        dataset: ds.clone(),
                                        field: col_name.clone(),
                                        transformations: vec![Transformation {
                                            trans_type: "DIRECT".to_string(),
                                            subtype: "IDENTITY".to_string(),
                                            description: Some("wildcard select (except)".to_string()),
                                            masking: None,
                                        }],
                                    }],
                                    transformation_type: "DIRECT".to_string(),
                                    transformation_subtype: "IDENTITY".to_string(),
                                }).unwrap_or_else(|| ColumnLineage {
                                    input_fields: vec![],
                                    transformation_type: "DIRECT".to_string(),
                                    transformation_subtype: "IDENTITY".to_string(),
                                });
                                columns.insert(col_name, lineage);
                            }
                        }
                    }
                    SelectItem::QualifiedWildcard(kind, _) => {
                        let qualifier = kind.to_string();
                        if let Some(last) = input_datasets.last() {
                            let lineage = ColumnLineage {
                                input_fields: vec![InputField {
                                    dataset: last.clone(),
                                    field: format!("{}.*", qualifier),
                                    transformations: vec![Transformation {
                                        trans_type: "DIRECT".to_string(),
                                        subtype: "IDENTITY".to_string(),
                                        description: Some("qualified wildcard".to_string()),
                                        masking: None,
                                    }],
                                }],
                                transformation_type: "DIRECT".to_string(),
                                transformation_subtype: "IDENTITY".to_string(),
                            };
                            columns.insert(format!("{}.*", qualifier), lineage);
                        }
                    }
                }
            }
        }
        SetExpr::SetOperation { left, right, .. } => {
            analyze_query(left, input_datasets, columns, project);
            analyze_query(right, input_datasets, columns, project);
        }
        SetExpr::Values(_) => {}
        _ => {}
    }
}

fn extract_tables_from_select(select: &Select, project: &Project) -> Vec<String> {
    let mut tables = Vec::new();

    if let Some(ref from) = select.from.first() {
        extract_table_from_relation(&from.relation, &mut tables, project);

        for join in &from.joins {
            extract_table_from_relation(&join.relation, &mut tables, project);
        }
    }

    if let Some(ref selection) = select.selection {
        extract_tables_from_expr(selection, &mut tables, project);
    }

    tables
}

fn extract_table_from_relation(relation: &TableFactor, tables: &mut Vec<String>, project: &Project) {
    match relation {
        TableFactor::Table { name, .. } => {
            tables.push(name.to_string());
        }
        TableFactor::Derived { subquery, .. } => {
            if let SetExpr::Select(inner_select) = &*subquery.body {
                tables.extend(extract_tables_from_select(inner_select, project));
            }
        }
        TableFactor::Function { name, .. } => {
            tables.push(name.to_string());
        }
        TableFactor::TableFunction { expr, .. } => {
            tables.push(format!("TABLE({})", expr));
        }
        TableFactor::UNNEST { .. } => {
            tables.push("unnest".to_string());
        }
        TableFactor::JsonTable { .. } => {
            tables.push("json_table".to_string());
        }
        _ => {
            tables.push("subquery".to_string());
        }
    }
}

fn extract_tables_from_expr(expr: &Expr, tables: &mut Vec<String>, project: &Project) {
    match expr {
        Expr::Subquery(subquery) => {
            if let SetExpr::Select(inner_select) = &*subquery.body {
                tables.extend(extract_tables_from_select(inner_select, project));
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            extract_tables_from_expr(left, tables, project);
            extract_tables_from_expr(right, tables, project);
        }
        Expr::UnaryOp { expr, .. } => {
            extract_tables_from_expr(expr, tables, project);
        }
        Expr::Nested(expr) => {
            extract_tables_from_expr(expr, tables, project);
        }
        Expr::InSubquery { subquery, .. } => {
            if let SetExpr::Select(inner_select) = &*subquery.body {
                tables.extend(extract_tables_from_select(inner_select, project));
            }
        }
        Expr::Exists { subquery, .. } => {
            if let SetExpr::Select(inner_select) = &*subquery.body {
                tables.extend(extract_tables_from_select(inner_select, project));
            }
        }
        Expr::Cast { expr, .. } => {
            extract_tables_from_expr(expr, tables, project);
        }
        Expr::Extract { expr, .. } => {
            extract_tables_from_expr(expr, tables, project);
        }
        Expr::Case { conditions, else_result, .. } => {
            for when in conditions {
                extract_tables_from_expr(&when.condition, tables, project);
                extract_tables_from_expr(&when.result, tables, project);
            }
            if let Some(else_expr) = else_result {
                extract_tables_from_expr(else_expr, tables, project);
            }
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(args) = &func.parameters {
                for arg in &args.args {
                    match arg {
                        FunctionArg::Named { arg, .. }
                        | FunctionArg::Unnamed(arg)
                        | FunctionArg::ExprNamed { arg, .. } => {
                            if let FunctionArgExpr::Expr(expr) = arg {
                                extract_tables_from_expr(expr, tables, project);
                            }
                        }
                    }
                }
            } else if let FunctionArguments::Subquery(query) = &func.parameters {
                if let SetExpr::Select(inner_select) = &*query.body {
                    tables.extend(extract_tables_from_select(inner_select, project));
                }
            }
        }
        _ => {}
    }
}

fn analyze_expression_lineage(
    expr: &Expr,
    input_datasets: &[String],
    project: &Project,
) -> ColumnLineage {
    match expr {
        Expr::Identifier(ident) => {
            ColumnLineage {
                input_fields: input_datasets.iter().map(|ds| InputField {
                    dataset: ds.clone(),
                    field: ident.value.clone(),
                    transformations: vec![Transformation {
                        trans_type: "DIRECT".to_string(),
                        subtype: "IDENTITY".to_string(),
                        description: None,
                        masking: None,
                    }],
                }).collect(),
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "IDENTITY".to_string(),
            }
        }
        Expr::CompoundIdentifier(parts) => {
            let field_name = parts.last().map(|p| p.value.clone()).unwrap_or_default();
            let table_prefix = parts.first().map(|p| p.value.clone()).unwrap_or_default();
            ColumnLineage {
                input_fields: input_datasets.iter().map(|ds| InputField {
                    dataset: ds.clone(),
                    field: format!("{}.{}", table_prefix, field_name),
                    transformations: vec![Transformation {
                        trans_type: "DIRECT".to_string(),
                        subtype: "IDENTITY".to_string(),
                        description: None,
                        masking: None,
                    }],
                }).collect(),
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "IDENTITY".to_string(),
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            let left_lineage = analyze_expression_lineage(left, input_datasets, project);
            let right_lineage = analyze_expression_lineage(right, input_datasets, project);
            let mut input_fields = left_lineage.input_fields;
            input_fields.extend(right_lineage.input_fields);
            ColumnLineage {
                input_fields,
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "TRANSFORMATION".to_string(),
            }
        }
        Expr::UnaryOp { expr, .. } => {
            analyze_expression_lineage(expr, input_datasets, project)
        }
        Expr::Nested(inner) => {
            analyze_expression_lineage(inner, input_datasets, project)
        }
        Expr::Function(func) => {
            let is_aggregate = matches!(
                func.name.to_string().to_uppercase().as_str(),
                "COUNT" | "SUM" | "AVG" | "MIN" | "MAX"
            );
            let mut input_fields = Vec::new();

            if let FunctionArguments::List(args) = &func.parameters {
                for arg in &args.args {
                    match arg {
                        FunctionArg::Named { arg, .. }
                        | FunctionArg::Unnamed(arg)
                        | FunctionArg::ExprNamed { arg, .. } => {
                            if let FunctionArgExpr::Expr(expr) = arg {
                                let arg_lineage = analyze_expression_lineage(expr, input_datasets, project);
                                input_fields.extend(arg_lineage.input_fields);
                            }
                        }
                    }
                }
            } else if let FunctionArguments::Subquery(_query) = &func.parameters {
                let sub_inputs = input_datasets.to_vec();
                input_fields.extend(sub_inputs.iter().map(|ds| InputField {
                    dataset: ds.clone(),
                    field: "subquery".to_string(),
                    transformations: vec![Transformation {
                        trans_type: "DIRECT".to_string(),
                        subtype: "TRANSFORMATION".to_string(),
                        description: Some("subquery function argument".to_string()),
                        masking: None,
                    }],
                }));
            }

            let subtype = if is_aggregate {
                "AGGREGATION"
            } else {
                "TRANSFORMATION"
            };
            ColumnLineage {
                input_fields,
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: subtype.to_string(),
            }
        }
        Expr::Cast { expr, .. } => {
            let inner = analyze_expression_lineage(expr, input_datasets, project);
            ColumnLineage {
                input_fields: inner.input_fields,
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "TRANSFORMATION".to_string(),
            }
        }
        Expr::Case { conditions, else_result, .. } => {
            let mut input_fields = Vec::new();
            for when in conditions {
                let cond_lineage = analyze_expression_lineage(&when.condition, input_datasets, project);
                input_fields.extend(cond_lineage.input_fields);
                let res_lineage = analyze_expression_lineage(&when.result, input_datasets, project);
                input_fields.extend(res_lineage.input_fields);
            }
            if let Some(else_expr) = else_result {
                let else_lineage = analyze_expression_lineage(else_expr, input_datasets, project);
                input_fields.extend(else_lineage.input_fields);
            }
            ColumnLineage {
                input_fields,
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "CONDITIONAL".to_string(),
            }
        }
        Expr::Subquery(_) | Expr::Exists { .. } => {
            ColumnLineage {
                input_fields: input_datasets.iter().map(|ds| InputField {
                    dataset: ds.clone(),
                    field: "*".to_string(),
                    transformations: vec![Transformation {
                        trans_type: "DIRECT".to_string(),
                        subtype: "TRANSFORMATION".to_string(),
                        description: Some("subquery".to_string()),
                        masking: None,
                    }],
                }).collect(),
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "TRANSFORMATION".to_string(),
            }
        }
        Expr::Value(_) => {
            ColumnLineage {
                input_fields: vec![],
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "IDENTITY".to_string(),
            }
        }
        _ => {
            ColumnLineage {
                input_fields: input_datasets.iter().map(|ds| InputField {
                    dataset: ds.clone(),
                    field: "*".to_string(),
                    transformations: vec![Transformation {
                        trans_type: "DIRECT".to_string(),
                        subtype: "TRANSFORMATION".to_string(),
                        description: None,
                        masking: None,
                    }],
                }).collect(),
                transformation_type: "DIRECT".to_string(),
                transformation_subtype: "TRANSFORMATION".to_string(),
            }
        }
    }
}

fn expr_to_column_name(expr: &Expr) -> String {
    match expr {
        Expr::Identifier(ident) => ident.value.clone(),
        Expr::CompoundIdentifier(parts) => {
            parts.iter().map(|p| p.value.clone()).collect::<Vec<_>>().join(".")
        }
        Expr::Function(func) => func.name.to_string(),
        Expr::BinaryOp { left, right, op, .. } => {
            format!("{}_{}_{}", expr_to_column_name(left), op, expr_to_column_name(right))
        }
        Expr::UnaryOp { expr, op, .. } => {
            format!("{}_{}", op, expr_to_column_name(expr))
        }
        Expr::Value(v) => format!("{:?}", v),
        Expr::Cast { expr, data_type, .. } => {
            format!("{}_{}", expr_to_column_name(expr), data_type)
        }
        Expr::Case { .. } => "case_when".to_string(),
        _ => "expr".to_string(),
    }
}
