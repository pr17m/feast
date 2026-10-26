// Package config reads the same settings as the Rust server. In containers
// everything comes from environment variables with the same names
// (FEAST_COUNTRY, FEAST_ENV, FEAST__HTTP__PORT, FEAST__DATABASE__URL …), so a
// Go API image is a drop-in replacement behind the web app's /v1 proxy.
// TODO(go): also read config/default.toml + countries/<cc>.yaml (BurntSushi/toml, yaml.v3).
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

type Config struct {
	Country     string
	Env         string
	HTTPHost    string
	HTTPPort    int
	DatabaseURL string
	RedisURL    string
	ProxyHops   int
}

func env(key, def string) string {
	if v, ok := os.LookupEnv(key); ok {
		return v
	}
	return def
}

func Load() (Config, error) {
	c := Config{
		Country:     strings.ToUpper(env("FEAST_COUNTRY", "")),
		Env:         env("FEAST_ENV", "development"),
		HTTPHost:    env("FEAST__HTTP__HOST", "0.0.0.0"),
		DatabaseURL: env("FEAST__DATABASE__URL", ""),
		RedisURL:    env("FEAST__BUS__REDIS__URL", ""),
	}
	var err error
	if c.HTTPPort, err = strconv.Atoi(env("FEAST__HTTP__PORT", "8080")); err != nil {
		return c, fmt.Errorf("FEAST__HTTP__PORT: %w", err)
	}
	if c.ProxyHops, err = strconv.Atoi(env("FEAST__HTTP__PROXY_HOPS", "0")); err != nil {
		return c, fmt.Errorf("FEAST__HTTP__PROXY_HOPS: %w", err)
	}
	if len(c.Country) != 2 {
		return c, fmt.Errorf("no country given: set FEAST_COUNTRY (each deployment serves one country)")
	}
	return c, nil
}
