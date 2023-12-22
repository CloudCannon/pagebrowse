use std::{
    borrow::Borrow,
    collections::HashMap,
    process::Stdio,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose, Engine};
use pagebrowse_manager::{PBRequest, PBRequestPayload, PBResponse, PBResponsePayload};
use thiserror::Error;
use tokio::{
    io::AsyncBufReadExt,
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::broadcast,
    sync::mpsc::{error::SendError, unbounded_channel, UnboundedReceiver, UnboundedSender},
};

#[derive(Error, Debug)]
pub enum PagebrowseError {
    #[error("unknown error")]
    Unknown,
}

pub struct PagebrowseBuilder {
    pool_size: usize,
    visible: bool,
}

impl PagebrowseBuilder {
    pub fn new(pool_size: usize) -> Self {
        Self {
            pool_size,
            visible: false,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn build(self) -> Result<Pagebrowser, PagebrowseError> {
        let PagebrowseBuilder { pool_size, visible } = self;

        let (tx_response, rx_response) = broadcast::channel::<PBResponse>(100);

        let mut command = Command::new("../../target/debug/pagebrowse_manager");
        command.arg("--count").arg(pool_size.to_string());

        command.kill_on_drop(true);
        if visible {
            command.arg("--visible");
        }

        command.stdin(Stdio::piped()).stdout(Stdio::piped());

        let mut child = command.spawn().map_err(|_| PagebrowseError::Unknown)?;

        let stdout = child.stdout.take().unwrap();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            loop {
                let mut buf = vec![];
                reader.read_until(b',', &mut buf).await.unwrap();

                if buf.pop().is_none() {
                    // EOF Reached
                    // TODO: Handle the manager dying
                    break;
                }

                let Ok(decoded) = general_purpose::STANDARD.decode(&buf) else {
                    let msg = std::str::from_utf8(&buf).unwrap();
                    panic!("Received garbled base64 from the manager: {msg}");
                };

                match serde_json::from_slice::<PBResponse>(&decoded) {
                    Ok(msg) => {
                        tx_response.send(msg).expect("Broadcast channel is open");
                    }
                    Err(e) => {
                        panic!("Received garbled json from the manager");
                    }
                }
            }
        });

        Ok(Pagebrowser::new(child, rx_response))
    }
}

struct PagebrowserInner {
    child: Child,
    latest_message_id: u32,
    rx_response: broadcast::Receiver<PBResponse>,
}

#[derive(Clone)]
pub struct Pagebrowser {
    inner: Arc<Mutex<PagebrowserInner>>,
}

impl Pagebrowser {
    async fn send_command(
        &self,
        command: PBRequestPayload,
    ) -> Result<PBResponsePayload, PagebrowseError> {
        let mut inner = self.inner.lock().map_err(|_| PagebrowseError::Unknown)?;

        let this_message_id = inner.latest_message_id;
        let request = PBRequest {
            message_id: Some(this_message_id),
            payload: command,
        };
        inner.latest_message_id += 1;

        let encoded = general_purpose::STANDARD.encode(serde_json::to_vec(&request).unwrap());

        if let Some(stdin) = inner.child.stdin.as_mut() {
            stdin.write_all(encoded.as_bytes()).await.unwrap();
            stdin.write(b",").await.unwrap();
            stdin.flush().await.unwrap();
        }

        while let Ok(response) = inner.rx_response.recv().await {
            if response.message_id == Some(this_message_id) {
                return Ok(response.payload);
            }
        }

        Err(PagebrowseError::Unknown)
    }
}

impl Pagebrowser {
    pub fn new(child: Child, rx_response: broadcast::Receiver<PBResponse>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PagebrowserInner {
                child,
                latest_message_id: 0,
                rx_response,
            })),
        }
    }

    pub async fn get_window(&self) -> Result<PagebrowserWindow, PagebrowseError> {
        let window_response = self
            .send_command(PBRequestPayload::NewWindow { start_url: None })
            .await?;

        let PBResponsePayload::NewWindowCreated { id } = window_response else {
            return Err(PagebrowseError::Unknown);
        };

        Ok(PagebrowserWindow {
            id,
            browser: self.clone(),
        })
    }
}

pub struct PagebrowserWindow {
    id: u32,
    browser: Pagebrowser,
}

impl PagebrowserWindow {
    pub async fn navigate(&self, url: String) -> Result<(), PagebrowseError> {
        let response = self
            .browser
            .send_command(PBRequestPayload::Navigate {
                window_id: self.id,
                url,
            })
            .await?;

        match response {
            PBResponsePayload::OperationComplete => Ok(()),
            _ => Err(PagebrowseError::Unknown),
        }
    }

    pub async fn evaluate_script(&self, script: String) -> Result<(), PagebrowseError> {
        let response = self
            .browser
            .send_command(PBRequestPayload::EvaluateScript {
                window_id: self.id,
                script,
            })
            .await?;

        match response {
            PBResponsePayload::OperationComplete => Ok(()),
            _ => Err(PagebrowseError::Unknown),
        }
    }

    pub async fn resize_window(&self, width: usize, height: usize) -> Result<(), PagebrowseError> {
        let response = self
            .browser
            .send_command(PBRequestPayload::ResizeWindow {
                window_id: self.id,
                width,
                height,
            })
            .await?;

        match response {
            PBResponsePayload::OperationComplete => Ok(()),
            _ => Err(PagebrowseError::Unknown),
        }
    }
}
