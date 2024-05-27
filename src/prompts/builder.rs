use std::collections::HashMap;

use handlebars::Handlebars;
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
        let mut reg = Handlebars::new();

        let default_template = include_str!("prompt.hbs");

        reg.register_template_string("tpl_1", default_template)
            .map_err(|_e| PromptBuilderError::TemplateError)?;

        Ok(Self {
            template_engine: reg,
        })
    }

    pub fn build(&self, data: &HashMap<String, String>) -> Result<String, PromptBuilderError> {
        self.template_engine
            .render("tpl_1", &data)
            .map_err(|_e| PromptBuilderError::RenderError)
    }
}
