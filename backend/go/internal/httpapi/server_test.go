package httpapi

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestContractBasics(t *testing.T) {
	h := NewRouter("AU", nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))
	if rec.Code != 200 || !strings.Contains(rec.Body.String(), `"status":"ok"`) {
		t.Fatalf("healthz: %d %s", rec.Code, rec.Body)
	}
	rec = httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/v1/markets", nil))
	if rec.Code != 501 || !strings.Contains(rec.Body.String(), `"code":"NOT_IMPLEMENTED"`) {
		t.Fatalf("unported route: %d %s", rec.Code, rec.Body)
	}
}
