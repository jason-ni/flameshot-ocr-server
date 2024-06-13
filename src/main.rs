use clap::{arg, command, value_parser, Command};
use tokio;

mod errors;
mod ocr;
mod server;
mod translate;

use server::ServerConf;

fn main() {
    let matches = command!()
        .subcommand(
            Command::new("server")
                .about("run ocr server")
                .arg(
                    arg!(-l --host <HOST> "server listen host")
                        .default_value("127.0.0.1")
                )
                .arg(
                    arg!(-p --port <PORT> "server listen port")
                        .default_value("5000")
                        .value_parser(value_parser!(u16))
                )
                .arg(
                    arg!(-d --tesseract_data <TESSERACT_DATA> "tesseract data path")
                        .default_value("tessdata")
                )
                .arg(
                    arg!(-u --default_lang <DEFAULT_LANG> "tesseract default language")
                        .default_value("eng")
                )
                .arg(
                    arg!(--llama_model_path <LLAMA_MODEL_PATH> "llama model path")
                )
        )
        .get_matches();

    let mut conf = ServerConf::default();
    if let Some(server_matches) = matches.subcommand_matches("server") {
        conf.host = server_matches.get_one::<String>("host").expect("host must input").to_owned();
        conf.port = *server_matches.get_one::<u16>("port").expect("port must input");
        conf.tesseract_data = server_matches.get_one::<String>("tesseract_data").expect("tesseract data must input").to_owned();
        conf.tesseract_default_lang = server_matches.get_one::<String>("default_lang").expect("tesseract default language must input").to_owned();
        conf.llama_model_path = server_matches.get_one::<String>("llama_model_path").map(|s| s.to_owned());

        // check default tesseract default lang traineddata exists
        let traineddata_path = format!("{}/{}.traineddata", conf.tesseract_data, conf.tesseract_default_lang);
        if !std::path::Path::new(&traineddata_path).exists() {
            eprintln!("tesseract default lang traineddata not exists: {}", traineddata_path);
            std::process::exit(1);
        }

        // create a single thread tokio runtime for llama
        let translate_req_sender = translate::run_llama_in_thread(conf.clone());

        let server_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("create tokio runtime error");

        let ret = server_rt.block_on(
            server::run_server(conf.clone(), translate_req_sender));
        if let Err(e) = ret {
            eprintln!("run server error: {}", e);
            std::process::exit(1);
        }
    }
}
