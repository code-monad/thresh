# Thresh - A birdge for CKB websocket notify of proposed transaction

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![Rust Version](https://img.shields.io/badge/rust-1.73+-blue.svg)

A high-performance WebSocket to MQTT bridge for monitoring CKB proposed transactions. Built with Rust for reliability and efficiency.

## Features

- Real-time monitoring of CKB proposed transactions
- Reliable WebSocket connection with automatic reconnection
- Efficient MQTT message batching and publishing
- Configurable retry mechanisms and error handling
- Concurrent processing capabilities
- Production-ready logging and monitoring

## Note

We recommend using [FlashMQ](https://www.flashmq.org) as the MQTT broker, for the best performance experience. We've provide a default FlashMQ configurations under `config/flashmq`. The default user/password is `thresh`.
But you can use any other brokers that supports MQTT 5.0, like [RabbitMQ](https://www.rabbitmq.com), [mosquitto](https://mosquitto.org), or you can try [Akasa](https://github.com/akasamq/akasa)

## Quick Start with Docker

### Using Docker Compose

Create a `docker-compose.yml`:

```yaml
version: '3'
services:
  thresh:
    image: nervape/thresh:latest
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - ./config:/app/config
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    networks:
      - ckb-network

  flashmq:
    image: codemonad/flashmq
    volumes:
      - ./config/flashmq:/etc/flashmq
    ports:
      - "1883:1883"
    restart: unless-stopped
    networks:
      - ckb-network

networks:
  ckb-network:
    driver: bridge
```

Start the services:

```bash
docker-compose up -d
```

### Using Docker Directly

Pull and run the image:

```bash
docker build . -t nervape/thresh:latest
docker run -d \
  -v $(pwd)/config:/app/config \
  -e RUST_LOG=info \
  --name thresh \
  nervape/thresh:latest
```

## Requirements

- Rust 1.70 or higher
- A running CKB node with WebSocket endpoint enabled
- An MQTT broker (e.g., Mosquitto, flashmq, akasa)

## Installation

```bash
git clone https://github.com/nervape/thresh
cd thresh
cargo build --release
```

## Configuration

Create a `config/config.yaml` file:

```yaml
websocket:
  endpoint: "ws://localhost:8114"
  retry_interval: 5
  max_retries: 3
  ping_interval: 30

mqtt:
  broker: "localhost"
  port: 1883
  topic: "ckb.transactions"
  client_id: "thresh"
  qos: 1
  reconnect_delay: 5
  batch_size: 50

processing:
  workers: 4
  queue_size: 1000
```

### Configuration Options

#### WebSocket
- `endpoint`: CKB node WebSocket endpoint
- `retry_interval`: Seconds between retry attempts
- `max_retries`: Maximum connection retry attempts
- `ping_interval`: Keep-alive ping interval in seconds

#### MQTT
- `broker`: MQTT broker address
- `port`: MQTT broker port
- `topic`: Topic to publish transactions
- `client_id`: Unique client identifier
- `qos`: Quality of Service (0-2)
- `reconnect_delay`: Reconnection delay in seconds
- `batch_size`: Number of messages to batch before publishing

#### Processing
- `workers`: Number of worker threads
- `queue_size`: Internal message queue size

## Usage

```bash
./target/release/thresh
```

## Development

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Running with Debug Logs

```bash
RUST_LOG=debug ./target/release/thresh
```

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Create a new Pull Request

## License

This project is licensed under the Apache License, Version 2.0 - see the LICENSE file for details.

## Contact

- Code Monad - code@lab-11.org, code@nervape.com
- Nervape Studio

## Acknowledgments

- CKB Team for the WebSocket API
- Tokio Team for the async runtime
- MQTT-rs Team for the MQTT client
