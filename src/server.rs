use axum::{
    body::Bytes,
    extract::State,
    routing::{get, post}, 
    response::IntoResponse,
    http::{header::HeaderMap, StatusCode},
    Router,
}; 
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::errors::OcrError;
use crate::translate::TranslateEvent;

#[derive(Debug, Default, Clone)]
pub struct ServerConf {
    pub host: String,
    pub port: u16,
    pub tesseract_data: String,
    pub tesseract_default_lang: String,
    pub llama_model_path: Option<String>,
}

#[derive(Debug, Clone)]
struct ServerState {
    pub conf: ServerConf,
    pub translate_req_sender: UnboundedSender<TranslateEvent>,
}

pub async fn run_server(conf: ServerConf, translate_req_sender: UnboundedSender<TranslateEvent>) -> Result<(), OcrError> {
    let server_state: ServerState = ServerState {
        conf: conf.clone(),
        translate_req_sender,
    };
    let app = Router::new()
        .route("/", get(root))
        .route("/ocr", post(ocr_image))
        .route("/translate", post(translate))
        .route("/imtranslate", post(immersive_translate))
        .route("/ocr_and_translate", post(ocr_and_translate))
        .with_state(server_state);
    
    let listener = tokio::net::TcpListener::bind(&format!("{}:{}", &conf.host, &conf.port)).await?; 
    axum::serve(listener, app).await.map_err(|e| OcrError::IoError(e))
}

async fn root() -> &'static str {
    "Hello, flameshot!"
}


// receive image binary bytes in body and return text in body
async fn ocr_image(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let ocr_lang = headers.get("flameshot_ocr_lang")
        .map(|h| h.to_str().unwrap_or(&state.conf.tesseract_default_lang))
        .unwrap_or(&state.conf.tesseract_default_lang).to_owned();
    let ret = tokio::task::spawn_blocking(move || {
        let tesseract_data = &state.conf.tesseract_data;
        let tesseract_default_lang = &ocr_lang;
        let tes = tesseract::Tesseract::new(
            Some(tesseract_data),
            Some(tesseract_default_lang),
        ).expect("new tesseract error");

        let mut ttes = tes.set_image_from_mem(&body).expect("set image error");
        match &ttes.get_text() {
            Ok(s) => {
                eprintln!("==> {}", s);
                (StatusCode::OK, s.to_string())
            },
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }).await;
    ret.unwrap_or_else(|e| {
        eprintln!("ocr_image error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "ocr_image error".to_string())
    })
}


async fn translate(
    State(state): State<ServerState>,
    _headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let req_text = String::from_utf8_lossy(&body);
    let (ret_sender, mut ret_receiver) = tokio::sync::mpsc::unbounded_channel();
    match state.translate_req_sender.send(TranslateEvent::Request{text: req_text.to_string(), ret_sender}) {
        Ok(_) => {
            let ret = ret_receiver.recv().await;
            match ret {
                Some(s) => {
                    (StatusCode::OK, s)
                },
                None => (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string()),
            }
        },
        Err(e) => {
            eprintln!("translate error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string())
        }
    }
}

async fn immersive_translate(
    State(state): State<ServerState>,
    _headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let body_str = String::from_utf8_lossy(&body);
    println!("==== {}", body_str);
    match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(body_json) => {
            // get "text_list" from body_json
            match body_json.get("text_list") {
                Some(text_list) => {
                    if text_list.is_array() {
                        let first_text = text_list.as_array().expect("must be array")
                            .get(0);
                        if let Some(first_text) = first_text {
                            if first_text.is_string() {
                                let source_text = first_text.as_str().expect("must be string");
                                let (ret_sender, mut ret_receiver) = unbounded_channel();
                                match state.translate_req_sender.send(TranslateEvent::Request{text: source_text.to_string(), ret_sender}) {
                                    Ok(_) => {
                                        match ret_receiver.recv().await {
                                            Some(s) => {
                                                let mut ret_map = serde_json::Map::new();
                                                let mut json_map = serde_json::Map::new();
                                                json_map.insert("text".to_string(), serde_json::Value::String(s.clone()));
                                                json_map.insert("from".to_string(), serde_json::Value::String("en".to_string()));
                                                json_map.insert("to".to_string(), serde_json::Value::String("zh_CN".to_string()));
                                                let ret_list = serde_json::Value::Array(vec![serde_json::Value::from(json_map)]);
                                                ret_map.insert("translations".to_string(), ret_list);
                                                (StatusCode::OK, serde_json::to_string_pretty(&ret_map).unwrap())
                                            },
                                            None => (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string()),
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("translate error: {}", e);
                                        (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string())
                                    }
                                }
                            } else {
                                (StatusCode::INTERNAL_SERVER_ERROR, "immersive_translate error: first_text not string".to_string())
                            }
                        } else {
                            (StatusCode::INTERNAL_SERVER_ERROR, "immersive_translate error: first_text not found in text_list".to_string())
                        }
                    } else {
                        (StatusCode::OK, "".to_string())
                    }
                }
                None => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "immersive_translate error: text_list not found in body_json".to_string())
                }
            }
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("immersive_translate error: {}", e))
        }
    }
}

async fn ocr_and_translate(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let ocr_lang = headers.get("flameshot_ocr_lang")
        .map(|h| h.to_str().unwrap_or(&state.conf.tesseract_default_lang))
        .unwrap_or(&state.conf.tesseract_default_lang).to_owned();

    let req_sender = state.translate_req_sender.clone();
    let ret = tokio::task::spawn_blocking(move || {
        let tesseract_data = &state.conf.tesseract_data;
        let tesseract_default_lang = &ocr_lang;
        let tes = tesseract::Tesseract::new(
            Some(tesseract_data),
            Some(tesseract_default_lang),
        ).expect("new tesseract error");

        let mut ttes = tes.set_image_from_mem(&body).expect("set image error");
        match &ttes.get_text() {
            Ok(s) => {
                eprintln!("==> {}", s);
                (StatusCode::OK, s.to_string())
            },
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }).await;
    match ret {
        Ok((sts, s)) => {
            let (ret_sender, mut ret_receiver) = unbounded_channel();
            if sts == StatusCode::OK {
                match state.translate_req_sender.send(TranslateEvent::Request{text: s.clone(), ret_sender}) {
                    Ok(_) => {
                        let ret = ret_receiver.recv().await;
                        match ret {
                            Some(ts) => (StatusCode::OK, format!("{}\n------------\n{}", s, ts)),
                            None => (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string()),
                        }
                    },
                    Err(e) => {
                        eprintln!("translate error: {}", e);
                        (StatusCode::INTERNAL_SERVER_ERROR, "translate error".to_string())
                    }
                }
            } else {
                (sts, s)
            }
        } 
        Err(e) => {
            eprintln!("ocr_image error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "ocr_image error".to_string())
        }
    }
}   
