# AI Leftovers — dọn "rác theo phiên" của AI coding agent: kế hoạch chi tiết

> Trạng thái: **xong** (2026-10-17) — lát 1 ✅ (catalog + bảo vệ lịch sử hội thoại mở rộng), lát 2 ✅ (nhận diện phiên), lát 3 ✅ (worktree qua `git worktree`), lát 4 ✅ (tab "AI leftovers", Health Score, gợi ý Home, CLI `nfd ai-leftovers`, `gui-smoke`). Mục **C10** ở `docs/PLAN.md` §13.2 (nhóm C, ①). Mọi thao tác xóa đi qua đường Phase 3: `Planner → SafetyGate → Executor → UndoJournal` (`docs/PHASE3_CLEAN.md`); lệnh git ghi vào Activity log (kind `worktree`) như lệnh C4. **Lịch sử hội thoại của AI agent không bao giờ là ứng viên** (PLAN §5.1).

## 1. Mục tiêu & DoD

Thời AI agent, rác không còn chỉ là cache toàn cục (đã có ở `rules/ai-tools.toml`). Mỗi phiên làm việc của agent để lại **thứ của riêng phiên đó**, nằm rải trong thư mục dự án và Temp, và người dùng **không biết cái nào an toàn để xóa** — họ chỉ chắc một điều: *cuộc hội thoại phải còn*.

Đo trên máy tham chiếu (2026-10-17): 6 worktree của Claude Code trong `D:\Working\SelfProject\NANOFreeDisk\.claude\worktrees` (hai cái ≈ 11,7 GB và 14 GB, phần lớn là build output; phiên đã xong và đã merge), scratchpad `%LOCALAPPDATA%\Temp\claude` 1,2 GB (62 thư mục — hiện chỉ được quét như Temp chung, không biết của phiên nào), `.playwright-mcp` (console log + page snapshot), `GitManagementGUI\.agent\screenshots` 115 MB, `test-results`, `.codex` trong dự án, `ms-playwright` 2,8 GB với 4 bản của mỗi trình duyệt.

**DoD:**
- Tìm được leftover theo **dự án → công cụ → phiên**, mỗi mục có "từ phiên X, hoạt động lần cuối N ngày trước".
- Leftover của phiên **còn mở** (desktop app chưa archive) hoặc **hoạt động trong 7 ngày** được liệt kê nhưng **không bao giờ chọn sẵn**.
- Worktree chỉ gỡ bằng `git worktree remove` (+ `git worktree prune`), chỉ khi sạch (không thay đổi / tệp chưa theo dõi) và nhánh đã merge vào nhánh mặc định hoặc đã push; không bao giờ xóa nhánh.
- Tên chung chung (`test-results`, `screenshots`, `scratch`…) chỉ khi git xác nhận không được theo dõi; thư mục có tệp git theo dõi không bao giờ đề xuất.
- Test: đường dẫn hội thoại / memory / transcript của mọi công cụ không bao giờ vào plan (proptest + fixture); worktree bẩn không bao giờ bị gỡ; thư mục git theo dõi không bao giờ đề xuất; leftover của phiên đang hoạt động không bao giờ chọn sẵn.
- EN + VI (`locales/{en,vi}/ai-leftovers.ftl`), màu là token theme, `gui-smoke` phủ tab bằng fixture riêng (không đụng worktree / Temp thật, không ghi lịch sử thật).

## 2. Thành phần

### 2.1 Catalog — `nfd_devtools::ai_leftovers::catalog`

Bảng marker trong code (như C9, vì cần logic mà rule pack không diễn tả được: phiên, git, "giữ bản mới nhất"). Mỗi mục: `id`, công cụ, phạm vi (`project` / `temp` / `global`), mẫu đường dẫn, mức rủi ro, `generic` (cần git xác nhận), `impact` EN + VI, `verified_on`, nguồn.

