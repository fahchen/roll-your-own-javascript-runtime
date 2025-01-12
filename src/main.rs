use deno_ast::MediaType;
use deno_ast::ParseParams;
use deno_ast::SourceTextInfo;
use deno_core::error::AnyError;
use deno_core::futures::FutureExt;
use deno_core::include_js_files;
use deno_core::op;
use deno_core::v8;
use deno_core::Extension;
use std::rc::Rc;

#[op]
async fn op_read_file(path: String) -> Result<String, AnyError> {
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
async fn op_write_file(path: String, contents: String) -> Result<(), AnyError> {
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
async fn op_fetch(url: String) -> Result<String, AnyError> {
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}

#[op]
fn op_remove_file(path: String) -> Result<(), AnyError> {
    std::fs::remove_file(path)?;
    Ok(())
}

struct TsModuleLoader;

impl deno_core::ModuleLoader for TsModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, deno_core::error::AnyError> {
        match specifier {
            "jet:runtime" => deno_core::ModuleSpecifier::parse(specifier).map_err(|e| e.into()),
            "jet:query" => deno_core::ModuleSpecifier::parse(specifier).map_err(|e| e.into()),
            _ => deno_core::resolve_import(specifier, referrer).map_err(|e| e.into()),
        }
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
    ) -> std::pin::Pin<Box<deno_core::ModuleSourceFuture>> {
        let module_specifier = module_specifier.clone();
        async move {
            let (media_type, module_type, should_transpile, code) =
                if module_specifier.scheme() == "jet" {
                    match module_specifier.path() {
                        "runtime" => {
                            let code = std::fs::read_to_string("src/jet_runtime.ts")
                                .expect("Can't find jet_runtime.ts.");

                            (
                                MediaType::TypeScript,
                                deno_core::ModuleType::JavaScript,
                                true,
                                code,
                            )
                        }
                        "query" => {
                            let code = std::fs::read_to_string("src/query.ts")
                                .expect("Can't find query.ts.");

                            (
                                MediaType::TypeScript,
                                deno_core::ModuleType::JavaScript,
                                true,
                                code,
                            )
                        }
                        path => panic!("path {} not found", path),
                    }
                } else {
                    let path = module_specifier.to_file_path().unwrap();

                    let code = std::fs::read_to_string(&path)?;

                    let media_type = MediaType::from(&path);
                    let (module_type, should_transpile) = match media_type {
                        MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                            (deno_core::ModuleType::JavaScript, false)
                        }
                        MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                        MediaType::TypeScript
                        | MediaType::Mts
                        | MediaType::Cts
                        | MediaType::Dts
                        | MediaType::Dmts
                        | MediaType::Dcts
                        | MediaType::Tsx => (deno_core::ModuleType::JavaScript, true),
                        MediaType::Json => (deno_core::ModuleType::Json, false),
                        _ => panic!("Unknown extension {:?}", path.extension()),
                    };

                    (media_type, module_type, should_transpile, code)
                };

            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.to_string(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed.transpile(&Default::default())?.text
            } else {
                code
            };
            let module = deno_core::ModuleSource {
                code: code.into_bytes().into_boxed_slice(),
                module_type,
                module_url_specified: module_specifier.to_string(),
                module_url_found: module_specifier.to_string(),
            };
            Ok(module)
        }
        .boxed_local()
    }
}

async fn run_js(file_path: &str) -> Result<(), AnyError> {
    let main_module = deno_core::resolve_path(file_path)?;
    let runjs_extension = Extension::builder("runjs")
        .esm(include_js_files!("runtime.js",))
        .ops(vec![
            op_read_file::decl(),
            op_write_file::decl(),
            op_remove_file::decl(),
            op_fetch::decl(),
        ])
        .build();
    let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
        module_loader: Some(Rc::new(TsModuleLoader)),
        extensions: vec![runjs_extension],
        ..Default::default()
    });

    let path = main_module.to_file_path().unwrap();
    let source_code = std::fs::read_to_string(&path)?;

    let module_id = js_runtime
        .load_main_module(&main_module, Some(source_code))
        .await
        .unwrap();

    let evaluation = js_runtime.mod_evaluate(module_id);

    js_runtime.run_event_loop(false).await.unwrap();

    let _r = evaluation.await?;

    let result = {
        let module_namespace = js_runtime.get_module_namespace(module_id).unwrap();

        let scope = &mut js_runtime.handle_scope();

        let module_namespace = v8::Local::<v8::Object>::new(scope, module_namespace);

        let default_export_name =
            v8::String::new(scope, "handle").expect("handle functino is not exported");
        let binding = module_namespace
            .get(scope, default_export_name.into())
            .unwrap();

        let handle_fn =
            v8::Local::<v8::Function>::try_from(binding).expect("handle is not a function");

        let request = {
            let data = r#"
        {
            "to": "Alice"
        }"#;

            let v: deno_core::serde_json::Value = deno_core::serde_json::from_str(data)?;

            deno_core::serde_v8::to_v8(scope, v).expect("Bad request")
        };
        let context = {
            let data = r#"
        {
            "current_user": {
                "name": "Alice"
            }
        }"#;

            let v: deno_core::serde_json::Value = deno_core::serde_json::from_str(data)?;

            deno_core::serde_v8::to_v8(scope, v).expect("Bad context")
        };

        let receiver = v8::undefined(scope);

        let result = handle_fn.call(scope, receiver.into(), &[request, context]);

        v8::Global::new(scope, result.unwrap())
    };

    js_runtime.run_event_loop(false).await?;
    let result_global = js_runtime.resolve_value(result).await.unwrap();
    let scope = &mut js_runtime.handle_scope();
    let local = v8::Local::new(scope, result_global);

    let deserialized_value =
        deno_core::serde_v8::from_v8::<deno_core::serde_json::Value>(scope, local).unwrap();

    let j = deno_core::serde_json::to_string(&deserialized_value).unwrap();

    print!("\nPromise Result: {:#?}", j);

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.is_empty() {
        eprintln!("Usage: runjs <file>");
        std::process::exit(1);
    }
    let file_path = &args[1];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_js(file_path)) {
        eprintln!("error: {error}");
    }
}
