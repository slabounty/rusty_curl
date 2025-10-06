mod cli;
mod http;
mod output;

use anyhow::Result;
use clap::{Parser as ClapParser};
use log::{info};

use crate::cli::{Cli, validate_cli};
use crate::output::{build_writer, write_results};
use crate::http::{make_client, request_many};

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
