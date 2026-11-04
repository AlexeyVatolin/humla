#![cfg(windows)]

#[test]
#[ignore = "requires the downloaded GigaAM fixture"]
fn gigaam_e2e_rnnt_returns_russian_punctuation_and_timestamps() {
    let directory = std::env::var("HUMLA_ASR_MODEL").expect("HUMLA_ASR_MODEL");
    let model = humla_windows::models::find_model("gigaam-v3-e2e-rnnt").unwrap();
    humla_windows::models::download(std::path::Path::new(&directory), &model.files, |_, _| {})
        .unwrap();
    let path = std::env::var("HUMLA_ASR_AUDIO").expect("HUMLA_ASR_AUDIO");
    let wave = sherpa_onnx::Wave::read(&path).unwrap();
    assert_eq!(wave.sample_rate(), 16_000);
    let engine =
        humla_windows::inference::Recognizer::load(std::path::Path::new(&directory), false, "ru")
            .unwrap();
    let result = engine.transcribe(wave.samples(), true, "ru").unwrap();
    assert!(result
        .text
        .chars()
        .any(|letter| ('А'..='я').contains(&letter)));
    assert!(result.text.chars().any(|letter| ".,?!:;".contains(letter)));
    assert!(!result.words.is_empty());
    assert!(result.words.iter().all(
        |word| word.start_ms <= word.end_ms && word.end_ms <= wave.samples().len() as u64 / 16
    ));
    assert!(result
        .words
        .windows(2)
        .all(|words| words[0].start_ms <= words[1].start_ms));
}
