# VERA Multiplayer Campaign Expansion 开发记录

本文件记录 VERA Multiplayer Campaign Expansion（VMCE）的开发里程碑、设计决定、验收证据和遗留事项，不是发布说明，也不是玩家手册。玩家安装、写图和配置方法以 [`docs/zh-CN/veramultiplayer-campaign-expansion.md`](docs/zh-CN/veramultiplayer-campaign-expansion.md) 为准。

当前开发版本：`0.1.21-dev`。0.1.21 的主要实现提交为 `5d0437cc9`（未推送）。

## 当前状态

| 项目 | 状态 |
|---|---|
| 2 人 LAN 从大厅开局到结算、胜利后进入下一关 | 已实现，并用真实地图触发器和真实双进程验收 |
| 结果页 ready/Continue | 房主自动 ready，客人 Continue 表示 ready，全员 ready 后只有房主能进入下一关 |
| 地图剧情/共享 House | 不参加多人自动判负；只有角色主 House 的自动判负保留 |
| 任务内战役语义 | 随机箱和 ShortGame 关闭；Special House 自动结盟/Secret Lab 随机分配不执行 |
| VERA 外部战役 `NextScenario` | 胜利后两端本地解析、重算并加载下一关 |
| `AltNextScenario`、`SkipMapSelect`、`EndOfGame` 完成页 | 未实现；`EndOfGame` 目前只阻止出现下一关 |
| 失败重试 | 未实现；失败结果页只能退出 |
| 下一关角色、资金、seed | 角色沿用上一关；资金不继承；沿用上一关 seed |
| 跨关进度、`coopsave.ini` | 不保存，也不写 `coopsave.ini` |
| 3–4 人 | 会话和结算数据结构按最多 4 个角色处理；结果页原生布局仍只绘制两行，未做完整真机验收 |
| WOL/CnCNet | 不支持 |
| 任务内存档/读档、中途加入、掉线 AI 接管、房主迁移 | 不支持 |
| 暂停/恢复、任务中离场、房主强制结束 | 未实现，归入 `0.1.23` 计划 |

## `0.1.21-dev`（已提交）

本版完成两件事：

1. 全量触发语义审计的两个引擎修复：地图 `[Countries]` 合并、大厅难度映射到触发器难度；同时把地图 `[Houses] Allies=` 的盟友图安装进 VERA 模拟。
2. 结算页 ready/Continue 和下一关：房主自动 ready、客人 ready、全员 ready 后只有房主 Continue；胜利且存在 `NextScenario` 时进入下一关，失败/`EndOfGame`/无下一关时只提供退出。结果页不再显示 stock 的国家、颜色、起始位置组合和难度滑杆。

### 触发审计与修复

审计范围是 action 0–145 和 event 0–61（默认构建）。结论是绝大多数行动天然全队共享，或者需要地图作者按角色各写一份；真正需要引擎改代码的只有少数项。审计分报告和决策表在 `.scratch/vera-coop-live/research/`、`decisions.md`，仅作开发证据，不入库。

已实现：

- `RuleSet::merge_map_countries` 只在 VERA Campaign 装载时调用：地图 `[Countries]` 追加到本次场景的 HouseType 注册表，已有国家保持原索引，新国家按地图顺序追加；对应 `[Colors]` 合并并重建颜色渐变。这样作者可以用“不同国家 + 各自 House”给每个角色写触发 owner，而不依赖 `<Player @ X>`。
- 地图 `[Countries]` 合并后，角色主 House、共享 House、触发 owner 和自定义国家都能使用地图新增国家；原 Cooperative 和普通遭遇战的规则注册表与哈希不变。
- `VeraCoopHouseAssignments` 增加冻结的 `difficulty`（0=Easy、1=Normal、2=Hard）：大厅难度由房主选择并在开局冻结，装载器安装 `ScenarioSession::vera_coop_trigger_difficulty`，触发器难度不再恒为 Normal。普通存档没有该行，stock 会话的哈希流不变。
- `install_vera_cooperative_house_authority` 在安装命令授权和人类标记后，安装地图 `[Houses] Allies=` 的盟友图。网络遭遇战只在 GameMode 0 读地图盟友，VERA 没有 launch teams，此前角色主 House 会默认互不结盟；现在按地图原有规则结盟。
- 审计中确认暂不做的项目：`<Player @ X>` 改为作者写自定义国家；同 Country 多角色不折叠，由作者用不同国家规避；51 黑幕、139 雷达归作者；AllowWin(15) 不作合作胜利门；事件 33 不支持，列入待议。

