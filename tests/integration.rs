#[cfg(test)]
mod tests {
    use reqwest::{Method};
    use rusty_curl::http::{make_client, request};

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
}
