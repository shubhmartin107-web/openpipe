package executor

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

type EngineClient struct {
	baseURL    string
	httpClient *http.Client
}

type LoadProjectRequest struct {
	Path string `json:"path"`
}

type CompileRequest struct {
	Model       string `json:"model,omitempty"`
	FullRefresh bool   `json:"full_refresh"`
}

type CompileResponse struct {
	Models []CompiledModel `json:"models"`
	DagEdges []DAGEdge     `json:"dag_edges"`
}

type CompiledModel struct {
	Name           string                 `json:"name"`
	CompiledSQL    string                 `json:"compiled_sql"`
	Config         map[string]interface{} `json:"config"`
	Materialization string                `json:"materialization"`
	RelationName   string                 `json:"relation_name"`
	DependsOn      []string               `json:"depends_on"`
}

type DAGEdge struct {
	From     string `json:"from"`
	To       string `json:"to"`
	EdgeType string `json:"edge_type"`
}

type LineageResult struct {
	Models []ModelLineage `json:"models"`
}

type ModelLineage struct {
	ModelName      string                    `json:"model_name"`
	RelationName   string                    `json:"relation_name"`
	Columns        map[string]ColumnLineage  `json:"columns"`
	InputDatasets  []string                  `json:"input_datasets"`
	OutputDataset  string                    `json:"output_dataset"`
}

type ColumnLineage struct {
	InputFields           []InputField `json:"input_fields"`
	TransformationType    string       `json:"transformation_type"`
	TransformationSubtype string       `json:"transformation_subtype"`
}

type InputField struct {
	Dataset        string           `json:"dataset"`
	Field          string           `json:"field"`
	Transformations []Transformation `json:"transformations"`
}

type Transformation struct {
	Type        string  `json:"type"`
	Subtype     string  `json:"subtype"`
	Description *string `json:"description,omitempty"`
	Masking     *bool   `json:"masking,omitempty"`
}

type ProjectInfo struct {
	Name    string       `json:"name"`
	Models  []ModelInfo  `json:"models"`
	Sources []SourceInfo `json:"sources"`
}

type ModelInfo struct {
	Name   string `json:"name"`
	Config map[string]interface{} `json:"config"`
}

type SourceInfo struct {
	Name   string        `json:"name"`
	Schema string        `json:"schema,omitempty"`
	Tables []SourceTable `json:"tables"`
}

type SourceTable struct {
	Name   string `json:"name"`
	Identifier string `json:"identifier,omitempty"`
}

func NewEngineClient(baseURL string) *EngineClient {
	return &EngineClient{
		baseURL: baseURL,
		httpClient: &http.Client{
			Timeout: 60 * time.Second,
		},
	}
}

func (c *EngineClient) Health() error {
	resp, err := c.httpClient.Get(c.baseURL + "/api/v1/health")
	if err != nil {
		return fmt.Errorf("engine unreachable: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("engine returned status %d", resp.StatusCode)
	}
	return nil
}

func (c *EngineClient) LoadProject(path string) (*ProjectInfo, error) {
	body := LoadProjectRequest{Path: path}
	data, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("marshal error: %w", err)
	}

	resp, err := c.httpClient.Post(c.baseURL+"/api/v1/project/load", "application/json", bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("load project error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("engine error (%d): %s", resp.StatusCode, string(respBody))
	}

	var project ProjectInfo
	if err := json.NewDecoder(resp.Body).Decode(&project); err != nil {
		return nil, fmt.Errorf("decode error: %w", err)
	}
	return &project, nil
}

func (c *EngineClient) Compile(request CompileRequest) (*CompileResponse, error) {
	data, err := json.Marshal(request)
	if err != nil {
		return nil, fmt.Errorf("marshal error: %w", err)
	}

	// Remove trailing slash issue
	url := c.baseURL + "/api/v1/compile"
	resp, err := c.httpClient.Post(url, "application/json", bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("compile error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("engine error (%d): %s", resp.StatusCode, string(respBody))
	}

	var result CompileResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode error: %w", err)
	}
	return &result, nil
}

func (c *EngineClient) GetLineage() (*LineageResult, error) {
	resp, err := c.httpClient.Post(c.baseURL+"/api/v1/lineage", "application/json", nil)
	if err != nil {
		return nil, fmt.Errorf("lineage error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("engine error (%d): %s", resp.StatusCode, string(respBody))
	}

	var result LineageResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode error: %w", err)
	}
	return &result, nil
}
