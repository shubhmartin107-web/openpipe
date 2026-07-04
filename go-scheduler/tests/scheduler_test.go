package tests

import (
	"testing"

	"github.com/openingest/openpipe/go-scheduler/internal/dag"
	"github.com/openingest/openpipe/go-scheduler/internal/scheduler"
	"github.com/openingest/openpipe/go-scheduler/internal/executor"
)

func TestDAGTopologicalSort(t *testing.T) {
	d := dag.New()
	d.AddNode("a", "Model A", dag.NodeModel, nil)
	d.AddNode("b", "Model B", dag.NodeModel, nil)
	d.AddNode("c", "Model C", dag.NodeModel, nil)
	d.AddEdge("a", "b", "ref")
	d.AddEdge("b", "c", "ref")

	err := d.TopologicalSort()
	if err != nil {
		t.Fatalf("TopologicalSort failed: %v", err)
	}

	// Verify order: a before b before c
	positions := make(map[string]int)
	for i, id := range d.OrderedIDs {
		positions[id] = i
	}

	if positions["a"] > positions["b"] {
		t.Errorf("Expected a before b, got a=%d, b=%d", positions["a"], positions["b"])
	}
	if positions["b"] > positions["c"] {
		t.Errorf("Expected b before c, got b=%d, c=%d", positions["b"], positions["c"])
	}
}

func TestDAGCycleDetection(t *testing.T) {
	d := dag.New()
	d.AddNode("a", "Model A", dag.NodeModel, nil)
	d.AddNode("b", "Model B", dag.NodeModel, nil)
	d.AddEdge("a", "b", "ref")
	d.AddEdge("b", "a", "ref") // cycle

	err := d.TopologicalSort()
	if err == nil {
		t.Fatal("Expected cycle detection error, got nil")
	}
}

func TestDAGDownstream(t *testing.T) {
	d := dag.New()
	d.AddNode("a", "Model A", dag.NodeModel, nil)
	d.AddNode("b", "Model B", dag.NodeModel, nil)
	d.AddNode("c", "Model C", dag.NodeModel, nil)
	d.AddNode("d", "Model D", dag.NodeModel, nil)
	d.AddEdge("a", "b", "ref")
	d.AddEdge("b", "c", "ref")
	d.AddEdge("a", "d", "ref")

	downstream := d.GetDownstream("a")
	if len(downstream) != 3 {
		t.Errorf("Expected 3 downstream nodes, got %d: %v", len(downstream), downstream)
	}
}

func TestDAGSubgraph(t *testing.T) {
	d := dag.New()
	d.AddNode("a", "A", dag.NodeModel, nil)
	d.AddNode("b", "B", dag.NodeModel, nil)
	d.AddNode("c", "C", dag.NodeModel, nil)
	d.AddEdge("a", "b", "ref")
	d.AddEdge("b", "c", "ref")

	sub := d.Subgraph([]string{"a", "b"})
	if len(sub.Nodes) != 2 {
		t.Errorf("Expected 2 nodes in subgraph, got %d", len(sub.Nodes))
	}
	if len(sub.Edges) != 1 {
		t.Errorf("Expected 1 edge in subgraph, got %d", len(sub.Edges))
	}
}

func TestRunManager(t *testing.T) {
	engineClient := executor.NewEngineClient("http://localhost:9090")
	sqlExecutor := executor.NewSQLLakehouseExecutor("stdout", "")
	rm := executor.NewRunManager(engineClient, sqlExecutor)

	// Should be able to list runs (empty initially)
	runs := rm.ListRuns()
	if len(runs) != 0 {
		t.Errorf("Expected 0 runs initially, got %d", len(runs))
	}
}

func TestSchedulerAddRemove(t *testing.T) {
	engineClient := executor.NewEngineClient("http://localhost:9090")
	sqlExecutor := executor.NewSQLLakehouseExecutor("stdout", "")
	rm := executor.NewRunManager(engineClient, sqlExecutor)

	s := scheduler.New(scheduler.Config{RunManager: rm})
	err := s.Start()
	if err != nil {
		t.Fatalf("Failed to start scheduler: %v", err)
	}
	defer s.Stop()

	err = s.AddSchedule(scheduler.ScheduleDef{
		Name: "test",
		Expr: "*/5 * * * * *",
	})
	if err != nil {
		t.Fatalf("Failed to add schedule: %v", err)
	}

	schedules := s.ListSchedules()
	if len(schedules) != 1 {
		t.Errorf("Expected 1 schedule, got %d", len(schedules))
	}

	s.RemoveSchedule("test")
	schedules = s.ListSchedules()
	if len(schedules) != 0 {
		t.Errorf("Expected 0 schedules after removal, got %d", len(schedules))
	}
}
