use std::fs::File;
use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser as ClapParser, ValueEnum};
use log::{info, warn};
use env_logger::Env;
use reqwest::{Client, Method};

// Define an enum for a specific argument's possible values
#[derive(Debug, Clone, ValueEnum, PartialEq)]
enum CliMethod {
    Get,
    Post,
    Put,
    Delete
}

#[derive(ClapParser)]
#[command(version, about, long_about = None)]
struct Cli {
    // Sets an output file to write to (not currently implemented)
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    // Sets a body
    #[arg(short, long, value_name = "BODY")]
    body: Option<String>,

    // Sets a json
    #[arg(short, long, value_name = "JSON")]
    json: Option<String>,

    // Add headers (e.g. -H "Accept: application/json")
    #[arg(short = 'H', long = "header", value_parser = parse_key_val, num_args = 0..)]
    headers: Vec<(String, String)>,

    // Choose a method
    #[arg(short, long, value_enum, default_value_t = CliMethod::Get)]
    method: CliMethod,

    url: String,
}

struct HttpResult {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub content_length: Option<u64>,
    pub body: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Rusty Curl");

    let cli = Cli::parse();
    let url = cli.url;

    // Check url is well formed
    if !valid_url(&url) {
        anyhow::bail!("Invalid URL: must start with http:// or https://");
    }

    // Warn if there's a body on a GET or DELETE
    if ((cli.method == CliMethod::Get) || (cli.method == CliMethod::Delete)) && cli.body.is_some() {
        warn!("Body not allowed for GET or DELETE");
    }

    // Check is there's both json and a body
    if cli.body.is_some() && cli.json.is_some() {
        anyhow::bail!("Can't have both a body and json");
    }

    // Check if there's json, that it's valid
    if let Some(json) = &cli.json {
        // Validate the JSON
        if let Err(e) = serde_json::from_str::<serde_json::Value>(json) {
            anyhow::bail!("JSON is not valid: {}", e);
        }
    }

    // Create a reqwest client
    let client = reqwest::Client::new();

    let http_result = match cli.method {
        CliMethod::Get => request(&client, &url, Method::GET, None, &cli.headers).await?,
        CliMethod::Post => request(&client, &url, Method::POST, cli.json.as_deref().or(cli.body.as_deref()), &cli.headers).await?,
        CliMethod::Put => request(&client, &url, Method::PUT, cli.json.as_deref().or(cli.body.as_deref()), &cli.headers).await?,
        CliMethod::Delete => request(&client, &url, Method::DELETE, None, &cli.headers).await?,
    };

    // Check for valid status response
    if !http_result.status.is_success() {
        anyhow::bail!("Request failed with status: {}", http_result.status);
    }

    if let Some(output_file) = cli.output {
        let mut file = File::create(output_file)?;
        write_result(&mut file, &http_result)?;
    }
    else {
        let mut stdout = io::stdout();
        write_result(&mut stdout, &http_result)?;
    }

    Ok(())
}

// A function that takes any type implementing the Write trait
fn write_result<W: Write>(writer: &mut W, http_result: &HttpResult) -> io::Result<()> {
    writeln!(writer, "Status: {}", http_result.status)?;
    writeln!(writer, "Content-Length: {:?}", http_result.content_length)?;
    writeln!(writer, "Headers: {:#?}", http_result.headers)?;
    writeln!(writer, "Body:\n{}", http_result.body)?;
    writer.flush()?;

    Ok(())
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s.find(':').ok_or_else(|| format!("invalid KEY:VALUE: no `:` found in `{}`", s))?;
    let key = s[..pos].trim().to_string();
    let value = s[pos + 1..].trim().to_string();
    Ok((key, value))
}

async fn request(client: &Client, url: &str, method: Method, body: Option<&str>, headers: &[(String, String)]) -> Result<HttpResult> {
    info!("Request: method = {}", method);
    let mut builder = client.request(method, url);

    // Add headers
    info!("Request: adding headers");
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    info!("Request: checking body");
    if let Some(b) = body {
        builder = builder.body(b.to_string());
    }

    info!("Request: calling send");
    let resp = builder.send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    info!("Request: returning result");
    Ok(HttpResult {
        status,
        headers,
        content_length,
        body,
    })
}

