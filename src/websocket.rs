use crate::config::{SubscriptionTopic, WebSocketConfig};
use crate::error::{Error, Result};
use backoff::{Error as BackoffError, ExponentialBackoff};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub struct TopicMessage {
    pub topic: String,
    pub payload: String,
}

pub struct WebSocketClient {
    endpoint: String,
    tx: mpsc::Sender<TopicMessage>,
    config: WebSocketConfig,
    subscription_map: Mutex<HashMap<String, String>>, // subscription_id -> topic_name
    health_sender: watch::Sender<bool>,
    health_receiver: watch::Receiver<bool>,
}

impl WebSocketClient {
    pub fn new(endpoint: String, tx: mpsc::Sender<TopicMessage>, config: WebSocketConfig) -> Self {
        let (health_sender, health_receiver) = watch::channel(false);
        
        Self {
            endpoint,
            tx,
            config,
            subscription_map: Mutex::new(HashMap::new()),
            health_sender,
            health_receiver,
        }
    }

    async fn subscribe_to_topic(
        &self,
        write: &mut futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        topic: &SubscriptionTopic,
    ) -> Result<()> {
        let subscribe_msg = json!({
            "id": topic.id.unwrap_or_else(|| rand::random::<u32>()),
            "jsonrpc": "2.0",
            "method": "subscribe",
            "params": [topic.name]
        });

        debug!(
            "Sending subscription request for {}: {}",
            topic.name, subscribe_msg
        );
        
        // Add timeout for the send operation
        let send_future = write.send(Message::Text(subscribe_msg.to_string()));
        match time::timeout(Duration::from_secs(10), send_future).await {
            Ok(result) => {
                result?;
                debug!("Subscription request for {} sent successfully", topic.name);
            },
            Err(_) => {
                error!("Timeout while sending subscription request for {}", topic.name);
                return Err(Error::Connection(format!("Subscription request timeout for {}", topic.name)));
            }
        }

        Ok(())
    }

