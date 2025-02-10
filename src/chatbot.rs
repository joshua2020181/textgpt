use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};

use futures::stream::StreamExt;

const DAILY_MESSAGE_LIMIT: i32 = 10;

/// Struct representing a message for the GPT API
#[derive(Deserialize, Debug, Serialize)]
struct GPTMessage {
    role: String, // valid values: "User", "System"
    content: String,
}

/// Struct representing a user in the db
#[derive(Debug)]
struct User {
    phone_number: String,
    total_received: i32,
    total_sent: i32,
    received_today: i32,
    messages: Vec<GPTMessage>,
    last_reset: DateTime<Utc>,
}

pub struct ChatBot {
    db_pool: Arc<SqlitePool>,
}

impl ChatBot {
    pub async fn new(connection_string: String) -> Self {
        ChatBot {
            db_pool: Arc::new(Self::init_db(&connection_string).await),
        }
    }

    /// Handle an incoming message from a phone number
    pub async fn handle_message(&self, from: String, message: String) -> String {
        let mut user = self.find_user(&from).await.unwrap();

        if let Some(short_circuit) = Self::handle_short_circuits(&mut user, &message) {
            self.update_db(&user).await;
            user.total_sent += 1;
            return short_circuit;
        }

        user.messages.push(GPTMessage {
            role: "User".to_string(),
            content: message.clone(),
        });

        let messages = Self::make_chat_completion_message(&user.messages);
        println!("messages: {:?}", messages);

        let returned_message = Self::get_gpt_response(messages).await;

        user.messages.push(GPTMessage {
            role: "System".to_string(),
            content: returned_message.clone(),
        });

        user.total_sent += 1;
        self.update_db(&user).await;

        returned_message
    }

    /// Handle special commands without going through GPT
    ///
    /// Returns a response if the message is a special command, otherwise None
    fn handle_short_circuits(user: &mut User, msg: &str) -> Option<String> {
        // first reset the daily quota if needed
        if Utc::now() >= user.last_reset + Duration::days(1) {
            user.received_today = 0;
            user.last_reset = Utc::now();
        }

        user.received_today += 1;
        user.total_received += 1;

        // check if user is over daily limit
        if user.received_today >= DAILY_MESSAGE_LIMIT {
            return Some(format!(
                "You have reached the daily message limit of {}. Your quota will reset at {}",
                DAILY_MESSAGE_LIMIT,
                user.last_reset + Duration::days(1)
            ));
        }

        match msg {
            "!help" => Some("Commands: !help, !stats".to_string()),
            "!stats" => Some(format!(
                "Total messages received: {}, Total messages sent: {}, Messages received today: {}",
                user.total_received, user.total_sent, user.received_today
            )),
            _ => None,
        }
    }

    /// Initialize the database with the necessary table
    async fn init_db(connection_string: &str) -> SqlitePool {
        // create the database if it doesn't exist
        let db_pool = SqlitePool::connect(connection_string)
            .await
            .expect("Failed to connect to database");
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
        .execute(&db_pool)
        .await
        .expect("Failed to create table");

        db_pool
    }

    /// Convert a slice of GPT messages to a vec of ChatCompletionMessages
    fn make_chat_completion_message(messages: &[GPTMessage]) -> Vec<ChatCompletionMessage> {
        messages
            .iter()
            .map(|msg| ChatCompletionMessage {
                role: match msg.role.as_str() {
                    "User" => ChatCompletionMessageRole::User,
                    "Assistant" | "System" => ChatCompletionMessageRole::System,
                    _ => ChatCompletionMessageRole::System,
                },
                content: Some(msg.content.clone()),
                name: None,
                function_call: None,
                tool_call_id: None,
                tool_calls: vec![],
            })
            .collect()
    }

    /// Get a response from the GPT API
    async fn get_gpt_response(messages: Vec<ChatCompletionMessage>) -> String {
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

    /// Find a user in the database
    async fn find_user(&self, phone_number: &str) -> Option<User> {
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
            .fetch(&*self.db_pool);

        if let Some(row) = rows.next().await {
            let row = row.unwrap();
            user.total_received = row.get("total_received");
            user.total_sent = row.get("total_sent");
            user.received_today = row.get("received_today");
            user.messages = serde_json::from_str(&row.get::<String, _>("messages")).unwrap();
            user.last_reset = DateTime::from_timestamp(row.get("last_reset"), 0).unwrap();
        }

        if user.messages.is_empty() {
            user.messages.push(GPTMessage {
                role: "System".to_string(),
                content: "You are a helpful assistant. Please keep your responses concise."
                    .to_string(),
            });
        }

        Some(user)
    }

    /// Update a user in the database
    async fn update_db(&self, user: &User) {
        sqlx::query(
            r#"
            INSERT INTO messages (phone_number, total_received, total_sent, received_today, messages, last_reset)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(phone_number) DO UPDATE SET
                total_received = excluded.total_received,
                total_sent = excluded.total_sent,
                received_today = excluded.received_today,
                messages = excluded.messages,
                last_reset = excluded.last_reset
            "#,
        )
        .bind(&user.phone_number)
        .bind(user.total_received)
        .bind(user.total_sent)
        .bind(user.received_today)
        .bind(serde_json::to_string(&user.messages).unwrap())
        .bind(user.last_reset.timestamp())
        .execute(&*self.db_pool)
        .await
        .expect("Failed to update database");
    }
}
