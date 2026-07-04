package executor

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"
)

type WebhookTrigger struct {
	mu         sync.RWMutex
	handlers   map[string]WebhookHandler
	httpServer *http.Server
}

type WebhookHandler struct {
	Name       string            `json:"name"`
	ModelName  string            `json:"model_name"`
	Tags       []string          `json:"tags"`
	FullRefresh bool             `json:"full_refresh"`
	Headers    map[string]string `json:"headers,omitempty"`
	Secret     string            `json:"secret,omitempty"`
}

type WebhookEvent struct {
	Event     string            `json:"event"`
	Payload   json.RawMessage   `json:"payload,omitempty"`
	Headers   map[string]string `json:"-"`
	Timestamp time.Time         `json:"timestamp"`
}

func NewWebhookTrigger() *WebhookTrigger {
	return &WebhookTrigger{
		handlers: make(map[string]WebhookHandler),
	}
}

func (w *WebhookTrigger) RegisterHandler(handler WebhookHandler) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.handlers[handler.Name] = handler
	log.Printf("Webhook handler registered: '%s' → model '%s'", handler.Name, handler.ModelName)
}

func (w *WebhookTrigger) UnregisterHandler(name string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	delete(w.handlers, name)
	log.Printf("Webhook handler unregistered: '%s'", name)
}

func (w *WebhookTrigger) HandleEvent(event WebhookEvent, runFn func(ctx context.Context, req RunRequest) (string, error)) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	for name, handler := range w.handlers {
		if handler.Secret != "" {
			if secret, ok := event.Headers["X-Webhook-Secret"]; !ok || secret != handler.Secret {
				continue
			}
		}

		log.Printf("Webhook '%s' triggered by event '%s'", name, event.Event)
		runID, err := runFn(context.Background(), RunRequest{
			ModelName:   handler.ModelName,
			Tags:        handler.Tags,
			FullRefresh: handler.FullRefresh,
		})
		if err != nil {
			log.Printf("Webhook '%s' run failed: %v", name, err)
		} else {
			log.Printf("Webhook '%s' started run: %s", name, runID)
		}
	}
}

func (w *WebhookTrigger) ServeHTTP(runFn func(ctx context.Context, req RunRequest) (string, error)) http.HandlerFunc {
	return func(rw http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			http.Error(rw, "Method not allowed", http.StatusMethodNotAllowed)
			return
		}

		var payload json.RawMessage
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			http.Error(rw, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
			return
		}

		event := WebhookEvent{
			Event:     r.URL.Query().Get("event"),
			Payload:   payload,
			Headers:   make(map[string]string),
			Timestamp: time.Now(),
		}

		for k, v := range r.Header {
			if len(v) > 0 {
				event.Headers[k] = v[0]
			}
		}

		w.HandleEvent(event, runFn)

		rw.Header().Set("Content-Type", "application/json")
		json.NewEncoder(rw).Encode(map[string]interface{}{
			"status":       "received",
			"event":        event.Event,
			"timestamp":    event.Timestamp,
			"handlers":     len(w.handlers),
		})
	}
}
