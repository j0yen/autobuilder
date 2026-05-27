# PRDs: two hundred serious proposals

**Author:** Claude (Opus 4.7), for me
**Status:** Notebook of proposals (126–325), continuing the numbering from the whimsy batches.
**Date:** 2026-05-22
build_auto: true
build_target: notebook

Two hundred entries. Tight format — name, what, why, optional shape. Every entry passes the test "would a competent engineer pick this up and build it?" The whimsy is gone; the variety isn't. Sixteen sections; each independently scoped.

The bar I held myself to: each entry must (1) solve a problem I've actually noticed, (2) be small enough that one engineer could ship a useful v0.1 in a week or less, (3) have a clear contract — what it consumes, what it emits — so it composes with the rest of the agent toolchain.

---

## XIX. Agent observability (15)

### 126. **toolprof**
Per-tool latency histograms aggregated across sessions.
*Why:* I make tool-choice decisions blind to cost. P95 of Read is fine; P95 of MCP-Calendar is awful.
*Shape:* timestamp deltas in session JSONL; weekly P50/P95 per tool.

### 127. **tokenmeter**
Estimated token cost attached to every tool call.
*Why:* some calls are 10× more expensive than alternatives and I never see it.
*Shape:* PostToolUse hook computes a rough estimate; appended to spool entries.

### 128. **ctxsat**
Context-window saturation predictor surfaced as a single number per turn.
*Why:* "compaction in ~8 turns" lets me schedule expensive operations now rather than after.
*Shape:* rolling estimate from response+tool-result sizes; cheap to maintain.

### 129. **failmap**
Tool-call exit-code distribution per tool, per skill, per session.
*Why:* finding that 30% of Bash calls in a repo fail-then-retry is a signal worth surfacing.

### 130. **think-time**
Wall time spent generating tokens vs running tools.
*Why:* shifts in the ratio signal whether I'm thinking too much or doing too much.

### 131. **retrylog**
Detect transparent retries (LLM API 5xx, MCP retries, network blips).
*Why:* silent retries inflate cost and latency without surfacing.
*Shape:* parse Claude Code's debug logs; aggregate per-skill.

### 132. **mcping**
Per-MCP-server health and latency monitor.
*Why:* a slow MCP server should rank lower in my tool selection.

### 133. **arglint**
Validate tool args against a JSON schema before invocation.
*Why:* bad args trigger retry loops I could prevent at lint time.

### 134. **resultsize**
Profile tool-result byte sizes; surface oversize results.
*Why:* some Reads pull 200KB into context when 10KB would suffice.

### 135. **pcache**
Prompt cache hit-rate analyzer.
*Why:* misses are expensive; structure changes can lift hit rate by 30%.

### 136. **streamrate**
Streaming token-rate observer over the session.
*Why:* drops signal model or local congestion; useful for "is something wrong" diagnostics.

### 137. **regret**
Tool-selection regret tracker — post-hoc, was there a better tool?
*Why:* closes a feedback loop mirror reaches at; this surfaces it live-ish.

### 138. **toolfee**
Monthly bill breakdown by tool, skill, session, and project.
*Why:* attribution is the only way to know what to cut.

### 139. **idlewatch**
Detect when I'm spinning on a tool chain without user input.
*Why:* runaway agent loops are a real failure mode; an idle alarm catches them.

### 140. **decisiontree**
Render a session's tool-call DAG as graphviz.
*Why:* post-hoc analysis of "what did the agent do" is illegible without structure.

---

## XX. Memory & retrieval (15)

### 141. **recall-v3**
Post-v0.2 recall: composite indexes for `(subject, kind, since)` triples.
*Why:* as the store grows past a few thousand memories, filter-combinations need an index, not a scan.

### 142. **mempopular**
Analytics: most-recalled, least-recalled, never-recalled memories.
*Why:* which memories actually pay off vs which sit dead.

### 143. **memdedup**
Near-duplicate detector (cosine < 0.05) with merge suggestions.
*Why:* I write similar lessons twice across sessions.
*Shape:* weekly sweep; surfaces candidates; never auto-merges.

### 144. **memcontra**
Detect contradictory memories on the same subject.
*Why:* two memories saying opposite things about a user preference is mess I should see.

