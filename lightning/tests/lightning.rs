use bitcoin_hashes::{sha256, Hash};
use lightning_client::{Lightning, Result};
use rand::RngCore;

pub fn get_env(key: &str) -> String {
    std::env::var(key).expect(&format!("missing env: {key}"))
}

fn rand_preimage() -> Vec<u8> {
    let mut store_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    store_key_bytes.to_vec()
}

pub async fn get_info<L: Lightning>(client: &mut L) -> Result<()> {
    let info = client.get_info().await?;
    assert_eq!(info.id.len(), 33);
    // println!("inf {:?}", info);
    Ok(())
}

pub async fn create_invoice<L: Lightning>(client: &mut L) -> Result<()> {
    let image = rand_preimage();
    let expiry = 60 * 10; // 10 minutes
    let amount = 100_000;
    let desc = "test".to_owned();
    let hash = sha256::Hash::hash(&image);
    let invoice = client
        .create_invoice(desc.clone(), amount, Some(image.clone()), Some(expiry))
        .await?;
    assert_eq!(invoice.description, desc);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.expiry, expiry);
    assert_eq!(invoice.payment_hash, hash.to_byte_array());

    Ok(())
}