fn valid_url(url: &str) -> bool {
    if url.starts_with("http://") || url.starts_with("https://") {
        true
    }
    else {
        false
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn test_get_request_returns_body_mock() {
        // Start a mock server on a random local port
        let server = MockServer::start_async().await;

        // Define what the mock server should return
        let mock = server.mock_async(|when, then| {
            when.method(GET)
                .path("/get")
                .header("Accept", "application/json");

            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{ "url": "http://localhost/get" }"#);
        }).await;

        let client = Client::new();
        let url = format!("{}/get", server.base_url());

        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "rusty_curl_test".to_string()),
        ];

        // Call your own request function
        let http_result = request(&client, &url, reqwest::Method::GET, None, &headers)
            .await
            .unwrap();

        // Verify body contains mocked JSON
        assert!(http_result.body.contains("\"url\": \"http://localhost/get\""));

        // Verify that the mock was actually called
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_post_request_returns_body_mock() {
        // 1. Start a local mock server
        let server = MockServer::start();

        // 2. Define the mock: it expects POST and responds with JSON
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/submit")
                .header("Content-Type", "application/json")
                .body(r#"{"hello":"world"}"#);
            then.status(201)
                .header("Content-Type", "application/json")
                .body(r#"{"status":"ok"}"#);
        });

        // 3. Prepare the request
        let client = Client::new();
        let url = format!("{}/submit", &server.base_url());
        let headers = vec![("Content-Type".into(), "application/json".into())];
        let body = Some(r#"{"hello":"world"}"#);

        // 4. Call your request function
        let http_result = request(&client, &url, Method::POST, body, &headers)
            .await
            .expect("Request should succeed");

        // 5. Verify the response your code processed
        assert_eq!(http_result.status.as_u16(), 201);
        assert!(http_result.body.contains(r#""status":"ok""#));
        // Verify that the mock was actually called
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_put_request_returns_body_mock() {
        // 1. Start a local mock server
        let server = MockServer::start();

        // 2. Define the mock: it expects PUT and responds with JSON
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/submit")
                .header("Content-Type", "application/json")
                .body(r#"{"hello":"world"}"#);
            then.status(201)
                .header("Content-Type", "application/json")
                .body(r#"{"status":"ok"}"#);
        });

        // 3. Prepare the request
        let client = Client::new();
        let url = format!("{}/submit", &server.base_url());
        let headers = vec![("Content-Type".into(), "application/json".into())];
        let body = Some(r#"{"hello":"world"}"#);

        // 4. Call your request function
        let http_result = request(&client, &url, Method::PUT, body, &headers)
            .await
            .expect("Request should succeed");

        // 5. Verify the response your code processed
        assert_eq!(http_result.status.as_u16(), 201);
        assert!(http_result.body.contains(r#""status":"ok""#));
        // Verify that the mock was actually called
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_delete_request_returns_body_mock() {
        // Start a mock server on a random local port
        let server = MockServer::start_async().await;

        // Define what the mock server should return
        let mock = server.mock_async(|when, then| {
            when.method(DELETE)
                .path("/delete")
                .header("Accept", "application/json");

            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{ "url": "http://localhost/delete" }"#);
        }).await;

        let client = Client::new();
        let url = format!("{}/delete", server.base_url());

        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "rusty_curl_test".to_string()),
        ];

        // Call your own request function
        let http_result = request(&client, &url, reqwest::Method::DELETE, None, &headers)
            .await
            .unwrap();

        // Verify body contains mocked JSON
        assert!(http_result.body.contains("\"url\": \"http://localhost/delete\""));

        // Verify that the mock was actually called
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_get_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/get";

        // Manually define headers as Vec<(String, String)>
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "rusty_curl_test".to_string()),
        ];

        let http_result = request(&client, url, Method::GET, None, &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/get\""));
    }

    #[tokio::test]
    async fn test_get_request_uuid() {
        let client = Client::new();
        let url = "https://httpbin.org/uuid";
        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = request(&client, url, Method::GET, None, &headers).await.unwrap();

        // httpbin returns JSON with a uuid field
        assert!(http_result.body.contains("uuid"));
    }

    #[tokio::test]
    async fn test_post_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/post";
        let body = "hello world";

        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = request(&client, url, Method::POST, Some(body), &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/post\""));
        assert!(http_result.body.contains("hello world"));
    }

    #[tokio::test]
    async fn test_put_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/put";
        let body = "hello world";

        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = request(&client, url, Method::PUT, Some(body), &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/put\""));
        assert!(http_result.body.contains("hello world"));
    }

    #[tokio::test]
    async fn test_delete_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/delete";

        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = request(&client, url, Method::DELETE, None, &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/delete\""));
    }

    #[test]
    fn test_valid_url() -> Result<()> {
        let url = "https://route/to/page";

        assert_eq!(valid_url(&url), true);

        Ok(())
    }

    #[test]
    fn test_invalid_url() -> Result<()> {
        let url = "not_http://route/to/page";

        assert_eq!(valid_url(&url), false);

        Ok(())
    }
}
