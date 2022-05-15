use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
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

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.spawn(webserver());

    window().unwrap();
}

fn window() -> wry::Result<()> {
    use wry::{
        application::{
            event::{Event, StartCause},
            event_loop::{ControlFlow, EventLoop},
            window::WindowBuilder,
        },
        webview::WebViewBuilder,
    };

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_always_on_top(true)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .build(&event_loop)?;
    let _webview = WebViewBuilder::new(window)?
        .with_url("http://localhost:42069/index.html")?
        .build()?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => println!("The Ramblathon begins!"),
            _ => {}
        }
    });
}

async fn webserver() {
    let connected = Arc::new(AtomicBool::new(false));

    let buffer = Arc::new(Mutex::new(String::new()));
    let buffer2 = buffer.clone();

    let ws_route = warp::path("ws").and(warp::ws()).map(
        move |ws: warp::ws::Ws| -> Box<dyn warp::reply::Reply> {
            let connected = connected.clone();
            if connected.load(Ordering::SeqCst) {
                return Box::new(warp::reply::with_status(
                    "Already connected with client",
                    warp::http::StatusCode::TOO_MANY_REQUESTS,
                ));
            } else {
                connected.store(true, Ordering::SeqCst);
            }

            let buffer = buffer.clone();

            Box::new(ws.on_upgrade(move |websocket| async move {
                let (mut tx, mut rx) = websocket.split();

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

                while let Some(msg) = rx.next().await {
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
            }))
        },
    );

    let file_routes = warp::path("index.html")
        .map(|| {
            warp::reply::with_header(include_str!("web/index.html"), "content-type", "text/html")
        })
        .or(warp::path("style.css").map(|| {
            warp::reply::with_header(include_str!("web/style.css"), "content-type", "text/css")
        }))
        .or(warp::path("script.js").map(|| {
            warp::reply::with_header(
                include_str!("web/script.js"),
                "content-type",
                "text/javascript",
            )
        }));

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
        warp::serve(ws_route.or(file_routes)).run(([127, 0, 0, 1], 42069)),
        append_to_buffer_task,
        copy_task
    )
    .1
    .unwrap();
}
