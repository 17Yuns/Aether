//! TypeSafe's native System One protocol. Typed decisions have no chat conversion.

use serde_json::Value;

pub const SYSTEMONE_API_FORMAT: &str = "typesafe:systemone";
pub const SYSTEMONE_SYNC_PLAN_KIND: &str = "typesafe_systemone_sync";
pub const SYSTEMONE_SYNC_SUCCESS_REPORT_KIND: &str = "typesafe_systemone_sync_success";
pub const SYSTEMONE_SYNC_FINALIZE_REPORT_KIND: &str = "typesafe_systemone_sync_finalize";
pub const SYSTEMONE_SYNC_ERROR_REPORT_KIND: &str = "typesafe_systemone_sync_error";

fn is_text_structure(value: &Value) -> bool {
    value.is_string() || value.is_object() || value.is_array()
}

/// Validate native fields before selecting a provider or consuming its quota.
pub fn validate_systemone_request(body: &Value) -> Result<(), &'static str> {
    let object = body
        .as_object()
        .ok_or("System One request must be a JSON object")?;
    if object
        .get("model")
        .and_then(Value::as_str)
        .is_none_or(|model| model.trim().is_empty())
    {
        return Err("System One request model is required");
    }
    if object
        .get("stream")
        .is_some_and(|stream| stream != &Value::Bool(false))
    {
        return Err("System One requests do not support streaming");
    }
    if !object.get("state").is_some_and(is_text_structure) {
        return Err("System One state must be a string, object or array");
    }
    let questions = object
        .get("questions")
        .and_then(Value::as_object)
        .filter(|questions| !questions.is_empty())
        .ok_or("System One questions must be a non-empty object")?;
    for question in questions.values() {
        let question = question
            .as_object()
            .ok_or("System One questions must contain question objects")?;
        if !question.get("instructions").is_some_and(is_text_structure) {
            return Err("System One question instructions must be a string, object or array");
        }
        match question.get("type").and_then(Value::as_str) {
            Some("noul") => {
                if let Some(criteria) = question.get("criteria") {
                    let criteria = criteria
                        .as_object()
                        .ok_or("Noul criteria must be an object")?;
                    if criteria.iter().any(|(key, value)| {
                        !matches!(key.as_str(), "true" | "false") || !is_text_structure(value)
                    }) {
                        return Err("Noul criteria must describe true or false using text or structured data");
                    }
                }
            }
            Some("choice") => {
                let criteria = question
                    .get("criteria")
                    .and_then(Value::as_object)
                    .ok_or("Choice criteria must be an object")?;
                if criteria.is_empty()
                    || criteria.len() > 255
                    || criteria
                        .values()
                        .any(|value| !value.is_null() && !is_text_structure(value))
                {
                    return Err(
                        "Choice criteria must contain 1 to 255 text, structured or null options",
                    );
                }
            }
            Some("score") => {
                let criteria = question
                    .get("criteria")
                    .and_then(Value::as_array)
                    .ok_or("Score criteria must be an array")?;
                if !(2..=10).contains(&criteria.len())
                    || criteria.iter().any(|value| !is_text_structure(value))
                {
                    return Err("Score criteria must contain 2 to 10 text or structured levels");
                }
            }
            _ => return Err("System One question type must be noul, choice or score"),
        }
    }
    Ok(())
}

pub fn systemone_test_request(model: &str) -> Value {
    serde_json::json!({"model":model,"state":"Hello","questions":{
        "is_greeting":{"type":"noul","instructions":"Is this a greeting?"}
    }})
}

