---
layout: default
title: VMCE 待办
lang: zh-CN
---

# VMCE 待办

此页是工作顺序和简要索引；总体边界、阶段停点及验收见[发展路线](vmce-roadmap.md)，具体问题、证据与历史记录见[审查详情](vmce-review.md)。开始前应核对当前代码和相关条目的状态。

## 当前路线

1. 上游同步审查：已完成。
2. 独立结算、重试和续关：审查修复已验证并本地提交（`b2806138e`、`570af8d90`），等待用户审查；双人胜利进下一关、失败重试、最后一关结束以及 `--vera-outcome win/lose` 诊断均通过，详见[基线](../vmce-baseline.md)。
3. 阶段 3 首个子链“结算统计所有权分离”：已完成，等待用户审查。VMCE 的结算统计改由自己的角色/分配投影，Cooperative 保留原版行选择，双方只共用 VERA 的 House 统计事实。
4. 阶段 3 第二个子链“大厅加入、退出及相关模式事件分派”：验收修复已完成，等待用户审查。共享大厅按原生模式选择分派模式事件，VMCE 事件进入自己的大厅槽位，不再调用 Cooperative 专属入口；选图窗口打开时按窗口选中模式处理的语义保持不变；C0 按消息自身的安装身份归属，早到文本暂存到其 HostSetup，旧/非 Cooperative 文本不进入 Cooperative 状态。
5. 下一步（阶段 3 其余部分）：大厅其余部分（难度所有权、其他 Cooperative 依赖）与任务中流程；每阶段验证并停下审查。最终整体验收补充三人、四人流程。
6. 整个 VMCE 独立后实现 LAN/本地任务中存档，随后接入互联网建房和开局。

独立阶段只修阻塞当前流程或验收的问题；审查通过后再处理该阶段相关的已知问题。下方条目保留作为缺陷索引，并非要求先全部清空。

## 阶段 2 审查修复（已验证，待用户审查）

- **重试交接改为三段式、以应用层确认为准：** 房主 Go → 客人 RetryAck；房主 RetryCommit → 客人应用后回新的应用层 `RetryCommitAck`（不立即重载），房主每 120 bucket 重发提交（丢失的传输确认或应用确认都由新提交修复），收齐后才发 `RetryStart` 并冻结重载；客人只在收到 `RetryStart` 时重载。可靠队列排空不再被当作对端已应用；提交总期限只在进入提交阶段时设置一次（600 bucket），重发、重复 Go/提交/开始都不会延长。
- **应用层存活检测：** 结算轮打开期间双方每 60 bucket 发 VMCE 心跳，客人收到首个结果消息前也开始监测；正常慢操作不会因等待而超时，600 bucket 无心跳才以 `VERA:VmcePeerUnresponsive` 有界退出。重试交接进行中由交接自身期限判定。
- **有界可见失败：** 首个结果消息前房主静默和发送队列拥塞的失败通知有针对性自动测试；离房保留通知的有界发送机会。网络故障时各端可能暂时处于不同阶段，真实 OS 级丢包及跨机器场景仍未验证。
- **B10 关间期限按键（`b2806138e`）：** 关间交接期限到期时按 Continue 不再无提示退出；按键与 poll 共用同一分配所有者，期限/入队/传输失败会停止回合、向对端排队 `RoundFailed` 并显示既有 `VERA:VmceRoundSendFailed`，退出经有界离房后提示。定向测试覆盖期限前、恰好到期、到期后按键、通知入队失败与对端不响应；回退可见失败处理时两条到期测试失败。
- **加载阶段：** VMCE 的开局/重试/续关网络加载在对端等待超时时不再按原版丢弃对端继续安装，而是经既有 VMCE 加载失败所有者以 `VERA:VmceLoadPeerTimeout` 取消并返回大厅；普通 Cooperative/原生加载保持原行为。
- **结果诊断拆分：** VMCE 实时结果诊断（含 `--vera-outcome` 注入/武装段）与比分采集 VMCE fixture 各自成为独立 VMCE 文件，无标记区域、不调用 Cooperative 结果流程；`--vera-outcome win/lose` 改为识别 VMCE 自己的 `0x103` 胜利页与 VMCE 失败页（不再等待 VMCE 从不打开的旧 `coop_score_page`），退出走 VMCE 的 Leave。
- **架构检查：** 扫描本阶段的 VMCE 正式代码、线报文、相关测试和诊断，并检查可解析的包装调用。源码检查不能推断所有接收者类型；多层同名方法包装、动态分发和生成代码仍须只读审查。
- 失败页 Esc 行为、结果页布局与现有胜利/重试/最终关流程、模拟和校验和保持不变。
- 证据：本轮 fast 构建的双人 `2p-win`、`2p-retry`、`2p-final` 及注入 `win/lose` 均通过；两端各有 300 条一致的 checksum。注入地图锁住了玩家输入，未验证移动/停止操作。普通 Cooperative 会话和 LAN 的相关自动测试通过；B10 续修的 fast 构建 `b10-continue-94eea03f6-r3` 的 `2p-win`/`2p-retry`/`2p-final` 也通过（各 300 条一致）。详见[基线](../vmce-baseline.md)。