验证：审计修复当时跑过 `rules::ruleset`、`map::houses`、`sim::scenario_session`、`sim::trigger_runtime` 和装载准入相关定向测试；五个真实触发器变体均出现 `VERA Campaign authored alliances installed for ... Houses` 日志。此前的审计证据和逐项结论仍保留在 scratch。

### 结算 ready/Continue 与下一关

- 结果行仍按每名角色主 House 一行，共用 House 不单独占行；行数、结果和时间来自当前结果页模型。VERA 结果页没有单独胜利/失败标题，也不显示难度控件。
- VERA 房主进入 BC6 即自动 ready，并广播自己的 ready；客人按 Continue 只标记自己 ready。全员 ready 后，只有房主 Continue 会返回 `Assign` 并开始下一关；客人 Continue 不能推进下一关。
- 胜利且当前 `campaign.ini` 的 `[Scenario:<当前地图>] NextScenario` 存在时，两端本地解析下一关：按已准入 campaign 的目录与 `campaign.ini` 摘要复核，读取并校验下一关配置和地图，再按与首关相同的冻结开始时序装载。LAN 不传输地图或配置。
- 新 `VeraCoopNextStage` 携带下一关地图路径、外部战役场景配置、地图元数据、本地 loose map source 和 effective digest。`freeze_cooperative_round` 把它和上一关的角色 House 授权一起装进新的 `PreparedNetworkStartup`，同时刷新下一关 LAN 选项使用的 map metadata；下一关加载仍走外部战役路径并再次核对身份。
- 结果页在 VERA 下隐藏 stock 的国家/颜色/起始位置组合、难度滑杆和地图预览，只保留分数行、ready 标记、下一关路径、Continue、Quit 和聊天；`CoopScoreModel.vera` 由结果页拥有者设置，渲染和输入共用。
- 失败、`EndOfGame=yes` 或没有 `NextScenario` 时 `can_continue=false`，结果页只提供 Quit；不写 `coopsave.ini`，不写跨关进度。

验证（真实双进程，2026-11-06；证据在 `.scratch/vera-coop-live/run1/evidence-0.1.21/`，不入库）：

- 真实胜利：`stage01` 的地图触发链以 action 67 结束，两端结果页行数都是 2、任务时间都是 `00:00:23`、`can_continue=true`；房主本地 ready，客人 Continue 后房主 Continue，两端进入 `stage02/all01umd.map`。下一关 native frame 60 两端 `state_hash` 相同，收尾干净；各出 `vera-result-real-win.png`、`vera-next-mission.png`。
- 真实失败：两端 `can_continue=false`、时间 `00:00:13`，只走 Quit 并回合作大厅。
- 真实触发器五变体：之前生成的最小失败/最小胜利/每角色失败/每角色胜利/自定义国家 owner 五张验收图，均两端同帧打开结果页；自定义国家 owner 的触发链被真实解析。详细触发链和结果表在 `evidence-0.1.21/` 的采集收据和日志里。
- 定向测试：`app::match_runtime::session::cooperative_round` 10、`app::shell_network_setup::coop_map` 5、`ui::coop_score_shell` 4、`app::diagnostics::shell_capture` 89、core `sim::scenario_bootstrap` 70、core `sim::world::house_defeat` 21；`cargo build --profile fast --bin vera20k` 通过。

#### 结算后修复：战役地图的空场景 House 不再自动判负

现象：进入下一关时，左上角连续出现 `法国被击败了`、`德国被击败了` 等六条消息；单人战役不会出现。

