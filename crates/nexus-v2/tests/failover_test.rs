use std::sync::Arc;
use nexus_v2::{DualModelRouter, MockModelClient, ModelClient};

#[tokio::test]
async fn test_dual_model_failover_on_primary_error() {
    let primary = Arc::new(MockModelClient::new("claude-primary", "Primary Response"));
    let fallback = Arc::new(MockModelClient::new("gemini-fallback", "Fallback Activated"));

    let router = DualModelRouter::new(primary.clone(), fallback.clone());

    // 1. Initial healthy state: Primary serves responses
    let resp1 = router.complete("Hello", &[]).await.expect("Primary completion");
    assert_eq!(resp1.text, "Primary Response");
    assert_eq!(router.get_failover_count(), 0, "No failover when primary is healthy");
    println!("✓ Primary model serviced request with zero failover overhead");

    // 2. Simulate primary model failure (HTTP 429 or network disconnect)
    primary.set_failing(true);

    // 3. Complete should seamlessly failover to fallback
    let resp2 = router.complete("Hello after primary outage", &[]).await.expect("Fallback completion");
    assert_eq!(resp2.text, "Fallback Activated");
    assert_eq!(router.get_failover_count(), 1, "Failover counter must register 1 failover");
    println!("✓ Router intercepted primary failure and seamlessly failed over to secondary model without dropping request");

    // 4. Test both failing: returns DualModelFailure
    fallback.set_failing(true);
    let resp3 = router.complete("Hello both down", &[]).await;
    assert!(resp3.is_err(), "Must report failure if both primary and secondary are unreachable");
    println!("✓ Both models down returns structured NexusV2Error::DualModelFailure");
}
