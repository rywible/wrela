FROM --platform=linux/amd64 rust:1.92-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo run -p wrela -- build apps/ledger-lite -o /tmp/wrela_app --integration-mode

FROM --platform=linux/amd64 ubuntu:24.04
RUN apt-get update \
&& DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates \
&& rm -rf /var/lib/apt/lists/* \
&& useradd -m app
WORKDIR /app
COPY --from=build /tmp/wrela_app /app/wrela_app
RUN mkdir -p /data/wreladb && chown -R app:app /app /data
USER app
ENV PORT=8080
EXPOSE 8080
CMD ["/app/wrela_app"]