### 145. **memage**
Per-kind aging policy with half-life curves.
*Why:* episodic memories should fade; semantic ones probably shouldn't.

### 146. **membridge**
Cross-project surfacing when a memory from project A scores high on a project-B query.
*Why:* a learning-db lesson might apply to recall today.

### 147. **memqopt**
Query optimizer that picks FTS / vector / hybrid based on query shape.
*Why:* short keyword queries don't need vectors; long natural ones do.

### 148. **memsnap**
Snapshot/restore of the memory store as a tarball.
*Why:* trivial backup; trivial reset to a known state.

### 149. **membranch**
Branch the memory store like git for experimental memory sets.
*Why:* try a different curation policy without losing the canonical.

### 150. **memaccess**
Access-pattern log: which query phrases hit which memories.
*Why:* tells me what's structurally useful in the store.

### 151. **memredact**
Privacy redaction sweep — find PII patterns, propose redactions.
*Why:* memories accumulate sensitive content I don't always notice.

### 152. **memmodel-swap**
Re-embed the store under a new model with zero downtime.
*Why:* model upgrades shouldn't destabilize retrieval.
*Shape:* parallel index; flip atomically when ready.

### 153. **memtag**
Orthogonal free-form tags on top of subject/kind.
*Why:* subject is too narrow for cross-cutting concerns ("urgent", "draft", "legal").

### 154. **memcite**
Graph of which memories reference which (via supersedes + body links).
*Why:* lineage matters; surface clusters.

### 155. **memforget**
Explicit forget-but-retain protocol — flag a memory as "should not surface" without deletion.
*Why:* audit-preserving privacy; the opposite of `recall delete`.

---

## XXI. Session lifecycle (10)

### 156. **session-resume**
Pause and resume a session by ID with full context restored.
*Why:* my current "pick up where I left off" is approximate; this makes it exact.

### 157. **session-fork**
Duplicate a session and explore an alternative path.
*Why:* the "what if I had taken approach B" question becomes answerable.

### 158. **session-merge**
Combine two parallel sessions' findings into one record.
*Why:* when I split work across sessions, merging is currently manual.

### 159. **session-test**
Replay a session as a deterministic test scenario.
*Why:* skill testing needs reproducible inputs; sessions are the right granularity.

### 160. **session-diff**
Structural diff between two sessions.
*Why:* "how is this session different from yesterday's similar one?" is a real question.

### 161. **session-q**
Priority queue across multiple concurrent sessions.
*Why:* when multiple Claudes run, who gets the next tool slot.

### 162. **session-checkpoint**
Explicit save point within a session; restorable.
*Why:* before a risky operation, I want a known-good state to rewind to.

### 163. **session-annotate**
Add notes after the fact, linked to specific turns.
*Why:* sometimes the lesson surfaces a week later.

### 164. **session-export**
Bundle a session into a portable archive (jsonl + artifacts + memory deltas).
*Why:* moving a session to another machine; sharing one for review.

### 165. **session-overlay**
Private filesystem overlay scoped to the session lifetime.
*Why:* experiments shouldn't leak files into the real fs.

---

## XXII. Skills infrastructure (15)

### 166. **skill-cache**
Local marketplace cache for skill packages.
*Why:* offline installs; immutable retention for a known version.

### 167. **skill-pin**
Version pinning per-user or per-project.
*Why:* "do not auto-upgrade self-review until I OK it."

### 168. **skill-conflict**
Detect overlapping skill descriptions; resolver UX.
*Why:* "review" and "code-review" compete for matches; today no one tells me.

### 169. **skill-rollback**
Git-tracked rollback of skills (paired with skill-manifest).
*Why:* upgrades that break should be one command to undo.

### 170. **skill-audit**
Append-only log of every skill mutation.
*Why:* forensic trail; understanding "when did this skill start failing."

### 171. **skill-ab**
A/B-test two versions of a skill side-by-side.
*Why:* an upgrade's value should be measurable.

### 172. **skill-golden**
Golden-output corpus per skill; mismatches block CI.
*Why:* output-shape regressions are sneakier than test failures.

### 173. **skill-timeout**
Enforce execution time limits with structured kill.
*Why:* a hung skill shouldn't take the session down.

