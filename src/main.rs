use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use warp::ws::Message;
use warp::Filter;

const WRITE_INTERVAL: u64 = 30;
const BACKUP_INTERVAL: u64 = 3600;

#[tokio::main]
async fn main() {
    let buffer = Arc::new(Mutex::new(String::new()));
    let buffer2 = buffer.clone();

    let routes = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let buffer = buffer.clone();

            ws.on_upgrade(move |websocket| async move {
                let (mut tx, rx) = websocket.split();

                let mut initial_buffer = String::new();
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("./out/buffer.txt")
                    .await
                    .unwrap();
                file.read_to_string(&mut initial_buffer).await.unwrap();

                tx.send(Message::text(initial_buffer)).await.unwrap();

                rx.for_each(|msg| {
                    let buffer = buffer.clone();
                    async move {
                        match msg {
                            Err(e) => eprintln!("websocket error: {:?}", e),
                            Ok(msg) => {
                                if let Ok(msg) = msg.to_str() {
                                    buffer.lock().await.push_str(msg);
                                    eprintln!("Processed delta {msg:?} into delta buffer");
                                }
                            }
                        }
                    }
                })
                .await;
            })
        });

    // Append to the current buffer file
    let append_to_buffer_task = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(WRITE_INTERVAL));

        let buffer = buffer2;

        loop {
            interval.tick().await;

            let mut file = OpenOptions::new()
                .append(true)
                .open("./out/buffer.txt")
                .await
                .unwrap();

            let mut buffer = buffer.lock().await;

            if !buffer.is_empty() {
                file.write_all(buffer.as_bytes()).await.unwrap();
                buffer.clear();
                eprintln!("Written delta buffer");
            } else {
                eprintln!("Delta buffer is empty");
            }
        }
    });

    // Make a backup of the buffer file
    let copy_task = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(BACKUP_INTERVAL));

        loop {
            interval.tick().await;

            let start = SystemTime::now();
            let since_the_epoch = start
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards");

            tokio::fs::copy(
                "./out/buffer.txt",
                format!("./out/backup/buffer_{}.txt", since_the_epoch.as_millis()),
            )
            .await
            .unwrap();
        }
    });

    tokio::join!(
        warp::serve(routes).run(([127, 0, 0, 1], 42069)),
        append_to_buffer_task,
        copy_task
    )
    .1
    .unwrap();
}
