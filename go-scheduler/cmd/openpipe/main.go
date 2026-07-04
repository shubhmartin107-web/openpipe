package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/go-chi/chi/v5"
	chimw "github.com/go-chi/chi/v5/middleware"
	"github.com/go-chi/cors"

	"github.com/openingest/openpipe/go-scheduler/internal/api"
	"github.com/openingest/openpipe/go-scheduler/internal/executor"
	"github.com/openingest/openpipe/go-scheduler/internal/mcp"
	"github.com/openingest/openpipe/go-scheduler/internal/scheduler"
)

type Config struct {
	EngineURL   string `json:"engine_url"`
	APIAddr     string `json:"api_addr"`
	MCPAddr     string `json:"mcp_addr"`
	SQLConnStr  string `json:"sql_conn_str"`
	SQLDriver   string `json:"sql_driver"`
}

func main() {
	cfg := loadConfig()

	engineClient := executor.NewEngineClient(cfg.EngineURL)
	sqlExecutor := executor.NewSQLLakehouseExecutor(cfg.SQLDriver, cfg.SQLConnStr)
	runManager := executor.NewRunManager(engineClient, sqlExecutor)

	sched := scheduler.New(scheduler.Config{
		RunManager: runManager,
	})

	if err := sched.Start(); err != nil {
		log.Fatalf("Failed to start scheduler: %v", err)
	}

	// REST API
	apiHandler := api.NewHandler(runManager, sched, engineClient)

	r := chi.NewRouter()
	r.Use(chimw.Logger)
	r.Use(chimw.Recoverer)
	r.Use(chimw.Timeout(30 * time.Second))
	r.Use(cors.Handler(cors.Options{
		AllowedOrigins:   []string{"*"},
		AllowedMethods:   []string{"GET", "POST", "PUT", "DELETE", "OPTIONS"},
		AllowedHeaders:   []string{"Accept", "Authorization", "Content-Type"},
		AllowCredentials: false,
		MaxAge:           300,
	}))

	r.Route("/api/v1", func(r chi.Router) {
		r.Get("/health", apiHandler.Health)
		r.Get("/runs", apiHandler.ListRuns)
		r.Post("/runs", apiHandler.TriggerRun)
		r.Get("/runs/{runID}", apiHandler.GetRun)
		r.Get("/dag", apiHandler.GetDAG)
		r.Get("/models", apiHandler.ListModels)
		r.Get("/lineage", apiHandler.GetLineage)
		r.Post("/backfill", apiHandler.Backfill)
		r.Post("/webhook", apiHandler.WebhookHandler())
		r.Post("/webhooks/register", apiHandler.RegisterWebhook)
		r.Post("/webhooks/unregister/{name}", apiHandler.UnregisterWebhook)
		r.Get("/schedules", apiHandler.ListSchedules)
		r.Post("/schedules", apiHandler.AddSchedule)
		r.Delete("/schedules/{name}", apiHandler.RemoveSchedule)
	})

	// HTTP server
	httpServer := &http.Server{
		Addr:    cfg.APIAddr,
		Handler: r,
	}

	go func() {
		log.Printf("OpenPipe scheduler API listening on %s", cfg.APIAddr)
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("HTTP server error: %v", err)
		}
	}()

	// MCP Server
	mcpServer := mcp.New(runManager, engineClient, sched)
	go func() {
		log.Printf("OpenPipe MCP server listening on %s", cfg.MCPAddr)
		if err := mcpServer.Start(cfg.MCPAddr); err != nil {
			log.Printf("MCP server error: %v", err)
		}
	}()

	// Graceful shutdown
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down...")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	sched.Stop()
	httpServer.Shutdown(ctx)
}

func loadConfig() Config {
	cfg := Config{
		EngineURL:  getEnv("OPENPIPE_ENGINE_URL", "http://localhost:9090"),
		APIAddr:    getEnv("OPENPIPE_API_ADDR", ":8080"),
		MCPAddr:    getEnv("OPENPIPE_MCP_ADDR", ":8081"),
		SQLConnStr: getEnv("OPENPIPE_SQL_CONN", ""),
		SQLDriver:  getEnv("OPENPIPE_SQL_DRIVER", "spark"),
	}

	if path := os.Getenv("OPENPIPE_CONFIG"); path != "" {
		data, err := os.ReadFile(path)
		if err == nil {
			json.Unmarshal(data, &cfg)
		}
	}

	return cfg
}

func getEnv(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

func init() {
	log.SetFlags(log.LstdFlags | log.Lshortfile)
	fmt.Println("OpenPipe scheduler v0.1.0")
	fmt.Println("https://github.com/openingest/openpipe")
}
