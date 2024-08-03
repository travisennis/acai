use anyhow::Result;
use clap::Args;
use log::{error, info};
use std::{env, error::Error, path::PathBuf};

use crate::{
    cli::CmdRunner,
    files::{get_content_blocks, get_files, parse_patterns},
    models::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    // Path to the codebase directory
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Patterns to include
    #[clap(long)]
    pub include: Option<String>,

    /// Patterns to exclude
    #[clap(long)]
    pub exclude: Option<String>,

    // Path to a handlebars template
    #[clap(long)]
    pub template: Option<PathBuf>,

    /// Prompt
    #[clap(long)]
    pub prompt: Option<String>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        match env::current_dir() {
            Ok(path) => info!("The current working directory is: {}", path.display()),
            Err(e) => error!("Error: {e}"),
        }

        // Parse Patterns
        let include_patterns = parse_patterns(&self.include);
        let exclude_patterns = parse_patterns(&self.exclude);

        let file_objects = self.path.as_ref().map_or(Vec::new(), |path| {
            get_files(path.as_path(), &include_patterns, &exclude_patterns)
                .map_or(Vec::new(), |files| files)
        });

        let content_blocks = get_content_blocks(&file_objects);

        let final_prompt = self
            .prompt
            .as_ref()
            .map_or_else(String::new, ToString::to_string);

        let mut prompt_builder = crate::prompts::Builder::new(&self.template)?;

        prompt_builder.add_variable("prompt".to_string(), final_prompt);
        prompt_builder.add_vec_variable("files".to_string(), &content_blocks);

        if prompt_builder.contains_variables() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build()?,
            };

            println!("{}", msg.content);
        }

        Ok(())
    }
}

// let bpe = o200k_base().unwrap();

// let query_tokens = bpe.encode_with_special_tokens("impl CmdRunner");

// for result in Walk::new(self.path.clone().unwrap()) {
//     // Each item yielded by the iterator is either a directory entry or an
//     // error, so either print the path or the error.
//     match result {
//         Ok(entry) => {
//             let path = entry.path();
//             if path.is_file()
//                 && should_include_file(path, &include_patterns, &exclude_patterns)
//             {
//                 println!("{}", path.display());
//                 let contents = read_file_contents(path)?;
//                 println!("{}", contents.len());
//                 let tokens = bpe.encode_with_special_tokens(&contents);
//                 println!("{}", tokens.len());
//                 let similarity = jaccard_similarity_vec(&tokens, &query_tokens);
//                 println!("{similarity}");

//                 let file_extension =
//                     path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

//                 let code_block = wrap_code_block(&contents, path);

//                 files.push(json!({
//                     "path": path.display().to_string(),
//                     "extension": file_extension,
//                     "code": code_block,
//                 }));
//             }
//         }
//         Err(err) => eprintln!("ERROR: {err}"),
//     }
// }

// fn wrap_code_block_orig(
//     code: &str,
//     extension: &str,
//     line_numbers: bool,
//     no_codeblock: bool,
// ) -> String {
//     let delimiter = "`".repeat(3);
//     let mut code_with_line_numbers = String::new();

//     if line_numbers {
//         for (line_number, line) in code.lines().enumerate() {
//             code_with_line_numbers.push_str(&format!("{:4} | {}\n", line_number + 1, line));
//         }
//     } else {
//         code_with_line_numbers = code.to_string();
//     }

//     if no_codeblock {
//         code_with_line_numbers
//     } else {
//         format!("{delimiter}{extension}\n{code_with_line_numbers}\n{delimiter}")
//     }
// }

// fn wrap_code_block2(code: &str, extension: &str) -> String {
//     let markdown_delimiter = "`".repeat(3);
//     let language_name = extension_to_name(extension);
//     let start_delimiter = format!("{markdown_delimiter} {language_name}");
//     let end_delimiter = "`".repeat(3);

//     let block = code.to_string();

//     format!("{start_delimiter}\n{block}\n{end_delimiter}")
// }

// fn jaccard_similarity<'a, 'b>(
//     sa: impl Iterator<Item = &'a usize>,
//     sb: impl Iterator<Item = &'b usize>,
// ) -> usize {
//     let s1: HashSet<&usize> = sa.collect::<HashSet<_>>();
//     let s2: HashSet<&usize> = sb.collect::<HashSet<_>>();

//     let i = s1.intersection(&s2).count();
//     let u = s1.union(&s2).count();

//     i / u
// }
