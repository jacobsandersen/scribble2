FROM rust:1.95-trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
RUN mkdir -p /tmp/ssh && \
  ssh-keyscan github.com >> /tmp/ssh/known_hosts && \
  ssh-keyscan gitlab.com >> /tmp/ssh/known_hosts && \
  ssh-keyscan bitbucket.org >> /tmp/ssh/known_hosts

FROM gcr.io/distroless/cc-debian13:nonroot AS final
COPY --from=builder /tmp/ssh/known_hosts /home/nonroot/.ssh/known_hosts
COPY --from=builder /app/target/release/scribble /home/nonroot/main
USER nonroot:nonroot
EXPOSE 9000
ENV TZ=UTC
ENTRYPOINT ["/home/nonroot/main"]