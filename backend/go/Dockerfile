# Go port of the Feast API (scaffold). Drop-in for the Rust image: same env vars, port 8080.
FROM golang:1.24-alpine AS build
WORKDIR /src
COPY . .
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /feast ./cmd/feast

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /feast /feast
EXPOSE 8080
ENTRYPOINT ["/feast"]
