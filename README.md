# Rust Based HTTP Requester

A rust base curl like program. This was done mostly as a learning
exercise in rust. There was pretty extensive use of chatGPT for much of
the code as in "how do you do X in rust".

## Examples
### Get is the default  
cr -- https://httpbin.org/get  -H "Accept: application/json" -H "User-Agent: rusty_curl"

### With method included  
cr -- https://httpbin.org/get --method get  -H "Accept: application/json" -H "User-Agent: rusty_curl"

### Post method with body  
cr -- https://httpbin.org/post --method post  -H "Accept: application/json" -H "User-Agent: rusty_curl" --body "hello world"

### Put method with body  
cr -- https://httpbin.org/put --method put -H "Accept: application/json" -H "User-Agent: rusty_curl" --body "hello world"

### Post method with json (put will be similar)  
cr -- https://httpbin.org/put --method post -H "Accept: application/json" -H "User-Agent: rusty_curl"  --json "{\"user\": \"some_user\"}"

### Delete method  
cr -- https://httpbin.org/delete --method delete   -H 'Accept: application/json' -H 'User-Agent: rusty_curl'

## --help response
Usage: rusty_curl [OPTIONS] <URL>...

Arguments:  
  <URL>...  
  
Options:  
  -o, --output <FILE>  
  -b, --body <BODY>  
  -j, --json <JSON>  
  -f, --form <FORM>  
  -H, --header [<HEADERS>...]  
  -m, --method <METHOD>        [default: get] [possible values: get, post, put, delete]  
  -l, --latency  
  -h, --help                   Print help  
  -V, --version                Print version  

## Script
There is one script in the scriptes directory. It's a naive search for
public functions that don't need to be public. It works about 90% of the
time. If you truly need something like this, there's better ways to do
it.

## Original Plan

Mostly did the plan but skipped hour 9.

### Hour 1 — Project setup & dependencies

#### Create the project:

cargo new rusty_curl
cd rusty_curl


Add dependencies (example; feature list is illustrative — pick features you need):

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
reqwest = { version = "0.11", features = ["json", "gzip", "brotli", "rustls-tls"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"


Create skeleton main.rs with tokio::main and a Cli struct using clap.

Run cargo run to confirm the binary builds.

### Hour 2 — Basic GET request (async)

Implement a minimal async GET using reqwest:

Read a URL from CLI and await a reqwest::Client::get.

Print status and the first 500 bytes of the body.

Example core code:

let client = reqwest::Client::new();
let res = client.get(&url).send().await?;
println!("Status: {}", res.status());
let text = res.text().await?;
println!("{}", &text[..text.len().min(500)]);


Concepts: async fn, .await, Result propagation with ?.

### Hour 3 — Pretty-print headers & status, handle errors

Print response headers and content-length if present.

Improve error handling: use anyhow::Context to add useful messages.

Handle non-UTF8 bodies gracefully (show as bytes or try text().await with fallback).

Log concise errors for invalid URLs, network failures, timeouts.

### Hour 4 — Add CLI options: method, headers, body

Extend Cli (clap) to accept:

--method (GET, POST, PUT, DELETE)

--header multiple times (-H "Accept: application/json")

--data or --body for POST payload

--output to save response to file

Parse header strings into HeaderName/HeaderValue (or keep as raw) and add them to reqwest::RequestBuilder.

Example:

let builder = client.request(method, &url);
let builder = headers.iter().fold(builder, |b, (k,v)| b.header(k, v));
let res = if let Some(body) = body { builder.body(body).send().await? } else { builder.send().await? };

### Hour 5 — POST with JSON and form support

Add flags to choose body type:

--json '{"key":"value"}' → builder.json(&value).send().await?

--form "k1=v1&k2=v2" → builder.form(&pairs).send().await?

Or raw --data

Validate JSON input and give helpful errors.

Concept: use serde_json::Value if you want to parse/validate JSON before sending.

### Hour 6 — Timeouts, retries, and status mapping

Configure client-level timeouts and retry logic:

Client::builder().timeout(Duration::from_secs(10)).build()?

Implement a simple retry loop for 5xx responses or timeouts.

Add exit codes based on status (e.g., non-2xx → non-zero exit code).

Track response time using tokio::time::Instant to print latency.

### Hour 7 — Save response to disk & streaming large bodies

Implement --output <file> which writes the response to disk.

For large bodies, stream bytes instead of loading whole body:

let mut stream = res.bytes_stream();
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    file.write_all(&chunk)?;
}


Concepts: streaming, tokio::io::AsyncWrite vs blocking std::fs::File with tokio::task::spawn_blocking if needed.

### Hour 8 — Concurrency & multiple URLs

Add support for multiple URLs in one command and make concurrent requests.

Use futures::stream::FuturesUnordered or futures::stream::iter(urls).map(|u| async move { ... }).buffer_unordered(n).

Limit concurrency with buffer_unordered or a semaphore.

Collect results and print a summary table (URL, status, time, bytes).

### Hour 9 — Add advanced features (auth, cookies, redirects)

Add optional flags:

--auth user:pass → Basic auth via client.basic_auth()

--bearer TOKEN → builder.bearer_auth(TOKEN)

--follow/--no-follow redirects via Client::builder().redirect(...)

Cookie jar support (if desired)

Add an option to print response as raw (incl. chunked transfer encoding) vs prettified.

### Hour 10 — Polish, logging, tests, and stretch

Add logging with tracing/env_logger to trace requests, headers, and retries.

Create a README and example commands:

cargo run -- --method GET --header "Accept: application/json" https://httpbin.org/get

cargo run -- --method POST --json '{"x":1}' https://httpbin.org/post

Write unit tests for argument parsing and integration tests using httpmock or wiremock crates.

Stretch goals:

Save metrics (latency, success rate) to a CSV.

Add a --raw mode to output curl-compatible command lines.

Build a simple UI (TUI) for showing live concurrent requests.

