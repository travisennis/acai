use std::path::{Path, PathBuf};

use handlebars::{no_escape, to_json, Handlebars};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("template error")]
    TemplateError,
    #[error("render error")]
    RenderError,
}

pub struct Builder<'a> {
    template_engine: Handlebars<'a>,
    data: Map<String, Value>,
    template: (String, String),
}

impl Builder<'_> {
    pub fn new(path: &Option<PathBuf>) -> anyhow::Result<Self> {
        let template = get_template(path)?;

        let mut reg = Handlebars::new();

        reg.register_escape_fn(no_escape);

        reg.register_template_string(&template.1, template.0.clone())
            .map_err(|_e| Error::TemplateError)?;

        Ok(Self {
            template_engine: reg,
            data: Map::new(),
            template,
        })
    }

    pub fn new_from_path(path: &Option<PathBuf>) -> anyhow::Result<Self> {
        let template = get_template(path)?;

        let mut reg = Handlebars::new();

        reg.register_escape_fn(no_escape);

        reg.register_template_string(&template.1, template.0.clone())
            .map_err(|_e| Error::TemplateError)?;

        Ok(Self {
            template_engine: reg,
            data: Map::new(),
            template,
        })
    }

    pub fn new_from_string(template: String) -> anyhow::Result<Self> {
        let template = (template, "custom".to_string());

        let mut reg = Handlebars::new();

        reg.register_escape_fn(no_escape);

        reg.register_template_string(&template.1, template.0.clone())
            .map_err(|_e| Error::TemplateError)?;

        Ok(Self {
            template_engine: reg,
            data: Map::new(),
            template,
        })
    }

    pub fn add_vec_variable(&mut self, key: String, values: &Vec<Value>) {
        self.data.insert(key, to_json(values));
    }

    pub fn add_variable(&mut self, key: String, value: String) {
        self.data.insert(key, to_json(value));
    }

    pub fn clear_variables(&mut self) {
        self.data.clear();
    }

    pub(crate) fn contains_variables(&self) -> bool {
        !self.data.is_empty()
    }

    pub fn build(&self) -> Result<String, Error> {
        self.template_engine
            .render(&self.template.1, &self.data)
            .map_err(|_e| Error::RenderError)
    }
}

fn get_template(path: &Option<PathBuf>) -> anyhow::Result<(String, String)> {
    if let Some(template_path) = path {
        let content = std::fs::read_to_string(template_path)?;
        Ok((content, "custom".to_string()))
    } else {
        Ok((
            include_str!("prompt.hbs").to_string(),
            "default".to_string(),
        ))
    }
}