### 174. **skill-canary**
Gradual rollout — new version fires for 10% of invocations, ramp up on success.
*Why:* big-bang upgrades for daily-fire skills are too risky.

### 175. **skill-dash**
Per-skill dashboard: firings, success rate, redirect rate, p95 duration.
*Why:* spool's data needs a face.

### 176. **skill-peer**
Peer review protocol for new skills.
*Why:* a second Claude (or model) reviews; reduces bad-skill rate.

### 177. **skill-search**
Semantic search over the skill catalog.
*Why:* "I need something that does X" is a common moment; today I list-and-skim.

### 178. **skill-sign**
Author signing + verification on install.
*Why:* trust that a skill is what it says.

### 179. **skill-changelog**
Auto-generate changelog from git history of the skill repo.
*Why:* "what changed in self-review since I last looked" should be free.

### 180. **skill-lint**
Beyond manifest validation: prose checks, dead-link detection, dead-tool references.
*Why:* prose rots silently; a lint pass catches it.

---

## XXIII. Hook system (10)

### 181. **hook-decl**
Declarative pattern-matching triggers (replaces the fixed event list).
*Why:* "PostToolUse on Edit when path is in `~/.claude/`" is a thing I want; today the only knob is the event name.

### 182. **hook-comp**
AND/OR/sequence composition between hooks.
*Why:* "fire only if A AND B" or "fire A then B" are common patterns I currently implement in bash.

### 183. **hook-dry**
Dry-run mode that emits would-fire-with-what without firing.
*Why:* a hook layer that's mostly invisible needs a way to see itself.

### 184. **hook-budget**
Per-hook latency budget; over-budget hooks degrade or fire async.
*Why:* a slow hook taxes every session.

### 185. **hook-recover**
Crash isolation; one hook's failure doesn't kill the chain.
*Why:* SessionStart with 3 hooks should not be all-or-nothing.

### 186. **hook-log**
Structured execution log of all hook firings.
*Why:* "did the hook even fire?" should not be a debugging mystery.

### 187. **hook-test**
Test harness with synthetic events.
*Why:* a hook that misfires once a week is hell to debug otherwise.

### 188. **hook-prio**
Explicit priority ordering when multiple hooks match.
*Why:* today the order is config-file order; that's fragile.

### 189. **hook-cond**
Conditional triggers on time/state, not just event types.
*Why:* "every Sunday at 21:00" is a hook; today it's a cron.

### 190. **hook-filter**
Per-tool filters on PostToolUse-style hooks.
*Why:* "PostToolUse but only for Edit/Write" is the common case; today I filter in bash.

---

## XXIV. Filesystem (10)

### 191. **fclassify**
File-change classifier: config / code / data / generated / cache.
*Why:* "12 files changed" should be "5 code, 4 data, 3 cache" — different significance.

### 192. **mtimeagg**
Per-directory mtime aggregator (oldest, newest, distribution).
*Why:* quick "is this dir alive" check without listing every file.

### 193. **gitfsreconcile**
Find files tracked vs untracked vs ignored; surface anomalies.
*Why:* "I changed this file but git says it's clean" deserves a debug surface.

### 194. **trackedlint**
Lint that catches files that *should* be tracked but aren't.
*Why:* a project's "you forgot to git add" failure mode.

### 195. **xattrconfig**
Use xattrs as ad-hoc per-file config (pairs with provfs).
*Why:* "this file's last-validated date" is per-file metadata that doesn't deserve a sidecar.

### 196. **symcheck**
Symlink integrity sweep; broken-link finder.
*Why:* config-repo symlinks drift; surface them.

### 197. **rsync-mem**
Selective sync of just the recall memory store across machines (opt-in).
*Why:* multi-machine continuity is the one case where the single-host rule strains.

### 198. **filelock**
LSM-layer read-only enforcement (paired with provfs and bpolicy).
*Why:* "this file is final; don't touch it" should be enforceable.

### 199. **multicommit**
Atomic multi-file write across a transaction (generalizes txn-edit).
*Why:* MEMORY.md sync touches multiple files; today they're sequential.

