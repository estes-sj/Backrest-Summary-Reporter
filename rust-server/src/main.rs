use axum::{routing::post, Router, http::StatusCode, extract::ConnectInfo, body::Bytes};
use std::{fs::OpenOptions, io::Write, net::SocketAddr};
use chrono::Local;

async fn receive_summary(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> (StatusCode, &'static str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let summary = String::from_utf8_lossy(&body);
    println!("=== Summary Received from {} at {} ===\n{}", addr, timestamp, summary);

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("Backrest_Listener.log")
    {
        let _ = writeln!(
            file,
            "=== Summary Received from {} at {} ===\n{}\n",
            addr, timestamp, summary
        );
    }

    (StatusCode::OK, "ok")
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/summary", post(receive_summary))
        .into_make_service_with_connect_info::<SocketAddr>();

    let addr = SocketAddr::from(([0, 0, 0, 0], 2682));
    println!("Listening on {}", addr);

    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}
