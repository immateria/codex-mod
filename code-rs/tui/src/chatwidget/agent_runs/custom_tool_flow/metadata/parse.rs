use super::InvocationMetadata;
use super::super::*;
use serde_json::{Map, Value};

pub(super) fn populate_from_params(meta: &mut InvocationMetadata, params: Option<&Value>) {
    if let Some(Value::Object(map)) = params {
        populate_from_map(meta, map);
    }
}

pub(super) fn finalize(meta: &mut InvocationMetadata, _tool_name: &str) {
    if meta.label.is_none()
        && let Some(first) = meta.agent_ids.first()
    {
        meta.label = Some(first.clone());
    }
    meta.agent_ids = dedup(std::mem::take(&mut meta.agent_ids));
    meta.models = dedup(std::mem::take(&mut meta.models));
}

fn populate_from_map(meta: &mut InvocationMetadata, map: &Map<String, Value>) {
    apply_root_fields(meta, map);
    apply_agents(meta, map);
    apply_create(meta, map);
    apply_action_object(meta, map, "wait");
    apply_action_object(meta, map, "status");
    apply_action_object(meta, map, "result");
    apply_action_object(meta, map, "cancel");
    apply_list(meta, map);
}

fn apply_root_fields(meta: &mut InvocationMetadata, map: &Map<String, Value>) {
    if let Some(action) = map.get("action").and_then(Value::as_str) {
        meta.action = Some(action.to_string());
    }
    if let Some(batch) = map.get("batch_id").and_then(Value::as_str) {
        meta.batch_id = Some(batch.to_string());
    }
    if let Some(write_flag) = map.get("write").and_then(Value::as_bool) {
        meta.write = Some(write_flag);
    }
    if let Some(ro_flag) = map.get("read_only").and_then(Value::as_bool) {
        meta.read_only = Some(ro_flag);
    }
    if let Some(agent_id) = map.get("agent_id").and_then(Value::as_str) {
        meta.agent_ids.push(agent_id.to_string());
    }
    if let Some(agent_name) = map.get("agent_name").and_then(Value::as_str) {
        meta.label = Some(agent_name.to_string());
    }
    if let Some(task) = map.get("task").and_then(Value::as_str) {
        meta.task = Some(task.to_string());
    }
    if let Some(context) = map.get("context").and_then(Value::as_str) {
        meta.context = Some(context.to_string());
    }
    if let Some(plan) = map.get("plan").and_then(Value::as_array) {
        meta.plan = plan
            .iter()
            .filter_map(|step| step.as_str().map(std::string::ToString::to_string))
            .collect();
    }
    if let Some(models) = map.get("models").and_then(Value::as_array) {
        for model in models {
            if let Some(name) = model.as_str() {
                meta.models.push(name.to_string());
            }
        }
    }
}

fn apply_agents(meta: &mut InvocationMetadata, map: &Map<String, Value>) {
    let Some(agents) = map.get("agents").and_then(Value::as_array) else {
        return;
    };

    for entry in agents {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        if let Some(id) = obj.get("id").and_then(Value::as_str) {
            meta.agent_ids.push(id.to_string());
        }
        if meta.label.is_none()
            && let Some(name) = obj.get("name").and_then(Value::as_str)
        {
            meta.label = Some(name.to_string());
        }
        if let Some(model) = obj.get("model").and_then(Value::as_str) {
            meta.models.push(model.to_string());
        }
        if let Some(backend) = obj.get("backend").and_then(Value::as_str) {
            meta.models.push(backend.to_string());
        }
        if meta.write.is_none()
            && let Some(write_flag) = obj.get("write").and_then(Value::as_bool)
        {
            meta.write = Some(write_flag);
        }
        if meta.read_only.is_none()
            && let Some(ro_flag) = obj.get("read_only").and_then(Value::as_bool)
        {
            meta.read_only = Some(ro_flag);
        }
    }
}

fn apply_create(meta: &mut InvocationMetadata, map: &Map<String, Value>) {
    let Some(create) = map.get("create").and_then(Value::as_object) else {
        return;
    };

    if meta.task.is_none()
        && let Some(task) = create.get("task").and_then(Value::as_str)
    {
        meta.task = Some(task.to_string());
    }
    if let Some(name) = create.get("name").and_then(Value::as_str) {
        meta.label = Some(name.to_string());
    }
    if meta.context.is_none()
        && let Some(context) = create.get("context").and_then(Value::as_str)
    {
        meta.context = Some(context.to_string());
    }
    if meta.write.is_none()
        && let Some(write_flag) = create.get("write").and_then(Value::as_bool)
    {
        meta.write = Some(write_flag);
    }
    if meta.read_only.is_none()
        && let Some(ro_flag) = create.get("read_only").and_then(Value::as_bool)
    {
        meta.read_only = Some(ro_flag);
    }
    if meta.plan.is_empty()
        && let Some(plan) = create.get("plan").and_then(Value::as_array)
    {
        meta.plan = plan
            .iter()
            .filter_map(|step| step.as_str().map(std::string::ToString::to_string))
            .collect();
    }
}

fn apply_action_object(meta: &mut InvocationMetadata, map: &Map<String, Value>, key: &str) {
    let Some(obj) = map.get(key).and_then(Value::as_object) else {
        return;
    };
    if meta.batch_id.is_none()
        && let Some(batch) = obj.get("batch_id").and_then(Value::as_str)
    {
        meta.batch_id = Some(batch.to_string());
    }
    if let Some(agent_id) = obj.get("agent_id").and_then(Value::as_str) {
        meta.agent_ids.push(agent_id.to_string());
    }
}

fn apply_list(meta: &mut InvocationMetadata, map: &Map<String, Value>) {
    if let Some(list) = map.get("list").and_then(Value::as_object)
        && meta.batch_id.is_none()
        && let Some(batch) = list.get("batch_id").and_then(Value::as_str)
    {
        meta.batch_id = Some(batch.to_string());
    }
}
