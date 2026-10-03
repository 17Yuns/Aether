pub(crate) fn normalized_signature(api_format: &str) -> Option<&'static str> {
    match crate::ai_serving::normalize_api_format_alias(api_format).as_str() {
        "typesafe:systemone" | "typesafe" | "jev" => Some("typesafe:systemone"),
        _ => None,
    }
}

pub(crate) fn local_path(api_format: &str) -> Option<&'static str> {
    normalized_signature(api_format).map(|_| "/v1/systemone")
}
