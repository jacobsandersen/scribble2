FROM rust:1.95-trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
RUN mkdir -p /tmp/libs && \
  ldd /app/target/release/scribble | grep "=> /" | awk '{print $3}' | \
  xargs -I {} cp {} /tmp/libs

FROM gcr.io/distroless/base-debian13:nonroot AS final
COPY --from=builder /tmp/libs /usr/lib
WORKDIR /home/nonroot
COPY --from=builder /app/target/release/scribble ./main
USER nonroot:nonroot
EXPOSE 9000
ENV TZ=UTC
ENTRYPOINT ["/home/nonroot/main"]