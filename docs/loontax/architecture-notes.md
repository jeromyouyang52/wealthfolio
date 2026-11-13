# LoonTax 架构笔记与第 1～3 周计划

基于 Wealthfolio 主干 2026-11-11 的代码（v3.13.0）。只读了代码和文档，**还没有编译和运行过**。

## 0. 先更正提示词里的一处错误

提示词第 2 节写的是"lot 系统目前是 FIFO，要新增 ACB 计算路径"。读完引擎后发现这不准确：

- 上游引擎**已经支持按账户设置成本法**：FIFO（默认）、LIFO、HIFO、WAC（移动加权平均）。设置存在账户的
  `meta.accounting.costBasisMethod` 里。
- **ROC 和 Phantom Distribution 也已经支持**：`DIVIDEND` 的子类型
  `RETURN_OF_CAPITAL` 和
  `NOTIONAL_DISTRIBUTION`，ACB 减到负数时多出部分记为已实现收益。上游还带了四个加拿大测试夹具（NOM-ACB-01 到 04，取自 CRA 的例子）。
- 上游文档 `docs/architecture/portfolio-engine-rules.md` R7.3
  **明确写了不支持**的两件事：
  1. 跨账户合并成本池（"Pooling lots across accounts is not supported"）
  2. 需要"往后看"的规则，原文点名了加拿大的 Superficial Loss

结论：单账户的平均成本上游已经做完了。我们真正要补的是**跨账户 ACB 池、Superficial
Loss、按当日汇率折算 CAD、报税报告、额度规则**。工作量比原计划小，差异化也更清楚。

## 1. 五个问题的答案

### 1.1 交易从导入到持仓的数据流

CSV 或券商同步 → `crates/core/src/activities/`（`csv_parser.rs`、导入模板、去重
`idempotency.rs`）→ 存入 SQLite 的 activities 表 →
`crates/core/src/portfolio/coordinator/`
把数据库里的账户、资产、交易、报价、汇率整理成 `RawFacts` →
`crates/portfolio-engine/` 纯计算（compile → resolve → project →
measure）→ 结果写回 `lots`、`lot_disposals`、快照表 → 前端通过 `@/adapters`
读取。

引擎是**按账户、只往前算**的：一笔卖出的成本只取决于当时这个账户里的 lot。

### 1.2 成本在哪算，最小改动点在哪

- 成本法定义：`crates/portfolio-engine/src/model/policy.rs`（`CostBasisMethod`）；WAC 的池化逻辑在引擎的 project/measure 阶段。
- 已实现盈亏持久化在 `lot_disposals` 表（`crates/core/src/lots/mod.rs` 的
  `LotDisposal`），有按账户和日期范围查询的接口。
- `accounts_model.rs` 里已经有 `PoolingScope::Portfolio`
  这个枚举值，但校验时会直接拒绝（"only ACCOUNT is supported"）。

**建议：不改引擎。** 新建独立的 `crates/canada-tax`，做一个纯函数的"税务账本"：

- 输入：纳税人名下所有非注册账户的交易（直接读 activities）、每日汇率、规则表。
- 输出：每只证券的 CAD ACB 池、每笔处置的利得/亏损、被否认的亏损、年度报告。
- App 里原有的持仓、业绩继续用上游引擎（给加拿大账户默认设成 WAC），税务数字单独算、单独展示。

理由：引擎规则文档要求"改规则先改文档并获批"，且引擎的单账户、只往前的假设贯穿全部测试。跨账户和往后看的规则硬塞进去改动面大，以后每次合并上游都会冲突。独立 crate 几乎不碰上游文件。

代价：同一只证券会有两个成本数字（引擎的单账户账面成本、我们的税务 ACB）。UI 上要讲清楚，税务页只显示税务 ACB。

### 1.3 账户类型

- `account_type`
  是字符串，只有四种：SECURITIES、CASH、CREDIT_CARD、CRYPTOCURRENCY（`accounts_constants.rs`）。
