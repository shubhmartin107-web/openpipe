const API_BASE = '/api/v1'

export interface CompiledModel {
  name: string
  compiled_sql: string
  materialization: string
  relation_name: string
  depends_on: string[]
  config: Record<string, unknown>
}

export interface DAGEdge {
  from: string
  to: string
  edge_type: string
}

export interface CompileResponse {
  models: CompiledModel[]
  dag_edges: DAGEdge[]
}

export interface ColumnLineage {
  transformation_type: string
  transformation_subtype: string
  input_fields: Array<{
    dataset: string
    field: string
    transformations: Array<{
      type: string
      subtype: string
    }>
  }>
}

export interface ModelLineage {
  model_name: string
  relation_name: string
  columns: Record<string, ColumnLineage>
  input_datasets: string[]
  output_dataset: string
}

export interface LineageResult {
  models: ModelLineage[]
}

export interface Run {
  id: string
  model_name: string
  status: string
  started_at?: string
  completed_at?: string
  error?: string
  steps: Array<{
    name: string
    status: string
    model_name?: string
    error?: string
  }>
}

export async function getDAG(): Promise<CompileResponse> {
  const res = await fetch(`${API_BASE}/dag`)
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export async function getLineage(): Promise<LineageResult> {
  const res = await fetch(`${API_BASE}/lineage`)
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export async function getRuns(): Promise<Run[]> {
  const res = await fetch(`${API_BASE}/runs`)
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export async function triggerRun(model?: string, fullRefresh = false): Promise<{ run_id: string }> {
  const res = await fetch(`${API_BASE}/runs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_name: model, full_refresh: fullRefresh }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}
