# syntax=docker/dockerfile:1.7
# 42ctl container image. Multi-stage: build with the pinned Rust toolchain, then ship the
# single binary on distroless (non-root). P6 switches the runtime to scratch+musl for a
# truly minimal multi-arch image and adds cosign signing + SBOM/provenance. For now this
# is a reproducible build proven in CI (no push).
# Builder uses Docker Hub official image (not public.ecr.aws) to avoid unauthenticated
# ECR Public data-limit 429s on shared GHA runner IPs.
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /build
COPY . .
ARG FT_GIT_SHA=unknown
ENV FT_GIT_SHA=${FT_GIT_SHA}
RUN cargo build --release && cp target/release/42ctl /42ctl

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /42ctl /42ctl
USER nonroot:nonroot
ENTRYPOINT ["/42ctl"]
