# Feast API — one image for every country and every role (healthcheck: GET /healthz):
#   docker run feast-api all|gateway|worker|notification|migrate|seed|markets sync
# The country is chosen at deploy time: FEAST_COUNTRY=au (config/countries/au.yaml).
# Build context: this backend/ folder.

FROM rust:1.97-trixie AS build
WORKDIR /src
COPY rust/ rust/
COPY protos/ protos/
COPY migrations/ migrations/
COPY seed/ seed/
# Plain RUN (no BuildKit cache mounts) so every builder works: Docker, `az acr build`, Cloud Build.
RUN cd rust && cargo build --release --locked -p feast-server && cp target/release/feast /usr/local/bin/feast

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tini && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app feast && mkdir -p /app/data && chown feast /app/data
WORKDIR /app
COPY --from=build /usr/local/bin/feast /usr/local/bin/feast
COPY config/ /app/config/
ENV FEAST_CONFIG_DIR=/app/config \
    FEAST__HTTP__HOST=0.0.0.0 \
    FEAST__HTTP__PORT=8080 \
    FEAST__LOG__FORMAT=json \
    RUST_BACKTRACE=0
USER feast
EXPOSE 8080 50051
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /healthz HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q ok <&3"]
ENTRYPOINT ["/usr/bin/tini", "--", "feast"]
CMD ["all"]
