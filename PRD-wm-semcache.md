# PRD: wm-semcache — she asked an hour ago; answer for free

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-semcache
**Depends on:** recall (consumes its `embed` socket RPC); wm-skills (for the cache-safe flag)
**Codename:** *refrain* — the line of the song that comes back around.

## TL;DR

An elderly companion's speaker is repetitive: the same questions, an hour apart,
in slightly different words. `wm-semcache` is an embedding-keyed response cache.
When an utterance is a near-duplicate (cosine ≥ threshold) of one already
answered, it returns the stored response with **zero API cost** — no Sonnet
turn, no local model. It reuses recall's `embed` RPC and vector substrate so it
shares the system's one embedder. Crucially, it **refuses to cache or serve
time-sensitive answers**: anything whose answer decays (time, weather, "what's
on my calendar today") is routed to a `wm-skills` handler instead, never the
cache — a cache hit there would speak a stale lie.

## 1. Why this exists

- **Repetition is the deployment reality.** The thrift vision's end-state step 3
  is built on the observation that a companion for jsy's mother fields the same
  questions repeatedly. Each repeat is a full Sonnet turn today.
- **The vector substrate is already here.** recall exposes `embed`
  (`recall/src/daemon.rs:27`) on BGE-small (384-dim) with a HashEmbedder fallback
  (256-dim), and stores L2-normalized vectors (`recall/src/embeddings.rs`) —
  cosine similarity is a dot product. wm-semcache reuses this rather than
  standing up a second embedder.
- **Staleness is the failure mode to design against.** A naive response cache
  would happily answer "what time is it" with an hour-old timestamp. The
  cache-safe boundary (a `cache_safe` flag wm-skills already exposes per its PRD)
  is the load-bearing safety property, not an afterthought.

## 2. What this builds

A library crate at `~/wintermute/wm-semcache/`.

### 2.1 Store + lookup

```rust
pub struct SemCache { /* embedder client + entry store */ }
pub struct Entry { pub vec: Vec<f32>, pub utterance: String, pub response: String,
                   pub stored_at: i64, pub ttl_secs: u64 }
impl SemCache {
    pub fn lookup(&self, utterance: &str) -> Option<CachedHit>; // None on miss/expired/unsafe
    pub fn store(&mut self, utterance: &str, response: &str, ttl: Ttl) -> Result<()>;
}
```

- **lookup**: embed the utterance (recall `embed` RPC), cosine-compare against
  live (non-expired) entries, return the best if ≥ similarity threshold.
- **store**: only when the producing route marked the answer cache-safe. TTL is
  per-entry; a global default plus per-intent overrides.

### 2.2 The cache-unsafe gate (the safety property)

The cache **never stores and never serves** an entry whose intent is
cache-unsafe. The unsafe set is sourced from the `cache_safe = false` skills
(time, date, weather, calendar-today, reminders). A cache-unsafe utterance
returns `None` from `lookup` so the router falls through to the skill tier.

### 2.3 Eviction + bound

TTL-based expiry plus a max-entry bound (LRU eviction) so the cache can't grow
unbounded on a device left running for months.

## 3. Acceptance criteria

1. `store` then `lookup` of a paraphrase (different words, same meaning) returns
   the cached response when cosine ≥ threshold — proven with a fixture pair and
   a stubbed embedder (deterministic vectors, no live socket in tests).
2. `lookup` of an unrelated utterance (cosine < threshold) returns `None` (no
   false hit) — proven with a fixture pair.
3. **Cache-unsafe gate:** an utterance classified cache-unsafe returns `None`
   from `lookup` and is rejected by `store`, even when an identical entry exists
   — proven by a test that pre-seeds a "what time is it" entry and asserts it is
   never served. This is the cardinal safety AC.
4. TTL expiry: an entry past its `ttl_secs` is not served (frozen/injected clock,
   no wall-clock flakiness) and is eligible for eviction.
5. Max-entry bound enforced with LRU eviction; exceeding the bound evicts the
   least-recently-hit entry, proven by a test.
6. Embedder client is dim-agnostic (works with 384-dim fastembed and 256-dim
   hash vectors) and degrades safely if recall's `embed` socket is unreachable —
   `lookup` returns `None` (cache miss → router escalates), never panics.
7. A deflection metric is exposed (hits / total lookups) so the cache's cost
   saving is measurable per the vision's "prove deflection" requirement.
8. `cargo test` green; `cargo clippy -D warnings` clean (new crate, high bar);
   MSRV 1.85, no let-chains.
