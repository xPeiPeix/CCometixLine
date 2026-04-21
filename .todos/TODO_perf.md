# Transcript Parse 性能优化（方案 A+C）

> 目标：把 CCometixLine 的 `TranscriptStats::parse` 从"每个 transcript-based segment 各自独立全量 parse"优化为"单次 ccline.exe 进程内共享 + 跨进程增量 parse"，让长会话场景下单次 statusline 生成从潜在 2.5s 降到 ~50ms。
>
> **Phase 1（方案 A）**：共享 parse 结果——消除同一 ccline 进程内 N 次重复 parse（基础设施 `InputData.transcript_stats()` 已存在于 `src/config/types.rs:165-179`，但 5 个 segment 未迁移）。
> **Phase 2（方案 C）**：增量 parse——持久化 offset + 累积 stats 到磁盘 cache，跨 ccline 进程调用只读新追加行。
>
> 背景讨论记录：本任务由 ralph-setup 诊断发现；Phase 1 是半完成的重构（基础设施已有，segment 未迁移），Phase 2 是全新设计。

---

## 🎯 自动采用的默认配置

> ⚠️ 以下默认值由 ralph-setup 阶段 0 探测 + 阶段 1 未覆盖项推断得出。执行前请 review，如需修改直接 Edit 此表。

| 决策点 | 采用值 | 来源 |
|---|---|---|
| 测试策略 | 关键逻辑加 `#[cfg(test)] mod tests` 单元测试 + 部署后 smoke test | 项目 `.gitignore` 里 ignore 了 `tests/` 说明无外部测试习惯；`CLAUDE.md` 也未强制 TDD |
| 测试框架 | Rust 内置 `cargo test`（不引入额外依赖） | `Cargo.toml` 无 test 依赖 |
| 构建命令 | `cargo build --release` | `CLAUDE.md` 项目规则明确 |
| 部署命令 | `cp target/release/ccometixline.exe ~/.claude/ccline/ccline.exe` | `.claude/rules/build-windows.md` 三步连击 |
| Smoke test | `/tmp/smoke_test.sh`（若不存在则跳过，标 `[⏸️]`） | `CLAUDE.md` 提及 |
| 验收标准 | `cargo build --release` 通过 + 部署后长会话 statusline 不再卡顿 | 默认 |
| 分支策略 | Stacked：Phase 2 从 Phase 1 切出 | Phase 1 / 2 天然串行依赖 |
| PR 策略 | 每 Phase 一个 PR，**base 在主人 fork `xPeiPeix/CCometixLine` 内部** | 用户选方案 X |
| 容错次数 | 单轮内失败 3 次 → `git reset --hard HEAD` + `[⚠️] BLOCKED` 跳过 | 默认 |
| 完成 promise | 一次性全做完 | Ralph 单 promise 约束 |
| 硬边界 | 禁止 merge / push upstream / `--force` / `--amend` / `--no-verify` | 默认 |
| Commit 语言 | 中文 ≤50 字，**禁止** `Co-Authored-By` 行 | 全局 `CLAUDE.md` 规则 |

---

## 🚦 启动前预检查（用户手动执行一次）

**必须在运行 Ralph 启动命令前完成，Ralph 进程默认不做这些操作**：

```bash
cd /d/dev_code/AI_related/CCometixLine

# 1. Fork upstream 并自动重配 remote
#    执行后：origin = xPeiPeix/CCometixLine, upstream = Haleclipse/CCometixLine
gh repo fork Haleclipse/CCometixLine --remote=true

# 2. 验证 remote
git remote -v
# 应该看到：
#   origin    https://github.com/xPeiPeix/CCometixLine.git (fetch/push)
#   upstream  https://github.com/Haleclipse/CCometixLine.git (fetch/push)

# 3. push 当前 merge-community-prs 分支到 fork
git push -u origin merge-community-prs

# 4. 确认 Smart App Control 处于 Off（否则 cargo build 会被拦）
#    Windows 设置 → 隐私和安全 → Windows 安全中心 → 应用和浏览器控制 → 智能应用控制 → Off

# 5. 一次性备份原版 ccline.exe（未做过的话）
cp ~/.claude/ccline/ccline.exe ~/.claude/ccline/ccline.exe.original 2>/dev/null || true
```

---

## 🤖 自治运行模式

本流程设计为无人监工执行。执行代理（Ralph Loop）一次跑完所有项 + 全部 PR 的 review 循环，**仅最后合并到 `merge-community-prs` 需要人工操作**。

