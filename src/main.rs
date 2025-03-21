pub mod controller;
pub mod config;
pub mod exchange;

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let p = config::ProxyBuilder::from_env()?.build().await?;
    p.serve_all().await;
    Ok(())
}
