use std::fs::File;
use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use clap::{Parser as ClapParser, ValueEnum};
use log::{info, warn, error};
use reqwest::{Client, Method};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use tokio::time::Instant;

const REQUEST_TIMEOUT: u64 = 10;

// Define an enum for a specific argument's possible values
#[derive(Default, Debug, Clone, ValueEnum, PartialEq)]
enum CliMethod {
    #[default]
    Get,
    Post,
    Put,
    Delete
}

#[derive(ClapParser, Default)]
#[command(version, about, long_about = None)]
struct Cli {
    // Sets an output file to write to
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    // Sets a body
    #[arg(short, long, value_name = "BODY")]
    body: Option<String>,

    // Sets a json
    #[arg(short, long, value_name = "JSON")]
    json: Option<String>,

    // Sets a form
    #[arg(short, long, value_name = "FORM")]
    form: Option<String>,

    // Add headers (e.g. -H "Accept: application/json")
    #[arg(short = 'H', long = "header", value_parser = parse_key_val, num_args = 0..)]
    headers: Vec<(String, String)>,

    // Choose a method
    #[arg(short, long, value_enum, default_value_t = CliMethod::Get)]
    method: CliMethod,

    // Print latency
    #[arg(short, long, value_name = "LATENCY")]
    latency: bool,

    // One or more URLs to fetch
    #[arg(value_name = "URL", required = true)]
    urls: Vec<String>,
}

struct HttpResult {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub content_length: Option<u64>,
    pub body: String,
    pub latency: Duration,
}

#[derive(Debug, Default)]
struct ValidationReport {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl ValidationReport {
    fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    fn check_and_exit(&self) -> Result<()> {
        if self.has_warnings() {
            warn!("Warnings:");
            for warn in &self.warnings {
                warn!("  - {}", warn);
            }
        }

        if self.has_errors() {
            error!("Errors:");
            for err in &self.errors {
                error!("  - {}", err);
            }
            anyhow::bail!("Exiting with errors");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    info!("Rusty Curl");

    let cli = Cli::parse();

    validate_cli(&cli).check_and_exit()?;

    let client = make_client();

    let body = cli.json.as_deref()
        .or(cli.body.as_deref())
        .or(cli.form.as_deref());
    let results = request_many(&client, &cli.urls, cli.method, body, &cli.headers).await;

    let writer = build_writer(&cli.output)?;

    let had_failure = write_results(cli.urls, results, writer, cli.latency)?;

    if had_failure {
        std::process::exit(1);
    }

    Ok(())
}

fn write_results(
    urls: Vec<String>,
    results: Vec<anyhow::Result<HttpResult>>,
    mut writer: Box<dyn Write>,
    latency: bool,
) -> io::Result<bool> {
    let mut had_failure = false;

    for (url, res) in urls.iter().zip(results) {
        match res {
            Ok(resp) => {
                write_result(&mut writer, &resp, latency)?;
                if !resp.status.is_success() {
                    eprintln!("Request to {} returned {}", url, resp.status);
                    had_failure = true;
                }
            }
            Err(e) => {
                eprintln!("Request to {} failed: {}", url, e);
                had_failure = true;
            }
        }
    }

    Ok(had_failure)
}

fn build_writer(path: &Option<String>) -> io::Result<Box<dyn Write>> {
    let writer: Box<dyn Write> = if let Some(path) = path {
        Box::new(File::create(path)?) // use `?` to propagate errors
    } else {
        Box::new(io::stdout())        // directly box stdout
    };

    Ok(writer)
}

fn make_client() -> ClientWithMiddleware {
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

// A function that takes any type implementing the Write trait
fn write_result<W: Write>(writer: &mut W, http_result: &HttpResult, output_latency: bool) -> io::Result<()> {
    writeln!(writer, "Status: {}", http_result.status)?;
    writeln!(writer, "Content-Length: {:?}", http_result.content_length)?;
    writeln!(writer, "Headers: {:#?}", http_result.headers)?;
    writeln!(writer, "Body:\n{}", http_result.body)?;
    if output_latency {
        writeln!(writer, "Latency: {:?}", http_result.latency)?;
    }

    writer.flush()?;

    Ok(())
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s.find(':').ok_or_else(|| format!("invalid KEY:VALUE: no `:` found in `{}`", s))?;
    let key = s[..pos].trim().to_string();
    let value = s[pos + 1..].trim().to_string();
    Ok((key, value))
}

fn validate_cli(cli: &Cli) -> ValidationReport {
    let mut report = ValidationReport::default();

    // Check that urls are well formed
    for url in cli.urls.iter() {
        if !valid_url(&url) {
            report.errors.push(format!("Invalid URL {}: must start with http:// or https://", url));
        }
    }

    // Warn if there's a body/json/form on a GET or DELETE
    if ((cli.method == CliMethod::Get) || (cli.method == CliMethod::Delete)) &&
        (cli.body.is_some() || cli.json.is_some() || cli.form.is_some()) {
        report.warnings.push("Body not allowed for GET or DELETE".to_string());
    }

    // Check if there's only one or zero of body, json, form
    if [cli.body.as_ref(), cli.json.as_ref(), cli.form.as_ref()]
        .iter()
        .filter(|opt| opt.is_some())
        .count() > 1
    {
        report.errors.push("Can't have more than one of body, json, and form".into());
    }

    // Check if there's json, that it's valid
    if let Some(json) = &cli.json {
        // Validate the JSON
        if let Err(e) = serde_json::from_str::<serde_json::Value>(json) {
            report.errors.push(format!("JSON is not valid: {}", e));
        }
    }

    // Return the generated report
    report
}

async fn request_many(
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

async fn request(client: &ClientWithMiddleware, url: &str, method: Method, body: Option<&str>, headers: &[(String, String)]) -> Result<HttpResult> {
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

fn valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Write, Read};
    use httpmock::prelude::*;
    use httpmock::{Mock, MockServer};
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    use tempfile::tempdir;

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

    #[tokio::test]
    async fn test_get_request_returns_body() {
        let client = make_client();
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
        let client = make_client();
        let url = "https://httpbin.org/uuid";
        // No headers
        let headers: Vec<(String, String)> = vec![];

        let http_result = request(&client, url, Method::GET, None, &headers).await.unwrap();

        // httpbin returns JSON with a uuid field
        assert!(http_result.body.contains("uuid"));
    }

    #[tokio::test]
    async fn test_post_request_returns_body() {
        let client = make_client();
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
        let client = make_client();
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
        let client = make_client();
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

    #[test]
    fn test_report_no_errors() -> Result<()> {
        let report = ValidationReport::default();

        assert_eq!(report.has_errors(), false);

        Ok(())
    }

    #[test]
    fn test_report_has_errors() -> Result<()> {
        let mut report = ValidationReport::default();

        report.errors.push("Some error".to_string());

        assert_eq!(report.has_errors(), true);

        Ok(())
    }

    #[test]
    fn test_report_no_warnings() -> Result<()> {
        let report = ValidationReport::default();

        assert_eq!(report.has_warnings(), false);

        Ok(())
    }

    #[test]
    fn test_report_has_warnings() -> Result<()> {
        let mut report = ValidationReport::default();

        report.warnings.push("Some warning".to_string());

        assert_eq!(report.has_warnings(), true);

        Ok(())
    }

    #[test]
    fn test_validate_cli_valid_url() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_errors(), false);

        Ok(())
    }

    #[test]
    fn test_validate_cli_invalid_url() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("httpX://example.com".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_errors(), true);

        assert!(
            report.errors.iter().any(|e| e.contains("Invalid URL")),
            "Expected an error containing 'Invalid URL'"
        );

        Ok(())
    }

    #[test]
    fn test_validate_cli_get_body() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());
        cli.body = Some("hello world".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_warnings(), true);

