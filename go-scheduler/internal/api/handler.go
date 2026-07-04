package api

import (
	"context"
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/openingest/openpipe/go-scheduler/internal/dag"
	"github.com/openingest/openpipe/go-scheduler/internal/executor"
	"github.com/openingest/openpipe/go-scheduler/internal/scheduler"
)

type Handler struct {
	runManager    *executor.RunManager
	sched         *scheduler.Scheduler
	engineClient  *executor.EngineClient
	webhook       *executor.WebhookTrigger
}

func NewHandler(runManager *executor.RunManager, sched *scheduler.Scheduler, engineClient *executor.EngineClient) *Handler {
	return &Handler{
		runManager:   runManager,
		sched:        sched,
		engineClient: engineClient,
		webhook:      executor.NewWebhookTrigger(),
	}
}

func (h *Handler) WebhookHandler() http.HandlerFunc {
	return h.webhook.ServeHTTP(func(ctx context.Context, req executor.RunRequest) (string, error) {
		req.FullRefresh = true
		return h.runManager.StartRun(ctx, req)
	})
}

func (h *Handler) RegisterWebhook(w http.ResponseWriter, r *http.Request) {
	var hw executor.WebhookHandler
	if err := json.NewDecoder(r.Body).Decode(&hw); err != nil {
		respondError(w, http.StatusBadRequest, "Invalid request: "+err.Error())
		return
	}
	if hw.Name == "" || hw.ModelName == "" {
		respondError(w, http.StatusBadRequest, "name and model_name are required")
		return
	}
	h.webhook.RegisterHandler(hw)
	respondJSON(w, http.StatusCreated, map[string]string{
		"status": "registered",
		"name":   hw.Name,
	})
}

func (h *Handler) UnregisterWebhook(w http.ResponseWriter, r *http.Request) {
	name := chi.URLParam(r, "name")
	if name == "" {
		respondError(w, http.StatusBadRequest, "name is required")
		return
	}
	h.webhook.UnregisterHandler(name)
	respondJSON(w, http.StatusOK, map[string]string{"status": "unregistered"})
}

func (h *Handler) ListSchedules(w http.ResponseWriter, r *http.Request) {
	schedules := h.sched.ListSchedules()
	respondJSON(w, http.StatusOK, schedules)
}

func (h *Handler) AddSchedule(w http.ResponseWriter, r *http.Request) {
	var sd scheduler.ScheduleDef
	if err := json.NewDecoder(r.Body).Decode(&sd); err != nil {
		respondError(w, http.StatusBadRequest, "Invalid request: "+err.Error())
		return
	}
	if sd.Name == "" || sd.Expr == "" {
		respondError(w, http.StatusBadRequest, "name and expr are required")
		return
	}
	if err := h.sched.AddSchedule(sd); err != nil {
		respondError(w, http.StatusInternalServerError, err.Error())
		return
	}
	respondJSON(w, http.StatusCreated, map[string]string{"status": "created", "name": sd.Name})
}

func (h *Handler) RemoveSchedule(w http.ResponseWriter, r *http.Request) {
	name := chi.URLParam(r, "name")
	if name == "" {
		respondError(w, http.StatusBadRequest, "name is required")
		return
	}
	h.sched.RemoveSchedule(name)
	respondJSON(w, http.StatusOK, map[string]string{"status": "removed"})
}

func (h *Handler) Health(w http.ResponseWriter, r *http.Request) {
	engineOK := h.engineClient.Health() == nil

	respondJSON(w, http.StatusOK, map[string]interface{}{
		"status":     "ok",
		"engine_ok":  engineOK,
		"scheduler":  "running",
	})
}

func (h *Handler) ListRuns(w http.ResponseWriter, r *http.Request) {
	runs := h.runManager.ListRuns()
	respondJSON(w, http.StatusOK, runs)
}

func (h *Handler) TriggerRun(w http.ResponseWriter, r *http.Request) {
	var req executor.RunRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		respondError(w, http.StatusBadRequest, "Invalid request: "+err.Error())
		return
	}

	runID, err := h.runManager.StartRun(r.Context(), req)
	if err != nil {
		respondError(w, http.StatusInternalServerError, err.Error())
		return
	}

	respondJSON(w, http.StatusAccepted, map[string]string{
		"run_id": runID,
		"status": "pending",
	})
}

func (h *Handler) GetRun(w http.ResponseWriter, r *http.Request) {
	runID := chi.URLParam(r, "runID")
	run, ok := h.runManager.GetRun(runID)
	if !ok {
		respondError(w, http.StatusNotFound, "Run not found")
		return
	}
	respondJSON(w, http.StatusOK, run)
}

func (h *Handler) GetDAG(w http.ResponseWriter, r *http.Request) {
	// Compile all models to get the DAG
	compileResp, err := h.engineClient.Compile(executor.CompileRequest{})
	if err != nil {
		respondError(w, http.StatusInternalServerError, "Failed to compile: "+err.Error())
		return
	}
	respondJSON(w, http.StatusOK, compileResp)
}

func (h *Handler) ListModels(w http.ResponseWriter, r *http.Request) {
	compileResp, err := h.engineClient.Compile(executor.CompileRequest{})
	if err != nil {
		respondError(w, http.StatusInternalServerError, err.Error())
		return
	}
	respondJSON(w, http.StatusOK, compileResp.Models)
}

func (h *Handler) GetLineage(w http.ResponseWriter, r *http.Request) {
	lineage, err := h.engineClient.GetLineage()
	if err != nil {
		respondError(w, http.StatusInternalServerError, err.Error())
		return
	}
	respondJSON(w, http.StatusOK, lineage)
}

func (h *Handler) Backfill(w http.ResponseWriter, r *http.Request) {
	var req executor.RunRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		respondError(w, http.StatusBadRequest, "Invalid request: "+err.Error())
		return
	}
	req.FullRefresh = true

	// Build DAG from compiled models
	compileResp, err := h.engineClient.Compile(executor.CompileRequest{})
	if err != nil {
		respondError(w, http.StatusInternalServerError, "Compile failed: "+err.Error())
		return
	}

	modelDAG := dag.New()
	for _, m := range compileResp.Models {
		modelDAG.AddNode(m.Name, m.Name, dag.NodeModel, m.Config)
	}
	for _, e := range compileResp.DagEdges {
		modelDAG.AddEdge(e.From, e.To, "parent_child")
	}
	modelDAG.TopologicalSort()

	runID, err := h.runManager.Backfill(r.Context(), req, modelDAG)
	if err != nil {
		respondError(w, http.StatusInternalServerError, err.Error())
		return
	}

	respondJSON(w, http.StatusAccepted, map[string]string{
		"run_id":       runID,
		"status":       "pending",
		"full_refresh": "true",
	})
}

func respondJSON(w http.ResponseWriter, status int, data interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(data)
}

func respondError(w http.ResponseWriter, status int, msg string) {
	respondJSON(w, status, map[string]string{"error": msg})
}