## 阶段 3 首个子链：结算统计所有权（已完成，待用户审查）

- **所有者：** VMCE 的胜利、失败和关间统计由 [vmce_result/statistics.rs](../../src/app/vmce_result/statistics.rs) 独占：从冻结的 `VmceHouseAssignments`、地图角色元数据和 `raw_slot_index` 生成每名参战者一行，共享 House 不成行，重复 House 只计一次；击杀总数与轮次人数按去重后的角色主 House 计。
- **共享事实：** [sim/score.rs](../../src/sim/score.rs) 新增 `HouseScoreFacts` 与 `Simulation::house_score_facts(owner)`，只投影一个 House 的既有 `MatchStatistics`/`Economy`/`HouseTracking` 计数，不选择玩家行；`cooperative_score_rows` 只保留 Cooperative 自己的人类 House 行选择，不含 VMCE/模式分支。
- **共用展示：** `0x103` 战役比分模型移入中性的 [campaign_score_model.rs](../../src/app/campaign_score_model.rs)，离线战役出口与 VMCE 结果页共用；删除 `sim_tick` 中无调用者的重复 VMCE 比分模型和重复构建辅助。
- **行为保持：** 结果分派、轮次状态、终局计分锁存、Scenario 随机数、失败页实时统计、胜利页布局与交互不变；VMCE 终局快照仍不锁存，`Terminal` 来源继续回退实时事实。
- **架构检查：** `cooperative_score_rows` 从共享能力允许清单与引用上限删除；新增负向 fixture 拒绝 VMCE 统计调用它；新增语义边界检查断言 Cooperative 行选择不含 VMCE/模式分支、共享事实投影不选择行、VMCE 统计所有者不读取 Cooperative 行选择。guards 45 条两配置通过；`check -p vera20k --lib --bins` 两配置通过。
- **测试：** 核心 `sim::score` 13（默认）/15（vmce，含 VMCE 分配世界中 Cooperative 行仍按人类 House 选择的行为测试）以及恢复的空授权源 viewpoints 覆盖；VMCE 统计 7（多角色归属、共享 House 不计、重复去重、被动零值、终局优先、观察者不成行，另移入时间头测试）；`app::vmce_result` 25、`app::lan_service::vmce_round::tests` 20、`app::lan_service::tests` 29、普通 Cooperative session 回合 7。
- **证据：** fast 构建 `vmce-stats-570af8d90-r2`（SHA-256 `4407334D567700848E04B9EF88FE30201B5778F928E212BC50EE793DCD6D555C`）的 `2p-win`、`2p-retry`、`2p-final` 通过：各 300 个 native checksum 与 20 个模拟 hash 两端一致；下一关、同关重载、最终关进入 credits/`campaign-end` 回执符合预期；失败页两端 `vmce_statistics_rows` 完全一致，胜利页两端回执均为 `dialog.victory=true, rows=2`，最终关两端 `campaign-end` 回执一致，且每端诊断按自己的角色 House 事实校验统计行。证据在 `.scratch/vmce-stats-accept-r2/`（未提交）。
- **残余：** A19/B71 已按用户决定移入“用户接受的残余”，代码未修复；三人、四人真实结算统计与多角色联机流程未跑（多角色/去重归属仅由客户端单元测试覆盖）；跨机器、互联网与真实 OS 级丢包不在本子链范围。

