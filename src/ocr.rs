use tesseract::Tesseract;

pub fn do_ocr(tesseract_data: &str, tesseract_default_lang: &str, image_path: &str) -> String {
    let tes = Tesseract::new(
        Some(tesseract_data),
        Some(tesseract_default_lang),
    ).expect("new tesseract error");

    let mut ttes = tes.set_image(image_path).expect("set image error");
    let result_text = &ttes.get_text().expect("get text error");
    result_text.to_string()
}