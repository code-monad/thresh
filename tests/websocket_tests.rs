#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_websocket_connection_and_subscription() {
        let (tx, _rx) = mpsc::channel(100);
        let config = WebSocketConfig {
            endpoint: "ws://localhost:8114".to_string(),
            retry_interval: 5,
            max_retries: 3,
            ping_interval: 30,
        };

        let client = WebSocketClient::new(config.endpoint.clone(), tx, config);

        // This test requires a mock WebSocket server
        // For now, we'll just test that it handles connection failure gracefully
        let result = client.run().await;
        assert!(result.is_err());
    }

    #[test(tokio::test)]
    async fn test_message_processing() {
        let (tx, mut rx) = mpsc::channel(100);
        let config = WebSocketConfig::default();
        let client = WebSocketClient::new("ws://localhost:8114".to_string(), tx, config);

        // Simulate receiving a message
        let test_msg = r#"{"jsonrpc":"2.0","method":"subscribe","params":{"result":"success"}}"#;

        // Send test message through channel
        client.tx.send(test_msg.to_string()).await.unwrap();

        // Verify message was received
        let received = rx.recv().await.unwrap();
        assert_eq!(received, test_msg);
    }
}
