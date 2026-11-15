---
layout: default
title: VMCE 待办
lang: zh-CN
---

# VMCE 待办

此页是工作顺序和简要索引；总体边界、阶段停点及验收见[发展路线](vmce-roadmap.md)，具体问题、证据与历史记录见[审查详情](vmce-review.md)。开始前应核对当前代码和相关条目的状态。

## 当前路线

1. 上游同步审查：0.3.3 同步已完成并验证（`78b0aef4a`），详见[基线](../vmce-baseline.md)。
2. 独立结算、重试和续关：审查修复已验证并本地提交（`b2806138e`、`570af8d90`），等待用户审查；双人胜利进下一关、失败重试、最后一关结束以及 `--vera-outcome win/lose` 诊断均通过，详见[基线](../vmce-baseline.md)。
3. 阶段 3 首个子链“结算统计所有权分离”：已完成，等待用户审查。VMCE 的结算统计改由自己的角色/分配投影，Cooperative 保留原版行选择，双方只共用 VERA 的 House 统计事实。
4. 阶段 3 第二个子链“大厅加入、退出及相关模式事件分派”：验收修复已完成，等待用户审查。共享大厅按原生模式选择分派模式事件，VMCE 事件进入自己的大厅槽位，不再调用 Cooperative 专属入口；选图窗口打开时按窗口选中模式处理的语义保持不变；C0 按消息自身的安装身份归属，早到文本暂存到其 HostSetup，旧/非 Cooperative 文本不进入 Cooperative 状态。
5. 阶段 3 第三个大厅子链“共享大厅难度所有权”：已完成，等待用户审查。原生进程行 `[0xA8EB68]` 由共享大厅所有者持有并类型化，LAN 载荷保留原生字段名 `cooperative_difficulty`；VMCE 在开局冻结时取一次，加载 TriggerType 与结算页只读冻结值，不再读实时大厅行；Cooperative 读取同一所有者，行为不变。
6. 阶段 3 第四个子链“房间队伍席位策略所有权”：已完成，等待用户审查。共享房间判断“本模式是否按队伍席位分组”原先用 `uses_cooperative_setup()` 反问 Cooperative，现移入模式自己的 `ModePolicy.room_teams`：普通模式分组，Cooperative 与 VMCE 不分组；setup 视图中重复的模式判断已删除，玩家可见行为不变。
7. 阶段 3 第五个子链“开始许可归 VMCE”：已完成，等待用户审查。共享开始命令只保留一次 VMCE 分派并显示 VMCE 给出的拒绝原因；VMCE 自己的大厅模块拥有开始许可（模式运行时可用性、选图、LAN、地图配置、共享种子、地图角色解析）与最低人数规则；迁移桥构建继续以“无可用 VMCE 后端”拒绝开局。
8. 下一步（阶段 3 其余部分）：大厅余下的 Cooperative 依赖与任务中流程；每阶段验证并停下审查。最终整体验收补充三人、四人流程。
9. 按实际依赖推进“取消 VMCE 游戏行为编译期门控”迁移：首条“入口至开始许可”链已有候选实现、聚焦回归和审查修复后的本机双人生产验收。旧版及混合版本仍待验证；参照路线 G0–G5。
10. 门控迁移完成后实现 LAN/本地任务中存档，再按其决定接入互联网建房和开局。
11. 当前阶段验收误报修复与角色规则所有权收尾（2026-11-15）：已完成，待用户审查。重试统计行按冻结 `lan.startup.sources` 校验完整性与唯一性，比较 `retry_mission.seed`，允许缺失时仍比较所有实际存在的哈希；角色列表、选择与换图归 VMCE setup，`MatchSession` 保留模式/取值/重复占用的状态不变量，删除只转发的包装层。证据见[基线](../vmce-baseline.md)。
12. 下一条 Ready/GO 边界链调查与 A03 收尾（2026-11-15）：未发现 VMCE 在该链上依赖 Cooperative 专属规则；选项修订失效、Ready/MapStatus 和 GO 前复验分别由共享会话/`lan_service/vmce_setup` 边界持有，未改生产代码。A03 采集已改为接受 B10 peer-timeout 路径并完成：`a03-stale-later-map` + `a03-recovery-2p-win` 实机通过；见 B58。

独立阶段只修阻塞当前流程或验收的问题；审查通过后再处理该阶段相关的已知问题。下方条目保留作为缺陷索引，并非要求先全部清空。