### 授权范围

| 允许 | 禁止（硬边界） |
|---|---|
| ✅ `git push origin perf/*`（push 到主人 fork） | ❌ `git push upstream *`（永不推 upstream） |
| ✅ `gh pr create --repo xPeiPeix/CCometixLine --base <base>` | ❌ `gh pr merge` / `gh pr merge --auto` |
| ✅ `gh pr checks --watch`（后台） | ❌ `git push --force` / `--force-with-lease` |
| ✅ `gh pr view --json comments` | ❌ `git commit --amend` |
| ✅ `cargo build --release` / `cargo test` | ❌ `git commit --no-verify` |
| ✅ `cp target/release/ccometixline.exe ~/.claude/ccline/ccline.exe` | ❌ 改 `.github/workflows/**` / `.claude/settings*.json` |
| ✅ 改 `src/core/**` / `src/config/types.rs` + 本文件 | ❌ 改 `merge-community-prs` 分支的任何已有 commit |
| ✅ 读 `~/.claude/ccline/.api_usage_cache.json`（参考 cache 模式） | ❌ 改 `master` / `merge-community-prs`（必须走 feature 分支） |

### 状态标记规范

| 标记 | 含义 | 后续行为 |
|---|---|---|
| `[ ] TODO` | 未开始 | 按顺序取下一个 |
| `[🔄] IN_PROGRESS` | 单轮临时态 | 完成后立即改 `[x]` |
| `[x] DONE` | 完成并 commit | 跳过 |
| `[⚠️] BLOCKED: <原因>` | 失败 3 次跳过 | 计入"归来待办" |
| `[⏸️] WAITING_REVIEW: <建议>` | bot 给 B 类建议不自主改 | 计入"归来待办" |
| `[⏸️] CI_STUCK: <PR#>` | CI 超 15 分钟无结论 | 计入"归来待办" |

### 容错机制

- **单项单轮内修代码→跑测试重复 3 次仍失败** → `git reset --hard HEAD` 丢弃改动 → 标 `[⚠️] BLOCKED: <简述>` → 下一项（**禁止 commit 失败代码**）
- **Bot review 给 B 类建议**（设计偏好 / 跨模块重构 / 低置信）→ 标 `[⏸️] WAITING_REVIEW` 跳过
- **CI 卡 > 15 分钟** → 标 `[⏸️] CI_STUCK` 跳过
- **Review 循环 > 5 轮仍有 A 类** → 标 `[⏸️] WAITING_REVIEW` 跳过
- **依赖安装失败 / Smart App Control 拦截**（重试 1 次仍失败）→ 输出 `<promise>AUTONOMOUS_ABORT: <原因></promise>`

**说明**：3 次失败是**单轮迭代内部的小循环**（同一轮迭代里反复试），非跨 Ralph 迭代计数。

### 分支流水线

```
merge-community-prs  ← 主人 fork 的工作分支（Rin 从这里切出，而非 master）
  │
  └── perf/transcript-parse-phase1  ← 从 merge-community-prs 切出
        ├── [Phase 1 所有 commit]
        └── PR #A: base=merge-community-prs, head=perf/transcript-parse-phase1
              │  （全部在主人 fork xPeiPeix/CCometixLine 内部）
              │
              └── perf/transcript-parse-phase2  ← 从 Phase 1 切出（stacked）
                    ├── [Phase 2 所有 commit]
                    └── PR #B: base=perf/transcript-parse-phase1, head=perf/transcript-parse-phase2
```

**Stacked rebase 规则**：
- Phase 2 从 Phase 1 切出后，Phase 1 的 PR review 修改不自动传播到 Phase 2
- 若 Phase 1 review 产生实质性修改（非 nit 清理），代理开工 Phase 2 前先 `git rebase perf/transcript-parse-phase1` 拉取新 commit
- rebase 冲突 > 3 处 → 标 `[⏸️] WAITING_REVIEW: stacked-rebase-conflict` 跳过

### 进度速览（代理每轮更新）

| 分组 | DONE / BLOCKED / WAITING / 总数 | 分支 | PR URL |
|---|---|---|---|
| Phase 1（方案 A：共享 parse） | 0 / 0 / 0 / 8 | - | - |
| Phase 2（方案 C：增量 parse） | 0 / 0 / 0 / 7 | - | - |

---

## 💤 用户归来待办（全流程结束后自动填充此区）

