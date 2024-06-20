FROM rust:latest as builder

WORKDIR /

COPY . .

RUN cargo build --release && mv ./target/release/pointing_tool ./pointing_tool

RUN rustup default nightly
EXPOSE 8080

CMD ["./pointing_tool", ":8080"]
