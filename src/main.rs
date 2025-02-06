use std::borrow::BorrowMut;

use axum::{routing::get, routing::post, Form, Router};
use chrono::{DateTime, Duration, Utc};
use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};

use futures::stream::StreamExt;

const DAILY_MESSAGE_LIMIT: i32 = 10;

#[derive(Deserialize)]
struct TwilioWebhook {
    From: String,
    Body: String,
}

#[derive(Deserialize, Debug, Serialize)]
struct GPTMessage {
    role: String,
    content: String,
}

#[derive(Debug)]
struct User {
    phone_number: String,
    total_received: i32,
    total_sent: i32,
    received_today: i32,
    messages: Vec<GPTMessage>,
    last_reset: DateTime<Utc>,
}

async fn chat(messages: Vec<ChatCompletionMessage>) -> String {
    let credentials = openai::Credentials::from_env();
    let chat_completion = ChatCompletion::builder("gpt-4o", messages)
        .credentials(credentials)
        .create()
        .await;
    if let Ok(chat_completion) = chat_completion {
        chat_completion
            .choices
            .first()
            .unwrap()
            .message
            .clone()
            .content
            .unwrap()
            .trim()
            .to_string()
    } else {
        "Failed to get response.".to_string()
    }
}

fn handle_short_circuits(mut user: User, msg: &str) -> Option<String> {
    // first reset the daily quota if needed
    if Utc::now() >= user.last_reset + Duration::days(1) {
        user.received_today = 0;
        user.last_reset = Utc::now();
    }

    // check if user is over daily limit
    if user.received_today >= DAILY_MESSAGE_LIMIT {
        return Some(format!(
            "You have reached the daily message limit of {}. Your quota will reset at {}",
            DAILY_MESSAGE_LIMIT,
            user.last_reset + Duration::days(1)
        ));
    }

    user.received_today += 1;
    user.total_received += 1;

    match msg {
        "!help" => Some("Commands: !help, !stats".to_string()),
        "!stats" => Some(format!(
            "Total messages received: {}, Total messages sent: {}, Messages received today: {}",
            user.total_received, user.total_sent, user.received_today
        )),
        _ => None,
    }
}

async fn find_user(db_pool: &SqlitePool, phone_number: &str) -> Option<User> {
    let mut user = User {
        phone_number: phone_number.to_string(),
        total_received: 0,
        total_sent: 0,
        received_today: 0,
        messages: vec![],
        last_reset: Utc::now(),
    };

    let mut rows = sqlx::query("SELECT * FROM messages WHERE phone_number = ?")
        .bind(phone_number)
        .fetch(db_pool);

    if let Some(row) = rows.next().await {
        let row = row.unwrap();
        user.total_received = row.get("total_received");
        user.total_sent = row.get("total_sent");
        user.received_today = row.get("received_today");
        user.messages = serde_json::from_str(&row.get::<String, _>("messages")).unwrap();
        user.last_reset = DateTime::from_timestamp(row.get("last_reset"), 0).unwrap();
    }

    Some(user)
}

async fn reply_sms(phone_number: &str, message: &str) {
    println!("Replying to {}: {}", phone_number, message);
}

async fn handle_sms(Form(webhook): Form<TwilioWebhook>, db_pool: SqlitePool) {
    println!("Received SMS from: {}", webhook.From);
    println!("Message: {}", webhook.Body);

    let mut user = find_user(&db_pool, &webhook.From).await.unwrap();

    if let Some(short_circuit) = handle_short_circuits(user, &webhook.Body) {
        println!("Short circuiting: {}", short_circuit);
        user.total_sent += 1;
        reply_sms(&webhook.From, &short_circuit).await;
        return;
    }

    let credentials = openai::Credentials::from_env();
    let messages = vec![
        ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some("You are a helpful assistant.".to_string()),
            name: None,
            function_call: None,
            tool_call_id: None,
            tool_calls: vec![],
        },
        ChatCompletionMessage {
            role: ChatCompletionMessageRole::User,
            content: Some(webhook.Body),
            name: None,
            function_call: None,
            tool_call_id: None,
            tool_calls: vec![],
        },
    ];

    let chat_completion = ChatCompletion::builder("gpt-4o", messages.clone())
        .credentials(credentials.clone())
        .create()
        .await
        .unwrap();

    let returned_message = chat_completion.choices.first().unwrap().message.clone();
    println!(
        "{:#?}: {}",
        returned_message.role,
        returned_message.content.unwrap().trim()
    );

    let new_message = vec![ChatCompletionMessage {
        role: ChatCompletionMessageRole::User,
        content: Some("What is string programmers print as an example?".to_string()),
        name: None,
        function_call: None,
        tool_call_id: None,
        tool_calls: vec![],
    }];
}

async fn init_db(db_pool: &SqlitePool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            phone_number TEXT PRIMARY KEY,
            total_received INTEGER NOT NULL DEFAULT 0,
            total_sent INTEGER NOT NULL DEFAULT 0,
            received_today INTEGER NOT NULL DEFAULT 0,
            messages TEXT NOT NULL DEFAULT "[]",
            last_reset INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(db_pool)
    .await
    .expect("Failed to create table");
}

#[tokio::main]
async fn main() {
    let db_pool = SqlitePool::connect("sqlite://messages.db")
        .await
        .expect("Failed to connect to database");

    init_db(&db_pool).await;

    let app = Router::new()
        .route("/sms", post(handle_sms))
        .route("/", get(|| async { "Hello, world!" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