    async fn subscribe_all(
        &self,
        write: &mut futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
    ) -> Result<()> {
        for topic in self.config.topics.iter().filter(|t| t.enabled) {
            info!("Subscribing to topic: {}", topic.name);
            
            // Try to subscribe with retries
            let mut retries = 0;
            let max_retries = 3;
            let mut delay = 100; // ms
            
            loop {
                match self.subscribe_to_topic(write, topic).await {
                    Ok(_) => break,
                    Err(e) => {
                        retries += 1;
                        if retries >= max_retries {
                            error!("Failed to subscribe to {} after {} attempts: {}", topic.name, max_retries, e);
                            return Err(Error::Connection(format!("Failed to subscribe to {}", topic.name)));
                        }
                        
                        error!("Failed to subscribe to {}, retrying in {}ms: {}", topic.name, delay, e);
                        time::sleep(Duration::from_millis(delay)).await;
                        delay *= 2; // Exponential backoff
                    }
                }
            }

            // Wait a short time between subscriptions
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    async fn handle_message(&self, msg: Message) -> Result<()> {
        match msg {
            Message::Text(text) => self.process_text_message(text).await,
            Message::Pong(_) => {
                debug!("Received pong from server");
                Ok(())
            }
            Message::Close(frame) => {
                error!("Received close frame: {:?}", frame);
                // Set health status to false
                let _ = self.health_sender.send(false);
                Err(Error::Connection("Server closed connection".into()))
            }
            _ => {
                debug!("Received other type of message");
                Ok(())
            }
        }
    }

    /// Processes incoming JSON-RPC messages, handling both subscription confirmations
    /// and actual subscription data
    async fn process_text_message(&self, text: String) -> Result<()> {
        debug!("Received message: {}", text);

        let value: Value = serde_json::from_str(&text).map_err(|e| {
            error!("Failed to parse message as JSON: {}", e);
            Error::Parse(e.to_string())
        })?;

        // Handle subscription confirmation first as it's a special case
        if self.try_handle_subscription_confirmation(&value).is_some() {
            return Ok(());
        }

        self.try_handle_subscription_message(&value).await
    }

    /// Handles the initial subscription confirmation message from the WebSocket server
    /// Maps subscription IDs to their corresponding topics for future message routing
    fn try_handle_subscription_confirmation(&self, value: &Value) -> Option<()> {
        let (result, id) = match (value.get("result"), value.get("id")) {
            (Some(r), Some(i)) => (r, i),
            _ => return None,
        };

        let subscription_id = result.as_str()?;
        let topic = self
            .config
            .topics
            .iter()
            .find(|t| t.id == Some(id.as_u64().unwrap_or(0) as u32))?;

        info!(
            "Subscription confirmed for topic {} with ID: {}",
            topic.name, subscription_id
        );

        if let Ok(mut map) = self.subscription_map.lock() {
            map.insert(subscription_id.to_string(), topic.name.clone());
        } else {
            error!("Failed to acquire lock for subscription map");
            return None;
        }

        Some(())
    }

    /// Processes subscription messages containing transaction data
    /// Extracts and validates the message before forwarding to MQTT
    async fn try_handle_subscription_message(&self, value: &Value) -> Result<()> {
        let params = match value.get("params") {
            Some(p) => p,
            None => return Ok(()),
        };

        debug!("Processing params: {:?}", params);

        let (subscription, result) = match (
            params.get("subscription").and_then(|s| s.as_str()),
            params.get("result"),
        ) {
            (Some(s), Some(r)) => (s, r),
            _ => return Ok(()),
        };

        debug!("Found subscription: {} and result", subscription);

        let topic = match self.get_topic_for_subscription(subscription) {
            Ok(topic) => topic,
            Err(e) => {
                error!("Error getting topic for subscription {}: {}", subscription, e);
                return Ok(());
            }
        };
        
        let transaction = match self.parse_transaction_from_result(result) {
            Ok(tx) => tx,
            Err(e) => {
                error!("Error parsing transaction from result: {}", e);
                return Ok(());
            }
        };

        // Use timeout for sending to prevent blocking indefinitely
        match time::timeout(
            Duration::from_secs(5),
            self.send_transaction_message(topic, transaction)
        ).await {
            Ok(result) => result,
            Err(_) => {
                error!("Timeout while sending transaction message to MQTT");
                Err(Error::Connection("Timeout sending to MQTT".into()))
            }
        }
    }

    /// Retrieves the corresponding topic configuration for a given subscription ID
    /// Returns error if the topic is not found or disabled
    fn get_topic_for_subscription(&self, subscription: &str) -> Result<&SubscriptionTopic> {
        let map = self
            .subscription_map
            .lock()
            .map_err(|_| Error::Lock("Failed to acquire subscription map lock".into()))?;

        let topic_name = map
            .get(subscription)
            .ok_or_else(|| Error::NotFound("No topic found for subscription".into()))?;

        self.config
            .topics
            .iter()
            .find(|t| &t.name == topic_name && t.enabled)
            .ok_or_else(|| Error::NotFound("No matching enabled topic found".into()))
    }

    /// Parses and extracts the transaction data from the subscription result
    /// Handles nested JSON parsing and validation
    fn parse_transaction_from_result(&self, result: &Value) -> Result<String> {
        let result_str = result
            .as_str()
            .ok_or_else(|| Error::Parse("Result is not a string".into()))?;

        let result_obj: Value = serde_json::from_str(result_str)
            .map_err(|e| Error::Parse(format!("Failed to parse result string as JSON: {}", e)))?;

        let transaction = result_obj
            .get("transaction")
            .ok_or_else(|| Error::Parse("No transaction field found in result object".into()))?;

        serde_json::to_string(transaction)
            .map_err(|e| Error::Parse(format!("Failed to serialize transaction: {}", e)))
    }

    /// Forwards the processed transaction message to the MQTT publisher
    /// through the channel
    async fn send_transaction_message(
        &self,
        topic: &SubscriptionTopic,
        payload: String,
    ) -> Result<()> {
        info!("Sending transaction to MQTT topic: {}", topic.mqtt_topic);
        debug!("Payload: {}", payload);

        let topic_message = TopicMessage {
            topic: topic.mqtt_topic.clone(),
            payload,
        };

        self.tx.send(topic_message).await.map_err(|e| {
            error!("Failed to send message to processor: {}", e);
            Error::Connection("Channel closed".into())
        })?;

        Ok(())
    }

    async fn connect_and_process(&self) -> Result<()> {
        info!("Connecting to WebSocket endpoint: {}", self.endpoint);
        
        // Add timeout for the connection
        let connect_future = connect_async(&self.endpoint);
        let ws_stream = match time::timeout(Duration::from_secs(30), connect_future).await {
            Ok(result) => {
                let (stream, _) = result?;
                stream
            },
            Err(_) => {
                error!("Timeout while connecting to WebSocket endpoint: {}", self.endpoint);
                return Err(Error::Connection("Connection timeout".into()));
            }
        };
        
        // Set health status to true once connected
        let _ = self.health_sender.send(true);
        
        let (mut write, mut read) = ws_stream.split();

        // Send subscription immediately after connection
        if let Err(e) = self.subscribe_all(&mut write).await {
            error!("Failed to subscribe to topics: {}", e);
            let _ = self.health_sender.send(false);
            return Err(e);
        }

        // Set up ping/pong keepalive
        let write = Arc::new(tokio::sync::Mutex::new(write));
        let write_clone = write.clone();

        // Spawn ping task
        let ping_handle = {
            let interval = self.config.ping_interval;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(interval));
                loop {
                    interval.tick().await;
                    let mut write = write_clone.lock().await;
                    if let Err(e) = write.send(Message::Ping(vec![])).await {
                        error!("Failed to send ping: {}", e);
                        break;
                    }
                }
            })
        };