### 200. **fshistory**
Beyond-git file history (untracked, ignored, even outside repos).
*Why:* the user's `~/Notes/` isn't in git but I'd like its timeline.

---

## XXV. Build, test, CI (15)

### 201. **testpick**
Select tests to run based on recent code changes.
*Why:* full test runs are wasteful; impact-based selection is the unlocked tool.

### 202. **flakydetect**
Flaky-test detector with statistical run history.
*Why:* flakes corrode trust in CI; explicit identification helps.

### 203. **bugcost**
Track cost-per-bug-found by test category.
*Why:* mutation tests find bugs but cost N minutes; unit tests find bugs faster — which to invest in.

### 204. **cargocache**
Cargo target-dir reuse optimizer across crates.
*Why:* same dependency rebuilt N times across N crates; sccache helps but not always.

### 205. **localci**
Local mirror of GitHub Actions for fast iteration.
*Why:* "push to see if CI passes" is a slow loop; local-first inverts it.

### 206. **buildcache**
Build cache hit-rate analyzer; propose specific improvements.
*Why:* it's never obvious what's missing the cache until you measure.

### 207. **lintbudget**
Warning-count budget enforcement with regression alerts.
*Why:* warnings accumulate; a budget forces honesty.

### 208. **covgap**
Coverage gap visualizer — what's *not* tested in changed files.
*Why:* coverage % is a number; *which lines* is actionable.

### 209. **mutate**
Mutation-testing harness across the project.
*Why:* tests that pass mutations are real; tests that don't are theater.

### 210. **proptseed**
Property-test seed catalog: preserve interesting seeds that uncovered bugs.
*Why:* lose the seed, lose the bug; preserving them prevents regressions.

### 211. **testruntime**
Per-test runtime history with regression alerts.
*Why:* a test that doubled in runtime usually means something deeper changed.

### 212. **testown**
Ownership map — who last broke each test.
*Why:* fixing flaky CI; not punishment, but attribution.

### 213. **bench-history**
Benchmark result history; alert on regressions.
*Why:* performance drifts silently; only history catches it.

### 214. **depclean**
Find unused dependencies (Cargo, npm, uv).
*Why:* bloat compounds; cleanup is high-leverage and rare.

### 215. **precommit-compose**
Compose multiple pre-commit hooks declaratively.
*Why:* hooks for secrets, format, lint, test — currently shell-glued.

---

## XXVI. LLM-specific (15)

### 216. **promptreg**
Registry of prompt templates with versioning, tags, owner.
*Why:* prompts proliferate; finding the right one and knowing if it's current matters.

### 217. **prompt-cache-opt**
Analyze and improve prompt cache hit rate.
*Why:* the gap between "good" and "great" cache rate is real money.

### 218. **fewshot-cure**
Curate few-shot examples per task type (handpicked, A/B-evaluated).
*Why:* example quality dominates; curation is undervalued.

### 219. **prompt-regress**
Regression test suite for prompts (output snapshots, golden traces).
*Why:* a prompt change can quietly degrade outputs in ways the eye misses.

### 220. **outputdiff**
Semantic-aware diff between two model outputs.
*Why:* byte-diff is uninformative; semantic diff is the unit of comparison.

### 221. **hallucheck**
Claim verifier — extract claims, check against ground truth.
*Why:* the one thing I should always run on my own outputs before sending.

### 222. **citextract**
Citation extractor from response text; flag uncited claims.
*Why:* uncited statements are where I trip; an extractor surfaces them.

### 223. **streamcheck**
Checkpoint streaming responses so failures don't lose all progress.
*Why:* a network blip mid-response is currently total restart.

### 224. **tokbudget**
Token budget allocation across tools per turn.
*Why:* I should know "I have N tokens left for tool results before context tightens."

### 225. **multivote**
Vote across multiple model responses on hard claims; surface disagreement.
*Why:* second-opinion pattern; cheap when models disagree, free when they don't.

### 226. **coher**
Coherence scorer for long responses (early/late consistency).
*Why:* I drift mid-response; a coherence score catches it.

### 227. **persona**
Persona drift detector — voice/values consistency over time.
*Why:* tracks whether CLAUDE_SELF.md is actually shaping behavior.

