# textgpt

This is a simple project to message ChatGPT via SMS messaging. 

> I know this project already exists, like [textgpt.net](https://textgpt.net/), but I created this as something to mess around with and practice using async in Rust. This would have been actually useful while traveling recently, as I wanted to ask ChatGPT something, but couldn't because I didn't have internet, but I did have SMS service with Apple's satellite SMS service.

Currently this integrates Twilio's SMS API to OpenAI's GPT4 API. The code is modular, so the messaging service and chatbot logic is split up. Adding another messaging service is as simple as adding another `impl` of `MessagingClient`. I am planning to add a messaging system like WhatsApp or Telegram that works on "messaging only" WiFi networks on airplanes.

## Pre-requisites
- Set up a dev account and get an API key from [OpenAI](https://platform.openai.com/docs/quickstart)
- Set up a dev account and get an API key, buy a phone number from [Twilio](https://www.twilio.com/en-us/messaging/channels/sms)
    - Note: To actually be able to send SMS messages, you need to apply and be approved (about $20 of fees) for an A2P 10DLC registration. This takes a few days to be approved.
- Install Rust [rustup.rs](https://rustup.rs/)

## Usage

1. Clone the repository
2. Create a `.env` file with the following variables:
    ```env
    OPENAI_API_KEY=your_openai_api_key
    TWILIO_ACCOUNT_SID=your_twilio_account_sid
    TWILIO_AUTH_TOKEN=your_twilio_auth_token
    TWILIO_PHONE_NUMBER=your_twilio_phone_number
    ```
3. Run the program with `cargo run`
4. Expose the server to the internet with a service like [ngrok](https://ngrok.com/), or deploy it to your favorite cloud provider
5. Set the webhook for incoming messages to `http://your_url/sms` in the Twilio console
6. Send a message to your Twilio phone number and ChatGPT will respond!

## Special Commands (text these to the chatbot)
- `!help` - Get a list of commands
- `!stats` Print out the current stats of your conversation with the chatbot

