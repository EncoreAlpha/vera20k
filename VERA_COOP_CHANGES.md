# VERA 合作战役开发记录

这是 VERA 合作战役 fork 的开发记录，不是发布品牌或正式发布说明。开发版本号：`0.1.18-dev`。

## 当前状态

### `0.1.18-dev`

VERA Coop 的 LAN 开局路径已接通。房主点击 Start 时，`mode_start` 先用冻结的外部战役配置、实际选中的 loose 地图和共享 seed 复核角色与 House 分配，通过后放行 GO；接受的开局再按同一份配置冻结分配，加载器在读取地图对象前调用 `initialize_vera_cooperative_houses`，用有效地图建立 sender→House 命令授权表。本地玩家的主 House 成为 `current_house`；任何角色被授权控制的 House 都按共享 `IsHuman` 标记为人类 House（其 AI 不再接管），未被授权的 House 仍由 AI 运行。大厅的 "cooperative campaign launch is not implemented yet" 与加载器的 "shared House command authority is not available" 两处开局拦截均已移除。WOL 仍只做资源一致性检查，不参与开局。

验证结果：`sim::world::world_commands` 17 项、`sim::scenario_bootstrap` 70 项、`app::shell_network_setup` 22 项、`app::input::dispatch` 80 项、`app::loading::init` 39 项、`app::input::context_order` 150 项通过。新增 bootstrap 测试覆盖两名玩家加共享额外 House 的授权表、快照往返和房主/客机 `state_hash` 一致；同时修正两处 fork 回归：stock 模式前缀测试改为按 9 个 retail 模式过滤 VERA Coop 模式，`Command::Select` 快照恢复为不加本地输入门禁（门禁只由 app 输入账本执行）。

尚未做真实双进程 LAN 开局实测，也未验证开局后首帧、触发器、结算与关卡推进，因此不代表合作战役可完整游玩。其他共享模拟和 presentation 路径里的 `current_house` 判断、其他单位命令 handler 和点击路径仍待处理。LAN 玩家仍须预装相同地图和 `campaign.ini`；LAN 只交换摘要，不传输地图或配置。

### `0.1.17-dev` 历史记录

VERA Coop 额外 House 现在可通过普通单元格点击下达移动命令。本地点击沿用 House grant，命令继续使用现有 MegaMission 编码和 network sender；接收端按 sender grant 校验 actor，再由正常 tick 执行。Enemy 和未授权 House 仍不能下令。单人和原版 Cooperative 继续走原有判断。

回归先在旧行为下失败，再通过 sim cell-click producer、native record 编码、network admission 和 DoList tick。验证结果：`move_cell_input` 模块 16 项通过，context-order 直接消费者 1 项通过。测试未覆盖完整 App 鼠标/选择分发、跨端运行、object-target 点击或 building/rally 点击，因此不代表 LAN 实测。

VERA Coop 开局保护仍启用，合作任务尚不能开始（该版本的状态；`0.1.18-dev` 已接通 LAN 开局）。其他共享模拟和 presentation 路径里的 `current_house` 判断、其他单位命令 handler、Engineer/ARES 输入和其他点击路径仍待处理。LAN 玩家仍须预装相同地图和 `campaign.ini`；LAN 只交换摘要，不传输地图或配置。WOL 资源身份同步尚未实现。

### `0.1.16-dev` 历史记录

延续 `0.1.15-dev` 的 VERA Coop House grant 准备路径，并修正 Infantry 第二次 idle fidget 的 `VoiceComment=` Main RNG 消耗：Coop 网络端按各角色主/额外 House grant 的并集，在同一共享条件下消费该 draw；声音仍只由本地 current House 中未选中的单位播放，不扩展到额外 House。未被 grant 的 AI House 不消费该 draw。其他模式保留旧的本地 House/selection gate。定向测试分别切换 local selection 和 current House，并检查无 grant AI；比较 Main RNG 内部状态、下一次 draw、共享模拟哈希及本地声音。

配置根目录为用户设置目录下的 `Maps/VeraCoop`，每个直接子目录包含 `campaign.ini`；清单地图路径相对 `Maps/VeraCoop` 根目录。房主选择地图时，程序发现唯一包含该地图的有效战役、检查其清单地图均存在且路径留在该根目录，并从 `[Scenario:<map>]` 读取角色及 House 控制键。每个角色的主 House 必须不同，地图中的旧合作角色字段不会用于填补缺失角色。

