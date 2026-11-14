---
layout: default
title: VMCE 审查总文档
lang: zh-CN
---

# VMCE 审查总文档

[玩家与地图作者指南](vmce-guide.md) · [中文总览](vmce.md) · [English hub](../vmce.md) · [English backlog](../vmce-backlog.md)

本页保留 VMCE 逐项问题、状态、证据、验证边界和已锁定的决定；日常工作顺序参见[精简待办](vmce-todo.md)。`A##` 为审查条目，括号内 `B##` 为英文台账对应编号；只有 B 编号的条目是仅英文台账记录、本页正式纳入的残余项。

原始 55 条审查项保留原编号。以下待办按优先级排列，已实现的条目移到完成区；未验证的场景和已知残余继续跟踪。B101–B105 是上游同步后的新增项。

原始审阅基线是 `70d569b86`；后续 B5、B106、B57 和 B10 的实现与验证见完成区。历史证据的源码行号可能已随重构变化，应对照当前代码核实。

验收口径：用户于 2026-11-11 决定暂不要求真实机器人工验收，以理论分析、代码检查和定向自动化测试为准；跨机器、丢包及其他未执行的场景继续列为残余，不代表已经验证。

## 大厅队伍与采集修复（2026-11-14）

- 模式策略分别决定队伍席位和选图时保留既有队伍；共享会话仍是唯一队伍所有者。普通模式经两张 VMCE 地图返回普通模式时，四名玩家的队伍布局由生产选图入口测试保持；测试在修复前失败、修复后通过。真实房间中的模式往返尚未采集。
- 未启用角色探针时允许随机角色；客人每次重新准备时比较该次准备前后的快照。两项回归测试均在修复前失败、修复后通过。
- fast 构建的双人 `2p-win` 与未启用角色探针的 `2p-final` 通过，双方各有 300 条一致校验和与 20 条一致模拟哈希。角色和难度探针调用生产动作处理器，不覆盖鼠标命中或滑杆拖动。
- `2p-retry` 使用失败战役时已到失败结算，但额外的“座位与所属方顺序必须反转”诊断断言在本次随机角色分配下失败，重试流程尚未验收通过。开始许可拒绝分支和混合版本仍未覆盖。

## 优先核查

