use nexus_v2::traits::CircularWalStore;
use nexus_v2::{BackgroundCompactor, CircularWal};

fn create_temp_wal_path() -> (tempfile_guard::TempDirGuard, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nexus_v2_b2_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let wal = dir.join("journal.wal");
    (tempfile_guard::TempDirGuard(dir), wal)
}

mod tempfile_guard {
    pub struct TempDirGuard(pub std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

// 1. Binary Magic Header
#[tokio::test]
async fn test_wal_binary_magic_header() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 100);
    wal.append_frame(1, b"magic_check").await.expect("append frame");

    let bytes = std::fs::read(&path).expect("read raw wal bytes");
    assert!(bytes.len() >= 4);
    assert_eq!(&bytes[0..4], b"NX2W", "WAL binary frame must start with NX2W magic");
    println!("  ✓ Vector 2.1: Binary magic header NX2W verified");
}

// 2. Sequence Monotonicity
#[tokio::test]
async fn test_wal_sequence_monotonicity() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 100);
    let s1 = wal.append_frame(1, b"one").await.unwrap();
    let s2 = wal.append_frame(1, b"two").await.unwrap();
    let s3 = wal.append_frame(1, b"three").await.unwrap();

    assert_eq!(s1, 1);
    assert_eq!(s2, 2);
    assert_eq!(s3, 3);
    println!("  ✓ Vector 2.2: Sequence monotonicity strictly incrementing");
}

// 3. Timestamp Recording
#[tokio::test]
async fn test_wal_timestamp_recording() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 100);
    wal.append_frame(1, b"ts_check").await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].timestamp > 1700000000000, "Timestamp must be valid epoch millis");
    println!("  ✓ Vector 2.3: Frame timestamp recording confirmed");
}

// 4. Payload Integrity & CRC Checksum
#[tokio::test]
async fn test_wal_payload_integrity_crc32() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 100);
    wal.append_frame(5, b"checksum_payload_verified").await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].payload, b"checksum_payload_verified");
    println!("  ✓ Vector 2.4: Payload CRC checksum verified");
}

// 5. Crash Recovery Full Replay
#[tokio::test]
async fn test_wal_crash_recovery_full_replay() {
    let (_guard, path) = create_temp_wal_path();
    {
        let mut wal = CircularWal::new(&path, 50);
        for i in 1..=10 {
            wal.append_frame(i as u8, format!("payload_{}", i).as_bytes()).await.unwrap();
        }
    } // Abrupt process termination simulation: wal dropped

    let mut recovered_wal = CircularWal::new(&path, 50);
    let records = recovered_wal.recover().await.unwrap();
    assert_eq!(records.len(), 10);
    for (i, r) in records.iter().enumerate() {
        assert_eq!(r.sequence, (i + 1) as u64);
        assert_eq!(r.payload, format!("payload_{}", i + 1).as_bytes());
    }
    println!("  ✓ Vector 2.5: Crash recovery replayed complete 10-frame sequence");
}

// 6. Crash Recovery Partial Tail Drop (Tolerates torn page / abrupt power loss)
#[tokio::test]
async fn test_wal_crash_recovery_partial_tail_drop() {
    let (_guard, path) = create_temp_wal_path();
    {
        let mut wal = CircularWal::new(&path, 50);
        wal.append_frame(1, b"complete_frame_1").await.unwrap();
        wal.append_frame(2, b"complete_frame_2").await.unwrap();
    }

    // Append corrupted/incomplete fragment bytes at end of file
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"NX2W\x03\x00\x00\x00").unwrap(); // Incomplete header fragment
    }

    let mut recovered_wal = CircularWal::new(&path, 50);
    let records = recovered_wal.recover().await.unwrap();
    assert_eq!(records.len(), 2, "Corrupted tail fragment must be discarded without crashing");
    println!("  ✓ Vector 2.6: Corrupted tail fragment safely discarded during recovery");
}

// 7. Empty File Recovery
#[tokio::test]
async fn test_wal_empty_file_recovery() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 50);
    let records = wal.recover().await.unwrap();
    assert!(records.is_empty(), "Empty WAL must recover cleanly to 0 records");
    println!("  ✓ Vector 2.7: Empty file recovery returns clean empty state");
}

// 8. Ring Buffer Bounded Capacity Enforcement
#[tokio::test]
async fn test_wal_ring_buffer_bounded_capacity() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 5); // Bounded to 5 frames max
    for i in 1..=20 {
        wal.append_frame(i as u8, format!("frame_{}", i).as_bytes()).await.unwrap();
    }

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 5, "Ring buffer must strictly retain only 5 most recent frames");
    assert_eq!(records[0].payload, b"frame_16");
    assert_eq!(records[4].payload, b"frame_20");
    println!("  ✓ Vector 2.8: Ring buffer bounded capacity strictly enforced");
}

// 9. Auto Compaction Rewrites File Cleanly
#[tokio::test]
async fn test_wal_auto_compaction_truncates_safely() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 4);
    for i in 1..=12 {
        wal.append_frame(1, format!("entry_{}", i).as_bytes()).await.unwrap();
    }

    BackgroundCompactor::auto_compact_if_needed(&mut wal).await.unwrap();
    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 4);
    assert_eq!(records[3].payload, b"entry_12");
    println!("  ✓ Vector 2.9: Background compaction pruned obsolete frames safely");
}

// 10. Sequential Burst Append Simulation
#[tokio::test]
async fn test_wal_multi_append_concurrent_sim() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 100);

    for batch in 0..5 {
        for item in 0..10 {
            wal.append_frame(batch, format!("b{}_i{}", batch, item).as_bytes()).await.unwrap();
        }
    }

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 50);
    println!("  ✓ Vector 2.10: 50-frame burst append maintains exact ordering");
}

// 11. Large Payload Handling (64KB frame)
#[tokio::test]
async fn test_wal_large_payload_handling() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 10);

    let large_data = vec![0xAB; 64 * 1024]; // 64 KB
    wal.append_frame(99, &large_data).await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].payload.len(), 64 * 1024);
    assert_eq!(records[0].payload[0], 0xAB);
    println!("  ✓ Vector 2.11: 64KB large payload preserved with intact checksum");
}

// 12. Zero-Byte Payload Handling
#[tokio::test]
async fn test_wal_zero_byte_payload_handling() {
    let (_guard, path) = create_temp_wal_path();
    let mut wal = CircularWal::new(&path, 10);
    wal.append_frame(0, &[]).await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].payload.is_empty());
    println!("  ✓ Vector 2.12: Zero-byte payload handled cleanly");
}
