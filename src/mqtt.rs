use crate::{error::Result, websocket::TopicMessage};
use rumqttc::{AsyncClient, ConnectionError, Event, EventLoop, MqttOptions, QoS};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time;
use tracing::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub struct MqttPublisher {
    client: AsyncClient,
    eventloop: Option<EventLoop>,
    rx: mpsc::Receiver<TopicMessage>,
    batch_size: usize,
    qos: QoS,
    broker: String,
    port: u16,
    client_id: String,
    username: Option<String>,
    password: Option<String>,
    reconnect_delay: Duration,
    health_sender: watch::Sender<bool>,
    health_receiver: watch::Receiver<bool>,
    // Track reconnection attempts and status
    reconnect_attempts: Arc<AtomicU32>,
    is_reconnecting: Arc<AtomicBool>,
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
        reconnect_delay: Duration,
    ) -> Result<Self> {
        let (health_sender, health_receiver) = watch::channel(false);
        
        // Create initial MqttOptions
        let mut mqttopts = MqttOptions::new(&client_id, &broker, port);
        if username.is_some() {
            mqttopts.set_credentials(
                username.clone().unwrap_or_default(), 
                password.clone().unwrap_or_default()
            );
        }
        mqttopts.set_keep_alive(Duration::from_secs(30));
        mqttopts.set_max_packet_size(10 * 1024 * 1024, 10 * 1024 * 1024); // 10MB for both send and receive
        
        // Use clean_session(false) to maintain session state on server
        mqttopts.set_clean_session(false);
        
        // Set manual_acks to false to let the library handle acks automatically
        mqttopts.set_manual_acks(false);
        
        // Create client and eventloop
        let (client, eventloop) = AsyncClient::new(mqttopts, 10);
        
        let publisher = Self {
            client,
            eventloop: Some(eventloop),
            rx,
            batch_size,
            qos,
            broker,
            port,
            client_id,
            username,
            password,
            reconnect_delay,
            health_sender,
            health_receiver,
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
            is_reconnecting: Arc::new(AtomicBool::new(false)),
        };
        
        // Set health to false initially
        let _ = publisher.health_sender.send(false);
        
        Ok(publisher)
    }
    
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to MQTT broker: {}:{}", self.broker, self.port);
        
        let mut mqttopts = MqttOptions::new(&self.client_id, &self.broker, self.port);
        
        if self.username.is_some() {
            mqttopts.set_credentials(
                self.username.clone().unwrap_or_default(), 
                self.password.clone().unwrap_or_default()
            );
        }

        mqttopts.set_keep_alive(Duration::from_secs(30));
        mqttopts.set_max_packet_size(10 * 1024 * 1024, 10 * 1024 * 1024); // 10MB for both send and receive
        
        // Use clean_session(false) to maintain session state on server
        mqttopts.set_clean_session(false);
        
        // Set manual_acks to false to let the library handle acks automatically
        mqttopts.set_manual_acks(false);
        
        // Create a new client and eventloop
        let (client, eventloop) = AsyncClient::new(mqttopts, 100);
        self.client = client;
        self.eventloop = Some(eventloop);
        
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        // Spawn the event loop handler
        let health_sender = self.health_sender.clone();
        let mut eventloop = self.eventloop.take().expect("EventLoop should be initialized");
        let reconnect_attempts = self.reconnect_attempts.clone();
        let is_reconnecting = self.is_reconnecting.clone();
        
        let event_handle = tokio::spawn(async move {
            let mut last_successful_poll = std::time::Instant::now();
            
            loop {
                // Check if we're in a stalled state - no successful poll for too long
                let elapsed = last_successful_poll.elapsed();
                if elapsed > Duration::from_secs(60) {  // No successful poll for 1 minute
                    error!("MQTT connection appears stalled - no events for {} seconds", elapsed.as_secs());
                    let _ = health_sender.send(false);
                    break;
                }
                
                match eventloop.poll().await {
                    Ok(Event::Incoming(packet)) => {
                        last_successful_poll = std::time::Instant::now();
                        
                        match packet {
                            rumqttc::Packet::ConnAck(_) => {
                                info!("MQTT connection established");
                                let _ = health_sender.send(true);
                                reconnect_attempts.store(0, Ordering::SeqCst);
                                is_reconnecting.store(false, Ordering::SeqCst);
                            },
                            rumqttc::Packet::PingResp => {
                                debug!("Received MQTT ping response");
                            },
                            rumqttc::Packet::Disconnect => {
                                info!("MQTT client disconnected");
                                let _ = health_sender.send(false);
                                break;
                            },
                            _ => {
                                // Other packets we don't need to handle specifically
                            }
                        }
                    },
                    Ok(_) => {
                        last_successful_poll = std::time::Instant::now();
                        // Other events we don't need to handle specifically
                    },
                    Err(e) => {
                        match e {
                            ConnectionError::Io(io_err) => {
                                error!("MQTT IO error: {}", io_err);
                            }
                            ConnectionError::MqttState(state_err) => {
                                error!("MQTT state error: {}", state_err);
                            }
                            _ => {
                                error!("MQTT connection error: {}", e);
                            }
                        }
                        let _ = health_sender.send(false);
                        
                        // Sleep before exiting the event loop
                        time::sleep(Duration::from_secs(1)).await;
                        break;
                    }
                }
            }
            
            Result::<()>::Ok(())
        });

        let mut message_batch = Vec::with_capacity(self.batch_size);
        let mut health_check_interval = time::interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                // Process incoming messages
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => {
                            message_batch.push(msg);
                            
                            if message_batch.len() >= self.batch_size {
                                if let Err(e) = self.publish_batch(&message_batch).await {
                                    error!("Failed to publish batch: {}", e);
                                    
                                    // If publishing fails, check connection health
                                    if !*self.health_receiver.borrow() && 
                                       !self.is_reconnecting.load(Ordering::SeqCst) {
                                        warn!("MQTT connection appears to be down, attempting to reconnect...");
                                        if let Err(e) = self.reconnect().await {
                                            error!("Failed to reconnect to MQTT broker: {}", e);
                                        }
                                    }
                                }
                                message_batch.clear();
                            }
                        }
                        None => {
                            info!("Channel closed, shutting down MQTT publisher");
                            break;
                        }
                    }
                }
                
                // Periodic health check
                _ = health_check_interval.tick() => {
                    // Only attempt reconnection if not already reconnecting
                    if !*self.health_receiver.borrow() && !self.is_reconnecting.load(Ordering::SeqCst) {
                        warn!("MQTT connection health check failed, attempting to reconnect...");
                        if let Err(e) = self.reconnect().await {
                            error!("Failed to reconnect to MQTT broker: {}", e);
                        }
                    } else {
                        // Check connection health via regular health check
                        debug!("MQTT connection health check passed");
                    }
                }
            }
        }

        // Publish any remaining messages
        if !message_batch.is_empty() {
            if let Err(e) = self.publish_batch(&message_batch).await {
                error!("Failed to publish final batch: {}", e);
            }
        }

        // Clean shutdown
        if let Err(e) = self.client.disconnect().await {
            error!("Error disconnecting from MQTT broker: {}", e);
        }
        
        // Abort the event loop task
        event_handle.abort();
        
        Ok(())
    }
    
    async fn reconnect(&mut self) -> Result<()> {
        // Mark that we're attempting to reconnect
        self.is_reconnecting.store(true, Ordering::SeqCst);
        
        let attempt = self.reconnect_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        info!("Attempting to reconnect to MQTT broker (attempt #{})...", attempt);
        
        // Disconnect cleanly if possible
        let _ = self.client.disconnect().await;
        
        // Implement exponential backoff with maximum delay cap
        let backoff_secs = std::cmp::min(
            self.reconnect_delay.as_secs() * (1 << std::cmp::min(attempt, 6)), 
            300 // Cap at 5 minutes
        );
        
        info!("Waiting {} seconds before reconnecting...", backoff_secs);
        time::sleep(Duration::from_secs(backoff_secs)).await;
        
        // Try to reconnect
        match self.connect().await {
            Ok(_) => {
                info!("Successfully reconnected to MQTT broker");
                self.is_reconnecting.store(false, Ordering::SeqCst);
                Ok(())
            },
            Err(e) => {
                error!("Failed to reconnect: {}", e);
                self.is_reconnecting.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    async fn publish_batch(&self, batch: &[TopicMessage]) -> Result<()> {
        // Check connection health before publishing
        if !*self.health_receiver.borrow() {
            return Err(crate::error::Error::Connection("MQTT connection is not healthy".into()));
        }
        
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
