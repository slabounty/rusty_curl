use std::fs::File;
use std::io::{self, Write};

use crate::http::HttpResult;

pub fn build_writer(path: &Option<String>) -> io::Result<Box<dyn Write>> {
    let writer: Box<dyn Write> = if let Some(path) = path {
        Box::new(File::create(path)?) // use `?` to propagate errors
    } else {
        Box::new(io::stdout())        // directly box stdout
    };

    Ok(writer)
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

pub fn write_results<W: Write>(
    urls: Vec<String>,
    results: Vec<anyhow::Result<HttpResult>>,
    mut writer: W,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Write, Read};
    use tempfile::tempdir;
    use std::fs;
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

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
    fn test_write_results_all_success() {
        // Arrange
        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
        ];

        let results = vec![
            Ok(sample_http_result()),
            Ok(sample_http_result()),
        ];

        let mut buffer = Vec::new();

        // Act
        let had_failure =
            write_results(urls, results, Box::new(&mut buffer), true).unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Assert
        assert_eq!(had_failure, false); // all were successful
        assert!(output.contains("Status: 200 OK"));
        assert!(output.contains("Content-Length: Some(123)"));
        assert!(output.contains("application/json"));
        assert!(output.contains(r#"{"message":"hello"}"#));
        assert!(output.contains("Latency:")); // because latency flag is true
    }

    #[test]
    fn test_write_results_with_failure() {
        // Arrange
        let urls = vec![
            "https://good.example.com".to_string(),
            "https://bad.example.com".to_string(),
        ];

        let mut bad_resp = sample_http_result();
        bad_resp.status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;

        let results = vec![
            Ok(sample_http_result()),         // first OK
            Ok(bad_resp),                      // second has error status
        ];

        let mut buffer = Vec::new();

        // Act
        let had_failure =
            write_results(urls, results, Box::new(&mut buffer), false).unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Assert
        assert_eq!(had_failure, true); // at least one failure
        assert!(output.contains("Status: 200 OK"));
        assert!(output.contains("Status: 500 Internal Server Error"));
        assert!(output.contains(r#"{"message":"hello"}"#));
        assert!(!output.contains("Latency:")); // latency flag is false here
    }

    #[test]
    fn test_write_results_with_err_variant() {
        // Arrange
        let urls = vec![
            "https://good.example.com".to_string(),
            "https://error.example.com".to_string(),
        ];

        let results = vec![
            Ok(sample_http_result()),                     // first OK
            Err(anyhow::anyhow!("Network error")),        // second failed
        ];

        let mut buffer = Vec::new();

        // Act
        let had_failure =
            write_results(urls, results, Box::new(&mut buffer), true).unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Assert
        assert_eq!(had_failure, true); // because of the Err
        assert!(output.contains("Status: 200 OK")); // first result still written
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

    #[test]
    fn test_write_result_includes_all_fields() {
        let mut buffer = Vec::new(); // this implements Write
        let http_result = sample_http_result();

        // Call write_result with latency enabled
        write_result(&mut buffer, &http_result, true).unwrap();

        // Convert buffer into a String
        let output = String::from_utf8(buffer).unwrap();

        // Assert important fields appear
        assert!(output.contains("Status: 200 OK"));
        assert!(output.contains("Content-Length: Some(123)"));
        assert!(output.contains("application/json"));
        assert!(output.contains(r#"{"message":"hello"}"#));
        assert!(output.contains("Latency:")); // because we enabled output_latency
    }

    #[test]
    fn test_write_result_without_latency() {
        let mut buffer = Vec::new();
        let http_result = sample_http_result();

        write_result(&mut buffer, &http_result, false).unwrap();

        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Status: 200 OK"));
        assert!(!output.contains("Latency:")); // no latency printed
    }
}
