pub mod controller;

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    // simple_logger::init_with_env().unwrap();
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let p = controller::ProxyBuilder::from_env()?.build().await?;
    p.serve().await
}
