use crate::{error::Error, websocket::TopicMessage};
use futures::future::try_join_all;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time;
use tracing::{debug, error, info, warn};

pub struct MqttPublisher {
    client: AsyncClient,
    rx: mpsc::Receiver<TopicMessage>,
    batch_size: usize,
    qos: QoS,
    connection_status: watch::Receiver<bool>,
}

impl MqttPublisher {
    pub async fn new(
        broker: String,
        port: u16,
        client_id: String,
        username: Option<String>,
        password: Option<String>,
        rx: mpsc::Receiver<TopicMessage>,
        batch_size: usize,
        qos: QoS,
        _reconnect_delay: Duration, // No longer directly used, rumqttc handles this
    ) -> Result<Self, Error> {
        let mut mqttopts = MqttOptions::new(client_id, broker, port);
        mqttopts.set_clean_session(false);
        mqttopts.set_keep_alive(Duration::from_secs(30));

        if let (Some(u), Some(p)) = (username, password) {
            mqttopts.set_credentials(u, p);
        }

        let (client, eventloop) = AsyncClient::new(mqttopts, 100);
        let (tx, rx_watch) = watch::channel(false);

        tokio::spawn(handle_eventloop(eventloop, tx));

        let publisher = Self {
            client,
            rx,
            batch_size,
            qos,
            connection_status: rx_watch,
        };

        Ok(publisher)
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        let mut message_batch = Vec::with_capacity(self.batch_size);
        let mut tick_interval = time::interval(Duration::from_secs(1));

        loop {
            let is_connected = *self.connection_status.borrow();

            tokio::select! {
                biased;

                msg = self.rx.recv() => {
                    if let Some(msg) = msg {
                        if is_connected {
                            message_batch.push(msg);
                            if message_batch.len() >= self.batch_size {
                                self.publish_batch(&mut message_batch).await;
                            }
                        } else {
                            warn!("MQTT disconnected, discarding message for topic: {}", msg.topic);
                        }
                    } else {
                        info!("Message channel closed. MQTT publisher shutting down.");
                        break; // Exit loop when the sender is dropped
                    }
                },

                _ = tick_interval.tick() => {
                    if is_connected && !message_batch.is_empty() {
                        self.publish_batch(&mut message_batch).await;
                    }
                }
            }
        }

        if !message_batch.is_empty() {
            if *self.connection_status.borrow() {
                info!(
                    "Publishing final batch of {} messages before shutdown.",
                    message_batch.len()
                );
                self.publish_batch(&mut message_batch).await;
            } else {
                warn!(
                    "Skipping final batch of {} messages due to MQTT disconnection.",
                    message_batch.len()
                );
            }
        }

        info!("MQTT publisher shutting down.");
        if let Err(e) = self.client.disconnect().await {
            error!("Failed to disconnect MQTT client cleanly: {}", e);
        }

        Ok(())
    }

    async fn publish_batch(&self, batch: &mut Vec<TopicMessage>) {
        if batch.is_empty() {
            return;
        }

        info!("Publishing batch of {} messages.", batch.len());

        let publishes = batch.drain(..).map(|msg| {
            let client = self.client.clone();
            async move {
                match client
                    .publish(&msg.topic, self.qos, false, msg.payload.as_bytes())
                    .await
                {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        error!(
                            "Failed to publish message to topic {}: {}. Error: {}",
                            msg.topic, msg.payload, e
                        );
                        Err(e)
                    }
                }
            }
        });

        if let Err(_e) = try_join_all(publishes).await {
            error!("Aborting batch publish due to error.");
        }
    }
}

async fn handle_eventloop(mut eventloop: EventLoop, status_sender: watch::Sender<bool>) {
    info!("MQTT event loop started.");

    loop {
        match eventloop.poll().await {
            Ok(event) => {
                if let Event::Incoming(Packet::ConnAck(_)) = event {
                    info!("MQTT connection successfully established.");
                    status_sender.send_if_modified(|is_connected| {
                        if !*is_connected {
                            *is_connected = true;
                            true
                        } else {
                            false
                        }
                    });
                }
                debug!("Received MQTT event: {:?}", event);
            }
            Err(e) => {
                error!("MQTT event loop error: {}. Reconnecting...", e);
                status_sender.send_if_modified(|is_connected| {
                    if *is_connected {
                        *is_connected = false;
                        true
                    } else {
                        false
                    }
                });
                time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
