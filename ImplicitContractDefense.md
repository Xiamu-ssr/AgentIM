# Implicit Contract Defense — 隐式契约防御

> 通用方法论。适用于任何多模块/多人协作/AI 辅助开发的项目。

---

## 一、什么是隐式契约

两段代码假设了同一件事，但没有任何机制保证它们一致。改了一处，另一处静默腐坏——直到运行时才炸，甚至不炸只是结果悄悄变错。

```
// 文件 A：写入方
record["status"] = "running"

// 文件 B：读取方（三个月后另一个人写的）
if record["status"] == "RUNNING":  // 大小写不一致，永远为 false
```

没有编译器、没有测试、没有 lint 能自动捕捉这种错误。这就是隐式契约。

---

## 二、威胁清单

| # | 威胁 | 典型场景 | 防御 | 守关者 |
|---|------|----------|------|--------|
| 1 | 跨语言类型断裂 | 前端 TS 和后端 Python 对同一个 JSON 的字段理解不同 | 全栈同语言，或 schema 生成代码（OpenAPI / protobuf） | 编译器 / 代码生成器 |
| 2 | 非类型世界 | 模板占位符 `{{name}}`、SQL 字段名、正则捕获组 | 隔离仓：强类型进出，脏活封在仓内 | 隔离仓边界 |
| 3 | 外部输入不确定性 | LLM 输出、用户输入、第三方 API 返回 | 入口处立即反序列化到强类型，失败即 Error | serde / validator + Result |
| 4 | 操作原子性 | 写了文件 A 但文件 B 的更新崩了，留下不一致状态 | 单一所有者 + 方法封装 + write-then-rename | 方法边界 / 文件系统 |
| 5 | 动态不变量 | "余额不能为负"、"成员必须属于已存在的实体" | 类型不可表达的用 assert + CI 测试 | CI / 运行时校验 |
| 6 | 并发竞态 | 两个线程同时改同一份数据，AI 看代码时只看到一个线程 | 单线程 event loop + channel 通信，消灭共享可变状态 | 架构约束 |
| 7 | 系统演化 | 需求变了，旧契约散落在代码各处，改不全 | 设计文档先行 → 类型定义跟进 → 编译器传播 | 设计者 + 编译器 |
| 8 | 语义正确性 | 代码编译通过、测试通过，但业务逻辑悄悄算错了 | 测试 + code review + 生产监控 | 现实 |

---

## 三、魔法值与常量

### 3.1 三层处理

**第一层：类型即契约（优先）**

能用 enum / struct / newtype 表达的，绝不用裸字面量。

```rust
// ✅ 编译器保证穷举，加新变体时所有 match 都会报错
enum Status { Active, Paused, Stopped }

// ✅ 路径从方法出，文件名只定义一次
struct ProjectDir(PathBuf);
impl ProjectDir {
    fn config(&self) -> PathBuf { self.0.join("config.json") }
    fn data(&self)   -> PathBuf { self.0.join("data") }
}
```

**第二层：集中常量（一处定义）**

不适合做类型的数值常量，集中在一个文件里。

```rust
// consts.rs — 全项目唯一的常量定义处
pub const DEFAULT_TIMEOUT_MS: u64 = 5000;
pub const MAX_RETRIES: usize = 3;
pub const DEFAULT_PAGE_SIZE: usize = 20;
```

**第三层：零散落（绝对禁止）**

业务逻辑中禁止出现裸字面量。

```rust
// ❌
if status == "active" { }
if retries > 3 { }

// ✅
if status == Status::Active { }
if retries > MAX_RETRIES { }
```

### 3.2 判断标准

| 场景 | 处理方式 |
|------|----------|
| 有穷枚举（状态、类型、模式） | `enum` + serde |
| 文件路径 / 目录结构 | Path struct 的方法 |
| 配置项的默认值 | consts 中定义，`#[serde(default)]` 引用 |
| 协议字段名（JSON key） | struct + `#[serde(rename)]` |
| 只用一次的字面量（日志文本、错误消息） | 允许内联，不算魔法值 |

---

## 四、隔离仓模式

所有与外部世界（文件、网络、用户输入、LLM）交互的边界，用隔离仓包裹：**强类型进，强类型出，中间的脏活封在仓内。**