地图选择只在 VERA Coop 模式显示通过校验的战役 Entry；加载时要求 source 是 `Maps/VeraCoop/<listed path>` 对应的 loose 文件，拒绝同名 retail 地图和 MIX 来源。配置会验证 `NextScenario` / `AltNextScenario` 的路径与清单成员关系、`EndOfGame` 布尔格式及物理重复区段/键；实际关卡跳转和结算流程尚未实现。LAN VERA Coop 只交换战役目录名、完整 INI 摘要和所选地图源摘要，不传地图或配置；客机使用本地文件，并在房间准备/开局前重新核对。扩展选项仍沿用 32 KiB 序列化载荷限制。WOL 尚未接入本地资源身份同步。真实双进程 LAN 记录仍只覆盖 `0.1.12-dev` 的 GO 前流程；本轮只有聚焦回归，没有双端实测。通过一致性校验后，VERA Coop 仍由现有开局保护拦截，合作战役尚不能开始。

## 长期路线图（目标，尚未实现）

LAN 合作战役现在可以开始任务（`0.1.18-dev`）；WOL 开局、关卡推进、结算和任务内存档仍未实现。当前已接入战役 Entry 发现、地图清单、当前关配置校验和 LAN 任务启动。玩家须自行预先安装相同地图与战役配置；LAN 不自动传图或同步完整 campaign.ini，并已接入房间资源身份检查；WOL 的身份检查尚未实现。新模式与原 Cooperative 并存，并尽量复用现有战役、联网、保存和地图逻辑；原 Cooperative 及其他模式的既有联网机制不变。以下路线描述尚未完成的目标行为。

### 战役文件与配置

- 每个合作战役（包括单关战役）放在合作专用文件夹根目录下自己的战役文件夹。所有路径都以合作专用文件夹根目录为基准（不是战役子文件夹），允许跨子目录。战役使用一份统一的外部战役 INI，集中定义入口地图、完整地图清单、每关角色与 House 控制关系、关卡跳转。每关使用独立区段。
- 所有合作配置都从外部战役 INI 读取，绝不读取地图内的合作配置。地图内的普通单人规则仍照常读取。旧式每关 `.coop.ini` 和地图内 `PlayerX` 回退均弃用；缺少外部 INI 或必需字段时，不得开始合作任务。
- 角色数必须显式配置；只配置实际使用的 `PlayerX`，`Player1` 也必须明确填写，不回退到 `Player=` 或 Basic Player。当前解析器已接入 `[Campaign]` 的 `Entry`、`ScenarioCount`、`ScenarioN`，以及 `[Scenario:<map>]` 的 `PlayerCount` 和 `Player1` 至 `Player4`；控制键按下方示例读取。该清单和角色配置用于主机选择验证及客机本地推导；LAN 会核对完整配置摘要但不传输完整 INI，关卡推进尚未实现。所有玩家自行安装相同的地图和 campaign.ini。
- 每关的每个 House 区段使用 `PlayerXControl=yes/no` 标记角色控制权，缺省为 `no`。`Player1Control` 缺省时读取同区段 `PlayerControl`；两者冲突时 `Player1Control` 优先。各角色的主所属方必须不同；其他角色对该主所属方的控制标记无效。附加 House 可以由多人共享。控制范围和 AI 行为沿用 VERA 单人逻辑，实施时须核实对应现有逻辑。
- 入口、`NextScenario` / `AltNextScenario` 和全局变量 `1` 分支沿用单人战役的跳转逻辑，但合作数据来自外部 INI。目标必须出现在战役地图清单中。目标缺失或不在清单时报错并留在大厅。清单中任一地图文件缺失时，房主不能开局。缺少配置或入口无效的战役在大厅隐藏并记日志；其他配置不完整时禁止开始。跨关全局变量和战役计时器沿用原单人战役现有继承规则。
- `EndOfGame=yes` 表示战役完成；所有玩家确认结算后回合作大厅。它与 `NextScenario` 同时存在时的优先级留待实现时决定。

### 大厅、任务与联机

