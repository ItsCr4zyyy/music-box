//! Music-box bez displeje (jen server + přehrávání) – např. pro Linux server bez obrazovky.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    musicbox_core::init_logging();
    let _state = musicbox_core::start(musicbox_core::CoreConfig::from_env()).await?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
