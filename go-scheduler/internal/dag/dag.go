package dag

import (
	"fmt"
	"sort"
	"strings"
)

type NodeType string

const (
	NodeModel  NodeType = "model"
	NodeSource NodeType = "source"
	NodeTest   NodeType = "test"
)

type Node struct {
	ID         string                 `json:"id"`
	Name       string                 `json:"name"`
	Type       NodeType               `json:"type"`
	Config     map[string]interface{} `json:"config,omitempty"`
	Status     string                 `json:"status"`
	DependsOn  []string               `json:"depends_on"`
}

type Edge struct {
	From      string `json:"from"`
	To        string `json:"to"`
	EdgeType  string `json:"edge_type"`
}

type DAG struct {
	Nodes      map[string]*Node `json:"nodes"`
	Edges      []Edge            `json:"edges"`
	OrderedIDs []string          `json:"ordered_ids"`
}

func New() *DAG {
	return &DAG{
		Nodes: make(map[string]*Node),
		Edges: make([]Edge, 0),
	}
}

func (d *DAG) AddNode(id string, name string, nodeType NodeType, config map[string]interface{}) *Node {
	node := &Node{
		ID:     id,
		Name:   name,
		Type:   nodeType,
		Config: config,
		Status: "pending",
	}
	d.Nodes[id] = node
	return node
}

func (d *DAG) AddEdge(from, to, edgeType string) {
	d.Edges = append(d.Edges, Edge{From: from, To: to, EdgeType: edgeType})
}

func (d *DAG) TopologicalSort() error {
	if len(d.OrderedIDs) > 0 {
		return nil
	}

	inDegree := make(map[string]int)
	for id := range d.Nodes {
		inDegree[id] = 0
		_ = id
	}

	adjList := make(map[string][]string)
	for id := range d.Nodes {
		adjList[id] = make([]string, 0)
	}

	for _, edge := range d.Edges {
		if _, ok := d.Nodes[edge.From]; !ok {
			return fmt.Errorf("node '%s' not found in DAG", edge.From)
		}
		if _, ok := d.Nodes[edge.To]; !ok {
			return fmt.Errorf("node '%s' not found in DAG", edge.To)
		}
		adjList[edge.From] = append(adjList[edge.From], edge.To)
		inDegree[edge.To]++
	}

	queue := make([]string, 0)
	for id, deg := range inDegree {
		if deg == 0 {
			queue = append(queue, id)
		}
	}

	var result []string
	for len(queue) > 0 {
		node := queue[0]
		queue = queue[1:]
		result = append(result, node)

		for _, neighbor := range adjList[node] {
			inDegree[neighbor]--
			if inDegree[neighbor] == 0 {
				queue = append(queue, neighbor)
			}
		}
	}

	if len(result) != len(d.Nodes) {
		var cycle []string
		for id, deg := range inDegree {
			if deg > 0 {
				cycle = append(cycle, id)
			}
		}
		return fmt.Errorf("cycle detected involving nodes: %s", strings.Join(cycle, ", "))
	}

	d.OrderedIDs = result
	return nil
}

func (d *DAG) SelectByTags(tags []string) ([]string, error) {
	tagSet := make(map[string]bool)
	for _, t := range tags {
		tagSet[t] = true
	}

	selected := make([]string, 0)
	for _, id := range d.OrderedIDs {
		node := d.Nodes[id]
		if node.Config == nil {
			continue
		}
		if nodeTags, ok := node.Config["tags"]; ok {
			if tagList, ok := nodeTags.([]string); ok {
				for _, t := range tagList {
					if tagSet[t] {
						selected = append(selected, id)
						break
					}
				}
			}
		}
	}
	return selected, nil
}

func (d *DAG) Subgraph(nodeIDs []string) *DAG {
	idSet := make(map[string]bool)
	for _, id := range nodeIDs {
		idSet[id] = true
	}

	sub := New()
	for _, id := range nodeIDs {
		if node, ok := d.Nodes[id]; ok {
			sub.Nodes[id] = node
		}
	}

	for _, edge := range d.Edges {
		if idSet[edge.From] && idSet[edge.To] {
			sub.Edges = append(sub.Edges, edge)
		}
	}

	sub.TopologicalSort()
	return sub
}

func (d *DAG) GetDownstream(nodeID string) []string {
	visited := make(map[string]bool)
	var downstream []string

	adjList := make(map[string][]string)
	for _, edge := range d.Edges {
		adjList[edge.From] = append(adjList[edge.From], edge.To)
	}

	var dfs func(id string)
	dfs = func(id string) {
		for _, neighbor := range adjList[id] {
			if !visited[neighbor] {
				visited[neighbor] = true
				downstream = append(downstream, neighbor)
				dfs(neighbor)
			}
		}
	}
	dfs(nodeID)

	sort.Strings(downstream)
	return downstream
}
