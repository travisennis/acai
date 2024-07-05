static MODEL_INSTRUCTION: &str = "// model:";
static TEMPERATURE_INSTRUCTION: &str = "// temperature:";
static RETURN_FORMAT_INSTRUCTION: &str = "// return_format:";

pub struct EmbeddedInstructions {
    pub model: Option<String>,
    pub temperature: f32,
    pub return_format: Option<String>,
    pub context: String,
}

pub fn parse_context(input: &str) -> EmbeddedInstructions {
    let mut model = None::<String>;
    let mut temperature = 0.0;
    let mut return_format = None::<String>;
    let mut context = String::new();

    for line in input.lines() {
        if line.starts_with(MODEL_INSTRUCTION) {
            model = Some(
                line.trim_start_matches(MODEL_INSTRUCTION)
                    .trim()
                    .to_string(),
            );
        } else if line.starts_with(TEMPERATURE_INSTRUCTION) {
            temperature = line
                .trim_start_matches(TEMPERATURE_INSTRUCTION)
                .trim()
                .parse()
                .unwrap_or(0.0);
        } else if line.starts_with(RETURN_FORMAT_INSTRUCTION) {
            return_format = Some(
                line.trim_start_matches(RETURN_FORMAT_INSTRUCTION)
                    .trim()
                    .to_string(),
            );
        } else {
            context.push_str(line);
            context.push('\n');
        }
    }

    EmbeddedInstructions {
        model,
        temperature,
        return_format,
        context: context.trim().to_string(),
    }
}
