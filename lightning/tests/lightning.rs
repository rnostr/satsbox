use lightning_client::{Lightning, Result};

pub fn get_env(key: &str) -> String {
    std::env::var_os(key)
        .expect(&format!("missing env: {key}"))
        .to_str()
        .unwrap()
        .to_owned()
}

pub async fn get_info<L: Lightning>(client: &mut L) -> Result<()> {
    let info = client.get_info().await?;
    assert_eq!(info.id.len(), 33);
    Ok(())
}
