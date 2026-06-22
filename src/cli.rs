use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Config {
    pub path: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum CliError {
    MissingArg,
    Io(String),
}

pub fn parse_args(args: Vec<String>) -> Result<Config, CliError> {
    let path = args.get(1).ok_or(CliError::MissingArg)?;
    Ok(Config { path: PathBuf::from(path) })
}

pub fn read_source(cfg: &Config) -> Result<String, CliError> {
    std::fs::read_to_string(&cfg.path).map_err(|e| CliError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_path_arg() {
        let cfg = parse_args(vec!["mdcat".into(), "README.md".into()]).unwrap();
        assert_eq!(cfg.path, PathBuf::from("README.md"));
    }

    #[test]
    fn errors_when_no_path() {
        assert_eq!(parse_args(vec!["mdcat".into()]), Err(CliError::MissingArg));
    }
}
