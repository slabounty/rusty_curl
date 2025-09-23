use clap::{Parser as ClapParser};
use log::{info};
use env_logger::Env;

#[derive(ClapParser)]
#[command(version, about, long_about = None)]
struct Cli {
    // Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    url: String,
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Rusty Curl");

    let cli = Cli::parse();

    println!("Fetching URL: {}", cli.url);

    if let Some(output_file) = cli.output {
        println!("Saving output to: {}", output_file);
    } else {
        println!("No output file specified. Printing to stdout.");
    }
}
