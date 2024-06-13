use llama_cpp_rs::{
    options::{ModelOptions, PredictOptions},
    LLama,
};
use tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded_channel};
use crate::server::ServerConf;
use crate::errors::OcrError;

fn create_llama(model_path: &str) -> Result<LLama, OcrError> {
    let model_options = ModelOptions {
        context_size: 8192,
        n_gpu_layers: 50,
        ..Default::default()
    };

    LLama::new(model_path.into(), &model_options).map_err(|e| OcrError::LlamaError(e))
}

#[derive(Debug, Clone)]
pub struct LlamaTranslator {
    llama: LLama,
}

impl LlamaTranslator {
    pub fn new(model_path: &str) -> Result<Self, OcrError> {
        let llama = create_llama(model_path)?;
        Ok(Self { llama })
    }

}

#[derive(Debug, Clone)]
pub enum TranslateEvent {
    Request{text: String, ret_sender: UnboundedSender<String>},
}

pub fn run_llama_in_thread(server_conf: ServerConf) -> UnboundedSender<TranslateEvent> {
    let (req_sender, req_receiver) = unbounded_channel();
    std::thread::spawn(move || {
        let llama_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime error");
        if let Some(model_path) = server_conf.llama_model_path {
            let llama_translator = LlamaTranslator::new(&model_path).expect("create llama translator error");
            llama_rt.block_on(async move {
                let mut llama = llama_translator.llama;
                let mut req_receiver = req_receiver;
                while let Some(event) = req_receiver.recv().await {
                    match event {
                        TranslateEvent::Request{text, ret_sender} => {
                            let predict_options = PredictOptions {
                                tokens: 512,
                                threads: 32,
                                top_k: 5,
                                top_p: 0.25,
                                stop_prompts:  vec![
                                    "User:".to_owned(), 
                                    "\n\n".to_owned(),
                                ],
                                ..Default::default()
                            };
                            let query = format!("User: 翻译：{} \n根据上面信息翻译成中文。 Assistant:", text);
                            eprintln!(">>> query: {}", &query);
                            let ret = llama.predict(query, &predict_options).expect("llama predict error");
                            eprint!(">>> predict ret:{}", &ret.0);
                            if let Err(e) = ret_sender.send(ret.0) {
                                eprintln!("failed to send translated text: {}", e);
                            }
                        }
                    }
                }
            });

        }
    });
    req_sender
}
