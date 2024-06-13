#!/bin/bash

./target/release/flameshot-ocr-server server -d /usr/share/tesseract-ocr/5/tessdata/ -u chi_sim --llama_model_path /media/msd/models/qwen1_5-7b-chat-q4_0.gguf -l 127.0.0.1 -p 8888
