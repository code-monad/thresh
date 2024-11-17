use crate::{error::Result, websocket::TopicMessage};
use backoff::{Error as BackoffError, ExponentialBackoff};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error};

pub struct MqttPublisher {
    client: AsyncClient,
    rx: mpsc::Receiver<TopicMessage>,
    batch_size: usize,
    qos: QoS,
}

impl MqttPublisher {
    pub async fn new(
        broker: String,
        port: u16,
        client_id: String,
        rx: mpsc::Receiver<TopicMessage>,
        batch_size: usize,
        qos: QoS,
    ) -> Result<Self> {
        let mut mqttopts = MqttOptions::new(client_id, broker, port);
        mqttopts.set_keep_alive(Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(mqttopts, 10);

        tokio::spawn(async move {
            loop {
                if let Err(e) = eventloop.poll().await {
                    error!("MQTT event loop error: {}", e);
                }
            }
        });

        Ok(Self {
            client,
            rx,
            batch_size,
            qos: QoS::try_from(qos).unwrap_or(QoS::AtLeastOnce),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut message_batch = Vec::with_capacity(self.batch_size);

        while let Some(msg) = self.rx.recv().await {
            message_batch.push(msg);

            if message_batch.len() >= self.batch_size {
                self.publish_batch(&message_batch).await?;
                message_batch.clear();
            }
        }

        if !message_batch.is_empty() {
            self.publish_batch(&message_batch).await?;
        }

        Ok(())
    }

    async fn publish_batch(&self, batch: &[TopicMessage]) -> Result<()> {
        for msg in batch {
            match self
                .client
                .publish(&msg.topic, self.qos, false, msg.payload.clone())
                .await
            {
                Ok(_) => {
                    debug!("Published message to topic: {}", msg.topic);
                }
                Err(e) => {
                    error!("Failed to publish message to {}: {}", msg.topic, e);
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
}