原因：原生 `HouseClass::Update` 的自动判负门（`0x004F8E86`）只在非战役模式运行；单人战役的 GameMode 0 直接跳过，所以地图里没有单位的剧情 House 不会被判负。VERA 用 LAN 非零 GameMode 跑战役地图，旧逻辑把所有“没有单位、不是 `MultiplayPassive`”的场景 House 都判负，并发出 `TXT_PLAYER_DEFEATED`。

修复：`Simulation::house_rung` 在 VERA Campaign 下只让每名角色主 House（`outcome_viewpoints`）通过自动判负门；共享 House 和地图剧情 House 保持战役/触发器生命周期，不再在开局被标 defeated 或播报。角色主 House 的判负和继续任务行为不变。

验证：新增 `sim::world::house_defeat` 回归 `vera_coop_does_not_auto_defeat_empty_scenario_houses`（21 项该模块测试通过）；真实双进程下一关截图不再出现六条被击败消息，下一关 native frame 60 两端 `state_hash` 仍一致。

#### 战役语义修正：随机箱、ShortGame、Special House/Secret Lab

VERA 跑的是战役地图，但游戏模式非零；只关掉自动判负还不够，以下多人专属步骤也会改变战役：

- `install_vera_cooperative_campaign_options` 在 VERA fresh 装载时关闭 `short_game` 和 `crates`：角色主 House 的判负按“所有对象都没了”计算，而不是只掉建筑；随机箱系统不再运行。
- `finalize_scenario_post_map` 识别 VERA 的 trigger-only 战役语义，跳过 `Post_Map_Init` 的 Special House 全体结盟和 Secret Lab 随机生产；地图作者自己的 `[Houses] Allies=` 图仍由 VERA authority 安装。
- 这两项不改变普通 LAN/遭遇战；普通模式仍按 GameMode 非零和对应 lobby 选项执行。VERA 大厅里的 ShortGame/Crates 复选框保留显示但禁用，避免出现“勾了却不生效”的误解。

验证：新增 `vera_coop_post_map_skips_the_special_house_pass`（9 项 `sim::scenario_post_map` 测试通过）、`vera_coop_campaign_options_keep_role_houses_alive_with_units`（21 项 `sim::world::house_defeat` 测试通过）；真实双进程胜利仍两关同帧，下一关 native frame 60 两端 `state_hash` 一致。

### `0.1.21-dev` 遗留

- 下一关沿用上一关 seed；角色分配沿用上一关；资金不继承。跨关资金、新 seed 和角色重选归 `0.1.22`。
- `AltNextScenario`、`SkipMapSelect`/全局变量分支、`EndOfGame` 战役完成页、失败重试未实现。
- `coopsave.ini` 和持久跨关进度未实现。
- 3–4 人只做到会话/数据结构支持；结果页 ready 图标和分数行仍是原生两行布局，未做完整真机验收。
- `0.1.19` 遗留的“对方 House 的命令被拒绝”运行时断言仍未补；目前由既有单元测试覆盖。
- 结果页“前一个任务”行和描述区仍未接完整的战役身份，内容不完整。

## 下一阶段计划

### `0.1.22-dev` 下一关与战役完成

已完成的基础：单胜利链的 `NextScenario` 解析、冻结下一关配置、下一关装载和结果页 ready/Continue 已在 `0.1.21` 打通。下一阶段补规则和状态：

- 战役身份在比赛期完整携带；全局变量和战役计时器按维度继承。
- 每关使用新的共享 seed（当前复用上一关），并由回合/选项同步给所有玩家。
- 角色保留上一关分配；下一关角色不存在时由玩家重新选择。
- 按角色继承资金，使用目的地图的 `CarryOverMoney` / `CarryOverCap`；沿用现有 `CampaignCarryOver`。
- 实现 `SkipMapSelect=yes` + 全局变量 1 时的 `AltNextScenario` 分支。
- `EndOfGame=yes` 的战役完成页：全员确认后回合作大厅。
- 失败由房主选择重试本关或回大厅；重试保持同一关和同一战役身份。
- 两端 campaign/地图摘要不一致时拒绝继续并提示，不能静默加载。
- 每关胜利的持久进度写盘可以继续留到后续版本；本阶段先完成内存内的下一关、角色/资金和结束规则。

