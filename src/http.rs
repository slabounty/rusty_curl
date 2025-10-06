use std::time::Duration;

use anyhow::Result;
use log::{info};
use reqwest::{Client, Method};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use tokio::time::Instant;

use crate::cli::{CliMethod};

const REQUEST_TIMEOUT: u64 = 10;

pub struct HttpResult {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub content_length: Option<u64>,
    pub body: String,
    pub latency: Duration,
}

pub fn make_client() -> ClientWithMiddleware {
    info!("make_client: Creating Client");

    let base_client = Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT))
        .build();

    // Retry up to 3 times with increasing intervals between attempts.
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    ClientBuilder::new(base_client.unwrap())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

pub async fn request_many(
    client: &ClientWithMiddleware,
    urls: &[String],
    method: CliMethod,
    body: Option<&str>,
    headers: &[(String, String)],
) -> Vec<anyhow::Result<HttpResult>> {
    // Each item in the iterator becomes an async block that returns a future
    let futures = urls.iter().map(|url| {
        let client = client.clone(); // clone client so each future owns it
        let url = url.clone();
        let headers = headers.to_vec();
        let body = body.map(|b| b.to_string());
        let method = method.clone();

        async move {
            match method {
                CliMethod::Get    => request(&client, &url, Method::GET,    None,              &headers).await,
                CliMethod::Post   => request(&client, &url, Method::POST,   body.as_deref(),   &headers).await,
                CliMethod::Put    => request(&client, &url, Method::PUT,    body.as_deref(),   &headers).await,
                CliMethod::Delete => request(&client, &url, Method::DELETE, None,              &headers).await,
            }
        }
    });

    futures::future::join_all(futures).await
}

pub async fn request(client: &ClientWithMiddleware, url: &str, method: Method, body: Option<&str>, headers: &[(String, String)]) -> Result<HttpResult> {
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
    let start_time = Instant::now();

    info!("Request: calling send");
    let resp = builder.send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let content_length = resp.content_length();
    let body = resp.text().await?;

    let latency = start_time.elapsed();

    info!("Request: returning result");
    Ok(HttpResult {
        status,
        headers,
        content_length,
        body,
        latency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use httpmock::{Mock, MockServer};

    #[tokio::test]
    async fn test_get_request_returns_body_mock() {
        // Start a mock server on a random local port
        let server = MockServer::start_async().await;

        let client = make_client();
        let mock = build_get_mock(&server, "").await;

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

    async fn build_get_mock<'a>(server: &'a MockServer, trailer: &str) -> Mock<'a> {
        server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/get{}", trailer))
                .header("Accept", "application/json");

            then.status(200)
                .header("Content-Type", "application/json")
                .body(format!(r#"{{ "url": "http://localhost/get{}" }}"#, trailer));
        })
        .await
    }

    #[tokio::test]
    async fn test_get_request_many_returns_bodys_mock() {
        // Start a mock server on a random local port
        let server = MockServer::start_async().await;

        let mock_1 = build_get_mock(&server, "_1").await;
        let mock_2 = build_get_mock(&server, "_2").await;

        let client = make_client();

        let mut urls: Vec<String> = Vec::new();
        urls.push(format!("{}/get_1", server.base_url()));
        urls.push(format!("{}/get_2", server.base_url()));

        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "rusty_curl_test".to_string()),
        ];

        // Call your own request function
        let http_results = request_many(&client, &urls, CliMethod::Get, None, &headers)
            .await;

        let http_result_1 = http_results[0].as_ref().expect("First request failed");
        assert!(http_result_1.body.contains("\"url\": \"http://localhost/get_1\""));

        let http_result_2 = http_results[1].as_ref().expect("First request failed");
        assert!(http_result_2.body.contains("\"url\": \"http://localhost/get_2\""));

        // Verify that the mock was actually called
        mock_1.assert_async().await;

        // Verify that the mock was actually called
        mock_2.assert_async().await;
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
        let client = make_client();
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
        let client = make_client();
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

        let client = make_client();
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

}
