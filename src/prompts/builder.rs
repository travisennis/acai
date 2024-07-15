use std::collections::HashMap;

use handlebars::{no_escape, Handlebars};
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
    data: HashMap<String, String>,
}

impl Builder<'_> {
    pub fn new() -> Result<Self, Error> {
        let default_template = include_str!("prompt.hbs");

        let mut reg = Handlebars::new();

        reg.register_escape_fn(no_escape);

        reg.register_template_string("default", default_template)
            .map_err(|_e| Error::TemplateError)?;

        Ok(Self {
            template_engine: reg,
            data: HashMap::new(),
        })
    }

    pub fn add_variable(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    pub fn clear_variables(&mut self) {
        self.data.clear();
    }

    pub(crate) fn contains_variables(&self) -> bool {
        !self.data.is_empty()
    }

    pub fn build(&self) -> Result<String, Error> {
        self.template_engine
            .render("default", &self.data)
            .map_err(|_e| Error::RenderError)
    }
}
