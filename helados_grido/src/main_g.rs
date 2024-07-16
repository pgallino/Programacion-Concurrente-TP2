use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use helados_grido::gateway::server::PaymentGateway;

#[tokio::main]
async fn main() {
    let gateway_address: SocketAddr =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6000);
    env_logger::init();
    let gateway = match PaymentGateway::new(gateway_address) {
        Ok(gateway) => gateway,
        Err(error) => {
            eprintln!("ERROR: {}", error);
            return;
        }
    };

    log::info!("Starting Gateway...");

    if let Ok(()) = gateway.start() {
        println!("Gateway started successfully");
    } else {
        eprintln!("Failed to start gateway");
    }
}