| ID | Công cụ | Ở đâu | Mức | Ghi chú (đã kiểm) |
|---|---|---|---|---|
| `claude-code.worktree` | Claude Code | `<repo>/.claude/worktrees/<tên>` | 🟢 khi sạch + đã merge/push; ngược lại 🟡 | `claude --worktree` và desktop app tạo ở đây (docs `code.claude.com/docs/en/worktrees`, máy tham chiếu) |
| `claude-code.session-temp` | Claude Code | `%TEMP%\claude\<dự án mã hóa>\<session-id>\` (`scratchpad\`, `tasks\`) | 🟢 | tên thư mục = session id của transcript |
| `playwright-mcp.output` | Playwright MCP | `.playwright-mcp\` trong dự án | 🟢 | mặc định của `--output-dir` (console-*.log, page-*.yml, ảnh chụp, trace) |
| `agent.screenshots` / `agent.tmp` / `agent.logs` | thư mục `.agent` (Antigravity & agent khác) | `.agent\screenshots`, `.agent\tmp`, `.agent\logs` | 🟢 khi git bỏ qua, 🟡 khi chỉ "không theo dõi" | `.agent\rules`, `.agent\workflows`, ghi chú… là nội dung người dùng → không chạm |
| `codex.project-log` | Codex | `<dự án>\.codex\log` (khi `CODEX_HOME` trỏ vào dự án) | 🟢 | `.codex\config.toml`, `hooks.json`, rules… là cấu hình → không chạm; `sessions`, `history.jsonl`… → 🔒 |
| `aider.tags-cache` | Aider | `.aider.tags.cache.v*` | 🟢 | cache repo-map, Aider tự dựng lại |
| `generic.test-results` / `playwright-report` / `screenshots` / `scratch` | agent nói chung | trong dự án (tới 3 cấp) | 🟢 git bỏ qua, 🟡 chỉ không theo dõi | không có git → không đề xuất |
| `playwright.old-browser` | Playwright | `%LOCALAPPDATA%\ms-playwright\<trình duyệt>-<revision>` | 🟢 | giữ revision **mới nhất** mỗi trình duyệt; `mcp-chrome-*` (profile đã đăng nhập của MCP) và thư mục lạ không bao giờ chạm |

**🔒 Giữ lại (hội thoại):** `.aider.chat.history.md`, `.aider.input.history`, `.aider.llm.history`, `.specstory\` (history), mọi `sessions` / `history.jsonl` / `memories` của `.codex`, `.claude\projects`, `.gemini\tmp`… ở **bất kỳ đâu** — hiện trong danh sách với lý do, không có ô chọn. Cursor / Windsurf / Kiro / Trae / Cline / Devin chỉ để **cấu hình do người dùng viết** trong dự án (`.cursor\rules`, `.windsurf\rules`, `.kiro\specs|steering`, `.trae\rules`, `.clinerules`) → không có gì để dọn; hội thoại của chúng ở `%APPDATA%` (đã 🔒 ở SafetyGate).

### 2.2 Mở rộng SafetyGate (nfd-core)

- `AGENT_HISTORY_NAMES` thêm `.aider.llm.history`.
- **`AGENT_HISTORY_SUFFIXES`** mới: cặp thành phần bị chặn ở mọi nơi (khi `CODEX_HOME` / `CLAUDE_CONFIG_DIR` / `GEMINI_CLI_HOME` bị dời): `.codex/{sessions, archived_sessions, history.jsonl, session_index.jsonl, memories, attachments}`, `.claude/{projects, sessions, history.jsonl}`, `.gemini/{tmp, history}`, `.qwen/tmp`, `.continue/sessions`, `.copilot/{session-state, history-session-state}`. `is_protected_template` dùng chung.
- `AGENT_HISTORY_TEMPLATES` thêm đường dẫn của bản Claude desktop dạng MSIX: `${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/{claude-code-sessions, local-agent-mode-sessions, side-session-notes, IndexedDB, Local Storage}` (+ rule 🔴 Keep tương ứng).
- Kiểm tra **con cháu**: gate từ vựng chỉ thấy chính đường dẫn; trước khi đề xuất một thư mục, AI Leftovers đi hết cây (không follow link, giới hạn số entry) — gặp tên/cặp ở trên → mục bị chặn `contains-history`; chạm giới hạn → `too-large`.

### 2.3 Nhận diện phiên — `ai_leftovers::sessions`

Chỉ **đọc** (các tệp này nằm trong vùng 🔒):
- **Claude Code CLI**: `~/.claude/projects/<dự án mã hóa>/<session-id>.jsonl` (hoặc `CLAUDE_CONFIG_DIR`): 64 KB đầu → `cwd`, `gitBranch`, thời điểm bắt đầu; 256 KB cuối → `custom-title` / `summary` / `agent-name`; `<session-id>/custom-title.json` nếu có; hoạt động cuối = mtime của tệp.
- **Claude desktop**: `%APPDATA%\Claude\claude-code-sessions\*\*\local_*.json` (+ bản MSIX trong `Packages\Claude_*`): `cliSessionId`, `cwd`, `title`, `createdAt`, `lastActivityAt`, `isArchived` (+ `archived-sessions.idx`). **"Còn mở" = chưa archive** — tiếp tục một phiên đã bị gỡ worktree sẽ hỏng.
- **Codex**: dòng đầu `~/.codex/sessions/**/rollout-*.jsonl` (`session_meta`: id, cwd, timestamp).

Gán leftover ↔ phiên: worktree ↔ phiên có `cwd` nằm trong worktree; scratchpad Temp ↔ session id trong tên thư mục; mục trong dự án ↔ phiên của dự án có khung thời gian [bắt đầu, hoạt động cuối] chứa lần ghi mới nhất của mục. **Giữ (không chọn sẵn)** khi phiên còn mở, phiên hoạt động < 7 ngày (Settings "giữ build output N ngày" dùng chung), hoặc chính mục mới được ghi < 7 ngày.

Dự án được xét = `cwd` của mọi phiên (worktree quy về repo cha) + thư mục gốc Repo Finder (`repoRoot`, quét nông tìm `.git`).

### 2.4 Worktree — `ai_leftovers::worktrees`

1. `git -C <repo> worktree list --porcelain` → path, HEAD, branch, `locked`, `prunable`. Chỉ xét worktree trong `<repo>\.claude\worktrees\`; thư mục ở đó mà git không biết → 🟡 "mồ côi", không chọn sẵn.
2. `git -C <wt> status --porcelain --untracked-files=all` không rỗng → 🟡 "có thay đổi chưa commit" + đề xuất **chỉ build output** của nó (C9, `repos::find`).
3. Nhánh mặc định = `refs/remotes/origin/HEAD` (fallback `main` / `master`); **đã merge** = `merge-base --is-ancestor HEAD <mặc định>` (local hoặc `origin/`); **đã push** = `for-each-ref --contains HEAD refs/remotes` có kết quả. Không merge, không push → 🟡 (squash-merge cũng rơi vào đây — an toàn).
4. `git ls-files --others --ignored --exclude-standard --directory` → các mục bị git bỏ qua (build output, `.env`…): mỗi mục qua SafetyGate + kiểm tra con cháu; bất kỳ mục nào bị chặn → 🟡.
5. **Gỡ** (mục 🟢 được chọn): (a) các mục bị bỏ qua vào **Quarantine** qua Executor (khôi phục 7 ngày); (b) kiểm tra lại ngay trước khi gỡ — vẫn sạch, vẫn merge/push, không còn mục bị bỏ qua (mục nào chuyển lỗi → **không** chạy `git worktree remove`, tránh git xóa vĩnh viễn); (c) `git worktree remove <path>` (không `--force`) rồi `git worktree prune`; (d) ghi Activity kind `worktree` (như lệnh C4: không khôi phục tự động) + audit log, kèm cách dựng lại: `git worktree add "<path>" <branch>` rồi Khôi phục thao tác quarantine. **Không bao giờ xóa nhánh.** Tệp được theo dõi bị git xóa nằm nguyên trong nhánh.

### 2.5 CLI

```
nfd ai-leftovers [list] [--root <dir>]... [--keep-recent 7] [--json]
nfd ai-leftovers clean [--root <dir>]... [--keep-recent 7] [--all-safe] [--apply] [--json]
```

Không `--apply` → chỉ in plan (dry-run). `clean` chỉ lấy mục 🟢 được chọn sẵn (phiên đã xong). Schema `nanofreedisk.ai-leftovers.v1`.

### 2.6 GUI

- Tab **AI leftovers** trong view Cache lập trình: nhóm **dự án → công cụ → phiên** (tiêu đề phiên, "hoạt động N ngày trước", nhãn "còn mở" / "gần đây"), mỗi mục: mức rủi ro, "Nếu xóa thì…", kích thước, lý do bị chặn; worktree hiện nhánh, merged/pushed, số thay đổi; mục 🔒 Giữ lại (hội thoại) không có ô chọn.
- Nút **"Dọn phiên đã xong (chỉ an toàn)"**, dialog tổng GB + danh sách + lệnh git sẽ chạy; tiến trình + Dừng dùng chung DevCache; Expert: lệnh `nfd ai-leftovers` tương đương.
- Health Score: finding `ai-leftovers` (≥ 10 GB −6, ≥ 2 GB −3, ≥ 500 MB −1; sửa ở tab này). Home: gợi ý "≈ X GB leftover của AI agent từ phiên đã xong".

## 3. Kiểm thử

- Unit: parse `worktree list --porcelain`, đọc transcript/desktop session/rollout mẫu, gán phiên, chọn revision Playwright mới nhất, `.agent` chỉ thư mục con đã biết.
- `proptest`: mọi đường dẫn hội thoại (template + tên + cặp, ở mọi độ sâu, hoa/thường) không bao giờ vào plan; thư mục chứa chúng bị chặn `contains-history`.
- Git thật (khi có git): worktree bẩn / có tệp chưa theo dõi / chưa merge không bao giờ bị gỡ; worktree sạch + merged được gỡ, build output vào quarantine, nhánh còn nguyên; thư mục có tệp git theo dõi không bao giờ đề xuất; tên chung chung không có git không đề xuất.
- Phiên hoạt động < 7 ngày / còn mở → không chọn sẵn.
- `gui-smoke`: fixture riêng (repo + worktree + Temp giả, `window.__nfdAiFixture`), không đụng dữ liệu thật.

## 4. Lát cắt

1. **Catalog + gate** — §2.1, §2.2, quét dự án / Temp / Playwright.
2. **Phiên** — §2.3.
3. **Worktree** — §2.4, thực thi + Activity/audit.
4. **UI + CLI** — §2.5, §2.6, locales, Health, Home, gui-smoke.

## 5. Ngoài phạm vi

- Tự xóa nhánh đã merge (chỉ hướng dẫn `git branch -d`).
- Dọn `workspaceStorage` của IDE cho dự án đã xóa (C7, v1.x).
- Hội thoại của bất kỳ công cụ nào — không bao giờ, kể cả khi người dùng chọn tay.

## 6. Rủi ro

- Định dạng transcript / session của desktop app đổi → chỉ mất phần "phiên" (mục vẫn liệt kê, không chọn sẵn khi không biết thời gian); đọc phòng thủ, không panic.
- `git worktree remove` xóa vĩnh viễn tệp bị bỏ qua → luôn chuyển chúng vào quarantine trước và kiểm tra lại; còn sót → không gỡ.
- Squash-merge làm worktree đã xong trông như "chưa merge" → 🟡, người dùng vẫn có thể dọn build output của nó.