- 上游的立场写在代码注释里："Investment wrappers are not account
  types"。券商同步来的 TFSA、RRSP、FHSA、RESP、LIRA 等一律映射成 SECURITIES，原始类型存进
  `meta.broker_account_type`（`crates/connect/src/broker/models.rs`）。

**建议：不加新的 account_type。** 在账户 `meta` 里加一个我们自己的键，例如
`meta.canada = { "registration": "TFSA", "holder": "self" }`，非注册账户为
`"NON_REGISTERED"`。券商同步的账户可以从 `broker_account_type`
自动预填。好处：不用数据库迁移，不用改 `account_supports_purpose` 和它的测试。

要确认的一点：`accounting_settings()` 对 meta 里不认识的 `accounting`
字段会报错，但其它顶层键（如已有的 `allocation`）是允许的，所以加 `canada`
键应该安全。第 1 周用测试验证。

### 1.4 现有 contribution limits 能否扩展成 TFSA 规则

现有模型（`crates/core/src/limits/`）是通用的：一个额度 = 组名 + 年份 + 金额 + 一组账户 + 起止日期 + 是否每年重复。它统计 DEPOSIT、外部 TRANSFER_IN、外部 CREDIT，能识别内部转账并排除，用户可手动排除某笔。

它**不能**表达：

- 取款在次年 1 月 1 日恢复额度（TFSA）
- 未用额度逐年结转（TFSA 无上限、FHSA 最多 $8,000）
- 终身上限（FHSA $40,000）
- 超额罚税估算

**建议：** 额度规则也放进
`canada-tax`，复用现有的"取某些账户的供款类交易"这层查询（`ContributionActivity`）和它的内部转账识别，自己算可用额度。现有的通用额度页面保留不动。RRSP 按提示词的设计，让用户输入 NOA 上的数字。

### 1.5 汇率

- 接口：`FxServiceTrait::get_exchange_rate_for_date(from, to, date)`
  可以按日期取历史汇率。
- 数据源：默认 Yahoo（`DATA_SOURCE_YAHOO`），也支持手动录入。**没有 Bank of
  Canada 数据源。**
- 每笔交易自己还可以带一个 `fx_rate`（券商实际成交汇率）。

需要你决定用哪个汇率（见第 3 节问题 3）。

## 2. 其它发现

- **导入**：已有"导入模板"机制，内置了 Wealthsimple 的系统模板（`system_wealthsimple`）。Questrade 和 IBKR 没有内置模板。所以第 6 周的工作是验证 Wealthsimple 模板、新增另外两个。
- **AI**：`crates/agent-tools` 有统一的 `AgentTool` trait，已有
  `get_contribution_limits` 工具。加我们的四个工具是照着写。
- **双端**：每个后端调用都要接两条路（Tauri 命令 + Axum 路由），有
  `adapter-command-parity.test.ts` 检查。
- **上游节奏很快**：共 59 个迁移，最近一个是 2026-11-06。上游作者显然了解加拿大规则（夹具用的是 CRA 例子），**存在上游以后自己做跨账户 ACB 的可能**。这是这个方向最大的产品风险，对策是把重心放在上游明确不做的事上：税务合规报告、Superficial
  Loss、额度规则、每年的规则更新。
- **商标**：`TRADEMARKS.md` 禁止 fork 使用名称和 Logo，改名是硬要求。

## 3. 已拍板的问题（2026-11-12）

决定：1 独立 `canada-tax` crate；2 注册类型放 `meta.canada`；3a 新增 Bank of
Canada 汇率，税务计算统一用它；4 暂定名 LoonTax；5 仓库
`jeromyouyang52/wealthfolio`。原问题如下。

1. **ACB 引擎放哪**：独立的 `canada-tax` crate（推荐），还是改上游引擎？
2. **注册账户怎么标记**：放在账户 `meta.canada`（推荐），还是新增账户类型？
3. **汇率来源**：CRA 接受的做法是用交易当日的汇率，常用加拿大银行的日汇率。三个选项：
   - a. 新增 Bank of Canada 数据源，税务计算统一用它（推荐，报告最好解释）
   - b. 优先用交易自带的 `fx_rate`（券商实际汇率），没有才用加拿大银行汇率
   - c. 直接用现有的 Yahoo 汇率（最省事，但和用户自己查到的数字会有小差异）
