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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Rusty Curl");

    let cli = Cli::parse();

    let url = cli.url;

    if let Some(output_file) = cli.output {
        println!("Saving output to: {}", output_file);
    } else {
        println!("No output file specified. Printing to stdout.");
    }


    let client = reqwest::Client::new();
    let body = get_request(&client, &url).await?;
    println!("Got:\n{}", body);

    Ok(())
}

async fn get_request(client: &Client, url: &str) -> Result<String> {
    let body = client.get(url)
        .send()
        .await?
        .text()
        .await?;

    Ok(body)
}


#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    #[tokio::test]
    async fn test_get_request_returns_body() {
        let client = Client::new();
        let url = "https://httpbin.org/get";

        let body = get_request(&client, url).await.unwrap();

        assert!(body.contains("\"url\": \"https://httpbin.org/get\""));
    }

    #[tokio::test]
    async fn test_get_request_uuid() {
        let client = Client::new();
        let url = "https://httpbin.org/uuid";

        let body = get_request(&client, url).await.unwrap();

        // httpbin returns JSON with a uuid field
        assert!(body.contains("uuid"));
    }
}