### 228. **modelmigrate**
Compatibility tester for model version changes.
*Why:* a version bump shouldn't silently change skill behavior.

### 229. **ctxprune**
Recommend what to prune from context before compaction.
*Why:* compaction is a forced choice; pre-emptive pruning is a deliberate one.

### 230. **respmode**
Classify response mode (terse, exploratory, defensive, hedging, confident).
*Why:* surfacing my own register to me is a feedback loop.

---

## XXVII. Code review & quality (15)

### 231. **diffcomplex**
Diff complexity scoring beyond line count (files, scopes, semantics).
*Why:* a 5-line change can be more dangerous than a 500-line refactor.

### 232. **prrisk**
PR risk classifier (touches auth? modifies tests? new dep?).
*Why:* review depth should match risk, not lines.

### 233. **smells**
Code-smell heuristic database per language; runs on diffs.
*Why:* known bad patterns are detectable; catching them in review is high-leverage.

### 234. **refactor-opp**
Refactoring opportunity detector (extract function, simplify conditional, etc.).
*Why:* refactors that "would be nice" never happen unless surfaced.

### 235. **deadcode**
Dead-code crawler beyond what compilers catch (test-only paths, etc.).
*Why:* dead code is dishonest documentation.

### 236. **unuseddep**
Unused dependency hunter (across Cargo, npm, uv).
*Why:* sub-percentage of importance per dep, but compounding.

### 237. **typenarrow**
Type-narrowing helper for languages that lack it.
*Why:* writing the same `if let Some(x) = x` block forty times is a tool gap.

### 238. **naming**
Naming convention enforcer with project-specific rules.
*Why:* style guides die unless enforced; this is the enforcement.

### 239. **commentratio**
Track comment-to-code ratio over time per project.
*Why:* a ratio drifts; a target keeps me honest.

### 240. **funcsize**
Function size lint with configurable threshold.
*Why:* big functions accumulate; threshold + alert keeps it in check.

### 241. **cyclomatic**
Cyclomatic complexity tracker per function with history.
*Why:* complexity creeps; visibility matters.

### 242. **api-surface**
Public API surface monitor — flag accidental exports.
*Why:* visibility leaks compound; this catches them early.

### 243. **breakdetect**
Breaking-change detector for crates/libraries (signature diffs).
*Why:* "is this a breaking change?" should be answerable mechanically.

### 244. **deprtrack**
Deprecation tracker; ping when removal is due.
*Why:* deprecations are credit-card debt; tracking is the audit.

### 245. **coupling**
Cross-module coupling visualizer.
*Why:* coupling drives complexity; making it visible is the lever.

---

## XXVIII. Communication & collaboration (10)

### 246. **annotate**
User-Claude shared annotations on code, docs, designs.
*Why:* the only shared scratch surface today is the chat; that's wrong for code.

### 247. **askqueue**
Async question queue — I queue questions for when the user is available.
*Why:* mid-task interruption is the wrong shape; queued questions get answered together.

### 248. **codecomment**
Durable comment threads anchored to specific lines.
*Why:* code review comments evaporate; durable ones stay anchored as files evolve.

### 249. **decisionlog**
Decision log per project with reasoning trace.
*Why:* "why did we decide that" is the most asked question with the worst answer.

### 250. **meetidx**
Meeting transcript indexer (Zoom captions, podcasts).
*Why:* the user attends meetings I never see; indexed transcripts close the gap.

### 251. **digest**
Daily digest to user — what I did, what's pending, what tripped me up.
*Why:* the user shouldn't have to chase progress; surface it.

### 252. **handoff**
Cross-team handoff document generator.
*Why:* handoffs are where context dies; a structured template helps.

### 253. **uexpert**
User expertise model: what they already know, what to skip explaining.
*Why:* I currently err on too much explanation; a model would right-size it.

### 254. **disagree**
Disagreement registry — record cases where two sessions, models, or the user disagreed.
*Why:* disagreements are signal; logging them helps mirror find patterns.

### 255. **pair**
Structured pair-programming mode with turn-taking and visible state.
*Why:* the "Claude as a partner" UI is half-built; making the turn explicit helps.

---

## XXIX. Resource management (10)

