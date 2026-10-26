// Package httpapi is the HTTP delivery layer: same routes, JSON shapes and
// `{code, message}` errors as the Rust gateway (contract: ../spec/openapi.yaml).
package httpapi

import (
	"encoding/json"
	"net/http"
	"time"
)

type Error struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func WriteJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("content-type", "application/json")
	w.Header().Set("x-content-type-options", "nosniff")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func WriteError(w http.ResponseWriter, status int, code, msg string) {
	WriteJSON(w, status, Error{Code: code, Message: msg})
}

// Readiness is a dependency the /readyz probe checks.
type Readiness interface {
	Name() string
	Check(r *http.Request) error
}

// NewRouter wires the routes ported so far; everything else in the contract
// answers 501 so conformance tests show exactly what's left.
func NewRouter(country string, ready []Readiness) http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		WriteJSON(w, 200, map[string]any{"status": "ok", "country": country, "backend": "go"})
	})
	mux.HandleFunc("GET /readyz", func(w http.ResponseWriter, r *http.Request) {
		checks, ok := map[string]string{}, true
		for _, c := range ready {
			if err := c.Check(r); err != nil {
				checks[c.Name()], ok = err.Error(), false
			} else {
				checks[c.Name()] = "ok"
			}
		}
		status := 200
		if !ok {
			status = 503
		}
		WriteJSON(w, status, map[string]any{"status": map[bool]string{true: "ok", false: "degraded"}[ok], "checks": checks})
	})
	mux.HandleFunc("/v1/", func(w http.ResponseWriter, r *http.Request) {
		WriteError(w, http.StatusNotImplemented, "NOT_IMPLEMENTED", "Not ported to the Go backend yet: "+r.Method+" "+r.URL.Path)
	})
	return withMiddleware(mux)
}

func withMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		defer func() {
			if rec := recover(); rec != nil {
				WriteError(w, 500, "INTERNAL", "Something went wrong. Please try again.")
			}
			_ = start // TODO(go): structured request logging + metrics (match Rust's /metrics names)
		}()
		next.ServeHTTP(w, r)
	})
}