DONE：第 1 关胜利后再进第 2 关且角色、资金、seed 一致；`EndOfGame` 回大厅；失败重试同图；摘要不一致时两端拒绝继续。

### `0.1.23-dev` 边界规则

- 任意玩家可暂停；只有暂停者或房主能恢复；两人都掉线时自动恢复。
- 任务中离场：不接管 AI、不允许中途加入，最后一人离开时结束房间。
- 房主强制结束回大厅。
- 难度仅房主可改且全队统一；游戏速度房主可调。
- 3–4 人真机验收，以及结果页 3–4 行布局（如果仍沿用原生两行画面，需要明确这是展示限制还是做扩展布局）。

DONE：状态机定向测试 + 双进程各跑一遍暂停/恢复/离场。

### 暂不排期

WOL/CnCNet 开局与身份同步、任务内存档/读档、AI 断线接管、中途加入、房主迁移、完整跨关进度持久化、逐项触发器合作语义审查、原 Cooperative 与普通联机行为变更。

## 当前实现边界

- 只支持 LAN；WOL/CnCNet 不支持。
- 玩家需自行安装相同地图和 `settings/Maps/VeraCoop/<campaign>/campaign.ini`；LAN 只核对摘要，不传地图或配置。
- 配置格式、字段、64 KiB 上限、地图必须为 loose `.map`、角色 House 必须互不相同等规则以 VMCE 说明书为准。
- 地图 `[Countries]` 合并、`[Colors]` 合并和 `[Houses] Allies=` 安装只在 VERA 装载路径生效；地图 `[Sides]` 不合并。
- 结果页复用原生 BC6 对话框：行数由角色主 House 数决定，但原生布局只有两行，超过两行的分数/ready 显示需要后续补。
- `coopsave.ini` 不写；当前关卡推进只来自外部 `campaign.ini` 的 `NextScenario` 和内存状态，不保存持久跨关进度。

## 历史版本

### `0.1.20-dev`（已提交）

完成团队胜负和最小结算闭环：VERA 的胜负只来自地图触发器的 outcome 动作（1/67 胜、2/68 负）；主 House 全灭不再自动多人结算，每个角色主 House 标为 defeated 后任务继续。结果路由复用 `CooperativeResult`：两端各自进入 BC6，每名角色主 House 一行，共用 House 不另起行；首版结果页只有 Quit，`can_continue=false`，不写 `coopsave.ini`，也不选下一关。房主强制结束作为后续边界规则，未在本版验证。

实现要点：

- `Simulation::outcome_viewpoints` 返回每个网络 source 的主 House；结果行和胜负标记共用这份判定。普通联机仍按真人 House 选择，离线仍按当前 House。
- `multiplayer_decision` 在 VERA 下返回 `None`；`complete_multiplayer_after_defeat` 只标记 House，不结束任务。
- `request_player_outcome` 在 VERA 下对全部角色主 House 应用团队结果；动作 69 保留先前结果；原 Cooperative 的逐 PlayerPtr 判定不变。
- 结果页模式标签用 VERA 模式行自身文本；装载器在 `install_vera_cooperative_house_authority` 里安装已准入的 mode class。
- 采集目标新增 `--vera-outcome win|lose|houses`，在同一 committed frame 通过生产 outcome 所有者注入，并登记结果页、继续任务后和任务全景 PNG。

验证：win/lose 两端进入 BC6，行数 2、`can_continue=false`，注入帧同为 native frame 1400，Quit 后回大厅；300/300 帧校验和与 20 个固定帧 `state_hash` 一致。houses 模式把两端两个角色主 House 标为 defeated 后没有结算页、任务继续。真实地图触发器和真实下一关由 `0.1.21` 接续验证。

