# syntax=docker/dockerfile:1

# ---- builder ----
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY . .
RUN npm run build

# ---- runtime ----
FROM nginx:alpine AS runtime
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
# SIGQUIT = nginx graceful shutdown (drain in-flight requests, then exit). Docker
# sends this on `stop`/`compose down` instead of the default SIGTERM (fast kill).
STOPSIGNAL SIGQUIT