        assert!(
            report.warnings.iter().any(|e| e.contains("Body not allowed")),
            "Expected an warning containing 'Body not allowed'"
        );

        Ok(())
    }

    #[test]
    fn test_validate_cli_delete_body() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());
        cli.method = CliMethod::Delete;
        cli.body = Some("hello world".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_warnings(), true);

        assert!(
            report.warnings.iter().any(|e| e.contains("Body not allowed")),
            "Expected an warning containing 'Body not allowed'"
        );

        Ok(())
    }

    #[test]
    fn test_validate_cli_json_and_body() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());
        cli.method = CliMethod::Post;
        cli.body = Some("some body".to_string());
        cli.json = Some("some json".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_errors(), true);

        assert!(
            report.errors.iter().any(|e| e.contains("Can't have more than one of body, json, and form")),
            "Expected an error containing 'Can't have more than one of body, json, and form'"
        );

        Ok(())
    }

    #[test]
    fn test_validate_cli_form_and_body() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());
        cli.method = CliMethod::Post;
        cli.body = Some("some body".to_string());
        cli.form = Some("some form".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_errors(), true);

        assert!(
            report.errors.iter().any(|e| e.contains("Can't have more than one of body, json, and form")),
            "Expected an error containing 'Can't have more than one of body, json, and form'"
        );

        Ok(())
    }

    #[test]
    fn test_validate_cli_valid_json() -> Result<()> {
        let mut cli = Cli::default();   // all fields defaulted
        cli.urls.push("https://example.com".to_string());
        cli.method = CliMethod::Post;
        cli.json = Some("some not json".to_string());

        let report = validate_cli(&cli);

        assert_eq!(report.has_errors(), true);

        assert!(
            report.errors.iter().any(|e| e.contains("JSON is not valid")),
            "Expected an warning containing 'JSON is not valid'"
        );

        Ok(())
    }

    #[test]
    fn test_validation_report_errors() -> Result<()> {
        let mut report = ValidationReport::default();

        report.errors.push("Some Error".to_string());

        assert_eq!(report.has_errors(), true);

        Ok(())
    }

    #[test]
    fn test_validation_report_warnings() -> Result<()> {
        let mut report = ValidationReport::default();

        report.warnings.push("Some Warning".to_string());

        assert_eq!(report.has_warnings(), true);

        Ok(())
    }

    // A minimal struct definition for tests (if not imported)
    // Adjust if your actual struct has more fields.
    fn make_report(warnings: Vec<&str>, errors: Vec<&str>) -> ValidationReport {
        ValidationReport {
            warnings: warnings.into_iter().map(String::from).collect(),
            errors: errors.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn check_and_exit_returns_ok_if_no_warnings_or_errors() -> Result<()> {
        let report = make_report(vec![], vec![]);
        let result = report.check_and_exit();

        assert!(result.is_ok(), "Expected Ok(()) when there are no warnings or errors");
        Ok(())
    }

    #[test]
    fn check_and_exit_returns_ok_with_warnings_only() -> Result<()> {
        let report = make_report(vec!["deprecated flag"], vec![]);
        let result = report.check_and_exit();

        assert!(result.is_ok(), "Expected Ok(()) when there are only warnings");
        Ok(())
    }

    #[test]
    fn check_and_exit_returns_err_with_errors() {
        let report = make_report(
            vec!["deprecated flag"],
            vec!["missing required argument"]
        );
        let result = report.check_and_exit();

        assert!(result.is_err(), "Expected Err(_) when there are errors");
    }

    #[test]
    fn build_writer_returns_stdout_when_none() {
        // We can't easily inspect stdout itself, but we can check that it didn't error.
        let writer = build_writer(&None);
        assert!(writer.is_ok(), "Expected Ok(_) when path is None");
    }

    #[test]
    fn build_writer_creates_file_when_path_given() -> std::io::Result<()> {
        let dir = tempdir()?;                                   // create temp directory
        let file_path = dir.path().join("out.txt");
        let path_str = file_path.to_string_lossy().to_string();

        {
            let mut writer = build_writer(&Some(path_str.clone()))?;
            writeln!(writer, "Hello test!")?;                  // write to the file
        }

        // Check that the file was created and contains the expected text
        let mut contents = String::new();
        fs::File::open(&file_path)?.read_to_string(&mut contents)?;
        assert!(contents.contains("Hello test!"));

        Ok(())
    }

    #[test]
    fn build_writer_fails_for_unwritable_path() {
        // Try to write to an invalid directory (most likely will fail)
        let path = "/root/should_fail.txt".to_string(); // adjust if test runner runs as root
        let writer = build_writer(&Some(path));
        assert!(writer.is_err(), "Expected Err(_) when path is unwritable");
    }

    #[test]
    fn parse_key_val_valid_pair() {
        let input = "Content-Type: application/json";
        let result = parse_key_val(input).unwrap();
        assert_eq!(result, ("Content-Type".to_string(), "application/json".to_string()));
    }

    #[test]
    fn parse_key_val_trims_spaces() {
        let input = "  key  :   value with spaces  ";
        let result = parse_key_val(input).unwrap();
        assert_eq!(result, ("key".to_string(), "value with spaces".to_string()));
    }

    #[test]
    fn parse_key_val_handles_empty_value() {
        let input = "key:";
        let result = parse_key_val(input).unwrap();
        assert_eq!(result, ("key".to_string(), "".to_string()));
    }

    #[test]
    fn parse_key_val_handles_empty_key() {
        let input = ":value";
        let result = parse_key_val(input).unwrap();
        assert_eq!(result, ("".to_string(), "value".to_string()));
    }

    #[test]
    fn parse_key_val_error_no_colon() {
        let input = "keyvalue";
        let result = parse_key_val(input);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "invalid KEY:VALUE: no `:` found in `keyvalue`"
        );
    }

    #[test]
    fn parse_key_val_error_only_colon() {
        let input = ":";
        let result = parse_key_val(input).unwrap();
        assert_eq!(result, ("".to_string(), "".to_string())); // still valid: empty key and value
    }


    fn sample_http_result() -> HttpResult {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        HttpResult {
            status: reqwest::StatusCode::OK,
            content_length: Some(123),
            headers, // <-- now a real HeaderMap
            body: r#"{"message":"hello"}"#.to_string(),
            latency: std::time::Duration::from_millis(42),
        }
    }

    #[test]
    fn write_result_without_latency() {
        let mut buffer: Vec<u8> = Vec::new();
        let http_result = sample_http_result();

        write_result(&mut buffer, &http_result, false).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("Status: 200 OK"));
        assert!(output.contains("Content-Length: Some(123)"));
        assert!(output.contains(r#"Body:
{"message":"hello"}"#));
        assert!(!output.contains("Latency:")); // should NOT include latency
    }

    #[test]
    fn write_result_with_latency() {
        let mut buffer: Vec<u8> = Vec::new();
        let http_result = sample_http_result();

        write_result(&mut buffer, &http_result, true).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("Status: 200 OK"));
        assert!(output.contains("Latency:")); // should include latency now
    }

    #[test]
    fn write_result_flushes_output() {
        let mut buffer: Vec<u8> = Vec::new();
        let http_result = sample_http_result();

        // If flush wasn't called, some data could be missing
        write_result(&mut buffer, &http_result, false).unwrap();

        assert!(!buffer.is_empty(), "Buffer should contain written data");
    }
}
