use remoc::prelude::*;
use std::time::Duration;
use tokio::{io::split, net::windows::named_pipe::ClientOptions};
use tracing::{error, info};

use ipc::{Counter, CounterClient, PIPE_NAME};

#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder().init();

    let pipe = match ClientOptions::new().open(PIPE_NAME) {
        Ok(pipe) => pipe,
        Err(e) => {
            error!("Failed to open named pipe: {:?}", e);
            return;
        }
    };

    let (read_half, write_half) = split(pipe);

    let mut client: CounterClient = remoc::Connect::io(remoc::Cfg::default(), read_half, write_half)
        .consume()
        .await
        .expect("Failed to consume CounterClient from server");

    info!("Subscribing to counter change notifications...");
    let mut watch_rx = client.watch().await.unwrap();
    let watch_task = tokio::spawn(async move {
        loop {
            if watch_rx.changed().await.is_err() {
                info!("Watch channel closed or server disconnected.");
                break;
            }
            let value = watch_rx.borrow_and_update().unwrap();
            info!("Counter change notification: {}", *value);
        }
    });
    info!("Done subscribing!");

    let value = client.value().await.unwrap();
    info!("Current counter value is {}", value);

    info!("Increasing counter value by 5");
    client.increase(5).await.unwrap();
    info!("Done!");

    let value = client.value().await.unwrap();
    info!("New counter value is {}", value);

    info!("Asking the server to count to the current value with a step delay of 300ms...");
    let mut rx = client
        .count_to_value(1, Duration::from_millis(300))
        .await
        .unwrap();
    while let Ok(Some(i)) = rx.recv().await {
        info!("Server counts {}", i);
    }
    info!("Server is done counting.");

    watch_task.await.unwrap();
}
