use nexus_core::polyglot::{PolyglotDetector, PolyglotDoctor, PolyglotLanguage};
use nexus_core::sandbox::Sandbox;
use nexus_core::tools::edit_file::{safe_replace, PatchTier};
use nexus_core::tools::run_command::compact_output_stream;
use nexus_core::tools::{ToolCall, ToolManager};
use std::fs;
use uuid::Uuid;

#[tokio::test]
async fn test_polyglot_python_coding_and_doctor_repair() {
    println!("\n\x1b[1;36m=== POLYGLOT TEST 1: Python Engine & Doctor Auto-Repair ===\x1b[0m");
    let test_dir = std::env::temp_dir().join(format!("nexus_polyglot_python_{}", Uuid::new_v4()));
    fs::create_dir_all(&test_dir).unwrap();

    // 1. Create a Python project with an Async TTL Cache with an intentional logical bug
    let cache_code = r#"import time

class AsyncTtlCache:
    def __init__(self, capacity: int, ttl_seconds: float):
        self.capacity = capacity
        self.ttl_seconds = ttl_seconds
        self.store = {}
        self.timestamps = {}

    def get(self, key):
        if key not in self.store:
            return None
        # BUG: Inverted TTL expiration check (< instead of >)
        if time.time() - self.timestamps[key] < self.ttl_seconds:
            return None
        return self.store[key]

    def set(self, key, value):
        if len(self.store) >= self.capacity:
            oldest = min(self.timestamps, key=self.timestamps.get)
            del self.store[oldest]
            del self.timestamps[oldest]
        self.store[key] = value
        self.timestamps[key] = time.time()
"#;

    let test_code = r#"import unittest
import time
from cache import AsyncTtlCache

class TestTtlCache(unittest.TestCase):
    def test_cache_hit_and_eviction(self):
        c = AsyncTtlCache(capacity=2, ttl_seconds=5.0)
        c.set("alpha", 100)
        c.set("beta", 200)
        # Immediate read within TTL window must succeed
        self.assertEqual(c.get("alpha"), 100)
        self.assertEqual(c.get("beta"), 200)

        # Capacity eviction
        c.set("gamma", 300)
        self.assertIsNone(c.get("alpha"))
        self.assertEqual(c.get("gamma"), 300)

if __name__ == "__main__":
    unittest.main()
"#;

    fs::write(test_dir.join("cache.py"), cache_code).unwrap();
    fs::write(test_dir.join("test_cache.py"), test_code).unwrap();

    // 2. Detect language
    let detected = PolyglotDetector::detect(&test_dir);
    assert_eq!(detected, PolyglotLanguage::Python);
    println!("✓ Successfully detected Python project");

    // 3. Polyglot Doctor check: must catch the test failure
    let check_res = PolyglotDoctor::run_check(&test_dir).await.unwrap();
    assert_eq!(check_res.language, PolyglotLanguage::Python);
    assert!(!check_res.success, "Test suite must fail due to inverted TTL expiration bug");
    assert!(!check_res.diagnostics.is_empty(), "Must capture test failure diagnostic");
    println!("✓ Polyglot Doctor captured Python test failure:\n  {}", check_res.diagnostics[0].message);

    // 4. Autonomous repair using Nexus 3-Tier Patcher with unaligned indentation (4 spaces vs 8 spaces in file)
    let target = "    # BUG: Inverted TTL expiration check (< instead of >)\n    if time.time() - self.timestamps[key] < self.ttl_seconds:\n        return None";
    let replacement = "    # FIXED: Expire only after TTL has elapsed\n    if time.time() - self.timestamps[key] > self.ttl_seconds:\n        return None";

    let patch_res = safe_replace(&test_dir.join("cache.py"), target, replacement).unwrap();
    assert_eq!(patch_res.tier, PatchTier::Tier2WhitespaceNormalized);
    println!("✓ 3-Tier Patcher successfully applied indentation-rebased patch via {:?}", patch_res.tier);

    // Verify indentation in cache.py was rebased to 8 spaces
    let patched_content = fs::read_to_string(test_dir.join("cache.py")).unwrap();
    assert!(patched_content.contains("        if time.time() - self.timestamps[key] > self.ttl_seconds:\n            return None"));

    // 5. Re-run Polyglot Doctor: must now report 100% green
    let post_fix_res = PolyglotDoctor::run_check(&test_dir).await.unwrap();
    assert!(post_fix_res.success, "All Python unit tests must pass after patch");
    assert!(post_fix_res.diagnostics.is_empty());
    println!("✓ Polyglot Doctor verified 100% green Python test suite!");

    // 6. Test sandbox run_command execution
    let sandbox = Sandbox::new(&test_dir, false).unwrap();
    let manager = ToolManager::new(sandbox);

    let run_call = ToolCall {
        id: "call_py_test".to_string(),
        name: "run_command".to_string(),
        arguments: serde_json::json!({ "command": "python3 -m unittest test_cache.py" }),
    };

    let result = manager.execute(&run_call).await;
    assert!(!result.is_error);
    assert!(result.output.contains("OK") || result.output.contains("Ran 1 test"));
    println!("✓ Sandbox tool execution ran Python suite successfully:\n{}", result.output.trim());

    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test]