- 首版支持最多四个玩家角色及地图基地节点。逐项审查 VERA 已实现的单人战役触发器并为每项制定合作语义；尚未实现的触发器沿用 VERA 现有处理。动画、镜头和语音也逐项审查其合作播放语义。
- 最多四个玩家角色。房主自动准备，客人自行准备。玩家可选角色；分配不能重复，Random 由系统补齐。没有玩家的角色由 AI 操作。同盟与视野遵循地图原有规则。战役配置或地图改变时，清除所有客人的准备状态；角色改变时清除受影响客人的准备状态；房主保持自动准备。
- 大厅可调整游戏速度。胜负由地图触发器决定，胜负结果和任务时间对全队一致；即使所有玩家的主所属 House 被摧毁，任务也继续运行。额外 House 的击杀记到实际 House。计分按各玩家主 House 分别统计。
- 胜利后所有在线玩家确认结算，房主启动下一关；下一关默认保留玩家原角色，若该角色不存在则由玩家重新选择。失败后所有在线玩家确认，房主可选择重试或返回大厅。房主也可主动结束任务并返回大厅。
- 不做 AI 断线接管或中途重新加入。房主断线不会迁移；任务卡住后，剩余玩家各自退出，最后一人退出时结束房间。所有玩家掉线沿用 VERA 现有机制。任务开始后不允许新玩家加入；下一关或读取任务存档时可加入。房主掉线后，其他人完成结算即结束房间，在线玩家可使用自动战役进度重新开房。
- 任一玩家可暂停；只有暂停者或房主能恢复。若两者都掉线，自动恢复。
- 每次战役胜利，为每名在线玩家自动保存一份战役进度；每完成一关保留一份。任一份都可用于重开。战役进度保存全局变量和按角色继承的资金。
- 任务中任何玩家可发起同步保存，但存档文件只写在发起者一端；由该玩家开房恢复。恢复完整任务状态后，所有人重新自由选角色，空缺角色由 AI 补齐。
- 读取战役进度时，如果地图或战役配置已变化，提示房主并由房主决定是否继续；所有客户端一致性检查仍然有效。检查以已安装文件及其摘要为准，不负责分发或覆盖文件。
- 资金按 `PlayerX` 独立继承，AI 角色也包括在内；使用目的地图的 `CarryOverMoney` / `CarryOverCap` 规则。若下一关没有该角色，资金即丢弃，不会在后续角色再次出现时恢复。
- 难度全队统一且只有房主可以更改。进入下一关默认沿用上一关难度；读取跨关战役进度时房主可更改；读取任务中存档时锁定存档难度。

### 预装与兼容边界

- LAN 和 WOL 玩家都须预先安装战役清单中的全部地图及相同的统一战役 INI。LAN VERA Coop 已在房间阶段检查本地清单地图是否齐全，并核对完整 campaign.ini 与当前地图来源摘要；WOL 尚未接入该检查。资源不一致时客机可留在房间，但不能准备/开局，并提示自行安装匹配版本。自动传图、自动安装以及完整 INI 同步已取消，不覆盖同名文件。
- 角色选择等房间状态沿用现有 VERA Coop 状态载荷；LAN 地图与配置摘要只用于一致性检查，不承担文件传输。双端 LAN 实测及 WOL 状态同步仍未完成。
- 原 Cooperative 和其他模式沿用各自现有的 LAN/WOL 机制，不因 VERA Coop 的预装和一致性检查而改变。继续沿用现有 Mod 检测；检测未覆盖的内容由玩家自行保证一致。

### 外部战役 INI 目标示例（字段部分已接入）

下面用于说明目标数据组织方式。玩家须自行安装相同的地图文件和 campaign.ini；此示例不是可分发包，也不表示程序会传输配置。程序会读取入口、地图清单、当前关角色及 House 控制键；`NextScenario` / `AltNextScenario` 目前只验证路径及清单成员关系，不执行跳转。`EndOfGame` 目前只验证布尔格式，不触发结算。LAN 开局前的摘要核对已接入；WOL 尚未接入：

```ini
[Campaign]
Entry=Allied/01.map
ScenarioCount=2
Scenario1=Allied/01.map
Scenario2=Allied/02.map

[Scenario:Allied/01.map]
PlayerCount=2
Player1=Americans
Player2=Russians
NextScenario=Allied/02.map

[Scenario:Allied/01.map:Americans]
PlayerControl=yes
Player1Control=yes

[Scenario:Allied/01.map:Russians]
Player2Control=yes

[Scenario:Allied/01.map:Neutral]
Player1Control=yes
Player2Control=yes

[Scenario:Allied/02.map]
PlayerCount=2
Player1=Americans
Player2=Russians
EndOfGame=yes
```

清单和其他路径均相对 `settings/Maps/VeraCoop` 合作根目录。每关都要单独列出实际角色和其主所属 House；每关各 House 的控制键也在该关自己的配置区段定义。所有玩家必须自行预装相同清单地图及此 campaign.ini；房间计划通过一致性检查确认已安装内容相同，不会传输或覆盖这些文件。示例的 `Neutral` 是可共享的附加 House，避免与角色各自唯一的主 House 混淆。缺少必需值不作地图内回退。

## `0.1.12-dev`

