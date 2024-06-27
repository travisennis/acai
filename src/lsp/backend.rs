use std::cmp::max;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    ExecuteCommandOptions, ExecuteCommandParams, InitializeParams, InitializeResult,
    InitializedParams, MessageType, Position, Range, ServerCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
    VersionedTextDocumentIdentifier, WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::operations::{Complete, Document, Fix, Instruct, Optimize, Suggest};

#[derive(Serialize, Deserialize, Debug)]
struct CodeActionData {
    document_uri: Url,
    range: Range,
}

#[derive(Debug)]
struct State {
    sources: HashMap<Url, String>,
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    state: Mutex<State>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            state: Mutex::new(State {
                sources: HashMap::new(),
            }),
        }
    }

    async fn on_code_action(&self, params: CodeActionParams) -> CodeActionResponse {
        self.client
            .log_message(MessageType::INFO, "on code action")
            .await;

        let text_doc = params.text_document;
        let document_uri = text_doc.uri;
        let range = params.range;
        // let diagnostics = params.context.diagnostics;
        // let error_id_to_ranges = build_error_id_to_ranges(diagnostics);

        let mut response = CodeActionResponse::new();

        let actions = [
            "CA: Instruct",
            "CA: Document",
            "CA: Fix",
            "CA: Optimize",
            "CA: Suggest",
            "CA: Fill-in-middle",
        ];

        for title in &actions {
            let action = CodeAction {
                title: (*title).to_string(),
                command: None,
                diagnostics: None,
                edit: None,
                disabled: None,
                kind: Some(CodeActionKind::QUICKFIX),
                is_preferred: Some(true),
                data: Some(serde_json::json!(CodeActionData {
                    document_uri: document_uri.clone(),
                    range,
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
            |data| {
                let result: core::result::Result<CodeActionData, serde_json::Error> =
                    serde_json::from_value::<CodeActionData>(data.clone());
                Some(result)
            },
        );

        if let Some(some_cad) = code_action_data {
            match some_cad {
                Ok(cad) => {
                    let state = self.state.lock().await;
                    let context = get_source_range(&state, &cad.document_uri, &cad.range);

                    let response = execute_operation(params.title.as_str(), context).await;

                    if let Some(str_edit) = response {
                        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

                        let edits = changes.entry(cad.document_uri.clone()).or_default();

                        let edit = TextEdit {
                            range: cad.range,
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
                Err(err) => self.client.log_message(MessageType::ERROR, err).await,
            }
        }

        new_params
    }
}

async fn execute_operation(op_title: &str, context: Option<String>) -> Option<String> {
    match op_title {
        "CA: Instruct" => {
            let op = Instruct {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            response.map_or(None, |result| result.map(|msg| msg.content))
        }
        "CA: Document" => {
            let op = Document {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            response.map_or(None, |result| result.map(|msg| msg.content))
        }
        "CA: Fix" => {
            let op = Fix {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            response.map_or(None, |result| result.map(|msg| msg.content))
        }
        "CA: Optimize" => {
            let op = Optimize {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            response.map_or(None, |result| result.map(|msg| msg.content))
        }
        "CA: Suggest" => {
            let op = Suggest {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            response.map_or(None, |result| result.map(|msg| msg.content))
        }
        "CA: Fill-in-middle" => {
            let op = Complete {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            };

            let response = op.send().await;

            if let Ok(Some(response_msg)) = response {
                Some(response_msg)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Initializing {:?}", params.root_uri.unwrap().path()),
            )
            .await;

        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // completion_provider: Some(CompletionOptions {
                //     resolve_provider: Some(true),
                //     trigger_characters: Some(vec![".".to_owned(), ":".to_owned()]),
                //     work_done_progress_options: WorkDoneProgressOptions::default(),
                //     all_commit_characters: None,
                //     ..Default::default()
                // }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["codingassistant/instruct".to_owned()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: Some(true),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                // Some(CodeActionProviderCapability::Simple(true)),
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

        let mut state = self.state.lock().await;
        insert_source(&mut state, &params.text_document);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file changed! {}", params.text_document.uri),
            )
            .await;

        // let mut state = self.state.lock().await;
        // reload_source(&mut state, &params.text_document, params.content_changes);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file saved! {}", params.text_document.uri),
            )
            .await;

        let mut state = self.state.lock().await;
        update_source(&mut state, &params.text_document, params.text);
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

        let range = Range {
            start: Position {
                line: max(position.line - 3, 0),
                character: 0,
            },
            end: position,
        };

        let state = self.state.lock().await;
        let context = get_source_range(&state, &uri, &range);

        self.client
            .log_message(MessageType::INFO, context.clone().unwrap())
            .await;

        let op = Complete {
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            prompt: None,
            context,
        };

        let response = op.send().await;

        let msg = if let Ok(Some(response_msg)) = response {
            Some(response_msg)
        } else {
            None
        };

        self.client
            .log_message(MessageType::INFO, msg.clone().unwrap())
            .await;

        msg.map_or(Ok(None), |msg| {
            Ok(Some(CompletionResponse::Array(vec![
                CompletionItem::new_simple(msg.clone(), msg),
            ])))
        })
    }
}

fn insert_source(state: &mut State, document: &TextDocumentItem) {
    if !state.sources.contains_key(&document.uri) {
        state
            .sources
            .insert(document.uri.clone(), document.text.clone());
    }
}

fn update_source(state: &mut State, document: &TextDocumentIdentifier, text: Option<String>) {
    if let Some(text) = text {
        state.sources.insert(document.uri.clone(), text);
    }
}

fn reload_source(
    state: &mut State,
    document: &VersionedTextDocumentIdentifier,
    changes: Vec<TextDocumentContentChangeEvent>,
) {
    if let Some(src) = state.sources.get(&document.uri) {
        let mut source = src.to_owned();
        for change in changes {
            if (change.range, change.range_length) == (None, None) {
                source = change.text;
            } else if let Some(range) = change.range {
                let mut lines: Vec<&str> = source.lines().collect();
                let new_lines: Vec<&str> = change.text.lines().collect();
                let start = usize::try_from(range.start.line).unwrap();
                let end = usize::try_from(range.end.line).unwrap();
                lines.splice(start..end, new_lines);
                source = lines.join("\n");
            }
        }
        state.sources.insert(document.uri.clone(), source);
    } else {
        panic!("attempted to reload source that does not exist");
    }
}

fn get_source_range(state: &State, document_uri: &Url, range: &Range) -> Option<String> {
    state.sources.get(document_uri).and_then(|src| {
        let source = src.to_owned();
        let lines: Vec<&str> = source.lines().collect();
        let start = usize::try_from(range.start.line).unwrap();
        let end = usize::try_from(range.end.line).unwrap();
        let range_lines = lines.get(start..end);

        range_lines.map(|target_lines| target_lines.join("\n"))
    })
}