> 尚未开始运行。代理跑完后自动写入：
> - Phase 1 / Phase 2 PR URL + 合并命令（`gh pr merge <PR#> --merge` 或 `--squash`）
> - BLOCKED 项清单及阻塞原因
> - WAITING_REVIEW 项清单及 bot 建议摘要
> - CI_STUCK 项清单
> - 部署后实测数据（长会话 statusline 耗时 before / after）
> - 总耗时 / 完成率

---

## 📋 每轮迭代标准流程

1. **定位**：Read 本文件找当前分组第一个 `[ ] TODO`（跳过非 TODO 状态）
2. **实现规则**：
   - 按任务项里的"修复范围"精确改动，禁止扩大改动面
   - 关键逻辑新增 `#[cfg(test)] mod tests`（仅在本任务涉及的模块里）
   - 运行 `cargo test` 必须全绿才能进下一步
   - 运行 `cargo build --release` 必须通过
3. **Commit**：`git add <改动文件>` + `git commit -m "<中文 ≤50 字>"`
   - **严禁** `--no-verify` / `--amend` / 任何绕过 hook 的选项
   - **严禁** `Co-Authored-By: ...` 行
4. **更新本文件**：Edit 把任务项状态改为 `[x] DONE`，commit 此更新
5. **失败处理**：任一步失败 → 计数 +1，第 3 次 → `git reset --hard HEAD` + 标 `[⚠️] BLOCKED`

---

## 🔁 每组完成后自动执行的 PR Review 循环

完成某组所有项后（含 BLOCKED / WAITING_REVIEW 的非 TODO 态），立即执行以下 4 步循环（内嵌自 pr-reply-review）。

### 步骤 1：创建 PR

```bash
git push -u origin perf/transcript-parse-phase<N>
gh pr create --repo xPeiPeix/CCometixLine \
  --base <base-branch> \
  --head perf/transcript-parse-phase<N> \
  --title "<PR 标题>" \
  --body "<按模板生成>"
```

- Phase 1: `--base merge-community-prs`
- Phase 2: `--base perf/transcript-parse-phase1`（stacked）

PR body 模板：
```
## 背景
<任务描述 + 关联本文件路径>

## Stacked on
<如 Phase 2：上一 PR URL>（merge 前置依赖）

## 自治运行摘要
- ✅ DONE: <勾选项列表>
- ⚠️ BLOCKED: <阻塞项 + 原因>
- ⏸️ WAITING_REVIEW: <等待裁决的 bot 建议>

## Test plan
- [x] `cargo test` 全通过
- [x] `cargo build --release` 通过
- [ ] 部署后长会话实测（需人工验证）
```

### 步骤 2：等 CI（后台）

```bash
gh pr checks <PR> --watch  # run_in_background=true
```

后台任务超过 15 分钟无结论 → 标本 PR 为 `[⏸️] CI_STUCK` → 跳过步骤 3-4 → 进入下一组。

### 步骤 3：读 review 评论 + 分级

```bash
gh pr view <PR> --json comments
```

**零竞态原则**：只读**同步后**的 bot 顶层评论，不读 inline comment（diff review）。

**上下文隔离**：评论输出可能较多时，委托 sonnet subagent 解析，主上下文只保留结构化结论。

对每条建议按严重级 `[Critical] / [Warning] / [High] / [Nit] / [Low]` 和类型分流：

| 类型 | 判定 | 处理 |
|---|---|---|
| **真·bug** | [Critical] / [High] 经 Read 具体 file:line 核对确实有问题 | A 类自主采纳 + 修复 |
| **纯风格** | [Nit] 空格/命名/import 顺序/注释格式 | A 类自主采纳（与项目既有风格冲突时忽略 bot） |
| **明显误报** | 经代码核对确认 bot 没看懂上下文 / 功能已实现 / 约束已满足 | A 类自主拒绝（理由必须附 file:line 代码依据） |
| **设计偏好** | [Warning] 命名哲学 / 抽象层次 / 架构选择 / 模式取舍 | **B 类**：标 `[⏸️] WAITING_REVIEW: <建议摘要>` 跳过 |
| **跨模块重构** | 建议范围 > 单函数 / 涉及多个模块 | **B 类**：标 `[⏸️] WAITING_REVIEW` 跳过 |
| **低置信** | 模型判断"可能是 bug 也可能是有意设计" | **B 类**：标 `[⏸️] WAITING_REVIEW` 跳过 |