- 用真实两进程 LAN 和用户提供的林德拉克地图副本验证了大厅流程。只有房主提供 sidecar；双方配置摘要一致。客人手动 Ready 后，房主收到客人的准备状态。
- 两端都停在 GO 之前，两名玩家的角色均为 Random，实际开局未验证。当前 13 项 `vera_coop_` 定向测试通过，其中包括四席生产 Session 测试；四人 LAN、共享控制、配置变化后重新准备和 WOL 均未验证。
- 原版 Cooperative 模式中房主会自动准备；VERA Coop 客人需要手动准备。界面继续使用原版按钮。
- 该版本仍不能启动合作任务。

## `0.1.11-dev`

- 增加外部合作配置草稿及地图/配置身份校验：loose 地图从地图旁读取；MIX 地图从游戏目录或 `settings_dir/Maps` 查找；两处都找到配置时拒绝加载。文件上限为 4 KiB，只允许 `[Basic]` 的 `Player1` 至 `Player4` 和地图 `[Houses]` 已列出的 House 段中的 `PlayerControl`、`Player1Control` 至 `Player4Control`。
- 地图或合作配置发生变化后，LAN VERA Coop 房主保持自动准备，其他玩家需重新准备。
- LAN 已完成底层配置处理、定向及单端大厅画面；尚无双端实测，合作任务仍不能开局。WOL 暂无外部配置同步。

### 旧版外部合作配置（`0.1.11-dev` 实验格式）

以下记录的是 `0.1.11-dev` 已实现的旧式 sidecar 实验格式，不是后续合作战役的配置方案。该格式计划弃用：未来合作配置统一放在外部战役 INI，不从地图旁或地图目录加载每关 `.coop.ini`。当前仍不能开始合作战役。

旧实现把 `.coop.ini` 追加在完整地图文件名后，例如 `ALL01UMD.MAP.coop.ini`。loose 地图从地图文件旁读取；MIX 地图从游戏目录或 `settings_dir/Maps` 查找，两处都有时拒绝加载。文件上限 4 KiB。

旧格式只接受 `[Basic]` 的 `Player1` 至 `Player4`，以及地图 `[Houses]` 已列出的 House 段中的 `PlayerControl`、`Player1Control` 至 `Player4Control`。例如：

```ini
[Basic]
Player1=Americans
Player2=Alliance
[Neutral]
Player1Control=yes
```

示例中的 `Americans`、`Alliance` 和 `Neutral` 只有在地图 `[Houses]` 中列出时才适用。旧解析器中 `Player1Control` 显式值优先于 `PlayerControl`；显式为空的 `Player1` 会回退到地图原有 `Player`。这些是旧实现行为，不是已决定的新方案；新方案要求 `Player1` 显式填写且不回退。

## `0.1.10-dev`

- 增加外部合作配置的底层白名单解析与验证：仅支持 `[Basic]` 的 `Player1` 至 `Player4`，以及地图 `[Houses]` 中已列出 House 的 `PlayerControl`、`Player1Control` 至 `Player4Control`。
- `Player1Control` 显式值优先于旧式 `PlayerControl`；外部配置显式清空 `Player1` 时，按地图读取语义回退到 `Player=`。
- 这只是 map 层解析与验证，尚未接入房间同步或开局，不能供游戏实际使用；合作战役仍不能开始。

## `0.1.9-dev`

- 合作大厅显示已选角色的地图所属方颜色，并增加战役简报、难度和游戏速度控件。
- 用盟军战役第一关检查了 800×600 大厅画面；该地图没有内置缩略图，缺少缩略图时显示提示；其他分辨率尚需实际画面验证。
- 仍保留合作战役开局保护；共享所属方的联网控制尚未完成。

## `0.1.8-dev`

- VERA Options 增加 Coop Expansion 开关：开启后，房主可以在原版合作模式之外选择 VERA Coop。默认关闭；扩展战役仍不能开局。

## `0.1.7-dev`

- 合作大厅复用原版侧栏布局，让预览、地图信息和按钮与底图对齐。

## `0.1.6-dev`

- 大厅玩家和角色选择增加清晰边框并对齐，空的观察者占位不再显示。

## `0.1.5-dev`

- 合作大厅使用独立布局：按窗口尺寸调整，角色选择直接对应地图所属方，操作按钮放在地图预览右侧区域。
- 模式名和地图名移到预览框下方。已检查 800×600 的实际画面；其他尺寸仍需实际画面验证。

## `0.1.4-dev`

- 合作角色到主所属 House 的映射会在接收 Go 后冻结，并在加载地图前用已同步角色、地图配置和共享 seed 再次校验。
- 该冻结与复核流程仍受现有 Start 拦截保护；House 初始化尚未接通，VERA Coop 合作战役仍不能开始。

