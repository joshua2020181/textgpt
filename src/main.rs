use axum::{routing::post, routing::get, Form, Router};
use serde::Deserialize;
use std::sync::Arc;
use chrono::Utc;
use sqlx::{sqlite::SqlitePool, Row};

#[derive(Deserialize)]
struct TwilioWebhook {
    From: String,
    Body: String,
}

async fn handle_sms(Form(webhook): Form<TwilioWebhook>) {
    println!("Received SMS from: {}", webhook.From);
    println!("Message: {}", webhook.Body);

    let current_time = Utc::now();
    let today_start = current_time.date().and_hms(0, 0, 0);
}

async fn init_db(db_pool: &SqlitePool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            phone_number TEXT PRIMARY KEY,
            total_received INTEGER NOT NULL DEFAULT 0,
            total_sent INTEGER NOT NULL DEFAULT 0,
            received_today INTEGER NOT NULL DEFAULT 0,
            last_reset TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(db_pool)
    .await
    .expect("Failed to create table");
}

#[tokio::main]
async fn main() {
    let db_pool = SqlitePool::connect("sqlite://messages.db").await.expect("Failed to connect to database");

    init_db(&db_pool).await;

    let app = Router::new().route(
        "/sms",
        post(handle_sms),
    ).route(
        "/",
        get(|| async {
            "Hello, world!"
        }),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

