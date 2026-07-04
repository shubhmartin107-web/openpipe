package scheduler

import (
	"context"
	"log"
	"sync"
	"time"

	"github.com/robfig/cron/v3"

	"github.com/openingest/openpipe/go-scheduler/internal/executor"
)

type Config struct {
	RunManager *executor.RunManager
}

type Scheduler struct {
	cron       *cron.Cron
	runManager *executor.RunManager
	events     chan Event
	mu         sync.RWMutex
	schedules  map[string]*ScheduleEntry
}

type Event struct {
	Type    string      `json:"type"`
	Payload interface{} `json:"payload"`
}

type ScheduleEntry struct {
	ID        cron.EntryID
	Name      string
	Expr      string
	ModelName string
	Tags      []string
}

type ScheduleDef struct {
	Name      string   `json:"name"`
	Expr      string   `json:"expr"`
	ModelName string   `json:"model_name,omitempty"`
	Tags      []string `json:"tags,omitempty"`
}

func New(cfg Config) *Scheduler {
	return &Scheduler{
		cron:       cron.New(cron.WithSeconds()),
		runManager: cfg.RunManager,
		events:     make(chan Event, 100),
		schedules:  make(map[string]*ScheduleEntry),
	}
}

func (s *Scheduler) Start() error {
	s.cron.Start()
	log.Println("Scheduler started")
	return nil
}

func (s *Scheduler) Stop() {
	ctx := s.cron.Stop()
	<-ctx.Done()
	log.Println("Scheduler stopped")
}

func (s *Scheduler) AddSchedule(sd ScheduleDef) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	entryID, err := s.cron.AddFunc(sd.Expr, func() {
		log.Printf("Cron trigger: %s at %s", sd.Name, time.Now().Format(time.RFC3339))

		ctx := context.Background()
		runID, err := s.runManager.StartRun(ctx, executor.RunRequest{
			ModelName: sd.ModelName,
			Tags:      sd.Tags,
			FullRefresh: false,
		})
		if err != nil {
			log.Printf("Cron run failed for %s: %v", sd.Name, err)
			return
		}
		log.Printf("Cron run started: %s (run %s)", sd.Name, runID)
	})
	if err != nil {
		return err
	}

	s.schedules[sd.Name] = &ScheduleEntry{
		ID:        entryID,
		Name:      sd.Name,
		Expr:      sd.Expr,
		ModelName: sd.ModelName,
		Tags:      sd.Tags,
	}

	log.Printf("Schedule added: %s (%s)", sd.Name, sd.Expr)
	return nil
}

func (s *Scheduler) RemoveSchedule(name string) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if entry, ok := s.schedules[name]; ok {
		s.cron.Remove(entry.ID)
		delete(s.schedules, name)
		log.Printf("Schedule removed: %s", name)
	}
}

func (s *Scheduler) ListSchedules() []ScheduleEntry {
	s.mu.RLock()
	defer s.mu.RUnlock()

	result := make([]ScheduleEntry, 0, len(s.schedules))
	for _, entry := range s.schedules {
		result = append(result, *entry)
	}
	return result
}

func (s *Scheduler) Events() <-chan Event {
	return s.events
}

func (s *Scheduler) emit(event Event) {
	select {
	case s.events <- event:
	default:
	}
}
