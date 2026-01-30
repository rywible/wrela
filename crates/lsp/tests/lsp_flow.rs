use serde_json::Value;
use tower::util::ServiceExt;
use tower::Service;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::*;
use tower_lsp::LspService;
use wrela_lsp::Backend;

#[tokio::test]
async fn lsp_flow_basic() {
    let (mut service, _socket) = LspService::new(|client| Backend::new(client));

    let init_params = InitializeParams {
        root_uri: Some(Url::parse("file:///tmp").unwrap()),
        capabilities: ClientCapabilities::default(),
        ..InitializeParams::default()
    };
    let init_request = Request::build("initialize")
        .id(1)
        .params(serde_json::to_value(init_params).unwrap())
        .finish();
    let init_response = service
        .ready()
        .await
        .unwrap()
        .call(init_request)
        .await
        .unwrap()
        .unwrap();
    assert!(init_response.is_ok());

    let uri = Url::parse("file:///test.wr").unwrap();
    let did_open = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "wrela".to_string(),
            version: 1,
            text: "to main():\n    foo = Foo()\n    foo.\n\nA Foo:\n    has:\n        value: Int\n"
                .to_string(),
        },
    };
    let open_request = Request::build("textDocument/didOpen")
        .params(serde_json::to_value(did_open).unwrap())
        .finish();
    let open_response = service
        .ready()
        .await
        .unwrap()
        .call(open_request)
        .await
        .unwrap();
    assert!(open_response.is_none());

    let completion_params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line: 2, character: 8 },
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let completion_request = Request::build("textDocument/completion")
        .id(2)
        .params(serde_json::to_value(completion_params).unwrap())
        .finish();
    let completion_response = service
        .ready()
        .await
        .unwrap()
        .call(completion_request)
        .await
        .unwrap()
        .unwrap();
    let result = completion_response.result().unwrap();
    let items: CompletionResponse = serde_json::from_value(result.clone()).unwrap();
    match items {
        CompletionResponse::Array(items) => assert!(!items.is_empty()),
        CompletionResponse::List(list) => assert!(!list.items.is_empty()),
    }

    let format_params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        options: FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..FormattingOptions::default()
        },
        work_done_progress_params: Default::default(),
    };
    let format_request = Request::build("textDocument/formatting")
        .id(3)
        .params(serde_json::to_value(format_params).unwrap())
        .finish();
    let format_response = service
        .ready()
        .await
        .unwrap()
        .call(format_request)
        .await
        .unwrap()
        .unwrap();
    let format_result = format_response.result().unwrap().clone();
    let edits: Vec<TextEdit> = serde_json::from_value(format_result).unwrap();
    assert!(!edits.is_empty());

    let execute_params = ExecuteCommandParams {
        command: "wrela.goToTypeDefinition".to_string(),
        arguments: vec![Value::String(uri.to_string()), Value::from(4), Value::from(6)],
        work_done_progress_params: Default::default(),
    };
    let execute_request = Request::build("workspace/executeCommand")
        .id(4)
        .params(serde_json::to_value(execute_params).unwrap())
        .finish();
    let execute_response = service
        .ready()
        .await
        .unwrap()
        .call(execute_request)
        .await
        .unwrap()
        .unwrap();
    assert!(execute_response.is_ok());
}
