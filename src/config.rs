use serde::Deserialize;

use figment::{
    providers::{Format, Toml},
    Figment,
};

use crate::rpc::network_type::NetworkType;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub query_interval_ms: u64,
}

#[test]
fn test_config() {
    let config: Config = Figment::new()
        .merge(Toml::file("config.test.toml").nested())
        .extract()
        .expect("extract");

    println!("{:?}", config);
}
