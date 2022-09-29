use rust_email_newsletter_api::{configuration::get_configuration, startup::run};
use std::{fmt::format, net::TcpListener};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let configuration = get_configuration().expect("Failed to read configuration.");

    let address = format!("127.0.0.1:{}", configuration.application_port);
    let listener = TcpListener::bind(address).expect("Failed to bind port 8000");

    run(listener)?.await
}
