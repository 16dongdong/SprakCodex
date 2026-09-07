use codexmanager_core::storage::{ManagedModelV2, ModelFastPolicyV2};
use serde_json::Value;

pub(crate) const FAST_REQUEST_BLOCKED: &str = "fast_request_blocked";

pub(crate) fn apply(
    body: Vec<u8>,
    model: &ManagedModelV2,
    client_service_tier: Option<&str>,
) -> Result<(Vec<u8>, bool), &'static str> {
    let policy = model.fast_policy;
    if policy == ModelFastPolicyV2::Block && is_fast_request_tier(client_service_tier) {
        return Err(FAST_REQUEST_BLOCKED);
    }

    let Ok(mut payload) = serde_json::from_slice::<Value>(&body) else {
        return Ok((body, false));
    };
    let Some(object) = payload.as_object_mut() else {
        return Ok((body, false));
    };
    let mut changed = match policy {
        ModelFastPolicyV2::Filter => object.remove("service_tier").is_some(),
        ModelFastPolicyV2::Force => {
            object.insert(
                "service_tier".to_string(),
                Value::String("priority".to_string()),
            );
            true
        }
        ModelFastPolicyV2::Passthrough | ModelFastPolicyV2::Block => false,
    };
    if object
        .get("service_tier")
        .and_then(Value::as_str)
        .is_some_and(|tier| !model_advertises_service_tier(model, tier).unwrap_or(true))
    {
        object.remove("service_tier");
        changed = true;
    }
    if !changed {
        return Ok((body, false));
    }
    Ok((serde_json::to_vec(&payload).unwrap_or(body), true))
}

fn is_fast_request_tier(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "fast" | "priority" | "ultrafast"
        )
    })
}

fn model_advertises_service_tier(model: &ManagedModelV2, requested: &str) -> Option<bool> {
    let requested = requested.trim();
    if matches!(
        requested.to_ascii_lowercase().as_str(),
        "" | "auto" | "default" | "standard"
    ) {
        // The catalog lists opt-in tiers only. Explicit Standard is always valid and is
        // normalized by the transport layer (the Codex backend omits it on the wire).
        return Some(true);
    }
    let advertised = model
        .capabilities
        .get("service_tiers")
        .or_else(|| model.capabilities.get("serviceTiers"))?
        .as_array()?;
    if advertised.is_empty() {
        // Bundled catalog entries use an empty list to mean that no opt-in tier is
        // supported. Custom models historically inherit an empty list when their
        // upstream metadata is unknown, so keep their passthrough compatibility.
        return (model.origin == "builtin").then_some(false);
    }
    let requested =
        if requested.eq_ignore_ascii_case("fast") || requested.eq_ignore_ascii_case("priority") {
            "priority"
        } else {
            requested
        };
    Some(advertised.iter().filter_map(Value::as_str).any(|tier| {
        tier.eq_ignore_ascii_case(requested)
            || (requested.eq_ignore_ascii_case("priority") && tier.eq_ignore_ascii_case("fast"))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model(policy: ModelFastPolicyV2, service_tiers: Option<&[&str]>) -> ManagedModelV2 {
        ManagedModelV2 {
            fast_policy: policy,
            capabilities: service_tiers
                .map(|tiers| serde_json::json!({ "service_tiers": tiers }))
                .unwrap_or_else(|| serde_json::json!({})),
            ..Default::default()
        }
    }

    fn service_tier(body: &[u8]) -> Option<String> {
        serde_json::from_slice::<Value>(body)
            .ok()?
            .get("service_tier")?
            .as_str()
            .map(str::to_string)
    }

    #[test]
    fn passthrough_preserves_service_tier() {
        let model = test_model(
            ModelFastPolicyV2::Passthrough,
            Some(&["priority", "ultrafast"]),
        );
        let body = br#"{"service_tier":"ultrafast"}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("ultrafast")).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("ultrafast"));
    }

    #[test]
    fn passthrough_omits_a_tier_not_advertised_for_the_model() {
        let model = test_model(ModelFastPolicyV2::Passthrough, Some(&["priority"]));
        let body = br#"{"service_tier":"ultrafast"}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("ultrafast")).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body), None);
    }

    #[test]
    fn passthrough_keeps_tiers_for_models_without_advertised_metadata() {
        let model = test_model(ModelFastPolicyV2::Passthrough, None);
        let body = br#"{"service_tier":"ultrafast"}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("ultrafast")).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("ultrafast"));
    }

    #[test]
    fn passthrough_keeps_tiers_for_empty_advertised_metadata() {
        let mut model = test_model(ModelFastPolicyV2::Passthrough, Some(&[]));
        model.origin = "custom".to_string();
        let body = br#"{"service_tier":"priority"}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("priority")).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("priority"));
    }

    #[test]
    fn passthrough_omits_tiers_for_builtin_models_with_an_empty_catalog_list() {
        let mut model = test_model(ModelFastPolicyV2::Passthrough, Some(&[]));
        model.origin = "builtin".to_string();
        let body = br#"{"service_tier":"flex"}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("flex")).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body), None);
    }

    #[test]
    fn explicit_standard_does_not_require_a_catalog_service_tier() {
        let model = test_model(ModelFastPolicyV2::Passthrough, Some(&["priority"]));
        for tier in ["default", "standard"] {
            let body = serde_json::to_vec(&serde_json::json!({ "service_tier": tier })).unwrap();
            let (body, applied) = apply(body, &model, Some(tier)).unwrap();
            assert!(!applied);
            assert_eq!(service_tier(&body).as_deref(), Some(tier));
        }
    }

    #[test]
    fn filter_removes_service_tier() {
        let model = test_model(ModelFastPolicyV2::Filter, Some(&["priority"]));
        let body = br#"{"service_tier":"fast","input":[]}"#.to_vec();
        let (body, applied) = apply(body, &model, Some("fast")).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body), None);
    }

    #[test]
    fn force_sets_priority() {
        let model = test_model(ModelFastPolicyV2::Force, Some(&["priority"]));
        let body = br#"{"input":[]}"#.to_vec();
        let (body, applied) = apply(body, &model, None).unwrap();
        assert!(applied);
        assert_eq!(service_tier(&body).as_deref(), Some("priority"));
    }

    #[test]
    fn block_rejects_client_requested_accelerated_tiers() {
        let model = test_model(ModelFastPolicyV2::Block, None);
        for tier in ["fast", "priority", " FAST ", "ultrafast"] {
            let body = serde_json::to_vec(&serde_json::json!({ "service_tier": tier })).unwrap();
            assert_eq!(
                apply(body, &model, Some(tier)),
                Err(FAST_REQUEST_BLOCKED),
                "tier {tier} must be blocked"
            );
        }

        for tier in ["auto", "default", "standard", "flex", "invalid"] {
            let body = serde_json::to_vec(&serde_json::json!({ "service_tier": tier })).unwrap();
            let (body, applied) =
                apply(body, &model, Some(tier)).expect("non-accelerated service tier allowed");
            assert!(!applied);
            assert_eq!(service_tier(&body).as_deref(), Some(tier));
        }

        let model = test_model(ModelFastPolicyV2::Block, Some(&["priority", "ultrafast"]));
        let body = br#"{"service_tier":"ultrafast"}"#.to_vec();
        let (body, applied) = apply(body, &model, None).unwrap();
        assert!(!applied);
        assert_eq!(service_tier(&body).as_deref(), Some("ultrafast"));
    }
}
