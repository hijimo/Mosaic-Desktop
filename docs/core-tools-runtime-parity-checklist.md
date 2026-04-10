# `core/tools` 运行时语义对拍清单

> 更新日期: 2026-04-02
> 对比范围:
> - `Mosaic`: `/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools`
> - `codex-main`: `/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools`

## 结论先行

这份清单不是“工具定义是否对齐”的复述，而是下一阶段要逐项证明的运行时语义对拍范围。

当前可以确认：

- `Mosaic` 的 builtin tool 装配面已经能覆盖 `apply_patch`、shell/execution、文件工具、MCP resource、协作工具等主干能力，装配入口集中在 [spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L220)。
- 当前 `Session` 默认稳定暴露面已经切到 `exec_command` / `write_stdin` 路径，而不是旧 `shell` 默认；这个行为有测试覆盖，见 [session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L862) 和 [session.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/session.rs#L952)。
- `apply_patch` 已经补掉一个真实运行时缺口：审批通过后的 `Update/Move` 现在会真实落盘，而不是只发成功事件，见 [apply_patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/apply_patch.rs#L354) 和 [patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/patch.rs#L95)。

当前还不能确认：

- 每个工具在成功、失败、审批、重试、流式事件、边界条件下，是否与 `codex-main` 完全同语义。
- `Mosaic` 里仍有若干工具只做到“contract 对齐”或“主路径可跑”，还没有做到与 `codex-main` 的 runtime/orchestrator 行为完全一致。

## 统一对拍维度

除非某个工具天然不适用，否则每个工具都按以下六项验证：

1. 输入解析
2. 成功输出
3. 错误输出
4. 审批 / 沙箱 / 权限
5. 流式输出 / 事件时序
6. 边界条件

其中第 4 和第 5 项是最容易“定义一样、行为不一样”的位置。

## 工具总表

`Mosaic` 当前 `build_specs(...)` 会按配置装配下列 builtin / provider-native tools，见 [spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L220)。

状态说明：

- `[~]` 已开始对拍，有部分证据，但未完成
- `[ ]` 仅完成 contract/主路径，尚未做逐边界条件对拍
- `[!]` 已知仍存在明显运行时差距

### 执行与编辑

- [~] `apply_patch`
  来源: [apply_patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/apply_patch.rs), [patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/patch.rs), [codex-main apply_patch handler](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/handlers/apply_patch.rs#L105), [codex-main apply_patch runtime](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/runtimes/apply_patch.rs#L1)
  当前状态: direct path、approval request、`move_path`、approval resumed 落盘、direct/shell/exec 三入口失败链路、direct/shell/exec 审批拒绝终态都已有测试；`shell` / `exec_command` 的 warning 文案也已对齐上游；另外已确认 `NotApplyPatch` 在 `shell`/`exec_command` 入口会回退到普通命令执行而不是发 patch lifecycle，direct correctness error 也已补到“缺少 `*** End Patch`”和“缺少 file op”两个边界；本轮还补上了 `bash -lc "apply_patch <<'PATCH' ..."` heredoc 直连拦截、`cd ... && apply_patch <<'PATCH' ...` 的脚本内 `workdir` 语义、`shell_command` 对同类 heredoc / `cd` 组合的兼容覆盖，以及 Windows `cmd.exe /c ...` 和 PowerShell `-Command ...` 在 `shell` / `exec_command(shell=...)` 两条入口上的拦截与落盘语义；但 runtime/orchestrator 仍未与上游完全同层。
  对拍项: direct `apply_patch` 成功/失败/拒绝；`shell` / `exec_command` / `shell_command` 拦截成功/失败/拒绝；`auto_approved` 语义；`PatchApplyBegin` / `PatchApplyEnd` / `Error` 时序；warning 文案与触发时机；`CorrectnessError` / `NotApplyPatch` / `ShellParseError` 的入口级差异；move/update/delete/add 的终态一致性。

- [~] `exec_command`
  来源: [unified_exec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/unified_exec.rs)
  当前状态: 已接到 `UnifiedExecProcessManager`，并具备 `apply_patch` 拦截；本轮新增了 manager spawn failure 时错误文案的对拍证据，当前已从 `exec_command failed: {Display}` 收紧为上游同款 `exec_command failed: {Debug}`，即 `CreateProcess { ... }` 结构会直接暴露给模型侧；同时也补上了 `max_output_tokens` 截断证据，确认响应正文会附带 `[... truncated N tokens]` 标记；但审批/沙箱和更多 shell 选择细节仍未全量跑完。
  对拍项: `cmd` / `workdir` / `shell` / `tty` / `yield_time_ms` / `max_output_tokens` / `login` 解析；短命令一次性输出；长跑命令返回 `session_id`；非零退出码；stdout/stderr 聚合；approval denied / escalated；`apply_patch` 拦截优先级；超时；空 `cmd`；无效 `workdir`。

- [~] `write_stdin`
  来源: [unified_exec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/unified_exec.rs)
  当前状态: 已有 unified exec 会话管理；本轮已补上几条关键边界证据并跑绿：未知 session 会返回 `write_stdin failed: Unknown process id ...`，已退出 session 再次 poll 也会回到同样的 `UnknownProcessId`，空 `chars` poll 可以取回之前因短 yield 未收集到的延迟输出，`tty=false` 的长会话写 stdin 会返回 `StdinClosed`；同时也确认了 `max_output_tokens` 截断、空 poll 仍会发 `TerminalInteractionEvent(stdin="")`、以及第二次空 poll 不会重复返回已 drain 的旧输出；但更复杂的多轮 chunk 关联和更大输出量场景还没全量和上游对拍。
  对拍项: 合法 `session_id` 写入；空 `chars` 轮询； session 不存在； session 已退出；输出截断；交互事件顺序；多次连续调用一致性。

- [~] `shell`
  来源: [shell.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/shell.rs)
  当前状态: 有 classic shell handler，并兼容 `container.exec` / `local_shell` 别名；`apply_patch` 拦截链路已有覆盖。但与 codex-main 对比后已确认仍是“直跑子进程”实现，而不是上游统一 exec/orchestrator/runtime 链，因此 approval、sandbox、output formatting、env 注入、事件链和 local shell 深层语义都还未对齐。
  已确认差异:
  - 本地未接入上游 `run_exec_like` 的审批策略校验、`additional_permissions` 归一化、统一 ToolEmitter、ShellRuntime、环境注入与 orchestrator。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 正常返回值已改为 freeform 文本，格式为 `Exit code: ... / Wall time: ... / Output: ...`。
  - 超时返回值已改为 `Exit code: 124`，并包含 `command timed out after {timeout_ms} milliseconds`。
  - `sandbox_permissions=require_escalated` 在非 `OnRequest` 审批策略下会提前拒绝，文案与上游一致。
  - `with_additional_permissions` 缺少 `additional_permissions` 时的报错文案已对齐。
  - 单独传 `additional_permissions` 而未开启 `with_additional_permissions` 时的报错文案已对齐。
  - router 层已新增定点对拍，确认 `local_shell` 与 `container.exec` 两个别名都能真正路由到 `ShellHandler`，并返回与 `shell` 主入口同形态的 freeform 输出，而不是只在 `matches_kind()` 上“声明支持”。
  对拍项: 参数数组解析；approval/sandbox；stdout/stderr；exit code；超时；别名入口是否完全同语义；`apply_patch` 拦截；`container.exec` / `local_shell` 告警与输出兼容；解析错误文案；output formatting。

- [~] `shell_command`
  来源: [shell_command.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/shell_command.rs)
  当前状态: 已实现；本轮已有 `apply_patch <<'PATCH' ...` 审批事件链路，以及 `cd ... && apply_patch <<'PATCH' ...` 对脚本内 `workdir` 的测试证据。但与 codex-main 对比后已确认，当前仍未走上游 `run_exec_like` 主链，输出格式、超时语义、审批/权限、shell snapshot、env 注入和 runtime backend 还有明显差距。
  已确认差异:
  - 本地直接返回结构化 `CommandOutputEnvelope`；上游 `shell_command` 面向模型侧是 freeform 文本，格式形如 `Exit code: 0\nWall time: ... seconds\nOutput:\n...`。
  - 本地超时语义仍是 `exit_code = -1` + `"command timed out"`；上游测试明确要求 `Exit code: 124` + `command timed out after {timeout} milliseconds`。
  - 本地尚未接入上游的 approval policy guard、`additional_permissions` 校验、ToolEmitter begin/finish、ShellRuntime Classic/ZshFork 真实 backend、shell snapshot 与依赖环境注入。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 正常返回值已改为 freeform 文本，格式为 `Exit code: ... / Wall time: ... / Output: ...`。
  - 超时返回值已改为 `Exit code: 124`，并包含 `command timed out after {timeout_ms} milliseconds`。
  - `sandbox_permissions=require_escalated` 在非 `OnRequest` 审批策略下会提前拒绝，文案与上游一致。
  - `with_additional_permissions` 缺少 `additional_permissions` 时的报错文案已对齐。
  - 单独传 `additional_permissions` 而未开启 `with_additional_permissions` 时的报错文案已对齐。
  对拍项: `command` 字符串执行；登录 shell；approval/sandbox；超时；命令拼接与 quoting；`apply_patch` 拦截；非 0 返回；空命令；Classic / ZshFork 分支差异；解析错误文案；freeform output formatting。

### 文件与搜索

- [~] `list_dir`
  来源: [list_dir.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/list_dir.rs)
  当前状态: 主路径已实现，但未全量对拍排序、隐藏文件、路径越界与所有错误文案。本轮已补上与 codex-main 一致的解析错误前缀测试。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 成功返回值已收敛为纯文本字符串，不再带本地 `{ content: ... }` envelope；同时移除了本地额外的 `Absolute path: ...` 头部，和 codex-main 当前输出形状一致。
  - `offset exceeds directory entry count`、depth 遍历、排序分页、长文件名截断、目录/符号链接标记已有本地单测基础。
  对拍项: `path` 解析；返回排序；文件/目录标记；不存在路径；非目录路径；权限错误；大目录截断；相对路径与 cwd 解析；错误文案逐字对齐。

- [~] `read_file`
  来源: [read_file.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/read_file.rs)
  当前状态: 主路径已实现，但还未逐项对拍 offset/limit、超大文件、路径安全与错误结构。本轮已补上与 codex-main 一致的解析错误前缀测试。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 成功返回值已收敛为纯文本字符串，不再带本地 `{ content: ... }` envelope，输出形状与 codex-main 的 `L{line}: ...` 文本一致。
  - `offset exceeds file length`、`max_lines must be greater than zero`、indentation block/siblings/Python sample/blank-line effective indent 等已有本地单测基础。
  对拍项: `path` / `offset` / `limit` 解析；正常读取；空文件；不存在文件；目录路径；超大文件截断；编码异常；路径越界；错误码与消息；indentation 细粒度边界。

- [~] `grep_files`
  来源: [grep_files.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/grep_files.rs)
  当前状态: 已实现 ripgrep/内建 fallback。本轮已补上与 codex-main 一致的解析错误前缀测试，并修正了相对 `path` 必须基于 tool context `cwd` 解析的运行时语义。仍未对拍 include/filter/limit/无命中/rg 缺失时完整语义。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 相对 `path` 不再错误依赖宿主进程当前目录，而是和上游一样基于 invocation context 的 `cwd` 解析。
  - 成功命中时返回纯文本路径列表、无命中时返回 `No matches found.` 的主路径形状与上游方向一致。
  对拍项: `pattern` / `include` / `path` / `limit`；命中格式；无命中；非法正则；不存在路径；`rg` 缺失时报错语义是否要移除本地 fallback；大结果截断；二进制文件处理。

### MCP 与外部工具

- [~] `list_mcp_resources`
  来源: [mcp_resource.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/mcp_resource.rs)
  当前状态: 已接到真实 manager，本轮已补上与 codex-main 一致的解析错误前缀测试。仍未做 server 不可用、空列表、过滤条件等边界全量对拍。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - router 与 tool handler 现有测试已覆盖“有 manager 时读真实资源列表”和“无 manager 时返回空列表 payload”的主路径。
  对拍项: 空/单 server 列表；错误 server；连接缺失；返回结构；稳定排序；错误透传；cursor 边界。

- [~] `list_mcp_resource_templates`
  来源: [mcp_resource.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/mcp_resource.rs)
  当前状态: 同上，本轮已补上与 codex-main 一致的解析错误前缀测试。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - tool handler 现有测试已覆盖“有 manager 时读取模板列表”的主路径，router 测试覆盖了“无 manager 时返回空模板列表 payload”。
  对拍项: 模板枚举；空结果；server 不存在；错误透传；返回字段一致性；cursor 边界。

- [~] `read_mcp_resource`
  来源: [mcp_resource.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/mcp_resource.rs)
  当前状态: 已接通真实读取，本轮已补上与 codex-main 一致的解析错误前缀测试。仍缺少 uri 错误、server 不可用、内容 envelope 的全量对拍。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - tool handler 现有测试已覆盖“有 manager 时读取资源内容”的主路径；router 测试覆盖了“无 manager 时返回 requires MCP connection manager 错误”。
  对拍项: `server` / `uri` 解析；文本/二进制资源；不存在资源；server 不存在；错误结构；大内容处理。

- [~] `search_tool_bm25`
  来源: [search_tool_bm25.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/search_tool_bm25.rs)
  当前状态: 本轮已把对模型侧的成功输出形状收敛到与 codex-main 同款 JSON 文本字符串，而不是本地结构化 `serde_json::Value`；同时补上了解析错误、无 manager、空索引、基础命中查询的定点测试。仍未完成 selection merge、connector/filter、排序稳定性和更深运行时语义对拍。
  已验证:
  - 参数解析错误前缀已改为 `failed to parse function arguments: ...`，与上游一致。
  - 无 `mcp_manager` 时，返回值已改为 JSON 文本字符串，payload 为 `{query,total_tools,active_selected_tools,tools}`。
  - 空索引时同样返回 JSON 文本字符串，`total_tools=0` 且 `tools=[]`。
  - 基础命中查询会返回 JSON 文本字符串，结果项包含 `name/server/title/description/connector_name/input_keys/score`。
  - router 对拍已覆盖 `search_tool_bm25` 打开时的主路径，并确认 `ToolRouter` 返回的也是 JSON 文本字符串，而不是对象值。
  对拍项: 查询命中；排序稳定性；空 query；空索引；server/tool 元数据缺失；结果字段；分页/limit。

- [~] `mcp__<server>__<tool>` 动态 MCP tools
  来源: [router.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/router.rs), [mcp.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/mcp.rs)
  当前状态: 路由已接通，本轮补上了“已注册测试结果时成功透传响应 envelope”以及“server 未注册时错误透传”的定点证据。仍未做 tool selection filtering、断连/超时和更多 MCP 错误路径的全量对拍。
  已验证:
  - `mcp__filesystem__read_file` 这类 qualified name 会从 router 正确解析为 `(server, tool)` 并委托给 `McpConnectionManager::call_tool(...)`。
  - 注册测试结果时，router 会原样返回 manager 的 JSON envelope。
  - server 未注册时，错误会透传为 `MCP server '{server}' not registered`，错误码为 `McpServerUnavailable`。
  对拍项: 路由命中；请求透传；响应 envelope；server/tool 不存在；禁用 selection；MCP 错误透传；超时/断连。

- [~] `dynamic tools`
  来源: [dynamic.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/dynamic.rs), [router.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/router.rs)
  当前状态: 注册/路由主路径已接通，本轮补上了 `RouterNestedToolExecutor` 两个关键错误路径，与当前 codex-main 编排层预期一致：缺少 dynamic handler 的上下文拒绝，以及未知工具统一 not-found。仍缺取消、失败回传、schema 差异和事件时序的全量对拍。
  已验证:
  - 已有 handler 单测覆盖注册/注销、未注册工具、request/response 生命周期、unknown call id。
  - `RouterNestedToolExecutor` 在没有 `dynamic_tool_handler` 时会报 `dynamic tool '{tool_name}' cannot be invoked in this context`。
  - `RouterNestedToolExecutor` 对未知工具会统一报 `no handler found for tool: {name}`。
  对拍项: 注册/注销；路由命中；未注册错误；返回 envelope；调用失败；同名冲突；在不同上下文是否可调用。

### 交互与协作

- [~] `request_user_input`
  来源: [request_user_input.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/request_user_input.rs)
  当前状态: 已与 codex-main 对齐并验证 mode availability 文案、tool description 文案、取消文案、`options` 必填报错、事件 schema 归一化，以及 `is_other=true` 强制语义；真实等待/恢复链路也已打通。仍缺超时、重复答复、active turn 缺失后的全链路对拍。
  已验证:
  - `request_user_input is unavailable in {Mode} mode` 文案与默认模式开关一致。
  - tool description 在 `Plan` / `Default or Plan` 两种配置下与上游逐字一致。
  - 取消时固定返回 `request_user_input was cancelled before receiving a response`，不再带本地 request id。
  - 每个问题都要求非空 `options`，报错文案与上游一致。
  - 事件 `schema.questions[*].options` 会把 legacy string 归一化成 `{ label }`，结构化 option 保留 `description`。
  - 事件里的 `is_other` 会被强制设为 `true`，即使调用方显式传了 `false`。
  对拍项: schema 校验；事件发出；`Op::UserInputAnswer` 恢复；无答案超时；重复提交；plan/default 模式限制；结构化 options 必填校验；active turn 缺失；错误返回。

- [~] `update_plan`
  来源: [plan.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/plan.rs)
  当前状态: 已对齐上游的 Plan mode 禁用语义、payload 解析错误前缀、事件强类型结构、以及返回值 `"Plan updated"`。本地原先额外拦截“多个 in_progress”已移除，改为按上游直接透传。仍缺事件消费侧、空 plan、重复 step、状态迁移顺序的全量对拍。
  已验证:
  - `update_plan is a TODO/checklist tool and is not allowed in Plan mode` 与上游一致。
  - malformed payload 失败前缀改为 `failed to parse function arguments: ...`。
  - `PlanUpdate` 事件从原始 JSON 变为结构化 `explanation + plan[{step,status}]`，其中 `status` 为 `pending | in_progress | completed`。
  - 工具返回值与上游一致，为纯文本 `"Plan updated"`，不再是本地 JSON envelope。
  - 多个 `in_progress` 步骤不会在 handler 层被额外拒绝，而是照常透传为事件。
  - router 对拍夹具已补齐事件通道，确认从 `ToolRouter` 入口调用 `update_plan` 时会成功返回 `"Plan updated"` 并真实发出 `PlanUpdate` 事件，而不是因为测试上下文缺少消费者而伪失败。
  对拍项: plan payload 校验；事件顺序；Plan mode 禁用；多步状态迁移；重复 step；空 plan；错误消息；前端消费语义。

- [~] `spawn_agent`
  来源: [multi_agents.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/multi_agents.rs)
  当前状态: 已对齐为 JSON 文本返回，与 `codex-main` 同样返回 `{"agent_id","nickname"}`；parse error 前缀、空消息、`message/items` 互斥、深度限制与基础成功路径已有定点测试。仍缺 `fork_context`、角色配置继承、事件时序和真实 turn/runtime 级对拍。
  对拍项: 基本创建；`fork_context`；`items` 与 `message`；模型/推理参数；权限不足；错误 envelope。

- [~] `send_input`
  来源: [multi_agents.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/multi_agents.rs)
  当前状态: 已对齐为 JSON 文本返回，成功时返回 `submission_id`；`message/items` 校验和未知 agent 错误文案已收紧到更接近上游。仍缺 `interrupt` 真正打断语义、排队事件和 submission 生命周期对拍。
  对拍项: 正常发送；`interrupt`；目标不存在；items/message 兼容；排队后状态。

- [~] `resume_agent`
  来源: [multi_agents.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/multi_agents.rs)
  当前状态: 已补成功路径与 JSON 文本 `{"status": ...}` 返回测试；未知 agent 错误映射也已统一。仍缺“已运行 agent”“跨 turn 恢复”和事件 begin/end 时序对拍。
  对拍项: 正常恢复；无效 id；已运行 agent；恢复后继续 `send_input` / `wait`。

- [~] `wait`
  来源: [multi_agents.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/multi_agents.rs)
  当前状态: 已接受 `ids` / `agent_ids` 双字段，`timeout_ms <= 0` 错误文案与短超时 clamp 行为已有测试；返回值已改成上游同类 JSON 文本 `{"status": {...}, "timed_out": bool}`。仍缺多 agent 先返回者、completed/errored 终态明细与事件链对拍。
  对拍项: 单/多 target；timeout；先返回者；空结果；完成态 envelope。

- [~] `close_agent`
  来源: [multi_agents.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/multi_agents.rs)
  当前状态: 已改为返回关闭前状态的 JSON 文本快照，并补了基本关闭成功路径测试。仍缺递归关闭子代理、重复关闭和真实 runtime 生命周期对拍。
  对拍项: 正常关闭；递归关闭子代理；已关闭重复调用；返回前态。

- [~] `spawn_agents_on_csv`
  来源: [agent_jobs.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/agent_jobs.rs)
  当前状态: 已从纯 stub 推进到最小可运行 runtime：可以读取 CSV、校验 header/空 instruction、渲染 `{column}` 模板、生成去重 `item_id`、返回上游同类的字符串化 JSON 结果，并导出 `item_id/source_id/status/result_json/error` CSV。当前实现仍是进程内 job store，同步完成，不具备上游那套真实 sub-agent 调度、长时运行、进度事件和 sqlite job runtime。
  已验证:
  - parse error 前缀保持 `failed to parse function arguments: ...`
  - `instruction must be non-empty` 与上游一致
  - 成功路径返回字符串化 JSON，字段包含 `job_id/status/output_csv_path/total_items/completed_items/failed_items/job_error/failed_item_errors`
  - 成功路径会真实写出 output CSV，且结果 JSON 中包含渲染后的 row/instruction 内容
  对拍项: CSV 输入校验；批量派发；部分失败；结果汇总；并发控制；实际子任务落地。

- [~] `report_agent_job_result`
  来源: [agent_jobs.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/agent_jobs.rs)
  当前状态: 已从纯 stub 推进到最小可运行 runtime：会校验 `result` 必须是 JSON object，按 `job_id/item_id/turn_id` 在进程内 job store 中回写结果，并返回上游同类的字符串化 JSON `{\"accepted\":bool}`。当前仍未接入上游 sqlite runtime、worker thread 身份、stop/cancel 全链路及真实 job aggregation。
  已验证:
  - parse error 前缀保持 `failed to parse function arguments: ...`
  - 非 object `result` 会返回 `result must be a JSON object`
  - turn id 匹配时返回 `{"accepted":true}`，不匹配时返回 `{"accepted":false}`
  对拍项: job id / worker id / result envelope；找不到 job；重复上报；聚合状态更新。

### 辅助工具

- [~] `js_repl`
  来源: [js_repl.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/js_repl.rs)
  当前状态: 本地已接入持久 Node 内核，状态持久化、freeform pragma、内嵌 `codex.tool(...)`、动态工具调用、递归调用拒绝、超时后恢复等路径已有测试；但与上游仍有可观察差异，尤其是 function/custom payload 形态、成功输出从 `content_items/text` 简化成 exec 风格 JSON，以及 feature gate/事件细节还未完全同语义。
  对拍项: freeform 输入；持久上下文；模块导入；stdout/stderr；reset 后状态；语法错误；运行时错误。

- [~] `js_repl_reset`
  来源: [js_repl.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/js_repl.rs)
  当前状态: reset 主路径已可用，本轮把成功返回值收紧为上游同款 `"js_repl kernel reset"`；仍缺 feature gate 与 manager 缺失场景的全量对拍。
  对拍项: reset 成功；无现存 session；reset 后上下文清空；错误返回。

- [~] `presentation_artifact`
  来源: [presentation_artifact.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/presentation_artifact.rs)
  当前状态: 已从纯 stub 推进到最小真实文件 runtime：在保留 parse error / path traversal / `ctx.cwd` 授权的前提下，支持 `create/read/update/list/delete` 五个动作，并返回字符串化 JSON 结果。当前仍不是上游的 `artifact_id + actions[] + snapshot/export/import` 体系，只是“文件级最小可运行子集”。
  已验证:
  - parse error 前缀保持 `failed to parse function arguments: ...`
  - `..` traversal 会在 runtime 前被拒绝
  - 相对路径授权基于 invocation context `cwd`，不是宿主进程 cwd
  - `create/read/update/list/delete` round-trip 已有测试，router 主路径也能成功返回字符串化 JSON
  对拍项: 输入 schema；文件落盘；路径限制；读取/更新；错误路径；feature gate。

- [~] `test_sync_tool`
  来源: [test_sync.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/test_sync.rs)
  当前状态: 成功返回值已收紧成上游同款纯文本 `"ok"`；parse error 前缀、timeout=0、participants=0、barrier timeout 与 barrier participant mismatch 语义已和上游实现逐项比对。仍缺多调用并发 rendezvous 的端到端验证。
  对拍项: barrier 注册；参与者计数；超时；重复 barrier；错误返回。

- [~] `view_image`
  来源: [view_image.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/view_image.rs)
  当前状态: 已对齐相对路径按 `ctx.cwd` 解析、缺文件/目录错误、text-only model 拒绝、带 `call_id` 的事件发射；本轮还补了非图片文件返回 placeholder `input_text`，不再把任意本地文件伪装成图片。仍缺上游对真实 MIME 探测、缩放/截断和 content item 精细形态的全量对拍。
  对拍项: 正常图片读取；不存在路径；非图片；大文件；本地路径限制；返回结构。

- [~] `web_search`
  来源: [spec.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/spec.rs#L356)
  当前状态: provider-native spec 已进入 tool surface；`Cached/Live/Disabled`、`external_web_access`、显式配置与 tools toggle 优先级、DangerFullAccess 下 cached→live 提升、以及 turn constraints 注入/移除都已有 session 级测试。仍缺与上游更多 profile/model 限制联动的对拍证据。
  对拍项: `Cached/Live/Disabled` 暴露；`external_web_access` 标记；session 配置优先级；与 model constraints 的合并结果。

## `apply_patch` 专项差异

这是当前最接近“可观察语义已逼近上游”的工具，但也最容易被误判为“已经完全持平”。

### 已经补齐的关键点

- `shell` / `exec_command` 入口拦截 `apply_patch` 后，会走统一 lifecycle，而不是各自直接执行补丁，见 [apply_patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/apply_patch.rs#L283)。
- approval required 时会发 `ApplyPatchApprovalRequest` 并返回 `ApprovalDenied`，见 [apply_patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/apply_patch.rs#L367)。
- approval resumed path 现在会真实落盘 `Update` / `Move`，见 [patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/patch.rs#L164)。
- 失败路径现在已有 direct / `shell` / `exec_command` 三入口测试，均验证 `PatchApplyBegin -> PatchApplyEnd(status=Failed) -> Error`，并确认 `shell` / `exec_command` 额外带 warning。
- direct / `shell` / `exec_command` 经审批后若用户 Denied，当前已验证只会补一个 `PatchApplyEnd(status=Declined)`，不会重复发 `PatchApplyBegin`，且 stderr 已对齐为 `patch rejected by user`。
- `shell` / `exec_command` 入口触发时的 warning 文案已对齐 `codex-main` 当前字面值。
- 已确认 `NotApplyPatch` 在 `shell` / `exec_command` 拦截里会 `Ok(None)` 回退给原执行器，不会发 warning 或 patch lifecycle；这和上游 `intercept_apply_patch(...)` 的分流方向一致。
- direct `apply_patch` 的 correctness error 目前至少已覆盖两个边界: 缺少 `*** End Patch`、以及只有 patch envelope 但没有任何 file operation。
- 已补上 `bash -lc "apply_patch <<'PATCH' ... PATCH"` 这种 heredoc 直连主路径的拦截测试，`shell` 和 `exec_command` 现在都能在 approval required 场景下发出 warning + `PatchApplyBegin` + `ApplyPatchApprovalRequest`。
- 已补上 `shell` / `exec_command` 在拦截 `apply_patch` 时对显式 `workdir` 的尊重，避免 patch 仍然错误落在 turn cwd。
- 已补上 `cd nested && apply_patch <<'PATCH' ... PATCH` 这种脚本内改 cwd 的主路径测试，`shell` / `exec_command` 现在都会把 patch 落到脚本选择的新目录，而不是 turn cwd。
- `shell_command` 这条兼容入口也已验证两点：heredoc 直连时会发 warning + `PatchApplyBegin` + `ApplyPatchApprovalRequest`；脚本内 `cd ... && apply_patch <<...` 也会尊重 script workdir。
- `shell_command` 现在还多了两条证据：patch 失败时会发 `Warning -> PatchApplyBegin -> PatchApplyEnd(Failed) -> Error`；坏掉的 heredoc 会 fallback 给原执行器，并且不会发 patch lifecycle 事件。
- Windows `cmd.exe /c` 变体已开始追平：`shell` 入口现在会拦截 `cmd.exe /c "cd nested && apply_patch <<..."`
  并按脚本里的 `cd` 落盘；`exec_command` 在显式 `shell: "cmd.exe"` 时也不再硬编码 `-c`，而会按 `cmd.exe /c` 组装并进入同一拦截逻辑。
- PowerShell 变体也已补齐主路径：`shell` 入口现在会拦截 `powershell.exe -Command "cd nested && apply_patch <<..."`
  并按脚本里的 `cd` 落盘；`exec_command` 在显式 `shell: "powershell.exe"` 或 `pwsh` 时，会按 `-Command` 组装并进入同一拦截逻辑。
- 已补上“坏掉的 heredoc”可观察语义：当 shell script 以 `apply_patch <<'PATCH' ...` 起头但 heredoc 不完整时，`shell` / `exec_command` 当前都会回退给原执行器，并且不会发 warning / `PatchApplyBegin` / `PatchApplyEnd` 等 patch lifecycle 事件；这与上游 `ShellParseError -> Ok(None)` 的外显分流方向一致。
- 对于 `*** Update File: missing.txt` 这类“更新缺失文件”的失败样本，direct / `shell` / `exec_command` / `shell_command` 现在都已收紧到同一个逐字 stderr：
  `Failed to read file to update missing.txt: No such file or directory (os error 2)`；
  对应的 `Error.message` 也统一为 `patch application failed: ...`。
- 对于 `echo foo && apply_patch <<'PATCH' ...` 这类“前面还有别的命令”的脚本，`shell` / `exec_command` / `shell_command` 现在都已验证会 fallback 给原执行器，不会误发 patch lifecycle；这与上游 whole-script 严格匹配的方向一致。

### 仍然不是完全同语义的地方

- `codex-main` 的 `apply_patch` 是“handler 做验证与分流，runtime/orchestrator 做审批、sandbox、retry、自调用执行”，见 [codex-main apply_patch handler](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/handlers/apply_patch.rs#L119) 和 [codex-main apply_patch runtime](/Users/zhaojimo/Downloads/codex-main/codex-rs/core/src/tools/runtimes/apply_patch.rs#L49)。
- `Mosaic` 仍主要是“handler + `core/patch.rs` 本地落盘”模型；虽然有 [handlers/runtimes/apply_patch.rs](/Users/zhaojimo/Documents/git/Mosaic-Desktop/src-tauri/src/core/tools/handlers/runtimes/apply_patch.rs)，但当前真实主链并不等同于 `codex-main` 的 orchestrator/runtime 执行模型。
- 因此现在更准确的说法是：“`apply_patch` 的可观察主链语义更接近了”，而不是“runtime 分层和 sandbox/retry 语义已完全追平”。

### 下一批必须补的测试

1. resumed approval path 的 `auto_approved` 含义，是否与 `codex-main` 对“之前已审批、当前正在恢复执行”的定义一致。
2. `CorrectnessError` / `NotApplyPatch` / 上游 `ShellParseError` 在 direct / `shell` / `exec_command` 三入口的错误类型与文案是否完全一致。
3. `ShellParseError` 在 heredoc 缺失闭合、脚本以 `apply_patch` 起头但体不完整时，是否与上游保持同样的“回退还是报错”分流。
4. `shell` / `exec_command` / `shell_command` 入口触发时，warning 触发时机是否在所有边界条件下都与上游一致。
5. patch 执行失败时的 stderr 文案格式，是否在 direct / `shell` / `exec_command` / `shell_command` 四入口与上游逐字一致。

## 执行顺序建议

如果下一步要继续做“运行时语义对拍”，优先级建议如下：

1. `apply_patch`
2. `exec_command` / `write_stdin`
3. `shell` / `shell_command`
4. `request_user_input` / `update_plan`
5. `read_file` / `list_dir` / `grep_files`
6. `mcp_resource` / MCP-qualified tools / dynamic tools
7. `js_repl` / `presentation_artifact` / `agent_jobs`

原因很简单：

- 前四组决定主链工具行为是否真的接近 `codex-main`
- 中间两组决定文件与 MCP 能力的边界条件是否稳定
- 最后一组目前仍有明显实现缺口，先做全量对拍只会重复证明“没实现完”
