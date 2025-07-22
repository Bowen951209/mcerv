use std::{error::Error, fmt::Display};

use crate::Config;

#[derive(Debug, Clone, Copy)]
pub enum SelectServerError {
    ServerNotFound,
}

impl Display for SelectServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for SelectServerError {}

pub struct State {
    config: Config,
    selected_server: Option<String>,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            selected_server: None,
        }
    }
    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn get_selected_server(&self) -> Option<String> {
        self.selected_server.clone()
    }

    pub fn set_selected_server(&mut self, server: String) -> Result<(), SelectServerError> {
        if !self.config.get_servers().contains_key(&server) {
            return Err(SelectServerError::ServerNotFound);
        }

        self.selected_server = Some(server);
        Ok(())
    }
}