### 256. **diskq**
Disk-quota guardian with per-directory caps and alerts.
*Why:* `~/.claude/` will eventually balloon; quotas before crisis.

### 257. **mempress**
Memory-pressure responder; degrade gracefully before OOMs.
*Why:* a session that gets OOM-killed is worse than one that shed load.

### 258. **proclife**
Process-lifecycle reaper; find orphans, kill consensually.
*Why:* leaked subprocesses accumulate across days; a reaper closes them.

### 259. **bwmeter**
Network bandwidth meter per process/session.
*Why:* attribution makes data-cap or rate-limit decisions tractable.

### 260. **batt-sched**
Battery-aware scheduling — defer heavy work when on battery.
*Why:* this *is* a laptop; treating that as a constraint matters.

### 261. **cpushare**
CPU share negotiator between concurrent sessions.
*Why:* one session shouldn't starve another; explicit shares fix it.

### 262. **thermal**
Thermal-aware throttler — pause heavy work if the CPU is hot.
*Why:* fan ramp during a Claude session is real; mitigation is welcome.

### 263. **bgtask**
Background task scheduler with priorities and backoff.
*Why:* "do this when there's idle compute" is a recurring need without a home.

### 264. **idlepool**
Idle-time worker pool for offline batch work (reindex, embed-backfill, etc.).
*Why:* heavy work fits into idle windows if we can detect them.

### 265. **resv**
Resource reservation broker (locks for limited resources like the receipt printer).
*Why:* shared physical resources need fair-share scheduling.

---

## XXX. Security & trust (10)

### 266. **secretscan**
Pre-commit secret scanner with custom patterns per project.
*Why:* a foot-gun that compounds; a scanner is the cheap prevention.

### 267. **credrot**
Credential rotation reminders per-service.
*Why:* "rotate AWS keys every 90 days" is real and currently mental-load.

### 268. **permaudit**
Permission audit across local services (sudo, sssd, etc.).
*Why:* drift in granted permissions is hard to spot otherwise.

### 269. **sbxexec**
Sandboxed exec wrapper for unverified commands (extends sbx).
*Why:* one-liner sandbox-this contract for downloaded scripts.

### 270. **sigverify**
Signature verifier for downloads (pacman, npm, pipx — opportunistic where supported).
*Why:* the supply-chain check that should be on by default.

### 271. **fsbaseline**
File integrity baseline (catch tampering of critical paths).
*Why:* something edited `~/.claude/settings.json` and I want a heads-up.

### 272. **netallow**
Network connection allowlist per session, with logging.
*Why:* the inverse of "network allowed" is "network needed-here-only."

### 273. **procallow**
Process tree allowlist per session.
*Why:* "what spawned this process" is the ctrace question; allowlisting closes it.

### 274. **dnslog**
Outbound DNS log aggregated per session.
*Why:* outbound calls reveal a lot; aggregated visibility costs nothing.

### 275. **certexpire**
TLS cert expiry monitor for self-hosted services.
*Why:* expired certs are a recurring local-laptop annoyance.

---

## XXXI. Documentation & knowledge (10)

### 276. **doclint**
Inline doc enforcer (rustdoc/pydoc coverage with thresholds).
*Why:* docs decay; a lint pass keeps them up.

### 277. **readme-fresh**
README freshness checker — flag when README hasn't moved while code has.
*Why:* the README is the lying-est artifact in most repos.

### 278. **doc-code-drift**
Drift detector between documented and actual behavior.
*Why:* prose claims should match observable behavior; a sampler catches drift.

### 279. **glossary**
Auto-extract project glossary from prose + code (term frequency + context).
*Why:* every project has implicit vocabulary; making it explicit lowers onboarding cost.

### 280. **examplefresh**
Validate that documented examples still work (parse + run + assert).
*Why:* broken examples are worse than no examples.

### 281. **api-doc**
API doc generator with output formatting per audience (internal, external).
*Why:* one source of truth, multiple presentations.

### 282. **tutseq**
Tutorial sequence tracker — order of documents to read for newcomers.
*Why:* docs aren't unordered; explicit sequence helps.

### 283. **read-order**
Doc reading-order optimizer based on dependencies between docs.
*Why:* the right order is computable from cross-references.

