use rust_email_newsletter_api::{
    configuration::loader::get_configuration, startup::Application, telemetry,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let subscriber = telemetry::get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    telemetry::initialize_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");

    let application = Application::build(configuration)?;

    application.run_until_stopped().await?;

    Ok(())
}
