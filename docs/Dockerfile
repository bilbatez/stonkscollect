# syntax=docker/dockerfile:1

# Static nginx image for the Docsify documentation site. Docsify renders the
# markdown client-side (hash routing), so nginx just serves the files as-is —
# no SPA fallback or build step needed.
FROM nginx:alpine
COPY . /usr/share/nginx/html
EXPOSE 80