async fn test_polyglot_javascript_coding_and_doctor_repair() {
    println!("\n\x1b[1;36m=== POLYGLOT TEST 2: JavaScript / Node.js Engine & Doctor Repair ===\x1b[0m");
    let test_dir = std::env::temp_dir().join(format!("nexus_polyglot_node_{}", Uuid::new_v4()));
    fs::create_dir_all(&test_dir).unwrap();

    // 1. Create a Node.js ESM project with an async pipeline bug
    let package_json = r#"{
  "name": "nexus-async-pipeline",
  "version": "1.0.0",
  "type": "module"
}"#;

    let pipeline_js = r#"export class AsyncPipeline {
  constructor() {
    this.stages = [];
  }

  use(fn) {
    this.stages.push(fn);
    return this;
  }

  async process(data) {
    let current = data;
    for (const stage of this.stages) {
      // BUG: forgot await on async stage function!
      current = stage(current);
    }
    return current;
  }
}
"#;

    let test_js = r#"import test from 'node:test';
import assert from 'node:assert/strict';
import { AsyncPipeline } from './pipeline.js';

test('pipeline processes async transformations sequentially', async () => {
  const p = new AsyncPipeline();
  p.use(async (x) => x * 2);
  p.use(async (x) => x + 10);

  const res = await p.process(5);
  // (5 * 2) + 10 = 20
  assert.equal(res, 20);
});
"#;

    fs::write(test_dir.join("package.json"), package_json).unwrap();
    fs::write(test_dir.join("pipeline.js"), pipeline_js).unwrap();
    fs::write(test_dir.join("test_pipeline.test.js"), test_js).unwrap();

    // 2. Detect language
    let detected = PolyglotDetector::detect(&test_dir);
    assert_eq!(detected, PolyglotLanguage::JavaScript);
    println!("✓ Successfully detected JavaScript ESM project");

    // 3. Polyglot Doctor check: catches the assertion failure
    let check_res = PolyglotDoctor::run_check(&test_dir).await.unwrap();
    assert_eq!(check_res.language, PolyglotLanguage::JavaScript);
    assert!(!check_res.success, "Node test must fail due to unawaited promise in pipeline");
    println!("✓ Polyglot Doctor captured Node.js assertion failure:\n  {}", check_res.diagnostics[0].message);

    // 4. Safe patch using Tier 1 Exact Match
    let target = "      // BUG: forgot await on async stage function!\n      current = stage(current);";
    let replacement = "      // FIXED: await async transformation\n      current = await stage(current);";

    let patch_res = safe_replace(&test_dir.join("pipeline.js"), target, replacement).unwrap();
    assert_eq!(patch_res.tier, PatchTier::Tier1Exact);
    println!("✓ 3-Tier Patcher applied precision patch to JavaScript file");

    // 5. Polyglot Doctor re-check
    let post_fix_res = PolyglotDoctor::run_check(&test_dir).await.unwrap();
    assert!(post_fix_res.success, "Node.js test suite must pass after patch");
    println!("✓ Polyglot Doctor verified 100% green Node.js test suite!");

    // 6. Test tool execution via code_check tool
    let sandbox = Sandbox::new(&test_dir, false).unwrap();
    let manager = ToolManager::new(sandbox);

    let check_call = ToolCall {
        id: "call_node_check".to_string(),
        name: "code_check".to_string(),
        arguments: serde_json::json!({}),
    };

    let result = manager.execute(&check_call).await;
    assert!(!result.is_error);
    assert!(result.output.contains("All compiler syntax and test gates passing cleanly"));
    println!("✓ ToolManager code_check tool verified clean status:\n{}", result.output.trim());

    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test]
