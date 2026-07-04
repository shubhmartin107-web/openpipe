package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"

	"github.com/openingest/openpipe/go-scheduler/internal/executor"
	"github.com/openingest/openpipe/go-scheduler/internal/scheduler"
)

type Server struct {
	runManager   *executor.RunManager
	engineClient *executor.EngineClient
	sched        *scheduler.Scheduler
	mu           sync.Mutex
}

type MCPRequest struct {
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
	ID     string          `json:"id,omitempty"`
}

type MCPResponse struct {
	Result interface{} `json:"result,omitempty"`
	Error  *MCPError   `json:"error,omitempty"`
	ID     string      `json:"id,omitempty"`
}

type MCPError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type ToolDefinition struct {
	Name        string        `json:"name"`
	Description string        `json:"description"`
	InputSchema InputSchema   `json:"inputSchema"`
}

type InputSchema struct {
	Type       string                    `json:"type"`
	Properties map[string]PropertySchema `json:"properties"`
}

type PropertySchema struct {
	Type        string   `json:"type"`
	Description string   `json:"description,omitempty"`
	Enum        []string `json:"enum,omitempty"`
}

func New(runManager *executor.RunManager, engineClient *executor.EngineClient, sched *scheduler.Scheduler) *Server {
	return &Server{
		runManager:   runManager,
		engineClient: engineClient,
		sched:        sched,
	}
}

func (s *Server) Tools() []ToolDefinition {
	return []ToolDefinition{
		{
			Name:        "pipe_run",
			Description: "Run a model or entire project",
			InputSchema: InputSchema{
				Type: "object",
				Properties: map[string]PropertySchema{
					"model":       {Type: "string", Description: "Model name to run (empty = all)"},
					"full_refresh": {Type: "boolean", Description: "Force full refresh"},
				},
			},
		},
		{
			Name:        "pipe_compile",
			Description: "Compile a project and return the DAG",
			InputSchema: InputSchema{
				Type: "object",
				Properties: map[string]PropertySchema{
					"model": {Type: "string", Description: "Specific model to compile"},
				},
			},
		},
		{
			Name:        "pipe_lineage",
			Description: "Get column-level lineage for all models",
			InputSchema: InputSchema{
				Type:       "object",
				Properties: map[string]PropertySchema{},
			},
		},
		{
			Name:        "pipe_list_runs",
			Description: "List recent pipeline runs",
			InputSchema: InputSchema{
				Type:       "object",
				Properties: map[string]PropertySchema{},
			},
		},
		{
			Name:        "pipe_get_run",
			Description: "Get details of a specific run",
			InputSchema: InputSchema{
				Type: "object",
				Properties: map[string]PropertySchema{
					"run_id": {Type: "string", Description: "Run ID"},
				},
			},
		},
		{
			Name:        "pipe_health",
			Description: "Check OpenPipe health status",
			InputSchema: InputSchema{
				Type:       "object",
				Properties: map[string]PropertySchema{},
			},
		},
	}
}

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	var req MCPRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		json.NewEncoder(w).Encode(MCPResponse{
			Error: &MCPError{Code: -32700, Message: "Parse error"},
		})
		return
	}

	var resp MCPResponse
	switch req.Method {
	case "list_tools":
		resp = MCPResponse{Result: map[string]interface{}{"tools": s.Tools()}, ID: req.ID}
	case "call_tool":
		resp = s.handleCallTool(req)
	case "health":
		resp = s.handleHealth()
	default:
		resp = MCPResponse{
			Error: &MCPError{Code: -32601, Message: fmt.Sprintf("Method not found: %s", req.Method)},
			ID:    req.ID,
		}
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(resp)
}

type CallToolParams struct {
	Name   string          `json:"name"`
	Params json.RawMessage `json:"params,omitempty"`
}

func (s *Server) handleCallTool(req MCPRequest) MCPResponse {
	var params CallToolParams
	if err := json.Unmarshal(req.Params, &params); err != nil {
		return MCPResponse{
			Error: &MCPError{Code: -32602, Message: "Invalid params: " + err.Error()},
			ID:    req.ID,
		}
	}

	switch params.Name {
	case "pipe_run":
		var args struct {
			Model       string `json:"model"`
			FullRefresh bool   `json:"full_refresh"`
		}
		json.Unmarshal(params.Params, &args)

		runID, err := s.runManager.StartRun(context.Background(), executor.RunRequest{
			ModelName:   args.Model,
			FullRefresh: args.FullRefresh,
		})
		if err != nil {
			return MCPResponse{
				Error: &MCPError{Code: -32000, Message: err.Error()},
				ID:    req.ID,
			}
		}
		return MCPResponse{
			Result: map[string]string{"run_id": runID, "status": "started"},
			ID:     req.ID,
		}

	case "pipe_compile":
		var args struct {
			Model string `json:"model"`
		}
		json.Unmarshal(params.Params, &args)

		resp, err := s.engineClient.Compile(executor.CompileRequest{Model: args.Model})
		if err != nil {
			return MCPResponse{
				Error: &MCPError{Code: -32000, Message: err.Error()},
				ID:    req.ID,
			}
		}
		return MCPResponse{Result: resp, ID: req.ID}

	case "pipe_lineage":
		lineage, err := s.engineClient.GetLineage()
		if err != nil {
			return MCPResponse{
				Error: &MCPError{Code: -32000, Message: err.Error()},
				ID:    req.ID,
			}
		}
		return MCPResponse{Result: lineage, ID: req.ID}

	case "pipe_list_runs":
		runs := s.runManager.ListRuns()
		return MCPResponse{Result: runs, ID: req.ID}

	case "pipe_get_run":
		var args struct {
			RunID string `json:"run_id"`
		}
		json.Unmarshal(params.Params, &args)

		run, ok := s.runManager.GetRun(args.RunID)
		if !ok {
			return MCPResponse{
				Error: &MCPError{Code: -32000, Message: "Run not found"},
				ID:    req.ID,
			}
		}
		return MCPResponse{Result: run, ID: req.ID}

	case "pipe_health":
		engineOK := s.engineClient.Health() == nil
		return MCPResponse{
			Result: map[string]interface{}{"status": "ok", "engine_ok": engineOK},
			ID:     req.ID,
		}

	default:
		return MCPResponse{
			Error: &MCPError{Code: -32602, Message: fmt.Sprintf("Unknown tool: %s", params.Name)},
			ID:    req.ID,
		}
	}
}

func (s *Server) handleHealth() MCPResponse {
	err := s.engineClient.Health()
	if err != nil {
		return MCPResponse{Result: map[string]interface{}{
			"status": "degraded",
			"error":  err.Error(),
		}}
	}
	return MCPResponse{Result: map[string]interface{}{"status": "ok"}}
}

func (s *Server) Start(addr string) error {
	log.Printf("OpenPipe MCP starting on %s", addr)
	mux := http.NewServeMux()
	mux.HandleFunc("/mcp", s.ServeHTTP)
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
	})
	return http.ListenAndServe(addr, mux)
}
