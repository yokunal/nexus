use std::io;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Malformed header: missing Content-Length")]
    MissingContentLength,
    #[error("Invalid Content-Length value")]
    InvalidContentLength,
    #[error("Connection closed unexpectedly")]
    ConnectionClosed,
}

pub struct LspTransport;

impl LspTransport {
    /// Reads one full JSON payload from an async reader respecting LSP Content-Length framing.
    pub async fn read_message<R>(reader: &mut R) -> Result<String, TransportError>
    where
        R: AsyncBufReadExt + Unpin,
    {
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                return Err(TransportError::ConnectionClosed);
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                // Empty line marks end of headers
                break;
            }

            if let Some(val) = trimmed.strip_prefix("Content-Length:") {
                let len_str = val.trim();
                let len = len_str
                    .parse::<usize>()
                    .map_err(|_| TransportError::InvalidContentLength)?;
                content_length = Some(len);
            }
        }

        let length = content_length.ok_or(TransportError::MissingContentLength)?;
        let mut buffer = vec![0u8; length];
        reader.read_exact(&mut buffer).await?;

        let text =
            String::from_utf8(buffer).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(text)
    }

    /// Writes one JSON payload to an async writer with Content-Length header.
    pub async fn write_message<W>(writer: &mut W, payload: &str) -> Result<(), TransportError>
    where
        W: AsyncWrite + Unpin,
    {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        writer.write_all(header.as_bytes()).await?;
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_transport_roundtrip() {
        let payload = r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#;
        let mut buffer = Vec::new();

        LspTransport::write_message(&mut buffer, payload)
            .await
            .unwrap();

        let mut reader = BufReader::new(&buffer[..]);
        let read_back = LspTransport::read_message(&mut reader).await.unwrap();

        assert_eq!(read_back, payload);
    }
}