**代码核对铁律**：对每条判"真·bug"或"明显误报"的建议，**必须 Read 具体 file:line 做代码核对**。禁止不读代码就采纳 / 拒绝。

**拒绝理由合法来源**：
- 用户明确给出的理由
- 基于代码核对得出的理由（必须在 Review Response commit 里附 `文件:行号` 论证）

禁止：编造拒绝理由 / 把 B 类伪装成误报 / 用"我觉得没必要"这种无代码依据话术。

### 步骤 4：修复 → commit → push → 回步骤 2

A 类建议采纳后 commit 格式严格：

```
<subject 中文 ≤50 字>

Review Response:

✅ 已采纳
- <建议摘要> — <file>:<line>

❌ 未采纳
- <建议摘要>：<理由 + file:line 代码依据>
```

规则：
- 禁止 `--no-verify` / `--amend` / `--force`
- 禁止 `Co-Authored-By`
- 一轮修复一个 commit（采纳项跨模块彼此独立可多 commit）
- 测试跑绿前不 commit

Push 后自动回步骤 2 等新一轮 review。**循环上限 5 轮**，超出 → 剩余 A 类标 `[⏸️] WAITING_REVIEW` 跳过。

**合并到 `merge-community-prs` 由用户手动执行**。代理循环退出后停止当前 PR 的操作，进入下一组。

---

## Phase 1 — 方案 A：共享 parse 结果

### 背景

`src/config/types.rs:165-179` 已存在 `InputData.transcript_stats()` 方法（用 `OnceCell` lazy 缓存），但 5 个 transcript-based segment **仍在直接调用** `TranscriptStats::parse(&input.transcript_path)?`，每次 ccline.exe 运行会 parse 同一个 transcript 5 次。

**本 Phase 目标**：把 5 个 segment 迁移到 `input.transcript_stats()` API，验证单次 ccline 进程内只 parse 一次。

### 风险提示

- `context_window.rs:86 parse_transcript_usage` 是**另一个独立的 transcript 解析函数**（返回 `Option<u32>`，只找 `parentUuid == null` 的 root message token），结构与 `TranscriptStats` 不同——**是否纳入共享 cache 属于 P1-6 决策项**，不在默认迁移范围内
- `transcript_stats_cache: OnceCell<Option<TranscriptStats>>` 用 `std::cell::OnceCell`（非 Sync），ccline.exe 是单线程 subprocess，当前安全；若未来引入多线程 segment 采集需换 `std::sync::OnceLock`

---

#### P1-1 迁移 CacheHitSegment 到共享 parse

**状态**: `[x] DONE`（预先完成于 `abd8669` 重构 commit）

**修复范围**: `src/core/segments/cache_hit.rs:16`

**验证结果**:
- [x] 文件第 16 行已用 `let stats = input.transcript_stats()?;`
- [x] 顶部无 `use crate::core::transcript::TranscriptStats;`
- [x] `abd8669 refactor: 引入 SegmentGroup 分组` commit message 明确"InputData 加 OnceCell 缓存，避免 5 段重复解析 .jsonl"

---

#### P1-2 迁移 TurnsSegment 到共享 parse

**状态**: `[x] DONE`（预先完成于 `abd8669` 重构 commit）

**修复范围**: `src/core/segments/turns.rs:16`

**验证结果**:
- [x] 文件第 16 行已用 `let stats = input.transcript_stats()?;`
- [x] 顶部无 `use crate::core::transcript::TranscriptStats;`

---

#### P1-3 迁移 ToolsSegment 到共享 parse

**状态**: `[x] DONE`（预先完成于 `abd8669` 重构 commit）

**修复范围**: `src/core/segments/tools.rs:16`

**验证结果**:
- [x] 文件第 16 行已用 `let stats = input.transcript_stats()?;`
- [x] 顶部无 `use crate::core::transcript::TranscriptStats;`

---

#### P1-4 迁移 ToolSuccessSegment 到共享 parse

**状态**: `[x] DONE`（预先完成于 `abd8669` 重构 commit）

**修复范围**: `src/core/segments/tool_success.rs:16`

**验证结果**:
- [x] 文件第 16 行已用 `let stats = input.transcript_stats()?;`
- [x] 顶部无 `use crate::core::transcript::TranscriptStats;`

---

#### P1-5 迁移 StopReasonSegment 到共享 parse

**状态**: `[x] DONE`（预先完成于 `abd8669` 重构 commit）

**修复范围**: `src/core/segments/stop_reason.rs:27`