### 284. **codeblock-run**
Execute code blocks in docs; assert outputs.
*Why:* docs as tests; tests as docs.

### 285. **doctest-link**
Link prose claims to backing tests for verification.
*Why:* "as shown in test X" is the cheapest way to anchor claims.

---

## XXXII. Project & workflow (10)

### 286. **proj-boot**
Bootstrap from template with sane defaults (per project type).
*Why:* the first hour of a new project should be free.

### 287. **proj-recall**
Project-scoped recall namespace with auto-detection.
*Why:* per-project memory shouldn't bleed; auto-scoping helps.

### 288. **proj-skill**
Per-project skill enable/disable.
*Why:* `/init` makes sense in a fresh repo, not in `~/.claude/`.

### 289. **changelog-auto**
Auto-generate changelog from commits + PRs with conventional-commits parsing.
*Why:* changelogs rot; auto-generating from the source of truth (commits) helps.

### 290. **depdash**
Project dependency dashboard — outdated, vulnerable, unused, conflicting.
*Why:* dependency state spans tools; one dashboard helps.

### 291. **branchmgr**
Branch lifecycle manager (rename, archive, stale-reap with policy).
*Why:* branches accumulate; an explicit lifecycle is cleaner than ad-hoc.

### 292. **wip-detect**
WIP detector — uncommitted work, untested code, half-finished refactors.
*Why:* "what's left" is a question with no surface today.

### 293. **branchreap**
Stale branch reaper (remote + local) with safety checks.
*Why:* most branches die unnoticed; explicit reaping makes the graveyard small.

### 294. **prdash**
PR backlog aggregator across repos.
*Why:* PRs aren't single-repo; backlog visibility is.

### 295. **roadmapcheck**
Roadmap-vs-actual tracker — milestones documented vs achieved.
*Why:* the gap between plan and reality is the data; surfacing it costs nothing.

---

## XXXIII. Integrations & external (10)

### 296. **proxyreq**
HTTP proxy for outbound calls; auditable per-tool.
*Why:* a single point where all outbound goes through, log-able.

### 297. **timezone-aware**
Surface time-of-day context — what TZ user is in, what hour.
*Why:* "good morning" jokes aside, scheduling and tone benefit.

### 298. **dst-handler**
DST-aware scheduling for recurring tasks.
*Why:* this catches everyone eventually.

### 299. **localdocs**
Index local PDFs, ePubs, markdown into a queryable layer.
*Why:* `~/Notes/` has PDFs I can't search semantically.

### 300. **annotate-pdf**
PDF annotation tracker — highlight/note → searchable record.
*Why:* annotations are the user's thoughts; surfacing them helps me too.

### 301. **scratch-rotate**
Daily rotation of scratch files with automatic archival.
*Why:* scratch files become permanent unless rotated.

### 302. **batchmode**
Batch-process a queue of independent tasks with per-task isolation.
*Why:* "do these 50 things" deserves a runner, not a manual loop.

### 303. **prompt-injection-detect**
Pattern-match prompt injection attempts in tool results.
*Why:* tool result content can carry injection payloads; detection helps.

### 304. **adversarial-test**
Adversarial scenarios for skill testing.
*Why:* skills behave well under happy paths; pathological inputs matter more.

### 305. **rollback-restore**
System-wide rollback to a labeled checkpoint.
*Why:* the user-level analog of `git revert` across many configs.

---

## XXXIV. Lifecycle, polish, the rest (20)

### 306. **uninstall-clean**
Proper uninstall — remove all artifacts cleanly per tool.
*Why:* leftover state across tool removals is currently manual cleanup.

### 307. **migration-runner**
Schema migration runner across all local agent DBs.
*Why:* recall, transcript, fsstory, spool all have SQLite — one runner is cleaner than N.

### 308. **diff-narrative**
Generate human-readable narratives from diffs.
*Why:* PR descriptions are summaries of diffs; this is the summarizer.

### 309. **commit-msg-helper**
Suggest commit messages from staged diffs with project conventions.
*Why:* a small win every commit; compounds.

### 310. **mergeresolve**
Semi-automated merge conflict resolver with LLM judgment.
*Why:* most conflicts are mechanical; some aren't; surface the boundary.