/// Preserve the provider's answer objects and model version after validating the envelope.
pub fn validate_systemone_response(body: &Value) -> Result<(), &'static str> {
    if body
        .get("model")
        .and_then(Value::as_str)
        .is_none_or(|model| model.is_empty())
        || body
            .get("answers")
            .and_then(Value::as_object)
            .is_none_or(|answers| answers.is_empty())
        || body
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64)
            .is_none()
        || body
            .pointer("/usage/output_tokens")
            .and_then(Value::as_u64)
            .is_none()
    {
        return Err("Invalid System One response: expected model, answers and token usage");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_native_structured_decisions_and_validates_all_three_question_types() {
        let body = json!({"model":"jev-latest","state":{"text":"Hello"},"questions":{
            "hello":{"type":"noul","instructions":["Is this a greeting?"]},
            "category":{"type":"choice","instructions":"Choose category","criteria":{"greeting":null,"other":{"description":"Other"}}},
            "score":{"type":"score","instructions":"Rate urgency","criteria":["Low","High"]}
        }});
        assert_eq!(validate_systemone_request(&body), Ok(()));
        assert_eq!(
            validate_systemone_request(&systemone_test_request("jev-latest")),
            Ok(())
        );
    }

    #[test]
    fn rejects_chat_streaming_and_invalid_question_contracts() {
        let mut body = systemone_test_request("jev-latest");
        body["stream"] = json!(true);
        assert!(validate_systemone_request(&body).is_err());
        body.as_object_mut().unwrap().remove("stream");
        body["questions"]["is_greeting"]["type"] = json!("chat");
        assert!(validate_systemone_request(&body).is_err());
        assert!(validate_systemone_request(&json!({"model":"jev-latest","messages":[]})).is_err());
        assert!(validate_systemone_request(&json!({"model":"jev-latest","state":"Hello","questions":{"score":{"type":"score","instructions":"Score","criteria":["Only one"]}}})).is_err());
    }
    #[test]
    fn systemone_question_limits_match_the_documented_contract() {
        for (count, valid) in [(0, false), (1, true), (255, true), (256, false)] {
            let mut body = systemone_test_request("jev-latest");
            let options = (0..count)
                .map(|i| (format!("option-{i}"), Value::Null))
                .collect::<serde_json::Map<_, _>>();
            body["questions"]["is_greeting"] =
                json!({"type":"choice","instructions":"Select", "criteria":options});
            assert_eq!(
                validate_systemone_request(&body).is_ok(),
                valid,
                "choice count {count}"
            );
        }
        for (count, valid) in [(1, false), (2, true), (10, true), (11, false)] {
            let mut body = systemone_test_request("jev-latest");
            body["questions"]["is_greeting"] = json!({"type":"score","instructions":"Rate", "criteria":vec![json!({"description":"Level"});count]});
            assert_eq!(
                validate_systemone_request(&body).is_ok(),
                valid,
                "score count {count}"
            );
        }
        for state in [
            json!("text"),
            json!({"amount":42}),
            json!(["text",{"nested":true}]),
        ] {
            let mut body = systemone_test_request("jev-latest");
            body["state"] = state;
            assert_eq!(validate_systemone_request(&body), Ok(()));
        }
    }

    #[test]
    fn systemone_protocol_is_isolated_from_chat_and_streaming() {
        use crate::formats::matrix::{
            request_candidate_api_format_preference, request_candidate_api_formats,
        };
        assert_eq!(
            request_candidate_api_formats(SYSTEMONE_API_FORMAT, false),
            vec![SYSTEMONE_API_FORMAT]
        );
        assert!(request_candidate_api_formats(SYSTEMONE_API_FORMAT, true).is_empty());
        assert_eq!(
            request_candidate_api_format_preference(SYSTEMONE_API_FORMAT, "openai:chat"),
            None
        );
        assert_eq!(
            request_candidate_api_format_preference("openai:chat", SYSTEMONE_API_FORMAT),
            None
        );
        assert_eq!(
            crate::normalize_api_format_alias("JEV:SYSTEMONE"),
            SYSTEMONE_API_FORMAT
        );
        assert!(!crate::api_format_permission_covers(
            "openai:chat",
            SYSTEMONE_API_FORMAT
        ));
    }

    #[test]
    fn systemone_finalization_preserves_answers_version_and_usage_for_json_and_bytes() {
        use crate::formats::shared::sync_products::{
            maybe_build_standard_sync_finalize_product_from_normalized_payload,
            StandardSyncFinalizeNormalizedProduct,
        };
        use base64::Engine as _;
        let body = json!({"model":"jev-1.13.0","answers":{
            "flag":{"type":"noul","noul":0.95},
            "department":{"type":"choice","choice":"billing","probabilities":{"billing":0.88,"support":0.12},"confidence":0.81},
            "rating":{"type":"score","score":1.05,"legend":{"0":"Calm","1":"Frustrated","2":"Angry"},"probabilities":{"0":0,"1":0.95,"2":0.05},"confidence":0.92}
        },"usage":{"input_tokens":304,"output_tokens":18},"future_field":{"preserved":true}});
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&body).unwrap());
        for (json_body, bytes) in [(Some(&body), None), (None, Some(encoded.as_str()))] {
            let product = maybe_build_standard_sync_finalize_product_from_normalized_payload(
                SYSTEMONE_SYNC_FINALIZE_REPORT_KIND,
                200,
                None,
                json_body,
                bytes,
            )
            .unwrap()
            .unwrap();
            let StandardSyncFinalizeNormalizedProduct::SuccessBody(actual) = product else {
                panic!("expected native success")
            };
            assert_eq!(actual, body);
        }
        assert!(
            maybe_build_standard_sync_finalize_product_from_normalized_payload(
                SYSTEMONE_SYNC_FINALIZE_REPORT_KIND,
                200,
                None,
                Some(&json!({"choices":[]})),
                None
            )
            .is_err()
        );
    }
}
