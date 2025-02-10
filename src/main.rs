mod chatbot;
use std::sync::Arc;

use async_trait::async_trait;

use crate::chatbot::ChatBot;
use axum::{
    extract::{Form, State},
    response::IntoResponse,
    routing::post,
    Router,
};
use reqwest::Client;
use serde::Deserialize;

const DB_STRING: &str = "sqlite:messages.db";

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct TwilioWebhook {
    From: String,
    Body: String,
}

#[async_trait]
trait MessagingClient: Send + Sync {
    async fn send_message(&self, phone_number: &str, message: &str);
    async fn receive_message(&self, phone_number: &str, message: &str);
}

struct TwilioSMSClient {
    account_sid: String,
    auth_token: String,
    phone_number: String, // phone number to send messages from
    client: Client,
    chatbot: Arc<ChatBot>,
}

impl TwilioSMSClient {
    pub fn new(
        chatbot: Arc<ChatBot>,
        account_sid: String,
        auth_token: String,
        phone_number: String,
    ) -> Self {
        TwilioSMSClient {
            account_sid,
            auth_token,
            phone_number,
            client: Client::new(),
            chatbot,
        }
    }
}

#[async_trait]
impl MessagingClient for TwilioSMSClient {
    async fn send_message(&self, phone_number: &str, message: &str) {
        println!("Sending SMS to: {}", phone_number);
        println!("Message: {}", message);

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        let body = format!(
            "From={}&To={}&Body={}",
            self.phone_number, phone_number, message
        );

        self.client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to send message");
    }

    async fn receive_message(&self, phone_number: &str, message: &str) {
        println!("Received SMS from: {}", phone_number);
        println!("Message: {}", message);

        let reply_message = self
            .chatbot
            .handle_message(phone_number.to_string(), message.to_string())
            .await;

        self.send_message(phone_number, &reply_message).await;
    }
}

async fn handle_sms(
    State(messaging_client): State<Arc<dyn MessagingClient>>,
    Form(webhook): Form<TwilioWebhook>,
) -> impl IntoResponse {
    messaging_client
        .receive_message(&webhook.From, &webhook.Body)
        .await;
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // load .env file if it exists

    let chatbot = ChatBot::new(DB_STRING.to_string()).await;
    let messaging_client = Arc::new(TwilioSMSClient::new(
        Arc::new(chatbot),
        std::env::var("TWILIO_ACCOUNT_SID").unwrap(),
        std::env::var("TWILIO_AUTH_TOKEN").unwrap(),
        std::env::var("TWILIO_PHONE_NUMBER").unwrap(),
    ));

    let app = Router::new()
        .route("/sms", post(handle_sms))
        .with_state(messaging_client);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