## 编译期门控迁移（首条入口至开始许可链候选，待审查）

权威定义、职责表和未决项见[发展路线](vmce-roadmap.md#取消-vmce-游戏行为的编译期门控)。本区只列执行顺序；不得把勾选项写成已完成，直到对应构建、测试和审查实际运行。

- [x] **G0 清单与决定（本链）：** 已分类注册、可见、客机解析 ID、开始许可和执行门控；运行时来源是现有 `ScenarioCatalog` 的已验证 VMCE 记录。没有新增 INI/RuleSet 假设；WOL 和旧已发布版的完整矩阵仍留在 G4。
- [ ] **G1 身份先编译（本链候选）：** 模式类、固定 ID、`VmceSeries` 过滤与客机解析已进入所有构建；ID 与自定义模式冲突时保留自定义模式并禁用 VMCE。其余 VMCE 协议/配置类型仍由 feature 负责，留到 G3；旧版本兼容仍待验证。
- [ ] **G2 可见与许可（本链候选）：** 房主选图与 `mode_start` 从 catalog 可用性派生许可，feature-off 固定 unavailable；本机双人 VMCE 胜利和续关通过，混合构建仍待验收。
- [ ] **G3 完整 LAN 开始链：** 地图、配置、角色、Ready/MapStatus、开始请求、模式校验、共享种子、加载和首帧模拟不再依赖编译期差异；失败在 GO 前可见。
- [ ] **G4 兼容与 WOL 决定：** 建立旧编译裁剪版、迁移桥版和最终运行时版的 LAN/WOL 主机/客机矩阵；明确支持或拒绝，不能挂起。
- [ ] **G5 退场：** 删除 feature、迁移桥和残余游戏行为 `cfg`。迁移过程中即可将对上游有独立价值、可单独验证的最小通用能力拆成小 PR；优先已有上游消费者的能力，单纯改名的 VMCE 代码留在独立分支。

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

## 阶段 3 第三个子链：共享大厅难度所有权（已完成，等待用户审查）

- **所有者：** 原生进程行 `[0xA8EB68]` 由共享大厅的 [MultiplayerShellRuntime](../../src/app/frontend/skirmish_session.rs) 持有，并类型化为 [LobbyDifficulty](../../src/skirmish_launch.rs)；LAN 选项载荷保留原生字段名 `cooperative_difficulty`，接收边界统一归一到滑杆可写的 0..=2。它是共享大厅输入，不是 Cooperative 模式状态；没有第二份镜像或第二套判定。
- **VMCE：** 滑杆和收到的选项都写入该所有者；VMCE 开局冻结从共享 `commit_start` 移入 VMCE 自己的 [frozen_start_houses](../../src/app/shell_network_setup/vmce_setup.rs)，把当时的行值写入 `VmceHouseAssignments.difficulty`。加载的 TriggerType 难度与结算页只读这份冻结值，[flow.rs](../../src/app/vmce_result/flow.rs) 不再读实时进程行。
- **Cooperative：** 回合、BC7 文案与收到的 `OptionsApplied` 经同一所有者读写 `native_row()`；0..=2 的既有值语义不变。
- **架构检查：** 删除共享能力 allowlist 中的 `cooperative_difficulty`；新增 VMCE 大厅自有文件清单并把这些文件整体纳入既有依赖扫描，既拒绝直接调用 Cooperative 大厅拥有者，也追可解析的间接包装调用（真实源码 + 植入式直接/间接负向 fixture）；新增结算页冻结难度语义测试，以及开局冻结只读共享 `lobby_difficulty()`、只经 VMCE 自有入口冻结的语义检查；共享引用上限删除 `commit_start` 的 VMCE 判定，因为该判定已移入 VMCE 自有函数。
- **测试：** `app::lan_service::options` vmce 23/默认 13（含 LAN 往返与越界行归一）、`app::shell_network_setup` vmce 47/默认 33、`app::frontend::skirmish_session` vmce 79/默认 78、`app::vmce_result` 26、architecture guards 51 条两配置；`check -p vera20k --lib --bins` 两配置 `Finished`。新增行为测试针对被删除的旧取值路径：越界 LAN 行旧实现原样保存而非归一，结算页旧实现读实时行而非冻结值；旧实现与新的类型化所有者不兼容，未单独回退编译运行。
- **证据：** vmce feature 的 fast 构建 `vmce-lobby-difficulty-r3`（exe SHA-256 `6A578BAD51B1CF38A723278E965B8EB9805C76F05874B91036DAC199A4B50D63`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`、下一关 `stage02` 回执一致。证据在 `.scratch/vmce-lobby-difficulty-accept-r2/`（未提交）。默认 feature 的构建没有 `lan-live-vera-campaign-*` 检查点，不能用于本验收。
- **未覆盖：** 完整 `AppState` 的 `service_event` → HostSetup 胶水仍无夹具；验收系列不修改难度滑杆，不能据此宣称“界面改难度后双方冻结一致”已实机复现；WOL、混合版本与跨机器未跑。WOL 的 `Options::into_options` 与 LAN 解码共用同一归一构造函数，但没有 WOL 侧镜像测试；结果页无冻结值时回退 NORMAL 行是防御性默认，真实结果页必经冻结分配。

## 阶段 3 第四个子链：房间队伍席位策略所有权（已完成，等待用户审查）

- **所有者：** “本模式的房间是否把玩家排进队伍席位”属于模式自己的大厅规则，归 [ModePolicy](../../src/skirmish_modes.rs) 的 `room_teams`：普通模式为 true，Cooperative 与 VMCE 为 false。`MatchSession::network_team_room` 只合并共享房间事实（Setup 阶段、`several_observers`、`allies_allowed`、非 clan 房间），不再用 `uses_cooperative_setup()` 反问 Cooperative；VMCE 模式行的 `allies_allowed: true` 因此不需要额外排除。
- **行为保持：** Battle/ManBattle/Siege/Unholy/FreeForAll 房间仍按队伍分组、TEAMS 与队伍控件可用；Cooperative 与 VMCE 房间仍不分组、不显示队伍控件；clan 房间和 `allies_allowed=false` 的结果不变。setup 视图里重复的 `!mode_policy.authored_campaign_roles` 判断已删除，改为单一所有者；普通房间内玩家可见行为不变，这是所有权收敛，不是界面改版（选图切进 VMCE 再切回的边缘差异见“未覆盖”）。
- **架构检查：** `ModePolicy` 仍是穷尽构造，每个模式必须显式给出新字段；逐模式策略字面量测试含新字段。本改动不新增 VMCE 模式检查，共享能力 allowlist 与引用上限无需改动；architecture guards 51 条两配置通过。
- **测试：** 核心 `skirmish_modes` 13 条（含逐模式策略全量字面量）；`app::match_runtime::session` 默认 50 / vmce 60，含新增 `room_teams_follow_the_mode_policy`（经真实 `MatchSession::accept_network_setup` 断言 Battle=true、Cooperative=false、Vmce=false；并覆盖模式切换后残留 `options.teams: Some(2)` 时 Vmce 仍不分组且 `network_teams()` 为 `None`、`allies_allowed=false` 时不分组；已实测旧谓词下 Vmce 返回 true 使该测试失败）；`app::shell_network_setup` vmce 47/默认 33；`check -p vera20k --lib --bins` 两配置通过。
- **证据：** vmce feature 的 fast 构建 `vmce-team-seating-r1`（exe SHA-256 `D9929F3E95CA2DE47D9C9B5A4DE8CAD07CC5E6E3E76F7CE298363C005EB5D981`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`/`can_continue=true`，下一关 `stage02` 回执一致；证据在 `.scratch/vmce-team-seating-accept-r1/`（未提交）。
- **未覆盖：** 本改动不改变任何玩家可见路径（此前 setup 视图的 authored-roles 判断已让 VMCE/Cooperative 不分组），验收用于确认没有回归，不能证明“队伍控件在 VMCE 中由可用变不可用”；WOL 平衡、旁观者房间与 clan 房间的队伍路径仍只有既有测试，未单独补现场；三人、四人、跨机器与互联网未跑。另有一个边缘差异：普通模式房主先设置队伍数、再通过选图窗口切进 VMCE 时，旧谓词会把残留的队伍数当作 VMCE 房间的席位（继续 `keep_players_on_teams`、自动平衡、保留 `slot.team`、WOL Balance），新策略不会；VMCE 自身界面两种情况完全相同（TEAMS 禁用、行不分组），唯一可见差异是从 VMCE 再切回普通模式时的队伍布局：旧实现保留切换前的布局，新实现重新分配。该往返回归现已通过生产选图入口自动测试修复；真实房间往返尚未采集。

## 阶段 3 第五个子链：开始许可归 VMCE（已完成，等待用户审查）

- **所有者：** 一次 VMCE 开局是否被允许属于 VMCE 自己的战役大厅规则：[shell_network_setup/vmce_setup.rs](../../src/app/shell_network_setup/vmce_setup.rs) 的 `VmceStartPermission`/`start_permission` 拥有模式运行时可用性、已选地图、LAN 配置同步、地图配置、已接受共享种子与地图角色解析，并拥有面向玩家的拒绝原因；`minimum_players_met` 拥有“至少一名非旁观玩家”。共享开始命令 [actions/start.rs](../../src/app/shell_network_setup/actions/start.rs) 的 `mode_start`/`has_minimum_players` 只保留一次 `is_vmce()` 分派、原生检查顺序、Ready/发布/GO 与错误显示，不再读取 VMCE 地图配置、种子或角色表。
- **行为保持：** VMCE LAN 的允许/拒绝顺序、日志、错误文案、`Ok(true/false)` 与 GO 前冻结/发布不变；普通模式与 Cooperative 完全走原有 `validate_setup_start` 路径。迁移桥（feature-off）的 `vmce_setup_stub.rs` 提供同名分派，仍以“无可用 VMCE 后端”的同一原因拒绝，不会走到不完整后端。
- **架构检查：** 新增 VMCE 开始许可语义检查：共享 `mode_start` 必须调用 `vmce_setup::start_permission`，且不得再出现 `resolve_vmce_houses_with_config`/`network_vmce_map_config`/`load_network_map_ini`/`vmce_map::mode_available`/选图文案；`vmce_setup::start_permission` 必须持有这些规则；`has_minimum_players` 必须分派 `minimum_players_met`；迁移桥必须保留同名槽位与后端拒绝原因。移除共享文件中的 VMCE 编译期门控后，pinned gate 预算 `actions/start.rs` 2→1、总数 462→461（唯一故意的门控变化）。architecture guards 52 条两配置通过。
- **测试：** 新增 `the_vmce_lobby_requires_a_player_and_never_counts_an_observer_as_one` 直接经 VMCE 所有者断言单玩家通过、旁观者/空名单不通过；既有 `vmce_solo_host_passes_opponent_checks_but_observers_do_not` 等经生产 `has_minimum_players` 分派继续覆盖开局人数。`app::shell_network_setup` vmce 48/默认 33；`check -p vera20k --lib --bins` 两配置通过。
- **证据：** vmce feature 的 fast 构建 `vmce-start-permission-r1`（exe SHA-256 `90D71AFD07CC5CD187C88A93FB8A737F7B105AF32C0FE326FC560837E41DC99B`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`/`can_continue=true`，下一关 `stage02` 回执一致；证据在 `.scratch/vmce-start-permission-accept-r1/`（未提交）。
- **未覆盖：** 开始许可的拒绝路径需要完整 `AppState`（无夹具），由静态架构检查钉住分派与文案归属；正常 GO 由双人验收覆盖，未逐一实机触发“无后端/未选图/非 LAN/缺地图配置/角色冲突”各拒绝分支；迁移桥（feature-off）保留 VMCE 最低人数表达式，使有玩家的房间仍会显示后端拒绝原因，但 feature-off VMCE 房间当前不可达，未实机触发；WOL、旧版/混合构建、跨机器、三人/四人未跑。本次是无行为变化的所有权迁移，提取前后的允许/拒绝语义逐语句一致。相关既有守卫空白（三条历史归零门控 pin 被 `count==0` 跳过、`actions/start.rs` 的测试专用 `resolve_vmce_houses` 门控）未在本子链修复。

## 首条入口至开始许可迁移（候选，待审查）

- **运行时 owner：** [ScenarioCatalog](../../src/app/scenario_catalog.rs) 中已验证的 VMCE campaign 记录。模式 ID 和序列来源在所有构建中可识别；入口是否可见、是否允许 GO 都读取同一 owner，不新增 INI、RuleSet 或通用框架。
- **行为：** feature-off 构建不显示 VMCE 选图入口，收到 VMCE LAN Options/HostSetup 时在共享 session 变更前拒绝；feature-on 保留原地图、配置、角色和共享 seed 校验。
- **验证：** feature-off `check -p vera20k --lib --bins`、wasm feature check 和 VMCE gate guard 通过；ID 注册、客机解析/backend 门控、可见性、LAN prevalidation、feature-on 地图角色开始及 30 项原版 Cooperative 直接消费测试通过。
- **未覆盖：** 旧已发布版、WOL、跨机器和完整旧/新 LAN 矩阵仍属 G4。审查修复后的 feature-on 构建在沙箱外通过本机双人胜利与续关，双方 300 个 checksum 和 20 个模拟 hash 一致；该验证不覆盖 feature-off 客机或 ID 冲突提示，预校验回归也未贯穿完整 `receive_extended`/HostSetup 界面。架构检查未识别所有小写辅助函数，不能仅凭门控计数断言解耦。房主合成界面捕获在修复前候选和旧构建均因未认领角色失败，不能归因于本次修改。

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
- B58 已实现 Ready/Go 请求级复验；2026-11-15 用 `vmce-a03-peer-timeout-r2` 完成真实双进程 `a03-stale-later-map` + `a03-recovery-2p-win`：客人 Ready 后只改未选中下一关地图，被锁定拒绝并恢复；host loading 后未进游戏，以 B10 peer-timeout 返回 shell；恢复局进入 stage02，300 checksum/20 hash 一致。重复请求/旧扫描迟到仍由定向回归覆盖，不在本次双进程采集内。
- B102 已确认现有规则协议与系列准入覆盖开局/关间/重试；房主侧原因提示、关间房间握手本身与真实数据变更采集仍为残余。
- B36：跨机器、关间加入/离开和其他真实网络故障未由本机多进程测试证明。
- 大厅模式事件分派（阶段 3 第二子链）：加入/退出与选图窗口的分派由 dispatch 测试（vmce 配置使用生产谓词表）和双人 `2p-win` 建房/加入/开局覆盖；C0 的所属方与早到/正常/乱序/模式切换/离房边界由 `lobby_campaign` 生产处理器测试及 LAN/WOL 事件身份测试覆盖，但完整 `service_event` → HostSetup 事务仍无 `AppState` 夹具；在线大厅实机顺序未复现。真实双进程房间内的中途离房、三人/四人大厅未跑。用户接受的残余（B59、B71、B72、共享所属方战绩）见上方专节；正常玩家等待超时边界见“优先核查”，未修复。
- 大厅难度所有权（阶段 3 第三子链）：VMCE 冻结值、加载 TriggerType 与结算页读取由聚焦测试覆盖，LAN 选项发布/接收由生产函数覆盖，架构检查钉住所有者；但完整 `AppState` 的 HostSetup 事务和“界面滑杆 → 冻结”整条胶水仍未在单元测试内执行，WOL 与混合版本的 `cooperative_difficulty` 字段兼容未跑。
- 房间队伍席位策略所有权（阶段 3 第四子链）：`ModePolicy.room_teams`、`network_team_room` 与 setup 分组由 session 测试和双人验收覆盖；但原改动曾导致模式往返时队伍重新分配，该回归现已由生产选图入口测试修复，原生产验收仅证明正常开局无回归；WOL 平衡、旁观者房间与 clan 房间的队伍路径未单独实机复现，三人、四人未跑。
- 开始许可归 VMCE（阶段 3 第五子链）：`mode_start` 的允许分支由双人验收覆盖，拒绝分支（无后端、未选图、非 LAN、缺地图配置、共享种子/角色解析失败）因需要完整 `AppState` 未在单元或生产测试内逐一执行，由静态架构检查钉住调用与文案归属；迁移桥（feature-off）的拒绝对话未实机触发；WOL、混合版本、跨机器、三人/四人未跑。
- 大厅验收工具（阶段 3 第七子链）：`2p-win` 现在通过生产 Team 列按座位认领角色、房主调用生产难度滑条动作处理器设置 Hard（不覆盖鼠标拖动），并校验两端角色归属、大厅难度与冻结难度一致，实机通过；`2p-retry` 复用同一生产角色探针认领镜像角色，使失败重试诊断要求的席位/所属方顺序确定，比较器按场景校验精确映射与 `role_count`；`2p-final`/`4p-win` 与 Cooperative 采集仍不启用探针、保持随机角色。拒绝/离房/下一关角色重选、WOL、混合版本、跨机器、三人/四人未覆盖。

## 已完成

- **上游 0.3.3 同步（2026-11-15，本轮）：** 合并 upstream/main `0f7dc1bbc`（191 提交，版本 0.3.3）为 `78b0aef4a`；15 个冲突文件按“共享文件采用上游、VMCE 行为重放”解决，i18n 键取并集，运行时 Ares 开关与 VMCE preserve-empty 解析组合，任务收入字段/哈希保留。双配置检查与架构守卫 52/52 通过，聚焦 VMCE/Cooperative/LAN 测试通过；`vmce-upstream-0f7dc1bbc-r1` 的 `2p-win`、`2p-final`、`4p-win` 匹配 300 个 native checksum；`2p-retry` 匹配重试前 300 个 checksum/20 个模拟 hash，并以同关同 seed 重载。残余见[基线](../vmce-baseline.md)。
- **失败重试验收角色顺序（2026-11-14，本轮）：** `2p-retry` 通过生产 Team 列动作认领镜像地图角色（`role_count - 1 - 座位行`），经正常 LAN 选项/PlayerChoices 同步，固定了失败结算诊断要求的席位/所属方非单位映射；`2p-win` 保持身份映射，未启用探针的场景保持随机角色。比较器按场景校验身份或镜像映射并读取运行时 `role_count`。新增镜像/Ready 过渡与身份映射拒绝测试，回退到旧身份逻辑时失败、修复后通过。本轮 `vmce-retry-probe-r3` 的 `2p-retry`（`LDA-Retry`）、`2p-win`（`LDA-Continue-real`）和 `4p-win`（`LDA-4p`）通过；证据与残余见[基线](../vmce-baseline.md)。
- **大厅验收工具（席位角色与难度探针，2026-11-14）：** `2p-win` 生产验收通过生产 Team 列让双方各自认领与座位一致的地图角色、调用生产难度滑条动作处理器设置 Hard（不覆盖鼠标拖动），并在材料性 host-options 修订清掉 Ready 后重新 Accept；receipts 与比较器校验两端角色归属、大厅难度和冻结难度。验证与残余见[基线](../vmce-baseline.md)。
- **阶段 3 第五个子链（开始许可归 VMCE）：** VMCE 模式可用性、选图、LAN 配置、共享种子与角色解析的开始许可及最低人数规则移入 VMCE 自有大厅模块，共享开始命令只分派并显示原因；迁移桥继续拒绝，普通模式与 Cooperative 路径不变。残余见上方子链小节。
- **阶段 3 第四个子链（房间队伍席位策略所有权）：** 房间是否按队伍席位分组改由模式策略 `ModePolicy.room_teams` 决定，共享 `network_team_room` 不再用 Cooperative 判定代替模式规则，setup 视图重复判断删除；Cooperative 与 VMCE 不分组、普通模式保持，残余见上方子链小节。
- **阶段 3 第三个子链（共享大厅难度所有权）：** 原生 `[0xA8EB68]` 行由共享大厅类型化持有，VMCE 开局冻结一次并让加载与结算只读冻结值，Cooperative 读取同一所有者；LAN 字段名未改，越界行在接收边界归一。残余见上方子链小节。
- **阶段 3 第二个子链（大厅加入、退出与模式事件分派，含验收修复）：** 共享大厅经 dispatch 所有者在原生语义下选择模式，VMCE 模式事件进入自己的槽位，不再调用 Cooperative 专属入口；选图窗口模式下按窗口选中模式处理的语义保留。验收修复后 C0 按消息自身的安装身份归属：WOL 用载荷模式，LAN 用 scope/revision 并暂存早到文本；旧房间/旧修订/非 Cooperative 文本不进入 Cooperative 状态，离房清理待处理项。双人建房/加入/准备/开局验收通过，残余见上方子链小节。
- **阶段 3 首个子链（VMCE 结算统计所有权）：** VMCE 统计改由自己的角色/分配投影，Cooperative 保留原版行选择，双方只共用 `HouseScoreFacts`；双人三流程验收通过，残余见上方子链小节。
- **B5、B106：** 核心层整理和战役数据从大厅分离。
- **B57：** 确认结果后保存本机关卡解锁；残余见待验证。
- **B10：** 终端 LAN 交接增加有界等待和失败处理；关间期限到期按 Continue 的无提示退出已修复（`b2806138e`）；残余见待验证。
- **B102：** 规则/扩展身份与战役系列准入由现有 owner 覆盖，未改协议；残余见[审查详情](vmce-review.md#已完成保留验证边界)。
- 其他历史完成项及测试范围保留在[审查详情的完成区](vmce-review.md#已完成保留验证边界)。
