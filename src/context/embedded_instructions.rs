const MODEL_INSTRUCTION: &str = "// model:";
const TEMPERATURE_INSTRUCTION: &str = "// temperature:";
const RETURN_FORMAT_INSTRUCTION: &str = "// return_format:";

pub struct EmbeddedInstructions {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub return_format: Option<String>,
    pub context: String,
}

/// Parses the input string to extract embedded instructions and context.
///
/// This function processes the input string line by line, looking for specific
/// instruction prefixes to populate the `EmbeddedInstructions` struct fields.
/// Lines that don't match any instruction prefix are considered part of the context.
///
/// # Arguments
///
/// * `input` - A string slice containing the input text to parse.
///
/// # Returns
///
/// An `EmbeddedInstructions` struct with parsed values and remaining context.
///
/// # Examples
///
/// ```
/// let input = "model: gpt-3.5-turbo\ntemperature: 0.7\nreturn_format: json\nSome context here\nMore context";
/// let result = parse_context(input);
/// assert_eq!(result.model, Some("gpt-3.5-turbo".to_string()));
/// assert_eq!(result.temperature, Some(0.7));
/// assert_eq!(result.return_format, Some("json".to_string()));
/// assert_eq!(result.context, "Some context here\nMore context");
/// ```
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
        context: context.join("\n"),
    }
}
