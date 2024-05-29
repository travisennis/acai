use std::collections::HashMap;

use handlebars::{no_escape, Handlebars};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PromptBuilderError {
    #[error("template error")]
    TemplateError,
    #[error("render error")]
    RenderError,
}

pub struct PromptBuilder<'a> {
    template_engine: Handlebars<'a>,
}

impl PromptBuilder<'_> {
    pub fn new() -> Result<Self, PromptBuilderError> {
        let default_template = include_str!("prompt.hbs");
        println!("{default_template}");

        let mut reg = Handlebars::new();

        reg.register_escape_fn(no_escape);

        reg.register_template_string("default", default_template)
            .map_err(|_e| PromptBuilderError::TemplateError)?;

        Ok(Self {
            template_engine: reg,
        })
    }

    pub fn build(&self, data: &HashMap<String, String>) -> Result<String, PromptBuilderError> {
        self.template_engine
            .render("default", &data)
            .map_err(|_e| PromptBuilderError::RenderError)
    }
}
