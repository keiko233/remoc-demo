use remoc::{codec, prelude::*};
use std::{io, sync::Arc, time::Duration};
use tokio::{io::split, net::windows::named_pipe::ServerOptions, sync::RwLock, time::sleep};
use tracing::{info, warn};
use tracing_subscriber;

use ipc::{Counter, CounterServerSharedMut, IncreaseError, PIPE_NAME};

/// Server object for the counting service, keeping the state.
#[derive(Default)]
pub struct CounterObj {
    /// The current value.
    value: u32,
    /// The subscribed watchers.
    watchers: Vec<rch::watch::Sender<u32>>,
}

/// Implementation of remote counting service.
#[rtc::async_trait]
impl Counter for CounterObj {
    async fn value(&self) -> Result<u32, rtc::CallError> {
        info!("Returning current value: {}", self.value);
        Ok(self.value)
    }

    async fn watch(&mut self) -> Result<rch::watch::Receiver<u32>, rtc::CallError> {
        info!("Subscribing to counter change notifications...");
        let (tx, rx) = rch::watch::channel(self.value);
        self.watchers.push(tx);
        Ok(rx)
    }

    async fn increase(&mut self, by: u32) -> Result<(), IncreaseError> {
        info!("Increasing counter by {}", by);

        match self.value.checked_add(by) {
            Some(new_value) => self.value = new_value,
            None => {
                warn!("Overflow would occur when increasing by {}", by);

                return Err(IncreaseError::Overflow {
                    current_value: self.value,
                });
            }
        }
        let value = self.value;
        self.watchers
            .retain(|watch| !watch.send(value).into_disconnected().unwrap());
        Ok(())
    }

    async fn count_to_value(
        &self,
        step: u32,
        delay: Duration,
    ) -> Result<rch::mpsc::Receiver<u32>, rtc::CallError> {
        info!(
            "Server received RPC call: count_to_value(step = {}, delay = {:?})",
            step, delay
        );

        let (tx, rx) = rch::mpsc::channel(1);
        let value = self.value;

        tokio::spawn(async move {
            for i in (0..value).step_by(step as usize) {
                if tx.send(i).await.into_disconnected().unwrap() {
                    info!("Client disconnected, stopping count_to_value");
                    break;
                }
                sleep(delay).await;
            }
        });

        Ok(rx)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize logging (optional, but handy).
    tracing_subscriber::FmtSubscriber::builder().init();

    // Create a counter object that will be shared between all clients.
    let counter_obj = Arc::new(RwLock::new(CounterObj::default()));

    // Create the first pipe instance to ensure the pipe name is reserved.
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(PIPE_NAME)?;

    info!(
        "Named pipe server listening on '{}'. Press Ctrl+C to exit.",
        PIPE_NAME
    );

    loop {
        // Wait for a client to connect to this instance of the named pipe.
        server.connect().await?;
        let connected_client = server;

        // Create a new server pipe instance for the *next* client.
        // This ensures there's always a pipe open and listening
        // (otherwise a client could fail with NotFound).
        server = ServerOptions::new().create(PIPE_NAME)?;

        // Clone the shared counter object for the new task.
        let counter_obj = counter_obj.clone();

        // Spawn a task to handle this connected client.
        tokio::spawn(async move {
            let (server_side, client_side) =
                CounterServerSharedMut::<_, codec::Default>::new(counter_obj, 1);

            // Instead of splitting a TcpStream, we pass the same
            // NamedPipeServer object as both reader and writer. NamedPipeServer
            // implements both AsyncRead and AsyncWrite.
            let (read_half, write_half) = split(connected_client);

            let remoc_connect = remoc::Connect::io(remoc::Cfg::default(), read_half, write_half);

            // Provide the client stub to our Remoc connection,
            // so the remote side can call these methods.
            if let Err(e) = remoc_connect.provide(client_side).await {
                eprintln!("Remoc provide error: {:?}", e);
                return;
            }

            // Serve incoming requests from the client on this task.
            // `true` indicates that requests are handled in parallel.
            server_side.serve(true).await;
        });
    }
}
