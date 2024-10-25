use std::cmp::max;
use std::collections::HashMap;
use std::str::FromStr;

use dashmap::DashMap;
use log::debug;
use ropey::Rope;
use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    ExecuteCommandOptions, ExecuteCommandParams, InitializeParams, InitializeResult,
    InitializedParams, MessageType, Position, Range, SaveOptions, ServerCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, Url,
    VersionedTextDocumentIdentifier, WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::lsp::code_actions::CodeActionData;
use crate::lsp::complete::Complete;

use super::code_actions::AiCodeAction;

/// Logs a client message at info level.
///
/// # Examples
///
/// ```
/// # #[macro_use]
/// client_info!(self.client, "{uri}");
/// ```
macro_rules! client_info {
    ($client:expr, $($arg:tt)*) => {
        $client.log_message(MessageType::INFO, format!($($arg)*)).await;
    };
}

/// Logs a client message at error level.
///
/// # Examples
///
/// ```
/// # #[macro_use]
/// client_error!(self.client, "Error occurred: {error_message}");
/// ```
macro_rules! client_error {
    ($client:expr, $($arg:tt)*) => {
        $client.log_message(MessageType::ERROR, format!($($arg)*)).await;
    };
}

// macro_rules! client_warn {
//     ($client:expr, $($arg:tt)*) => {
//         $client.log_message(MessageType::WARNING, format!($($arg)*)).await;
//     };
// }

