import React, { useCallback, useEffect, useMemo, useState } from 'react'
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Node,
  Edge,
  useNodesState,
  useEdgesState,
  MarkerType,
  Panel,
  NodeToolbar,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'

import { getDAG, getLineage, getRuns, triggerRun, CompiledModel, DAGEdge, ModelLineage, Run } from './api/client'

const MATERIALIZATION_COLORS: Record<string, string> = {
  source: '#6366f1',
  view: '#06b6d4',
  table: '#10b981',
  incremental: '#f59e0b',
  ephemeral: '#8b5cf6',
  test: '#ef4444',
}

function getNodeColor(type: string, materialization?: string): string {
  if (type === 'source') return MATERIALIZATION_COLORS.source
  if (type === 'test') return MATERIALIZATION_COLORS.test
  return MATERIALIZATION_COLORS[materialization || 'view'] || MATERIALIZATION_COLORS.view
}

interface AppProps {}

export default function App({}: AppProps) {
  const [models, setModels] = useState<CompiledModel[]>([])
  const [edges, setEdges] = useState<DAGEdge[]>([])
  const [lineage, setLineage] = useState<ModelLineage[]>([])
  const [runs, setRuns] = useState<Run[]>([])
  const [selectedModel, setSelectedModel] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const fetchData = useCallback(async () => {
    try {
      const [dagData, lineageData, runsData] = await Promise.all([
        getDAG(),
        getLineage(),
        getRuns(),
      ])
      setModels(dagData.models)
      setEdges(dagData.dag_edges)
      setLineage(lineageData.models)
      setRuns(runsData)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch data')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchData()
    const interval = setInterval(fetchData, 5000)
    return () => clearInterval(interval)
  }, [fetchData])

  const reactFlowNodes: Node[] = useMemo(() => {
    const allNodes: Node[] = []

    // Source nodes
    const sourceNames = new Set<string>()
    for (const m of models) {
      for (const dep of m.depends_on) {
        if (dep.includes('.')) {
          sourceNames.add(dep)
        }
      }
    }
    sourceNames.forEach((name, i) => {
      allNodes.push({
        id: name,
        type: 'default',
        position: { x: 50, y: i * 100 + 50 },
        data: {
          label: name.split('.').pop() || name,
          type: 'source',
        },
        style: {
          background: MATERIALIZATION_COLORS.source,
          color: '#fff',
          border: 'none',
          borderRadius: 8,
          padding: '8px 16px',
          fontSize: 12,
        },
      })
    })

    // Model nodes
    models.forEach((m, i) => {
      allNodes.push({
        id: m.name,
        type: 'default',
        position: { x: 350, y: i * 80 + 30 },
        data: {
          label: m.name,
          materialization: m.materialization,
          config: m.config,
          sql: m.compiled_sql,
          relation: m.relation_name,
        },
        style: {
          background: getNodeColor('model', m.materialization),
          color: '#fff',
          border: selectedModel === m.name ? '3px solid #fff' : 'none',
          borderRadius: 8,
          padding: '8px 16px',
          fontSize: 13,
          fontWeight: 500,
          width: 180,
        },
      })
    })

    return allNodes
  }, [models, selectedModel])

  const reactFlowEdges: Edge[] = useMemo(() => {
    return edges.map((e, i) => ({
      id: `edge-${i}`,
      source: e.from,
      target: e.to,
      animated: true,
      style: {
        stroke: e.edge_type === 'source' ? '#6366f180' : '#94a3b880',
        strokeWidth: 2,
      },
      markerEnd: {
        type: MarkerType.ArrowClosed,
        color: e.edge_type === 'source' ? '#6366f1' : '#94a3b8',
      },
    }))
  }, [edges])

  const onNodeClick = useCallback((_: React.MouseEvent, node: Node) => {
    setSelectedModel(node.id)
  }, [])

  const onPaneClick = useCallback(() => {
    setSelectedModel(null)
  }, [])

  const selectedLineage = useMemo(() => {
    if (!selectedModel) return null
    return lineage.find(l => l.model_name === selectedModel) || null
  }, [selectedModel, lineage])

  const modelRuns = useMemo(() => {
    if (!selectedModel) return []
    return runs.filter(r => r.model_name === selectedModel || r.model_name === '')
  }, [selectedModel, runs])

  if (loading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh' }}>
        <p>Loading OpenPipe DAG...</p>
      </div>
    )
  }

  if (error) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', justifyContent: 'center', alignItems: 'center', height: '100vh', gap: 16 }}>
        <p style={{ color: '#ef4444' }}>Error: {error}</p>
        <button onClick={fetchData} style={{ padding: '8px 16px', cursor: 'pointer' }}>
          Retry
        </button>
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', height: '100vh' }}>
      <div style={{ flex: 1, position: 'relative' }}>
        <ReactFlow
          nodes={reactFlowNodes}
          edges={reactFlowEdges}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          fitView
          attributionPosition="bottom-left"
        >
          <Background />
          <Controls />
          <MiniMap
            nodeColor={(node) => node.style?.background as string || '#666'}
          />
          <Panel position="top-left">
            <div style={{
              background: '#1e293b',
              color: '#fff',
              padding: '12px 16px',
              borderRadius: 8,
              fontSize: 14,
              opacity: 0.9,
            }}>
              <strong>OpenPipe DAG</strong>
              <span style={{ marginLeft: 12, color: '#94a3b8' }}>
                {models.length} models · {edges.length} edges
              </span>
            </div>
          </Panel>
          <Panel position="top-right">
            <button
              onClick={() => triggerRun()}
              style={{
                background: '#10b981',
                color: '#fff',
                border: 'none',
                padding: '8px 16px',
                borderRadius: 6,
                cursor: 'pointer',
                fontWeight: 500,
                marginRight: 8,
              }}
            >
              Run All
            </button>
            <button
              onClick={fetchData}
              style={{
                background: '#475569',
                color: '#fff',
                border: 'none',
                padding: '8px 16px',
                borderRadius: 6,
                cursor: 'pointer',
              }}
            >
              Refresh
            </button>
          </Panel>
        </ReactFlow>
      </div>

      {/* Side Panel */}
      {selectedModel && (
        <div style={{
          width: 380,
          background: '#0f172a',
          color: '#e2e8f0',
          padding: 20,
          overflowY: 'auto',
          borderLeft: '1px solid #334155',
        }}>
          <h2 style={{ marginBottom: 16, fontSize: 18 }}>{selectedModel}</h2>

          <Section title="Lineage">
            {selectedLineage ? (
              <div>
                <p style={{ fontSize: 12, color: '#94a3b8', marginBottom: 8 }}>
                  Output: {selectedLineage.output_dataset}
                </p>
                {Object.entries(selectedLineage.columns).slice(0, 10).map(([col, lin]) => (
                  <div key={col} style={{
                    background: '#1e293b',
                    borderRadius: 6,
                    padding: '8px 12px',
                    marginBottom: 6,
                    fontSize: 12,
                  }}>
                    <strong style={{ color: '#38bdf8' }}>{col}</strong>
                    <span style={{ color: '#64748b', marginLeft: 8 }}>
                      ({lin.transformation_type}/{lin.transformation_subtype})
                    </span>
                    {lin.input_fields.length > 0 && (
                      <div style={{ marginTop: 4, color: '#94a3b8', fontSize: 11 }}>
                        ← {lin.input_fields.map(f => `${f.dataset}.${f.field}`).join(', ')}
                      </div>
                    )}
                  </div>
                ))}
                {Object.keys(selectedLineage.columns).length > 10 && (
                  <p style={{ color: '#64748b', fontSize: 11, marginTop: 4 }}>
                    +{Object.keys(selectedLineage.columns).length - 10} more columns
                  </p>
                )}
              </div>
            ) : (
              <p style={{ color: '#64748b', fontSize: 12 }}>No lineage data</p>
            )}
          </Section>

          <Section title="Recent Runs">
            {modelRuns.length > 0 ? (
              modelRuns.slice(-5).reverse().map(run => (
                <div key={run.id} style={{
                  background: '#1e293b',
                  borderRadius: 6,
                  padding: '8px 12px',
                  marginBottom: 6,
                  fontSize: 12,
                }}>
                  <div>
                    <StatusBadge status={run.status} />
                    <span style={{ marginLeft: 8, color: '#94a3b8' }}>
                      {run.id.slice(0, 8)}...
                    </span>
                  </div>
                  {run.started_at && (
                    <div style={{ color: '#64748b', fontSize: 11, marginTop: 4 }}>
                      {new Date(run.started_at).toLocaleString()}
                    </div>
                  )}
                  {run.error && (
                    <div style={{ color: '#ef4444', fontSize: 11, marginTop: 4 }}>
                      {run.error}
                    </div>
                  )}
                </div>
              ))
            ) : (
              <p style={{ color: '#64748b', fontSize: 12 }}>No runs yet</p>
            )}
          </Section>

          <Section title="Config">
            <pre style={{
              background: '#1e293b',
              borderRadius: 6,
              padding: '8px 12px',
              fontSize: 11,
              overflowX: 'auto',
              color: '#94a3b8',
            }}>
              {JSON.stringify(models.find(m => m.name === selectedModel)?.config || {}, null, 2)}
            </pre>
          </Section>
        </div>
      )}
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 20 }}>
      <h3 style={{
        fontSize: 13,
        fontWeight: 600,
        color: '#94a3b8',
        textTransform: 'uppercase',
        letterSpacing: '0.05em',
        marginBottom: 10,
      }}>
        {title}
      </h3>
      {children}
    </div>
  )
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: '#f59e0b',
    running: '#3b82f6',
    success: '#10b981',
    failed: '#ef4444',
    cancelled: '#64748b',
  }
  return (
    <span style={{
      background: colors[status] || '#64748b',
      color: '#fff',
      padding: '2px 8px',
      borderRadius: 4,
      fontSize: 10,
      fontWeight: 600,
      textTransform: 'uppercase',
    }}>
      {status}
    </span>
  )
}
