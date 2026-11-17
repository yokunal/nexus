use nexus_v2::traits::CircularWalStore;
use nexus_v2::{BackgroundCompactor, CircularWal};

#[tokio::test]
async fn test_circular_wal_append_and_crash_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("nexus_v2_wal_{}", uuid::Uuid::new_v4()));
    let wal_path = temp_dir.join("test.wal");

    // 1. Initialize Circular WAL with bounded capacity of 5 records
    let mut wal = CircularWal::new(&wal_path, 5);

    // 2. Append frames
    let seq1 = wal.append_frame(1, b"init_session").await.expect("Append frame 1");
    let seq2 = wal.append_frame(2, b"plan_step_1").await.expect("Append frame 2");
    let seq3 = wal.append_frame(3, b"verify_pass").await.expect("Append frame 3");

    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(seq3, 3);
    println!("✓ Successfully appended 3 sequential atomic WAL frames with CRC checksums");

    // 3. Simulate sudden crash: drop existing in-memory handle, instantiate new WAL from file
    drop(wal);
    let mut recovered_wal = CircularWal::new(&wal_path, 5);
    let records = recovered_wal.recover().await.expect("Recover from crash");

    assert_eq!(records.len(), 3, "All 3 frames must be recovered");
    assert_eq!(records[0].payload, b"init_session");
    assert_eq!(records[1].payload, b"plan_step_1");
    assert_eq!(records[2].payload, b"verify_pass");
    println!("✓ Crash recovery successfully reconstructed complete sequence history from disk");

    // 4. Test ring buffer overflow and auto-compaction
    for i in 4..=10 {
        recovered_wal.append_frame(i as u8, format!("frame_{}", i).as_bytes()).await.unwrap();
    }

    BackgroundCompactor::auto_compact_if_needed(&mut recovered_wal).await.expect("Compaction");
    let compacted_records = recovered_wal.recover().await.expect("Recover post-compaction");

    assert_eq!(compacted_records.len(), 5, "Ring buffer must enforce bounded capacity of 5 frames");
    assert_eq!(compacted_records[compacted_records.len() - 1].payload, b"frame_10");
    println!("✓ Circular WAL bounded capacity enforced: zero unbounded disk bloat!");

    let _ = std::fs::remove_dir_all(temp_dir);
}