#[derive(Debug)]
pub struct Backend {
    client: Client,
    document_map: DashMap<Url, Rope>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_map: DashMap::new(),
        }
    }

    fn insert_source(&self, uri: Url, text: &str) {
        let rope = Rope::from_str(text);
        self.document_map.insert(uri, rope);
    }

    fn update_source(&self, document: &TextDocumentIdentifier, text: Option<String>) {
        if let Some(text) = text {
            let rope = Rope::from_str(&text);
            self.document_map.insert(document.uri.clone(), rope);
        }
    }

    fn get_indexes_from_range(&self, range: &Range, source: &Rope) -> (usize, usize) {
        let start_line = usize::try_from(range.start.line).unwrap();
        let end_line = usize::try_from(range.end.line).unwrap();
        let start_char = usize::try_from(range.start.character).unwrap();
        let end_char = usize::try_from(range.end.character).unwrap();
        let start_idx = source.line_to_char(start_line);
        let end_idx = source.line_to_char(end_line);
        // client_info!(self.client, "{start},{end}={start_idx},{end_idx}");
        (start_idx + start_char, end_idx + end_char)
    }

    fn reload_source(
        &self,
        document: &VersionedTextDocumentIdentifier,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        if let Some(src) = self.document_map.get(&document.uri) {
            let mut source = src.to_owned();
            for change in changes {
                if (change.range, change.range_length) == (None, None) {
                    source = Rope::from_str(&change.text);
                } else if let Some(range) = change.range {
                    let new_lines: Vec<&str> = change.text.lines().collect();
                    let (start_idx, end_idx) = self.get_indexes_from_range(&range, &source);
                    source.remove(start_idx..end_idx);
                    source.insert(start_idx, new_lines.join("\n").as_str());
                }
            }
            self.document_map.insert(document.uri.clone(), source);
        } else {
            panic!("attempted to reload source that does not exist");
        }
    }

    fn get_source_range(&self, document_uri: &Url, range: &Range) -> Option<String> {
        self.document_map.get(document_uri).map(|src| {
            let source = src.to_owned();
            let (start_idx, end_idx) = self.get_indexes_from_range(range, &source);
            source.slice(start_idx..end_idx).to_string()
        })
    }

    async fn on_code_action(&self, params: CodeActionParams) -> CodeActionResponse {
        client_info!(self.client, "on code action");

        let text_doc = params.text_document;
        let document_uri = text_doc.uri;
        let range = params.range;
        let diagnostics = params.context.diagnostics;
        client_info!(self.client, "{diagnostics:?}");
        // let error_id_to_ranges = build_error_id_to_ranges(diagnostics);

        let mut response = CodeActionResponse::new();

        let code_actions = AiCodeAction::all();

        for code_action in &code_actions {
            let action = CodeAction {
                title: code_action.label().to_string(),
                command: None,
                diagnostics: None,
                edit: None,
                disabled: None,
                kind: Some(CodeActionKind::QUICKFIX),
                is_preferred: Some(true),
                data: Some(serde_json::json!(CodeActionData {
                    id: code_action.identifier().to_string(),
                    document_uri: document_uri.clone(),
                    range,
                    diagnostics: diagnostics.clone(),
                })),
            };
            response.push(CodeActionOrCommand::from(action));
        }

        response
    }

    async fn on_code_action_resolve(&self, params: CodeAction) -> CodeAction {
        let mut new_params = params.clone();

        let data = params.data;

        let code_action_data = data.map_or_else(
            || None,
            |json_obj| {
                let result: core::result::Result<CodeActionData, serde_json::Error> =
                    serde_json::from_value::<CodeActionData>(json_obj);
                Some(result)
            },
        );

        let args = if let Some(some_cad) = code_action_data {
            match some_cad {
                Ok(cad) => {
                    self.client
                        .log_message(MessageType::INFO, format!("Range {:#?}", &cad.range))
                        .await;

                    let context = self.get_source_range(&cad.document_uri, &cad.range);

                    Some((cad.document_uri.clone(), cad.range, context, cad.id))
                }
                Err(err) => {
                    client_error!(self.client, "{err}");
                    None
                }
            }
        } else {
            None
        };

        if let Some(arg) = args {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Executing {}", params.title.as_str()),
                )
                .await;

            let document_uri = arg.0;
            let range = arg.1;
            let context = arg.2;
            let id = arg.3;

            self.client
                .log_message(MessageType::INFO, format!("Context {context:?}"))
                .await;

            let code_action = AiCodeAction::from_str(&id).unwrap();
            let response = code_action.execute(context).await;

            if let Some(str_edit) = response {
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

                let edits = changes.entry(document_uri).or_default();

                let edit = TextEdit {
                    range,
                    new_text: str_edit,
                };

                edits.push(edit);

                let edit = Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                });

                new_params.edit = edit;
            }
        }

        new_params
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        // self.client
        //     .log_message(
        //         MessageType::INFO,
        //         format!(
        //             "Initializing {:?}",
        //             params.root_uri.unwrap_or_default().path()
        //         ),
        //     )
        //     .await;

        // Text Document Sync Configuration
        let text_document_sync = TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                include_text: Some(true),
            })),
            ..TextDocumentSyncOptions::default()
        });

        let completion_options = CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(vec!["C-x".to_owned(), ":".to_owned()]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            all_commit_characters: None,
            ..Default::default()
        };

        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(text_document_sync),
                // completion_provider: Some(completion_options),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["acai_instruct".to_owned()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: Some(true),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::INFO, "configuration changed!")
            .await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.client
            .log_message(MessageType::INFO, "watched files have changed!")
            .await;
    }

    async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
        self.client
            .log_message(MessageType::INFO, "command executed!")
            .await;

        match self.client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => self.client.log_message(MessageType::INFO, "applied").await,
            Ok(_) => self.client.log_message(MessageType::INFO, "rejected").await,
            Err(err) => self.client.log_message(MessageType::ERROR, err).await,
        }

        Ok(None)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file opened! {}", params.text_document.uri),
            )
            .await;
        self.insert_source(params.text_document.uri, &params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file changed! {}", params.text_document.uri),
            )
            .await;

        self.insert_source(
            params.text_document.uri,
            std::mem::take(&mut params.content_changes[0].text.as_str()),
        );
        // self.reload_source(&params.text_document, params.content_changes);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file saved! {}", params.text_document.uri),
            )
            .await;

        // self.insert_source(params.text_document.uri, &params.text.unwrap());
        // self.update_source(&params.text_document, params.text.clone());
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.client
            .log_message(MessageType::INFO, "code action!")
            .await;

        Ok(Some(self.on_code_action(params).await))
    }

    async fn code_action_resolve(&self, params: CodeAction) -> Result<CodeAction> {
        self.client
            .log_message(MessageType::INFO, "code action resolve!")
            .await;

        Ok(self.on_code_action_resolve(params).await)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.client
            .log_message(MessageType::INFO, "completion")
            .await;

        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(MessageType::INFO, uri.clone())
            .await;

        debug!(target: "acai", "{}", format!("### Completions position {position:?}"));

        let range = Range {
            start: Position {
                line: max(position.line - 3, 0),
                character: 0,
            },
            end: position,
        };

        let context = self.get_source_range(&uri, &range);

        let ctx = context.clone().unwrap();
        debug!(target: "acai", "{}", format!("### Completions context {ctx}"));

        let op = Complete {
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            context,
        };

        let response = op.send().await;

        if let Ok(Some(msg)) = response {
            debug!(target: "acai", "{}", format!("### Completions response  {msg}"));
            let detail = msg.chars().take(8).collect::<String>();
            debug!(target: "acai", "{}", format!("### Completions detail {detail}"));
            Ok(Some(CompletionResponse::Array(vec![
                CompletionItem::new_simple(msg, detail),
            ])))
        } else {
            Ok(None)
        }
    }
}
