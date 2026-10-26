# Feast web app — one image for every country. At deploy time set:
#   FEAST_API_URL   "/" when the API is on the same domain (recommended), or its URL
#   FEAST_COUNTRY   e.g. "au"
# Build context: this webapp/ folder.

FROM node:22-alpine AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

FROM node:22-alpine AS build
WORKDIR /app
ENV NEXT_TELEMETRY_DISABLED=1
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npx next build

FROM node:22-alpine
WORKDIR /app
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 PORT=3000 HOSTNAME=0.0.0.0
RUN addgroup -S feast && adduser -S feast -G feast
COPY --from=build --chown=feast:feast /app/.next/standalone ./
COPY --from=build --chown=feast:feast /app/.next/static ./.next/static
COPY --from=build --chown=feast:feast /app/public ./public
USER feast
EXPOSE 3000
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s CMD wget -qO- http://127.0.0.1:3000/healthz >/dev/null || exit 1
CMD ["node", "server.js"]
