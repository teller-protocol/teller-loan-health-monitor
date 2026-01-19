
# build with 

#    docker build -t scrapebot .

 

#  docker build -t scrapebot --target scrapebot_runtime .

#  docker build -t watchlister --target watchlister_runtime .




# compile the binary 
FROM rust:1.85.0-slim-bullseye AS builder
WORKDIR /app
COPY Cargo.toml /app/
COPY Cargo.lock /app/
COPY src /app/src/
RUN apt update && apt install -y pkg-config libssl-dev ca-certificates
RUN cargo build --release




#set up the scrapebot_runtime image with the binary and env
FROM debian:bullseye-slim AS combined_bot_runtime
WORKDIR /app
RUN apt update && apt install -y ca-certificates
COPY --from=builder /app/target/release/health_bot /app/health_bot
COPY --from=builder /app/src/endpoints.ron /app/src/endpoints.ron
#COPY .env /app/.env

#run the app
ENTRYPOINT ["/app/health_bot"]

 

  