package executor

import (
	"context"
	"fmt"
	"log"

	"github.com/openingest/openpipe/go-scheduler/internal/dag"
)

func (m *RunManager) Backfill(ctx context.Context, req RunRequest, modelDAG *dag.DAG) (string, error) {
	var modelsToRun []string

	if req.ModelName != "" {
		// Run the model and all downstream models
		modelsToRun = append(modelsToRun, req.ModelName)
		downstream := modelDAG.GetDownstream(req.ModelName)
		modelsToRun = append(modelsToRun, downstream...)
		log.Printf("Backfill '%s' will run %d models (self + %d downstream)", req.ModelName, len(modelsToRun), len(downstream))
	} else if len(req.Tags) > 0 {
		selected, err := modelDAG.SelectByTags(req.Tags)
		if err != nil {
			return "", fmt.Errorf("tag selection: %w", err)
		}
		modelsToRun = selected
	} else {
		return m.StartRun(ctx, RunRequest{
			FullRefresh: true,
		})
	}

	// Deduplicate while preserving order
	seen := make(map[string]bool)
	var deduped []string
	for _, m := range modelsToRun {
		if !seen[m] {
			seen[m] = true
			deduped = append(deduped, m)
		}
	}

	// Run each model sequentially (respecting dependency order)
	var lastRunID string
	for _, modelName := range deduped {
		runID, err := m.StartRun(ctx, RunRequest{
			ModelName:   modelName,
			FullRefresh: true,
		})
		if err != nil {
			return lastRunID, fmt.Errorf("backfill failed at '%s': %w", modelName, err)
		}
		lastRunID = runID
		log.Printf("Backfill: queued '%s' as run %s", modelName, runID)
	}

	return lastRunID, nil
}
