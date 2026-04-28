use super::*;
use pretty_assertions::assert_eq;

fn snapshot_with_nudge(
    key: &str,
    threshold: u8,
    copy_variant: UsageLimitNudgeCopyVariant,
) -> RateLimitSnapshot {
    RateLimitSnapshot {
        current_usage_limit_nudge: Some(Some(UsageLimitNudgePayload {
            key: key.to_string(),
            threshold,
            copy_variant,
        })),
        ..snapshot(f64::from(threshold))
    }
}

fn inactive_snapshot() -> RateLimitSnapshot {
    RateLimitSnapshot {
        current_usage_limit_nudge: Some(None),
        ..snapshot(/*percent*/ 75.0)
    }
}

fn next_open_url_event(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> Option<String> {
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::OpenUrlInBrowser { url } = event {
            return Some(url);
        }
    }
    None
}

#[tokio::test]
async fn proactive_usage_prompt_renders_backend_copy_variants() {
    let mut rendered_cases = Vec::new();

    for (threshold, copy_variant) in [
        (75, UsageLimitNudgeCopyVariant::AddCredits),
        (75, UsageLimitNudgeCopyVariant::Upgrade),
        (90, UsageLimitNudgeCopyVariant::AddCredits),
        (90, UsageLimitNudgeCopyVariant::Upgrade),
    ] {
        let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
        chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
            &format!("{threshold}-{copy_variant:?}"),
            threshold,
            copy_variant,
        )));
        chat.maybe_show_pending_rate_limit_prompt();
        rendered_cases.push(render_bottom_popup(&chat, /*width*/ 88));
    }

    assert_chatwidget_snapshot!(
        "proactive_usage_prompt_variants",
        rendered_cases.join("\n---\n")
    );
}

#[tokio::test]
async fn proactive_usage_prompt_dedupes_same_key() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let nudge = snapshot_with_nudge(
        "same-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    );

    chat.on_rate_limit_snapshot(Some(nudge.clone()));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());

    chat.on_rate_limit_snapshot(Some(nudge));
    assert!(!chat.maybe_show_pending_current_usage_limit_nudge_prompt());
}

#[tokio::test]
async fn proactive_usage_prompt_shows_changed_key_again() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "first-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "second-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());
}

#[tokio::test]
async fn proactive_usage_prompt_explicit_null_clears_suppression() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "repeatable-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());

    chat.on_rate_limit_snapshot(Some(inactive_snapshot()));
    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "repeatable-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());
}

#[tokio::test]
async fn proactive_usage_prompt_unknown_state_preserves_suppression() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "repeatable-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(chat.maybe_show_pending_current_usage_limit_nudge_prompt());

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 75.0)));
    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "repeatable-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(!chat.maybe_show_pending_current_usage_limit_nudge_prompt());
}

#[tokio::test]
async fn proactive_usage_prompt_yes_opens_browser_destination() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "browser-key",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::Upgrade,
    )));
    chat.maybe_show_pending_rate_limit_prompt();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert_eq!(
        next_open_url_event(&mut rx),
        Some(CURRENT_USAGE_LIMIT_NUDGE_URL.to_string())
    );
}

#[tokio::test]
async fn proactive_usage_prompt_yes_opens_workspace_owner_billing_destination() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.plan_type = Some(PlanType::SelfServeBusinessUsageBased);

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "workspace-browser-key",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    chat.maybe_show_pending_rate_limit_prompt();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert_eq!(
        next_open_url_event(&mut rx),
        Some(WORKSPACE_OWNER_USAGE_LIMIT_NUDGE_URL.to_string())
    );
}

#[tokio::test]
async fn proactive_usage_prompt_no_dismisses_without_opening_browser() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "dismiss-key",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::Upgrade,
    )));
    chat.maybe_show_pending_rate_limit_prompt();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

    assert_eq!(next_open_url_event(&mut rx), None);
}

#[tokio::test]
async fn proactive_usage_prompt_waits_for_between_turn_hook() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "between-turn-key",
        /*threshold*/ 75,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    let popup = render_bottom_popup(&chat, /*width*/ 88);
    assert!(!popup.contains("Approaching usage limit"), "popup: {popup}");

    chat.maybe_show_pending_rate_limit_prompt();
    assert!(render_bottom_popup(&chat, /*width*/ 88).contains("Approaching usage limit"));
}

#[tokio::test]
async fn proactive_usage_prompt_flag_disabled_skips_prompt_and_keeps_passive_warning() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::CurrentUsageLimitNudge, /*enabled*/ false);

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "disabled-key",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(!chat.maybe_show_pending_current_usage_limit_nudge_prompt());
    let popup = render_bottom_popup(&chat, /*width*/ 88);
    assert!(!popup.contains("Approaching usage limit"), "popup: {popup}");

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<String>();
    assert!(
        rendered.contains("less than 10% of your 1h limit left"),
        "rendered: {rendered}"
    );
}

#[tokio::test]
async fn proactive_usage_prompt_suppresses_only_duplicate_passive_warning() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "threshold-90",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(drain_insert_history(&mut rx).is_empty());

    chat.on_rate_limit_snapshot(Some(RateLimitSnapshot {
        current_usage_limit_nudge: None,
        ..snapshot(/*percent*/ 95.0)
    }));
    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<String>();
    assert!(
        rendered.contains("less than 5% of your 1h limit left"),
        "rendered: {rendered}"
    );
}

#[tokio::test]
async fn proactive_usage_prompt_suppresses_later_rate_limit_switch_prompt() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.on_rate_limit_snapshot(Some(snapshot_with_nudge(
        "threshold-90",
        /*threshold*/ 90,
        UsageLimitNudgeCopyVariant::AddCredits,
    )));
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Idle
    ));

    chat.maybe_show_pending_rate_limit_prompt();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    chat.maybe_show_pending_rate_limit_prompt();

    let popup = render_bottom_popup(&chat, /*width*/ 88);
    assert!(!popup.contains("Approaching rate limits"), "popup: {popup}");
    assert!(matches!(
        chat.rate_limit_switch_prompt,
        RateLimitSwitchPromptState::Idle
    ));
}