## 阶段 3 第二个子链：大厅加入、退出与模式事件分派（验收修复完成，待用户审查）

- **实际边界：** 共享大厅不再把 VMCE 的模式事件送进 Cooperative 专属入口。新增 [shell_network_setup/dispatch.rs](../../src/app/shell_network_setup/dispatch.rs) 作为模式事件分派所有者：建房打开、加入槽位、每次选项解析刷新、加入被接受、离房被接受、模式进入、Use Map 和宾客选择国家按原生规则选择模式。加入和退出在 Choose Map 打开时使用窗口中选中的游戏类型（窗口中 VERA 自有的 Favorites/Online Maps 行不指向模式时没有槽位，不回退到房间模式），其余事件使用当前房间模式——保留原语义，而不是只判断“当前是否 Cooperative”。模式事件测试直接调用生产谓词表 `live_owner_for`（vmce 配置），而不是测试内的模拟谓词。
- **C0 的所属方（验收修复）：** 新增 [shell_network_setup/lobby_campaign.rs](../../src/app/shell_network_setup/lobby_campaign.rs) 按消息自身的安装身份决定归属：WOL 的 C0 位于房主选项载荷内，归属是该载荷的 `launch.mode`，只有 Cooperative 载荷的文本在同一 HostSetup 之前应用；LAN 的 C0 是独立的短选项包，proof 的 scope 是它跟随的选项修订，旧 epoch/低于下限的旧 revision 丢弃，超前或落在旧非 Cooperative 模式上的文本进入有界待处理队列，由下一次 HostSetup 的模式与 revision 下限决定应用或丢弃。`campaign_options_applied` 在完整 HostSetup 接受后、模式进入前运行，保留“C0 先于切入 Cooperative 的 HostSetup 到达时客人最终得到房主战役、关卡和国家”的行为，也覆盖正常顺序、乱序晚到、模式再次切换和离房清理。应用仍只经过 Cooperative 进度所有者的 `accept_received_campaign`，没有第二份战役状态。
- **发送与清理：** Cooperative 的 `publish` 只在已准入的 Cooperative 房间排队 C0；提交非 Cooperative 模式、离开/被踢或出队时清理待发 C0；LAN/WOL 发送队列在出队时再次核对房间，不能把旧 C0 借用下一修订的 proof 发给客人。
- **行为保持：** 房主/客人的加入、退出、选图、准备与开局流程不变；选图窗口的异步结果失效、开局复验与随机角色确定性未改；普通 Cooperative 的加入/退出/国家准入/模式进入/Use Map/C0 行为不变；VMCE/其他非 Cooperative 房间收到不适用或延迟 C0 时不调用 Cooperative 专属处理、不改变其战役状态。
- **架构检查：** `lobby_mode_events_dispatch_before_the_cooperative_owner` 不再强制无条件转发 C0，而是检查真实边界：`deliver` 同时处理 `CampaignOrigin::Options`/`CampaignOrigin::Lan`、按 scope/revision 暂存并用 `campaign_may_apply` 判断；`options_applied` 取回待处理文本、观察新 revision 并只在 Cooperative 模式应用；WOL 解码器携带载荷模式、LAN 接收器携带 proof scope；Cooperative 的 `publish` 与 LAN/WOL 发送队列有房间门。上述均有植入式负向 fixture（无条件转发、只按当前房间模式过滤、丢掉暂存、无条件发布），扩展模式优先检查保留。
- **测试：** [lobby_campaign.rs](../../src/app/shell_network_setup/lobby_campaign.rs) 9 条生产处理器测试（早到 C0 最终状态、WOL 同载荷早到、正常顺序、乱序晚到、VMCE/其他房间不应用、模式再次切换、离房清理、待发队列清理、同一修订重复应用），使用真实 `MatchSession`、`MultiplayerShellRuntime`、`CooperativeProgressState` 和 wire 事件身份，不直接调用 Cooperative 状态对象；LAN 接收的 scope 身份 1 条、WOL 载荷身份 1 条；dispatch 测试在 vmce 配置使用生产谓词表。错误实现回退验证：无条件转发、只按当前房间模式过滤、关闭晚到 revision 下限、重复应用推进下限分别使对应测试失败。`app::shell_network_setup` vmce 47/默认 33、`app::shell_wol::live_service` 55、`app::lan_service` vmce 98/默认 42、`app::frontend::skirmish_session::cooperative::tests` 30、architecture guards 46 条两配置、`check -p vera20k --lib --bins` 两配置通过。
- **证据：** fast 构建 `vmce-lobby-c0-c33dc84be-r2`（exe SHA-256 `1038e55c50d426a93664fdb9f251f655b4f246a660f48985dc61fab662ae139d`）的双人本机 `2p-win` 建房、加入、准备、开局通过（各 300 native checksum 与 20 个模拟 hash 一致，下一关回执符合预期）。证据在 `.scratch/vmce-lobby-c0-accept-r2/`（未提交）。
- **未覆盖：** 本仓库仍没有可构造的完整 `AppState` 夹具，`service_event` → `dispatch` → HostSetup 事务的整条胶水、以及生产 `AppCampaignVisit` 的字段映射（协议待处理箱 ↔ `lan_service.setup_scope()` ↔ 已准入模式）没有在单元测试内执行；测试覆盖同一生产处理器、真实 LAN scope/WOL 载荷事件身份、真实 `accept_network_host_setup` 事务及接收/解码边界，调用点由架构检查钉住。HostSetup 钩子在后续模式进入步骤之前解析待处理文本；若后续步骤失败，会话/scope 回滚但待处理箱下限和已应用战役不回滚（原生 C0 应用本就独立于 HostSetup 成功），为窄窗口后续项。在线大厅（WOL/relay）的 C0 先到顺序与 LAN 中途离房未实机复现；验收跑的是 VMCE 系列，本身不含 Cooperative C0，不能据此宣称 Cooperative 路径已实机验证。真实双进程房间内的中途离房、三人/四人大厅与跨机器、互联网未跑。用户接受残余见下方“用户接受的残余”一节（B59、B71、B72；共享所属方战绩保持现状）；正常玩家等待不得被误判为超时仍在“待验证”中，未修复。