## `0.1.3-dev`

- 开局前会校验大厅角色与地图主所属 House，并检查玩家人数；未指定角色会按共享开局种子可复现地随机分配且不重复。
- 解析出的所属方尚未传入启动冻结或游戏 House 初始化；VERA Coop 合作战役仍无法开始。

## `0.1.2-dev`

- 联机大厅现按地图 `[Basic] Player1` 至 `Player4` 显示角色及所属 House 名称。玩家可选择自己的角色，房主可调整所有玩家；同一角色不可重复，未选角色保持 Random。仅 VERA Coop 使用该控件，原版队伍选择和原 Cooperative 模式保持不变。
- LAN 使用现有 VERA 扩展选项快照同步房主分配，并在玩家选择消息中把客机的角色选择送回房主；原版选项文本格式未改变。观察者身份随快照保留。
- WOL 客户端会随角色选择发送可选字段，但真实在线中继尚未验证该字段能否往返，不能据此认定 WOL 角色已可靠同步。
- VERA Coop 合作开局仍未实现；即使角色和地图配置有效，房主点击 Start 仍会被拒绝。当前功能只完成大厅角色选择与 LAN 状态同步。

## `0.1.1-dev`

- 已加入可选 LAN/WOL 网络模式入口和启动角色校验；当前即使校验通过仍会阻止开局，并提示合作游戏尚未实现。
- 已验证 `Player1Control` 缺省时回退到 `PlayerControl`，显式 `Player1Control` 优先；`Player2Control` 至 `Player4Control` 不回退。

### 旧版地图合作键（历史读取参考）

以下是早期版本读取地图合作键的历史记录，不是新合作战役的配置教程。后续方案从外部战役 INI 读取，地图内合作配置和 `Player=` 回退均不使用。早期地图的 `[Basic]` 用 `Player1` 到 `Player4` 指定合作角色对应的 House 名称；每个值必须与 `[Houses]` 中列出的名称一致。`Player1` 为空或省略时，旧读取逻辑回退到 `Player=`；`Player2` 到 `Player4` 没有旧式回退。例：

```ini
[Basic]
Player1=Americans
Player2=Russians
Player3=YuriCountry
Player4=AmericansAlly
```

这些键只描述角色到主所属 House 的映射，不会自动创建 House。VERA Coop 大厅会将映射与 `[Houses]` 名单核对；启动校验也会拒绝未知或重复的主所属 House，但合作任务目前仍被开局保护拦截，因此这些校验不表示角色控制已接入实际游戏。传统 `PlayerControl` 用于额外标记可控制的 House；角色自己的主所属 House 即使 `PlayerControl=no`，角色仍可控制它。`Player1Control` 至 `Player4Control` 是 VERA 为合作战役增加的扩展键，不是传统地图字段。额外 House 的共享控制及主所属方保护仍未实现：计划中的保护规则是，若某角色的主所属 House 上还设置了其他角色的 `PlayerNControl` 标记，那些额外标记将被忽略。

每个 `[Houses]` 名称对应的同名段可写四个角色控制标记 `Player1Control` 至 `Player4Control`，以及通用 `PlayerControl`。例如：

```ini
[Houses]
0=Americans
1=Russians
2=SharedSupport

[Americans]
PlayerControl=yes
Player1Control=yes
Player2Control=no

[Russians]
PlayerControl=yes

[SharedSupport]
Player1Control=yes
Player2Control=yes
```

示例中的额外 House `SharedSupport` 同时设置 `Player1Control=yes` 和 `Player2Control=yes`，展示一个 House 标记由两个角色共享。`Player1Control` 缺省时回退到通用 `PlayerControl`，若显式设置则以 `Player1Control` 为准；例如 `Americans` 的显式 `Player1Control=yes` 优先于通用值。`Player2Control`、`Player3Control` 和 `Player4Control` 不回退到 `PlayerControl`；例如 `Russians` 虽有 `PlayerControl=yes`，但没有 `Player2Control`，因此不能据此推定其控制 Player2。角色控制标记目前尚未接入实际游戏控制行为。VERA Coop 的可选 LAN/WOL 模式入口和启动角色校验已实现；但当前即使校验通过也会阻止开局，并提示合作游戏尚未实现。计划中的独立补充配置指外部配置文件，目前尚未实现。不要据此认为地图已支持合作游玩。

后续版本按版本号倒序追加，最新记录放在最前面。

## `0.1.0-dev`

- 仅支持读取合作战役中的角色和控制标记；合作战役尚不能游玩。