- **B102（已关闭）：规则与扩展数据一致性。** 现有联机规则协议与 VMCE 系列准入已在进入任务前拒绝不同构建或数据；证据与残余见[完成区](#b102-规则与扩展身份准入2026-11-13已关闭)。

## 编译期门控迁移审查（首条入口至开始许可链候选，其余未迁移）

本组不新增 A/B 缺陷编号。2026-11-14 的首条入口至开始许可链已有候选实现与聚焦回归；下面区分已迁移行为、仍由 feature 承担的后续路径和尚未完成的生产验证。权威阶段见[发展路线](vmce-roadmap.md#取消-vmce-游戏行为的编译期门控)。

| 编号 | 结论 | 依据与影响 |
|---|---|---|
| M01 | 首条链已移除注册、来源过滤、解析和许可门控，后续 VMCE 行为仍由 Cargo feature 承担。 | [根 feature](../../Cargo.toml#L62) 仍转发 core；当前 `cfg` 数量只表示 G3 之后的迁移余额，不能当作完成度或行为验证。 |
| M02 | 已迁移候选：模式身份与展示、可开始性分离。 | [模式注册](../../src/skirmish_modes.rs#L406) 使用固定 ID；自定义模式占用时不注册 VMCE，保留自定义模式并在有战役数据时提示房主。feature-off 行保持隐藏。 |
| M03 | 已迁移候选：新版识别 VMCE ID；冲突时不得把带 VMCE 身份的 Options 当普通模式。 | [LAN options](../../src/app/lan_service/options.rs) 的 feature-off 预校验保留并拒绝 VMCE series 标记；接受会话前的拒绝有定向测试。旧版本与实际混合房间仍待 G4 验证。 |
| M04 | 已迁移候选：GO 前许可先要求运行时 catalog 有 VMCE 入口，再进入 feature-on 原地图/配置/角色/seed 校验。 | [VMCE `mode_start`](../../src/app/shell_network_setup/actions/start.rs#L434) 在 feature-off 的 GO 前 fail closed；feature-on 原校验保留。Ready/MapStatus/加载/首帧、WOL 和真实跨进程矩阵仍属 G3/G4。 |
| M05 | 本链运行时来源已定。 | [ScenarioCatalog](../../src/app/scenario_catalog.rs) 中已验证的 VMCE campaign 条目拥有可用性；feature-off 迁移桥无 reader，固定 unavailable。未新增 INI 或 `RuleSet`。 |
| M06 | 上游候选尚未成立。 | `ModePolicy` 已有共享消费者，但当前仍含 `Vmce` 变体及 VMCE 专用字段；原样上游化只会输出私有设计。应拆出对上游有独立价值、可单独验证的最小能力，优先选择已有非 VMCE 生产消费者的部分。 |

结论：首条入口至开始许可链已有候选实现；feature-on 的本机双人胜利和续关通过生产验收。feature-off 实机拒绝、WOL 和旧/新版本矩阵尚未完成；不能把当前候选写成整体门控迁移完成或上游通用能力成立。

## P1 — 优先修复

原 P1 的十项实现已完成；未覆盖的故障和验证见下方待验证区。

## P2 — 随后修复

### B104 自动维修与共享 House 授权 — P2

自动维修需与共享 House 授权合流；关联 B70 的建筑操作和 B75 的部署/卸载，按各自操作链验证授权和联机一致性。

### A04 · B59 相同国家的不同角色仍可开局 — P2

- **问题与影响：** 两个不同主 House 使用同一 `Country` 时，当前角色校验仍可通过。按国家索引的胜利触发器会解析首个匹配 House，地图不能按约定明确区分玩家。即使玩家之间国家唯一，若先登记的 AI House 与玩家同国却互为敌人，[胜利判定](../../src/sim/world/house_defeat.rs#L933)也可能以该 AI 为胜方而把同国玩家判负。
- **依据：** [角色绑定](../../src/app/match_bootstrap.rs#L426)只检查 House 存在及角色不重复；[国家查找](../../src/sim/world/mod.rs#L6330)按登记顺序返回首个匹配项。普通原版动作按本地玩家国家直接比较，VMCE 又按首个 House 的联盟扩展胜方。大厅虽有国家冲突提示，但展示逻辑不是开局校验的权威所有者。
- **修正：** 在共享的角色解析/开局校验中，根据生效规则解析各主 House 的国家身份并拒绝玩家角色间重复；提示冲突角色和国家。另明确玩家与 AI 同国时胜方及联盟的歧义，在有证据支持前不要笼统禁止所有 AI 同国，也不能仅检查玩家角色就认为问题解决。保持国家索引和默认值的现有所有者。
- **状态（2026-11-13，用户决定）：** 同国多所属方的问题不额外兜底；本项不修，保留为已知残余，不宣称修复。
- **验收：** 两个不同主 House 同国的地图不能 Ready/开局；不同国家可开局；覆盖大小写、默认国家和随机角色。另覆盖 AI 先于玩家、玩家先于 AI、同国多 AI 及不同联盟关系，核对原版本国胜利与 VMCE 盟友扩展的约定，确保所有端结论一致。

### A06 · B62 解锁记录读取或写入失败对玩家静默 — P2

- **问题与影响：** 本机解锁文件损坏、目录不可写或磁盘写失败时，玩家可以继续，但稍后建房发现下一关没有解锁。读取失败时选关列表会隐藏已解锁的中途关，却没有界面说明。
- **依据：** [解锁调用](../../src/app/shell_network_setup/vmce_map.rs#L357)和[进度读取](../../src/app/shell_network_setup/vmce_map.rs#L390)捕获错误后只写日志；[选关过滤](../../src/app/shell_network_setup/vmce_map.rs#L448)把无法读取的进度视为没有解锁。[持久化所有者](../../src/app/persistence/vmce_progress.rs#L40)不会覆盖损坏记录。
- **修正：** 保持不覆盖损坏数据，将读取或持久化错误返回应用层，并以本地化提示说明已有进度无法读取或下一关未保存，提供可重试路径；避免把本机存储失败误报为联机结果不一致。
- **验收：** 模拟损坏文件及写入失败，确认提示、数据不被覆盖；修复环境并重试后确实能从本机选到下一关。

### A07 · B43 回到房间后随机玩家在角色板中消失 — P2

- **问题与影响：** 切换到下一关房间时所有人重置为随机角色，右侧角色板可能全显示等待，甚至看不到已准备玩家的状态；实际参战者仍在左侧玩家列表。本轮 2p-win/4p-win 真实采集的初始 VMCE 大厅截图（关间房间截图尚未采集）显示两名/四名玩家的 Owner 都重置为 `隨機`，右侧角色板对应行均为 `等待 ...`、Ready/Color 空白，支持该现象。
- **依据：** [关间重置](../../src/app/match_runtime/session/cooperative_round.rs#L574)将角色设为 `-2`；[角色板](../../src/app/shell_network_setup/view.rs#L395)只查找具体索引 `0..3`。
- **修正：** 用独立的随机玩家列表或包含姓名和准备状态的汇总呈现；不要为显示而提前抽取或锁定随机角色。
- **验收：** 两至四人全部随机、部分随机及角色更换后，房主和客人都能看到所有玩家与准备状态；开局时仍只分配未占角色。

### A08 · B60 人数超过下一关角色数时界面重叠 — P2

- **问题与影响：** 例如四人进入只有两个角色的下一关，真实玩家行仍保留，但简报和状态区域按角色数布局，可能与额外玩家行重叠，妨碍识别和操作。
- **依据：** [列表投影](../../src/app/shell_network_setup/view.rs#L541)保留超出角色数的真实玩家；[布局](../../src/ui/coop_lobby.rs#L112)按玩家行绘制控件，但简报起点按 `role_count` 计算；[布局调用](../../src/app/input/multiplayer_setup.rs#L50)也按角色数传入。
- **修正：** 玩家列表区域按实际可见人数与角色数中的较大值预留空间，或使用独立滚动区域；保持房间，不自动创建观察者或踢人。
- **验收：** 实际采集四人转两角色、三人转一角色及不同缩放，检查所有玩家、离开/移除操作、提示和简报不重叠；人数降至限制内后可以开始。

### A09 · B46/B60 超员原因和校验进行中不清楚 — P2

- **问题与影响：** 下一关人数超限时，房间没有明确的本地化人数提示；房主点击开始只看到泛化的英文错误。全系列校验进行时也没有内联进度状态，客人可能以为准备按钮无响应。
- **依据：** [开局错误](../../src/app/shell_network_setup/actions/start.rs#L482)使用硬编码英文并把超员、缺少角色、角色重复混为一条；[房间状态](../../src/app/shell_network_setup/view.rs#L414)只映射国家冲突及地图错误；[校验状态](../../src/app/lan_service/options.rs#L424)把 `Pending` 映射为无提示。
- **修正：** 用结构化失败原因统一驱动本地化提示，展示当前人数和角色上限；校验期间显示「正在校验战役文件」，并让按钮禁用态与提示一致。
- **验收：** 中英文分别覆盖超员、重复角色、缺图、不一致和校验中；修正原因后提示和按钮状态及时恢复。

### A13 · B66 外部战役路径使零售任务简报查找失败 — P2

- **问题与影响：** VMCE 地图使用 `VMCE/<系列>/<目录>/all01umd.map` 等目录路径时，大厅查找零售 `MISSIONMD.INI` 的任务简报会使用整条路径，无法匹配按文件名命名的节，显示简报不可用。本轮 2p-win/4p-win 初始 VMCE 大厅截图仍显示 `Mission briefing unavailable`。
- **依据：** [简报查找](../../src/app/shell_network_setup/view.rs#L173)直接把目录形式的地图路径传给 INI；零售 `MISSIONMD.INI` 的对应节为 `[ALL01UMD.MAP]`。这与 B45 的长简报截尾是两个问题。
- **修正：** 对零售简报查找取得地图文件名组件，同时正确处理两种路径分隔符，并继续使用现有简报键解析与缺失回退；不改变加载地图的完整路径身份。
- **验收：** 用带目录的 `all01umd.map` 通过生产简报读取路径取得对应文本；覆盖 `/`、`\\`、大小写和真正缺少简报的情况，并采集当前界面。

### A15 · B68 单角色地图仍允许第二人加入 — P2

- **问题与影响：** `PlayerCount=1` 的关卡已由一位房主占据唯一角色，但房间人数上限被限制为至少 2，第二人仍可能加入，随后因角色不足无法开始。
- **依据：** [人数上限](../../src/app/match_runtime/session.rs#L2094)强制最小值为 2；[关间房间](../../src/app/shell_network_setup.rs#L1156)也把角色数限制为 2 至 4，而战役配置允许一个角色。
- **修正：** 让 VMCE 的准入上限正确表达一个角色；从多人关卡切换到一角色关卡时保留已有玩家并提示超员，不自动踢人。
- **验收：** 一角色新房间拒绝第二人加入；两人切换到一角色关卡时房间保留且不能开始，人数符合后可继续。

### A16 · B11 合法的长关卡路径可能在完成本关后被拒绝 — P2

- **问题与影响：** 战役路径通过现有准入并能开始第一关，但下一关完整路径超过关卡报文的 215 字节上限；玩家完成本关后才发现无法继续。中文目录在 UTF-8 下更容易碰到字节上限。
- **依据：** [网络地图路径验证](../../src/map/source.rs#L104)允许较长名称；[下一关报文](../../src/net/lan_messages/cooperative.rs#L230)对路径施加更低上限。详细边界须按完整目录与编码实际测试。
- **修正：** 设计兼容的长路径传输；若暂不支持，在建房准入时检查所有可能使用的完整路径并给出明确提示，不能让玩家打完本关才失败。
- **验收：** 测试完整路径 215/216 UTF-8 字节、中文目录和下一关切换；非法路径应在开局前可理解地拒绝。

### A17 · B69 可靠队列已满时控制消息可能从未入队 — P2

- **问题与影响：** 联机拥塞导致全局可靠发送队列满时，结算准备、重试投票、`Begin`、选项或确认消息可能被丢弃；本地一次性状态已经变化，其他玩家却收不到消息，可能持续等待。这与 A01 中已入队消息发送后丢包不同。
- **依据：** [多目标发送](../../src/app/lan_service.rs#L3720)忽略 `send_bytes` 的入队返回值；[轮次请求](../../src/app/lan_service/cooperative.rs#L839)通过该路径发送上述控制消息。多个目标时还可能只有部分目标成功入队。
- **修正：** 复用既有可靠队列与背压处理所有者；保持待发送请求直到所有目标接纳，确保重试不会造成重复状态推进，并为失败提供明确出口。
- **验收：** 将队列填满后发出准备、投票、选项与确认，再释放容量，确认每个目标最终收到且本地状态与送达状态一致；覆盖四人部分目标先成功的情况。

### A18 · B70 获授权的共享 House 无法完整修理和售卖 — P2

- **问题与影响：** 玩家经 `PlayerNControl=yes` 获得共享 House 控制权后，修理和售卖光标仍只接受本地主 House 的建筑；共享 House 围墙也不能通过售卖光标卖出。主 House 被消灭后依赖共享单位的玩家同样受影响。
- **依据：** [建筑命中检查](../../src/app/input/commands.rs#L178)比较对象与本地主 House，修理和卖出路径均调用它；[围墙输入](../../src/app/input/commands.rs#L329)也仅允许本地主 House。[网络命令检查](../../src/sim/world/world_commands.rs#L3378)对围墙只比较发送方主 House，没有使用已认证来源对共享 House 的授权判断。
- **修正：** 输入侧通过现有本地控制授权所有者判断目标；执行侧按认证来源验证目标 House 授权。维持普通非 VMCE 模式行为，并确认建筑修理/售卖的原版入场条件仍由既有所有者处理。
- **验收：** 对授权与未授权玩家分别测试建筑修理、售卖和围墙售卖；覆盖主 House 消灭后、重生后及双人同步，未授权命令必须被拒绝。

### A19 · B71 被动国家主 House 可能破坏结算人数 — P2

- **问题与影响：** 地图作者把 `MultiplayPassive=true` 的国家用作玩家主 House 时，该角色可能不计入合作结算行。多人胜利页因此可能误以为只有一人并走单人继续路径；结算统计也可能缺失该玩家。
- **依据：** [角色校验](../../src/app/match_bootstrap.rs#L433)没有拒绝被动国家；[VMCE 结算统计投影](../../src/app/vmce_result/statistics.rs)从冻结分配取每名玩家的主 House，但 `MultiplayPassive` 的 House 不产生事实行；[轮次人数](../../src/app/vmce_result/victory.rs)仍取该投影的行数，[单人分支](../../src/app/match_runtime/session/cooperative_round.rs#L931)据此直接继续。零售 `RULESMD.INI` 的 Neutral 和 Special 设置了 `MultiplayPassive=true`。
- **修正：** 轮次参与人数应取冻结的参战者/实际连接状态，不能取可被显示过滤影响的统计行；明确 VMCE 是否允许被动国家作为主角色：若允许，按已分配角色展示统计；若不允许，在开局前给出清晰错误，并说明限制依据。
- **状态（2026-11-13，阶段 3 结算统计所有权分离后）：** 行数与值现在由 VMCE 自己的角色/统计所有者投影，不再经过 Cooperative 统计接口；但被动 House 仍按原显示语义被过滤，轮次人数仍取过滤后的投影行数，因此 A19/B71 本身未修复，未被本子链接管。用户已决定 B71 不修，本项保留为已知残余，不宣称修复。
- **验收：** 覆盖单人和双人包含被动主 House 的胜利、失败重试和关间继续；确认玩家行、握手人数与最终结果一致，或在开局前一致拒绝。

### A20 · B72 任务中替换地图可能让结果页失去 VMCE 控件 — P2

- **问题与影响：** 任务开始后本地已加载地图文件被覆盖或替换、但战役清单和下一关未变化时，胜利结果页可能无法构建角色板，退回普通计分页外观，缺少玩家胜负、准备和离开房间等 VMCE 控件。
- **依据：** [结果板](../../src/app/vmce_result/victory.rs)依赖当前角色元数据；[元数据投影](../../src/app/shell_network_setup/view.rs#L118)从磁盘重读已完成关卡并要求与冻结配置的源摘要匹配；[结果页](../../src/app/vmce_result/victory.rs)把可能为空的结果板交给界面。此时已加载的模拟状态和磁盘文件可能不同。
- **修正：** 从开局冻结的角色、所属方、国家和颜色数据建立结算投影，保持运行中任务的数据一致；若需要拒绝文件变化，应明确提示并走可靠的全员退出路径，不应静默退回普通页面。
- **状态（2026-11-13，用户决定）：** 任务中地图被删除或替换的情形不额外兜底；本项不修，保留为已知残余，不宣称修复。
- **验收：** 任务运行时仅替换当前地图，然后让地图正常触发胜利；确认各端结果页和操作仍正确，或各端得到明确失败并可靠返回。覆盖地图缺失及文件替换。

### A21 · B73 切入 VMCE 可能保留原大厅 AI 槽 — P2

- **问题与影响：** 普通 LAN 房间已有 AI 对手时切换到 VMCE，仍在新容量内的 AI 行可能被保留，且可继续选择难度；它占据人数/席位，却不是战役 `PlayerN` 的玩家角色，容易使房主误解和开局检查异常。
- **依据：** [选图与模式切换](../../src/app/match_runtime/session.rs#L2359)只处理容量变化时需要移除的行，[保留的 AI](../../src/app/match_runtime/session.rs#L2518)仍被更新；[玩家行选项](../../src/app/shell_network_setup/view.rs#L699)即使 VMCE 不允许新增 AI，也在已有 AI 时提供难度项。
- **修正：** 切入 VMCE 时通过现有槽位/会话所有者清理或明确阻止大厅 AI 配置；地图作者放置的 AI House 按战役逻辑保留。
- **验收：** 普通多人房间先添加多个 AI，再切到二至四角色 VMCE 地图，确认房间无虚假 AI、人数正确、其他玩家角色和准备状态得到合理处理；再切回普通模式验证原有语义。

### A23 · B75 共享单位的自点部署与部分卸载操作被拒绝 — P2

- **问题与影响：** 玩家已获授权并选中共享 House 的单位后，单击单位自身不能触发本应可用的部署或卸载操作，例如部署基地车或卸载载员；共享载员飞机的 D 键卸载也会被拒绝。主 House 被消灭后，依赖共享单位的玩家仍会遇到此限制。陆地单位的 D 键有独立路径，不能据此断言所有部署方式都失败。
- **依据：** [点击路径](../../src/app/input/context_order.rs#L2145)传入本地主 House；[自点判定](../../src/app/input/context_order.rs#L3555)要求对象所属方等于它。[D 键分发](../../src/app/input/dispatch.rs#L4253)对飞机等对象调用[卸载判定](../../src/app/input/transport_orders.rs#L20)，后者同样检查主 House。计划模式和光标还有相同判断，应一并核对。
- **修正：** 在 VMCE 中让点击、光标、计划模式和相关快捷键复用现有本地共享控制授权；命令执行仍由认证来源及所属方授权的既有所有者检查。普通模式与原版自点限制保持原有语义。
- **验收：** 对主 House、授权共享 House 与未授权 House 分别覆盖基地车自点部署、载员飞机 D 键卸载及光标，检查计划模式和双人同步；包括主 House 被消灭与重生。

### A26 · B78 下一关解析失败时玩家看不到原因与本关结果 — P2

- **问题与影响：** 战斗中 `campaign.ini` 被修改或删除、下一关文件消失等情况，会使胜利后的下一关解析失败；当前代码记录日志并离开，玩家可能直接返回大厅，看不到本关逐玩家结果，也没有可操作的失败提示。
- **依据：** [胜利页入口](../../src/app/vmce_result/mod.rs)在构建结果页之前解析下一关；失败后仅写日志并调用[退出路径](../../src/app/vmce_result/mod.rs)。[下一关解析](../../src/app/shell_network_setup/vmce_map.rs#L279)会检查清单身份和地图文件。
- **修正：** 将已完成的胜负展示与后续关卡解析错误分开；保留本关结果或提供能展示结果的失败流程，并以本地化提示说明可处理的原因，沿用可靠的全员退出机制。不要将解析错误冒充战役末关。
- **验收：** 任务中分别修改清单、删除下一地图，在房主及客人两端触发胜利；验证结果可见、原因明确、两端可安全退出且不解锁错误关卡。

### A29 · B81 反斜杠清单路径在部分平台无法加载 — P2

- **问题与影响：** 在 Linux/macOS，`Scenario1=Allied\01.map` 指向真实的 `Allied/01.map` 时，清单校验可通过，但目录扫描会按字面反斜杠查找文件，战役可能不显示；`NextScenario` 使用反斜杠时也可能在通关后加载失败。Windows 行为依赖其路径语义。
- **依据：** [清单校验](../../src/app/shell_network_setup/vmce_map.rs#L748)先把反斜杠换成斜杠；[地图加载](../../src/app/shell_network_setup/vmce_map.rs#L519)却直接 `join(entry)`；[发现](../../src/app/shell_network_setup/vmce_map.rs#L456)和[下一关解析](../../src/app/shell_network_setup/vmce_map.rs#L303)都调用该加载器。
- **修正：** 让清单校验、发现、加载及目录身份使用同一跨平台路径规范化所有者，并保留 canonicalize 防逃逸检查。
- **验收：** Linux/macOS 和 Windows 分别覆盖包含子目录的入口、ScenarioN 和下一关的两种分隔符，检查准入及切关结果一致，并拒绝路径逃逸。

### A30 · B82 大小写或分隔符不同的同义段未被拒绝 — P2

- **问题与影响：** `campaign.ini` 中两个关卡段或共享控制段，若仅大小写或 `/`、`\` 不同，可能均通过重复检查但运行时只使用第一段；不同授权值因此可能被静默忽略。
- **依据：** [物理重复检查](../../src/rules/ini_parser.rs#L602)比较原始段名；[关卡及控制段查询](../../src/app/shell_network_setup/vmce_map.rs#L217)忽略 ASCII 大小写并把两种路径分隔符视为相同，按首项匹配。
- **修正：** 在清单准入处按与查询一致的逻辑身份检测并拒绝重复，错误提示列出冲突段；不能仅改变查找顺序。
- **验收：** 覆盖关卡段、控制段的大小写、分隔符和 House 名称差异，以及相反授权值；确保各端在开始前一致拒绝。

### A33 · B85 非 ASCII 地图文件名无法按原名解析 — P2

- **问题与影响：** `campaign.ini` 用 UTF-8 写入 `Scenario1=第一关.map`，磁盘上也是该文件名时，解析结果可能变成逐字节扩展后的乱码路径，导致战役发现失败。系列目录名由操作系统单独读取，不应与清单里的地图文件名混为一谈。
- **依据：** [VMCE 外部配置读取](../../src/rules/ini_parser.rs#L514)使用[逐字节扩展](../../src/util/native_string.rs#L3)，而[清单路径校验](../../src/app/shell_network_setup/vmce_map.rs#L748)把所得字符串直接作为文件系统路径。
- **修正：** 为 VMCE 自有配置定义明确的文本编码，并在文件路径与联机身份边界一致解码；不要改变零售游戏 INI 所需的原生字节语义。
- **验收：** 用中英文文件名和子目录测试发现、房客整套校验、下一关与 UTF-8 报文字节边界；同时覆盖旧 ASCII 战役兼容性。

### A36 · B88 下一关身份记录未绑定轮次，迟到记录可让新轮次全员退回大厅 — P2

- **问题与影响：** 每轮开始时各端用 `0x36` 广播自己解析到的下一关身份，接收端只比较身份是否相同。记录本身不携带轮次/关间 epoch/nonce；退役期到达的记录还会被暂存并在新轮次重放。旧轮次迟到的 `0x36`（可靠传输重发、关间处理窗口）如果在下一轮到达，会被当成“对端解析到不同关卡”，触发退回大厅，即使本轮各端身份一致；错误或恶意客户端也可以发送任意身份强制全员退出。
- **依据：** [记录结构](../../src/net/lan_messages/cooperative.rs#L246)只有 `map_file`/`source_sha256`/`effective_digest`/`config_sha256`，[编码/解码](../../src/net/lan_messages/cooperative.rs#L254)没有轮次字段；[接收](../../src/app/lan_service/cooperative.rs#L684)直接解码为 StageIdentity 事件，既不校验发送者也不校验所属轮次（对照同文件 [关间 epoch 检查](../../src/app/lan_service/cooperative.rs#L646)要求发送者是房主且 epoch 不得复用）；[身份比较](../../src/app/match_runtime/session/cooperative_round.rs#L1175)不同即调用 [reject_vera_stage](../../src/app/match_runtime/session/cooperative_round.rs#L1523)发送拒绝并退出；[退役期暂存](../../src/app/lan_service.rs#L1929)与[新轮次重放](../../src/app/lan_service/cooperative.rs#L174)保留到达顺序。
- **修正：** 给 `0x36` 增加轮次绑定（本关结果 nonce、关间 epoch 或每轮递增 ID），接收端丢弃不属于当前轮次的记录，并限制每轮每端只接受一次；保留“双方解析不一致则退回大厅”的原语义，但只对当前轮次的身份生效。
- **验收：** 构造上一轮的 `0x36` 延迟/重复送达下一轮（覆盖退役期暂存和可靠重发），确认新轮次不再被误退回大厅；真实身份不一致仍退回；丢包、乱序、四人与关间新人场景不回归。

### A39 · B92 旧轮次结算记录会在 nonce 校验前触发全员退场 — P2

- **问题与影响：** 结果页各端交换带 nonce 和下一关摘要的结算记录；接收端先比较每槽结果 `entries` 与关身份摘要，最后才比较 nonce。上一轮或错误轮次的记录若保留了相同结果位（例如两关都是全员获胜且角色不变），摘要会用记录携带的旧 nonce 与当前关身份重新计算，必然不匹配，于是被当成“玩家结算不一致”，走 `quit_cooperative_round` 并离开到主界面——即使本轮记录一致。可靠传输重发、关间退役窗口暂存和新轮次重放都可能送达这种旧记录。
- **依据：** [结算记录](../../src/net/lan_messages/cooperative.rs#L27)只有 nonce/entries/confirmed/stage_digest，没有显式轮次；[结算接收](../../src/app/match_runtime/session/cooperative_round.rs#L419)先比较 `entries`（[L452](../../src/app/match_runtime/session/cooperative_round.rs#L452)）和 `stage_digest`（[L456](../../src/app/match_runtime/session/cooperative_round.rs#L456)），`agreement.nonce == record.nonce` 直到 [L482](../../src/app/match_runtime/session/cooperative_round.rs#L482) 才检查；[摘要推导](../../src/app/match_runtime/session/cooperative_round.rs#L138)使用的是记录 nonce 加当前关身份。[接收错误路径](../../src/app/lan_service/cooperative.rs#L621)在 `vmce_result_received` 返回错误时退出轮次并[回到主界面](../../src/app/cooperative_round.rs#L1066)；退役期[暂存](../../src/app/lan_service.rs#L1929)与[重放](../../src/app/lan_service/cooperative.rs#L174)保留到达顺序。
- **修正：** 先按轮次校验 nonce：`agreement.nonce == Some(record.nonce)` 时不匹配就直接丢弃并记日志，不进入 entries/digest 比较；nonce 未知的首包路径保留 used-nonce 检查，但已用 nonce 的记录只丢弃、不触发退场；只有当前 nonce 的 entries 或下一关摘要不一致才视为真实分歧。也接受给记录增加显式轮次字段。
- **验收：** 在房主和客人两侧分别构造上一轮记录在下一轮重放/延迟送达（首包与已建立 nonce 后），确认记录被丢弃且结算继续；当前轮真实 entries/下一关不一致仍退场；重复记录与关间暂存重放不回归。

### B05 全战役逐关配置预校验 — P2

- **问题与影响：** 联机准入已校验整套系列的字节与 `campaign.ini` 身份，但只完整校验 Entry 关的战役配置；后续关卡 `PlayerCount`、角色绑定、`NextScenario` 或控制段错误要等通关切换时才暴露。
- **依据：** [当前关配置加载](../../src/app/shell_network_setup/vmce_map.rs#L559)只在选中地图时校验；[整套系列扫描](../../src/app/shell_network_setup/vmce_map.rs#L770)只对清单和地图字节做摘要。
- **修正：** 在系列发现/准入阶段逐关调用 `load_vmce_scenario` 校验每关配置，缓存结果并纳入现有性能预算；错误在开局或 Ready 前列出具体关卡。
- **验收：** 构造后续关 PlayerCount 越界、NextScenario 缺失、控制段非法等系列；所有端在开局前一致拒绝并指出关卡；合法系列的准入耗时与现有校验一致。

## P3 — 最后修复

### B101 越南语 VMCE 文案缺失 — P3

补齐越南语中的 VMCE 词条并验证语言完整性。

### A05 · B61 房间内修复战役文件后无法重试校验 — P3

- **问题与影响：** 客人因其他关卡内容不一致而被拒绝，随后在房间内替换成正确文件；如果目录、清单和房主公布的摘要没变，旧的不一致结果仍被复用，玩家看不到恢复。
- **依据：** [系列校验器](../../src/app/lan_service/vmce_series.rs#L157)对同一身份返回缓存结果；[身份键](../../src/app/lan_service/vmce_series.rs#L355)不包含本地地图内容。离房或改变身份会取消，但同一身份下没有明确的用户重试入口；[中文提示](../../src/i18n/vera20k.zh-CN.ini#L765)却指导玩家修复文件后再次就绪。
- **修正：** 提供有明确状态的手动重新校验动作，或让既有重试行为使失败结果失效；不要在每帧自动重扫整套地图。
- **验收：** 同一身份下先制造未选地图不一致，再修正文件并触发重试；客人恢复地图可用及准备，且确认待机时不反复扫描。

### A10 · B63 中文外交界面的模式名仍为英文 — P3

- **问题与影响：** 使用简体中文进入多人战役并打开外交界面时，模式名显示 `Multiplayer Campaign`，与大厅的「多人战役」不一致。
- **依据：** [外交视图](../../src/app/input/diplomacy.rs#L322)直接使用模式的名称字段；[模式注册](../../src/skirmish_modes.rs#L374)将该字段设为英文，界面原样绘制。
- **修正：** 在模式名称投影的共同所有者使用已有 `VERA:VmceLobbyTitle` 本地化键，确保房间和外交界面采用同一名称；其他模式保持原有 CSF 路径。
- **验收：** 中文显示「多人战役」，英文显示相应英文，普通多人模式名称不受影响。

### A14 · B67 单人开局提示也出现在客人和多人房间 — P3

- **问题与影响：** 大厅未悬停控件时，客人或已有多人的房间也可能看到「单人也能开始：选好所属方后直接点开始即可」；客人没有房主的开始按钮，提示容易误导。
- **依据：** [提示条件](../../src/app/shell_network_setup.rs#L1090)只检查 VMCE 模式，没有检查房主身份和人数；[状态栏](../../src/app/shell_skirmish.rs#L1320)在无悬停提示时使用它。
- **修正：** 仅在本地是独自一人的房主时显示单人提示；客人与多人房间使用符合实际的准备或等待说明。
- **验收：** 覆盖单人房主、多人房主、未准备客人和已准备客人，并检查玩家加入或离开后提示更新。

### A34 · B86 大厅角色板不显示共享 House 授权 — P3

- **问题与影响：** 地图通过 `[Scenario:<map>:<House>]` 的 `PlayerNControl=yes` 把共享 House 授权给角色。大厅角色板只列主角色的 House、国家、旗帜和颜色，不显示每个角色额外控制的共享 House；玩家在准备和开局前无法确认谁控制哪些共享单位，容易重复操作或无人控制，地图作者也无法在大厅核对授权是否写对。本轮 4p-win 初始 VMCE 大厅截图（`LDA-4p` 系列把 `French House` 的 `Player1Control`/`Player2Control` 授权给角色 1/2）只显示玩家 Owner 与角色板的 Country/Ready/Color，没有任何共享 House 名称或授权提示，支持该问题。
- **依据：** [地图角色详情](../../src/app/shell_network_setup.rs#L144)只有 `owners`/`countries`/`colors`/`briefing`；[角色板](../../src/app/shell_network_setup/view.rs#L359)只用这些主角色字段生成行；共享授权只在[开局 House 授权](../../src/sim/scenario_bootstrap.rs#L3725)按地图 `campaign_player_controls` 生效。指南只说明共享控制存在，不说明其不在大厅显示。
- **修正：** 在既有地图详情/角色板所有者中投影每个角色的共享 House（名称，必要时附国家/颜色），用紧凑行或提示展示；复用已解析的 `VmceMapConfig` 或 `load_vmce_scenario` 的同一控制段读取，不新增第二套 `campaign.ini` 解析。无授权时保持现状。
- **验收：** `campaign.ini` 分别给单角色和多角色写共享 House 时，大厅对应角色显示共享 House；无授权地图不变；2–4 角色、800×600 与缩放布局不重叠。

### A35 · B87 越界 PlayerNControl 角色序号被静默接受 — P3

- **问题与影响：** `[Scenario:<map>:<House>]` 里的 `PlayerNControl=yes` 若 N 超过该关 `PlayerCount`，仍会被接受并写入地图；开局只按实际参战角色的 `player_index` 授权，越界键没有对应角色，所以被静默忽略。地图作者把角色序号写错（例如 `PlayerCount=1` 的关卡写 `Player2Control=yes`）时，共享 House 保持 AI，准入、大厅和结果都不报错。
- **依据：** [控制键白名单](../../src/map/vmce_config.rs#L194)只接受 `PlayerControl` 与 `Player1..4Control`；[关卡配置校验](../../src/app/shell_network_setup/vmce_map.rs#L668)只检查键名和布尔值，未与该关 `PlayerCount` 比较；[授权安装](../../src/sim/scenario_bootstrap.rs#L3725)按参与者的 `assignment.player_index` 读取 `campaign_player_controls`，越界索引无人消费。
- **修正：** 在 `load_vmce_scenario` 或战役控制段的现有校验所有者中，用该关 `PlayerCount` 校验每个控制键的角色序号；越界时以段名、House 和键名报准入错误。保留 `PlayerControl` 旧写法及四角色上限。
- **验收：** `PlayerCount=1/2` 的关卡出现 `Player2..4Control` 时在准入被拒并指出关卡与 House；合法授权不变；`PlayerControl` 仍等价于未写 `Player1Control` 时的角色 1。

### A37 · B89 失败页重试投票缺少逐玩家反馈 — P3

- **问题与影响：** 失败页 BC6 只有本机投票后 Retry 按钮变亮，房主在全部在线客人投票后变亮；3–4 人时其他玩家看不到谁已投票或还差几人，等待期间没有可读进度，容易重复点击或误以为卡住。胜利页每行有“准备”状态，失败页没有对应反馈。
- **依据：** [retry_state](../../src/app/match_runtime/session/cooperative_round.rs#L1118)只返回 `(is_host, lit, enabled)` 聚合，投票数据 `round.retry_votes[row]` 已由 [RetryVote 事件](../../src/app/match_runtime/session/cooperative_round.rs#L1195)在所有端维护，但没有投影到页面；失败页统计行 [VmceStatisticsRow](../../src/ui/campaign_score_shell.rs#L792)没有投票字段，[BC6 失败页绘制](../../src/app/frontend/skirmish_shell_render/coop_score.rs#L237)只画玩家、结果与五列统计。
- **修正：** 在失败页的统计行或标题行显示“已投票”字形/“已投票 N/M”，数据复用现有 `retry_votes`，不新增消息或状态；房主仍按现有聚合启用 Retry。保持四行与 800×600/缩放布局。
- **验收：** 2/3/4 人失败页中任一端投票后各端对应行出现已投票状态且计数一致；房主在全票后启用 Retry、未全票不启用；胜利页与普通 LAN 不变。

### A38 · B90 统计页玩家名未做原生截断 — P3

- **问题与影响：** 胜利/战役完成页的“统计数据”页用固定列宽绘制每行玩家名，但没有像结算板、失败页那样按列宽截断；长玩家名会压到击杀/损失列，四列数值可能被覆盖，3–4 人时更难核对。
- **依据：** [统计页行绘制](../../src/app/frontend/campaign_score_shell_render.rs#L560)直接把 `row.player` 推入固定 `cells`，未调用 `ellipsize_native`；同一渲染器的[结算板](../../src/app/frontend/campaign_score_shell_render.rs#L641)和[失败页](../../src/app/frontend/skirmish_shell_render/coop_score.rs#L251)都对名字调用 `vmce_panel::ellipsize_native`。 B91 的合成页在放入 `settings/Maps` 角色地图后已能启动，但 fixture 玩家名固定为 `LiveHost`/`LiveGuest1-3`，长名字仍无法验证，需要 fixture 支持自定义名字或真机长名。
- **修正：** 统计页玩家名列复用 `vmce_panel::ellipsize_native` 与同一字体宽度测量；数值列保持右/居中对齐。保持四行、800×600 与缩放布局。
- **验收：** 最长玩家名（含中文/双宽字符）在 2–4 行、各缩放与 800×600 下显示省略号且不覆盖数值列；普通长度名字不变；失败页与结算板行为不回归。

### A40 · B94 结算/关间轮次不发送 BC6 保活，也没有对端静默检测 — P3

- **问题与影响：** 结果/关间轮次打开期间，BC6 原生的 20 秒保活 `0x0D`（`0x0046EADD`）没有发送；同时 LAN 主服务在轮次阶段提前返回，跳过 setup 保活和租约轮询。两端都停在结果页且无人操作时没有周期流量，也不会发现对端静默（挂起、半开连接、丢包停摆）；按真实时间等待时只能无限等，或靠用户手动离开。原版会持续刷新对端存活并在失效时走掉线流程。**部分修复：** 阶段 2 验收修复已为 VMCE 结算轮加入应用层心跳与 600 bucket 静默检测（`VERA:VmcePeerUnresponsive`，正常慢操作不计时），关间房间的保活/静默检测仍为残余。
- **依据：** [BC6 保活未发送](../../src/app/cooperative_round.rs#L17)（RESIDUAL 注释，原生 0x0046EADD、20 s）；[LAN 服务在轮次阶段提前返回](../../src/app/lan_service.rs#L1778)，跳过 [announce](../../src/app/lan_service.rs#L1856)、[租约轮询](../../src/app/lan_service.rs#L1853) 和 [setup 保活](../../src/app/lan_service.rs#L877)。
- **修正：** 在 VMCE/co-op 轮次打开期间复用现有保活所有者：周期性发送原生 0x0D（或等价保活）并继续运行租约/对端静默检查；检测到静默时提供可行动的提示并走既有的失败/离开路径，不能静默无限等待。
- **状态（2026-11-13，用户决定）：** 正常玩家等待不得被误判为超时是明确要求，但该边界仍待处理：结算轮已有应用层心跳和静默检测，关间房间保活仍为残余；不能把本子链视为已修复该问题。
- **验收：** 结果页保持打开超过保活/租约周期，抓包确认两端周期保活、租约不过期；暂停一端进程，另一端在有界时间内提示/退出而不是无限等待；正常 Ready、重试与关间 Go 流程不回归。

### A41 · B95 VMCE 开局与关间继续不播放地图场景电影 — P3

- **问题与影响：** 自定义 VMCE 系列若在地图 `[Basic]` 设置 `Intro`/`Brief`/`Action`/`Win`/`Lose`/`PostScore`/`PreMapSelect` 电影，原生战役开局与 BC6 续关路径会播放对应影片，而 VMCE 的关卡开始、胜利/失败结算和关间继续都直接进入结果页或 `begin_loading`，这些过场会被静默跳过。作者用电影承担叙事或关间转场时玩家看不到；不影响胜负、解锁、同步和校验和。
- **依据：** [关间继续电影无所有权](../../src/app/cooperative_round.rs#L975)（RESIDUAL：Win/PostScore/PreMapSelect/Lose 及胜利后的 Intro/Brief/Action 无 owner）；[原生胜利电影入口](../../src/app/campaign_victory.rs#L49)只对 `CampaignScore`/`CampaignVictoryContinue` 返回 `basic.win`，[VMCE 结果目的地](../../src/app/match_runtime/scenario_exit.rs#L155)是 `CooperativeResult`，不经过该入口；[下一关加载](../../src/app/cooperative_round.rs#L1030)直接调用 `LoadingRequest::network`，没有电影阶段；战役开局的 [Intro/Brief 读取](../../src/app/shell_campaign.rs#L304)也未接入 VMCE 的加载路径。
- **修正：** 复用现有 `fullscreen_movie` 所有权：关卡开始按 `Intro`（缺省 `Brief`）、胜利按 `Win → PostScore → PreMapSelect`、失败按 `Lose` 播放地图设置的电影，再进入现有结果页/继续流程；每端本地播放，保持输入锁和返回语义，不改变胜负判定、解锁时机与随机种子；未设置键时保持现状。
- **验收：** 作者化两关系列分别为各键设置影片与留空；2p/4p 各端在开局、胜利、失败和关间继续的对应时点看到同一影片，并能返回正确页面；未设置时无额外停顿；300 个 checksum、结果一致性与解锁流程不回归。

### A42 · B97 关间房间的 Change Map 被禁用却仍按启用态绘制 — P3

- **问题与影响：** 进入关间房间后 `vmce_intermission_map` 非空，`CHOOSE_MAP` 的启用规则与点击处理都把它禁用；但 setup 右栏按钮绘制把 choose_map 的 disabled 参数写死为 `false`，按钮继续显示启用态的红色底与亮黄文字，点击没有任何反应或说明，玩家会以为按钮坏了。普通房主大厅截图显示 Change Map 处于启用态；绘制代码对该按钮的 disabled 参数写死为 `false`，因此关间锁定后仍保持这一外观（本轮尚未采集关间房间截图）；而本轮合成 guest 大厅截图（`vmce-lobby-guest-waiting`）右侧只有 Ready/Leave Room、没有 Change Map，说明客人侧不显示该按钮，问题影响面是房主侧；此前“客人也走同一绘制路径”的判断不准确。
- **依据：** [关间地图锁](../../src/app/shell_network_setup/view.rs#L1034)（`session.vmce_intermission_map().is_some()`）；[启用规则](../../src/ui/multiplayer_setup.rs#L545)（`CHOOSE_MAP => self.host && !self.map_locked`）；[绘制硬编码](../../src/app/frontend/skirmish_shell_render.rs#L396)（choose_map 按钮的 disabled 恒为 `false`）；[点击守卫](../../src/app/shell_network_setup/actions.rs#L66)（关间地图存在时直接返回）。绘制层已有[禁用 alpha](../../src/app/frontend/skirmish_shell_render.rs#L103)，只是没有接到这个按钮。
- **修正：** 绘制使用与命中测试同一 `view.enabled(CHOOSE_MAP)`（或把视图的禁用集合下发给 setup 绘制），让关间房间与客人侧显示原生禁用态；状态栏可补充「关间房间固定下一关」。普通房主房间保持可点击。
- **验收：** 关间房间房主侧的 Change Map 以禁用态绘制、点击无动作（最好有说明）；客人侧本就不显示该按钮，保持现状；普通房主的 Change Map 仍能打开选图；800×600 与缩放截图不回归；开始/准备流程不变。

### A43 · B98 campaign.ini 的未知键与超量 ScenarioN 被静默忽略 — P3

- **问题与影响：** `[Campaign]` 只读取已知键，未知键被静默忽略；`ScenarioN` 只读取 `Scenario1..=ScenarioCount`，超出数量的显式行既不报错也不生效（对比 `[Scenario:...]` 会显式拒绝超出 `PlayerCount` 的 `PlayerN`）。作者把 `ScenarioCount` 写少、残留旧 `ScenarioN` 或把键名拼错时，准入通过但战役在本应存在的关卡处提前结束，该行被无声丢弃；若 `NextScenario` 指向被丢弃的关卡才会报“目标不在清单”，否则玩家只看到少关卡。本轮用含 `Scenario2`（超出 `ScenarioCount=1`）与未知 `Foo=stray` 的 `campaign.ini` 跑 `2p-final`，准入与两端采集均 PASS（300 个 checksum 一致），证实这些键确实被静默忽略。
- **依据：** [Campaign 清单读取](../../src/app/shell_network_setup/vmce_map.rs#L722)按 `ScenarioCount` 循环读 `Scenario1..=count`，没有检查额外 `ScenarioN` 或未知键；[场景段白名单](../../src/app/shell_network_setup/vmce_map.rs#L592)对 `PlayerN` 超出 `PlayerCount` 会显式拒绝，两处校验力度不一致；[控制段白名单](../../src/app/shell_network_setup/vmce_map.rs#L624)同样逐键校验。
- **修正：** 在 `parse_vmce_manifest` 中增加 `[Campaign]` 键白名单（`Name`/`Entry`/`ScenarioCount`/`Scenario1..=ScenarioCount`），并拒绝 `ScenarioN` 中 N 超过 `ScenarioCount` 的显式键；错误包含段名与键名。保留 `ScenarioCount` 上限和 `Entry` 必须属于清单的现有校验。
- **验收：** `ScenarioCount=1` 同时写 `Scenario1`/`Scenario2`、未知键（如拼错的 `Scenariocount` 或额外 `Foo=`）都在准入被拒并指出段/键；合法系列行为不变；B05 的逐关预校验与整套字节身份不受影响。

### A44 · B99 关间房间的客人看不到下一关预览，也收不到简报提示 — P3

- **问题与影响：** 结果页 Continue 后进入关间房间时，每个 peer 先由 `enter_vmce_intermission_setup` 把下一关的 `map_metadata`/`selected_map_file` 写进会话，再用新 protocol 打开 setup。客人收到房主 options 时，`network_map_requires_verification` 比较本地已保留的下一关 metadata 与收到的相同，于是 `map_changed=false`；而 `local_map_verified` 只在 `map_changed` 分支里更新，客人的 `local_preview()` 因此一直是 false，`local_map_index()` 返回 None。渲染层随后走“收到房主预览图”的路径，但 VMCE LAN 从不请求/发送预览，客人的预览框为空；同一 `local_map_index()` 也驱动“该地图有简报”的聊天提示，所以客人也收不到该提示。房主在本地地图带可解码预览时因 `session.host()` 短路仍能看到（本轮把零售 `XMP29U2.MAP` 作为阶段地图后，2p-win 房主大厅截图确认地图预览正常显示）；作者化 `LDA-*` 系列地图本身没有可解码预览数据，因此在该系列上尚未复现 host/guest 差异。另一个独立成因：第一次在关间中途加入的客人走 `open_lan_joined_setup`，它不调用 `refresh_vmce_entries`，而进程启动时场景目录只扫描零售根目录，VMCE 系列记录可能根本不存在，所以即使地图验证通过，`local_map_index()` 也找不到记录。
- **依据：** [关间房间打开](../../src/app/shell_network_setup.rs#L1110)先 `enter_vmce_intermission_setup`（[写入手选地图/元数据](../../src/app/match_runtime/session/cooperative_round.rs#L572)）再新建 setup；[地图变化判定](../../src/app/match_runtime/session.rs#L1646)在 metadata/地图/mode 相同时返回 false；[只在 map_changed 分支更新](../../src/app/shell_network_setup/service.rs#L707) `local_map_verified`；[新 protocol 初始为 false](../../src/app/shell_network_setup.rs#L428) 与 [local_preview 判定](../../src/app/shell_network_setup.rs#L554)；[local_map_index 的非房主门槛](../../src/app/shell_network_setup/view.rs#L460)；[渲染选择预览的路径](../../src/app/frontend/skirmish_shell_render/multiplayer_setup.rs#L110)；[新加入者路径不刷新目录](../../src/app/shell_network_setup.rs#L871) 与 [刷新所有者](../../src/app/shell_network_setup/vmce_map.rs#L376)；[简报提示消费 local_map_index](../../src/app/shell_network_setup/service.rs#L2192)（合成 guest 大厅 fixture 强制 [`local_map_verified=true`](../../src/app/shell_network_setup.rs#L1388) 时预览可正常显示，说明该标志就是预览门槛）；[已有“接受已验证地图源”的 protocol 方法](../../src/app/shell_network_setup.rs#L566)目前只在测试里调用，生产路径没有把它接到 VMCE 客人。
- **修正：** 在 `open_vmce_intermission` 建好新 protocol 后，用会话里已保留的本地已验证地图源（`network_local_map_source()` 与 metadata 摘要）播种 `local_map_verified`/`local_map_loose`，或让 `local_map_index`/`local_preview` 直接消费该已验证来源，而不是依赖一次 `map_changed` 解析（现有 `accept_verified_map_source` 就是为此准备但未接线）；同时把“确保关间下一关在场景目录中”放到任何 peer 应用带 `vmce_intermission` 的 options 的共同路径，不要只在 `open_vmce_intermission` 里做。普通 LAN 与初始房间行为不变。
- **验收：** 2/3/4 人关间房间里，房间打开时已在场的每个客人，以及房间打开后才加入、此前从未进过 VMCE 关间房间的客户端，都看到与房主相同的下一关预览和“该地图有简报”提示；初始 VMCE 房间与普通 LAN 客人不回归；房主预览不变。基于生产调用链静态核对，尚无客人侧关间大厅截图。

### A45 · B100 满员的关间房间在 LAN 浏览器里仍显示可加入 — P3

- **问题与影响：** 关间房间的人数上限按下一关主角色数 clamp(2,4)，但房主回复 LAN Discovery 时固定发送 `advertisement(name, true)`，`open` 位恒为 1。2/2、4/4 已满（或 GO 已锁定）的关间房间仍以可加入状态出现在 LAN 游戏列表；新加入者点击 Join 后被 `join_denial` 以 reason 1 拒绝并弹出 `TXT_GAME_FULL`。这是通用 VERA 房间的状态问题，但关间房间的小上限让 2/2、4/4 成为常见路径。
- **依据：** [Discovery 固定 open=true](../../src/app/lan_service.rs#L2429)；[advertisement 的 open 位](../../src/net/lan_messages.rs#L362)；[人数上限](../../src/app/match_runtime/session.rs#L2113) 已有 `network_room_full()`；[join 拒绝的满员分支](../../src/app/lan_service.rs#L3058) 与 [拒绝原因映射](../../src/app/shell_lan.rs#L337)（reason 1 = `TXT_GAME_FULL`）；[LAN 浏览器只拒绝关闭的房间](../../src/app/shell_lan.rs#L328)；[关间房间上限设置](../../src/app/shell_network_setup.rs#L1156)。
- **修正：** Discovery 回复用 `!session.network_room_full()`（必要时再叠加 `protocol.editable()` 以处理 GO 锁定）作为 `open` 位；房主对每次 Discovery 都重新回答，成员/上限变化后无需额外状态刷新。
- **验收：** 2/2、4/4 的关间房间在 LAN 列表中不可加入（或以关闭态显示）；有人离开后恢复可加入；普通 8 人 LAN 房间的 open 语义与 GO 锁定拒绝不回归。

### B40 地图详情缓存依赖协议存在 — P3

- **问题与影响：** `vmce_map_details` 在每帧 setup view 中运行，缓存未命中会解析地图 INI；缓存只在 `network_setup_protocol` 存在时生效，没有 protocol 的诊断帧可能每帧重复解析。
- **依据：** [地图详情投影](../../src/app/shell_network_setup/view.rs#L118)与角色缓存同 Owner。
- **修正：** 把缓存移到始终存在的所有者，或加帧时间保护。
- **验收：** 无 protocol 的帧不重复解析地图；正常大厅行为不变。

### B41 关间目录记录可能重复 — P3

- **问题与影响：** 下一关不在本地场景目录时，`open_vmce_intermission` 会直接追加合成记录；查找忽略系列目录，重复进入可能累积重复记录。
- **依据：** [关间房间打开](../../src/app/shell_network_setup.rs#L1148)与场景目录所有者分离。
- **修正：** 按 `(系列目录, 地图文件)` 去重，或把「确保下一关在目录中」的职责移入目录所有者。
- **验收：** 重复进入关间房间不产生重复行；下一关仍可选择和开始。

### B45 大厅简报过长被静默截断 — P3

- **问题与影响：** 大厅简报画进 `262 - briefing` 高度的框，出现状态条时从 118px 缩到 64px，既无省略号也无滚动，长 `MISSIONMD.INI` 简报会丢失结尾。
- **依据：** [大厅简报布局](../../src/ui/coop_lobby.rs)。
- **修正：** 在末行加 `...` 或提供滚动，不要静默截断。
- **验收：** 最长简报在状态条显示/隐藏、800×600 与缩放下都能看出被截断；普通简报不变。

### B51 只读大厅板看起来可交互 — P3

- **问题与影响：** board 使用原生 list frame、纯黄文字和与可编辑 combo 相同的字号，却没有 hover/焦点/选中态，容易被当成可操作列表。
- **依据：** [大厅 board 绘制](../../src/ui/coop_lobby.rs)。
- **修正：** 用略暗的值文本色或提示标明只读，或让某一格真正可交互。
- **验收：** 玩家不会把只读格当成可编辑控件；可编辑控件仍保持原交互。

### B52 简报版式与面板不一致 — P3

- **问题与影响：** 任务简报是唯一没有面板的整宽块（`x=74, w=558`，裸文字），在两个有框面板之间形成过长行。
- **依据：** [大厅简报布局](../../src/ui/coop_lobby.rs)。
- **修正：** 像 board 一样加框，或收窄到约 450–500px 的阅读宽度。
- **验收：** 简报在 800×600 与缩放下不出现超长行，位置稳定。

### B54 状态条显示导致布局跳动 — P3

- **问题与影响：** 状态条显示/隐藏会把简报框在 118px 与 64px 间切换并移动下方文字（包括后台校验完成时）。
- **依据：** [状态条与简报布局](../../src/ui/coop_lobby.rs)。
- **修正：** 始终保留状态条区域，或把状态并入 board 底部。
- **验收：** 状态变化时简报和下方控件不跳动。

### B14/B16 错误提示难以触达或归因错误 — P3

- **问题与影响：** 发现阶段的过滤让部分无效战役提示难以触达；设置/冻结失败可能错误归因到本地席位，且存在按翻译后字符串判断错误类型。本轮探测：`Player2` 的 House 名大小写与地图 `[Houses]` 不一致时，准入的 `VmceMapConfig::apply_to_map` 拒绝该系列，`refresh_vmce_entries` 只写日志并跳过，大厅没有可行动提示。
- **依据：** [大厅错误状态](../../src/app/shell_network_setup/view.rs)、[开局准入](../../src/app/shell_network_setup/actions/start.rs)。
- **修正：** 用结构化错误身份代替翻译字符串匹配，让设置/冻结失败指出真正原因和行动。
- **验收：** 无效战役、地图缺失、系列不一致、冻结失败各自显示明确原因；不把其他玩家误记为本地席位。

### B91 合成结算/失败页采集 fixture 角色元数据缺失 — P3

- **问题与影响：** `vmce-result-*` 与 `vmce-defeat-*` 采集检查点曾以 `VMCE defeat fixture needs authored role metadata` 失败：大厅 fixture 把角色详情缓存在 `game_config.paths.ra2_dir` 下，而 `vmce_map_details` 按 `settings_dir/Maps` 查找。
- **依据：** [大厅 fixture 缓存根](../../src/app/shell_network_setup.rs#L1368)与[地图详情查询](../../src/app/shell_network_setup/view.rs#L135)不一致。本轮验证：在 `settings_dir/Maps/XMP29U2.MAP` 放一份带 4 个主角色 `[Basic]` 的地图后，`vmce_map_details` 的 fallback 能加载角色，10 个 `vmce-result-*`/`vmce-defeat-*` 检查点全部产出截图（`.scratch/vmce-synth/`，未提交）；说明失败只缺 settings/Maps 下的角色地图，缓存根/文件身份不一致仍是根因。
- **修正：** 统一 fixture 与投影的缓存根/文件身份（或用独立 fixture 地图放在 settings/Maps 规避），并让 fixture 支持自定义玩家名，以便关闭 A38 的长名字部分。
- **验收：** 两个合成检查点无需 workaround 就能生成截图；A37 能区分投票/未投票状态；A38 能用 fixture 提供的长名字验证截断。

### B93 采集运行器零售标记位置不一致 — P3

- **问题与影响：** `vmce_lan_acceptance run` 只把 `.vera20k-lan-capture-root` 写进每个进程的 settings，而采集路径按 `assets.ra2_dir()` 检查，普通 `--retail-source` 会失败。
- **依据：** [隔离根检查](../../src/app/diagnostics/shell_capture/movies/lan.rs#L1163)与运行器 `write_process_settings`。
- **修正：** 把标记写入每个零售副本，或要求并校验传入源已标记，并写入文档。
- **验收：** 用普通 retail 目录和标记副本运行场景都得到一致 PASS；错误提示说明原因。

## 待验证与已知残余

- **B10/A01：** 重试交接已改为应用层提交确认 + `RetryStart` 的有界三段式，对端静默经应用层心跳有界检测，队列满/部分提交/加载阶段超时均有定向自动化测试；关间期限到期时按 Continue 无提示退出的残余已修复（`b2806138e`）：按键与 poll 共用同一分配所有者，期限/入队/传输失败会停止回合、向对端排队 `RoundFailed` 并返回既有可见原因，退出经有界离房后提示；定向测试覆盖期限前、恰好到期、到期后按键、通知入队失败与对端不响应，回退可见失败处理时两条到期测试失败。真实 OS 级丢包与 ACK 丢失的多进程采集、跨机器场景仍未验证，其他边界见完成记录。
- **B57/A02：** 已实现协商后解锁；确认后立即离开结果页尚无直接现场采集，成员离开导致确认集合变化与同批记录顺序的残余见完成记录。
- **B58/A03：** 已实现请求级复验；真实双进程修改后恢复、重复请求和旧扫描迟到仍待验证。
- **B102：** 现有 owner 已覆盖准入判定；房主侧原因提示、关间房间握手本身与真实数据变更采集仍为残余，见完成区。
- **B64/B65/B74/B76/B77/B79/B80/B83/B84：** 伪造来源、丢包、加载失败或原版对照等未完成的验收，继续按完成区各条的验证边界跟踪。
- **B36：** 其他跨机器、关间加入/离开及真实网络故障验收见[英文台账](../vmce-backlog.md#connected-validation-b36)；本机多进程不能替代跨机器验证。
- **结算统计所有权（阶段 3 首个子链）：** 双人本机多进程三个流程已核对两端统计行与模拟一致；三人、四人真实结算统计未跑，多角色归属/去重目前由 VMCE 客户端单元测试覆盖。A19/B71 未修复（被动主 House 仍被显示过滤且轮次人数取过滤后行数）；VMCE 终局快照仍不锁存，胜利统计回退实时事实的行为保持不变。
- **大厅模式事件分派（阶段 3 第二个子链）：** 加入/退出/选图/模式进入的分派由 dispatch 测试（vmce 配置直接使用生产谓词表）和双人 `2p-win` 建房/加入/开局覆盖；C0 的所属方、早到/正常/乱序/模式切换/离房边界由 `lobby_campaign` 生产处理器测试及 LAN/WOL 事件身份测试覆盖，但完整 `service_event` → HostSetup 事务仍无 `AppState` 夹具。真实进程房间内的中途离房、三人/四人大厅未跑。用户已决定：B71 不修；共享所属方战绩保持现状；地图被删除或替换（A20/B72）及同国多所属方（A04/B59）问题不额外兜底。正常玩家等待不得被误判为超时（A40/B94 的存活/超时边界）仍是待处理事项，已移出“不修”区域，本子链未修复。
- **房间队伍席位策略所有权（阶段 3 第四个子链）：** `ModePolicy.room_teams`、`network_team_room` 与 setup 分组由 session 行为测试和双人验收覆盖；本改动无玩家可见变化，验收证明的是无回归；WOL 平衡、旁观者房间与 clan 房间的队伍路径未单独实机复现，三人、四人未跑。
- **开始许可归 VMCE（阶段 3 第五个子链）：** 共享开始命令的 VMCE 分派与 VMCE 所有者的规则归属由静态架构检查钉住，允许分支由双人验收覆盖；拒绝分支与迁移桥拒绝因需要完整 `AppState` 未逐一执行；WOL、混合版本、跨机器、三人/四人未跑。
- **大厅验收工具（阶段 3 第七子链）：** `2p-win` 的席位角色/难度探针实机通过；其他场景与 Cooperative 采集仍用旧脚本，拒绝/离房/重选、WOL、混合版本、跨机器、三人/四人未覆盖。

## 已完成（保留验证边界）

以下原始问题、旧行号与验收描述作为历史保留；实现完成不代表各条残余验证已经完成。

- **B103（已关闭）** 默认窗口改为 1024x768；合并基线已采集 `0x103` 胜利板/统计页与 `0xBC6` 失败页，三目标与 B2 同尺寸基线逐字节一致（2026-11-12）。
- **B105（已关闭）** 桌面默认模拟线程下，合并基线 LAN 验收 4/4、93/93，实时胜利板与失败统计页非空、两端结果字段与 state hash 一致（2026-11-12）。

### VMCE 结算统计所有权分离（2026-11-13，阶段 3 首个子链，待用户审查）

- **结论：** VMCE 的胜利/失败/关间统计不再调用 `Simulation::cooperative_score_rows`。共享层只保留一份只读 House 事实投影 `sim::score::HouseScoreFacts`（由 `Simulation::house_score_facts(owner)` 生成），它只投影一个 House 的既有 `MatchStatistics`/`Economy`/`HouseTracking` 计数，不选择玩家行；`cooperative_score_rows` 仍由 Cooperative 自己决定原版人类 House 行。VMCE 自己的统计所有者为 [vmce_result/statistics.rs](../../src/app/vmce_result/statistics.rs)：按冻结的 `VmceHouseAssignments`、地图角色元数据和 `raw_slot_index` 生成每名参战者一行，共享 House 不是玩家行，重复 House 只计一次（击杀总数与轮次人数按去重后的角色主 House 计）。`0x103` 战役比分模型的构建移入中性的 [campaign_score_model.rs](../../src/app/campaign_score_model.rs)，由离线战役出口和 VMCE 结果页共用；删除了 `sim_tick` 中无调用者的重复 VMCE 比分模型和两份重复的构建辅助。
- **行为保持：** 结果分派、轮次状态、终局计分锁存和 Scenario 随机数未改；VMCE 仍与 Cooperative 同属 `shares_cooperative_result`，因此终局快照不会被锁存，胜利统计的 `Terminal` 来源继续回退到实时事实（与旧实现一致）。被动主 House 的显示过滤、共享 House 不单独成行、失败页实时统计、胜利页布局与交互均保持原样。
- **测试：** 核心 `sim::score` 13 条（默认）和 15 条（vmce，含一条在 VMCE 分配世界中仍按人类 House 选择 Cooperative 行的行为测试）通过；`house_defeat` 恢复空授权源不计入 viewpoints 的覆盖。VMCE 统计所有者 7 条：6 条新增（多角色按 seat 顺序归属各自主 House、共享 House 不成行且不计击杀、重复 House 去重、被动角色行显示零值、终局行优先于实时事实、观察者不成行），另 1 条时间头测试从旧 victory 测试移入；`app::vmce_result` 25、`app::lan_service::vmce_round::tests` 20、`app::lan_service::tests` 29、普通 Cooperative session 回合 7 条通过。架构检查 45 条两配置通过：`cooperative_score_rows` 从共享能力允许清单与引用上限中删除，新增负向 fixture 拒绝 VMCE 统计调用该接口，并新增边界检查断言 Cooperative 行选择不含 VMCE/模式/会话授权入口、共享事实投影不选择行、VMCE 统计所有者不读取 Cooperative 行选择。`check -p vera20k --lib --bins` 与 `--features vmce` 均通过。
- **验收：** 双人本机多进程 `2p-win`/`2p-retry`/`2p-final` 通过，两端各 300 条 checksum/20 个模拟 hash 一致；失败重试两端回执的 `vmce_statistics_rows` 完全一致，胜利页两端回执均为 `dialog.victory=true, rows=2`，最终关两端 `campaign-end`/credits 回执一致，且每端诊断用自己的角色 House 事实校验统计行；详见[基线](../vmce-baseline.md)。
- **残余：** A19/B71（被动主 House 的轮次人数）未修复，用户已决定不修；共享所属方战绩保持现状（共享 House 不单独成行、其统计不并入角色行），同为用户已决定的范围。真实三人、四人结算统计和多角色联机流程仍待阶段整体验收（本子链仅以客户端单元测试覆盖多角色/去重归属）；跨机器、互联网和真实 OS 级丢包不在本子链范围。

### 大厅加入、退出与模式事件分派（2026-11-13 起，阶段 3 第二个子链，含 2026-11-14 验收修复，待用户审查）

- **结论：** 共享大厅不再把 VMCE 的加入、退出和相关模式事件送进 Cooperative 专属入口。新增 [shell_network_setup/dispatch.rs](../../src/app/shell_network_setup/dispatch.rs) 作为模式事件分派所有者：建房打开（BC `0x005ED6FE..0x005ED717`）、加入槽位（`0x005E1945` 的 `+0x1C`/`+0x28`）、选项解析刷新、加入被接受（`0x005E2FA0` 的 `+0x94`）、离房被接受（`0x005E11DC` 的 `+0x90`）、模式进入（`0x005E541D`）、Use Map（`0x005E7160`）和宾客选择国家按原生规则选择模式。加入和离房在 Choose Map 打开时读取窗口中选中的游戏类型；窗口选中 VERA 自有的 Favorites/Online Maps 行（不指向游戏类型）时没有模式槽位，不回退到房间模式；其余事件读取当前房间模式（`[0xA8B23C]`）。
- **C0 的所属方（验收修复）：** C0 不是模式分派事件，但它也不能无条件送进 Cooperative。新 [lobby_campaign.rs](../../src/app/shell_network_setup/lobby_campaign.rs) 按消息自身的安装身份决定归属，并拥有待处理文本：
  - WOL 的 C0 位于房主选项载荷内，归属就是该载荷的 `launch.mode`；只有 Cooperative 载荷的 C0 在同一个 HostSetup 之前应用，VMCE/其他模式载荷里的字段被丢弃。修订号更旧的在线选项由 live_service 的既有 revision 检查挡下。
  - LAN 的 C0 是独立的短选项包，proof 的 `scope`（epoch + revision）就是它跟随的选项修订。文本只在该修订已应用且房间为 Cooperative 时立即应用；旧 epoch、低于下限的旧 revision 丢弃；超前或与当前旧模式相等的文本进入有界待处理队列，由下一次 HostSetup 的模式和 revision 下限决定应用或丢弃。`campaign_options_applied` 在完整 HostSetup 接受后、模式进入前运行，因此“C0 先于切入 Cooperative 的 HostSetup 到达”和反过来的晚到顺序都能得到房主的战役、关卡与阶段国家；C0 的最终应用仍只经过 Cooperative 进度所有者的 `accept_received_campaign`（`receive_campaign` + `receive_stage`），没有第二份战役状态。
  - 发送侧同样按房间归属：Cooperative 的 `publish` 只在已准入的 Cooperative 房间排队 C0；提交非 Cooperative 模式、离开/被踢或排队到期时清理待发 C0；LAN/WOL 的发送队列在出队时再次核对房间，避免旧 C0 借用下一修订的 proof 发出。
- **行为保持：** 房主/客人的加入、退出、选图、准备与开局流程不变；选图窗口的异步结果失效、开局复验与随机角色确定性未改；选图窗口打开时加入/退出仍按窗口所选模式实际调用对应模式槽位。普通 Cooperative 的加入/退出/国家准入/模式进入/Use Map/C0 行为不变；VMCE/其他非 Cooperative 房间收到不适用或延迟 C0 时不调用 Cooperative 专属处理、不改变其战役状态。
- **架构检查：** `lobby_mode_events_dispatch_before_the_cooperative_owner` 现在检查真实 C0 边界，而不是强制无条件转发：共享 setup 文件与服务层仍不得直接调用 Cooperative 模式模块；模式事件仍按 `live_owner_for` 的同一生产谓词表分派（vmce 配置直接使用生产谓词），先扩展模式后 Cooperative；`deliver` 必须同时处理 `CampaignOrigin::Options` 与 `CampaignOrigin::Lan`、按 scope/revision 暂存并用 `campaign_may_apply` 判断；`options_applied` 必须取回待处理文本、观察新 revision 并只在 Cooperative 模式应用；WOL 解码器必须携带载荷模式，LAN 接收器必须携带 proof scope；Cooperative 的 `publish` 与 LAN/WOL 发送队列必须有房间门。上述均有植入式负向 fixture（无条件转发、仅按当前房间模式过滤、丢掉暂存、无条件发布均报错），事件分派与共享房间查询的扩展优先检查保留。
- **测试与验证：** 新增 [lobby_campaign.rs](../../src/app/shell_network_setup/lobby_campaign.rs) 9 条生产处理器测试（普通模式切入 Cooperative 的早到 C0 最终状态、WOL 同载荷早到、正常顺序、乱序晚到、VMCE/其他房间不应用、模式再次切换、离房清理、待发队列清理、同一修订重复应用），使用真实的 `MatchSession`、`MultiplayerShellRuntime`、`CooperativeProgressState` 和 wire 事件身份，不直接调用 Cooperative 状态对象；LAN 接收的 scope 身份 1 条、WOL 载荷身份 1 条。错误实现回退验证：无条件转发使“非 Cooperative 房间”测试失败；只按当前房间模式过滤使早到/VMCE/离房三条失败；关闭晚到 revision 下限使乱序晚到/切换后旧文本测试失败；重复应用推进下限使重复应用测试失败。`app::shell_network_setup` vmce 47/默认 33、`app::shell_wol::live_service` 55、`app::lan_service` vmce 98/默认 42、`app::frontend::skirmish_session::cooperative::tests` 30、architecture guards 46 条两配置、`check -p vera20k --lib --bins` 两配置通过。
- **验收：** fast 构建 `vmce-lobby-c0-c33dc84be-r2`（exe SHA-256 `1038e55c50d426a93664fdb9f251f655b4f246a660f48985dc61fab662ae139d`）的双人本机 `2p-win` 建房、加入、准备、开局通过，两端各 300 条 checksum 与 20 个模拟 hash 一致，下一关回执符合预期；详见[基线](../vmce-baseline.md)。验收跑的是 VMCE 系列，本身不含 Cooperative C0，不能据此宣称 Cooperative 路径已实机验证。
- **未覆盖与用户决定：** 本仓库仍没有可构造的完整 `AppState` 夹具，因此 `service_event` → `dispatch` → HostSetup 事务的整条胶水、以及生产 `AppCampaignVisit` 的字段映射（协议待处理箱 ↔ `lan_service.setup_scope()` ↔ 已准入模式）没有在单元测试内执行；测试覆盖的是同一生产处理器、真实的 LAN scope/WOL 载荷事件身份、真实 `accept_network_host_setup` 事务和接收/解码边界，调用点由架构检查钉住。HostSetup 钩子在后续模式进入步骤之前解析待处理文本；若后续步骤失败，会话/scope 会回滚，但待处理箱下限和已应用的战役不会回滚（原生 C0 应用本就独立于 HostSetup 成功），这是窄窗口后续项，不阻塞本地提交/审查。在线大厅（WOL/relay）的 C0 先到顺序与 LAN 中途离房没有实机复现；真实双进程房间内的中途离房、三人/四人大厅、跨机器和互联网不在本子链范围。B59、B71、B72 按用户决定保留为“不修/不额外兜底”的残余；共享所属方战绩保持现状；正常玩家等待不得被误判为超时（A40/B94）仍是待处理事项，已移出“不修”区域，本子链未修复也不宣称修复。


### 共享大厅难度所有权（2026-11-14，阶段 3 第三个子链，待用户审查）

- **结论：** 大厅难度行是原生的进程级状态 `[0xA8EB68]`（BSS 初始化为 0，由共享 LAN/WOL 设置对话框的难度滑条 `0x732` 写入 0..2），不是 Cooperative 专属状态，但 Rust 侧名为 `cooperative_difficulty`，且 VMCE 结算页在任务开始后仍读取这个实时行。本子链把它收敛为一个共享大厅事实：新增中性类型 `skirmish_launch::LobbyDifficulty`（EASY/NORMAL/HARD，边界行归一为 0..2），权威所有者为 [MultiplayerShellRuntime](../../src/app/frontend/skirmish_session.rs) 的单一 `lobby_difficulty` 字段；LAN 载荷保留原生字段名 `cooperative_difficulty` 与 WOL JSON 字段名，只在接收边界归一一次。VMCE 原先把取行与冻结写在大厅提交的内联闭包里，现在移入 VMCE 自己的 [shell_network_setup/vmce_setup.rs](../../src/app/shell_network_setup/vmce_setup.rs) `frozen_start_houses`：开局提交时从共享所有者取一次写入冻结的 `VmceHouseAssignments.difficulty`，加载器 TriggerType 位与结果页之后只读冻结值（结果页无冻结值时按 NORMAL 行）。Cooperative 的 BC6 轮次和 `0xBC7` 战役结束标签仍读取同一所有者，行为不变。
- **行为保持：** 大厅滑条写入、房主显示、客人 HostSetup 比对/应用、普通 Cooperative 轮次与 VMCE 局内触发难度、结算页显示的取值语义均不变（合法行 0..2 逐值一致）；WOL/LAN 线上字段名与载荷布局不变。唯一的边界差异是收到 0..2 之外的旧/异常行时在解码处归一，而不是把越界行带进后续 `VmceHouseAssignments` 校验。
- **架构检查：** 从共享能力允许清单和所属方映射中删除 `cooperative_difficulty`；新增 VMCE 大厅自有文件清单（`app/shell_network_setup/vmce_setup.rs`、`vmce_setup_stub.rs`、`vmce_map.rs`、`vmce_map_stub.rs`、`app/lan_service/vmce_setup.rs`、`app/lan_service/vmce_setup/`）并把这些文件整体纳入既有依赖扫描：既拒绝直接调用 Cooperative 大厅拥有者，也沿可解析的方法/关联函数/别名包装追间接调用，附植入式直接与间接负向 fixture；新增结果页语义检查，要求 `build_vmce_score_model` 只取 `network_vmce_assignments`/`result_difficulty`，且 `result_difficulty` 不读取实时大厅行；新增开局冻结语义检查，要求 `frozen_start_houses` 读取共享 `lobby_difficulty()` 并写入 `assignments.difficulty`，且共享 `commit_start` 只经该 VMCE 自有入口冻结；删除了不再成立的 `commit_start` 模式检查引用上限（模式判断已移入 VMCE 自有函数）。架构检查 51 条两配置通过。
- **测试：** `app::lan_service::options` vmce 23/默认 13（含新增的线上越界行归一测试与 session `accept_network_host_setup` 应用断言）、`app::frontend::skirmish_session` vmce 79/默认 78（含共享行归一新测试）、`app::shell_network_setup` vmce 47/默认 33、`app::vmce_result` 26（含结果页只读冻结难度测试）、`app::cooperative_round` 两配置编译通过（该模块无单元测试）；`check -p vera20k --lib --bins` 两配置通过。
- **验收：** vmce feature 的 fast 构建 `vmce-lobby-difficulty-r3`（exe SHA-256 `6A578BAD51B1CF38A723278E965B8EB9805C76F05874B91036DAC199A4B50D63`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`/`can_continue=true`，下一关 `stage02` 回执一致；证据在 `.scratch/vmce-lobby-difficulty-accept-r2/`（未提交）。默认 feature 构建不含 `lan-live-vera-campaign-*` 检查点，不能用于该验收。
- **未覆盖与残余：** 仍无可构造的完整 `AppState` 夹具，因此滑条写入 → `shell_network_setup/actions.rs` 冻结闭包 → `frozen_start_houses` 的整条胶水没有单元测试直接执行，覆盖的是同一生产函数、共享所有者、LAN 解码边界和结果页消费者，调用点由架构检查钉住；验收系列不改动难度滑条，不能证明运行时滑条选择的端到端效果；WOL、旧版本客户端、混合构建和跨机器未验证。越界行归一是边界防御性归一，不是原生行为的断言（原生只接受 0..2 的滑条输入；负值按防御处理归一为 Easy，而原生 Cooperative5C31A5 的 other 映射是 Hard，二者只在滑条与 VERA 编码器都写不出的行上不同）。WOL 的 `Options::into_options` 与 LAN 解码共用同一 `from_native_row` 归一，但没有 WOL 侧镜像测试；结果页无冻结分配时回退 NORMAL 行是防御性默认，真实 VMCE 结果页必经冻结分配（旧实现读实时行时通常得到 0，差异不可达）。


### 房间队伍席位策略所有权（2026-11-14，阶段 3 第四个子链，待用户审查）

- **结论：** “本模式的房间是否按队伍席位分组”原本由共享 `MatchSession::network_team_room`（[session.rs](../../src/app/match_runtime/session.rs) `network_team_room`）用 `!launch.mode.class.uses_cooperative_setup()` 判断，即拿 Cooperative 问题回答 VMCE/普通模式的房间策略；setup 视图（[view.rs](../../src/app/shell_network_setup/view.rs) `let grouped = ...`）又用 `!mode_policy.authored_campaign_roles` 复述一次。本子链把决策移入模式自己的策略所有者 [skirmish_modes.rs](../../src/skirmish_modes.rs) 的 `ModePolicy.room_teams`：Battle/ManBattle/Siege/Unholy/FreeForAll 为 true，Cooperative 与 Vmce 为 false。`network_team_room` 现在只合并真正共享的房间事实（Setup 阶段、`several_observers`、`allies_allowed`、非 clan 房间）与该策略字段；VMCE 模式行虽 `allies_allowed: true`，不再需要额外排除。视图里重复的模式判断删除，改为读取同一所有者。
- **行为保持：** 普通模式仍按队伍分组、TEAMS 与 Team 控件可用；Cooperative 与 VMCE 房间仍不分组、不显示队伍控件；`allies_allowed=false`、clan 房间与 `several_observers=false` 的结果不变。这是所有权收敛，不是界面改版；普通房间内玩家可见行为与改动前一致（选图切进 VMCE 再切回的边缘差异见“未覆盖”）。
- **架构检查：** `ModePolicy` 保持穷尽构造，每个模式必须显式写 `room_teams`；`mode_policy_maps_each_mode_without_combining_independent_capabilities` 的全量字面量同步更新。本改动没有新增 VMCE 模式检查，因此不涉及共享能力 allowlist 或 VMCE 共享引用上限；architecture guards 51 条两配置通过。
- **测试：** 核心 `skirmish_modes` 13 条（含逐模式策略字面量）；`app::match_runtime::session` 默认 50 / vmce 60，新增 `room_teams_follow_the_mode_policy` 经真实 `MatchSession::enter_network` + `accept_network_setup` 对 Battle/Cooperative/Vmce 断言分组结果，并覆盖选图切换残留的 `options.teams: Some(2)` 与 `allies_allowed=false`（旧谓词下 Vmce 返回 true，该测试实测失败；修复后通过）；`app::shell_network_setup` vmce 47/默认 33；`check -p vera20k --lib --bins` 两配置通过。
- **验收：** vmce feature 的 fast 构建 `vmce-team-seating-r1`（exe SHA-256 `D9929F3E95CA2DE47D9C9B5A4DE8CAD07CC5E6E3E76F7CE298363C005EB5D981`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`/`can_continue=true`，下一关 `stage02` 回执一致；详见[基线](../vmce-baseline.md)，证据在 `.scratch/vmce-team-seating-accept-r1/`（未提交）。
- **未覆盖：** 本改动不改变玩家可见路径（此前视图的 authored-roles 判断已让 VMCE/Cooperative 不分组），验收证明的是无回归，不能证明“队伍控件由可用变不可用”；WOL 平衡、旁观者房间与 clan 房间的队伍路径只有既有测试，未单独生产复现；三人、四人、跨机器与互联网未跑。另有一个边缘差异：普通模式房主先设置队伍数、再经选图窗口切进 VMCE 时，旧谓词会继续按该队伍数在 VMCE 房间执行队伍席位（`keep_players_on_teams`、自动平衡、保留 `slot.team`、WOL Balance），新策略不会；VMCE 自身界面一致（TEAMS 禁用、行不分组），唯一可见差异是从 VMCE 切回普通模式时，旧实现保留切换前的队伍布局，新实现重新分配。该往返回归现已通过生产选图入口的自动测试修复；真实房间往返尚未采集。

### 开始许可归 VMCE（2026-11-14，阶段 3 第五个子链，待用户审查）

- **结论：** 一次 VMCE 开局是否被允许，包括模式运行时可用性、已选地图、LAN 配置同步、地图配置、已接受共享种子与地图角色解析，以及 “至少一名非旁观玩家” 的最低人数规则，原先把实现内联在共享开始命令 [actions/start.rs](../../src/app/shell_network_setup/actions/start.rs) 的 `mode_start`/`has_minimum_players` 中（后者还包着 `#[cfg(feature = "vmce")]`）。本子链把规则与面向玩家的拒绝原因移入 VMCE 自有大厅模块 [vmce_setup.rs](../../src/app/shell_network_setup/vmce_setup.rs) 的 `VmceStartPermission`/`start_permission`/`minimum_players_met`；共享命令对 VMCE 只保留一次 `is_vmce()` 分派、原生检查顺序、Ready/发布/GO 与错误显示。迁移桥 [vmce_setup_stub.rs](../../src/app/shell_network_setup/vmce_setup_stub.rs) 提供同名槽位并以 “无可用 VMCE 后端” 拒绝，不落到不完整后端。
- **行为保持：** VMCE LAN 的允许/拒绝顺序、日志、错误文案、`Ok(true/false)` 与 GO 前冻结/发布逐语句不变；普通模式与 Cooperative 仍走原 `validate_setup_start`（None/FreeForAllSourceRoles/CooperativePartners）路径。feature-off 构建原本在共享代码里通过 `authored_campaign_map_catalog && !vmce_map::mode_available` 拒绝 VMCE 开局，现在由桥的 `start_permission` 用同一原因拒绝；桥的 `minimum_players_met` 保留原 VMCE 表达式（非空且全员非旁观），因此有玩家的 feature-off 房间仍会走到后端拒绝原因，而不是先被通用最低人数提示挡下（当前 feature-off VMCE 房间本身不可达）。唯一有意的门控变化是被删除的 `#[cfg(feature = "vmce")]`（pinned gate 预算已记录）。
- **架构检查：** 新增 `vmce_start_permission_is_owned_by_the_vmce_lobby_module`：以原始源码函数体断言共享 `mode_start` 必须分派 `vmce_setup::start_permission` 且不含 `resolve_vmce_houses_with_config`/`network_vmce_map_config`/`load_network_map_ini`/`vmce_map::mode_available`/选图文案，`has_minimum_players` 必须分派 `minimum_players_met`，VMCE 所有者持有上述规则，迁移桥保留同名槽位与拒绝原因；pinned gate 预算 `app/shell_network_setup/actions/start.rs` 2→1、`VMCE_PRODUCTION_GATE_TOTAL` 462→461。architecture guards 52 条两配置通过。
- **测试：** `app::shell_network_setup` vmce 48（含新增 `the_vmce_lobby_requires_a_player_and_never_counts_an_observer_as_one`，直接经 VMCE 所有者断言单玩家通过、旁观者/空名单不通过；既有 `vmce_solo_host_passes_opponent_checks_but_observers_do_not` 经生产 `has_minimum_players` 分派仍覆盖该规则）/默认 33；`check -p vera20k --lib --bins` 两配置通过。
- **验收：** vmce feature 的 fast 构建 `vmce-start-permission-r1`（exe SHA-256 `90D71AFD07CC5CD187C88A93FB8A737F7B105AF32C0FE326FC560837E41DC99B`）的双人本机 `2p-win` 建房、加入、准备、开局与下一关通过：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利页 `rows=2`/`can_continue=true`，下一关 `stage02` 回执一致；证据在 `.scratch/vmce-start-permission-accept-r1/`（未提交）。
- **未覆盖：** 开始许可的拒绝路径需要完整 `AppState`（仍无夹具），未在单元或生产测试内逐一执行（无后端、未选图、非 LAN、缺地图配置、共享种子/角色解析失败），由静态架构检查钉住调用与文案归属；双人验收走允许分支，未逐一实机触发拒绝对话；迁移桥（feature-off）的拒绝对话未实机触发（feature-off VMCE 房间当前不可达）；WOL、旧版/混合构建、跨机器、三人/四人未跑。相关既有工程空白：pinned 门控预算表仍含三条历史归零 pin（`app/cooperative_round.rs` 12、`app/lan_service/cooperative.rs` 19、`app/match_runtime/session/cooperative_round.rs` 37），而 guard 在计数为 0 时跳过预算比较，表总和与 `VMCE_PRODUCTION_GATE_TOTAL` 不相等；`actions/start.rs` 的 `resolve_vmce_houses` 仍是仅测试使用的 `#[cfg(feature = "vmce")]` 共享文件门控。两者均为既有问题，未在本子链修复。

### 大厅验收工具：席位角色与难度探针（2026-11-14，阶段 3 第七子链，待用户审查）

- **结论：** 双人本机 VMCE 验收原先只接受房主下发的 Random 角色和默认大厅难度，角色/难度链只有单元与架构检查覆盖。验收工具现在通过生产代码驱动这些输入：VMCE live LAN 的 `--lan-lobby-inputs` 让双方在 setup 的 Team 列（VMCE 角色槽）认领与自身座位行一致的地图作者角色；房主调用生产难度滑条动作处理器把共享大厅难度设为 Hard（行 2）。guest 若在后续材料性 host-options 修订中丢失 Ready，会重新走生产 Accept，而不是只接受第一版选项。
- **实现：** [shell_capture.rs](../../src/app/diagnostics/shell_capture.rs) 新增 `--lan-lobby-inputs`（仅 VMCE live LAN host/guest 合法，stock/其他 checkpoint 在开 socket 前拒绝）；[live.rs](../../src/app/diagnostics/shell_capture/movies/lan/live.rs) 驱动角色/难度、在 Ready 转换中钉住大厅行并要求每个座位持有自己行的角色；`vmce_setup_snapshot`、`vmce_runtime_receipt`、`vmce_lobby_receipt` 增加 `lobby_difficulty`、冻结 `frozen_difficulty`、`role_claims_enabled`/`roles_claimed` 与逐座位角色；[vmce_lan_acceptance.py](../../tools/vmce_lan_acceptance.py) 的 `2p-win` 场景请求探针并新增三项比较：每座位角色等于自身行且两端映射一致、两端大厅难度一致且等于请求行、冻结难度与大厅行一致。角色、难度和 Accept 均直接调用生产动作处理器，未覆盖指针命中或滑杆拖动。
- **验证：** `app::diagnostics::shell_capture` 默认 88 / vmce 100 通过；`check -p vera20k --lib --bins` 两配置 `Finished`；验收工具单元测试 33 条通过（新增比较器与命令行探针测试）。仓库级 `python -m tools.run_tests` 存在与本改动无关的既有失败/环境错误（cargo cache 符号链接测试、macOS notarization、既有 `src/app/restart.rs` 的 `<user-home>` 字面量、既有浏览器切换路径清单），三个本子链文件不属于这些失败范围。vmce feature fast 构建 `vmce-lobby-inputs-r2`（exe SHA-256 `D99162C7438EFE0BD4ADE123C639F281B1AF2772141944A89716FF6634EFA788`）的 `2p-win` 验收 PASS：两端各 300 个 native checksum 与 20 个模拟 hash 一致，胜利 `rows=2`/`can_continue=true`，下一关 `stage02` 一致；两端收据 `lobby_difficulty=2`、`frozen_difficulty=2`，角色映射 `[('LiveHost', 0, 0), ('LiveGuest', 1, 1)]`。证据在 `.scratch/vmce-lobby-inputs-accept-r9/`（未提交）。
- **未覆盖：** 只有 `2p-win` 请求探针，`2p-retry`/`2p-final`/`4p-win` 与普通 Cooperative 采集保持旧脚本（驱动为 VMCE-only，比较器只在场景请求时运行）；探针只覆盖 host/guest 正常允许路径，未覆盖被拒绝的角色/配置修改、关间房间离房/重选、WOL、旧版/混合构建、跨机器和三人/四人房间。本子链只改诊断与验收工具，未改游戏行为。

### B102 规则与扩展身份准入（2026-11-13，已关闭）

- **结论：** 现有 `MultiplayerAgreement`（`version` 含包/网络构建与 Ares/Phobos/NPatch/NPBUG 能力；`rules_digest` 覆盖所选 RULES/LANGRULE/ART、AIMD、模式 INI 与 Ares 数据开关）在 VMCE 首次开局和关间新 scope 的 Options/Ready/GO 握手中生效；`native_ini_hashes` 另在加入时比较个人规则/Options CRC。不同构建或规则在加载前被拒绝（`LanHostBuildDiffers` / `VERA:VmceBlockingVersionMismatch`），旧版本按版本串拒绝。
- **战役数据：** 系列内容摘要覆盖 `campaign.ini` 与全部清单地图，客人在准入时校验；下一关身份绑定进结果摘要；重试沿用已准入地图身份（retry nonce + source/effective digest/config），加载器再次校验物理地图字节与 include digest；规则/扩展数据为进程生命周期。
- **测试与验证：** `b7f42e2aa` 的重试/下一关冻结投影测试；既有系列摘要、`validate_peer_agreement` 与加载器地图字节复验测试。聚焦 60、guards 507/193 两配置、checks 两配置、`2p-win`/`2p-retry` 验收通过（生产代码未变，构建 `b10-ff9ecb725-r1`）。
- **残余：** 客人本地拒绝提示明确，但房主无法在不新增消息的情况下指出对端原因，只会经 B10 有界等待后给通用失败；下一关测试固定的是冻结投影，LAN 关间房间握手本身未由该测试覆盖；`ra2md.csf` 仅展示用未纳入系列摘要；会话中实际修改规则/地图文件的现场采集未做（加载器会拒绝已变地图字节）。

### B5 核心层去耦合（2026-11-12，仅重构）

本系列提交 `da212a8a4`、`86ca439a6`、`e8cfdcb93`、`dff07c1d9`、`7a722853c`、`1a5e02431` 只搬移模块所有权（后两个为 critic 修正与字节/守卫加固），不改协议、玩法、序列化与哈希。

- **已完成：** VMCE 线报文（`0x36`–`0x3E` 与 `VmceSignal` 的 `0x37`–`0x39`）从
  `net/lan_messages/cooperative.rs` 移到 `net/lan_messages/vmce.rs`，共享模块只保留原版 BC6 记录；
  UDP 间歇期传输从 `net/udp.rs` 移到 `net/udp/vmce.rs`，关闭 feature 时字段、模块和状态都不存在，
  共享路径只保留清晰的 gate 接入点；sim 会话辅助从 `sim/scenario_session.rs` 移到
  `sim/scenario_session/vmce.rs`，字段仍由 `ScenarioSession` 独占，三处 `fold_identity` 折叠位置与输入不变。
- **指标：** 共享 owner 生产 feature gate 556 → 503（48 个共享文件；诊断工具另钉 193 条）。该数字只是变更记录，不是解耦完成证据；
  共享接入点与 mode 检查仍刻意保留。
- **验证（代码 `1a5e02431`，不是 `70d569b86` 基线）：** 聚焦测试 core 159（默认）/195（vmce），
  app 275（默认）/396（vmce，4 ignored）；architecture guards 两种配置各 26 条通过（503 条共享 +
  193 条诊断工具固定）；`check -p vera20k --lib --bins` 两种配置通过。标签构建
  `b5-core-1a5e02431-r1` 的 `2p-win` 与 `2p-retry` 双进程验收通过（各 300 个 checksum 一致，重试无失败）；
  早期阶段也已在 `b5-udp-e8cfdcb93-r1` 与 `b5-core-dff07c1d9-r1` 上通过。证据在 `.scratch/b5/`
  （未提交）。本系列另经两次只读 critic 审查：APPROVE WITH NOTES，无阻塞项；可执行意见已在
  `7a722853c` 与 `1a5e02431` 处理。
- **残余：** `finish_lan_intermission` 与 `cancel_retired_lan_sends_to` 仍无直接 UDP 测试（既有缺口，
  本次搬移未改变行为）。
- **未关闭：** B40、B41、B10 以及上方 connected-validation 项继续有效，不因文件搬移关闭。
- **后续（已在 B106 关闭）：战役数据与大厅反向依赖。** 见下方“B106 战役数据所有者”一节。

### B106 战役数据所有者（2026-11-12，已关闭）

由 `a290f94de` 关闭。VMCE 系列读取、身份、整系列摘要与下一关解析从大厅设置模块移到
`src/app/vmce_campaign.rs`；大厅、LAN 准入与结算流程直接调用该所有者。

- **所有者：** 系列根与 `campaign.ini` 清单解析/校验、主客机地图配置解析、
  `local_vmce_series_manifest`/`LocalVmceSeries`、`host_vmce_identity`、`series_content_sha256`、
  `resolve_next_vmce_stage`/`VmceResolvedStage`、`unlock_catalog_stage`，以及读取战役文件的目录发现。
- **大厅：** `refresh_vmce_entries` 是唯一保留的战役接缝，负责移除过期 VMCE 目录记录并插入发现结果，
  输入来自 `AppState`。
- **LAN 与结算：** `lan_service/vmce_series`、`lan_service/vmce_setup/options`、`vmce_result/victory`、
  `vmce_round` 与 A03 诊断直接调用 `vmce_campaign`，不再经大厅读取战役数据。
- **单一所有者保留：** 地图加载/来源、进度存储、会话/网络配置与线协议均未改动；搬移复用它们，
  不复制解析或状态。
- **验证（代码 `a290f94de`）：** 战役所有者 9 条测试，LAN/系列/设置/结算/回合/大厅消费者
  66（vmce）+12（默认）；architecture guards 两种配置各 26 条（504 共享 + 193 诊断固定）；
  `check -p vera20k --lib --bins` 两种配置通过；标签构建 `b106-a290f94de-r1` 的 `2p-win`
  （战役开始、下一关、300 个 checksum 一致）与 `2p-retry`（300 个 checksum 一致）双进程验收通过；
  证据在 `.scratch/b106/`（未提交）。提交另经一次只读 critic 审查：APPROVE WITH NOTES，无阻塞项。
- **残余：** `refresh_vmce_entries`/`discover_entries` 无直接 retain/extend/早退测试（旧 refresh 也未测）；
  `listed_houses` 在所有者与 `map/vmce_config.rs` 仍各有一份（既有、返回形状不同，本次未引入）；
  A03 诊断仍自行重读 `campaign.ini` 的 `[Campaign]` 以构造其 owned-map mutation fixture
  （仅诊断、既有，不是生产所有者）。

### 原 P1 已完成条目

### A01 · B10 结果不一致时可能有人卡在结算页 — P1

- **问题与影响：** 某端发现逐玩家结果或下一关身份不一致时退出；若其最后一条退出消息首次发送丢失，另一端可能仍等待结果确认，不能可靠地共同退出。
- **依据：** [结果包处理](../../src/app/lan_service/cooperative.rs#L621)排队退出后立即返回大厅；[传输释放](../../src/app/lan_service/cooperative.rs#L46)只服务一次，就重置可靠连接和作用域。
- **修正：** 使用现有可靠传输所有者，在退出或拒绝消息得到确认之前保留重传；对超时和对端失联提供明确的本地失败出口。不要增加另一套竞争性的轮次状态。
- **当前状态（2026-11-13，`ff9ecb725` 关闭；`b2806138e` 补关间期限按键残余）：** 结果页离开与拒绝已进入有界排空，并在等待窗口内保留旧作用域以回应重传；超时显示本地错误。同一 600-bucket 期限覆盖关间 epoch/Go、战役结束 Go 以及重试/战役结束的分配握手（房主分配后等 Ack、客人收到 Begin 后等 Go，`awaiting_assignment` 在 Go 到达后结束等待）；已冻结后续的 Go 入队失败也会本地提示；成功、新访问或进入/结束关间时清除；超时先返回大厅再经既有离开失败通道提示，避免在结算页丢失提示；未新增第二套传输或回合状态机，可靠队列、回合状态与线协议不变。队列测试覆盖首包、首个 ACK 丢失与超时，延迟 Go 不会覆盖已退出状态。`b2806138e` 把 Continue 按键与 poll 的分配路径并为同一所有者：关间期限到期、setup FIFO 满、传输失败或缺少服务时停止回合、向对端排队 `RoundFailed` 并返回 `VERA:VmceRoundSendFailed`，由有界离房后的既有通道提示；旧的 Assign 错误路径只记日志、以 `quit_vmce_result` 丢弃回合并立即返回大厅，期限过后、下一次 poll 之前的按键会无提示退出，poll 的期限与 Go 等待路径也改为同一有界失败链（不再本地退出而不告知对端）。
- **验收：** 单元测试覆盖战役结束 Go 未确认超时、ACK 恰在截止 bucket 到达仍算成功、关间/战役/分配等待的过期与清除、房主/客人分配投影（经 Begin），以及既有的离开首包/ACK 丢失/超时与逐目标加载测试；标签构建 `b10-ff9ecb725-r1` 的 `2p-win`、`2p-retry`、`2p-final` 验收通过（各 300 checksum 一致；最终关到达 credits 与 campaign-end）。`b2806138e` 追加 Continue 按键路径测试：期限前保持结算页、ACK 落入期限后成功分配；恰好到期与到期后按键返回 `Failed(VERA:VmceRoundSendFailed)`，有界离房后提示保留，等待中的对端收到 `RoundFailed` 并以 `Exit(true)` 可见退出；通知入队失败（setup FIFO 超过 4096+槽位）仍保留本地原因；回退可见失败处理时两条到期测试失败。聚焦 20 `app::lan_service::vmce_round`、19 `app::vmce_result`、29 `app::lan_service::tests`、7 普通 Cooperative session 回合测试通过；guards 43/43 两配置、checks 两配置通过；fast 构建 `b10-continue-94eea03f6-r3` 的 `2p-win`、`2p-retry`、`2p-final` 通过（各 300 checksum 一致；下一关、重试、最终关进入 credits/结束页的既有回执不变）。残余：丢包/ACK 丢失在可靠队列层模拟，未做真实有损多进程采集；房主开始分配前客人等待没有期限（此时仍可离开）；约 10 秒的客人 Go 等待可能在房主等待时自动退出空闲客人；结果退出路径（关卡不匹配）的本地超时不排队 Leave 记录，对端依靠自身有界等待而非被明确告知；分配/期限的判定与对端通知已在 session/service 所有者层覆盖，但调用 `return_after_handoff_failure` 的 AppState 胶水与 poll 的 Failed 分支仍无 AppState 级测试，实际提示显示未断言。更窄的残余：延迟的 Next 分配依赖关间期限而非单独分配标记；收结果记录不一致时既有 receive 拒绝路径仍只记日志返回（相邻既有路径，本次未改）；分配测试只到 Begin，ACK 测试证明的是队列已排空优先于过期 bucket 而非 ACK 恰在 bucket 内；成功的 Retry 冻结会保留分配期限到下次访问（已追踪，因新回合在 `return_transport` 之后开始而无害）。

### A11 · B64 客人可以发送房主专属的轮次控制消息 — P1

- **问题与影响：** 已加入的客人若发送 `Go` 或 `Begin` 消息，接收端没有确认消息来自房主；在结果握手已完成时，客人可能绕过房主继续或重试的控制，使不同客户端进入不同的后续状态。这里的发送者是已加入的参与者，不假定任意外部地址能够冒充。
- **依据：** [轮次接收](../../src/app/lan_service/cooperative.rs#L563)只检查发送端是在线参与者，就将 `Go`/`Begin` 转成不带来源的事件；[事件处理](../../src/app/match_runtime/session/cooperative_round.rs#L1248)没有核对房主身份，`Go` 只额外检查结果一致性。[Ack 处理](../../src/app/match_runtime/session/cooperative_round.rs#L1268)也未检查本地是否为房主、是否已发起本轮分配或结果是否一致；累计的不同确认达到门槛时就能推进并排队 Go，具体是否达到门槛取决于各来源和状态。
- **修正：** 保留并校验消息来源：只有认证后的本轮房主能发出 `Begin`、轮次选项和 `Go`，仅房主在正确的已分配轮次中、结果一致且确认来自相应客人时计入 Ack；将控制消息绑定当前轮次身份并丢弃旧包。保留正常的多人准备、投票与重传流程。
- **当前状态：** Begin、选项、Ack 和 Go 已携带结果协商的本轮 nonce，并按已接纳的端点检查房主/客人来源；旧版报文不再用于 VMCE 控制。定向测试、本机双人继续与重试及四人继续通过；伪造来源和丢包实机验收尚未完成。端点与 nonce 不构成密码学认证。
- **验收：** 三人房间中由客人向房主和其他客人分别发送伪造的 `Begin`/`Go`、提前 Ack 和重复 Ack，确认均不能绕过闸门；覆盖客人收到 Ack、正常房主继续、旧轮次重放以及多个不同来源达到计数门槛的情况。

### A12 · B65 Begin 丢包或乱序可能让结算流程卡住 — P1

- **问题与影响：** 房主发送 `Begin` 和轮次选项时，若 `Begin` 首包丢失而选项先到达，或两个包乱序，客人可能不发送轮次确认，房主一直等待，无法继续或重试。
- **依据：** [房主分配](../../src/app/match_runtime/session/cooperative_round.rs#L1016)依次排队 `Begin` 和选项；[全局传输入队](../../src/net/global.rs#L373)按到达顺序接收并分别确认，不保证顺序交付。[事件处理](../../src/app/match_runtime/session/cooperative_round.rs#L1248)仅在先前已收到 `Begin` 时处理选项并发送确认；随后到达的 `Begin` 不会补发确认。
- **修正：** 在现有轮次所有者中分别记录本轮 `Begin` 与已验证选项已到达；两者齐备时幂等发送一次确认，不依赖 UDP 到达顺序，并与 A11 的来源及轮次校验一起处理。
- **当前状态：** 已记录两项到达状态并只排队一次 Ack；选项先到和重复 Begin 的定向测试、本机双人和四人正常继续通过，丢首包及多人不同顺序仍待实机验收。
- **验收：** 覆盖 `Options → Begin`、`Begin → Options`、首个 `Begin` 丢失、重复包和三四人不同到达顺序；确认不重复推进，正常情况下每端可继续或重试。

### A22 · B74 单人房主被通用最低人数检查阻止 — P1

- **问题与影响：** 单名房主进入 VMCE 房间、没有大厅 AI 时点击开始，会被当作只有一方而拒绝；与当前界面的单人开始提示及单角色关卡支持相矛盾。
- **依据：** [开始处理](../../src/app/shell_network_setup/actions/start.rs#L41)无条件调用 `has_minimum_players`；[检查函数](../../src/app/shell_network_setup/actions/start.rs#L365)要求至少两个不同 House 或实际 AI 对手，而 VMCE 大厅不提供新的 AI 项。
- **修正：** 在共享的开局准入所有者明确区分 VMCE 战役的一名人类角色与普通遭遇战的对手数要求；继续校验地图角色、全系列文件和无观察者约束。不要仅修改提示绕过真实开局路径。
- **当前状态：** VMCE 的开局准入允许一名非观察者，并跳过不适用于战役的全员结盟拒绝；普通遭遇战仍要求对手。定向准入和角色测试通过；真实 UI 从单人开局到完成、继续仍待验收。
- **验收：** 通过真实 UI 测试单人房主在一至四角色地图开局并完成/继续；双人房间正常；普通非 VMCE 房间的单人无对手仍按既有规则拒绝。

### A24 · B76 重试加载失败消息缺少来源和加载尝试校验 — P1

- **问题与影响：** 在 LAN 的 Prepared 网络加载阶段，接收端若收到格式正确的 VMCE 重试失败消息，就可能中止当前加载并返回大厅；接收代码没有确认发送者属于当前任务，也没有确认当前加载是对应的 VMCE 重试。普通 LAN 加载也可到达此阶段。传输 Active 阶段对未经封装的非进度包还有额外过滤，不能笼统推断所有加载阶段都能注入；实际发包和旧包到达条件仍需实测。
- **依据：** [消息接收](../../src/app/lan_service.rs#L1919)只检查 LAN 会话后调用处理函数；[失败处理](../../src/app/loading/pump.rs#L3429)对活动网络加载执行失败退出；[报文解码](../../src/net/lan_messages/cooperative.rs#L366)不携带加载尝试身份。[准备阶段传输](../../src/net/udp.rs#L2005)接纳全局消息，作用域[不是认证](../../src/net/udp.rs#L1172)。
- **修正：** 复用既有会话/加载所有者，核实发送者属于本次已接纳的在线参战者，并将失败通知绑定活动的 VMCE 重试加载尝试；忽略不匹配的来源、阶段和过期通知，保留合法失败时全员退出的行为。
- **当前状态：** 失败通知已绑定房间作用域和冻结加载尝试，接收端检查在线参与者、阶段及来源端点；本地 UDP 测试覆盖 Prepared 到 Active 和错误作用域，定向测试覆盖错误来源及过期尝试。真实进程伪造和普通 LAN 注入留待后续验证。
- **验收：** 分别向 VMCE 重试加载发送合法失败、非参战端来源及旧尝试通知，确认仅合法通知使各端退出；覆盖普通 LAN 加载和传输阶段边界。

### A25 · B77 加载失败通知可能在送达前被清除 — P1

- **问题与影响：** 一端在 VMCE 首次或重试网络加载期间本地失败，若失败通知进入可靠队列后立即返回 LAN 大厅并重置队列，后台 I/O 可能尚未首次发送，也可能虽发送但尚未得到确认。其他玩家因收不到通知可能留在加载或等待界面。部分构建失败因 A31 连通知都未入队；此处仅描述已入队后的竞态，有别于 A01 中结算退出消息的送达问题。
- **依据：** [失败路径](../../src/app/loading/pump.rs#L3458)排队通知后返回主菜单；[返回 LAN 房间](../../src/app/shell_network_setup.rs#L1539)调用 [restart_visit](../../src/app/lan_service.rs#L1162)，而[传输重置](../../src/net/udp.rs#L1487)清除待发送记录和全局队列。发送与后台 I/O 并发，故不能断言每次都会在首发前清除。
- **修正：** 在现有传输/失败退出所有者中引入有界的通知排空与确认阶段；全部目标确认后或发生明确超时再清理，超时要提供可理解的退出结果。与 A24 的来源、尝试校验共同设计。
- **当前状态：** 失败通知经现有可靠队列发送，退出前有界等待确认，超时或发送失败时提示可能未通知所有玩家；定向测试覆盖多目标部分确认及既有重发、超时路径。本机双进程注入客人重试地图缺失后，房主收到失败且双方退出加载；其他生产加载故障场景留待后续验证。
- **验收：** 经生产加载路径注入一端失败，验证其他端收到通知并退出；覆盖首包或 ACK 丢失、对端无响应、队列满及多目标部分确认。

### A27 · B79 VMCE 的 AllowWin 胜利门待端到端验收 — P1

- **问题与影响：** 审查基线中，VMCE 作为网络模式会绕过动作 15 的 AllowWin 标签计数门，可能比原版单人战役更早进入共同结算。扫描的 50 张零售战役/触发地图均未写动作 15；自定义 VMCE 系列地图使用该标签时受影响。实现已调整，尚需端到端验收。
- **依据：** 已在零售 `gamemd.exe` 上做指令级核对：`HouseClass::Update`（0x004F8440，0x004F8448 `xor ebp,ebp`）在 0x004F85F7 比较全局 `GameMode`（0x00A8B238）与 0，非零跳 0x004F860B 绕过标签门；只有 `GameMode==0` 时 0x004F85FF 才以 `cmp dword ptr [esi+0x27C], ebp` / `jg 0x004F86FE` 跳过胜利路径。AllowWin 标签析构 0x006E50A0 递减 House+0x27C，0x006E50B1 起把 House+0x298 计时器设为 60 帧（0x3C）；计数清零、登记与析构入口见 [House 字段注释](../../src/sim/house_state.rs#L577)。审查基线的 [House 更新](../../src/sim/world/house_defeat.rs#L257)把非零网络模式传给 [pending_outcome](../../src/sim/house_state.rs#L2920)，后者在该模式下不要求 `allow_win_tags <= 0`。审查基线中的实现与原版网络模式分支一致；现已按下述产品选择调整 VMCE。
- **当前状态：** VMCE 已采用战役的所属方胜利门，其他玩家的胜利或失败仍可先触发全局结算；普通 LAN/遭遇战不变。House 结果测试覆盖标签阻挡、移除后的 60 帧计时和其他玩家先结算；带动作 15 的测试地图和原版/多人实机验收仍未完成。
- **验收：** 构造使用动作 15 的测试地图，分别验证原版单人战役、原版网络模式与 VMCE 在 AllowWin 标签移除前后的胜利时点、60 帧计时器与同帧顺序；覆盖敌对玩家失败先到期、多人一致性及未使用标签的普通胜利。

### A28 · B80 任务退出框仍可能提供 Observe — P1

- **问题与影响：** 多名玩家的主 House 各不相同时，任务内退出确认框可能出现 Observe，违背 VMCE 不需要观察者的要求。点击后不是正常离房：它排队本方 `DESTRUCT` 并留在任务中；而 VMCE 抑制通常的空 House 败北处理，因此还可能让菜单闸门不能按普通多人路径重新打开。具体表现尚需运行验证。
- **依据：** [确认框选择](../../src/app/input/abort.rs#L28)按通用网络模式与存活 House 数提供 Observe，仅对共享同一主 House 的队友隐藏；[点击处理](../../src/app/in_game.rs#L533)排队自毁而不发送退出；[任务 House 处理](../../src/sim/world/house_defeat.rs#L245)对 VMCE 禁用普通空 House 败北门。[菜单准入](../../src/app/in_game.rs#L498)依赖本地败北标志重开清除的闸门。
- **修正：** 在退出确认框及对应执行入口的既有模式/动作所有者中明确排除 VMCE 的 Observe，保留普通 LAN 的原有行为；VMCE 使用正常离房，不能将玩家变为观察者或通过此入口意外自毁。
- **当前状态：** 退出框已隐藏 VMCE Observe，动作执行入口也将 Observe 和带自毁的离开转为正常离房；定向测试覆盖不同主 House 的显示与普通 LAN。真实多人的消灭、共享授权和离房路径仍待验收。
- **验收：** 真实二至四人 VMCE 中打开退出框，覆盖主 House 存活、消灭及共享授权场景；确认不出现 Observe、离房路径正确。普通 LAN 仍保持原有选项。

### A31 · B83 本地构建失败可能完全漏发失败通知 — P1

- **问题与影响：** VMCE 网络加载构建阶段发生错误时，加载会话已从前端状态取走；随后失败处理据此无法识别正在加载 VMCE，因此可能完全不向其他玩家发送失败通知，其他端只能等待超时或停留在后续阶段。
- **依据：** [加载泵](../../src/app/loading/pump.rs#L1692)取走 `loading_session`；[构建失败](../../src/app/loading/pump.rs#L2088)返回失败但没有放回会话；[失败识别](../../src/app/loading/pump.rs#L3371)依赖状态中仍有活动加载会话。与 A25 的入队后重置是先后不同的两个缺口。
- **修正：** 在既有加载/失败所有者中让冻结的 VMCE 身份和加载尝试信息随失败结果存活，确保构建失败能通知各端，并结合 A24、A25 的身份校验和可靠送达。
- **当前状态：** 加载会话被取走后，失败识别改用仍然存活的冻结 VMCE 启动信息；相关加载泵测试和双人正常加载、重试实测通过。准备、构建与渲染准备阶段的真实故障注入仍待验收。
- **验收：** 通过生产路径分别注入准备、构建和渲染准备阶段失败，检查首发、丢包重发、对端退出与本地错误提示；再测试失败时会话仍存在的路径。

### A32 · B84 最终网络安装失败不走全员失败流程 — P1

- **问题与影响：** 各端完成加载进度后，若一端最终安装网络连接或 House 绑定失败，本地仅显示启动失败卡片，不通过加载失败路径通知其他玩家；其他端可能已进入安装或游戏阶段，最终表现需要实测。
- **依据：** [安装调用](../../src/app/loading/transitions.rs#L783)可由 [install_gameplay](../../src/app/lan_service.rs#L3423)返回错误；[错误分支](../../src/app/loading/transitions.rs#L832)只清理 startup 并显示 `StartupRejected`，没有失败通知。[安装交接](../../src/app/loading/pump.rs#L3317)之前加载会话已消费。
- **修正：** 把最终安装失败纳入现有网络加载失败及可靠退出流程，保留已消费会话中的冻结任务/尝试身份；同时避免覆盖安装错误和破坏其他网络模式。
- **当前状态：** 最终安装错误进入统一失败退出流程；安装失败保留传输，启动握手未完成的已进入游戏端可接收通知并停止推进。传输恢复和跨阶段 UDP 定向测试通过；房主及客人的安装失败注入留待后续验证。
- **验收：** 各端到达最终安装时，分别在房主和客人侧注入连接/绑定失败；验证本地错误、对端收到一致失败并可退出，覆盖延迟、丢包和正常成功安装。

### A02 · B57 已确认的胜利仍可能不解锁下一关 — P2

- **问题与影响：** 结果与下一关身份已一致，但有人停在结果页或离开，参与者本机不记录已获得的下一关；用户之后新建房间时无法选它。
- **依据：** [结算退出分支](../../src/app/cooperative_round.rs#L786)只有在 `Exit(true)`、准备切换到关间房间时才调用解锁；[结果一致性握手](../../src/app/match_runtime/session/cooperative_round.rs#L494)本身没有写入解锁。
- **修正：** 把幂等的本地解锁触发移到本轮结果及下一关身份均得到在线成员确认的时点；不要依赖点击继续、进入房间或成功加载。保留单人房主和已离线成员的明确语义，不允许不一致或旧轮次报文解锁。
- **状态（2026-11-13，`9de82144e`）：** 已实现。结果协商确认后立即幂等写入下一关解锁；每收到一条回合记录后即写，位于 Left、投递失败或阶段关闭的早退之前，写入失败不记为已保存（下次 poll 重试）；单人房主走同一路径；重试、最后一关、旧 nonce、结果或身份不一致均不写入。进度存储仍是唯一写入者。
- **验收：** 单元测试覆盖主客机、单人房主、仅提案未确认、重试/最后一关与每轮 nonce 记忆；标签构建 `b57-9de82144e-r1` 的 `2p-win`/`2p-retry` 验收通过，`2p-win` 双方保留的 `VmceProgress.json` 均含 `stage02/all01umd.map`，重试不写。残余：poll 早退顺序与"确认后立即离开结果页"的字面现场采集无直接测试；同一批 proposal + confirmation + `Left` 可能在本地确认发出前退出（未达协商门槛，不写入）；干净 `Left` 会收缩 connected 集合，剩余房主可能写入其本地已解析且受 digest 绑定的关卡，尽管离开者从未确认（Continue 仍等待全部行）。触发点由单元测试覆盖且位于这些早退之前。

### A03 · B58 Ready 后修改其他地图可绕过 Go 前复验 — P2

- **状态：** 已实现请求级复验并通过定向生产回归。
- **问题与影响：** 玩家 Ready 后更改系列中尚未选中的地图，随后开始任务；已验证的清单和当前地图仍相同，开局可能继续使用旧的整套系列结果，不能保证双方文件一致。
- **依据：** [边界票据](../../src/app/lan_service/vmce_setup.rs#L637)绑定所选地图及系列摘要；[票据绑定](../../src/app/lan_service/vmce_setup.rs#L676)在发送 Ready、Go 或 GuestPrepare 前重新核对当前身份，旧票据不能替代新交接。
- **修正：** 为实际 Ready 或 Go 交接分配带身份和代次的一次性复验票据；异步扫描期间保留心跳，并让同一请求的 deferred 重试复用正在进行的扫描。文件身份变化和离房时失效；当前交接失败时撤销准备并显示原因。
- **验收：** 定向生产回归覆盖 Ready 后修改未选中地图、恢复后重新验证，以及旧票据/失败发布对当前交接的影响。原同机验收口径仍是：房主和客人在地图改变后拒绝 Go、恢复后可重新验证开局，并覆盖重复请求与旧扫描结果晚到。

## 已锁定的实现约束

修复下列缺陷时不得改变这些已确定的规则：

- **逐玩家胜负：** 以触发所属方的盟友为胜方、未结盟玩家为败方，触发者也可以是地图 AI；任务中只有地图触发器能改变结盟。以最先到达各自可接受原版结算时点的玩家作为全局结束边界，并在该时点按地图/所属方状态判定其他玩家；某所属方未设置原版胜负标志时按其最近一次地图胜负动作判定。主所属方被消灭不结束任务、不播失败 EVA、不揭全图、不锁操作，之后由该所属方生成的新单位可重新控制；地图授权的 `PlayerNControl` 不随外交改变。
- **结果与继续：** 有人获胜即全员可继续；全员失败由在线客人投票、房主重试，重试保持同一关的冻结角色、地图与随机种子。所有在线客人准备后房主才能继续；胜利/末关的结果页没有强制开始计时器。
- **关间房间：** 全部玩家回到指定下一关的战役房间，可重选角色或保留随机角色，随机分配避开已占角色；可加入/离开，角色数不足时保留房间并阻止开始；该房间不能更换指定下一关。
- **解锁：** 解锁不要求进入下一关，只在所有在线参与者确认本轮结果与下一关身份之后幂等写入；下一关人选由房主从本机已解锁关卡中选择；关间新人只解锁已确定的下一关，不补领此前关卡，断线重连同样处理；同一系列内容更新保留解锁。
- **不引入观察者：** VMCE 最多四个人类角色，任务中不加观察者席位。

## 已完成的验证证据

- 原版依据：`tools/native_inspect` 在零售 `gamemd.exe` 上指令级核对 AllowWin 门（`HouseClass::Update` 0x004F8440，`GameMode` 0x00A8B238，House+0x27C，0x006E50A0 起的 60 帧计时器）。
- 零售数据扫描：`MISSIONMD.INI` 24 个战役段 + `MISSION.INI` 26 个 ALL/SOV/TRN 段共 50 张地图按 8 字段动作块解析 `[Actions]`，动作 15 为 0；`MAPS01/02.MIX` 的 `C*M*` 条目是多人地图，没有 `[Actions]`。
- 合成结算/失败页采集（B91 workaround）：把带 4 个主角色的地图放到 `settings/Maps/XMP29U2.MAP` 后，10 个 `vmce-result-*`/`vmce-defeat-*` 检查点全部产出截图；失败页 Retry 的禁用/点亮外观可区分（A37 的聚合状态），统计页短名字布局正常；长名字仍缺 fixture 支持（A38）。
- 预览能力对照：把零售 `XMP29U2.MAP`（带 `[Preview]`/`[PreviewPack]`）作为 VMCE 阶段地图后，2p-win 房主大厅显示地图预览（截图）；该 run 在任务阶段因该地图没有可用机动单位而失败（测试数据原因），但大厅已确认 host 预览路径正常，A44 的 host/guest 差异仍需带预览地图的关间截图。
- 本机真实双进程：`2p-win`、`2p-retry`（作者化失败系列）均通过；300 个 native checksums 与 20 个 simulation hashes 两端一致；失败页无逐玩家投票状态，初始 VMCE 大厅随机 Owner/等待角色板/简报不可用，与 A07/A13/A37 对应。
- 本机真实三进程：`4p-win --players 3`（`LDA-4p` 四人角色地图）通过；3 个 receipt 的 300 checksums/20 hashes 一致，`dialog.rows=3`、下一关 stage02；大厅截图显示 3 名玩家加 1 个「開放」席位/4 个角色行，结算板 3 行；房主地图预览为空（该系列地图无可解码预览数据）。
- 本机真实四进程：`4p-win` 通过；4 个 receipt 的 300 checksums 与 20 个 hashes 一致；胜利板四行，四人初始 VMCE 大厅无共享 House 授权显示，与 A34 及 B30–B32 对应；初始大厅截图中 Change Map 处于启用态（关间房间截图未采集），与 A42 的绘制判断对应。
- 本机真实双进程（末关/完成流程）：`2p-final` 用 `LDA-2p-final`（Entry=末关，`--entry stage02/all01umd.map`）通过；`lan.vera_outcome.kind="end"`、`completed=true`、`screen=MainMenu`、`network_return=None`，两端 300 checksums 与 20 个 hashes 一致，截图含 `vera-result-real-win`、`vera-campaign-credits`、`vera-campaign-end`（`vera-campaign-end` 实际截到返回后的主菜单，不是完成对话框），支持片尾/返回主菜单流程。VMCE 末关完成走 `vmce_campaign_end` 的片尾/主菜单路径，不打开 0xBC7 完成对话框；`build_campaign_end` 属于旧 Cooperative 路径（最多两人），因此不构成 VMCE 完成页问题。
- 本机真实双进程（阶段 3 结算统计所有权）：fast 构建 `vmce-stats-570af8d90-r2`（SHA-256 `4407334D567700848E04B9EF88FE30201B5778F928E212BC50EE793DCD6D555C`）的 `2p-win`（进入 stage02）、`2p-retry`（同关重载）和 `2p-final`（campaign-end/credits）全部通过，各 300 个 native checksum 与 20 个模拟 hash 两端一致；失败页两端回执的 `vmce_statistics_rows` 相同，胜利/最终关两端 `dialog.rows=2`，每端诊断用自己的角色 House 事实校验统计行。证据在 `.scratch/vmce-stats-accept-r2/`（未提交）。
- 清单校验采集：用 `ScenarioCount=1` 但同时含 `Scenario2` 和未知 `Foo=stray` 的 `campaign.ini` 跑 `2p-final`，两端 PASS（`end/complete`、300 checksum 一致），证明 `[Campaign]` 的额外键/超量 `ScenarioN` 目前被静默忽略（A43）。
- 无效战役探测：`Player2=alliance house`（地图 `[Houses]` 为 `Alliance House`）的 2p 系列在发现阶段被过滤（主机日志 `ignoring VMCE stage ... with invalid configuration`），大厅没有原因提示；该行为归入既有的 B14/B16，而不是开局迟到失败。
- 合成 VMCE 大厅 UI 采集（本机，未提交，输出在 `.scratch/guest-lobby-capture/`）：`vmce-lobby-guest-waiting`、`vmce-lobby-inline-error`、`vmce-lobby-blocking-error` 均产出 800×600 截图。guest 等待页右侧没有 Change Map 按钮（A42 的范围据此改为房主侧）；fixture 强制 `local_map_verified=true` 时下一关预览正常显示（A44 的机制证据）；缺地图的内联橙框与无效战役阻塞框都能正常绘制（弹窗呈现本身不构成问题，静默过滤/归因仍见 B14/B16）。这些是合成 UI，不是原生等价证据。
- 说明：上述采集使用作者化测试系列和本机双/三/四进程，不代表跨机器或零售战役数据验收。

## 已核对但不构成问题的路径

以下路径按当前生产调用链核对，未发现需要修复的玩家可见问题；列出以避免重复追查（新增证据推翻时再改回缺陷）：

- **关间新人的 Ready/解锁提前：** 新人 admission 初始 `ready=false`、`map_status=0`；系列扫描 Pending 时暂扣本地 map status，验收只从 status 1 解锁；进入房间时已清空上一轮的 received options。未发现可提前 Ready 或提前写解锁的入口。
- **新人角色板空角色：** `apply_vmce_roles` 在已绑定行数与角色数不一致时整体推迟，roster revision 会重放 options；该窗口在同一 service pass 内，不会显示空角色（随机角色不可见另见 A07）。
- **离房与 HostLeft：** 离开走可靠 Departure，房主收到 `ParticipantLeft` 后重发 options/roster；房主离开走 HostLeft；`leave_setup` 清理 `vmce_intermission_map`，下一房间不会被上一关锁图。
- **房主缺新人也能开始：** `request_start` 要求其他每个 human 的 map status 非 0 且 `all_ready`；新人 admission 两值初始都不满足；进入 GO 锁定后 `join_denial` 也拒绝后到者。
- **GO/下一关加载身份：** 客人 Ready 的系列边界先启动 revalidation；Go/GuestPrepare 交接持有当前边界票据。A03/B58 的定向生产回归覆盖地图身份变化、旧票据和当前交接失败路径。
- **seed 与随机角色：** 下一关 seed 由上一关 seed 与双方已同意的 stage identity 派生；随机角色分配按 `raw_slot_index` 排序，与 humans 顺序无关。
- **结果页/计分美术泄漏到关间或加载：** `advance_vmce_intermission` 会清 campaign score page/art；失败页只进重试，不进入关间，故不会把上一页残留带进房间或加载。
- **`Start_Scenario` 的加载重试循环：** VMCE 网络加载失败已经由 `fail_loading_with` 通知对端并回到 LAN 大厅（`RetryNetworkShell`），大厅/关间房间保留，房主可再次 Start；原生 `0x00685C47` 的自动重试只是把重试位置放在加载器内，未发现会让玩家卡住或丢状态的独立缺口（失败通知本身见 A25/A31/A32）。
