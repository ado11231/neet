use serde_json::Value;

use super::Provider;

pub(super) fn render(provider: Provider, bytes: &[u8]) -> String {
    let mut messages = Vec::new();
    let mut malformed = 0;
    if provider == Provider::Gemini {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(value) => {
                if let Some(rows) = value["messages"].as_array() {
                    for row in rows {
                        message(row, &mut messages);
                    }
                }
            }
            Err(_) => malformed += 1,
        }
    } else {
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            match serde_json::from_slice::<Value>(line) {
                Ok(value) => match provider {
                    Provider::Claude => message(&value["message"], &mut messages),
                    Provider::Codex
                        if value["type"] == "response_item"
                            && value["payload"]["type"] == "message" =>
                    {
                        message(&value["payload"], &mut messages);
                    }
                    _ => {}
                },
                Err(_) => malformed += 1,
            }
        }
    }
    if messages.is_empty() {
        messages.push(
            "No supported conversation messages in this preview. Use o to reveal the file.".into(),
        );
    }
    if malformed > 0 {
        messages.push(format!(
            "[{malformed} incomplete or invalid records skipped.]"
        ));
    }
    messages.join("\n\n")
}

fn message(value: &Value, messages: &mut Vec<String>) {
    let role = value["role"]
        .as_str()
        .or_else(|| value["type"].as_str())
        .unwrap_or("");
    let label = match role {
        "user" => "You",
        "assistant" | "gemini" => "Assistant",
        _ => return,
    };
    let text = content(&value["content"]);
    if !text.is_empty() {
        messages.push(format!("{label}\n{text}"));
    }
}

pub(super) fn content(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    value
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    let kind = part["type"].as_str().unwrap_or("text");
                    matches!(kind, "text" | "input_text" | "output_text")
                        .then(|| part["text"].as_str())
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