当时的遗留：真实地图触发器到结局、ready/Continue 和下一关已由 `0.1.21` 接续；结果页“前一个任务”行/描述区仍不完整；`0.1.19` 的对方 House 命令拒绝运行时断言仍未补。

### `0.1.19-dev`（已提交）

完成真实双进程 LAN 开局实测基座：房主 Start、双方 GO、装载进入任务、300 帧 Move/Stop、逐帧校验和与固定帧 `state_hash` 采集；收据增加 VERA 的战役身份、角色名单、sender→House grant 矩阵和越权点击探针。

当时暴露并修复的阻塞：

- 客机先收到 options、后收到 roster 时整包被丢弃；现在缓存 options，roster 到达后重放。
- 客机预装战役 Entry 不在 Choose Map 目录，map_status 恒为 0；现在按本地 campaign.ini 和地图摘要准入，并绑定为 GO 前探针和装载 source。
- 恢复 VERA 房间时 campaign Entry 不在目录；建房间前先发现 Entry。
- 恢复 LAN 存档时两个座位都继承 role 0；VERA 座位默认 Random，角色只在大厅显式选择。
- 装载画面组装对 VERA 误用 stock-offline 访问器导致 panic；改为使用冻结的 network launch 名单。
- 装载器 fresh prefix 选择处 panic；新增 VERA Campaign fresh prefix 族，按 campaign 顺序建 authored House roster，populate 只安装冻结授权。
- 测试地图开场锁定输入；诊断目标等解锁后下发 Move 并记录延迟。
- 双进程收尾竞态；先完成的一端保持传输等另一端落定。

验证：两端完整走完房间发现、加入、Ready/Start/GO、装载进任务；Move/Stop 在解锁后下发并通过；300/300 校验和、20 个固定帧 `state_hash`、grant 矩阵、地图/战役身份一致。第 2 关改用第 1 关地图拷贝后重跑，结果不变。

遗留：显式选角色和“对方 House 命令被拒绝”的运行时断言未做；客机 Accept 瞬间本地 Host 行 Ready 显示有时序问题；任务内保存、结算和下一关不在本版范围。后续版本已补结算和下一关，但上述运行时断言仍未补。

### `0.1.18-dev`

接通 VERA 的 LAN 开局：房主 Start 时先用冻结的外部战役配置、选中 loose 地图和共享 seed 复核角色与 House 分配，再放行 GO；加载器在读取地图对象前调用 `initialize_vera_cooperative_houses`，建立 sender→House 命令授权表。本地玩家主 House 成为 `current_house`；被授权控制的 House 按共享 `IsHuman` 标记为人类，其 AI 不再接管；未授权 House 仍由 AI 运行。移除大厅和装载器的两处 VERA 开局拦截。

验证：`sim::world::world_commands`、`sim::scenario_bootstrap`、`app::shell_network_setup`、`app::input::dispatch`、`app::loading::init`、`app::input::context_order` 等定向测试通过；新增 bootstrap 测试覆盖两玩家加共享额外 House 的授权表、快照往返和房主/客机 `state_hash` 一致。

本版未做真实双进程开局；`0.1.19` 实测发现装载器 fresh prefix 仍会 panic，随后修复。本版没有覆盖共享模拟和呈现路径里的其他 `current_house` 判断、其他单位命令 handler 和点击路径；LAN 仍要求双方预装相同地图和 `campaign.ini`。

### `0.1.17-dev`

VERA 额外 House 支持普通单元格点击下达移动命令：本地点击沿用 House grant，命令继续使用现有 MegaMission 编码和 network sender；接收端按 sender grant 校验 actor，再由正常 tick 执行。Enemy 和未授权 House 仍不能下令；单人和原版 Cooperative 保持原判断。

回归先以旧行为失败，再通过 sim cell-click producer、native record 编码、network admission 和 DoList tick；`move_cell_input` 模块 16 项、context-order 直接消费者 1 项通过。未覆盖完整 App 鼠标/选择分发、跨端运行、object-target 点击或 building/rally 点击，因此不代表 LAN 实测。