**验证结果**:
- [x] 文件第 27 行已用 `let stats = input.transcript_stats()?;`
- [x] 顶部无 `use crate::core::transcript::TranscriptStats;`

---

#### P1-6 评估 context_window 的独立 parse 是否纳入共享

**状态**: `[x] DONE`（采用方案 b）

**决策**: 选 **(b) 保留独立函数 + 加职责说明注释**

**依据**:
- `parse_transcript_usage` 调用链包含 `try_find_usage_from_project_history`（`context_window.rs:215`），会跨同目录下其他 `.jsonl` 查找历史 session
- `TranscriptStats` 做的是**累加式统计**（`input_tokens += usage.input_tokens`），语义是全量求和；而 `parse_transcript_usage` 做的是**反向查找最后一条带 stop_reason 的 assistant message 的 usage**，只看最后一次
- 两者数据模型不同，强行合并会把简单的反向查找拖进累加逻辑里，属于过度设计

**产出**: `context_window.rs:86` 上方新增多行中文注释说明职责差异

**验收结果**:
- [x] `cargo build --release` 通过（见下方 commit）
- [x] `cargo test` 通过

**Commit 模板**: `docs: 标注 context_window 独立 parse 的职责差异`

---

#### P1-7 新增单元测试验证 single-parse 不变量

**状态**: `[x] DONE`

**修复范围**: `src/config/types.rs` 末尾新增 `#[cfg(test)] mod tests`

**改动**:
- 测试用例 1 `transcript_stats_returns_same_reference_across_calls`：写 3 行合法 JSONL 到临时文件，两次调用 `input.transcript_stats()`，用 `std::ptr::eq` 验证返回同一引用，附带 sanity check `turn_count==2 / input==10 / output==5`
- 测试用例 2 `transcript_stats_returns_none_when_file_missing`：指向不存在的 temp 路径，两次调用都返回 `None`
- 使用 `std::env::temp_dir()` + 进程 pid + 纳秒时间戳生成唯一文件名，不引入 `tempfile` crate

**验收结果**:
- [x] 2 个测试用例在 `cargo test --lib` 下通过（共 17/17 绿）
- [x] 不引入新 crate 依赖（`Cargo.toml` 未改动）

**Commit 模板**: `test: 新增 transcript_stats 共享 parse 单元测试`

---

#### P1-8 构建 + 部署 + smoke test

**状态**: `[ ] TODO`

**修复范围**: 无代码改动，仅验证

**执行步骤**:
```bash
cd /d/dev_code/AI_related/CCometixLine

# 1. Release build
cargo build --release

# 2. 部署
cp target/release/ccometixline.exe ~/.claude/ccline/ccline.exe

# 3. Smoke test（若 /tmp/smoke_test.sh 存在）
if [ -f /tmp/smoke_test.sh ]; then
    bash /tmp/smoke_test.sh
else
    echo "smoke_test.sh 不存在，跳过"
fi

# 4. 手动采样：连续调用 10 次 ccline.exe 看单次耗时
#    用一个真实的 transcript_path 作为 stdin 输入
```

**验收标准**:
- [ ] `cargo build --release` 无 warning / error
- [ ] 部署后 ccline.exe 能正常输出 statusline（至少 model + directory 段渲染正常）
- [ ] smoke_test.sh 通过（或标注"不存在，跳过"）

**Commit 模板**: 无代码改动则不 commit；若 smoke test 发现问题回前面 P1-x 修

---

### Phase 1 完成触发

Phase 1 所有项状态均非 `[ ] TODO` 后，执行：
```bash
git push -u origin perf/transcript-parse-phase1
gh pr create --repo xPeiPeix/CCometixLine \
  --base merge-community-prs \
  --head perf/transcript-parse-phase1 \
  --title "perf: transcript parse 共享（方案 A）" \
  --body <按步骤 1 模板>
```

进入 PR review 循环（步骤 2-4）。结束后更新进度速览表，切 `perf/transcript-parse-phase2` 继续。

---

## Phase 2 — 方案 C：增量 parse + 跨进程 cache

### 背景

Phase 1 把**单次 ccline 进程内** 5 次 parse 合并为 1 次，但每次新的 ccline.exe 进程启动仍然**全量 parse 整个 transcript 文件**（长会话可达 10MB+，单次耗时 300-500ms）。

**本 Phase 目标**：持久化 parse 进度（文件 offset + 累积 stats）到 `~/.claude/ccline/.transcript_cache_<hash>.json`，下次 ccline 调用只需读取新追加的行并累加到 cached stats。

