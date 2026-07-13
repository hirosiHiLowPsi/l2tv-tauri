use std::env;
use std::fs;

use serde_json::Value;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime construction failed");
    runtime.block_on(run());
}

async fn run() {
    let mut arguments = env::args().skip(1);
    let path = arguments
        .next()
        .unwrap_or_else(|| "/api/profile-from-db".to_string());
    let body_path = arguments.next().unwrap_or_default();
    if body_path.is_empty() {
        eprintln!("usage: api_probe <api-path> <body.json>");
        std::process::exit(2);
    }
    let body_text = fs::read_to_string(&body_path).unwrap_or_else(|error| {
        eprintln!("failed to read {body_path}: {error}");
        std::process::exit(2);
    });
    let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|error| {
        eprintln!("invalid JSON: {error}");
        std::process::exit(2);
    });
    match l2tv_tauri_lib::probe_api(&path, body).await {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
