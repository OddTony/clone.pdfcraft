use super::*;

#[test]
fn key_is_never_debugged() {
    let c = ProviderConfig { base_url: "https://openrouter.ai/api/v1".into(), api_key: "sk-or-secret".into(), model: "x".into() };
    assert!(!format!("{c:?}").contains("sk-or-secret"));
}

#[test]
fn bad_configs_are_refused_before_network() {
    let bad_url = ProviderConfig { base_url: "http://insecure".into(), api_key: "k".into(), model: "m".into() };
    assert!(matches!(chat(&bad_url, &[ChatMessage::user("hi")]), Err(AiError::Invalid(_))));
    let no_key = ProviderConfig { base_url: "https://openrouter.ai/api/v1".into(), api_key: "".into(), model: "m".into() };
    assert!(matches!(chat(&no_key, &[ChatMessage::user("hi")]), Err(AiError::Invalid(_))));
    let no_messages = ProviderConfig { base_url: "https://openrouter.ai/api/v1".into(), api_key: "k".into(), model: "m".into() };
    assert!(matches!(chat(&no_messages, &[]), Err(AiError::Invalid(_))));
}

#[test]
fn answers_parse_openai_shape() {
    let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hello"}}]}"#;
    assert_eq!(parse_answer(body).expect("parse"), "Hello");
    assert!(parse_answer(r#"{"error":"nope"}"#).is_err());
    assert!(parse_answer("not json").is_err());
}

#[test]
fn oversized_prompts_are_refused() {
    let c = ProviderConfig { base_url: "https://openrouter.ai/api/v1".into(), api_key: "k".into(), model: "m".into() };
    let big = "x".repeat(MAX_PROMPT_CHARS + 1);
    assert!(matches!(chat(&c, &[ChatMessage::user(big)]), Err(AiError::Invalid(_))));
}
