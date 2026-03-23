fn request_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    prompt: &Prompt,
) -> Result<String> {
    runtime.block_on(async {
        let mut stream = client.stream(prompt).await?;
        let mut out = String::new();
        while let Some(ev) = stream.next().await {
            match ev {
                Ok(ResponseEvent::OutputTextDelta { delta, .. }) => out.push_str(&delta),
                Ok(ResponseEvent::OutputItemDone {
                    item: ResponseItem::Message { content, .. },
                    ..
                }) => {
                    for c in content {
                        if let ContentItem::OutputText { text } = c {
                            out.push_str(&text);
                        }
                    }
                }
                Ok(ResponseEvent::Completed { .. }) => break,
                Err(err) => return Err(anyhow!("model stream error: {err}")),
                _ => {}
            }
        }
        Ok(out)
    })
}

fn parse_decision(raw: &str) -> Result<(InstallDecision, Value)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            let Some(json_blob) = extract_first_json_object(raw) else {
                return Err(anyhow!("model response was not valid JSON"));
            };
            serde_json::from_str(&json_blob).context("parsing JSON from model output")?
        }
    };
    let decision: InstallDecision = serde_json::from_value(value.clone())
        .context("decoding install decision")?;
    Ok((decision, value))
}
