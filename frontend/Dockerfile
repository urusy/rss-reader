# syntax=docker/dockerfile:1

# --- Build ----------------------------------------------------------------
FROM node:22-alpine AS build
WORKDIR /app
RUN corepack enable
# lockfile 込みで再現ビルド（監査 #7: lockfile 無しだと毎ビルドで依存が動く）
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY . .
RUN pnpm build

# --- Runtime (static serving + API proxy) ---------------------------------
FROM nginx:1.27-alpine AS runtime
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
