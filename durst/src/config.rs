use super::*;

use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;
use std::io::Read;

#[derive(Deserialize, Debug)]
pub struct Matches {
    pub app_name: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Rule {
    pub name: Option<String>,
    pub when_match: Vec<Matches>,
    pub action: String,
}

#[derive(Deserialize, Debug)]
pub struct Theme {
    pub name: String,
    pub color: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub rules: Option<Vec<Rule>>,
    pub history_lenght: Option<usize>,
    pub default_theme: Option<String>,
    pub themes: Option<Vec<Theme>>,
}

pub fn load_config(path: String) -> Vec<Config> {
    match read_file(path) {
        Ok(config_str) => {
            let config: Config = serde_yaml::from_str(&config_str).unwrap();
            debug!("{:?}", config);
            Vec::new()
        }
        Err(err) => {
            error!("Error reading config file: {:?}", err.to_string());
            Vec::new()
        }
    }
}

fn read_file(path: String) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    println!("file {:?}", file);
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}