参考实现模式：`src/core/segments/usage.rs:84-113` 的 `ApiUsageCache` load/save 逻辑。

### 风险提示

- **Cache 写失败必须容错**：磁盘满 / 权限问题 → fallback 全量 parse，不阻塞 statusline 返回
- **JSONL offset 必须停在换行边界**：追加 parse 时从 `\n` 后开始读，避免读到半行
- **文件切换/截断检测**：用 `file_size + file_mtime` 双重校验，任一不一致 → 全量 parse 刷新 cache
- **Hash 选型**：`transcript_path` 作 hash 的输入用 `fxhash` 或 `std::collections::hash_map::DefaultHasher`（不引入新 crate；用于生成 cache 文件名，非安全敏感场景）
- **并发写 cache**：理论上同一个 session 不会有并发 ccline 调用，但保险起见 write 用 "先写 .tmp 再 rename" 原子替换

---

#### P2-1 设计 TranscriptCache 数据结构

**状态**: `[ ] TODO`

**修复范围**: 新建 `src/core/transcript_cache.rs`

**改动要点**:
- 新文件模块 `transcript_cache`，在 `src/core/mod.rs` 加 `pub mod transcript_cache;`
- 定义 `struct TranscriptCache`（Serialize + Deserialize）：
  ```rust
  pub struct TranscriptCache {
      pub transcript_path: String,        // 用于路径校验
      pub file_size_at_parse: u64,        // 上次 parse 时的文件大小（offset）
      pub file_mtime: String,             // RFC3339，检测文件重写
      pub stats: TranscriptStats,         // 累积的统计
      pub cached_at: String,              // RFC3339 写 cache 时间
  }
  ```
- 要让 `TranscriptStats` 也 Serialize + Deserialize（当前 `src/core/transcript.rs:7` 只有 Debug/Clone/Default）

**验收标准**:
- [ ] `cargo build --release` 通过（含 serde derive）
- [ ] 可以 `serde_json::to_string(&TranscriptCache { ... })` 往返序列化

**Commit 模板**: `feat: 新增 TranscriptCache 结构体`

---

#### P2-2 实现 cache 文件 load/save

**状态**: `[ ] TODO`

**修复范围**: `src/core/transcript_cache.rs` 里添加 load/save 函数

**改动要点**:
- `fn cache_path(transcript_path: &Path) -> Option<PathBuf>`：用 `DefaultHasher` hash `transcript_path`，生成 `~/.claude/ccline/.transcript_cache_<hash>.json`
- `fn load_cache(transcript_path: &Path) -> Option<TranscriptCache>`：读文件 + 解析，失败返回 None
- `fn save_cache(cache: &TranscriptCache) -> std::io::Result<()>`：原子写（先写 `.tmp` 再 `fs::rename`）
- 仿照 `src/core/segments/usage.rs:84-103` 的 `get_cache_path` / `load_cache` / `save_cache` 模式

**验收标准**:
- [ ] 能够写入 + 读取同一份 cache，内容完全一致
- [ ] load 不存在的 cache 文件返回 None 不 panic
- [ ] save 时 parent 目录不存在会自动 `create_dir_all`

**Commit 模板**: `feat: 实现 transcript cache 文件读写`

---

#### P2-3 实现 TranscriptStats 增量 parse

**状态**: `[ ] TODO`

**修复范围**: `src/core/transcript.rs:20-84`（`TranscriptStats::parse` 函数）

**改动要点**:
- 新增 `pub fn parse_incremental<P: AsRef<Path>>(transcript_path: P, from_offset: u64, init: TranscriptStats) -> (TranscriptStats, u64)`
  - 参数 `init`：上次累积的 stats，本次在其基础上累加
  - 参数 `from_offset`：从此 byte offset 开始读
  - 返回：(新累积 stats, 新的 file_size_offset)
  - 实现：`File::open` + `seek(SeekFrom::Start(from_offset))` + `BufReader::lines()` 读剩余行 + 原有累加逻辑
- 保留原 `parse()` 作为全量入口（内部调用 `parse_incremental(path, 0, TranscriptStats::default())` 并丢弃 offset）

**验收标准**:
- [ ] 原有 `TranscriptStats::parse` 签名和行为不变
- [ ] `cargo build --release` 通过
- [ ] `cargo test` 通过

**Commit 模板**: `feat: TranscriptStats 支持从 offset 增量 parse`