### `0.1.16-dev`

延续 House grant 准备路径，并修正 Infantry 第二次 idle fidget 的 `VoiceComment=` Main RNG 消耗：VERA 网络端按各角色主/额外 House grant 的并集，在同一共享条件下消费该 draw；声音只由本地 current House 中未选中的单位播放，不扩展到额外 House。未被 grant 的 AI House 不消费该 draw；其他模式保留旧的本地 House/selection gate。

同时确定配置根目录为 `settings/Maps/VeraCoop`，每个直接子目录包含 `campaign.ini`；地图路径相对该根目录。房主选图时发现唯一包含该地图的有效战役，检查清单地图存在且路径留在根目录，并从 `[Scenario:<map>]` 读取角色和 House 控制键；每个角色主 House 必须不同，地图旧合作字段不用于填补缺失角色。加载时要求 source 是根目录下清单路径对应的 loose 文件，拒绝同名 retail 地图和 MIX 来源；配置验证 `NextScenario`/`AltNextScenario`/`EndOfGame` 及重复区段/键。LAN 只交换战役目录、完整 INI 摘要和地图源摘要，不传文件。当时实际关卡跳转和结算尚未实现。

### `0.1.12-dev` 及更早

`0.1.15-dev` 至 `0.1.13-dev` 没有留下独立条目，这里不补写；以下仅为现有记录能确认的里程碑。

| 版本 | 里程碑 |
|---|---|
| `0.1.12-dev` | 用真实两进程 LAN 验证大厅流程；只有房主提供 sidecar，双方摘要一致，客人手动 Ready，房主收到状态；两端停在 GO 前，未验证实际开局；13 项 VERA 定向测试通过，含四席生产 Session 测试；四人 LAN、共享控制、WOL 未验证 |
| `0.1.11-dev` | 外部合作配置草稿与地图/配置身份校验；loose 地图从旁读取，MIX 地图从游戏目录或 `settings_dir/Maps` 查找；旧的每关 `.coop.ini` sidecar 和 `[Basic] PlayerX` 方案后来被 `campaign.ini` 取代，仅作历史 |
| `0.1.10-dev` | 外部配置白名单解析：`[Basic] Player1..4` 和地图 `[Houses]` 中已有的 House 控制键；仅 map 层解析，未接房间或开局 |
| `0.1.9-dev` | 合作大厅显示角色颜色，增加战役简报、难度和游戏速度控件；检查 800×600 画面 |
| `0.1.8-dev` | VERA Options 增加多人战役开关；开启后房主可选 VERA Campaign，默认关闭 |
| `0.1.7-dev` | 合作大厅复用原版侧栏布局，使预览、地图信息和按钮与底图对齐 |
| `0.1.6-dev` | 大厅玩家/角色选择边框对齐，空的观察者占位不再显示 |
| `0.1.5-dev` | 合作大厅独立布局：按窗口调整，角色选择对应地图所属方，按钮放在地图预览右侧 |
| `0.1.4-dev` | 合作角色到主 House 的映射在收到 Go 后冻结，装载前用角色、地图配置和共享 seed 再校验 |
| `0.1.3-dev` | 开局前校验大厅角色与地图主 House，检查人数；未指定角色按共享开局种子可复现随机分配且不重复 |
| `0.1.2-dev` | 大厅按地图 `[Basic] Player1..4` 显示角色和 House；LAN 同步角色分配，原 Cooperative 不变 |
| `0.1.1-dev` | 增加可选 LAN/WOL 合作模式入口和启动角色校验；`Player1Control` 缺省回退 `PlayerControl` |
| `0.1.0-dev` | 仅支持读取合作角色和控制标记，合作战役尚不能游玩 |

历史格式说明：`0.1.11-dev` 的 `.coop.ini` sidecar 和更早的地图 `[Basic] PlayerX` 合作键只用于当时实验，已弃用；当前只从外部 `campaign.ini` 读取合作配置，地图内旧合作字段不用于 VERA 角色分配。