async fn test_polyglot_c_compiler_diagnostics_and_repair() {
    println!("\n\x1b[1;36m=== POLYGLOT TEST 3: C Systems Engine & Compiler Repair ===\x1b[0m");
    let test_dir = std::env::temp_dir().join(format!("nexus_polyglot_c_{}", Uuid::new_v4()));
    fs::create_dir_all(&test_dir).unwrap();

    // 1. Create a C project with a compiler warning/error under -Wall -Werror
    let header_c = r#"#ifndef RING_BUFFER_H
#define RING_BUFFER_H

#include <stddef.h>

typedef struct {
    int *buf;
    size_t capacity;
    size_t head;
    size_t tail;
} RingBuffer;

RingBuffer *rb_create(size_t capacity);
int rb_push(RingBuffer *rb, int val);
int rb_pop(RingBuffer *rb, int *out);
void rb_free(RingBuffer *rb);

#endif
"#;

    let source_c = r#"#include "ring_buffer.h"
#include <stdlib.h>

RingBuffer *rb_create(size_t capacity) {
    RingBuffer *rb = (RingBuffer *)malloc(sizeof(RingBuffer));
    rb->buf = (int *)malloc(sizeof(int) * capacity);
    rb->capacity = capacity;
    rb->head = 0;
    rb->tail = 0;
    return rb;
}

int rb_push(RingBuffer *rb, int val) {
    size_t next = (rb->head + 1) % rb->capacity;
    if (next == rb->tail) {
        return -1; // Full
    }
    rb->buf[rb->head] = val;
    rb->head = next;
    return 0;
}

int rb_pop(RingBuffer *rb, int *out) {
    if (rb->head == rb->tail) {
        return -1; // Empty
    }
    *out = rb->buf[rb->tail];
    rb->tail = (rb->tail + 1) % rb->capacity;
    return 0;
}

void rb_free(RingBuffer *rb) {
    if (rb) {
        free(rb->buf);
        free(rb);
    }
}
"#;

    let test_c = r#"#include "ring_buffer.h"
#include <assert.h>
#include <stdio.h>

int main(void) {
    RingBuffer *rb = rb_create(4);
    assert(rb_push(rb, 10) == 0);
    assert(rb_push(rb, 20) == 0);
    assert(rb_push(rb, 30) == 0);
    // Buffer has capacity 4, can hold 3 items before next == tail
    assert(rb_push(rb, 40) == -1);

    int val = 0;
    assert(rb_pop(rb, &val) == 0 && val == 10);
    assert(rb_pop(rb, &val) == 0 && val == 20);
    assert(rb_push(rb, 50) == 0);

    assert(rb_pop(rb, &val) == 0 && val == 30);
    assert(rb_pop(rb, &val) == 0 && val == 50);
    assert(rb_pop(rb, &val) == -1);

    rb_free(rb);
    printf("C RingBuffer integration test passed!\n");
    return 0;
}
"#;

    fs::write(test_dir.join("ring_buffer.h"), header_c).unwrap();
    fs::write(test_dir.join("ring_buffer.c"), source_c).unwrap();
    fs::write(test_dir.join("test_ring.c"), test_c).unwrap();

    // 2. Detect language
    let detected = PolyglotDetector::detect(&test_dir);
    assert_eq!(detected, PolyglotLanguage::C);
    println!("✓ Successfully detected C systems project");

    // 3. Polyglot Doctor check
    let check_res = PolyglotDoctor::run_check(&test_dir).await.unwrap();
    assert_eq!(check_res.language, PolyglotLanguage::C);
    assert!(check_res.success, "C code must compile cleanly under -fsyntax-only -Wall -Wextra");
    println!("✓ Polyglot Doctor verified clean C syntax and headers");

    // 4. Compile and run test binary using Sandbox run_command
    let sandbox = Sandbox::new(&test_dir, false).unwrap();
    let manager = ToolManager::new(sandbox);

    let compile_call = ToolCall {
        id: "call_gcc".to_string(),
        name: "run_command".to_string(),
        arguments: serde_json::json!({
            "command": "gcc -Wall -Wextra -Werror ring_buffer.c test_ring.c -o test_ring && ./test_ring"
        }),
    };

    let compile_res = manager.execute(&compile_call).await;
    assert!(!compile_res.is_error, "C compilation and execution must succeed: {}", compile_res.output);
    assert!(compile_res.output.contains("C RingBuffer integration test passed!"));
    println!("✓ Executed compiled C binary via Sandbox:\n{}", compile_res.output.trim());

    // 5. Test output clamping on large stack trace output
    let verbose_log = (0..500)
        .map(|i| format!("Traceback frame #{}: /src/module.c: line {}", i, i * 4))
        .collect::<Vec<_>>()
        .join("\n");
    let compacted = compact_output_stream(&verbose_log, 50);
    assert!(compacted.contains("omitted"));
    assert!(compacted.lines().count() <= 55);
    println!("✓ Verified output stream clamping on 500-line polyglot trace");

    let _ = fs::remove_dir_all(&test_dir);
}