---

#### P2-4 处理边界情况（文件切换/截断/损坏）

**状态**: `[ ] TODO`

**修复范围**: `src/core/transcript_cache.rs` 里添加 `pub fn parse_with_cache(transcript_path: &Path) -> Option<TranscriptStats>`

**改动要点**:
- 入口函数 `parse_with_cache`：
  1. 尝试 `load_cache(path)`
  2. 读当前文件 metadata（`fs::metadata(path)`: `len()` 和 `modified()`）
  3. 决策树：
     - cache 不存在 → 全量 parse + save_cache
     - cache 存在但 `transcript_path` 不匹配 → 全量 parse + save_cache（覆盖）
     - cache 存在、path 匹配、但 `mtime` 变化或 `size < cached size` → 文件被重写/截断，全量 parse + save_cache
     - cache 存在、path + mtime 匹配、`size >= cached size` → 增量 parse：`parse_incremental(path, cached.file_size_at_parse, cached.stats)` + save_cache
  4. 任何 IO 失败 → fallback `TranscriptStats::parse`（原全量）+ 不写 cache
- mtime 存为 RFC3339 字符串（统一复用 `chrono` 生态）

**验收标准**:
- [ ] 不存在 cache 场景：全量 parse + 新建 cache（文件存在）
- [ ] 相同 transcript 第二次调用：用增量 parse（可通过日志或单元测试观察）
- [ ] transcript 被截断场景：降级到全量 parse
- [ ] cache 文件损坏（手动写入非法 JSON）：降级不 panic

**Commit 模板**: `feat: parse_with_cache 处理文件切换 / 截断降级`

---

#### P2-5 集成到 InputData.transcript_stats()

**状态**: `[ ] TODO`

**修复范围**: `src/config/types.rs:174-178`

**改动要点**:
- 修改 `transcript_stats` 方法体：
  - 把 `TranscriptStats::parse(&self.transcript_path)` 改为 `crate::core::transcript_cache::parse_with_cache(Path::new(&self.transcript_path))`
  - import `use std::path::Path;`
- OnceCell 语义不变（单次进程内仍然只执行一次）

**验收标准**:
- [ ] 5 个 segment 依然正常工作（不改它们的代码）
- [ ] `cargo build --release` 通过
- [ ] `cargo test` 通过
- [ ] 观察 `~/.claude/ccline/` 目录会出现 `.transcript_cache_<hash>.json` 文件

**Commit 模板**: `refactor: InputData.transcript_stats 走增量 cache`

---

#### P2-6 新增单元测试（全量 vs 增量一致性 + 降级）

**状态**: `[ ] TODO`

**修复范围**: `src/core/transcript_cache.rs` 末尾 `#[cfg(test)] mod tests { ... }`

**改动要点**:
- 测试用例 1：全量 parse 结果 == 分两次增量 parse 结果（写 3 行 → parse → 追加 2 行 → 增量 parse，比对 stats 各字段）
- 测试用例 2：文件被截断（写 5 行 cache，再缩短文件到 3 行）→ 降级全量 parse
- 测试用例 3：cache 文件损坏（写入非法 JSON）→ 降级全量 parse 不 panic
- 用 `std::env::temp_dir()` + 唯一 filename 避免依赖 `tempfile` crate

**验收标准**:
- [ ] 3 个测试用例都通过
- [ ] 不引入新 crate 依赖

**Commit 模板**: `test: 增量 parse 全量一致性 + 降级路径`

---

#### P2-7 构建 + 部署 + 长会话性能验证

**状态**: `[ ] TODO`

**修复范围**: 无代码改动，仅验证 + 记录数据

**执行步骤**:
```bash
cd /d/dev_code/AI_related/CCometixLine

# 1. Release build
cargo build --release

# 2. 部署
cp target/release/ccometixline.exe ~/.claude/ccline/ccline.exe

# 3. 清理旧 cache（确保冷启动对比）
rm -f ~/.claude/ccline/.transcript_cache_*.json

# 4. 采样：找一个 > 1MB 的真实 transcript，用 hyperfine 或 time 采样
#    记录"首次调用（冷启动全量）" vs "第二次调用（增量）"的耗时
```

**验收标准**:
- [ ] `cargo build --release` 无 warning / error
- [ ] 部署后 ccline.exe 能正常输出 statusline
- [ ] 冷启动耗时与 Phase 1 完成时相当（无倒退）
- [ ] 第二次调用同一 transcript 耗时**明显低于**冷启动（期望 < 50ms）
- [ ] 手动截断一次 transcript，下次调用能降级到全量 parse（`.transcript_cache_*.json` 被覆盖，ccline 正常返回）

