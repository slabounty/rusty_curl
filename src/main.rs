use std::fs::File;
use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser as ClapParser, ValueEnum};
use log::{info};
use env_logger::Env;
use reqwest::Client;

// Define an enum for a specific argument's possible values
#[derive(Debug, Clone, ValueEnum)]
enum Method {
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

    // Add headers (e.g. -H "Accept: application/json")
    #[arg(short = 'H', long = "header", value_parser = parse_key_val, num_args = 0..)]
    headers: Vec<(String, String)>,

    // Choose a method
    #[arg(short, long, value_enum, default_value_t = Method::Get)]
    method: Method,

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

    // Create a reqwest client
    let client = reqwest::Client::new();

    let http_result = match cli.method {
        Method::Get => get_request(&client, &url, &cli.headers).await?,
        Method::Post => post_request(&client, &url, cli.body.as_deref(), &cli.headers).await?,
        Method::Put => put_request(&client, &url, cli.body.as_deref(), &cli.headers).await?,
        Method::Delete => delete_request(&client, &url, &cli.headers).await?,
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

async fn get_request(client: &Client, url: &str, headers: &[(String, String)]) -> Result<HttpResult> {
    use reqwest::Method; // import reqwest's Method

    // Start request builder
    let mut builder = client.request(Method::GET, url);

    // Add headers
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    // Send request
    let resp = builder.send().await?;let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    Ok(HttpResult { status, headers, content_length, body })
}

async fn post_request(client: &Client, url: &str, body: Option<&str>, headers: &[(String, String)]) -> Result<HttpResult> {
    use reqwest::Method; // import reqwest's Method
    let mut builder = client.request(Method::POST, url);

    // Add headers
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    if let Some(b) = body {
        builder = builder.body(b.to_string());
    }

    let resp = builder.send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    Ok(HttpResult {
        status,
        headers,
        content_length,
        body,
    })
}

async fn put_request(client: &Client, url: &str, body: Option<&str>, headers: &[(String, String)]) -> Result<HttpResult> {
    use reqwest::Method; // import reqwest's Method

    let mut builder = client.request(Method::PUT, url);

    // Add headers
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    if let Some(b) = body {
        builder = builder.body(b.to_string());
    }

    let resp = builder.send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    Ok(HttpResult {
        status,
        headers,
        content_length,
        body,
    })
}

async fn delete_request(client: &Client, url: &str, headers: &[(String, String)]) -> Result<HttpResult> {
    use reqwest::Method; // import reqwest's Method
    let mut builder = client.request(Method::DELETE, url);

    // Add headers
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    let resp = builder.send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

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

    #[tokio::test]
    async fn test_get_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/get";

        // Manually define headers as Vec<(String, String)>
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "rusty_curl_test".to_string()),
        ];

        let http_result = get_request(&client, url, &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/get\""));
    }

    #[tokio::test]
    async fn test_get_request_uuid() {
        let client = Client::new();
        let url = "https://httpbin.org/uuid";
        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = get_request(&client, url, &headers).await.unwrap();

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

        let http_result = post_request(&client, url, Some(body), &headers).await.unwrap();

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

        let http_result = put_request(&client, url, Some(body), &headers).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/put\""));
        assert!(http_result.body.contains("hello world"));
    }

    #[tokio::test]
    async fn test_delete_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/delete";

        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = delete_request(&client, url, &headers).await.unwrap();

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