        let mut consecutive_errors = 0;
        
        // Create a shared last activity time that can be updated from multiple places
        let last_activity_time = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
        let last_activity_time_clone = last_activity_time.clone();
        
        let max_idle_time = Duration::from_secs(60); // Consider connection dead if no messages for 60 seconds

        // Spawn a health check task
        let health_sender_clone = self.health_sender.clone();
        let health_receiver_clone = self.health_receiver.clone();
        let health_check_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                interval.tick().await;
                
                let elapsed = {
                    let time = last_activity_time_clone.lock().unwrap();
                    time.elapsed()
                };
                
                if elapsed > max_idle_time {
                    warn!("No WebSocket activity for {:?}, connection may be dead", elapsed);
                    let _ = health_sender_clone.send(false);
                } else if !*health_receiver_clone.borrow() {
                    // If we have activity but health is false, try to restore it
                    debug!("WebSocket activity detected but health status is false, restoring");
                    let _ = health_sender_clone.send(true);
                }
            }
        });

        while let Some(msg) = read.next().await {
            // Update last activity time
            {
                let mut time = last_activity_time.lock().unwrap();
                *time = std::time::Instant::now();
            }
            
            match msg {
                Ok(msg) => {
                    consecutive_errors = 0;
                    // Set health status to true as we're receiving messages
                    let _ = self.health_sender.send(true);
                    
                    if let Err(e) = self.handle_message(msg).await {
                        error!("Error handling message: {}", e);
                        
                        // If this is a connection error, break the loop to trigger reconnection
                        if matches!(e, Error::Connection(_)) {
                            let _ = self.health_sender.send(false);
                            ping_handle.abort();
                            health_check_handle.abort();
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    consecutive_errors += 1;
                    
                    // Set health status to false on error
                    let _ = self.health_sender.send(false);
                    
                    if consecutive_errors >= self.config.max_retries {
                        ping_handle.abort();
                        health_check_handle.abort();
                        return Err(Error::Connection("Too many consecutive errors".into()));
                    }
                    
                    // Add a small delay before continuing to prevent tight error loops
                    time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        // If we exit the loop, the connection is closed
        let _ = self.health_sender.send(false);
        ping_handle.abort();
        health_check_handle.abort();
        
        Err(Error::Connection("WebSocket connection closed".into()))
    }

    pub async fn run(&self) -> Result<()> {
        let backoff = ExponentialBackoff {
            initial_interval: Duration::from_secs(self.config.retry_interval),
            max_interval: Duration::from_secs(30),
            max_elapsed_time: None, // No maximum elapsed time - keep retrying indefinitely
            multiplier: 1.5,
            ..Default::default()
        };

        let operation = || async {
            match self.connect_and_process().await {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Connection error: {}", e);
                    // Set health status to false
                    let _ = self.health_sender.send(false);
                    Err(BackoffError::transient(e))
                }
            }
        };

        // This will keep retrying with backoff
        match backoff::future::retry(backoff, operation).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::Connection(e.to_string())),
        }
    }
}
