use anyhow::Result;
use clap::{Parser as ClapParser, ValueEnum};
use log::{warn, error};

// Define an enum for a specific argument's possible values
#[derive(Default, Debug, Clone, ValueEnum, PartialEq)]
pub enum CliMethod {
    #[default]
    Get,
    Post,
    Put,
    Delete
}

#[derive(ClapParser, Default)]
#[command(version, about, long_about = None)]
pub struct Cli {
    // Sets an output file to write to
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<String>,

    // Sets a body
    #[arg(short, long, value_name = "BODY")]
    pub body: Option<String>,

    // Sets a json
    #[arg(short, long, value_name = "JSON")]
    pub json: Option<String>,

    // Sets a form
    #[arg(short, long, value_name = "FORM")]
    pub form: Option<String>,

    // Add headers (e.g. -H "Accept: application/json")
    #[arg(short = 'H', long = "header", value_parser = parse_key_val, num_args = 0..)]
    pub headers: Vec<(String, String)>,

    // Choose a method
    #[arg(short, long, value_enum, default_value_t = CliMethod::Get)]
    pub method: CliMethod,

    // Print latency
    #[arg(short, long, value_name = "LATENCY")]
    pub latency: bool,

    // One or more URLs to fetch
    #[arg(value_name = "URL", required = true)]
    pub urls: Vec<String>,
}


#[derive(Debug, Default)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationReport {
    fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn check_and_exit(&self) -> Result<()> {
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


pub fn validate_cli(cli: &Cli) -> ValidationReport {
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

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s.find(':').ok_or_else(|| format!("invalid KEY:VALUE: no `:` found in `{}`", s))?;
    let key = s[..pos].trim().to_string();
    let value = s[pos + 1..].trim().to_string();
    Ok((key, value))
}

fn valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