## 优先核查

- 正常玩家等待不得被误判为超时仍是待处理事项（A40/B94）：结算轮已有应用层心跳，关间房间保活仍为残余；本子链未修复，也不宣称修复。
- 阶段 3 余下的大厅与任务中独立流程等待用户审查本子链后再开始。

## 玩家功能

- **共享控制：** 建筑维修/售卖、部署/卸载和自动维修的授权检查（B70、B75、B104）。
- **战役配置与角色：** 单角色房间容量、全部关卡的提前校验（B68、B05）。
- **进度与错误提示：** 进度文件读写失败、下一关解析失败、战役校验修复后重试，以及错误归因（B62、B78、B61、B14/B16）。
- **关卡身份与路径：** 长路径、跨平台分隔符、同义配置段、非 ASCII 文件名，以及旧轮次关卡/结算记录（B11、B81、B82、B85、B88、B92）。
- **结算与联机：** 控制消息队列满时的处理、对端静默检测（B69、B94）。
- **大厅人数与模式切换：** 随机角色不可见、超员布局和提示、切入战役时遗留 AI、任务简报查找及满员房间显示（B43、B60、B46、B73、B66、B100）。

## 用户接受的残余（不修，不额外兜底）

以下条目已由用户明确决定不修或不额外兜底，**不是待修项，也不代表代码已修复**：

- **B59 同国多所属方（A04）：** 玩家角色同国校验及国家归属歧义不修，不额外兜底。
- **B71 被动主 House 结算行与轮次人数（A19）：** 不修；VMCE 结算统计仍按原显示语义过滤被动 House。
- **B72 任务中地图被删除或替换（A20）：** 不额外兜底，不新增结果页恢复路径。
- **共享所属方战绩：** 保持现状；共享 House 不单独成行，其统计不并入角色行。

