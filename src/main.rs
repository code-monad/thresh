use std::time::Duration;
use thresh::{config::Config, error::Result, mqtt::MqttPublisher, websocket::WebSocketClient};
use tokio::sync::mpsc;
use tracing::{error, info}; // Add Duration import

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load config
    let config = Config::from_file("config/config.yaml")?;

    loop {
        info!("Starting CKB WebSocket monitor service...");

        let (tx, rx) = mpsc::channel(config.processing.queue_size);

        let ws_client = WebSocketClient::new(
            config.websocket.endpoint.clone(),
            tx,
            config.websocket.clone(),
        );

        let mut mqtt_publisher = MqttPublisher::new(
            config.mqtt.broker.clone(),
            config.mqtt.port,
            config.mqtt.client_id.clone(),
            rx,
            config.mqtt.batch_size, // Add batch_size parameter,
            config.mqtt.qos,
        )
        .await?;

        let result = tokio::select! {
            res = ws_client.run() => res,
            res = mqtt_publisher.run() => res,
        };

        match result {
            Ok(_) => {
                info!("Service completed successfully");
                break;
            }
            Err(e) => {
                error!("Service error: {}. Restarting in 5 seconds...", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}
