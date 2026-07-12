//! Shared IPC client used by CLI commands and the TUI.

use anyhow::{Context, Result};
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use crate::ipc::{ClientMsg, DaemonMsg};

/// Bound for request/response CLI calls (`status`, `kill`, config, …).
/// Attached cargo streaming uses [`Self::recv`] without a timeout — builds
/// can run for a long time between output lines.
const REQUEST_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(windows)]
const CONNECT_RETRY_TIMEOUT: Duration = Duration::from_millis(1500);

#[cfg(windows)]
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(50);

pub struct ShepherdClient {
    reader: BufReader<Box<dyn tokio::io::AsyncRead + Unpin + Send>>,
    writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
}

impl ShepherdClient {
    /// Connect to the running daemon.
    pub async fn connect() -> Result<Self> {
        #[cfg(unix)]
        {
            use tokio::net::UnixStream;
            let path = crate::ipc::socket_path();
            let stream = UnixStream::connect(&path).await.with_context(|| {
                format!(
                    "Cannot connect to shepherd daemon at {}\n\
                     Is it running? Start with: shepherd daemon",
                    path.display()
                )
            })?;
            let (reader, writer) = stream.into_split();
            Ok(Self {
                reader: BufReader::new(Box::new(reader)),
                writer: Box::new(writer),
            })
        }
        #[cfg(windows)]
        {
            use std::io;
            use tokio::net::windows::named_pipe::ClientOptions;

            let pipe_name = crate::ipc::pipe_name();
            let started = Instant::now();

            // Retry briefly: the server may still be creating the next pipe
            // instance. A missing daemon must not hang CLI commands forever.
            let client = loop {
                match ClientOptions::new().open(&pipe_name) {
                    Ok(c) => break c,
                    Err(e)
                        if e.kind() == io::ErrorKind::NotFound
                            || e.raw_os_error() == Some(231) /* ERROR_PIPE_BUSY */ =>
                    {
                        if started.elapsed() >= CONNECT_RETRY_TIMEOUT {
                            return Err(anyhow::anyhow!(
                                "Cannot connect to shepherd daemon at {}\n\
                                 Is it running? Start with: shepherd daemon\n\
                                 Error: {}",
                                pipe_name,
                                e
                            ));
                        }
                        tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Cannot connect to shepherd daemon at {}\n\
                             Is it running? Start with: shepherd daemon\n\
                             Error: {}",
                            pipe_name,
                            e
                        ));
                    }
                }
            };

            let (reader, writer) = tokio::io::split(client);
            Ok(Self {
                reader: BufReader::new(Box::new(reader)),
                writer: Box::new(writer),
            })
        }
    }

    /// Send one message and wait for one response (bounded).
    pub async fn send_recv(&mut self, msg: &ClientMsg) -> Result<DaemonMsg> {
        self.send(msg).await?;
        timeout(REQUEST_RESPONSE_TIMEOUT, self.recv())
            .await
            .context("Timed out waiting for daemon response")?
    }

    pub async fn send(&mut self, msg: &ClientMsg) -> Result<()> {
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Read the next daemon message. No timeout — used by attached cargo
    /// streaming where silence between lines is normal.
    pub async fn recv(&mut self) -> Result<DaemonMsg> {
        let mut resp_line = String::new();
        let n = self
            .reader
            .read_line(&mut resp_line)
            .await
            .context("Failed reading daemon response")?;

        if n == 0 || resp_line.is_empty() {
            return Err(anyhow::anyhow!("daemon closed the connection"));
        }

        serde_json::from_str(resp_line.trim()).context("Failed to parse daemon response as JSON")
    }
}