## 体验与维护

- **大厅展示：** 共享 House 未展示、单人提示、外交模式名、禁用按钮和客人预览（B86、B67、B63、B97、B99）。
- **大厅版面：** 简报过长、只读面板样式、简报区域和状态条布局（B45、B51、B52、B54）。
- **地图目录与配置：** 详情缓存、关间目录记录、越界控制角色和清单未知键（B40、B41、B87、B98）。
- **结算与演出：** 重试投票反馈、长名字截断、地图场景电影（B89、B90、B95）。
- **文案和工具：** 越南语 VMCE 词条、结算页采集 fixture 和验收工具零售标记（B101、B91、B93）。

## 待验证

- B10 已有有界投递与失败处理，重试交接改为应用层提交确认 + `RetryStart`，对端静默/丢包/部分提交/加载阶段超时均有自动化测试；关间期限到期按 Continue 的无提示退出已修复（`b2806138e`），期限前/恰好到期/到期后、通知入队失败与对端不响应有定向测试；真实 OS 级丢包、跨机器和关间未响应场景仍未验证。
- B94 结算页已加应用层心跳与静默检测（正常慢操作不计时）；关间房间的保活仍为残余。正常玩家等待不得被误判为超时仍是待处理事项，本子链未修复。
- B57 已实现结果确认后解锁；确认后立即离开及成员离开影响确认集合的边界仍需验证。
- B58 已实现 Ready/Go 请求级复验；真实双进程修改、恢复与旧扫描迟到仍需验证。
- B102 已确认现有规则协议与系列准入覆盖开局/关间/重试；房主侧原因提示、关间房间握手本身与真实数据变更采集仍为残余。
- B36：跨机器、关间加入/离开和其他真实网络故障未由本机多进程测试证明。
- 大厅模式事件分派（阶段 3 第二子链）：加入/退出与选图窗口的分派由 dispatch 测试（vmce 配置使用生产谓词表）和双人 `2p-win` 建房/加入/开局覆盖；C0 的所属方与早到/正常/乱序/模式切换/离房边界由 `lobby_campaign` 生产处理器测试及 LAN/WOL 事件身份测试覆盖，但完整 `service_event` → HostSetup 事务仍无 `AppState` 夹具；在线大厅实机顺序未复现。真实双进程房间内的中途离房、三人/四人大厅未跑。用户接受的残余（B59、B71、B72、共享所属方战绩）见上方专节；正常玩家等待超时边界见“优先核查”，未修复。

## 已完成

- **阶段 3 第二个子链（大厅加入、退出与模式事件分派，含验收修复）：** 共享大厅经 dispatch 所有者在原生语义下选择模式，VMCE 模式事件进入自己的槽位，不再调用 Cooperative 专属入口；选图窗口模式下按窗口选中模式处理的语义保留。验收修复后 C0 按消息自身的安装身份归属：WOL 用载荷模式，LAN 用 scope/revision 并暂存早到文本；旧房间/旧修订/非 Cooperative 文本不进入 Cooperative 状态，离房清理待处理项。双人建房/加入/准备/开局验收通过，残余见上方子链小节。
- **阶段 3 首个子链（VMCE 结算统计所有权）：** VMCE 统计改由自己的角色/分配投影，Cooperative 保留原版行选择，双方只共用 `HouseScoreFacts`；双人三流程验收通过，残余见上方子链小节。
- **B5、B106：** 核心层整理和战役数据从大厅分离。
- **B57：** 确认结果后保存本机关卡解锁；残余见待验证。
- **B10：** 终端 LAN 交接增加有界等待和失败处理；关间期限到期按 Continue 的无提示退出已修复（`b2806138e`）；残余见待验证。
- **B102：** 规则/扩展身份与战役系列准入由现有 owner 覆盖，未改协议；残余见[审查详情](vmce-review.md#已完成保留验证边界)。
- 其他历史完成项及测试范围保留在[审查详情的完成区](vmce-review.md#已完成保留验证边界)。
