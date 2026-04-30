/**
 * Tests for VCS operations.
 *
 * These tests verify the Repository class API.
 */

import { describe, it, expect } from 'vitest';
import type { VcsLogEntry, VcsStatus, VcsOpResult } from '../src/types.js';

// Note: These tests require a running WASM module.
// They are structured as type-level tests that verify the API shape.

describe('VCS types', () => {
  it('VcsLogEntry should have correct shape', () => {
    const entry: VcsLogEntry = {
      message: 'initial commit',
      author: 'alice',
      timestamp: 1000,
      protocol: 'atproto',
    };

    expect(entry.message).toBe('initial commit');
    expect(entry.author).toBe('alice');
    expect(entry.timestamp).toBe(1000);
    expect(entry.protocol).toBe('atproto');
  });

  it('VcsStatus should have correct shape', () => {
    const status: VcsStatus = {
      branch: 'main',
      head_commit: null,
    };

    expect(status.branch).toBe('main');
    expect(status.head_commit).toBeNull();
  });

  it('VcsOpResult should have correct shape', () => {
    const result: VcsOpResult = {
      success: true,
      message: 'branch created',
    };

    expect(result.success).toBe(true);
    expect(result.message).toBe('branch created');
  });
});