4. **产品名**：原代号 Maple Ledger。改名涉及包标识和域名，第 1 周就要定。
5. **代码放哪**：需要你在 GitHub 上 fork
   wealthfolio/wealthfolio，并告诉我仓库名，我才能推代码。

## 4. 第 1～3 周实施计划

### 第 1 周：跑通和打地基

- 你：fork 仓库、定名字。
- 编译并跑通桌面端和 Web 端（`pnpm tauri dev`、`pnpm dev:web`），跑一遍现有测试，记录基线。
- 改名：App 名称、图标、bundle
  id、About 页致谢上游。尽量集中在配置文件里改，列出改动清单方便以后合并上游。
- 新建 `crates/canada-tax`
  空壳：规则表格式（按年份、带来源链接的 TOML）、数据模型、测试框架。
- 写测试确认 `meta.canada` 键不影响现有账户读取。

### 第 2 周：ACB 引擎

- `canada-tax` 里实现 CAD 的 ACB 池：买入、卖出、拆股、ROC、Phantom
  Distribution、转入转出。
- 跨账户合并：同一纳税人的所有非注册账户里的同一证券合成一个池。
- 汇率折算（按问题 3 的决定）。
- 测试：提示词的用例 A、E；另外把上游的 NOM-ACB-01 到 04 四个 CRA 例子拿来对数，确认单账户时我们和引擎结果一致。
- 新增一个跨账户用例（两个券商各买一次、在其中一个卖出）。

### 第 3 周：接进 App

- 账户设置里加"注册类型"和"持有人"选择；同步账户从 `broker_account_type` 预填。
- 后端接口：`get_acb(symbol)`、`get_realized_gains(year)`，Tauri 和 Axum 两条路都接上。
- 前端新增
  `features/canada/`：一个税务 ACB 页面，列出每只证券的 ACB 池和参与计算的交易明细。
- 所有税务页面加免责声明。

### 触碰上游文件的清单（越少越好）

- `Cargo.toml`（workspace 加成员）
- `apps/tauri/src/lib.rs`、`apps/server/src/api/`（注册新命令和路由）
- `apps/frontend/src/routes.tsx`、导航菜单（加入口）
- 账户编辑表单（加两个字段）
- 导入页 `upload-step.tsx`（先选账户、后选文件时模板列映射丢失的修复）
- 测试初始化 `src/test/setup.ts`（加载 canada 命名空间）
- 改名相关的配置文件

其余全部是新增文件。

## 5. 供款额度（2026-11-13 实现）

- 计算在
  `crates/canada-tax/src/room.rs`，按《所得税法》207.01(1) 的定义写：TFSA 的
  excess 公式（含"取款中抵掉超额的部分"）、FHSA 的参与额度与结转、RRSP 用 NOA 的
  可用额度和"第 61 天到次年第 60 天"的供款窗口。13 个手算用例，包括 CRA 自己的
  FHSA 例子。
- 没有沿用 1.4 节建议的 `ContributionActivity`：它把内部转账一律排除，而这里需要
  区分"同类计划之间（不占额度）""非注册转入 TFSA（占额度）""RRSP 转入 FHSA
  （占 FHSA 额度）"，还要按持有人归集、要看取款。改为在
  `crates/canada-tax-service/src/room.rs` 用上游的 `TransferPairResolution`
  自己分类。上游的通用额度功能（/limits）不动。
- 用户填的起始数字（TFSA 1 月 1 日额度或合资格年份、RRSP 的 NOA 额度、FHSA 开户
  年份）存在现有的 app settings 表，键 `loontax_holder_profiles`，不加迁移。设备
  同步只同步白名单里的偏好键，这个键不在其中。
- 记不准的情况不硬算，列在"未计入或按假设计入"里：来自其他机构的转账（按直接
  转移处理）、待确认记录、配偶 RRSP 供款（不知道供款人）、FHSA 取款（未建模）、
  缺汇率。
