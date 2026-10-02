use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct ClinePassResponseFacts {
    pub channel: Option<String>,
    pub cache_read_tokens: Option<i64>,
    // Cache misses are ordinary input tokens, not evidence of a cache write.
    pub cache_miss_tokens: Option<i64>,
}

impl ClinePassResponseFacts {
    pub fn merge(&mut self, incoming: Self) {
        if incoming.channel.is_some() {
            self.channel = incoming.channel;
        }
        if incoming.cache_read_tokens.is_some() {
            self.cache_read_tokens = incoming.cache_read_tokens;
        }
        if incoming.cache_miss_tokens.is_some() {
            self.cache_miss_tokens = incoming.cache_miss_tokens;
        }
    }
}

pub fn parse_clinepass_response(value: &Value) -> ClinePassResponseFacts {
    if let Some(chunks) = value.get("chunks").and_then(Value::as_array) {
        let mut facts = ClinePassResponseFacts::default();
        for chunk in chunks {
            facts.merge(parse_clinepass_chunk(chunk));
        }
        return facts;
    }
    parse_clinepass_chunk(value)
}

fn parse_clinepass_chunk(value: &Value) -> ClinePassResponseFacts {
    let metadata = value
        .get("provider_metadata")
        .or_else(|| {
            value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| {
                    choices.iter().find_map(|choice| {
                        choice
                            .pointer("/message/provider_metadata")
                            .or_else(|| choice.pointer("/delta/provider_metadata"))
                    })
                })
        })
        .or_else(|| value.pointer("/response/provider_metadata"));
    let routing = metadata.and_then(|metadata| metadata.pointer("/gateway/routing"));
    let attempted_channel = routing
        .and_then(|routing| routing.get("modelAttempts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .rev()
        .find_map(|model| {
            model
                .get("providerAttempts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .rev()
                .find_map(|attempt| {
                    (attempt.get("success").and_then(Value::as_bool) == Some(true))
                        .then(|| attempt.get("provider").and_then(Value::as_str))
                        .flatten()
                })
        });
    let channel = attempted_channel
        .or_else(|| {
            routing
                .and_then(|routing| routing.get("finalProvider"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            routing
                .and_then(|routing| routing.get("resolvedProvider"))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("provider").and_then(Value::as_str))
        .map(str::trim)
        .filter(|channel| {
            !channel.is_empty() && channel.len() <= 128 && !channel.chars().any(char::is_control)
        })
        .map(ToOwned::to_owned);
    let counters = metadata.and_then(Value::as_object).and_then(|metadata| {
        let matching = channel
            .as_ref()
            .and_then(|channel| {
                metadata
                    .iter()
                    .find(|(name, _)| provider_key(name) == provider_key(channel))
            })
            .map(|(_, value)| value);
        matching.filter(|value| has_counters(value)).or_else(|| {
            metadata
                .iter()
                .filter(|(name, _)| name.as_str() != "gateway")
                .map(|(_, value)| value)
                .find(|value| has_counters(value))
        })
    });
    ClinePassResponseFacts {
        channel,
        cache_read_tokens: counters.and_then(|value| count(value.get("promptCacheHitTokens"))),
        cache_miss_tokens: counters.and_then(|value| count(value.get("promptCacheMissTokens"))),
    }
}

fn has_counters(value: &Value) -> bool {
    count(value.get("promptCacheHitTokens")).is_some()
        || count(value.get("promptCacheMissTokens")).is_some()
}

fn count(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64).filter(|count| *count >= 0)
}

fn provider_key(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clinepass_metadata_survives_sync_conversion_and_sync_to_stream_auditing() {
        let response = json!({"id":"chat-1","object":"chat.completion","model":"test","provider":"baseten",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hi","provider_metadata":{"baseten":{"promptCacheHitTokens":70,"promptCacheMissTokens":30}}},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":100,"completion_tokens":2,"total_tokens":102,"prompt_tokens_details":{"cache_write_tokens":12}}
        });
        let context = json!({"provider_type":"clinepass","provider_api_format":"openai:chat","client_api_format":"claude:messages","model":"test"});
        let product =
            crate::formats::shared::sync_products::maybe_build_standard_cross_format_sync_product(
                "claude_chat_sync_finalize",
                "openai:chat",
                "claude:messages",
                &context,
                response.clone(),
            )
            .unwrap();
        assert_eq!(product.client_body_json["content"][0]["text"], "hi");
        assert_eq!(
            parse_clinepass_response(&product.provider_body_json)
                .channel
                .as_deref(),
            Some("baseten")
        );
        let bridge =
            crate::formats::shared::sync_to_stream::maybe_bridge_standard_sync_json_to_stream(
                &response,
                "openai:chat",
                "openai:responses",
                Some(&context),
            )
            .unwrap()
            .unwrap();
        let summary = bridge.terminal_summary.unwrap();
        assert_eq!(summary.provider_channel.as_deref(), Some("baseten"));
        assert_eq!(summary.standardized_usage.unwrap().cache_read_tokens, 70);
    }

    #[test]
    fn clinepass_captured_stream_keeps_metadata_from_earlier_chunks() {
        let response = json!({"chunks":[
            {"choices":[{"delta":{"provider_metadata":{"gateway":{"routing":{"finalProvider":"baseten"}},"baseten":{"promptCacheHitTokens":70,"promptCacheMissTokens":30}}}}]},
            {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":2}}
        ]});
        let facts = parse_clinepass_response(&response);
        assert_eq!(facts.channel.as_deref(), Some("baseten"));
        assert_eq!(facts.cache_read_tokens, Some(70));
    }

    #[test]
    fn last_successful_attempt_is_the_actual_channel_and_selects_its_cache_counters() {
        let response = json!({"choices":[{"delta":{"provider_metadata":{
            "gateway":{"routing":{"resolvedProvider":"deepseek","finalProvider":"deepseek","modelAttempts":[{"providerAttempts":[{"provider":"deepseek","success":false},{"provider":"BaseTen","success":true}]}]}},
            "deepseek":{"promptCacheHitTokens":999,"promptCacheMissTokens":1},
            "baseten":{"promptCacheHitTokens":12,"promptCacheMissTokens":34}
        }}}]});
        let facts = parse_clinepass_response(&response);
        assert_eq!(facts.channel.as_deref(), Some("BaseTen"));
        assert_eq!(facts.cache_read_tokens, Some(12));
        assert_eq!(facts.cache_miss_tokens, Some(34));
    }
}
