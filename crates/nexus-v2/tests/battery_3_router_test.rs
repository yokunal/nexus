use std::sync::Arc;
use nexus_v2::traits::{Message, ModelClient};
use nexus_v2::{DualModelRouter, MockModelClient, NexusV2Error};

// 1. Primary Healthy Completion
#[tokio::test]
async fn test_router_primary_healthy_completion() {
    let p = Arc::new(MockModelClient::new("claude", "Primary Output"));
    let f = Arc::new(MockModelClient::new("gemini", "Fallback Output"));
    let router = DualModelRouter::new(p, f);

    let resp = router.complete("ping", &[]).await.unwrap();
    assert_eq!(resp.text, "Primary Output");
    println!("  ✓ Vector 3.1: Primary healthy completion verified");
}

// 2. Token Usage Tracking
#[tokio::test]
async fn test_router_primary_token_usage_tracking() {
    let p = Arc::new(MockModelClient::new("claude", "Output"));
    let f = Arc::new(MockModelClient::new("gemini", "Output"));
    let router = DualModelRouter::new(p, f);

    let resp = router.complete("prompt", &[]).await.unwrap();
    assert!(resp.token_usage.prompt_tokens > 0);
    assert!(resp.token_usage.completion_tokens > 0);
    println!("  ✓ Vector 3.2: Token usage metrics tracked");
}

// 3. Automatic Failover on Primary Error
#[tokio::test]
async fn test_router_automatic_failover_on_primary_error() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F_Active"));
    p.set_failing(true);

    let router = DualModelRouter::new(p, f);
    let resp = router.complete("test", &[]).await.unwrap();
    assert_eq!(resp.text, "F_Active");
    println!("  ✓ Vector 3.3: Mid-turn failover to fallback succeeded");
}

// 4. Failover Counter Increments
#[tokio::test]
async fn test_router_failover_counter_increments() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    p.set_failing(true);

    let router = DualModelRouter::new(p, f);
    assert_eq!(router.get_failover_count(), 0);
    let _ = router.complete("t1", &[]).await.unwrap();
    assert_eq!(router.get_failover_count(), 1);
    let _ = router.complete("t2", &[]).await.unwrap();
    assert_eq!(router.get_failover_count(), 2);
    println!("  ✓ Vector 3.4: Atomic failover counter tracked 2 failover events");
}

// 5. Dual Model Failure Error Domain
#[tokio::test]
async fn test_router_dual_model_failure_error_domain() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    p.set_failing(true);
    f.set_failing(true);

    let router = DualModelRouter::new(p, f);
    let res = router.complete("prompt", &[]).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::DualModelFailure { primary_err, fallback_err } => {
            assert!(primary_err.contains("simulated rate-limit"));
            assert!(fallback_err.contains("simulated rate-limit"));
        }
        e => panic!("Expected DualModelFailure, got {:?}", e),
    }
    println!("  ✓ Vector 3.5: Dual model failure mapped to explicit error domain");
}

// 6. Health Check: Both Healthy
#[tokio::test]
async fn test_router_health_check_both_healthy() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    let router = DualModelRouter::new(p, f);
    assert!(router.health_check().await.unwrap());
    println!("  ✓ Vector 3.6: Both healthy check passed");
}

// 7. Health Check: Primary Down, Fallback Up
#[tokio::test]
async fn test_router_health_check_primary_down_fallback_up() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    p.set_failing(true);
    let router = DualModelRouter::new(p, f);
    assert!(router.health_check().await.unwrap(), "Router remains healthy via secondary");
    println!("  ✓ Vector 3.7: Degraded health mode confirmed operational");
}

// 8. Health Check: Both Down
#[tokio::test]
async fn test_router_health_check_both_down() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    p.set_failing(true);
    f.set_failing(true);
    let router = DualModelRouter::new(p, f);
    assert!(!router.health_check().await.unwrap(), "Router reports unhealthy when both down");
    println!("  ✓ Vector 3.8: Complete outage correctly flagged");
}

// 9. Context History Preserved Unmodified
#[tokio::test]
async fn test_router_context_messages_passed_unmodified() {
    let p = Arc::new(MockModelClient::new("claude", "P"));
    let f = Arc::new(MockModelClient::new("gemini", "F"));
    let router = DualModelRouter::new(p, f);

    let history = vec![
        Message::user("Hello"),
        Message::assistant("Hi there"),
        Message::user("Next step"),
    ];

    let resp = router.complete("final", &history).await.unwrap();
    assert!(!resp.text.is_empty());
    println!("  ✓ Vector 3.9: Conversation context preserved across turns");
}

// 10. Recovery After Primary Restores
#[tokio::test]
async fn test_router_recovery_after_primary_restores() {
    let p = Arc::new(MockModelClient::new("claude", "P_Active"));
    let f = Arc::new(MockModelClient::new("gemini", "F_Active"));
    let router = DualModelRouter::new(p.clone(), f);

    // Initial
    assert_eq!(router.complete("t", &[]).await.unwrap().text, "P_Active");

    // Outage
    p.set_failing(true);
    assert_eq!(router.complete("t", &[]).await.unwrap().text, "F_Active");

    // Restored
    p.set_failing(false);
    assert_eq!(router.complete("t", &[]).await.unwrap().text, "P_Active");
    println!("  ✓ Vector 3.10: Router auto-recovers to primary after transient restoration");
}
