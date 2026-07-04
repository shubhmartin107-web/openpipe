package executor

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/google/uuid"
)

type RunStatus string

const (
	RunStatusPending   RunStatus = "pending"
	RunStatusRunning   RunStatus = "running"
	RunStatusSuccess   RunStatus = "success"
	RunStatusFailed    RunStatus = "failed"
	RunStatusCancelled RunStatus = "cancelled"
)

type Run struct {
	ID          string                 `json:"id"`
	ModelName   string                 `json:"model_name"`
	Status      RunStatus              `json:"status"`
	StartedAt   *time.Time             `json:"started_at,omitempty"`
	CompletedAt *time.Time             `json:"completed_at,omitempty"`
	Error       string                 `json:"error,omitempty"`
	Config      map[string]interface{} `json:"config,omitempty"`
	Steps       []RunStep              `json:"steps"`
}

type RunStep struct {
	Name       string     `json:"name"`
	Status     RunStatus  `json:"status"`
	StartedAt  *time.Time `json:"started_at,omitempty"`
	CompletedAt *time.Time `json:"completed_at,omitempty"`
	Error      string     `json:"error,omitempty"`
	ModelName  string     `json:"model_name,omitempty"`
}

type RunRequest struct {
	ModelName   string   `json:"model_name,omitempty"`
	Tags        []string `json:"tags,omitempty"`
	FullRefresh bool     `json:"full_refresh"`
}

type RunManager struct {
	engineClient *EngineClient
	sqlExecutor  *SQLLakehouseExecutor
	mu           sync.RWMutex
	runs         map[string]*Run
	activeRuns   map[string]context.CancelFunc
}

func NewRunManager(engineClient *EngineClient, sqlExecutor *SQLLakehouseExecutor) *RunManager {
	return &RunManager{
		engineClient: engineClient,
		sqlExecutor:  sqlExecutor,
		runs:         make(map[string]*Run),
		activeRuns:   make(map[string]context.CancelFunc),
	}
}

func (m *RunManager) StartRun(ctx context.Context, req RunRequest) (string, error) {
	runID := uuid.New().String()

	run := &Run{
		ID:        runID,
		ModelName: req.ModelName,
		Status:    RunStatusPending,
		Config: map[string]interface{}{
			"full_refresh": req.FullRefresh,
		},
	}

	m.mu.Lock()
	m.runs[runID] = run
	m.mu.Unlock()

	go m.executeRun(run, req)

	return runID, nil
}

func (m *RunManager) executeRun(run *Run, req RunRequest) {
	ctx := context.Background()

	m.updateRunStatus(run.ID, RunStatusRunning)

	now := time.Now()
	run.StartedAt = &now

	// Compile the project or model
	compileResp, err := m.engineClient.Compile(CompileRequest{
		Model:       req.ModelName,
		FullRefresh: req.FullRefresh,
	})
	if err != nil {
		m.failRun(run.ID, fmt.Sprintf("Compilation failed: %v", err))
		return
	}

	// Execute each compiled model
	for _, model := range compileResp.Models {
		step := RunStep{
			Name:      fmt.Sprintf("materialize:%s", model.Name),
			Status:    RunStatusRunning,
			ModelName: model.Name,
		}
		stepNow := time.Now()
		step.StartedAt = &stepNow
		run.Steps = append(run.Steps, step)

		if err := m.sqlExecutor.Execute(ctx, model); err != nil {
			step.Status = RunStatusFailed
			step.Error = err.Error()
			stepNow2 := time.Now()
			step.CompletedAt = &stepNow2
			m.updateRunSteps(run.ID, run.Steps)
			m.failRun(run.ID, fmt.Sprintf("Model '%s' failed: %v", model.Name, err))
			return
		}

		step.Status = RunStatusSuccess
		stepNow3 := time.Now()
		step.CompletedAt = &stepNow3
		m.updateRunSteps(run.ID, run.Steps)

		log.Printf("Model '%s' materialized as %s (%s)", model.Name, model.Materialization, model.RelationName)
	}

	m.updateRunStatus(run.ID, RunStatusSuccess)
	completedNow := time.Now()
	run.CompletedAt = &completedNow

	log.Printf("Run %s completed successfully (%d models)", run.ID, len(compileResp.Models))
}

func (m *RunManager) failRun(runID, errMsg string) {
	m.updateRunStatus(runID, RunStatusFailed)
	m.mu.Lock()
	if run, ok := m.runs[runID]; ok {
		run.Error = errMsg
		now := time.Now()
		run.CompletedAt = &now
		if run.StartedAt == nil {
			run.StartedAt = &now
		}
	}
	m.mu.Unlock()
	log.Printf("Run %s failed: %s", runID, errMsg)
}

func (m *RunManager) updateRunStatus(runID string, status RunStatus) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if run, ok := m.runs[runID]; ok {
		run.Status = status
	}
}

func (m *RunManager) updateRunSteps(runID string, steps []RunStep) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if run, ok := m.runs[runID]; ok {
		run.Steps = steps
	}
}

func (m *RunManager) GetRun(runID string) (*Run, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	run, ok := m.runs[runID]
	if !ok {
		return nil, false
	}
	return run, true
}

func (m *RunManager) ListRuns() []*Run {
	m.mu.RLock()
	defer m.mu.RUnlock()
	result := make([]*Run, 0, len(m.runs))
	for _, run := range m.runs {
		result = append(result, run)
	}
	return result
}

func (m *RunManager) CancelRun(runID string) error {
	m.mu.Lock()
	cancel, ok := m.activeRuns[runID]
	m.mu.Unlock()

	if ok {
		cancel()
		m.updateRunStatus(runID, RunStatusCancelled)
		return nil
	}

	return fmt.Errorf("run %s not found or already completed", runID)
}
