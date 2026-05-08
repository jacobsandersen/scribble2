FROM rust:1.95-trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
RUN mkdir -p /tmp/libs && \
  ldd /app/target/release/scribble | grep "=> /" | awk '{print $3}' | \
  xargs -I {} cp {} /tmp/libs
RUN mkdir -p /tmp/ssh && \
  ssh-keyscan github.com >> /tmp/ssh/known_hosts && \
  ssh-keyscan gitlab.com >> /tmp/ssh/known_hosts && \
  ssh-keyscan bitbucket.org >> /tmp/ssh/known_hosts

FROM gcr.io/distroless/base-debian13:nonroot AS final
COPY --from=builder /tmp/libs /usr/lib
COPY --from=builder /tmp/ssh/known_hosts /home/nonroot/.ssh/known_hosts
WORKDIR /home/nonroot
COPY --from=builder /app/target/release/scribble ./main
USER nonroot:nonroot
EXPOSE 9000
ENV TZ=UTC
ENTRYPOINT ["/home/nonroot/main"]