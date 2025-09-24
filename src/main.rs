use anyhow::Result;
use clap::{Parser as ClapParser};
use log::{info};
use env_logger::Env;
use reqwest::Client;


#[derive(ClapParser)]
#[command(version, about, long_about = None)]
struct Cli {
    // Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

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

    if let Some(output_file) = cli.output {
        println!("Saving output to: {}", output_file);
    } else {
        println!("No output file specified. Printing to stdout.");
    }


    let client = reqwest::Client::new();

    let http_result = get_request(&client, &url).await?;

    // Check for valid status response
    if !http_result.status.is_success() {
        anyhow::bail!("Request failed with status: {}", http_result.status);
    }
    println!("Status: {}", http_result.status);
    println!("Content-Length: {:?}", http_result.content_length);
    println!("Headers: {:#?}", http_result.headers);
    println!("Body:\n{}", http_result.body);

    Ok(())
}

fn valid_url(url: &str) -> bool {
    if url.starts_with("http://") || url.starts_with("https://") {
        true
    }
    else {
        false
    }
}

async fn get_request(client: &Client, url: &str) -> Result<HttpResult> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    Ok(HttpResult { status, headers, content_length, body })
}


#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    #[tokio::test]
    async fn test_get_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/get";

        let http_result = get_request(&client, url).await.unwrap();

        assert!(http_result.body.contains("\"url\": \"https://httpbin.org/get\""));
    }

    #[tokio::test]
    async fn test_get_request_uuid() {
        let client = Client::new();
        let url = "https://httpbin.org/uuid";

        let http_result = get_request(&client, url).await.unwrap();

        // httpbin returns JSON with a uuid field
        assert!(http_result.body.contains("uuid"));
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