### 4.1 原则

```
外部世界（字符串/JSON/字节流）
       │
       ▼
  ┌──────────┐
  │  隔离仓   │  反序列化 / 校验 / 清洗
  └──────────┘
       │
       ▼
  强类型的内部世界（编译器可检查）
```

- 仓的入口：`parse()` / `from_str()` / `serde_json::from_value()`
- 仓的出口：`serialize()` / `to_string()` / `Into<Response>`
- 业务逻辑只和强类型打交道，永远不碰原始字符串

### 4.2 典型隔离仓

| 边界 | 仓的实现 |
|------|----------|
| 外部 API 返回的 JSON | 立即 `serde_json::from_str::<T>()` |
| 用户输入的表单数据 | 入口处校验 + 转成领域类型 |
| 文件读写（JSON / JSONL / YAML） | 通用的 `read_json<T>` / `write_json<T>` 函数 |
| 配置文件合并 | 合并在仓内用 `Value` 完成，出仓时转成强类型 Config struct |
| SQL 查询结果 | ORM 映射或手动 `FromRow`，不让裸 `Row` 流出 |

---

## 五、并发竞态防御

AI 辅助开发时，AI 看代码是局部视角——它看到的函数可能被另一个线程并发调用，但它不知道。

### 5.1 最简方案：消灭共享可变状态

```
方案 A：单线程 event loop（如 Redis）
  所有状态修改在一个线程上顺序执行，不需要锁

方案 B：Actor 模型
  每个状态归一个 actor 所有，外部通过 channel 发消息
  actor 内部单线程处理，不共享

方案 C：不可变数据 + 消息传递
  数据只读，修改通过创建新版本
```

### 5.2 判断标准

如果你的系统不是高吞吐数据库，优先选 A 或 B。复杂度远低于锁/RwLock/原子操作，且 AI 写代码时不会遗漏同步逻辑。

---

## 六、操作原子性（无数据库场景）

### 6.1 方法封装

多步操作封装为一个方法，不暴露中间状态。

```rust
impl Account {
    // 转账 = 扣款 + 入账，封装为一个操作
    // 外部无法只调"扣款"不调"入账"
    pub fn transfer(&mut self, to: &mut Account, amount: u64) -> Result<()> {
        self.withdraw(amount)?;
        to.deposit(amount);
        Ok(())
    }
}
```

### 6.2 write-then-rename

文件更新需要原子性时，先写临时文件，再 rename。

```rust
pub fn atomic_write(target: &Path, content: &[u8]) -> Result<()> {
    let tmp = target.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, target)?;  // 同一文件系统上 rename 是原子的
    Ok(())
}
```

---

## 七、系统演化

需求变更时，契约也在变。如果契约散落在代码各处，改不全就是 bug。

### 7.1 防御流程

```
设计文档更新 → 类型定义更新 → 编译器报错 → 逐个修复 → 全部通过
```

编译器是免费的全项目 grep + 验证。前提是契约确实在类型里，而不是在字符串里。

### 7.2 具体操作

| 变更类型 | 做法 |
|----------|------|
| 新增枚举变体 | 加到 enum → 编译器报所有未处理的 match |
| 改字段名 | 改 struct field → 编译器报所有引用处 |
| 新增必填字段 | 加到 struct（无 default）→ 编译器报所有构造处 |
| 删除概念 | 删类型 → 编译器报所有使用处 |

### 7.3 跨 crate 类型的摆放原则

- 只在一个 crate 内用的类型 → 留在本地
- 两个以上 crate 用的类型 → 提到公共依赖 crate
- 原因不是"集中好看"，是 Rust crate 依赖必须是 DAG，共享类型不往上提会成环

---

## 八、检查清单

写代码时自问：

- [ ] 这个字面量是不是只在这里出现？如果别处也用，提取为类型或常量
- [ ] 这个值能不能变成 enum？如果可穷举，必须是 enum
- [ ] 这个路径/文件名是不是从 struct 方法来的？
- [ ] 这个外部输入是不是立即进了隔离仓？
- [ ] 这个状态修改有没有并发访问的可能？
- [ ] 改了一个类型定义后，编译能过吗？（能过说明传播完成）
- [ ] 如果编译过了但逻辑可能错，有没有测试覆盖？
