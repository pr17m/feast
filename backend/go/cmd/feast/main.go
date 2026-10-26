// feast (Go) — scaffold of the Go port of the Feast API. Same env config,
// same contract, same database schema as the Rust server; see GO_PORTING_PLAN.md.
package main

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/feast/backend/go/internal/config"
	"github.com/feast/backend/go/internal/httpapi"
)

func main() {
	log := slog.New(slog.NewJSONHandler(os.Stdout, nil))
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "Error:", err)
		os.Exit(1)
	}
	srv := &http.Server{
		Addr:              fmt.Sprintf("%s:%d", cfg.HTTPHost, cfg.HTTPPort),
		Handler:           httpapi.NewRouter(cfg.Country, nil),
		ReadHeaderTimeout: 10 * time.Second,
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() {
		log.Info("HTTP API listening", "addr", srv.Addr, "country", cfg.Country, "env", cfg.Env, "backend", "go")
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Error("server failed", "error", err)
			os.Exit(1)
		}
	}()
	<-ctx.Done()
	shutdown, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutdown)
	log.Info("stopped")
}
