use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Text(String),
    File { name: String, data: Vec<u8> },
    Ping,
}

/// Write a length-prefixed bincode message.
pub async fn write_msg<W: AsyncWriteExt + Unpin>(w: &mut W, msg: &Message) -> anyhow::Result<()> {
    let encoded = bincode::serialize(msg)?;
    let len = encoded.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&encoded).await?;
    w.flush().await?;
    Ok(())
}

/// Read a length-prefixed bincode message.
pub async fn read_msg<R: AsyncReadExt + Unpin>(r: &mut R) -> anyhow::Result<Message> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX: usize = 512 * 1024 * 1024; // 512 MB hard cap
    if len > MAX {
        anyhow::bail!("message too large: {} bytes", len);
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(bincode::deserialize(&buf)?)
}
