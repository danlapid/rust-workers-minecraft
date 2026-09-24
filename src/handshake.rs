//! The Minecraft handshake prefix of an inbound connection, read before the
//! connection is forwarded. Server-list requests are answered here from the
//! object's metadata so a ping never starts a server; login connections are
//! forwarded with the consumed bytes replayed.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_PACKET: usize = 1024;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub enum Handshake {
    /// A server-list exchange was answered; the connection is done.
    Status,
    /// A login handshake; `consumed` holds the bytes read so far.
    Login { consumed: Vec<u8> },
}

fn varint(mut value: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 128);
    }
}

fn read_varint(bytes: &[u8]) -> Result<Option<(u32, usize)>, String> {
    let mut value = 0u32;
    for i in 0..5 {
        let Some(&byte) = bytes.get(i) else {
            return Ok(None);
        };
        if i == 4 && byte & 0xf0 != 0 {
            return Err("Invalid VarInt".into());
        }
        value |= u32::from(byte & 127) << (7 * i);
        if byte & 128 == 0 {
            return Ok(Some((value, i + 1)));
        }
    }
    Err("Invalid VarInt".into())
}

fn required_varint(bytes: &[u8]) -> Result<(u32, usize), String> {
    read_varint(bytes)?.ok_or_else(|| "Truncated handshake".into())
}

fn frame(id: u32, body: &[u8]) -> Vec<u8> {
    let mut content = varint(id);
    content.extend_from_slice(body);
    let mut out = varint(content.len() as u32);
    out.extend(content);
    out
}

/// `(protocol, next state)` from a handshake packet body.
pub fn handshake_state(packet: &[u8]) -> Result<(u32, u32), String> {
    let (id, a) = required_varint(packet)?;
    let (protocol, b) = required_varint(&packet[a..])?;
    let (host_length, c) = required_varint(&packet[a + b..])?;
    let host_end = a + b + c + host_length as usize;
    if id != 0 || host_length as usize > MAX_PACKET || host_end + 2 >= packet.len() {
        return Err("Invalid handshake".into());
    }
    let (state, end) = required_varint(&packet[host_end + 2..])?;
    if host_end + 2 + end != packet.len() {
        return Err("Invalid handshake length".into());
    }
    if !(1..=3).contains(&state) {
        return Err("Invalid handshake state".into());
    }
    Ok((protocol, state))
}

/// Reads whole handshake/status packets from a byte stream, keeping every
/// byte consumed so a login connection can be replayed.
struct Packets<R> {
    reader: R,
    buffer: Vec<u8>,
    consumed: Vec<u8>,
    deadline: tokio::time::Instant,
}

impl<R: AsyncRead + AsyncWrite + Unpin> Packets<R> {
    fn new(reader: R) -> Self {
        Packets {
            reader,
            buffer: Vec::new(),
            consumed: Vec::new(),
            deadline: tokio::time::Instant::now() + TIMEOUT,
        }
    }

    /// The next packet body, or `None` at a clean end of stream.
    async fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            if let Some((size, start)) = read_varint(&self.buffer)? {
                let size = size as usize;
                if size < 1 || size > MAX_PACKET {
                    return Err("Invalid handshake/status packet size".into());
                }
                if self.buffer.len() >= start + size {
                    let rest = self.buffer.split_off(start + size);
                    let whole = std::mem::replace(&mut self.buffer, rest);
                    self.consumed.extend_from_slice(&whole);
                    return Ok(Some(whole[start..].to_vec()));
                }
            }
            let mut chunk = [0u8; 512];
            let read = tokio::time::timeout_at(self.deadline, self.reader.read(&mut chunk))
                .await
                .map_err(|_| "Handshake/status timed out".to_string())?
                .map_err(|e| e.to_string())?;
            if read == 0 {
                if !self.buffer.is_empty() {
                    return Err("Truncated handshake/status packet".into());
                }
                return Ok(None);
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    /// Everything read from the stream so far, including unparsed bytes.
    fn into_consumed(mut self) -> Vec<u8> {
        self.consumed.append(&mut self.buffer);
        self.consumed
    }
}

/// Classifies a connection by its handshake. `server_list(protocol)` supplies
/// the status JSON.
pub async fn accept<S, F, Fut>(socket: &mut S, server_list: F) -> Result<Handshake, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnOnce(u32) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut packets = Packets::new(&mut *socket);
    let Some(handshake) = packets.read().await? else {
        return Ok(Handshake::Status);
    };
    let (protocol, state) = handshake_state(&handshake)?;
    if state != 1 {
        return Ok(Handshake::Login {
            consumed: packets.into_consumed(),
        });
    }
    let request = packets.read().await?;
    if request.as_deref() != Some(&[0][..]) {
        return Err("Invalid status request".into());
    }
    let json = server_list(protocol).await?;
    let mut body = varint(json.len() as u32);
    body.extend_from_slice(json.as_bytes());
    let response = frame(0, &body);
    packets
        .reader
        .write_all(&response)
        .await
        .map_err(|e| e.to_string())?;
    packets.reader.flush().await.map_err(|e| e.to_string())?;
    match packets.read().await? {
        None => {}
        Some(ping) if ping.len() == 9 && ping[0] == 1 => {
            packets
                .reader
                .write_all(&frame(1, &ping[1..]))
                .await
                .map_err(|e| e.to_string())?;
            packets.reader.flush().await.map_err(|e| e.to_string())?;
        }
        Some(_) => return Err("Invalid status ping".into()),
    }
    Ok(Handshake::Status)
}
