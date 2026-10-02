use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const CLINEPASS_BASE_URL: &str = "https://api.cline.bot/api/v1";

pub fn normalize_clinepass_channel(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClinePassConfig {
    #[serde(default)]
    pub models: BTreeMap<String, ClinePassModelFilter>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClinePassModelFilter {
    #[serde(default)]
    pub only: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub available_channels: Vec<String>,
    #[serde(default)]
    pub pipeline: String,
    #[serde(default)]
    pub pinnable: Option<bool>,
    #[serde(default)]
    pub pin_reason: Option<String>,
}

pub fn normalize_clinepass_config(value: &Value) -> Result<Value, String> {
    let mut config: ClinePassConfig = serde_json::from_value(value.clone())
        .map_err(|_| "ClinePass 渠道商筛选配置格式无效".to_string())?;
    if config.models.len() > 200 {
        return Err("ClinePass 渠道商筛选最多支持 200 个模型".to_string());
    }
    for (model, rule) in &mut config.models {
        if model.trim().is_empty()
            || model != model.trim()
            || model.len() > 256
            || model.chars().any(char::is_control)
        {
            return Err("ClinePass 模型名称不能为空或包含首尾空格".to_string());
        }
        if !matches!(rule.pipeline.as_str(), "" | "planner" | "direct") {
            return Err("ClinePass 路由类型无效".to_string());
        }
        for channels in [
            &mut rule.only,
            &mut rule.exclude,
            &mut rule.available_channels,
        ] {
            if channels.len() > 100 {
                return Err("每个模型最多配置 100 个渠道商".to_string());
            }
            let mut normalized = Vec::new();
            for channel in channels.iter() {
                let channel = channel.trim();
                if channel.is_empty()
                    || channel.len() > 128
                    || channel.chars().any(char::is_control)
                {
                    return Err("ClinePass 渠道商名称无效".to_string());
                }
                let channel = normalize_clinepass_channel(channel);
                if channel.is_empty() {
                    return Err("ClinePass 渠道商名称无效".to_string());
                }
                if !normalized
                    .iter()
                    .any(|item: &String| item.eq_ignore_ascii_case(&channel))
                {
                    normalized.push(channel);
                }
            }
            *channels = normalized;
        }
        if rule.only.iter().any(|channel| {
            rule.exclude
                .iter()
                .any(|excluded| excluded.eq_ignore_ascii_case(channel))
        }) {
            return Err(format!("模型 {model} 的允许和排除渠道商不能重复"));
        }
        if rule.pinnable != Some(false) && !rule.exclude.is_empty() {
            let candidates = if rule.only.is_empty() {
                &rule.available_channels
            } else {
                &rule.only
            };
            if candidates.is_empty() {
                return Err(format!("模型 {model} 尚未探测到渠道商，不能配置排除项"));
            }
            if candidates.iter().all(|channel| {
                rule.exclude
                    .iter()
                    .any(|excluded| channel.eq_ignore_ascii_case(excluded))
            }) {
                return Err(format!("模型 {model} 的排除项不能覆盖全部渠道商"));
            }
        }
        if rule
            .pin_reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 200)
        {
            return Err("ClinePass 渠道商筛选状态说明过长".to_string());
        }
    }
    serde_json::to_value(config).map_err(|error| error.to_string())
}

pub fn apply_clinepass_channel_filter(
    body: &mut Value,
    config: Option<&Value>,
) -> Result<(), &'static str> {
    let Some(config) = config.and_then(|config| config.get("clinepass")) else {
        return Ok(());
    };
    let config: ClinePassConfig = serde_json::from_value(config.clone())
        .map_err(|_| "ClinePass channel filter configuration is invalid")?;
    let Some(rule) = body
        .get("model")
        .and_then(Value::as_str)
        .and_then(|model| config.models.get(model))
    else {
        return Ok(());
    };
    if rule.pinnable == Some(false) || (rule.only.is_empty() && rule.exclude.is_empty()) {
        return Ok(());
    }
    let candidates = if rule.only.is_empty() {
        &rule.available_channels
    } else {
        &rule.only
    };
    if candidates.is_empty() {
        return Err("ClinePass channel list is unknown; probe the model before excluding channels");
    }
    let allowed: Vec<_> = candidates
        .iter()
        .filter(|channel| {
            !rule
                .exclude
                .iter()
                .any(|excluded| excluded.eq_ignore_ascii_case(channel))
        })
        .cloned()
        .collect();
    if allowed.is_empty() {
        return Err("ClinePass channel filter excludes all channels");
    }
    let body = body
        .as_object_mut()
        .ok_or("ClinePass request body must be an object")?;
    if rule.pipeline != "direct" {
        let options = body.entry("providerOptions").or_insert_with(|| json!({}));
        let options = options
            .as_object_mut()
            .ok_or("ClinePass providerOptions must be an object")?;
        let gateway = options.entry("gateway").or_insert_with(|| json!({}));
        set_allowed_channels(gateway, &allowed)?;
    }
    if rule.pipeline != "planner" {
        let provider = body.entry("provider").or_insert_with(|| json!({}));
        set_allowed_channels(provider, &allowed)?;
    }
    Ok(())
}

fn set_allowed_channels(value: &mut Value, allowed: &[String]) -> Result<(), &'static str> {
    let object: &mut Map<String, Value> = value
        .as_object_mut()
        .ok_or("ClinePass channel preferences must be an object")?;
    object.insert("only".to_string(), json!(allowed));
    object.insert("order".to_string(), json!(allowed));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clinepass_config_normalizes_channels_and_rejects_unusable_exclusions() {
        let config = normalize_clinepass_config(
            &json!({"models":{"test":{"only":["BaseTen","BASEten","Z.AI"]}}}),
        )
        .unwrap();
        assert_eq!(config["models"]["test"]["only"], json!(["baseten", "z-ai"]));
        assert!(
            normalize_clinepass_config(&json!({"models":{"test":{"exclude":["baseten"]}}}))
                .is_err()
        );
        assert!(normalize_clinepass_config(
            &json!({"models":{"test":{"exclude":["baseten"],"available_channels":["BaseTen"]}}})
        )
        .is_err());
    }

    #[test]
    fn filter_overrides_client_preferences_on_both_pipelines() {
        let config = json!({"clinepass":{"models":{"cline-pass/test":{"only":["baseten"],"available_channels":["deepseek","baseten"]}}}});
        let mut body = json!({"model":"cline-pass/test","provider":{"only":["deepseek"]},"providerOptions":{"gateway":{"only":["deepseek"]}}});
        apply_clinepass_channel_filter(&mut body, Some(&config)).unwrap();
        assert_eq!(body["provider"]["only"], json!(["baseten"]));
        assert_eq!(
            body["providerOptions"]["gateway"]["only"],
            json!(["baseten"])
        );
    }

    #[test]
    fn exclusions_require_known_channels_and_cannot_remove_them_all() {
        let mut body = json!({"model":"test"});
        assert!(apply_clinepass_channel_filter(
            &mut body,
            Some(&json!({"clinepass":{"models":{"test":{"exclude":["a"]}}}}))
        )
        .is_err());
        assert!(apply_clinepass_channel_filter(&mut body, Some(&json!({"clinepass":{"models":{"test":{"exclude":["a"],"available_channels":["a"]}}}}))).is_err());
        apply_clinepass_channel_filter(&mut body, Some(&json!({"clinepass":{"models":{"test":{"exclude":["a"],"available_channels":["a","b"]}}}}))).unwrap();
        assert_eq!(body["provider"]["only"], json!(["b"]));
    }
}
