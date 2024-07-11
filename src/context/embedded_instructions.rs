const MODEL_INSTRUCTION: &str = "// model:";
const TEMPERATURE_INSTRUCTION: &str = "// temperature:";
const RETURN_FORMAT_INSTRUCTION: &str = "// return_format:";

pub struct EmbeddedInstructions {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub return_format: Option<String>,
    pub context: String,
}

pub fn parse_context(input: &str) -> EmbeddedInstructions {
    let mut model = None::<String>;
    let mut temperature = None::<f32>;
    let mut return_format = None::<String>;
    let mut context: Vec<&str> = Vec::new();

    for line in input.lines() {
        if line.starts_with(MODEL_INSTRUCTION) {
            model = Some(
                line.trim_start_matches(MODEL_INSTRUCTION)
                    .trim()
                    .to_string(),
            );
        } else if line.starts_with(TEMPERATURE_INSTRUCTION) {
            temperature = Some(
                line.trim_start_matches(TEMPERATURE_INSTRUCTION)
                    .trim()
                    .parse()
                    .unwrap_or(0.0),
            );
        } else if line.starts_with(RETURN_FORMAT_INSTRUCTION) {
            return_format = Some(
                line.trim_start_matches(RETURN_FORMAT_INSTRUCTION)
                    .trim()
                    .to_string(),
            );
        } else {
            context.push(line.trim());
        }
    }

    EmbeddedInstructions {
        model,
        temperature,
        return_format,
        context: context.join("\n").to_string(),
    }
}
