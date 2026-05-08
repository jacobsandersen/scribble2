FROM rust:1.95-bullseye AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/base-debian13:nonroot AS final
WORKDIR /home/nonroot
COPY --from=builder /app/target/release/scribble ./main
USER nonroot:nonroot
EXPOSE 9000
ENV TZ=UTC
ENTRYPOINT ["/home/nonroot/main"]