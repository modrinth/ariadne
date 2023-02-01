FROM rust:1.65 as build
ENV PKG_CONFIG_ALLOW_CROSS=1

WORKDIR /usr/src/ariadne

# Copy everything
COPY . .
# Add the wait script
ADD https://github.com/ufoscout/docker-compose-wait/releases/download/2.9.0/wait /wait
RUN chmod +x /wait
# Build our code
RUN cargo build --release


FROM debian:bullseye-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

RUN update-ca-certificates

COPY --from=build /usr/src/ariadne/target/release/ariadne /ariadne/ariadne
COPY --from=build /wait /wait
WORKDIR /ariadne

CMD /wait && /ariadne/ariadne