**Commit 模板**: 无代码改动则不 commit；发现问题回前面 P2-x 修

---

### Phase 2 完成触发

Phase 2 所有项状态均非 `[ ] TODO` 后，执行：
```bash
git push -u origin perf/transcript-parse-phase2
gh pr create --repo xPeiPeix/CCometixLine \
  --base perf/transcript-parse-phase1 \
  --head perf/transcript-parse-phase2 \
  --title "perf: transcript parse 增量 cache（方案 C，stacked on Phase 1）" \
  --body <按步骤 1 模板>
```

进入 PR review 循环。结束后更新进度速览表 + 填充"💤 用户归来待办"。

---

## 🎯 最终完成

满足所有条件：

- [ ] Phase 1 / Phase 2 所有项状态均非 `[ ] TODO`
- [ ] 两个 PR 均已 push + 创建 + review 循环退出
- [ ] 进度速览表已反映最终状态
- [ ] "💤 用户归来待办"区已填充：
  - 两个 PR URL + 建议合并顺序（先 Phase 1 后 Phase 2，均 merge 到 `merge-community-prs`）
  - BLOCKED / WAITING_REVIEW / CI_STUCK 清单
  - 部署前后耗时对比数据

全部满足后输出 `<promise>DONE</promise>`。

---

## 📦 启动命令

在 worktree 内的 Claude Code 会话粘贴（**极短版 88 字符，CLI 不会自动 wrap 插入换行**）：

```
/ralph-loop:ralph-loop .todos/TODO_perf.md --completion-promise DONE --max-iterations 80
```

Ralph 第一轮会 Read 这个 TODO 文件，获取完整流程描述、授权、状态规范、任务清单——所有细节都在 TODO 文件里。prompt 本身只负责指挥 Ralph 去读文件。

### 为什么必须这么短（踩坑教训）

`ralph-loop.md` 的 command 定义是 `$ARGUMENTS` **裸展开**（不带引号包裹）：

```bash
"${CLAUDE_PLUGIN_ROOT}/scripts/setup-ralph-loop.sh" $ARGUMENTS
```

这意味着用户输入里任何换行符都会被 bash 当作命令分隔符（等同于 `;`）。而 Claude Code CLI 的 input 对超过 ~90 字符的单行**会真实插入换行符**（不是视觉 wrap）。

如果命令长度超过 CLI wrap 阈值，换行可能插在 `--completion-promise` 和它的 value 之间，导致：

1. shell 把 `setup-ralph-loop.sh ... --completion-promise` 当一条完整命令（error: `--completion-promise requires a text argument`）
2. `DONE` 被当成第二条独立 shell 命令（error: `command not found`）

### 命令拆解

- **PROMPT**：`.todos/TODO_perf.md`（裸写，无引号）——`setup-ralph-loop.sh` 支持 "multiple words without quotes"，会把非 flag 的参数累积成 prompt
- **--completion-promise DONE**：无引号单 word value，Ralph 看到 `<promise>DONE</promise>` 才判完成（TODO 文件里会写）
- **--max-iterations 80**：上限，防 Ralph 无穷循环

### 禁用字符清单（bash shell 多层解析陷阱）

- ❌ 双引号包裹长 prompt（触发 CLI wrap 换行，换行被外层 shell 解析为命令分隔符）
- ❌ `<promise>...</promise>` 标签（`<>` 被 bash 识别为重定向符号）
- ❌ 方括号 `[⚠️]` / `[ ]`（bash glob 字符）
- ❌ 反引号（bash 命令替换）
- ❌ `→` / `+` 等视觉分段符号（用中文自然语言描述代替）

---

## 📚 参考

- 项目 CLAUDE.md：`D:/dev_code/AI_related/CCometixLine/CLAUDE.md`
- Windows 构建规则：`.claude/rules/build-windows.md`
- 全局 CLAUDE.md：`~/.claude/CLAUDE.md`（Commit 规范、Bash shell 约束）
- ApiUsageCache 参考实现：`src/core/segments/usage.rs:20-113`
- 基础设施已存在：`src/config/types.rs:165-179 InputData.transcript_stats()`
- 独立 parse 函数（P1-6 决策对象）：`src/core/segments/context_window.rs:86-230`