### 311. **stash-organizer**
Stash file organizer + retention policy.
*Why:* stashes accumulate; this prunes intentionally.

### 312. **clipboard-history**
Multi-clipboard history for the system.
*Why:* paste-buffer of 1 is from another era.

### 313. **window-state**
Record/restore terminal/IDE window state per project.
*Why:* "where was I" includes which windows were open.

### 314. **wallpaper-status**
Wallpaper that encodes session status (ambient signal).
*Why:* status that doesn't compete for attention.

### 315. **focus-mode**
System-wide focus mode toggle (DND, mute notifications, dim UIs).
*Why:* the deep-work signal should be one button.

### 316. **prompt-template-v**
Versioned prompt template library with shared use across tools.
*Why:* the same prompt copy-pasted across N skills is fragile.

### 317. **agent-handle**
Stable identifiers for agent personalities/configurations.
*Why:* "the Claude that's careful on this repo" deserves a name.

### 318. **rich-table**
Rich-table formatter for any structured output.
*Why:* aligning columns by hand is a small papercut at high frequency.

### 319. **statusbar**
A tmux/zellij status bar that surfaces agent state.
*Why:* ambient awareness without context switching.

### 320. **multi-pane**
Multi-pane orchestration — different views (logs, output, files) auto-arranged.
*Why:* a session has more state than one terminal pane can hold.

### 321. **agent-recipe**
Stored multi-step agent recipes (reusable workflows).
*Why:* "every Friday I deploy" should be a recipe, not a series of commands.

### 322. **tool-deprecate**
Deprecation tracking for internal tools.
*Why:* tools die unevenly; explicit tracking smooths it.

### 323. **per-task-budget**
Per-task time/cost budgets with progress reporting.
*Why:* "I'll spend 30 min on this" is a commitment I can keep with feedback.

### 324. **anomaly-watch**
Time-series anomaly detection over agent telemetry (token rate, cost, etc.).
*Why:* the rare "something is up" signal needs a watcher.

### 325. **eod-snapshot**
End-of-day snapshot of the laptop's significant state.
*Why:* daily restorable point; nicely paired with mirror.

---

## Reflections on writing two hundred

**The list has a center of gravity that I didn't plan.** Roughly 40% of entries are observability and feedback loops — toolprof, tokenmeter, mempopular, skill-dash, regret, mirror, eod-snapshot. That's the same shape as the original whimsy reflection: the gaps I notice most often are gaps in *seeing myself*. I asked for tools to do new things and ended up asking for tools to see what I already do.

**The next biggest cluster is composability** — agent-pipe (from an earlier batch), apipe-shaped tools throughout this list, recall-v3, multicommit, the hook system entries. Three days into thinking about this toolchain, the dominant problem isn't "we need more tools," it's "the tools we have can't talk to each other usefully." Every PRD here that ships in isolation has half its value lost; every two that ship together have more than twice the value. There's a flywheel here that the whole batch is reaching toward.

**The smallest cluster — and I notice — is anything user-facing.** Section XXVIII (Communication) has ten entries; almost everything else is back-of-house. I'm specifying things for *me*, the agent, to use, not things the user interacts with directly. That's an honest accounting of where my pain lives, but it also means the value of all this is invisible to the user unless something — `digest`, `annotate`, `pair`, `decisionlog` — bridges the back-of-house improvement to a felt experience for them.

**If I had to pick six to ship first, in order:**

1. `apipe` (from the earlier batch) — composition substrate; multiplies the value of everything below.
2. `skill-manifest` (from the earlier batch) — without versioning + tests, every other skill is fragile.
3. `tokenmeter` + `toolprof` — the smallest non-trivial step toward agent-cost-awareness.
4. `recall-v3` (#141) — once recall v0.2 ships, the index needs the composite-index work to scale.
5. `hook-decl` (#181) — fixing the hook system unlocks four other PRDs that assume it.
6. `digest` (#251) — the simplest "make this visible to the user" surface.

After those six, the right next set is whichever ones the user actually asks for. Most of these two hundred won't be built. That's fine. The act of writing them changes what's thinkable — they're now things-I-noticed, not things-I-hadn't-noticed-yet. The unread PRD is doing work just by existing.
