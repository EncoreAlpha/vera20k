---
layout: default
title: VMCE 审查总文档
lang: zh-CN
---

# VMCE 审查总文档

[中文说明书](vmce-guide.md) · [中文首页](vmce.md) · [English backlog](../vmce-backlog.md)

本页合并此前的中文审查页与中文待办，只保留**需要修复**的缺陷与实现时必须遵守的已锁定决定，并按修复优先级排列；A/B 编号保持不变，供提交、测试和旧页交叉引用。已实现项、纯验收任务与暂缓项不进入本页。

- **P1**：可能让整场联机卡死、错误结束、丢失胜利或解锁、无法继续，先修。
- **P2**：常见路径上的错误结果、错误阻挡或明显不一致，P1 后修。
- **P3**：特定输入、文案或体验问题，最后修。

| 优先级 | 条目 | 来自 A 审查 | 仅 B 待办 |
|---|---:|---:|---:|
| P1 | 10 | 10 | 0 |
| P2 | 24 | 22 | 2 |
| P3 | 23 | 14 | 9 |
| 合计 | 57 | 46 | 11 |

## P1 — 优先修复

### A01 · B10 结果不一致时可能有人卡在结算页 — P1

- **问题与影响：** 某端发现逐玩家结果或下一关身份不一致时退出；若其最后一条退出消息首次发送丢失，另一端可能仍等待结果确认，不能可靠地共同退出。
- **依据：** [结果包处理](../../src/app/lan_service/cooperative.rs#L621)排队退出后立即返回大厅；[传输释放](../../src/app/lan_service/cooperative.rs#L46)只服务一次，就重置可靠连接和作用域。
- **修正：** 使用现有可靠传输所有者，在退出或拒绝消息得到确认之前保留重传；对超时和对端失联提供明确的本地失败出口。不要增加另一套竞争性的轮次状态。
- **验收：** 丢弃首次退出/拒绝消息和首次 ACK，分别覆盖结果不一致、关卡不一致、退出及对端不响应；确认每端能退出或得到明确错误，而不是无限等待。

### A11 · B64 客人可以发送房主专属的轮次控制消息 — P1

- **问题与影响：** 已加入的客人若发送 `Go` 或 `Begin` 消息，接收端没有确认消息来自房主；在结果握手已完成时，客人可能绕过房主继续或重试的控制，使不同客户端进入不同的后续状态。这里的发送者是已加入的参与者，不假定任意外部地址能够冒充。
- **依据：** [轮次接收](../../src/app/lan_service/cooperative.rs#L563)只检查发送端是在线参与者，就将 `Go`/`Begin` 转成不带来源的事件；[事件处理](../../src/app/match_runtime/session/cooperative_round.rs#L1248)没有核对房主身份，`Go` 只额外检查结果一致性。[Ack 处理](../../src/app/match_runtime/session/cooperative_round.rs#L1268)也未检查本地是否为房主、是否已发起本轮分配或结果是否一致；累计的不同确认达到门槛时就能推进并排队 Go，具体是否达到门槛取决于各来源和状态。
- **修正：** 保留并校验消息来源：只有认证后的本轮房主能发出 `Begin`、轮次选项和 `Go`，仅房主在正确的已分配轮次中、结果一致且确认来自相应客人时计入 Ack；将控制消息绑定当前轮次身份并丢弃旧包。保留正常的多人准备、投票与重传流程。
- **验收：** 三人房间中由客人向房主和其他客人分别发送伪造的 `Begin`/`Go`、提前 Ack 和重复 Ack，确认均不能绕过闸门；覆盖客人收到 Ack、正常房主继续、旧轮次重放以及多个不同来源达到计数门槛的情况。

### A12 · B65 Begin 丢包或乱序可能让结算流程卡住 — P1

- **问题与影响：** 房主发送 `Begin` 和轮次选项时，若 `Begin` 首包丢失而选项先到达，或两个包乱序，客人可能不发送轮次确认，房主一直等待，无法继续或重试。
- **依据：** [房主分配](../../src/app/match_runtime/session/cooperative_round.rs#L1016)依次排队 `Begin` 和选项；[全局传输入队](../../src/net/global.rs#L373)按到达顺序接收并分别确认，不保证顺序交付。[事件处理](../../src/app/match_runtime/session/cooperative_round.rs#L1248)仅在先前已收到 `Begin` 时处理选项并发送确认；随后到达的 `Begin` 不会补发确认。
- **修正：** 在现有轮次所有者中分别记录本轮 `Begin` 与已验证选项已到达；两者齐备时幂等发送一次确认，不依赖 UDP 到达顺序，并与 A11 的来源及轮次校验一起处理。
- **验收：** 覆盖 `Options → Begin`、`Begin → Options`、首个 `Begin` 丢失、重复包和三四人不同到达顺序；确认不重复推进，正常情况下每端可继续或重试。

### A22 · B74 单人房主被通用最低人数检查阻止 — P1

- **问题与影响：** 单名房主进入 VMCE 房间、没有大厅 AI 时点击开始，会被当作只有一方而拒绝；与当前界面的单人开始提示及单角色关卡支持相矛盾。
- **依据：** [开始处理](../../src/app/shell_network_setup/actions/start.rs#L41)无条件调用 `has_minimum_players`；[检查函数](../../src/app/shell_network_setup/actions/start.rs#L365)要求至少两个不同 House 或实际 AI 对手，而 VMCE 大厅不提供新的 AI 项。
- **修正：** 在共享的开局准入所有者明确区分 VMCE 战役的一名人类角色与普通遭遇战的对手数要求；继续校验地图角色、全系列文件和无观察者约束。不要仅修改提示绕过真实开局路径。
- **验收：** 通过真实 UI 测试单人房主在一至四角色地图开局并完成/继续；双人房间正常；普通非 VMCE 房间的单人无对手仍按既有规则拒绝。

### A24 · B76 重试加载失败消息缺少来源和加载尝试校验 — P1

- **问题与影响：** 在 LAN 的 Prepared 网络加载阶段，接收端若收到格式正确的 VMCE 重试失败消息，就可能中止当前加载并返回大厅；接收代码没有确认发送者属于当前任务，也没有确认当前加载是对应的 VMCE 重试。普通 LAN 加载也可到达此阶段。传输 Active 阶段对未经封装的非进度包还有额外过滤，不能笼统推断所有加载阶段都能注入；实际发包和旧包到达条件仍需实测。
- **依据：** [消息接收](../../src/app/lan_service.rs#L1919)只检查 LAN 会话后调用处理函数；[失败处理](../../src/app/loading/pump.rs#L3429)对活动网络加载执行失败退出；[报文解码](../../src/net/lan_messages/cooperative.rs#L366)不携带加载尝试身份。[准备阶段传输](../../src/net/udp.rs#L2005)接纳全局消息，作用域[不是认证](../../src/net/udp.rs#L1172)。
- **修正：** 复用既有会话/加载所有者，核实发送者属于本次已接纳的在线参战者，并将失败通知绑定活动的 VMCE 重试加载尝试；忽略不匹配的来源、阶段和过期通知，保留合法失败时全员退出的行为。
- **验收：** 分别向 VMCE 重试加载发送合法失败、非参战端来源及旧尝试通知，确认仅合法通知使各端退出；覆盖普通 LAN 加载和传输阶段边界。

### A25 · B77 加载失败通知可能在送达前被清除 — P1

- **问题与影响：** 一端在 VMCE 首次或重试网络加载期间本地失败，若失败通知进入可靠队列后立即返回 LAN 大厅并重置队列，后台 I/O 可能尚未首次发送，也可能虽发送但尚未得到确认。其他玩家因收不到通知可能留在加载或等待界面。部分构建失败因 A31 连通知都未入队；此处仅描述已入队后的竞态，有别于 A01 中结算退出消息的送达问题。
- **依据：** [失败路径](../../src/app/loading/pump.rs#L3458)排队通知后返回主菜单；[返回 LAN 房间](../../src/app/shell_network_setup.rs#L1539)调用 [restart_visit](../../src/app/lan_service.rs#L1162)，而[传输重置](../../src/net/udp.rs#L1487)清除待发送记录和全局队列。发送与后台 I/O 并发，故不能断言每次都会在首发前清除。
- **修正：** 在现有传输/失败退出所有者中引入有界的通知排空与确认阶段；全部目标确认后或发生明确超时再清理，超时要提供可理解的退出结果。与 A24 的来源、尝试校验共同设计。
- **验收：** 经生产加载路径注入一端失败，验证其他端收到通知并退出；覆盖首包或 ACK 丢失、对端无响应、队列满及多目标部分确认。

### A27 · B79 原版战役 AllowWin 胜利门尚未用于 VMCE — P1

- **问题与影响：** 地图依赖带动作 15 的 AllowWin 标签延迟某所属方的胜利时，VMCE 作为网络模式会绕过该所属方的标签计数门；倒计时到期后可能比原版单人战役更早进入共同结算。这是指南已公开的当前限制；扫描的 50 张零售战役/触发地图均未写动作 15，因此该差异对零售战役没有可复现的玩家可见影响，只有自定义 VMCE 系列地图写入该标签时才可能出现，仍需明确产品语义。
- **依据：** 已在零售 `gamemd.exe` 上做指令级核对：`HouseClass::Update`（0x004F8440，0x004F8448 `xor ebp,ebp`）在 0x004F85F7 比较全局 `GameMode`（0x00A8B238）与 0，非零跳 0x004F860B 绕过标签门；只有 `GameMode==0` 时 0x004F85FF 才以 `cmp dword ptr [esi+0x27C], ebp` / `jg 0x004F86FE` 跳过胜利路径。AllowWin 标签析构 0x006E50A0 递减 House+0x27C，0x006E50B1 起把 House+0x298 计时器设为 60 帧（0x3C）；计数清零、登记与析构入口见 [House 字段注释](../../src/sim/house_state.rs#L577)。[House 更新](../../src/sim/world/house_defeat.rs#L257)把非零网络模式传给 [pending_outcome](../../src/sim/house_state.rs#L2920)，后者在该模式下不要求 `allow_win_tags <= 0`。当前实现与原版网络模式分支一致；网络模式即 LAN/遭遇战，VMCE 也走该分支。
- **修正：** 原版分支已确认后，先在现有 House 结果所有者中明确 VMCE 的产品选择：保持网络模式的最先到达各自可接受原版结算时点即共同结算，或显式采用战役门（`GameMode==0` 的 `+0x27C<=0` 条件）并同步计时器语义；不得让普通 LAN/遭遇战跟随改变。零售战役没有使用该标签，因此需要构造使用动作 15 的测试地图确认玩家可见差异。
- **验收：** 构造使用动作 15 的测试地图，分别验证原版单人战役、原版网络模式与 VMCE 在 AllowWin 标签移除前后的胜利时点、60 帧计时器与同帧顺序；覆盖敌对玩家失败先到期、多人一致性及未使用标签的普通胜利。

### A28 · B80 任务退出框仍可能提供 Observe — P1

- **问题与影响：** 多名玩家的主 House 各不相同时，任务内退出确认框可能出现 Observe，违背 VMCE 不需要观察者的要求。点击后不是正常离房：它排队本方 `DESTRUCT` 并留在任务中；而 VMCE 抑制通常的空 House 败北处理，因此还可能让菜单闸门不能按普通多人路径重新打开。具体表现尚需运行验证。
- **依据：** [确认框选择](../../src/app/input/abort.rs#L28)按通用网络模式与存活 House 数提供 Observe，仅对共享同一主 House 的队友隐藏；[点击处理](../../src/app/in_game.rs#L533)排队自毁而不发送退出；[任务 House 处理](../../src/sim/world/house_defeat.rs#L245)对 VMCE 禁用普通空 House 败北门。[菜单准入](../../src/app/in_game.rs#L498)依赖本地败北标志重开清除的闸门。
- **修正：** 在退出确认框及对应执行入口的既有模式/动作所有者中明确排除 VMCE 的 Observe，保留普通 LAN 的原有行为；VMCE 使用正常离房，不能将玩家变为观察者或通过此入口意外自毁。
- **验收：** 真实二至四人 VMCE 中打开退出框，覆盖主 House 存活、消灭及共享授权场景；确认不出现 Observe、离房路径正确。普通 LAN 仍保持原有选项。

### A31 · B83 本地构建失败可能完全漏发失败通知 — P1

- **问题与影响：** VMCE 网络加载构建阶段发生错误时，加载会话已从前端状态取走；随后失败处理据此无法识别正在加载 VMCE，因此可能完全不向其他玩家发送失败通知，其他端只能等待超时或停留在后续阶段。
- **依据：** [加载泵](../../src/app/loading/pump.rs#L1692)取走 `loading_session`；[构建失败](../../src/app/loading/pump.rs#L2088)返回失败但没有放回会话；[失败识别](../../src/app/loading/pump.rs#L3371)依赖状态中仍有活动加载会话。与 A25 的入队后重置是先后不同的两个缺口。
- **修正：** 在既有加载/失败所有者中让冻结的 VMCE 身份和加载尝试信息随失败结果存活，确保构建失败能通知各端，并结合 A24、A25 的身份校验和可靠送达。
- **验收：** 通过生产路径分别注入准备、构建和渲染准备阶段失败，检查首发、丢包重发、对端退出与本地错误提示；再测试失败时会话仍存在的路径。

### A32 · B84 最终网络安装失败不走全员失败流程 — P1

- **问题与影响：** 各端完成加载进度后，若一端最终安装网络连接或 House 绑定失败，本地仅显示启动失败卡片，不通过加载失败路径通知其他玩家；其他端可能已进入安装或游戏阶段，最终表现需要实测。
- **依据：** [安装调用](../../src/app/loading/transitions.rs#L783)可由 [install_gameplay](../../src/app/lan_service.rs#L3423)返回错误；[错误分支](../../src/app/loading/transitions.rs#L832)只清理 startup 并显示 `StartupRejected`，没有失败通知。[安装交接](../../src/app/loading/pump.rs#L3317)之前加载会话已消费。
- **修正：** 把最终安装失败纳入现有网络加载失败及可靠退出流程，保留已消费会话中的冻结任务/尝试身份；同时避免覆盖安装错误和破坏其他网络模式。
- **验收：** 各端到达最终安装时，分别在房主和客人侧注入连接/绑定失败；验证本地错误、对端收到一致失败并可退出，覆盖延迟、丢包和正常成功安装。
## P2 — 随后修复

### A02 · B57 已确认的胜利仍可能不解锁下一关 — P2

- **问题与影响：** 结果与下一关身份已一致，但有人停在结果页或离开，参与者本机不记录已获得的下一关；用户之后新建房间时无法选它。
- **依据：** [结算退出分支](../../src/app/cooperative_round.rs#L786)只有在 `Exit(true)`、准备切换到关间房间时才调用解锁；[结果一致性握手](../../src/app/match_runtime/session/cooperative_round.rs#L494)本身没有写入解锁。
- **修正：** 把幂等的本地解锁触发移到本轮结果及下一关身份均得到在线成员确认的时点；不要依赖点击继续、进入房间或成功加载。保留单人房主和已离线成员的明确语义，不允许不一致或旧轮次报文解锁。
- **验收：** 双人确认后直接离开结果页仍可从新建房间选择下一关；结果/身份冲突不写入；重复确认只写一次；单人房主也能解锁。

### A03 · B58 Ready 后修改其他地图可绕过 Go 前复验 — P2

- **问题与影响：** 玩家 Ready 后更改系列中尚未选中的地图，随后开始任务；已验证的清单和当前地图仍相同，开局可能继续使用旧的整套系列结果，不能保证双方文件一致。
- **依据：** [边界校验](../../src/app/lan_service/vmce_series.rs#L180)通过 `revalidated` 复用一次扫描结果；[发送路径](../../src/app/lan_service.rs#L4082)的 Ready、Go 和交接调用同一状态，未分别消费新的复验凭据。进入下一关房间会取消旧校验，本问题不依赖跨关缓存。
- **修正：** 为每个实际 Ready 或 Go 交接分配带身份和代次的一次性复验凭据；异步扫描期间保留心跳，并让同一请求的 deferred 重试复用正在进行的扫描。文件身份变化和离房时失效，失败时撤销准备并显示原因。
- **验收：** 初次校验和 Ready 成功后，仅改动未选中地图；Go 必须拒绝，恢复一致后可重新验证并开局。分别覆盖房主、客人、重复请求和旧扫描结果晚到。

### A04 · B59 相同国家的不同角色仍可开局 — P2

- **问题与影响：** 两个不同主 House 使用同一 `Country` 时，当前角色校验仍可通过。按国家索引的胜利触发器会解析首个匹配 House，地图不能按约定明确区分玩家。即使玩家之间国家唯一，若先登记的 AI House 与玩家同国却互为敌人，[胜利判定](../../src/sim/world/house_defeat.rs#L933)也可能以该 AI 为胜方而把同国玩家判负。
- **依据：** [角色绑定](../../src/app/match_bootstrap.rs#L426)只检查 House 存在及角色不重复；[国家查找](../../src/sim/world/mod.rs#L6330)按登记顺序返回首个匹配项。普通原版动作按本地玩家国家直接比较，VMCE 又按首个 House 的联盟扩展胜方。大厅虽有国家冲突提示，但展示逻辑不是开局校验的权威所有者。
- **修正：** 在共享的角色解析/开局校验中，根据生效规则解析各主 House 的国家身份并拒绝玩家角色间重复；提示冲突角色和国家。另明确玩家与 AI 同国时胜方及联盟的歧义，在有证据支持前不要笼统禁止所有 AI 同国，也不能仅检查玩家角色就认为问题解决。保持国家索引和默认值的现有所有者。
- **验收：** 两个不同主 House 同国的地图不能 Ready/开局；不同国家可开局；覆盖大小写、默认国家和随机角色。另覆盖 AI 先于玩家、玩家先于 AI、同国多 AI 及不同联盟关系，核对原版本国胜利与 VMCE 盟友扩展的约定，确保所有端结论一致。

### A06 · B62 解锁记录读取或写入失败对玩家静默 — P2

- **问题与影响：** 本机解锁文件损坏、目录不可写或磁盘写失败时，玩家可以继续，但稍后建房发现下一关没有解锁。读取失败时选关列表会隐藏已解锁的中途关，却没有界面说明。
- **依据：** [解锁调用](../../src/app/shell_network_setup/vmce_map.rs#L357)和[进度读取](../../src/app/shell_network_setup/vmce_map.rs#L390)捕获错误后只写日志；[选关过滤](../../src/app/shell_network_setup/vmce_map.rs#L448)把无法读取的进度视为没有解锁。[持久化所有者](../../src/app/persistence/vmce_progress.rs#L40)不会覆盖损坏记录。
- **修正：** 保持不覆盖损坏数据，将读取或持久化错误返回应用层，并以本地化提示说明已有进度无法读取或下一关未保存，提供可重试路径；避免把本机存储失败误报为联机结果不一致。
- **验收：** 模拟损坏文件及写入失败，确认提示、数据不被覆盖；修复环境并重试后确实能从本机选到下一关。

### A07 · B43 回到房间后随机玩家在角色板中消失 — P2

- **问题与影响：** 切换到下一关房间时所有人重置为随机角色，右侧角色板可能全显示等待，甚至看不到已准备玩家的状态；实际参战者仍在左侧玩家列表。本轮 2p-win/4p-win 真实采集的关间大厅截图显示两名/四名玩家的 Owner 都重置为 `隨機`，右侧角色板对应行均为 `等待 ...`、Ready/Color 空白，支持该现象。
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

- **问题与影响：** VMCE 地图使用 `VMCE/<系列>/<目录>/all01umd.map` 等目录路径时，大厅查找零售 `MISSIONMD.INI` 的任务简报会使用整条路径，无法匹配按文件名命名的节，显示简报不可用。本轮 2p-win/4p-win 关间大厅截图仍显示 `Mission briefing unavailable`。
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
- **依据：** [角色校验](../../src/app/match_bootstrap.rs#L433)没有拒绝被动国家；[结算行](../../src/sim/score.rs#L103)先取得 VMCE 角色，但随后仍过滤 `multiplay_passive`；[结果页](../../src/app/vmce_result.rs#L329)以这些行的数量作为轮次人数，[单人分支](../../src/app/match_runtime/session/cooperative_round.rs#L931)据此直接继续。零售 `RULESMD.INI` 的 Neutral 和 Special 设置了 `MultiplayPassive=true`。
- **修正：** 轮次参与人数应取冻结的参战者/实际连接状态，不能取可被显示过滤影响的统计行；明确 VMCE 是否允许被动国家作为主角色：若允许，按已分配角色展示统计；若不允许，在开局前给出清晰错误，并说明限制依据。
- **验收：** 覆盖单人和双人包含被动主 House 的胜利、失败重试和关间继续；确认玩家行、握手人数与最终结果一致，或在开局前一致拒绝。

### A20 · B72 任务中替换地图可能让结果页失去 VMCE 控件 — P2

- **问题与影响：** 任务开始后本地已加载地图文件被覆盖或替换、但战役清单和下一关未变化时，胜利结果页可能无法构建角色板，退回普通计分页外观，缺少玩家胜负、准备和离开房间等 VMCE 控件。
- **依据：** [结果板](../../src/app/vmce_result.rs#L205)依赖当前角色元数据；[元数据投影](../../src/app/shell_network_setup/view.rs#L118)从磁盘重读已完成关卡并要求与冻结配置的源摘要匹配；[结果页](../../src/app/vmce_result.rs#L401)把可能为空的结果板交给界面。此时已加载的模拟状态和磁盘文件可能不同。
- **修正：** 从开局冻结的角色、所属方、国家和颜色数据建立结算投影，保持运行中任务的数据一致；若需要拒绝文件变化，应明确提示并走可靠的全员退出路径，不应静默退回普通页面。
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
- **依据：** [胜利页入口](../../src/app/vmce_result.rs#L14)在构建结果页之前解析下一关；失败后仅写日志并调用[退出路径](../../src/app/vmce_result.rs#L296)。[下一关解析](../../src/app/shell_network_setup/vmce_map.rs#L279)会检查清单身份和地图文件。
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

### B46 系列校验进行中缺少 Ready 反馈 — P2

- **问题与影响：** 整套系列后台校验未完成时，大厅不显示内联状态，而 Ready 由 `accept_eligible` 门控，客人看到的是一个没有反应的按钮。
- **依据：** [校验状态映射](../../src/app/lan_service/options.rs#L424)没有 Pending 状态；[准备准入](../../src/app/shell_network_setup/actions/start.rs#L365)由 `accept_eligible` 门控。
- **修正：** 为 Pending 增加本地化「正在校验战役文件」状态并解释 Ready 暂不可用；区分失败、取消与完成。
- **验收：** 慢速校验期间中英文都显示状态，Ready 不会静默失效；失败/取消给出可行动原因；完成后按钮与状态及时恢复。
## P3 — 最后修复

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

- **问题与影响：** 地图通过 `[Scenario:<map>:<House>]` 的 `PlayerNControl=yes` 把共享 House 授权给角色。大厅角色板只列主角色的 House、国家、旗帜和颜色，不显示每个角色额外控制的共享 House；玩家在准备和开局前无法确认谁控制哪些共享单位，容易重复操作或无人控制，地图作者也无法在大厅核对授权是否写对。本轮 4p-win 关间大厅截图（`LDA-4p` 系列把 `French House` 的 `Player1Control`/`Player2Control` 授权给角色 1/2）只显示玩家 Owner 与角色板的 Country/Ready/Color，没有任何共享 House 名称或授权提示，支持该问题。
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
- **依据：** [统计页行绘制](../../src/app/frontend/campaign_score_shell_render.rs#L560)直接把 `row.player` 推入固定 `cells`，未调用 `ellipsize_native`；同一渲染器的[结算板](../../src/app/frontend/campaign_score_shell_render.rs#L641)和[失败页](../../src/app/frontend/skirmish_shell_render/coop_score.rs#L251)都对名字调用 `vmce_panel::ellipsize_native`。
- **修正：** 统计页玩家名列复用 `vmce_panel::ellipsize_native` 与同一字体宽度测量；数值列保持右/居中对齐。保持四行、800×600 与缩放布局。
- **验收：** 最长玩家名（含中文/双宽字符）在 2–4 行、各缩放与 800×600 下显示省略号且不覆盖数值列；普通长度名字不变；失败页与结算板行为不回归。

### A40 · B94 结算/关间轮次不发送 BC6 保活，也没有对端静默检测 — P3

- **问题与影响：** 结果/关间轮次打开期间，BC6 原生的 20 秒保活 `0x0D`（`0x0046EADD`）没有发送；同时 LAN 主服务在轮次阶段提前返回，跳过 setup 保活和租约轮询。两端都停在结果页且无人操作时没有周期流量，也不会发现对端静默（挂起、半开连接、丢包停摆）；按真实时间等待时只能无限等，或靠用户手动离开。原版会持续刷新对端存活并在失效时走掉线流程。
- **依据：** [BC6 保活未发送](../../src/app/cooperative_round.rs#L17)（RESIDUAL 注释，原生 0x0046EADD、20 s）；[LAN 服务在轮次阶段提前返回](../../src/app/lan_service.rs#L1778)，跳过 [announce](../../src/app/lan_service.rs#L1856)、[租约轮询](../../src/app/lan_service.rs#L1853) 和 [setup 保活](../../src/app/lan_service.rs#L877)。
- **修正：** 在 VMCE/co-op 轮次打开期间复用现有保活所有者：周期性发送原生 0x0D（或等价保活）并继续运行租约/对端静默检查；检测到静默时提供可行动的提示并走既有的失败/离开路径，不能静默无限等待。
- **验收：** 结果页保持打开超过保活/租约周期，抓包确认两端周期保活、租约不过期；暂停一端进程，另一端在有界时间内提示/退出而不是无限等待；正常 Ready、重试与关间 Go 流程不回归。

### A41 · B95 VMCE 开局与关间继续不播放地图场景电影 — P3

- **问题与影响：** 自定义 VMCE 系列若在地图 `[Basic]` 设置 `Intro`/`Brief`/`Action`/`Win`/`Lose`/`PostScore`/`PreMapSelect` 电影，原生战役开局与 BC6 续关路径会播放对应影片，而 VMCE 的关卡开始、胜利/失败结算和关间继续都直接进入结果页或 `begin_loading`，这些过场会被静默跳过。作者用电影承担叙事或关间转场时玩家看不到；不影响胜负、解锁、同步和校验和。
- **依据：** [关间继续电影无所有权](../../src/app/cooperative_round.rs#L975)（RESIDUAL：Win/PostScore/PreMapSelect/Lose 及胜利后的 Intro/Brief/Action 无 owner）；[原生胜利电影入口](../../src/app/campaign_victory.rs#L49)只对 `CampaignScore`/`CampaignVictoryContinue` 返回 `basic.win`，[VMCE 结果目的地](../../src/app/match_runtime/scenario_exit.rs#L155)是 `CooperativeResult`，不经过该入口；[下一关加载](../../src/app/cooperative_round.rs#L1030)直接调用 `LoadingRequest::network`，没有电影阶段；战役开局的 [Intro/Brief 读取](../../src/app/shell_campaign.rs#L304)也未接入 VMCE 的加载路径。
- **修正：** 复用现有 `fullscreen_movie` 所有权：关卡开始按 `Intro`（缺省 `Brief`）、胜利按 `Win → PostScore → PreMapSelect`、失败按 `Lose` 播放地图设置的电影，再进入现有结果页/继续流程；每端本地播放，保持输入锁和返回语义，不改变胜负判定、解锁时机与随机种子；未设置键时保持现状。
- **验收：** 作者化两关系列分别为各键设置影片与留空；2p/4p 各端在开局、胜利、失败和关间继续的对应时点看到同一影片，并能返回正确页面；未设置时无额外停顿；300 个 checksum、结果一致性与解锁流程不回归。

### A42 · B97 关间房间的 Change Map 被禁用却仍按启用态绘制 — P3

- **问题与影响：** 进入关间房间后 `vmce_intermission_map` 非空，`CHOOSE_MAP` 的启用规则与点击处理都把它禁用；但 setup 右栏按钮绘制把 choose_map 的 disabled 参数写死为 `false`，按钮继续显示启用态的红色底与亮黄文字，点击没有任何反应或说明，玩家会以为按钮坏了。客人（非房主）也走同一条绘制路径。本轮 4p-win 关间大厅截图里 Change Map 与 Start Mission 的按钮外观一致（同为启用态），支持该问题。
- **依据：** [关间地图锁](../../src/app/shell_network_setup/view.rs#L1034)（`session.vmce_intermission_map().is_some()`）；[启用规则](../../src/ui/multiplayer_setup.rs#L545)（`CHOOSE_MAP => self.host && !self.map_locked`）；[绘制硬编码](../../src/app/frontend/skirmish_shell_render.rs#L396)（choose_map 按钮的 disabled 恒为 `false`）；[点击守卫](../../src/app/shell_network_setup/actions.rs#L66)（关间地图存在时直接返回）。绘制层已有[禁用 alpha](../../src/app/frontend/skirmish_shell_render.rs#L103)，只是没有接到这个按钮。
- **修正：** 绘制使用与命中测试同一 `view.enabled(CHOOSE_MAP)`（或把视图的禁用集合下发给 setup 绘制），让关间房间与客人侧显示原生禁用态；状态栏可补充「关间房间固定下一关」。普通房主房间保持可点击。
- **验收：** 关间房间和客人侧的 Change Map 以禁用态绘制、点击无动作（最好有说明）；普通房主的 Change Map 仍能打开选图；800×600 与缩放截图不回归；开始/准备流程不变。

### A43 · B98 campaign.ini 的未知键与超量 ScenarioN 被静默忽略 — P3

- **问题与影响：** `[Campaign]` 只读取已知键，未知键被静默忽略；`ScenarioN` 只读取 `Scenario1..=ScenarioCount`，超出数量的显式行既不报错也不生效（对比 `[Scenario:...]` 会显式拒绝超出 `PlayerCount` 的 `PlayerN`）。作者把 `ScenarioCount` 写少、残留旧 `ScenarioN` 或把键名拼错时，准入通过但战役在本应存在的关卡处提前结束，该行被无声丢弃；若 `NextScenario` 指向被丢弃的关卡才会报“目标不在清单”，否则玩家只看到少关卡。本轮用含 `Scenario2`（超出 `ScenarioCount=1`）与未知 `Foo=stray` 的 `campaign.ini` 跑 `2p-final`，准入与两端采集均 PASS（300 个 checksum 一致），证实这些键确实被静默忽略。
- **依据：** [Campaign 清单读取](../../src/app/shell_network_setup/vmce_map.rs#L722)按 `ScenarioCount` 循环读 `Scenario1..=count`，没有检查额外 `ScenarioN` 或未知键；[场景段白名单](../../src/app/shell_network_setup/vmce_map.rs#L592)对 `PlayerN` 超出 `PlayerCount` 会显式拒绝，两处校验力度不一致；[控制段白名单](../../src/app/shell_network_setup/vmce_map.rs#L624)同样逐键校验。
- **修正：** 在 `parse_vmce_manifest` 中增加 `[Campaign]` 键白名单（`Name`/`Entry`/`ScenarioCount`/`Scenario1..=ScenarioCount`），并拒绝 `ScenarioN` 中 N 超过 `ScenarioCount` 的显式键；错误包含段名与键名。保留 `ScenarioCount` 上限和 `Entry` 必须属于清单的现有校验。
- **验收：** `ScenarioCount=1` 同时写 `Scenario1`/`Scenario2`、未知键（如拼错的 `Scenariocount` 或额外 `Foo=`）都在准入被拒并指出段/键；合法系列行为不变；B05 的逐关预校验与整套字节身份不受影响。

### A44 · B99 关间房间的客人看不到下一关预览，也收不到简报提示 — P3

- **问题与影响：** 结果页 Continue 后进入关间房间时，每个 peer 先由 `enter_vmce_intermission_setup` 把下一关的 `map_metadata`/`selected_map_file` 写进会话，再用新 protocol 打开 setup。客人收到房主 options 时，`network_map_requires_verification` 比较本地已保留的下一关 metadata 与收到的相同，于是 `map_changed=false`；而 `local_map_verified` 只在 `map_changed` 分支里更新，客人的 `local_preview()` 因此一直是 false，`local_map_index()` 返回 None。渲染层随后走“收到房主预览图”的路径，但 VMCE LAN 从不请求/发送预览，客人的预览框为空；同一 `local_map_index()` 也驱动“该地图有简报”的聊天提示，所以客人也收不到该提示。房主因 `session.host()` 短路仍看到预览。另一个独立成因：第一次在关间中途加入的客人走 `open_lan_joined_setup`，它不调用 `refresh_vmce_entries`，而进程启动时场景目录只扫描零售根目录，VMCE 系列记录可能根本不存在，所以即使地图验证通过，`local_map_index()` 也找不到记录。
- **依据：** [关间房间打开](../../src/app/shell_network_setup.rs#L1110)先 `enter_vmce_intermission_setup`（[写入手选地图/元数据](../../src/app/match_runtime/session/cooperative_round.rs#L572)）再新建 setup；[地图变化判定](../../src/app/match_runtime/session.rs#L1646)在 metadata/地图/mode 相同时返回 false；[只在 map_changed 分支更新](../../src/app/shell_network_setup/service.rs#L707) `local_map_verified`；[新 protocol 初始为 false](../../src/app/shell_network_setup.rs#L428) 与 [local_preview 判定](../../src/app/shell_network_setup.rs#L554)；[local_map_index 的非房主门槛](../../src/app/shell_network_setup/view.rs#L460)；[渲染选择预览的路径](../../src/app/frontend/skirmish_shell_render/multiplayer_setup.rs#L110)；[新加入者路径不刷新目录](../../src/app/shell_network_setup.rs#L871) 与 [刷新所有者](../../src/app/shell_network_setup/vmce_map.rs#L376)；[简报提示消费 local_map_index](../../src/app/shell_network_setup/service.rs#L2192)；[已有“接受已验证地图源”的 protocol 方法](../../src/app/shell_network_setup.rs#L566)目前只在测试里调用，生产路径没有把它接到 VMCE 客人。
- **修正：** 在 `open_vmce_intermission` 建好新 protocol 后，用会话里已保留的本地已验证地图源（`network_local_map_source()` 与 metadata 摘要）播种 `local_map_verified`/`local_map_loose`，或让 `local_map_index`/`local_preview` 直接消费该已验证来源，而不是依赖一次 `map_changed` 解析（现有 `accept_verified_map_source` 就是为此准备但未接线）；同时把“确保关间下一关在场景目录中”放到任何 peer 应用带 `vmce_intermission` 的 options 的共同路径，不要只在 `open_vmce_intermission` 里做。普通 LAN 与初始房间行为不变。
- **验收：** 2/3/4 人关间房间里，房间打开时已在场的每个客人，以及房间打开后才加入、此前从未进过 VMCE 关间房间的客户端，都看到与房主相同的下一关预览和“该地图有简报”提示；初始 VMCE 房间与普通 LAN 客人不回归；房主预览不变。基于生产调用链静态核对，尚无客人侧关间大厅截图。

### A45 · B100 满员的关间房间在 LAN 浏览器里仍显示可加入 — P3

- **问题与影响：** 关间房间的人数上限按下一关主角色数 clamp(2,4)，但房主回复 LAN Discovery 时固定发送 `advertisement(name, true)`，`open` 位恒为 1。2/2、4/4 已满（或 GO 已锁定）的关间房间仍以可加入状态出现在 LAN 游戏列表；新加入者点击 Join 后被 `join_denial` 以 reason 1 拒绝并弹出 `TXT_GAME_FULL`。这是通用 VERA 房间的状态问题，但关间房间的小上限让 2/2、4/4 成为常见路径。
- **依据：** [Discovery 固定 open=true](../../src/app/lan_service.rs#L2429)；[advertisement 的 open 位](../../src/net/lan_messages.rs#L362)；[人数上限](../../src/app/match_runtime/session.rs#L2113) 已有 `network_room_full()`；[join 拒绝的满员分支](../../src/app/lan_service.rs#L3058) 与 [拒绝原因映射](../../src/app/shell_lan.rs#L337)（reason 1 = `TXT_GAME_FULL`）；[LAN 浏览器只拒绝关闭的房间](../../src/app/shell_lan.rs#L328)；[关间房间上限设置](../../src/app/shell_network_setup.rs#L1156)。
- **修正：** Discovery 回复用 `!session.network_room_full()`（必要时再叠加 `protocol.editable()` 以处理 GO 锁定）作为 `open` 位；房主对每次 Discovery 都重新回答，成员/上限变化后无需额外状态刷新。
- **验收：** 2/2、4/4 的关间房间在 LAN 列表中不可加入（或以关闭态显示）；有人离开后恢复可加入；普通 8 人 LAN 房间的 open 语义与 GO 锁定拒绝不回归。

### A46 · B101 主角色 House 名直到开局才校验，准入可放过错误战役 — P3

- **问题与影响：** `load_vmce_scenario` 读取 `Player1..N` 时只要求非空并写入合成 `[Basic]`，不把 House 名与地图 `[Houses]` 对照；联机准入和 Ready/Go 都会通过。真正的校验在开局 `resolve_vmce_houses` 的 `validate_against_roster`，且按字符串精确匹配（大小写不同即视为不同 House）；失败后只显示“角色配置无效/可能人数超过角色或角色缺失、重复”的笼统提示。结果是：作者把 `Player2=Americans House` 误写为不同大小写或写错 House，系列看起来通过准入、客人也已 Ready，房主点开始时才失败，玩家不知道为什么不能开始。同一文件的控制段成员检查却使用大小写不敏感比较，校验策略不一致。
- **依据：** [准入读取 PlayerN](../../src/app/shell_network_setup/vmce_map.rs#L648)只检查存在性并写入合成段；[角色 House 去重](../../src/app/shell_network_setup/vmce_map.rs#L651)也是精确字符串；[控制段成员检查](../../src/app/shell_network_setup/vmce_map.rs#L615)已经用大小写不敏感比较；[开局校验调用](../../src/app/match_bootstrap.rs#L437)在 Start 时才 `validate_against_roster`；[名字精确匹配](../../src/map/basic.rs#L45)；[开局失败提示](../../src/app/shell_network_setup/actions/start.rs#L482)只给笼统原因。
- **修正：** 在 `load_vmce_scenario`（以及 B05 的逐关预校验）中按地图 `[Houses]` 解析主角色 House，并用与开局 `validate_against_roster` 相同的匹配策略（若开局改为大小写不敏感，两处一起改）在准入时拒绝未知/重复 House；错误包含关卡、PlayerN 和 House 名。确保“准入通过”的系列不会在 Start 再因 House 名失败。
- **验收：** `PlayerN` 指向不存在 House、大小写不同、重复 House 的 `campaign.ini` 在准入/Ready 前被拒并指出关卡与玩家；合法系列与初始 VMCE 房间不回归；控制段与主角色使用一致的 House 匹配策略；Start 不再出现该迟到错误。

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

- **问题与影响：** 发现阶段的过滤让部分无效战役提示难以触达；设置/冻结失败可能错误归因到本地席位，且存在按翻译后字符串判断错误类型。
- **依据：** [大厅错误状态](../../src/app/shell_network_setup/view.rs)、[开局准入](../../src/app/shell_network_setup/actions/start.rs)。
- **修正：** 用结构化错误身份代替翻译字符串匹配，让设置/冻结失败指出真正原因和行动。
- **验收：** 无效战役、地图缺失、系列不一致、冻结失败各自显示明确原因；不把其他玩家误记为本地席位。

### B91 合成结算/失败页采集 fixture 无法启动 — P3

- **问题与影响：** `vmce-result-*` 与 `vmce-defeat-*` 采集检查点以 `VMCE defeat fixture needs authored role metadata` 失败：大厅 fixture 把角色详情缓存在 `game_config.paths.ra2_dir` 下，而 `vmce_map_details` 按 `settings_dir/Maps` 查找。
- **依据：** [大厅 fixture 缓存根](../../src/app/shell_network_setup.rs#L1368)与[地图详情查询](../../src/app/shell_network_setup/view.rs#L135)不一致。
- **修正：** 统一 fixture 与投影的缓存根/文件身份后重新采集，用于关闭 A37/A38 的视觉部分。
- **验收：** 两个合成检查点都能生成截图并区分投票/长名字状态，不再报角色元数据缺失。

### B93 采集运行器零售标记位置不一致 — P3

- **问题与影响：** `vmce_lan_acceptance run` 只把 `.vera20k-lan-capture-root` 写进每个进程的 settings，而采集路径按 `assets.ra2_dir()` 检查，普通 `--retail-source` 会失败。
- **依据：** [隔离根检查](../../src/app/diagnostics/shell_capture/movies/lan.rs#L1163)与运行器 `write_process_settings`。
- **修正：** 把标记写入每个零售副本，或要求并校验传入源已标记，并写入文档。
- **验收：** 用普通 retail 目录和标记副本运行场景都得到一致 PASS；错误提示说明原因。

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
- 本机真实双进程：`2p-win`、`2p-retry`（作者化失败系列）均通过；300 个 native checksums 与 20 个 simulation hashes 两端一致；失败页无逐玩家投票状态，关间大厅随机 Owner/等待角色板/简报不可用，与 A07/A13/A37 对应。
- 本机真实四进程：`4p-win` 通过；4 个 receipt 的 300 checksums 与 20 个 hashes 一致；胜利板四行，四人关间大厅无共享 House 授权显示，与 A34 及 B30–B32 对应；关间大厅截图中 Change Map 与 Start Mission 外观一致（启用态），与 A42 对应。
- 本机真实双进程（末关/完成流程）：`2p-final` 用 `LDA-2p-final`（Entry=末关，`--entry stage02/all01umd.map`）通过；`lan.vera_outcome.kind="end"`、`completed=true`、`screen=MainMenu`、`network_return=None`，两端 300 checksums 与 20 个 hashes 一致，截图含 `vera-result-real-win`、`vera-campaign-credits`、`vera-campaign-end`（`vera-campaign-end` 实际截到返回后的主菜单，不是完成对话框），支持片尾/返回主菜单流程。VMCE 末关完成走 `vmce_campaign_end` 的片尾/主菜单路径，不打开 0xBC7 完成对话框；`build_campaign_end` 属于旧 Cooperative 路径（最多两人），因此不构成 VMCE 完成页问题。
- 清单校验采集：用 `ScenarioCount=1` 但同时含 `Scenario2` 和未知 `Foo=stray` 的 `campaign.ini` 跑 `2p-final`，两端 PASS（`end/complete`、300 checksum 一致），证明 `[Campaign]` 的额外键/超量 `ScenarioN` 目前被静默忽略（A43）。
- 说明：上述采集使用作者化测试系列和本机双/四进程，不代表跨机器或零售战役数据验收。

## 已核对但不构成问题

以下路径按当前生产调用链核对，未发现需要修复的玩家可见问题；列出以避免重复追查（新增证据推翻时再改回缺陷）：

- **关间新人的 Ready/解锁提前：** 新人 admission 初始 `ready=false`、`map_status=0`；系列扫描 Pending 时暂扣本地 map status，验收只从 status 1 解锁；进入房间时已清空上一轮的 received options。未发现可提前 Ready 或提前写解锁的入口。
- **新人角色板空角色：** `apply_vmce_roles` 在已绑定行数与角色数不一致时整体推迟，roster revision 会重放 options；该窗口在同一 service pass 内，不会显示空角色（随机角色不可见另见 A07）。
- **离房与 HostLeft：** 离开走可靠 Departure，房主收到 `ParticipantLeft` 后重发 options/roster；房主离开走 HostLeft；`leave_setup` 清理 `vmce_intermission_map`，下一房间不会被上一关锁图。
- **房主缺新人也能开始：** `request_start` 要求其他每个 human 的 map status 非 0 且 `all_ready`；新人 admission 两值初始都不满足；进入 GO 锁定后 `join_denial` 也拒绝后到者。
- **GO/下一关加载身份：** 客人 Ready 的系列边界会先启动 revalidation，GO 后的 handoff boundary 取其已验证结果；A03/B58 的边界复用仍是已知弱点，未发现额外缺口。
- **seed 与随机角色：** 下一关 seed 由上一关 seed 与双方已同意的 stage identity 派生；随机角色分配按 `raw_slot_index` 排序，与 humans 顺序无关。
- **结果页/计分美术泄漏到关间或加载：** `advance_vmce_intermission` 会清 campaign score page/art；失败页只进重试，不进入关间，故不会把上一页残留带进房间或加载。

